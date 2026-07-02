use anyhow::{bail, Context, Result};
use chrono::{TimeZone, Utc};

use crate::config::Config;
use crate::telegram::TelegramClient;
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
    telegram: Option<TelegramClient>,
}

impl AlertClient {
    pub fn new(config: &Config) -> Result<Self> {
        let telegram = match config.alert_mode {
            AlertMode::Stdout => None,
            AlertMode::Telegram => Some(TelegramClient::new(config)?),
        };

        Ok(Self {
            mode: config.alert_mode,
            telegram,
        })
    }

    pub async fn send_mint_alert(&self, event: &MintEvent) -> Result<()> {
        match self.mode {
            AlertMode::Stdout => println!("{}", format_alert_plain(event)),
            AlertMode::Telegram => {
                let telegram = self
                    .telegram
                    .as_ref()
                    .context("telegram client not configured")?;
                telegram.send_mint_alert(event).await?;
            }
        }

        Ok(())
    }
}

pub fn format_alert_plain(event: &MintEvent) -> String {
    let name = display_name(event);
    let symbol = display_symbol(event);
    let time = display_time(event);

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
        name = name,
        symbol = symbol,
        mint = event.mint,
        creator = event.creator,
        program = event.program.label(),
        time = time,
        sig = event.signature,
    )
}

pub fn format_alert_html(event: &MintEvent) -> String {
    let name = display_name(event);
    let symbol = display_symbol(event);
    let time = display_time(event);

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
        name = html_escape(name),
        symbol = html_escape(symbol),
        mint = html_escape(&event.mint),
        creator = html_escape(&event.creator),
        program = html_escape(event.program.label()),
        time = html_escape(&time),
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
            slot: 1,
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
        assert!(AlertMode::parse("invalid").is_err());
    }

    #[test]
    fn plain_alert_includes_fields() {
        let text = format_alert_plain(&sample_event());
        assert!(text.contains("Test Token"));
        assert!(text.contains("solscan.io/token/Mint111"));
    }

    #[test]
    fn html_alert_includes_fields() {
        let text = format_alert_html(&sample_event());
        assert!(text.contains("Test Token"));
        assert!(text.contains("solscan.io/token/Mint111"));
    }
}
