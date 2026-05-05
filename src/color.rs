/// ANSI color wrappers for MUD output.
/// All color functions return owned Strings with embedded escape codes.

pub fn bold(s: &str) -> String { format!("\x1b[1m{}\x1b[0m", s) }
pub fn dim(s: &str) -> String { format!("\x1b[2m{}\x1b[0m", s) }
pub fn italic(s: &str) -> String { format!("\x1b[3m{}\x1b[0m", s) }

pub fn red(s: &str) -> String { format!("\x1b[31m{}\x1b[0m", s) }
pub fn green(s: &str) -> String { format!("\x1b[32m{}\x1b[0m", s) }
pub fn yellow(s: &str) -> String { format!("\x1b[33m{}\x1b[0m", s) }
pub fn blue(s: &str) -> String { format!("\x1b[34m{}\x1b[0m", s) }
pub fn magenta(s: &str) -> String { format!("\x1b[35m{}\x1b[0m", s) }
pub fn cyan(s: &str) -> String { format!("\x1b[36m{}\x1b[0m", s) }
pub fn white(s: &str) -> String { format!("\x1b[37m{}\x1b[0m", s) }

pub fn bright_red(s: &str) -> String { format!("\x1b[91m{}\x1b[0m", s) }
pub fn bright_green(s: &str) -> String { format!("\x1b[92m{}\x1b[0m", s) }
pub fn bright_yellow(s: &str) -> String { format!("\x1b[93m{}\x1b[0m", s) }
pub fn bright_blue(s: &str) -> String { format!("\x1b[94m{}\x1b[0m", s) }
pub fn bright_magenta(s: &str) -> String { format!("\x1b[95m{}\x1b[0m", s) }
pub fn bright_cyan(s: &str) -> String { format!("\x1b[96m{}\x1b[0m", s) }
pub fn bright_white(s: &str) -> String { format!("\x1b[97m{}\x1b[0m", s) }

// Semantic aliases for MUD UI elements
pub fn room_title(s: &str) -> String { bold(&bright_cyan(s)) }
pub fn room_desc(s: &str) -> String { white(s) }
pub fn exit_list(s: &str) -> String { bright_green(s) }
pub fn item_name(s: &str) -> String { yellow(s) }
pub fn npc_name(s: &str) -> String { bright_yellow(s) }
pub fn player_name(s: &str) -> String { bright_white(s) }
pub fn damage_out(s: &str) -> String { bright_red(s) }
pub fn damage_in(s: &str) -> String { red(s) }
pub fn heal_text(s: &str) -> String { bright_green(s) }
pub fn say_text(s: &str) -> String { white(s) }
pub fn tell_text(s: &str) -> String { magenta(s) }
pub fn shout_text(s: &str) -> String { bright_magenta(s) }
pub fn error_msg(s: &str) -> String { red(s) }
pub fn success_msg(s: &str) -> String { green(s) }
pub fn info_msg(s: &str) -> String { dim(s) }
pub fn admin_msg(s: &str) -> String { bright_red(s) }

/// Horizontal separator line
pub fn separator() -> String {
    dim(&"-".repeat(60))
}

/// Health bar for combat display
pub fn health_bar(current: i32, max: i32, width: usize) -> String {
    let ratio = (current as f32 / max as f32).clamp(0.0, 1.0);
    let filled = (ratio * width as f32) as usize;
    let empty = width - filled;
    let color = if ratio > 0.6 { bright_green } else if ratio > 0.3 { yellow } else { bright_red };
    let bar = format!("[{}{}]", "#".repeat(filled), ".".repeat(empty));
    color(&bar)
}
