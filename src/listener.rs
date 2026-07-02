use std::time::Duration;

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::watch;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use crate::config::Config;
use crate::constants::TOKEN_PROGRAMS;
use crate::dedup::DedupStore;
use crate::rpc::HeliusRpc;
use crate::telegram::TelegramClient;
use crate::types::{is_initialize_mint_log, LogsNotification};

const WS_RECONNECT_BASE_SECS: u64 = 1;
const WS_RECONNECT_MAX_SECS: u64 = 60;

pub struct Listener {
    config: Config,
    rpc: HeliusRpc,
    telegram: TelegramClient,
    dedup: DedupStore,
    shutdown: watch::Receiver<bool>,
}

impl Listener {
    pub fn new(
        config: Config,
        rpc: HeliusRpc,
        telegram: TelegramClient,
        dedup: DedupStore,
        shutdown: watch::Receiver<bool>,
    ) -> Self {
        Self {
            config,
            rpc,
            telegram,
            dedup,
            shutdown,
        }
    }

    pub async fn run(self) -> Result<()> {
        let mut consecutive_failures = 0u32;
        let mut backoff_secs = WS_RECONNECT_BASE_SECS;
        let mut shutdown = self.shutdown;

        loop {
            if *shutdown.borrow() {
                info!("listener shutting down");
                return Ok(());
            }

            match run_websocket_session(
                &self.config,
                self.rpc.clone(),
                self.telegram.clone(),
                self.dedup.clone(),
                &mut shutdown,
            )
            .await
            {
                Ok(()) => {
                    consecutive_failures = 0;
                    backoff_secs = WS_RECONNECT_BASE_SECS;
                    warn!("websocket session ended cleanly, reconnecting");
                }
                Err(e) => {
                    consecutive_failures += 1;
                    warn!(
                        error = %e,
                        failures = consecutive_failures,
                        "websocket session ended"
                    );
                }
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

async fn run_websocket_session(
    config: &Config,
    rpc: HeliusRpc,
    telegram: TelegramClient,
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
                    info!("websocket session shutting down");
                    let _ = write.close().await;
                    return Ok(());
                }
            }
            _ = ping_timer.tick() => {
                if let Err(e) = write.send(Message::Ping(Vec::new().into())).await {
                    return Err(e).context("websocket ping failed");
                }
            }
            msg = read.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        handle_ws_message(&text, rpc.clone(), telegram.clone(), dedup.clone()).await;
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if let Err(e) = write.send(Message::Pong(payload)).await {
                            return Err(e).context("websocket pong failed");
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        return Err(anyhow::anyhow!("websocket closed by server"));
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        return Err(e).context("websocket read error");
                    }
                    None => {
                        return Err(anyhow::anyhow!("websocket stream ended"));
                    }
                }
            }
        }
    }
}

async fn handle_ws_message(
    text: &str,
    rpc: HeliusRpc,
    telegram: TelegramClient,
    dedup: DedupStore,
) {
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            debug!(error = %e, "ignoring non-json websocket message");
            return;
        }
    };

    if value.get("method").and_then(|m| m.as_str()) != Some("logsNotification") {
        if let Some(result) = value.get("result") {
            debug!(subscription = %result, "subscription acknowledged");
        }
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
    if logs_value.err.is_some() {
        return;
    }

    if !is_initialize_mint_log(&logs_value.logs) {
        return;
    }

    let signature = logs_value.signature;
    tokio::spawn(process_signature(signature, rpc, telegram, dedup));
}

async fn process_signature(
    signature: String,
    rpc: HeliusRpc,
    telegram: TelegramClient,
    dedup: DedupStore,
) {
    if !dedup.try_insert(&signature).await {
        debug!(signature, "duplicate signature skipped");
        return;
    }

    match rpc.build_mint_event(&signature).await {
        Ok(Some(event)) => {
            if !dedup.try_insert(&event.mint).await {
                debug!(mint = %event.mint, "duplicate mint skipped");
                return;
            }

            info!(
                mint = %event.mint,
                creator = %event.creator,
                signature = %event.signature,
                program = event.program.label(),
                "new mint detected"
            );

            if let Err(e) = telegram.send_mint_alert(&event).await {
                error!(
                    mint = %event.mint,
                    error = %e,
                    "failed to send telegram alert"
                );
            }
        }
        Ok(None) => {
            debug!(signature, "not an initializeMint event");
        }
        Err(e) => {
            warn!(signature, error = %e, "failed to process mint event");
        }
    }
}
