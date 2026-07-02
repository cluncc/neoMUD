//! Server-side line editor with command history and readline-style shortcuts.
//!
//! Both the telnet and SSH front-ends echo input server-side and feed raw
//! bytes to a [`LineEditor`]. The editor maintains the in-progress line, a
//! cursor position, and a per-session command history, translating keystrokes
//! (printable characters, control codes, and ANSI escape sequences) into the
//! terminal output needed to keep the client's display in sync.
//!
//! Supported comforts:
//!   * Up / Down .............. browse command history
//!   * Left / Right, Ctrl-B/F . move the cursor
//!   * Home / End, Ctrl-A/E ... jump to line start / end
//!   * Backspace, Del, Ctrl-D . delete a character
//!   * Ctrl-W ................. delete the previous word
//!   * Ctrl-U ................. clear the whole line
//!   * Ctrl-K ................. delete from the cursor to end of line
//!
//! Rendering is incremental and never repaints the prompt, so the editor
//! doesn't need to know what prompt string precedes the input.

/// Maximum number of past commands retained for history browsing.
pub const MAX_HISTORY: usize = 100;

/// Result of feeding a chunk of input bytes to the editor.
pub struct Feed {
    /// Bytes to write back to the terminal to reflect the edits.
    pub echo: String,
    /// Complete input lines the user submitted (usually zero or one).
    pub lines: Vec<String>,
}

/// Escape-sequence parse state, retained across `feed` calls in case a
/// sequence is split across TCP reads.
enum Esc {
    None,
    /// Saw a bare ESC (0x1b).
    Seen,
    /// Saw `ESC [` or `ESC O`; accumulating parameter bytes.
    Csi(Vec<u8>),
}

pub struct LineEditor {
    buf: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    /// Index into `history` while browsing; `None` when editing a fresh line.
    hist_idx: Option<usize>,
    /// The in-progress line stashed when history browsing began.
    stash: Vec<char>,
    esc: Esc,
    /// True when the last byte was a CR, so a following LF is swallowed.
    last_cr: bool,
    /// When true, characters are consumed but not echoed (password entry).
    mask: bool,
    max_len: usize,
}

fn move_left(out: &mut String, n: usize) {
    if n > 0 {
        out.push_str(&format!("\x1b[{}D", n));
    }
}

fn move_right(out: &mut String, n: usize) {
    if n > 0 {
        out.push_str(&format!("\x1b[{}C", n));
    }
}

impl LineEditor {
    pub fn new(max_len: usize) -> Self {
        Self {
            buf: Vec::new(),
            cursor: 0,
            history: Vec::new(),
            hist_idx: None,
            stash: Vec::new(),
            esc: Esc::None,
            last_cr: false,
            mask: false,
            max_len,
        }
    }

    /// Toggle password mode: input is still buffered but never echoed, and
    /// navigation/history keys are inert so a password can't be corrupted.
    pub fn set_mask(&mut self, mask: bool) {
        self.mask = mask;
    }

    /// The current in-progress line, for redrawing the prompt after
    /// out-of-band output. Empty while masking (never expose a password).
    pub fn render_buffer(&self) -> String {
        if self.mask {
            String::new()
        } else {
            self.buf.iter().collect()
        }
    }

    /// Process a chunk of raw input bytes, returning echo output and any
    /// completed lines.
    pub fn feed(&mut self, data: &[u8]) -> Feed {
        let mut echo = String::new();
        let mut lines = Vec::new();
        for &b in data {
            self.byte(b, &mut echo, &mut lines);
        }
        Feed { echo, lines }
    }

