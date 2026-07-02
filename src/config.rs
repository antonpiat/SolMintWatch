use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::alert::AlertMode;
use crate::constants::{
    COMMITMENT, FETCH_METADATA, METADATA_TIMEOUT_SECS, RPC_RETRY_BASE_MS, RPC_RETRY_MAX,
    SOLANA_NETWORK, WS_PING_INTERVAL_SECS,
};

#[derive(Debug, Clone)]
pub struct Config {
    pub alert_mode: AlertMode,
    pub telegram_bot_token: Option<String>,
    pub telegram_chat_id: Option<String>,
    pub rpc_url: String,
    pub ws_url: String,
    pub commitment: String,
    pub fetch_metadata: bool,
    pub metadata_timeout: Duration,
    pub ws_ping_interval: Duration,
    pub rpc_retry_max: u32,
    pub rpc_retry_base_ms: u64,
}

impl Config {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let helius_api_key = require_env("HELIUS_API_KEY")?;
        let alert_mode = AlertMode::parse(&require_env("ALERT_MODE")?)?;

        let (telegram_bot_token, telegram_chat_id) = match alert_mode {
            AlertMode::Stdout => (None, None),
            AlertMode::Telegram => (
                Some(require_env("TELEGRAM_BOT_TOKEN")?),
                Some(require_env("TELEGRAM_CHAT_ID")?),
            ),
        };

        let (rpc_host, ws_host) = match SOLANA_NETWORK {
            "mainnet" => ("mainnet.helius-rpc.com", "mainnet.helius-rpc.com"),
            "devnet" => ("devnet.helius-rpc.com", "devnet.helius-rpc.com"),
            other => bail!("unsupported SOLANA_NETWORK in constants.rs: {other}"),
        };

        let rpc_url = format!("https://{rpc_host}/?api-key={helius_api_key}");
        let ws_url = format!("wss://{ws_host}/?api-key={helius_api_key}");

        Ok(Self {
            alert_mode,
            telegram_bot_token,
            telegram_chat_id,
            rpc_url,
            ws_url,
            commitment: COMMITMENT.to_string(),
            fetch_metadata: FETCH_METADATA,
            metadata_timeout: Duration::from_secs(METADATA_TIMEOUT_SECS),
            ws_ping_interval: Duration::from_secs(WS_PING_INTERVAL_SECS),
            rpc_retry_max: RPC_RETRY_MAX,
            rpc_retry_base_ms: RPC_RETRY_BASE_MS,
        })
    }
}

fn require_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var: {key}"))
}
