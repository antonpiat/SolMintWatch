# solmintwatch

Production async Rust service that monitors new SPL Token mint creation events on Solana mainnet via Helius WebSocket, deduplicates mints in memory, and sends real-time Telegram alerts.

## Quick start

```bash
cp .env.example .env
# edit .env with HELIUS_API_KEY, TELEGRAM_BOT_TOKEN, TELEGRAM_CHAT_ID
cargo run --release
```

## What it does

1. Subscribes to Helius `logsSubscribe` for **SPL Token** and **Token-2022** programs
2. Filters for `InitializeMint` / `InitializeMint2` only (ignores transfers, `MintTo`, metadata updates)
3. Dedupes by transaction signature and mint address
4. Fetches the transaction to extract mint address, creator wallet, and timestamp
5. Optionally fetches Metaplex name/symbol (2s timeout)
6. Sends a formatted Telegram alert with Solscan links

## Configuration

Secrets go in `.env` (see `.env.example`). All other settings are in `Cargo.toml` under `[package.metadata.solmintwatch]`:

| Setting | Default | Description |
|---------|---------|-------------|
| `solana-network` | `mainnet` | `mainnet` or `devnet` |
| `commitment` | `confirmed` | RPC commitment level |
| `fetch-metadata` | `true` | Fetch Metaplex name/symbol |
| `metadata-timeout-secs` | `2` | Metadata fetch timeout |
| `ws-ping-interval-secs` | `30` | WebSocket keepalive interval |
| `rpc-retry-max` | `3` | HTTP retry attempts |
| `rpc-retry-base-ms` | `500` | Retry backoff base (ms) |
| `rust-log` | `info,solmintwatch=debug` | Default log filter (overridable via `RUST_LOG` env) |
| `spl-token-program` | `Tokenkeg...` | Classic SPL Token program ID |
| `token-2022-program` | `TokenzQd...` | Token-2022 program ID |
| `metaplex-metadata-program` | `metaqbxx...` | Metaplex metadata program ID |

`.env` secrets:

| Variable | Description |
|----------|-------------|
| `HELIUS_API_KEY` | Helius API key |
| `TELEGRAM_BOT_TOKEN` | Telegram bot token |
| `TELEGRAM_CHAT_ID` | Target chat ID |

## Project layout

```
solmintwatch/
├── Cargo.toml
├── .env.example
└── src/
    ├── main.rs
    ├── config.rs
    ├── types.rs
    ├── dedup.rs
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
