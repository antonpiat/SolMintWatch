use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Client;

use crate::alert::AlertMode;

// --- tweak these in source (not .env) ---

pub const SOLANA_NETWORK: &str = "mainnet";
pub const COMMITMENT: &str = "confirmed";
pub const FETCH_METADATA: bool = true;
pub const METADATA_TIMEOUT_SECS: u64 = 2;
pub const WS_PING_INTERVAL_SECS: u64 = 30;
pub const HTTP_RETRY_MAX: u32 = 3;
pub const HTTP_RETRY_BASE_MS: u64 = 500;
pub const RUST_LOG: &str = "info,solmintwatch=debug";

pub const SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const METAPLEX_METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
pub const TOKEN_PROGRAMS: [&str; 2] = [SPL_TOKEN_PROGRAM, TOKEN_2022_PROGRAM];

// --- runtime config from .env ---

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
    pub http_retry_max: u32,
    pub http_retry_base_ms: u64,
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
            other => bail!("unsupported SOLANA_NETWORK in config.rs: {other}"),
        };

        Ok(Self {
            alert_mode,
            telegram_bot_token,
            telegram_chat_id,
            rpc_url: format!("https://{rpc_host}/?api-key={helius_api_key}"),
            ws_url: format!("wss://{ws_host}/?api-key={helius_api_key}"),
            commitment: COMMITMENT.to_string(),
            fetch_metadata: FETCH_METADATA,
            metadata_timeout: Duration::from_secs(METADATA_TIMEOUT_SECS),
            ws_ping_interval: Duration::from_secs(WS_PING_INTERVAL_SECS),
            http_retry_max: HTTP_RETRY_MAX,
            http_retry_base_ms: HTTP_RETRY_BASE_MS,
        })
    }
}

pub fn http_client() -> Result<Client> {
    Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .context("failed to build HTTP client")
}

pub async fn http_backoff(attempt: u32, base_ms: u64) {
    let delay = base_ms.saturating_mul(1u64 << attempt.min(6));
    tokio::time::sleep(Duration::from_millis(delay)).await;
}

fn require_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var: {key}"))
}
