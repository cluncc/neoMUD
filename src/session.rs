/// Session: manages a single connected client.
/// Handles the state machine from initial connection → login/creation → playing.

use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufWriter};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::info;

use crate::color::*;
use crate::commands::{dispatch, hash_password, render_room, verify_password};
use crate::entity::{Class, Player, Race};
use crate::events::GameEvent;
use crate::state::GameHandle;

const INPUT_TIMEOUT_SECS: u64 = 300; // 5-minute idle disconnect
const MAX_INPUT_LEN: usize = 512;
const TELNET_IAC: u8 = 255;
const TELNET_WILL: u8 = 251;
const TELNET_WONT: u8 = 252;
const TELNET_DO: u8 = 253;
const TELNET_ECHO: u8 = 1;
const TELNET_NAWS: u8 = 31;

#[derive(Debug, PartialEq)]
enum SessionPhase {
    AwaitingName,
    AwaitingPassword(String),  // known name
    NewCharRace(String, String),  // name, password_hash
    NewCharClass(String, String, Race),
    Playing(String), // player name
}

pub async fn run_session(stream: TcpStream, handle: GameHandle) {
    let peer = stream.peer_addr().map(|a| a.to_string()).unwrap_or_else(|_| "?".to_string());
    info!("New connection from {}", peer);

    let (reader, raw_writer) = stream.into_split();
    let mut writer = BufWriter::new(raw_writer);
    let mut reader = reader;

    let (tx, mut rx): (mpsc::Sender<String>, mpsc::Receiver<String>) = mpsc::channel(256);

    // Negotiate telnet options: let the client handle local echo (WONT ECHO),
    // and ask for window size (DO NAWS).
    let _ = writer.write_all(&[TELNET_IAC, TELNET_WONT, TELNET_ECHO]).await;
    let _ = writer.write_all(&[TELNET_IAC, TELNET_DO, TELNET_NAWS]).await;
    let _ = writer.flush().await;

    let mut input_buf = Vec::<u8>::with_capacity(256);
    let mut raw_buf = [0u8; 512];

    // Send MOTD — normalise bare \n to \r\n for telnet
    let motd = handle.config.server.motd.replace('\n', "\r\n");
    let _ = writer.write_all(bright_cyan(&motd).as_bytes()).await;
    let _ = writer.write_all(b"\r\n").await;
    let _ = writer.write_all(bright_white("What is your name? ").as_bytes()).await;
    let _ = writer.flush().await;
    let mut phase = SessionPhase::AwaitingName;

    loop {
        tokio::select! {
            // Data from client
            result = timeout(Duration::from_secs(INPUT_TIMEOUT_SECS), reader.read(&mut raw_buf)) => {
                match result {
                    Err(_) => {
                        let _ = writer.write_all(yellow("\r\nIdle timeout. Goodbye!\r\n").as_bytes()).await;
                        let _ = writer.flush().await;
                        break;
                    }
                    Ok(Err(_)) | Ok(Ok(0)) => break,
                    Ok(Ok(n)) => {
                        let chunk = &raw_buf[..n];
                        if let Some(line) = process_telnet_input(chunk, &mut input_buf) {
                            let line = line.trim().to_string();
                            if line.len() > MAX_INPUT_LEN { continue; }

                            match &phase {
                                SessionPhase::AwaitingName => {
                                    let name = sanitize_name(&line);
                                    if name.len() < 2 || name.len() > 20 {
                                        let _ = writer.write_all(error_msg("Name must be 2-20 characters.\r\n").as_bytes()).await;
                                        let _ = writer.write_all(b"What is your name? ").await;
                                        let _ = writer.flush().await;
                                        continue;
                                    }
                                    let exists = Player::exists(&handle.config.game.players_path, &name);
                                    if exists {
                                        let _ = writer.write_all(format!("Welcome back, {}! Password: ", bright_white(&name)).as_bytes()).await;
                                        // Suppress client echo for password entry
                                        let _ = writer.write_all(&[TELNET_IAC, TELNET_WILL, TELNET_ECHO]).await;
                                        let _ = writer.flush().await;
                                        phase = SessionPhase::AwaitingPassword(name);
                                    } else {
                                        let _ = writer.write_all(format!("Creating new character '{}'.\r\nPassword: ", bright_white(&name)).as_bytes()).await;
                                        let _ = writer.write_all(&[TELNET_IAC, TELNET_WILL, TELNET_ECHO]).await;
                                        let _ = writer.flush().await;
                                        // Use AwaitingPassword but we'll detect new char after
                                        phase = SessionPhase::AwaitingPassword(format!("new:{}", name));
                                    }
                                }

                                SessionPhase::AwaitingPassword(name_tag) => {
                                    // Restore client-side echo now that password is submitted
                                    let _ = writer.write_all(&[TELNET_IAC, TELNET_WONT, TELNET_ECHO]).await;
                                    let hash = hash_password(&line);

                                    if name_tag.starts_with("new:") {
                                        let name = name_tag[4..].to_string();
                                        // Send race selection
                                        let race_menu = build_race_menu();
                                        let _ = writer.write_all(race_menu.as_bytes()).await;
                                        let _ = writer.flush().await;
                                        phase = SessionPhase::NewCharRace(name, hash);
                                    } else {
                                        let name = name_tag.clone();
                                        match Player::load(&handle.config.game.players_path, &name) {
                                            Err(_) => {
                                                let _ = writer.write_all(error_msg("Error loading character.\r\n").as_bytes()).await;
                                                let _ = writer.write_all(b"What is your name? ").await;
                                                let _ = writer.flush().await;
                                                phase = SessionPhase::AwaitingName;
                                            }
                                            Ok(player) => {
                                                if !verify_password(&player.password_hash, &line) {
                                                    let _ = writer.write_all(error_msg("Incorrect password.\r\n").as_bytes()).await;
                                                    let _ = writer.flush().await;
                                                    break;
                                                }
                                                // Successful login
                                                let login_msg = do_login(player, &handle, tx.clone()).await;
                                                let _ = writer.write_all(login_msg.as_bytes()).await;
                                                let _ = writer.flush().await;
                                                let pname = name.clone();
                                                phase = SessionPhase::Playing(name);
                                                // Show initial room
                                                let room_view = build_room_view(&handle, &pname).await;
                                                let _ = writer.write_all(room_view.as_bytes()).await;
                                                let _ = writer.flush().await;
                                            }
                                        }
                                    }
                                }

                                SessionPhase::NewCharRace(name, hash) => {
                                    let name = name.clone();
                                    let hash = hash.clone();
                                    match parse_race_choice(&line) {
                                        None => {
                                            let _ = writer.write_all(error_msg("Invalid choice. Choose a number 1-9.\r\n").as_bytes()).await;
                                            let _ = writer.flush().await;
                                        }
                                        Some(race) => {
                                            let class_menu = build_class_menu(&race);
                                            let _ = writer.write_all(class_menu.as_bytes()).await;
                                            let _ = writer.flush().await;
                                            phase = SessionPhase::NewCharClass(name, hash, race);
                                        }
                                    }
                                }

                                SessionPhase::NewCharClass(name, hash, race) => {
                                    let name = name.clone();
                                    let hash = hash.clone();
                                    let race = race.clone();
                                    match parse_class_choice(&line) {
                                        None => {
                                            let _ = writer.write_all(error_msg("Invalid choice.\r\n").as_bytes()).await;
                                            let _ = writer.flush().await;
                                        }
                                        Some(class) => {
                                            let start_room = handle.config.game.start_room.clone();
                                            let player = Player::new(&name, &hash, race, class, &start_room);
                                            let welcome = build_welcome(&player);
                                            let _ = writer.write_all(welcome.as_bytes()).await;

                                            let login_msg = do_login(player, &handle, tx.clone()).await;
                                            let _ = writer.write_all(login_msg.as_bytes()).await;
                                            let _ = writer.flush().await;

                                            let pname = name.clone();
                                            phase = SessionPhase::Playing(name);
                                            let room_view = build_room_view(&handle, &pname).await;
                                            let _ = writer.write_all(room_view.as_bytes()).await;
                                            let _ = writer.flush().await;
                                        }
                                    }
                                }

                                SessionPhase::Playing(player_name) => {
                                    let pn = player_name.clone();
                                    dispatch(&handle, &pn, &line).await;
                                    // Check if player quit
                                    if !handle.sessions.contains_key(&pn) {
                                        break;
                                    }
                                }

                            }

                            if matches!(phase, SessionPhase::Playing(_)) {
                                let _ = writer.write_all(b"\r\n> ").await;
                            }
                            let _ = writer.flush().await;
                        }
                    }
                }
            }

            // Output from game
            Some(msg) = rx.recv() => {
                let _ = writer.write_all(b"\r").await;
                let _ = writer.write_all(msg.as_bytes()).await;
                let _ = writer.write_all(b"\r\n> ").await;
                let _ = writer.flush().await;
            }
        }
    }

    // Cleanup on disconnect
    if let SessionPhase::Playing(player_name) = &phase {
        info!("Player {} disconnected", player_name);
        let mut state = handle.state.write().await;
        // Clear NPC's combat target before removing player
        let combat_npc = state.players.get(player_name).and_then(|p| p.in_combat_with.clone());
        if let Some(npc_id) = combat_npc {
            if let Some(npc) = state.npcs.get_mut(&npc_id) {
                npc.in_combat_with = None;
            }
        }
        if let Some(player) = state.players.remove(player_name) {
            let _ = player.save(&handle.config.game.players_path);
        }
        handle.sessions.remove(player_name);
        let _ = handle.events.send(GameEvent::PlayerDisconnected { player: player_name.clone() });
        state.tell_all(
            &dim(&format!("{} has left the game.", player_name)),
            &handle.sessions,
        ).await;
    }

    info!("Session {} ended", peer);
}

