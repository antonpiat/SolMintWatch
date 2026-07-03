use anyhow::{anyhow, bail, Context, Result};
use chrono::{TimeZone, Utc};
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use crate::config::{http_backoff, http_client, Config};
use crate::types::MintEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertMode {
    Stdout,
    Telegram,
}

impl AlertMode {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "stdout" => Ok(Self::Stdout),
            "telegram" => Ok(Self::Telegram),
            other => bail!("unsupported ALERT_MODE: {other} (use stdout or telegram)"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Telegram => "telegram",
        }
    }
}

#[derive(Clone)]
pub struct AlertClient {
    mode: AlertMode,
    telegram: Option<TelegramSender>,
}

impl AlertClient {
    pub fn new(config: &Config) -> Result<Self> {
        let telegram = match config.alert_mode {
            AlertMode::Stdout => None,
            AlertMode::Telegram => Some(TelegramSender::new(config)?),
        };
        Ok(Self {
            mode: config.alert_mode,
            telegram,
        })
    }

    pub async fn send_mint_alert(&self, event: &MintEvent) -> Result<()> {
        match self.mode {
            AlertMode::Stdout => println!("{}", format_plain(event)),
            AlertMode::Telegram => {
                self.telegram
                    .as_ref()
                    .context("telegram not configured")?
                    .send(event)
                    .await?;
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
struct TelegramSender {
    client: Client,
    bot_token: String,
    chat_id: String,
    retry_max: u32,
    retry_base_ms: u64,
}

impl TelegramSender {
    fn new(config: &Config) -> Result<Self> {
        Ok(Self {
            client: http_client()?,
            bot_token: config
                .telegram_bot_token
                .clone()
                .context("TELEGRAM_BOT_TOKEN missing")?,
            chat_id: config
                .telegram_chat_id
                .clone()
                .context("TELEGRAM_CHAT_ID missing")?,
            retry_max: config.http_retry_max,
            retry_base_ms: config.http_retry_base_ms,
        })
    }

    async fn send(&self, event: &MintEvent) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.bot_token
        );
        let body = json!({
            "chat_id": self.chat_id,
            "text": format_html(event),
            "parse_mode": "HTML",
            "disable_web_page_preview": false
        });

        let mut attempt = 0u32;
        loop {
            match self.client.post(&url).json(&body).send().await {
                Ok(resp) => {
                    let status = resp.status();
                    if status.is_success() {
                        debug!(mint = %event.mint, "telegram alert sent");
                        return Ok(());
                    }
                    let body_text = resp.text().await.unwrap_or_default();
                    if !(status.as_u16() == 429 || status.is_server_error()) {
                        return Err(anyhow!("Telegram HTTP {status}: {body_text}"));
                    }
                    if attempt >= self.retry_max {
                        return Err(anyhow!(
                            "Telegram HTTP {status} after retries: {body_text}"
                        ));
                    }
                }
                Err(e) => {
                    if attempt >= self.retry_max {
                        return Err(e).context("Telegram request failed");
                    }
                }
            }
            http_backoff(attempt, self.retry_base_ms).await;
            attempt += 1;
        }
    }
}

fn format_plain(event: &MintEvent) -> String {
    format!(
        "First Token Supply\n\
         Name: {name}\n\
         Symbol: {symbol}\n\
         Mint: {mint}\n\
         Creator: {creator}\n\
         Program: {program}\n\
         Time: {time}\n\
         Tx: https://solscan.io/tx/{sig}\n\
         Token: https://solscan.io/token/{mint}",
        name = display_name(event),
        symbol = display_symbol(event),
        mint = event.mint,
        creator = event.creator,
        program = event.program.label(),
        time = display_time(event),
        sig = event.signature,
    )
}

fn format_html(event: &MintEvent) -> String {
    format!(
        "🪙 <b>First Token Supply</b>\n\n\
         <b>Name:</b> {name}\n\
         <b>Symbol:</b> {symbol}\n\
         <b>Mint:</b> <code>{mint}</code>\n\
         <b>Creator:</b> <code>{creator}</code>\n\
         <b>Program:</b> {program}\n\
         <b>Time:</b> {time}\n\
         <b>Tx:</b> https://solscan.io/tx/{sig}\n\
         <b>Token:</b> https://solscan.io/token/{mint}",
        name = html_escape(display_name(event)),
        symbol = html_escape(display_symbol(event)),
        mint = html_escape(&event.mint),
        creator = html_escape(&event.creator),
        program = html_escape(event.program.label()),
        time = html_escape(&display_time(event)),
        sig = html_escape(&event.signature),
    )
}

fn display_name(event: &MintEvent) -> &str {
    event
        .name
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown")
}

fn display_symbol(event: &MintEvent) -> &str {
    event
        .symbol
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("—")
}

fn display_time(event: &MintEvent) -> String {
    match event.block_time {
        Some(ts) => Utc
            .timestamp_opt(ts, 0)
            .single()
            .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
            .unwrap_or_else(|| "Unknown".to_string()),
        None => "Unknown".to_string(),
    }
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
    use crate::types::TokenProgram;

    fn sample_event() -> MintEvent {
        MintEvent {
            mint: "Mint111".into(),
            creator: "Creator222".into(),
            signature: "Sig333".into(),
            block_time: Some(1_700_000_000),
            name: Some("Test Token".into()),
            symbol: Some("TEST".into()),
            program: TokenProgram::Spl,
        }
    }

    #[test]
    fn parse_alert_modes() {
        assert_eq!(AlertMode::parse("stdout").unwrap(), AlertMode::Stdout);
        assert_eq!(AlertMode::parse("Telegram").unwrap(), AlertMode::Telegram);
        assert!(AlertMode::parse("both").is_err());
    }

    #[test]
    fn plain_alert_includes_fields() {
        let text = format_plain(&sample_event());
        assert!(text.contains("Test Token"));
        assert!(text.contains("solscan.io/token/Mint111"));
    }

    #[test]
    fn html_alert_includes_fields() {
        let text = format_html(&sample_event());
        assert!(text.contains("Test Token"));
        assert!(text.contains("solscan.io/token/Mint111"));
    }
}
