mod color;
mod combat;
mod commands;
mod config;
mod entity;
mod events;
mod scripting;
mod server;
mod session;
mod ssh;
mod state;
mod time;
mod world;

use clap::Parser;
use std::time::Duration;
use tracing::info;

use config::Config;
use state::GameHandle;

#[derive(Parser)]
#[command(name = "neomud", about = "A modern, scriptable MUD engine")]
struct Args {
    #[arg(short, long, default_value = "config.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "neomud=info,warn".to_string())
        )
        .init();

    let args = Args::parse();
    let config = Config::load(&args.config)?;

    // Build player save directory
    std::fs::create_dir_all(&config.game.players_path)?;

    let handle = GameHandle::new(config.clone()).await?;

    // Game tick loop
    let tick_handle = handle.clone();
    tokio::spawn(async move {
        let tick_ms = tick_handle.config.game.tick_rate_ms;
        let mut interval = tokio::time::interval(Duration::from_millis(tick_ms));
        loop {
            interval.tick().await;
            tick_handle.tick().await;
        }
    });

    if handle.config.server.ssh_enabled {
        let ssh_handle = handle.clone();
        tokio::spawn(async move {
            if let Err(e) = ssh::run_ssh_server(ssh_handle).await {
                tracing::error!("SSH server error: {}", e);
            }
        });
    } else {
        info!("SSH disabled");
    }

    if handle.config.server.telnet_enabled {
        server::run(handle).await?;
    } else {
        info!("Telnet disabled");
        std::future::pending::<()>().await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_world_loads_from_toml() {
        for fname in &["world/areas/nexus.toml", "world/areas/deepwood.toml"] {
            let content = std::fs::read_to_string(fname).expect(fname);
            let (area, _, _) = crate::world::parse_area_file_str(&content)
                .unwrap_or_else(|e| panic!("{}: {}", fname, e));
            assert!(!area.rooms.is_empty(), "{}: no rooms loaded", fname);
        }
    }
}
