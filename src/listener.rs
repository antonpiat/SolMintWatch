use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::{watch, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::alert::AlertClient;
use crate::config::{Config, TOKEN_PROGRAMS};
use crate::rpc::HeliusRpc;
use crate::types::{is_mint_to_log, LogsNotification};

const WS_RECONNECT_BASE_SECS: u64 = 1;
const WS_RECONNECT_MAX_SECS: u64 = 60;

#[derive(Clone)]
struct DedupStore {
    seen: Arc<Mutex<HashSet<String>>>,
}

impl DedupStore {
    fn new() -> Self {
        Self {
            seen: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    async fn try_insert(&self, key: &str) -> bool {
        self.seen.lock().await.insert(key.to_string())
    }
}

pub struct Listener {
    config: Config,
    rpc: HeliusRpc,
    alerts: AlertClient,
    dedup: DedupStore,
    shutdown: watch::Receiver<bool>,
}

impl Listener {
    pub fn new(
        config: Config,
        rpc: HeliusRpc,
        alerts: AlertClient,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        Self {
            config,
            rpc,
            alerts,
            dedup: DedupStore::new(),
            shutdown,
        }
    }

    pub async fn run(self) -> Result<()> {
        let mut backoff_secs = WS_RECONNECT_BASE_SECS;
        let mut shutdown = self.shutdown;

        loop {
            if *shutdown.borrow() {
                info!("listener shutting down");
                return Ok(());
            }

            match run_session(
                &self.config,
                self.rpc.clone(),
                self.alerts.clone(),
                self.dedup.clone(),
                &mut shutdown,
            )
            .await
            {
                Ok(()) => {
                    backoff_secs = WS_RECONNECT_BASE_SECS;
                    warn!("websocket session ended, reconnecting");
                }
                Err(e) => warn!(error = %e, "websocket session ended"),
            }

            if *shutdown.borrow() {
                return Ok(());
            }

            info!(secs = backoff_secs, "reconnecting websocket");
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {},
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        return Ok(());
                    }
                }
            }
            backoff_secs = (backoff_secs * 2).min(WS_RECONNECT_MAX_SECS);
        }
    }
}

async fn run_session(
    config: &Config,
    rpc: HeliusRpc,
    alerts: AlertClient,
    dedup: DedupStore,
    shutdown: &mut watch::Receiver<bool>,
) -> Result<()> {
    let (ws_stream, _) = connect_async(&config.ws_url)
        .await
        .context("websocket connect failed")?;

    info!("websocket connected to Helius");

    let (mut write, mut read) = ws_stream.split();

    for (idx, program_id) in TOKEN_PROGRAMS.iter().enumerate() {
        let subscribe = json!({
            "jsonrpc": "2.0",
            "id": idx + 1,
            "method": "logsSubscribe",
            "params": [
                { "mentions": [program_id] },
                { "commitment": config.commitment }
            ]
        });
        write
            .send(Message::Text(subscribe.to_string().into()))
            .await
            .context("failed to send logsSubscribe")?;
        info!(program = program_id, "subscribed to token program logs");
    }

    let mut ping_timer = tokio::time::interval(config.ws_ping_interval);
    ping_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    let _ = write.close().await;
                    return Ok(());
                }
            }
            _ = ping_timer.tick() => {
                write.send(Message::Ping(Vec::new().into())).await
                    .context("websocket ping failed")?;
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_message(&text, rpc.clone(), alerts.clone(), dedup.clone()).await;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        write.send(Message::Pong(payload)).await
                            .context("websocket pong failed")?;
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(anyhow::anyhow!("websocket closed by server"));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => return Err(e).context("websocket read error"),
                    None => return Err(anyhow::anyhow!("websocket stream ended")),
                }
            }
        }
    }
}

async fn handle_message(
    text: &str,
    rpc: HeliusRpc,
    alerts: AlertClient,
    dedup: DedupStore,
) {
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(_) => return,
    };

    if value.get("method").and_then(|m| m.as_str()) != Some("logsNotification") {
        return;
    }

    let notification: LogsNotification = match serde_json::from_value(value) {
        Ok(n) => n,
        Err(e) => {
            warn!(error = %e, "failed to parse logsNotification");
            return;
        }
    };

    let logs_value = notification.params.result.value;
    if logs_value.err.is_some() || !is_mint_to_log(&logs_value.logs) {
        return;
    }

    tokio::spawn(process_signature(
        logs_value.signature,
        rpc,
        alerts,
        dedup,
    ));
}

async fn process_signature(
    signature: String,
    rpc: HeliusRpc,
    alerts: AlertClient,
    dedup: DedupStore,
) {
    if !dedup.try_insert(&signature).await {
        return;
    }

    match rpc.build_mint_event(&signature).await {
        Ok(Some(event)) => {
            if !dedup.try_insert(&event.mint).await {
                return;
            }

            info!(
                mint = %event.mint,
                creator = %event.creator,
                signature = %event.signature,
                program = event.program.label(),
                "first token supply detected"
            );

            if let Err(e) = alerts.send_mint_alert(&event).await {
                error!(mint = %event.mint, error = %e, "failed to send alert");
            }
        }
        Ok(None) => debug!(signature, "not a first-supply mintTo"),
        Err(e) => warn!(signature, error = %e, "failed to process mint event"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dedup_rejects_duplicates() {
        let store = DedupStore::new();
        assert!(store.try_insert("mint1").await);
        assert!(!store.try_insert("mint1").await);
        assert!(store.try_insert("mint2").await);
    }
}
