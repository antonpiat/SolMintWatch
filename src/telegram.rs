use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use chrono::{TimeZone, Utc};
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use crate::config::Config;
use crate::types::MintEvent;

#[derive(Clone)]
pub struct TelegramClient {
    client: Client,
    bot_token: String,
    chat_id: String,
    retry_max: u32,
    retry_base_ms: u64,
}

impl TelegramClient {
    pub fn new(config: &Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("failed to build Telegram HTTP client")?;

        Ok(Self {
            client,
            bot_token: config.telegram_bot_token.clone(),
            chat_id: config.telegram_chat_id.clone(),
            retry_max: config.rpc_retry_max,
            retry_base_ms: config.rpc_retry_base_ms,
        })
    }

    pub async fn send_mint_alert(&self, event: &MintEvent) -> Result<()> {
        let text = format_alert(event);
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );

        let body = json!({
            "chat_id": self.chat_id,
            "text": text,
            "parse_mode": "HTML",
            "disable_web_page_preview": false
        });

        let mut attempt = 0u32;
        loop {
            let response = self.client.post(&url).json(&body).send().await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        debug!(mint = %event.mint, "telegram alert sent");
                        return Ok(());
                    }

                    let body_text = resp.text().await.unwrap_or_default();
                    if status.as_u16() == 429 || status.is_server_error() {
                        if attempt >= self.retry_max {
                            return Err(anyhow!(
                                "Telegram HTTP {status} after retries: {body_text}"
                            ));
                        }
                    } else {
                        return Err(anyhow!("Telegram HTTP {status}: {body_text}"));
                    }
                }
                Err(e) => {
                    if attempt >= self.retry_max {
                        return Err(e).context("Telegram request failed");
                    }
                }
            }

            let delay = self.retry_base_ms.saturating_mul(1u64 << attempt.min(6));
            tokio::time::sleep(Duration::from_millis(delay)).await;
            attempt += 1;
        }
    }
}

fn format_alert(event: &MintEvent) -> String {
    let name = event
        .name
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown");
    let symbol = event
        .symbol
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("—");

    let time = match event.block_time {
        Some(ts) => Utc
            .timestamp_opt(ts, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "Unknown".to_string()),
        None => "Unknown".to_string(),
    };

    format!(
        "🪙 <b>New SPL Mint</b>\n\n\
         <b>Name:</b> {name}\n\
         <b>Symbol:</b> {symbol}\n\
         <b>Mint:</b> <code>{mint}</code>\n\
         <b>Creator:</b> <code>{creator}</code>\n\
         <b>Program:</b> {program}\n\
         <b>Time:</b> {time}\n\
         <b>Tx:</b> https://solscan.io/tx/{sig}\n\
         <b>Token:</b> https://solscan.io/token/{mint}",
        name = html_escape(name),
        symbol = html_escape(symbol),
        mint = html_escape(&event.mint),
        creator = html_escape(&event.creator),
        program = html_escape(event.program.label()),
        time = html_escape(&time),
        sig = html_escape(&event.signature),
    )
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{MintEvent, TokenProgram};

    #[test]
    fn format_alert_includes_fields() {
        let event = MintEvent {
            mint: "Mint111".into(),
            creator: "Creator222".into(),
            signature: "Sig333".into(),
            slot: 1,
            block_time: Some(1_700_000_000),
            name: Some("Test Token".into()),
            symbol: Some("TEST".into()),
            program: TokenProgram::Spl,
        };

        let text = format_alert(&event);
        assert!(text.contains("Test Token"));
        assert!(text.contains("TEST"));
        assert!(text.contains("Mint111"));
        assert!(text.contains("Creator222"));
        assert!(text.contains("solscan.io/token/Mint111"));
    }
}