    fn byte(&mut self, b: u8, out: &mut String, lines: &mut Vec<String>) {
        // Escape-sequence state machine runs first, and always — even while
        // masking — so arrow-key bytes are swallowed rather than inserted as
        // literal '[' / 'A' characters.
        match &mut self.esc {
            Esc::None => {}
            Esc::Seen => {
                self.esc = if b == b'[' || b == b'O' {
                    Esc::Csi(Vec::new())
                } else {
                    Esc::None
                };
                return;
            }
            Esc::Csi(params) => {
                if (0x40..=0x7e).contains(&b) {
                    let seq = std::mem::take(params);
                    self.esc = Esc::None;
                    self.csi(b, &seq, out);
                } else {
                    params.push(b);
                }
                return;
            }
        }

        // Enter: treat CR as submit and swallow an immediately following LF so
        // a CRLF pair doesn't submit twice.
        if b == b'\r' {
            self.last_cr = true;
            self.submit(out, lines);
            return;
        }
        if b == b'\n' {
            if self.last_cr {
                self.last_cr = false;
                return;
            }
            self.submit(out, lines);
            return;
        }
        self.last_cr = false;

        match b {
            0x1b => self.esc = Esc::Seen,
            0x01 => self.home(out),        // Ctrl-A
            0x05 => self.end(out),         // Ctrl-E
            0x02 => self.left(out),        // Ctrl-B
            0x06 => self.right(out),       // Ctrl-F
            0x04 => self.delete(out),      // Ctrl-D → forward delete
            0x0b => self.kill_to_end(out), // Ctrl-K
            0x15 => self.kill_line(out),   // Ctrl-U
            0x17 => self.delete_word(out), // Ctrl-W
            0x08 | 0x7f => self.backspace(out),
            _ => {
                if b.is_ascii() && !b.is_ascii_control() {
                    self.insert(b as char, out);
                }
            }
        }
    }

    fn csi(&mut self, final_byte: u8, params: &[u8], out: &mut String) {
        match final_byte {
            b'A' => self.history_prev(out),
            b'B' => self.history_next(out),
            b'C' => self.right(out),
            b'D' => self.left(out),
            b'H' => self.home(out),
            b'F' => self.end(out),
            b'~' => match params {
                b"1" | b"7" => self.home(out),
                b"4" | b"8" => self.end(out),
                b"3" => self.delete(out),
                _ => {}
            },
            _ => {}
        }
    }

    fn submit(&mut self, out: &mut String, lines: &mut Vec<String>) {
        out.push_str("\r\n");
        let line: String = self.buf.iter().collect();
        if !self.mask {
            let trimmed = line.trim();
            let is_dup = self.history.last().map(|h| h == trimmed).unwrap_or(false);
            if !trimmed.is_empty() && !is_dup {
                self.history.push(trimmed.to_string());
                if self.history.len() > MAX_HISTORY {
                    self.history.remove(0);
                }
            }
        }
        self.buf.clear();
        self.cursor = 0;
        self.hist_idx = None;
        self.stash.clear();
        lines.push(line);
    }

    fn insert(&mut self, c: char, out: &mut String) {
        if self.buf.len() >= self.max_len {
            return;
        }
        self.hist_idx = None;
        self.buf.insert(self.cursor, c);
        self.cursor += 1;
        if self.mask {
            return;
        }
        if self.cursor == self.buf.len() {
            out.push(c);
        } else {
            // Middle insert: print the new char and the trailing text, then
            // move the cursor back to just after the inserted char.
            let tail: String = self.buf[self.cursor..].iter().collect();
            out.push(c);
            out.push_str(&tail);
            move_left(out, tail.chars().count());
        }
    }

    fn backspace(&mut self, out: &mut String) {
        if self.cursor == 0 {
            return;
        }
        self.hist_idx = None;
        self.buf.remove(self.cursor - 1);
        self.cursor -= 1;
        if self.mask {
            return;
        }
        let tail: String = self.buf[self.cursor..].iter().collect();
        out.push('\x08');
        out.push_str(&tail);
        out.push(' ');
        move_left(out, tail.chars().count() + 1);
    }