/// Returns Some(line) when a complete line is ready, stripping telnet IAC sequences.
fn process_telnet_input(chunk: &[u8], buf: &mut Vec<u8>) -> Option<String> {
    let mut i = 0;
    while i < chunk.len() {
        let b = chunk[i];
        if b == TELNET_IAC {
            // Skip IAC + command byte + option byte; clamp so split sequences
            // at the end of a chunk don't cause the index to wrap or skip data.
            i = (i + 3).min(chunk.len());
            continue;
        }
        if b == b'\r' || b == b'\n' {
            if buf.is_empty() { i += 1; continue; }
            let line = String::from_utf8_lossy(buf).into_owned();
            buf.clear();
            return Some(line);
        }
        // Handle backspace
        if b == 8 || b == 127 {
            buf.pop();
        } else if b.is_ascii() && !b.is_ascii_control() {
            buf.push(b);
        }
        i += 1;
    }
    None
}

pub fn sanitize_name(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if c.is_alphabetic() {
            if out.is_empty() {
                out.push(c.to_ascii_uppercase());
            } else {
                out.push(c.to_ascii_lowercase());
            }
        }
    }
    out
}

pub async fn do_login(player: Player, handle: &GameHandle, tx: mpsc::Sender<String>) -> String {
    let player_name = player.name.clone();
    {
        let mut state = handle.state.write().await;
        state.players.insert(player_name.clone(), player);
    }
    handle.sessions.insert(player_name.clone(), tx);
    let _ = handle.events.send(GameEvent::PlayerConnected { player: player_name.clone() });
    {
        let state = handle.state.read().await;
        state.tell_all(
            &dim(&format!("{} has entered the game.", player_name)),
            &handle.sessions,
        ).await;
    }
    success_msg(&format!("\r\nWelcome, {}!\r\n", player_name))
}

