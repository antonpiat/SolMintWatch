use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tracing::{debug, warn};

use crate::config::Config;
use crate::constants::METAPLEX_METADATA_PROGRAM;
use crate::types::{
    AccountInfoResult, Instruction, MintEvent, RpcResponse, TokenProgram, TransactionResult,
    is_initialize_mint_type,
};

#[derive(Clone)]
pub struct HeliusRpc {
    client: Client,
    rpc_url: String,
    commitment: String,
    fetch_metadata: bool,
    metadata_timeout: Duration,
    retry_max: u32,
    retry_base_ms: u64,
}

impl HeliusRpc {
    pub fn new(config: &Config) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .context("failed to build HTTP client")?;

        Ok(Self {
            client,
            rpc_url: config.rpc_url.clone(),
            commitment: config.commitment.clone(),
            fetch_metadata: config.fetch_metadata,
            metadata_timeout: config.metadata_timeout,
            retry_max: config.rpc_retry_max,
            retry_base_ms: config.rpc_retry_base_ms,
        })
    }

    pub async fn build_mint_event(&self, signature: &str) -> Result<Option<MintEvent>> {
        let tx = match self.get_transaction(signature).await? {
            Some(tx) => tx,
            None => {
                debug!(signature, "transaction not found yet");
                return Ok(None);
            }
        };

        if tx.meta.as_ref().and_then(|m| m.err.as_ref()).is_some() {
            debug!(signature, "skipping failed transaction");
            return Ok(None);
        }

        let transaction = tx
            .transaction
            .as_ref()
            .ok_or_else(|| anyhow!("transaction body missing"))?;
        let message = &transaction.message;

        let creator = message
            .account_keys
            .first()
            .map(|k| k.pubkey().to_string())
            .ok_or_else(|| anyhow!("no account keys in transaction"))?;

        let mut mint: Option<String> = None;
        let mut program = TokenProgram::Spl;

        for inst in &message.instructions {
            if let Some((found, prog)) = extract_mint_from_instruction(inst) {
                mint = Some(found);
                program = prog;
                break;
            }
        }

        if mint.is_none()
            && let Some(inner_groups) = tx.meta.as_ref().and_then(|m| m.inner_instructions.as_ref())
        {
            'outer: for group in inner_groups {
                for inst in &group.instructions {
                    if let Some((found, prog)) = extract_mint_from_instruction(inst) {
                        mint = Some(found);
                        program = prog;
                        break 'outer;
                    }
                }
            }
        }

        let mint = match mint {
            Some(m) => m,
            None => {
                debug!(signature, "no initializeMint instruction found");
                return Ok(None);
            }
        };

        let (name, symbol) = if self.fetch_metadata {
            match tokio::time::timeout(self.metadata_timeout, self.fetch_metadata(&mint)).await {
                Ok(Ok(meta)) => meta,
                Ok(Err(e)) => {
                    warn!(mint, error = %e, "metadata fetch failed");
                    (None, None)
                }
                Err(_) => {
                    debug!(mint, "metadata fetch timed out");
                    (None, None)
                }
            }
        } else {
            (None, None)
        };

        Ok(Some(MintEvent {
            mint,
            creator,
            signature: signature.to_string(),
            slot: tx.slot,
            block_time: tx.block_time,
            name,
            symbol,
            program,
        }))
    }

    async fn get_transaction(&self, signature: &str) -> Result<Option<TransactionResult>> {
        let params = json!([
            signature,
            {
                "encoding": "jsonParsed",
                "commitment": self.commitment,
                "maxSupportedTransactionVersion": 0
            }
        ]);

        let response: RpcResponse<TransactionResult> =
            self.rpc_call("getTransaction", params).await?;
        if let Some(err) = response.error {
            return Err(anyhow!("RPC error {}: {}", err.code, err.message));
        }
        Ok(response.result)
    }

    async fn fetch_metadata(&self, mint: &str) -> Result<(Option<String>, Option<String>)> {
        let metadata_pda = derive_metadata_pda(mint)?;
        let params = json!([
            metadata_pda,
            {
                "encoding": "base64",
                "commitment": self.commitment
            }
        ]);

        let response: RpcResponse<AccountInfoResult> =
            self.rpc_call("getAccountInfo", params).await?;
        if let Some(err) = response.error {
            return Err(anyhow!("RPC error {}: {}", err.code, err.message));
        }

        let data_b64 = response
            .result
            .and_then(|r| r.value)
            .and_then(|v| v.data.base64_data().map(str::to_string));

        let data_b64 = match data_b64 {
            Some(d) => d,
            None => return Ok((None, None)),
        };

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_b64)
            .context("invalid base64 metadata")?;

        Ok(parse_metaplex_metadata(&bytes))
    }

    async fn rpc_call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        params: Value,
    ) -> Result<T> {
        let body = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": method,
            "params": params
        });

        let mut attempt = 0u32;
        loop {
            let response = self
                .client
                .post(&self.rpc_url)
                .json(&body)
                .send()
                .await;

            match response {
                Ok(resp) => {
                    let status = resp.status();
                    if status.as_u16() == 429 || status.is_server_error() {
                        if attempt >= self.retry_max {
                            return Err(anyhow!("RPC HTTP {status} after retries"));
                        }
                        self.backoff(attempt).await;
                        attempt += 1;
                        continue;
                    }

                    return resp
                        .json::<T>()
                        .await
                        .with_context(|| format!("failed to decode RPC response for {method}"));
                }
                Err(e) => {
                    if attempt >= self.retry_max {
                        return Err(e).with_context(|| format!("RPC request failed for {method}"));
                    }
                    self.backoff(attempt).await;
                    attempt += 1;
                }
            }
        }
    }

    async fn backoff(&self, attempt: u32) {
        let delay = self.retry_base_ms.saturating_mul(1u64 << attempt.min(6));
        tokio::time::sleep(Duration::from_millis(delay)).await;
    }
}

