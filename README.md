# solmintwatch

Production async Rust service that monitors the **first real token supply** (`MintTo`) for SPL and Token-2022 mints on Solana mainnet via Helius WebSocket, deduplicates mints in memory, and sends alerts to stdout or Telegram.

## Quick start

```bash
cp .env.example .env
# edit .env with HELIUS_API_KEY, ALERT_MODE, and Telegram creds if needed
cargo run --release
```

## What it does

1. Subscribes to Helius `logsSubscribe` for **SPL Token** and **Token-2022** programs
2. Filters for `MintTo` / `MintToChecked`, then verifies **first supply only** (no pre-tx balances for that mint)
3. Ignores `InitializeMint` and later re-mints
4. Dedupes by transaction signature and mint address
5. Fetches the transaction to extract mint address, creator wallet, and timestamp
6. Optionally fetches token name/symbol (2s timeout) — SPL via Metaplex; Token-2022 via [MetadataPointer + TokenMetadata](https://solana.com/docs/tokens/extensions/metadata)
7. Sends alerts to stdout or Telegram (`ALERT_MODE` in `.env`)

## Configuration

Secrets go in `.env` (see `.env.example`). Other settings are constants at the top of [`src/config.rs`](src/config.rs):

| Constant | Default | Description |
|----------|---------|-------------|
| `SOLANA_NETWORK` | `mainnet` | `mainnet` or `devnet` |
| `COMMITMENT` | `confirmed` | RPC commitment level |
| `FETCH_METADATA` | `true` | Fetch token name/symbol |
| `METADATA_TIMEOUT_SECS` | `2` | Metadata fetch timeout |
| `WS_PING_INTERVAL_SECS` | `30` | WebSocket keepalive interval |
| `HTTP_RETRY_MAX` | `3` | HTTP retry attempts (RPC + Telegram) |
| `HTTP_RETRY_BASE_MS` | `500` | Retry backoff base (ms) |
| `RUST_LOG` | `info,solmintwatch=debug` | Default log filter (overridable via `RUST_LOG` env) |

`.env` variables:

| Variable | Description |
|----------|-------------|
| `HELIUS_API_KEY` | Helius API key |
| `ALERT_MODE` | `stdout` or `telegram` |
| `TELEGRAM_BOT_TOKEN` | Required when `ALERT_MODE=telegram` |
| `TELEGRAM_CHAT_ID` | Required when `ALERT_MODE=telegram` |

## Project layout

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

## Build

Requires Rust 1.96+ (edition 2024).

```bash
cargo build --release
./target/release/solmintwatch
```

## Safety

`.env` is gitignored. Do not commit your bot token, chat ID, or Helius API key.
