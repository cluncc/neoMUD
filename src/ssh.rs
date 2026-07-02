//! SSH transport layer — accepts SSH connections and runs the same MUD session
//! state machine as the telnet path.  Players connect with:
//!   ssh -p 2222 <username>@host
//! The SSH username is used as a name hint; MUD authentication still applies.

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::info;

use russh::server::{Auth, Handler, Msg, Server, Session};
use russh::ChannelId;

use crate::color::*;
use crate::commands::{dispatch, hash_password, render_room, verify_password};
use crate::entity::{Player, Race};
use crate::events::GameEvent;
use crate::session::{
    build_class_menu, build_race_menu, build_welcome, do_login, parse_class_choice,
    parse_race_choice, sanitize_name,
};
use crate::state::GameHandle;

const MAX_INPUT_LEN: usize = 512;

// ─── Host key ────────────────────────────────────────────────────────────────

pub fn load_or_generate_host_key(path: &str) -> anyhow::Result<russh_keys::key::KeyPair> {
    let p = std::path::Path::new(path);
    if p.exists() {
        Ok(russh_keys::load_secret_key(p, None)
            .map_err(|e| anyhow::anyhow!("Host key load failed: {}", e))?)
    } else {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let kp = russh_keys::key::KeyPair::generate_ed25519()
            .ok_or_else(|| anyhow::anyhow!("Ed25519 key generation failed"))?;
        // Create the host key file with 0600 perms (owner read/write only) so
        // that the private key isn't world-readable. On non-Unix platforms we
        // fall back to the default umask.
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut file = opts.open(p)?;
        russh_keys::encode_pkcs8_pem(&kp, &mut file)
            .map_err(|e| anyhow::anyhow!("Host key save failed: {}", e))?;
        info!("Generated SSH host key at {}", path);
        Ok(kp)
    }
}

// ─── SSH server ───────────────────────────────────────────────────────────────

pub async fn run_ssh_server(game_handle: GameHandle) -> anyhow::Result<()> {
    let key_path = &game_handle.config.server.ssh_host_key_path.clone();
    let keypair = load_or_generate_host_key(key_path)?;

    let config = Arc::new(russh::server::Config {
        inactivity_timeout: Some(Duration::from_secs(300)),
        auth_rejection_time: Duration::from_secs(1),
        auth_rejection_time_initial: None,
        keys: vec![keypair],
        ..russh::server::Config::default()
    });

    let addr = format!(
        "{}:{}",
        game_handle.config.server.bind_addr,
        game_handle.config.server.ssh_port
    );
    info!("SSH server listening on {}", addr);
    let mut server = SshServer { game_handle };
    server.run_on_address(config, addr).await
        .map_err(|e| anyhow::anyhow!("SSH server error: {}", e))
}

// ─── Per-server state ─────────────────────────────────────────────────────────

struct SshServer {
    game_handle: GameHandle,
}

impl Server for SshServer {
    type Handler = SshHandler;

    fn new_client(&mut self, peer_addr: Option<std::net::SocketAddr>) -> SshHandler {
        SshHandler {
            game_handle: self.game_handle.clone(),
            peer: peer_addr.map(|a| a.to_string()).unwrap_or_else(|| "?".into()),
            ssh_username: None,
            input_tx: None,
        }
    }
}

// ─── Per-connection handler ───────────────────────────────────────────────────

pub struct SshHandler {
    game_handle: GameHandle,
    peer: String,
    ssh_username: Option<String>,
    input_tx: Option<mpsc::UnboundedSender<Vec<u8>>>,
}

#[async_trait::async_trait]
impl Handler for SshHandler {
    type Error = anyhow::Error;

    // Accept all SSH authentication — MUD handles its own login flow.
    async fn auth_none(&mut self, user: &str) -> Result<Auth, Self::Error> {
        self.ssh_username = Some(user.to_string());
        Ok(Auth::Accept)
    }

    async fn auth_password(&mut self, user: &str, _password: &str) -> Result<Auth, Self::Error> {
        self.ssh_username = Some(user.to_string());
        Ok(Auth::Accept)
    }

    // Called after signature verification; accept any key.
    async fn auth_publickey(
        &mut self,
        user: &str,
        _public_key: &russh_keys::key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        self.ssh_username = Some(user.to_string());
        Ok(Auth::Accept)
    }

    async fn auth_publickey_offered(
        &mut self,
        user: &str,
        _public_key: &russh_keys::key::PublicKey,
    ) -> Result<Auth, Self::Error> {
        self.ssh_username = Some(user.to_string());
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: russh::Channel<Msg>,
        _session: &mut Session,
    ) -> Result<bool, Self::Error> {
        Ok(true)
    }