fn extract_mint_from_instruction(inst: &Instruction) -> Option<(String, TokenProgram)> {
    let program_id = inst.program_id.as_deref()?;
    let token_program = TokenProgram::from_program_id(program_id)?;

    let parsed = inst.parsed.as_ref()?;
    if !is_initialize_mint_type(&parsed.inst_type) {
        return None;
    }

    if let Some(info) = &parsed.info {
        if let Some(mint) = info.get("mint").and_then(|v| v.as_str()) {
            return Some((mint.to_string(), token_program));
        }
        if let Some(account) = info.get("account").and_then(|v| v.as_str()) {
            return Some((account.to_string(), token_program));
        }
    }

    let accounts = inst.accounts.as_ref()?;
    let mint = accounts.first()?.clone();
    Some((mint, token_program))
}

fn derive_metadata_pda(mint: &str) -> Result<String> {
    let mint_bytes = bs58::decode(mint)
        .into_vec()
        .context("invalid mint base58")?;
    let metadata_program = bs58::decode(METAPLEX_METADATA_PROGRAM)
        .into_vec()
        .context("invalid metadata program base58")?;

    if mint_bytes.len() != 32 || metadata_program.len() != 32 {
        return Err(anyhow!("pubkey must be 32 bytes"));
    }

    let mint_arr: [u8; 32] = mint_bytes.try_into().map_err(|_| anyhow!("invalid mint length"))?;
    let program_arr: [u8; 32] = metadata_program
        .try_into()
        .map_err(|_| anyhow!("invalid program length"))?;

    let (pda, _) = find_program_address(
        &[
            b"metadata",
            program_arr.as_ref(),
            mint_arr.as_ref(),
        ],
        &program_arr,
    )
    .context("failed to derive metadata PDA")?;

    Ok(bs58::encode(pda).into_string())
}

fn find_program_address(seeds: &[&[u8]], program_id: &[u8; 32]) -> Result<([u8; 32], u8)> {
    for bump in (0u8..=255).rev() {
        let hash = create_program_address(seeds, bump, program_id);
        if !is_on_curve(&hash) {
            return Ok((hash, bump));
        }
    }
    Err(anyhow!("unable to find valid program address bump"))
}

fn create_program_address(seeds: &[&[u8]], bump: u8, program_id: &[u8; 32]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for seed in seeds {
        hasher.update(seed);
    }
    hasher.update([bump]);
    hasher.update(program_id);
    hasher.update(b"ProgramDerivedAddress");
    hasher.finalize().into()
}

/// Returns true if the bytes represent a point on the ed25519 curve.
fn is_on_curve(bytes: &[u8; 32]) -> bool {
    curve25519_dalek::edwards::CompressedEdwardsY(*bytes)
        .decompress()
        .is_some()
}

fn parse_metaplex_metadata(data: &[u8]) -> (Option<String>, Option<String>) {
    if data.len() < 65 {
        return (None, None);
    }

    let name = read_borsh_string(&data[65..]);
    let name_len = 4 + name.as_ref().map(|s| s.len()).unwrap_or(0);
    let symbol_start = 65 + name_len;
    if symbol_start > data.len() {
        return (name, None);
    }

    let symbol = read_borsh_string(&data[symbol_start..]);
    (name, symbol)
}

fn read_borsh_string(data: &[u8]) -> Option<String> {
    if data.len() < 4 {
        return None;
    }
    let len = u32::from_le_bytes(data[..4].try_into().ok()?) as usize;
    if data.len() < 4 + len {
        return None;
    }
    let raw = &data[4..4 + len];
    let s = String::from_utf8_lossy(raw).trim_matches('\0').trim().to_string();
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}