async fn build_room_view(handle: &GameHandle, player_name: &str) -> String {
    // Capture room render by temporarily subscribing to the player's channel
    // We call render_room which sends to the session channel
    render_room(handle, player_name).await;
    String::new() // render_room already sent it
}

pub fn build_welcome(player: &Player) -> String {
    let stats = &player.stats;
    format!(
        "\r\n{}\r\n{}\r\n\
         Race: {}   Class: {}\r\n\
         HP: {}  MP: {}  STR:{}  DEX:{}  CON:{}  INT:{}  WIS:{}  CHA:{}\r\n\
         {}\r\n\r\n",
        bold("=== Character Created! ==="),
        separator(),
        bright_yellow(&player.race.to_string()),
        bright_cyan(&player.class.to_string()),
        stats.max_hp, stats.max_mp,
        stats.strength, stats.dexterity, stats.constitution,
        stats.intelligence, stats.wisdom, stats.charisma,
        dim(&format!("Starting in: {}", player.room)),
    )
}

pub fn build_race_menu() -> String {
    let mut out = format!("\r\n{}\r\n{}\r\n", bold("Choose your Race:"), separator());
    for (i, race) in Race::all().iter().enumerate() {
        out.push_str(&format!(
            "  {}. {:10}  {}\r\n",
            bright_yellow(&(i + 1).to_string()),
            cyan(&race.to_string()),
            dim(race.description()),
        ));
    }
    out.push_str("\r\nEnter a number: ");
    out
}

pub fn build_class_menu(race: &Race) -> String {
    let mut out = format!(
        "\r\n{}\r\n{}\r\n",
        bold(&format!("Choose your Class ({}): ", race)),
        separator()
    );
    for (i, class) in Class::all().iter().enumerate() {
        out.push_str(&format!(
            "  {}. {:10}  {}\r\n",
            bright_yellow(&(i + 1).to_string()),
            cyan(&class.to_string()),
            dim(class.description()),
        ));
    }
    out.push_str("\r\nEnter a number: ");
    out
}

pub fn parse_race_choice(s: &str) -> Option<Race> {
    let idx: usize = s.trim().parse().ok()?;
    Race::all().get(idx.checked_sub(1)?).cloned()
}

pub fn parse_class_choice(s: &str) -> Option<Class> {
    let idx: usize = s.trim().parse().ok()?;
    Class::all().get(idx.checked_sub(1)?).cloned()
}
