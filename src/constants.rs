// Network
pub const SOLANA_NETWORK: &str = "mainnet";
pub const COMMITMENT: &str = "confirmed";

// Behavior
pub const FETCH_METADATA: bool = true;
pub const METADATA_TIMEOUT_SECS: u64 = 2;
pub const WS_PING_INTERVAL_SECS: u64 = 30;
pub const RPC_RETRY_MAX: u32 = 3;
pub const RPC_RETRY_BASE_MS: u64 = 500;
pub const RUST_LOG: &str = "info,solmintwatch=debug";

// Program IDs
pub const SPL_TOKEN_PROGRAM: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
pub const TOKEN_2022_PROGRAM: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";
pub const METAPLEX_METADATA_PROGRAM: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";

pub const TOKEN_PROGRAMS: [&str; 2] = [SPL_TOKEN_PROGRAM, TOKEN_2022_PROGRAM];