    async fn pty_request(
        &mut self,
        _channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel_id: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let (input_tx, input_rx) = mpsc::unbounded_channel::<Vec<u8>>();
        self.input_tx = Some(input_tx);

        let ssh_handle = session.handle();
        let game_handle = self.game_handle.clone();
        let peer = self.peer.clone();
        let ssh_username = self.ssh_username.clone();

        tokio::spawn(async move {
            run_ssh_session(channel_id, input_rx, ssh_handle, game_handle, peer, ssh_username).await;
        });

        Ok(())
    }

    async fn data(
        &mut self,
        _channel_id: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(tx) = &self.input_tx {
            let _ = tx.send(data.to_vec());
        }
        Ok(())
    }
}

// ─── Session I/O helper ───────────────────────────────────────────────────────

async fn ssh_send(handle: &russh::server::Handle, channel: ChannelId, msg: &str) {
    let _ = handle
        .data(channel, russh::CryptoVec::from_slice(msg.as_bytes()))
        .await;
}

// ─── Session state machine ────────────────────────────────────────────────────

#[derive(Debug, PartialEq, Clone)]
enum SessionPhase {
    AwaitingName,
    AwaitingPassword(String), // "new:<name>" for new chars, plain name for existing
    NewCharRace(String, String),        // name, password_hash
    NewCharClass(String, String, Race), // name, password_hash, race
    Playing(String),                    // player_name
}

