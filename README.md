# SolMintWatch

**Real-time alerts when new tokens receive their first supply on Solana.**

SolMintWatch watches Solana mainnet around the clock and notifies you the moment a token is minted for the first time — not when the mint account is created, but when tokens actually enter circulation. Alerts include the token name, symbol, mint address, creator wallet, and direct links to Solscan.

Ideal for traders, researchers, and teams who want early visibility into new SPL and Token-2022 launches without manually scanning the chain.

---

## What you get

Each alert includes:

- **Token name & symbol** (when available)
- **Mint address** — the token’s on-chain identifier
- **Creator wallet** — who initiated the mint
- **Timestamp** — when it happened on-chain
- **Links** — one-click view on Solscan (transaction + token page)

Alerts can be delivered to your **terminal** (for testing) or **Telegram** (for production use).

---

## Quick start

### 1. Prerequisites

- [Rust](https://rustup.rs/) 1.96 or newer
- A [Helius](https://helius.dev/) API key (free tier works)
- *(Optional)* A Telegram bot if you want mobile alerts

### 2. Configure

```bash
cp .env.example .env
```

Open `.env` and set your values:

| Setting | What to enter |
|---------|---------------|
| `HELIUS_API_KEY` | Your Helius API key |
| `ALERT_MODE` | `stdout` (terminal) or `telegram` |
| `TELEGRAM_BOT_TOKEN` | Bot token from [@BotFather](https://t.me/BotFather) — required for Telegram |
| `TELEGRAM_CHAT_ID` | Your chat or group ID — required for Telegram |

**Telegram setup (one-time):**

1. Message [@BotFather](https://t.me/BotFather) on Telegram and create a new bot. Copy the bot token into `TELEGRAM_BOT_TOKEN`.
2. Start a chat with your new bot (or add it to a group).
3. Get your chat ID — message [@userinfobot](https://t.me/userinfobot) or use the Telegram API — and paste it into `TELEGRAM_CHAT_ID`.

### 3. Run

```bash
cargo run --release
```

For a production build you can run the binary directly:

```bash
cargo build --release
./target/release/solmintwatch
```

The service connects to Helius, starts listening, and sends alerts as new first-supply mints appear.

---

## Example alert

**Telegram:**

```
🪙 First Token Supply

Name: My Token
Symbol: MTK
Mint: 7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU
Creator: 9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM
Program: SPL Token
Time: 2026-07-03 10:42:15 UTC
Tx: https://solscan.io/tx/...
Token: https://solscan.io/token/...
```

**Terminal** (`ALERT_MODE=stdout`) prints the same fields in plain text — useful for local testing before switching to Telegram.

---

## How it works

SolMintWatch monitors Solana’s token programs (SPL Token and Token-2022) via Helius WebSocket. When it detects a **first mint** — the initial time supply is created for a token — it:

1. Confirms this is the token’s **first supply** (not a re-mint or empty mint creation)
2. Skips **NFTs** (total supply of 1 base unit)
3. Looks up token name and symbol when possible (Metaplex for SPL; on-chain metadata for Token-2022)
4. Sends a formatted alert to your chosen destination

Duplicate events are filtered automatically, so you only see each new token once.

---

## Configuration reference

### Environment variables (`.env`)

| Variable | Required | Description |
|----------|----------|-------------|
| `HELIUS_API_KEY` | Yes | Helius API key for WebSocket and RPC |
| `ALERT_MODE` | Yes | `stdout` or `telegram` |
| `TELEGRAM_BOT_TOKEN` | If using Telegram | Bot token from BotFather |
| `TELEGRAM_CHAT_ID` | If using Telegram | Target chat or group ID |

### Advanced settings

Other options (network, metadata timeout, retry behavior, logging) are defined as constants in [`src/config.rs`](src/config.rs). Defaults are tuned for mainnet production use. Change them there if you need devnet, different timeouts, or custom log levels.

| Setting | Default | Purpose |
|---------|---------|---------|
| `SOLANA_NETWORK` | `mainnet` | `mainnet` or `devnet` |
| `COMMITMENT` | `confirmed` | How finalized a block must be before alerting |
| `FETCH_METADATA` | `true` | Whether to fetch token name/symbol |
| `METADATA_TIMEOUT_SECS` | `2` | Max wait for metadata lookup |
| `WS_PING_INTERVAL_SECS` | `30` | WebSocket keepalive interval |
| `HTTP_RETRY_MAX` | `3` | Retries for RPC and Telegram calls |
| `HTTP_RETRY_BASE_MS` | `500` | Backoff between retries (ms) |
| `RUST_LOG` | `info,solmintwatch=debug` | Log verbosity (overridable via env) |

---

## Security

- `.env` is gitignored — never commit API keys, bot tokens, or chat IDs.
- Keep your Helius key and Telegram credentials private; treat them like passwords.

---

## For developers

```
src/
├── main.rs       entry + shutdown
├── config.rs     settings, program IDs, shared HTTP helpers
├── listener.rs   websocket, dedup, event loop
├── rpc.rs        Helius RPC + mint event builder
├── metadata.rs   SPL / Token-2022 / Metaplex name resolution
├── types.rs      MintEvent, RPC types, supply filters
└── alert.rs      stdout + Telegram alerts
```
