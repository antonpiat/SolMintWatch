mod config;
mod dedup;
mod listener;
mod rpc;
mod telegram;
mod types;

use anyhow::Result;
use tokio::sync::watch;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use crate::config::Config;
use crate::dedup::DedupStore;
use crate::listener::Listener;
use crate::rpc::HeliusRpc;
use crate::telegram::TelegramClient;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(env!("SOLMINTWATCH_RUST_LOG"))
        }))
        .init();

    let config = Config::from_env()?;
    info!(
        commitment = %config.commitment,
        fetch_metadata = config.fetch_metadata,
        "starting solmintwatch"
    );

    let rpc = HeliusRpc::new(&config)?;
    let telegram = TelegramClient::new(&config)?;
    let dedup = DedupStore::new();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let listener = Listener::new(config, rpc, telegram, dedup, shutdown_rx);

    tokio::select! {
        result = listener.run() => {
            if let Err(e) = result {
                error!(error = %e, "listener exited with error");
            }
        }
        _ = tokio::signal::ctrl_c() => {
            info!("shutdown signal received");
            let _ = shutdown_tx.send(true);
        }
    }

    info!("solmintwatch stopped");
    Ok(())
}
