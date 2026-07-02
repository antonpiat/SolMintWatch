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
3. Ignores `InitializeMint` (mint account creation with zero supply) and later re-mints
3. Dedupes by transaction signature and mint address
4. Fetches the transaction to extract mint address, creator wallet, and timestamp
5. Optionally fetches token name/symbol (2s timeout) — SPL via Metaplex; Token-2022 via [MetadataPointer + TokenMetadata extensions](https://solana.com/docs/tokens/extensions/metadata)
6. Sends alerts to stdout or Telegram (set via `ALERT_MODE` in `.env`)

## Configuration

Secrets and alert routing go in `.env` (see `.env.example`). Other settings are in [`src/constants.rs`](src/constants.rs):

| Constant | Default | Description |
|----------|---------|-------------|
| `SOLANA_NETWORK` | `mainnet` | `mainnet` or `devnet` |
| `COMMITMENT` | `confirmed` | RPC commitment level |
| `FETCH_METADATA` | `true` | Fetch token name/symbol |
| `METADATA_TIMEOUT_SECS` | `2` | Metadata fetch timeout |
| `WS_PING_INTERVAL_SECS` | `30` | WebSocket keepalive interval |
| `RPC_RETRY_MAX` | `3` | HTTP retry attempts |
| `RPC_RETRY_BASE_MS` | `500` | Retry backoff base (ms) |
| `RUST_LOG` | `info,solmintwatch=debug` | Default log filter (overridable via `RUST_LOG` env) |
| `SPL_TOKEN_PROGRAM` | `Tokenkeg...` | Classic SPL Token program ID |
| `TOKEN_2022_PROGRAM` | `TokenzQd...` | Token-2022 program ID |
| `METAPLEX_METADATA_PROGRAM` | `metaqbxx...` | Metaplex metadata program ID |

`.env` variables:

| Variable | Description |
|----------|-------------|
| `HELIUS_API_KEY` | Helius API key |
| `ALERT_MODE` | `stdout` or `telegram` |
| `TELEGRAM_BOT_TOKEN` | Telegram bot token (required when `ALERT_MODE=telegram`) |
| `TELEGRAM_CHAT_ID` | Target chat ID (required when `ALERT_MODE=telegram`) |

## Project layout

```
solmintwatch/
├── Cargo.toml
├── .env.example
└── src/
    ├── main.rs
    ├── config.rs
    ├── constants.rs
    ├── alert.rs
    ├── types.rs
    ├── dedup.rs
    ├── metadata.rs
    ├── rpc.rs
    ├── listener.rs
    └── telegram.rs
```

## Build

Requires Rust 1.96+ (edition 2024).

```bash
cargo build --release
./target/release/solmintwatch
```

## Safety

`.env` is gitignored. Do not commit your bot token, chat ID, or Helius API key.
