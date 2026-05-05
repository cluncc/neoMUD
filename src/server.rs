use tokio::net::TcpListener;
use tracing::{error, info};

use crate::session::run_session;
use crate::state::GameHandle;

pub async fn run(handle: GameHandle) -> anyhow::Result<()> {
    let addr = format!("{}:{}", handle.config.server.bind_addr, handle.config.server.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("neoMUD listening on {}", addr);

    loop {
        match listener.accept().await {
            Ok((stream, peer)) => {
                info!("Connection from {}", peer);
                let handle = handle.clone();
                tokio::spawn(async move {
                    run_session(stream, handle).await;
                });
            }
            Err(e) => {
                error!("Accept error: {}", e);
            }
        }
    }
}
