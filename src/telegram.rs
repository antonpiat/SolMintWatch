use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use reqwest::Client;
use serde_json::json;
use tracing::debug;

use crate::alert::format_alert_html;
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
        let bot_token = config
            .telegram_bot_token
            .as_ref()
            .context("telegram bot token missing")?
            .clone();
        let chat_id = config
            .telegram_chat_id
            .as_ref()
            .context("telegram chat id missing")?
            .clone();

        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("failed to build Telegram HTTP client")?;

        Ok(Self {
            client,
            bot_token,
            chat_id,
            retry_max: config.rpc_retry_max,
            retry_base_ms: config.rpc_retry_base_ms,
        })
    }

    pub async fn send_mint_alert(&self, event: &MintEvent) -> Result<()> {
        let text = format_alert_html(event);
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