    fn delete(&mut self, out: &mut String) {
        if self.mask || self.cursor >= self.buf.len() {
            return;
        }
        self.hist_idx = None;
        self.buf.remove(self.cursor);
        let tail: String = self.buf[self.cursor..].iter().collect();
        out.push_str(&tail);
        out.push(' ');
        move_left(out, tail.chars().count() + 1);
    }

    fn left(&mut self, out: &mut String) {
        if self.mask || self.cursor == 0 {
            return;
        }
        self.cursor -= 1;
        out.push('\x08');
    }

    fn right(&mut self, out: &mut String) {
        if self.mask || self.cursor >= self.buf.len() {
            return;
        }
        self.cursor += 1;
        out.push_str("\x1b[C");
    }

    fn home(&mut self, out: &mut String) {
        if self.mask {
            return;
        }
        move_left(out, self.cursor);
        self.cursor = 0;
    }

    fn end(&mut self, out: &mut String) {
        if self.mask {
            return;
        }
        move_right(out, self.buf.len() - self.cursor);
        self.cursor = self.buf.len();
    }

    fn kill_line(&mut self, out: &mut String) {
        self.hist_idx = None;
        if !self.mask {
            move_left(out, self.cursor);
            out.push_str("\x1b[K");
        }
        self.buf.clear();
        self.cursor = 0;
    }

    fn kill_to_end(&mut self, out: &mut String) {
        if self.mask || self.cursor >= self.buf.len() {
            return;
        }
        self.hist_idx = None;
        self.buf.truncate(self.cursor);
        out.push_str("\x1b[K");
    }

    fn delete_word(&mut self, out: &mut String) {
        if self.mask || self.cursor == 0 {
            return;
        }
        self.hist_idx = None;
        let mut start = self.cursor;
        while start > 0 && self.buf[start - 1] == ' ' {
            start -= 1;
        }
        while start > 0 && self.buf[start - 1] != ' ' {
            start -= 1;
        }
        let removed = self.cursor - start;
        self.buf.drain(start..self.cursor);
        self.cursor = start;
        let tail: String = self.buf[self.cursor..].iter().collect();
        move_left(out, removed);
        out.push_str(&tail);
        for _ in 0..removed {
            out.push(' ');
        }
        move_left(out, tail.chars().count() + removed);
    }

    fn history_prev(&mut self, out: &mut String) {
        if self.mask || self.history.is_empty() {
            return;
        }
        let new_idx = match self.hist_idx {
            None => {
                self.stash = self.buf.clone();
                self.history.len() - 1
            }
            Some(0) => return, // already at the oldest entry
            Some(i) => i - 1,
        };
        self.hist_idx = Some(new_idx);
        let text: Vec<char> = self.history[new_idx].chars().collect();
        self.replace_line(text, out);
    }

    fn history_next(&mut self, out: &mut String) {
        if self.mask {
            return;
        }
        match self.hist_idx {
            None => {}
            Some(i) if i + 1 < self.history.len() => {
                self.hist_idx = Some(i + 1);
                let text: Vec<char> = self.history[i + 1].chars().collect();
                self.replace_line(text, out);
            }
            Some(_) => {
                // Stepped past the newest entry → restore the stashed line.
                self.hist_idx = None;
                let text = std::mem::take(&mut self.stash);
                self.replace_line(text, out);
            }
        }
    }

