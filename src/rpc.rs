use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use base64::Engine;
use reqwest::Client;
use serde_json::{json, Value};
use tracing::{debug, warn};

use crate::config::Config;
use crate::metadata;
use crate::types::{
    AccountInfoResult, Instruction, MintEvent, RpcResponse, TokenProgram, TransactionResult,
    is_first_supply_mint, is_mint_to_type,
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

        let meta = tx.meta.as_ref().ok_or_else(|| anyhow!("transaction meta missing"))?;

        if meta.err.is_some() {
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

        let mut candidates = Vec::new();

        for inst in &message.instructions {
            if let Some(found) = extract_mint_to_from_instruction(inst) {
                candidates.push(found);
            }
        }

        if let Some(inner_groups) = meta.inner_instructions.as_ref() {
            for group in inner_groups {
                for inst in &group.instructions {
                    if let Some(found) = extract_mint_to_from_instruction(inst) {
                        candidates.push(found);
                    }
                }
            }
        }

        let (mint, program) = match candidates
            .into_iter()
            .find(|(mint, _)| is_first_supply_mint(meta, mint))
        {
            Some(found) => found,
            None => {
                debug!(signature, "no first-supply mintTo found");
                return Ok(None);
            }
        };

        let (name, symbol) = if self.fetch_metadata {
            match tokio::time::timeout(
                self.metadata_timeout,
                metadata::resolve(self, &mint, program),
            )
            .await
            {
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

    pub(crate) async fn get_account_bytes(&self, address: &str) -> Result<Option<Vec<u8>>> {
        let params = json!([
            address,
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

        let Some(data_b64) = data_b64 else {
            return Ok(None);
        };

        let bytes = base64::engine::general_purpose::STANDARD
            .decode(data_b64)
            .context("invalid base64 account data")?;

        Ok(Some(bytes))
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

fn extract_mint_to_from_instruction(inst: &Instruction) -> Option<(String, TokenProgram)> {
    let program_id = inst.program_id.as_deref()?;
    let token_program = TokenProgram::from_program_id(program_id)?;

    let parsed = inst.parsed.as_ref()?;
    if !is_mint_to_type(&parsed.inst_type) {
        return None;
    }

    if let Some(info) = &parsed.info
        && let Some(mint) = info.get("mint").and_then(|v| v.as_str())
    {
        return Some((mint.to_string(), token_program));
    }

    let accounts = inst.accounts.as_ref()?;
    let mint = accounts.first()?.clone();
    Some((mint, token_program))
}