async fn run_ssh_session(
    channel: ChannelId,
    mut input_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    ssh: russh::server::Handle,
    handle: GameHandle,
    peer: String,
    ssh_username: Option<String>,
) {
    info!("SSH session started from {}", peer);

    // MOTD — normalise bare \n to \r\n for SSH terminals
    let motd = handle.config.server.motd.replace('\n', "\r\n");
    ssh_send(&ssh, channel, &format!("{}\r\n", bright_cyan(&motd))).await;

    // If the SSH username looks like a valid player name, use it as the initial name,
    // skipping the "What is your name?" prompt.
    let hint = ssh_username.as_deref().and_then(|u| {
        let s = sanitize_name(u);
        if s.len() >= 2 && s.len() <= 20 { Some(s) } else { None }
    });

    let mut phase = if let Some(ref name) = hint {
        if Player::exists(&handle.config.game.players_path, name) {
            ssh_send(&ssh, channel, &format!("Welcome back, {}!\r\nPassword: ", bright_white(name))).await;
            SessionPhase::AwaitingPassword(name.clone())
        } else {
            ssh_send(&ssh, channel, &format!("Creating new character '{}'.\r\nPassword: ", bright_white(name))).await;
            SessionPhase::AwaitingPassword(format!("new:{}", name))
        }
    } else {
        ssh_send(&ssh, channel, bright_white("What is your name? ").as_str()).await;
        SessionPhase::AwaitingName
    };

    // Channel for game→player output (registered in handle.sessions on login).
    let (game_tx, mut game_rx) = mpsc::channel::<String>(256);

    let mut buf = Vec::<u8>::new();
    let mut done = false;

    loop {
        if done { break; }
        tokio::select! {
            maybe = input_rx.recv() => {
                match maybe {
                    None => break,
                    Some(data) => {
                        for &b in &data {
                            if b == b'\r' || b == b'\n' {
                                if buf.is_empty() { continue; }
                                ssh_send(&ssh, channel, "\r\n").await;
                                let line = String::from_utf8_lossy(&buf).into_owned();
                                buf.clear();
                                if line.len() > MAX_INPUT_LEN { continue; }

                                done = handle_line(
                                    &line, &mut phase, &ssh, channel,
                                    &handle, game_tx.clone(),
                                ).await;

                                if matches!(phase, SessionPhase::Playing(_)) && !done {
                                    ssh_send(&ssh, channel, "> ").await;
                                }
                                if done { break; }
                            } else if b == 8 || b == 127 {
                                if buf.pop().is_some() && !matches!(phase, SessionPhase::AwaitingPassword(_)) {
                                    ssh_send(&ssh, channel, "\x08 \x08").await;
                                }
                            } else if b.is_ascii() && !b.is_ascii_control() {
                                if buf.len() < MAX_INPUT_LEN {
                                    buf.push(b);
                                    if !matches!(phase, SessionPhase::AwaitingPassword(_)) {
                                        ssh_send(&ssh, channel, &(b as char).to_string()).await;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Some(msg) = game_rx.recv() => {
                // Erase the current input line so the async message doesn't
                // overwrite what the player is midway through typing, then
                // redraw the prompt and re-echo the buffered input so their
                // in-progress keystrokes aren't lost.
                ssh_send(&ssh, channel, &format!("\r\x1b[K{}\r\n", msg)).await;
                if matches!(phase, SessionPhase::Playing(_)) {
                    let partial = String::from_utf8_lossy(&buf);
                    ssh_send(&ssh, channel, &format!("> {}", partial)).await;
                }
            }
        }
    }

    // Flush any pending game messages (e.g. quit farewell) before closing
    while let Ok(msg) = game_rx.try_recv() {
        ssh_send(&ssh, channel, &format!("\r{}\r\n", msg)).await;
    }

    // Cleanup
    if let SessionPhase::Playing(player_name) = &phase {
        info!("SSH player {} disconnected from {}", player_name, peer);
        let mut state = handle.state.write().await;
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
        state.tell_all(&dim(&format!("{} has left the game.", player_name)), &handle.sessions).await;
    }

    let _ = ssh.close(channel).await;
    info!("SSH session {} ended", peer);
}

/// Process one complete input line through the session state machine.
/// Returns `true` if the session should terminate.
async fn handle_line(
    line: &str,
    phase: &mut SessionPhase,
    ssh: &russh::server::Handle,
    channel: ChannelId,
    handle: &GameHandle,
    game_tx: mpsc::Sender<String>,
) -> bool {
    match phase.clone() {
        SessionPhase::AwaitingName => {
            let name = sanitize_name(line);
            if name.len() < 2 || name.len() > 20 {
                ssh_send(ssh, channel, "Name must be 2-20 characters.\r\nWhat is your name? ").await;
                return false;
            }
            if Player::exists(&handle.config.game.players_path, &name) {
                ssh_send(ssh, channel, &format!("Welcome back, {}!\r\nPassword: ", bright_white(&name))).await;
                *phase = SessionPhase::AwaitingPassword(name);
            } else {
                ssh_send(ssh, channel, &format!("Creating new character '{}'.\r\nPassword: ", bright_white(&name))).await;
                *phase = SessionPhase::AwaitingPassword(format!("new:{}", name));
            }
            false
        }

        SessionPhase::AwaitingPassword(name_tag) => {
            if let Some(name) = name_tag.strip_prefix("new:") {
                let name = name.to_string();
                let hash = hash_password(line);
                ssh_send(ssh, channel, &build_race_menu()).await;
                *phase = SessionPhase::NewCharRace(name, hash);
                false
            } else {
                let name = name_tag.clone();
                match Player::load(&handle.config.game.players_path, &name) {
                    Err(_) => {
                        ssh_send(ssh, channel, "Error loading character.\r\nWhat is your name? ").await;
                        *phase = SessionPhase::AwaitingName;
                        false
                    }
                    Ok(player) => {
                        if !verify_password(&player.password_hash, line) {
                            ssh_send(ssh, channel, &error_msg("Incorrect password.\r\n")).await;
                            return true; // disconnect
                        }
                        let login_msg = do_login(player, handle, game_tx).await;
                        ssh_send(ssh, channel, &login_msg).await;
                        let pname = name.clone();
                        *phase = SessionPhase::Playing(name);
                        render_room(handle, &pname).await;
                        false
                    }
                }
            }
        }

        SessionPhase::NewCharRace(name, hash) => {
            match parse_race_choice(line) {
                None => {
                    ssh_send(ssh, channel, &error_msg("Invalid choice. Choose a number 1-9.\r\n")).await;
                    false
                }
                Some(race) => {
                    ssh_send(ssh, channel, &build_class_menu(&race)).await;
                    *phase = SessionPhase::NewCharClass(name, hash, race);
                    false
                }
            }
        }

        SessionPhase::NewCharClass(name, hash, race) => {
            match parse_class_choice(line) {
                None => {
                    ssh_send(ssh, channel, &error_msg("Invalid choice.\r\n")).await;
                    false
                }
                Some(class) => {
                    let start_room = handle.config.game.start_room.clone();
                    let player = Player::new(&name, &hash, race, class, &start_room);
                    ssh_send(ssh, channel, &build_welcome(&player)).await;
                    let login_msg = do_login(player, handle, game_tx).await;
                    ssh_send(ssh, channel, &login_msg).await;
                    let pname = name.clone();
                    *phase = SessionPhase::Playing(name);
                    render_room(handle, &pname).await;
                    false
                }
            }
        }

        SessionPhase::Playing(player_name) => {
            dispatch(handle, &player_name, line).await;
            // Detect quit (session removed from sessions map)
            !handle.sessions.contains_key(&player_name)
        }
    }
}