    /// Replace the entire current line with `new`, redrawing from the start of
    /// the input (just after the prompt) without touching the prompt itself.
    fn replace_line(&mut self, new: Vec<char>, out: &mut String) {
        move_left(out, self.cursor);
        out.push_str("\x1b[K");
        let s: String = new.iter().collect();
        out.push_str(&s);
        self.cursor = new.len();
        self.buf = new;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience: feed a string, return completed lines.
    fn lines(ed: &mut LineEditor, s: &str) -> Vec<String> {
        ed.feed(s.as_bytes()).lines
    }

    #[test]
    fn submits_a_line_on_enter() {
        let mut ed = LineEditor::new(512);
        assert_eq!(lines(&mut ed, "look\r"), vec!["look".to_string()]);
    }

    #[test]
    fn crlf_submits_once() {
        let mut ed = LineEditor::new(512);
        let out = ed.feed(b"north\r\n");
        assert_eq!(out.lines, vec!["north".to_string()]);
    }

    #[test]
    fn backspace_edits_buffer() {
        let mut ed = LineEditor::new(512);
        assert_eq!(lines(&mut ed, "lookk\x08\r"), vec!["look".to_string()]);
    }

    #[test]
    fn up_arrow_recalls_previous_command() {
        let mut ed = LineEditor::new(512);
        lines(&mut ed, "kill goblin\r");
        // Press Up, then Enter — should resubmit the recalled command.
        assert_eq!(lines(&mut ed, "\x1b[A\r"), vec!["kill goblin".to_string()]);
    }

    #[test]
    fn up_then_down_returns_to_fresh_line() {
        let mut ed = LineEditor::new(512);
        lines(&mut ed, "score\r");
        // Type partial, browse up (recall "score"), then down (restore partial).
        let out = ed.feed(b"inv\x1b[A\x1b[B\r");
        assert_eq!(out.lines, vec!["inv".to_string()]);
    }

    #[test]
    fn history_walks_multiple_entries() {
        let mut ed = LineEditor::new(512);
        lines(&mut ed, "one\r");
        lines(&mut ed, "two\r");
        // Up twice reaches the oldest entry.
        assert_eq!(lines(&mut ed, "\x1b[A\x1b[A\r"), vec!["one".to_string()]);
    }

    #[test]
    fn consecutive_duplicates_not_stored_twice() {
        let mut ed = LineEditor::new(512);
        lines(&mut ed, "look\r");
        lines(&mut ed, "look\r");
        // A single Up should still be the most recent; a second Up must not
        // find another "look" — it should stay put (oldest reached).
        let recalled = lines(&mut ed, "\x1b[A\x1b[A\r");
        assert_eq!(recalled, vec!["look".to_string()]);
    }

    #[test]
    fn cursor_move_and_insert() {
        let mut ed = LineEditor::new(512);
        // Type "lok", move left once, insert "o" → "look".
        assert_eq!(lines(&mut ed, "lok\x1b[Do\r"), vec!["look".to_string()]);
    }

    #[test]
    fn ctrl_u_clears_line() {
        let mut ed = LineEditor::new(512);
        assert_eq!(lines(&mut ed, "garbage\x15look\r"), vec!["look".to_string()]);
    }

    #[test]
    fn ctrl_w_deletes_previous_word() {
        let mut ed = LineEditor::new(512);
        assert_eq!(lines(&mut ed, "kill orc\x17goblin\r"), vec!["kill goblin".to_string()]);
    }

    #[test]
    fn masked_input_is_buffered_but_navigation_inert() {
        let mut ed = LineEditor::new(512);
        ed.set_mask(true);
        // Echo must be empty, arrow keys must not corrupt the buffer.
        let out = ed.feed(b"se\x1b[Acret\r");
        assert_eq!(out.lines, vec!["secret".to_string()]);
        assert!(out.echo.chars().all(|c| c == '\r' || c == '\n'));
    }

    #[test]
    fn masked_input_not_recorded_in_history() {
        let mut ed = LineEditor::new(512);
        ed.set_mask(true);
        lines(&mut ed, "hunter2\r");
        ed.set_mask(false);
        // Up should find nothing to recall.
        assert_eq!(lines(&mut ed, "\x1b[A\r"), vec!["".to_string()]);
    }

    #[test]
    fn respects_max_len() {
        let mut ed = LineEditor::new(4);
        assert_eq!(lines(&mut ed, "abcdef\r"), vec!["abcd".to_string()]);
    }
}
