mod alert;
mod config;
mod constants;
mod dedup;
mod listener;
mod metadata;
mod rpc;
mod telegram;
mod types;

use anyhow::Result;
use tokio::sync::watch;
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use crate::alert::AlertClient;
use crate::config::Config;
use crate::constants::RUST_LOG;
use crate::dedup::DedupStore;
use crate::listener::Listener;
use crate::rpc::HeliusRpc;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(RUST_LOG)
        }))
        .init();

    let config = Config::from_env()?;
    info!(
        alert_mode = config.alert_mode.label(),
        commitment = %config.commitment,
        fetch_metadata = config.fetch_metadata,
        "starting solmintwatch"
    );

    let rpc = HeliusRpc::new(&config)?;
    let alerts = AlertClient::new(&config)?;
    let dedup = DedupStore::new();

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let listener = Listener::new(config, rpc, alerts, dedup, shutdown_rx);

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
