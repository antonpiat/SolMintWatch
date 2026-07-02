use std::time::Duration;

use anyhow::{bail, Context, Result};

pub const SPL_TOKEN_PROGRAM: &str = env!("SOLMINTWATCH_SPL_TOKEN_PROGRAM");
pub const TOKEN_2022_PROGRAM: &str = env!("SOLMINTWATCH_TOKEN_2022_PROGRAM");
pub const METAPLEX_METADATA_PROGRAM: &str = env!("SOLMINTWATCH_METAPLEX_METADATA_PROGRAM");

pub const TOKEN_PROGRAMS: [&str; 2] = [SPL_TOKEN_PROGRAM, TOKEN_2022_PROGRAM];

#[derive(Debug, Clone)]
pub struct Config {
    pub telegram_bot_token: String,
    pub telegram_chat_id: String,
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
        let telegram_bot_token = require_env("TELEGRAM_BOT_TOKEN")?;
        let telegram_chat_id = require_env("TELEGRAM_CHAT_ID")?;

        let network = env!("SOLMINTWATCH_SOLANA_NETWORK");
        let (rpc_host, ws_host) = match network {
            "mainnet" => ("mainnet.helius-rpc.com", "mainnet.helius-rpc.com"),
            "devnet" => ("devnet.helius-rpc.com", "devnet.helius-rpc.com"),
            other => bail!("unsupported solana-network in Cargo.toml: {other}"),
        };

        let rpc_url = format!("https://{rpc_host}/?api-key={helius_api_key}");
        let ws_url = format!("wss://{ws_host}/?api-key={helius_api_key}");

        Ok(Self {
            telegram_bot_token,
            telegram_chat_id,
            rpc_url,
            ws_url,
            commitment: env!("SOLMINTWATCH_COMMITMENT").to_string(),
            fetch_metadata: parse_bool(env!("SOLMINTWATCH_FETCH_METADATA")),
            metadata_timeout: Duration::from_secs(parse_u64(
                env!("SOLMINTWATCH_METADATA_TIMEOUT_SECS"),
                "metadata-timeout-secs",
            )?),
            ws_ping_interval: Duration::from_secs(parse_u64(
                env!("SOLMINTWATCH_WS_PING_INTERVAL_SECS"),
                "ws-ping-interval-secs",
            )?),
            rpc_retry_max: parse_u32(env!("SOLMINTWATCH_RPC_RETRY_MAX"), "rpc-retry-max")?,
            rpc_retry_base_ms: parse_u64(env!("SOLMINTWATCH_RPC_RETRY_BASE_MS"), "rpc-retry-base-ms")?,
        })
    }
}

fn require_env(key: &str) -> Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var: {key}"))
}

fn parse_bool(s: &str) -> bool {
    matches!(s, "true" | "1" | "yes")
}

fn parse_u64(s: &str, field: &str) -> Result<u64> {
    s.parse()
        .with_context(|| format!("invalid {field} in Cargo.toml: {s}"))
}

fn parse_u32(s: &str, field: &str) -> Result<u32> {
    s.parse()
        .with_context(|| format!("invalid {field} in Cargo.toml: {s}"))
}
