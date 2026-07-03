use serde::Deserialize;
use serde_json::Value;

use crate::config::{SPL_TOKEN_PROGRAM, TOKEN_2022_PROGRAM};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenProgram {
    Spl,
    Token2022,
}

impl TokenProgram {
    pub fn label(self) -> &'static str {
        match self {
            Self::Spl => "SPL Token",
            Self::Token2022 => "Token-2022",
        }
    }

    pub fn from_program_id(program_id: &str) -> Option<Self> {
        match program_id {
            SPL_TOKEN_PROGRAM => Some(Self::Spl),
            TOKEN_2022_PROGRAM => Some(Self::Token2022),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MintEvent {
    pub mint: String,
    pub creator: String,
    pub signature: String,
    pub block_time: Option<i64>,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub program: TokenProgram,
}

#[derive(Debug, Deserialize)]
pub struct RpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct TransactionResult {
    #[serde(rename = "blockTime")]
    pub block_time: Option<i64>,
    pub meta: Option<TransactionMeta>,
    pub transaction: Option<TransactionBody>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionMeta {
    pub err: Option<Value>,
    #[serde(rename = "innerInstructions")]
    pub inner_instructions: Option<Vec<InnerInstructionGroup>>,
}

#[derive(Debug, Deserialize)]
pub struct InnerInstructionGroup {
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Deserialize)]
pub struct TransactionBody {
    pub message: TransactionMessage,
}

#[derive(Debug, Deserialize)]
pub struct TransactionMessage {
    #[serde(rename = "accountKeys")]
    pub account_keys: Vec<AccountKey>,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AccountKey {
    Parsed { pubkey: String },
    Raw(String),
}

impl AccountKey {
    pub fn pubkey(&self) -> &str {
        match self {
            Self::Parsed { pubkey } => pubkey,
            Self::Raw(s) => s,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Instruction {
    #[serde(rename = "programId")]
    pub program_id: Option<String>,
    pub accounts: Option<Vec<String>>,
    pub parsed: Option<ParsedInstruction>,
}

#[derive(Debug, Deserialize)]
pub struct ParsedInstruction {
    #[serde(rename = "type")]
    pub inst_type: String,
    pub info: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct AccountInfoResult {
    pub value: Option<AccountValue>,
}

#[derive(Debug, Deserialize)]
pub struct AccountValue {
    pub data: (String, String),
}

#[derive(Debug, Deserialize)]
pub struct LogsNotification {
    pub params: LogsParams,
}

#[derive(Debug, Deserialize)]
pub struct LogsParams {
    pub result: LogsResult,
}

#[derive(Debug, Deserialize)]
pub struct LogsResult {
    pub value: LogsValue,
}

#[derive(Debug, Deserialize)]
pub struct LogsValue {
    pub signature: String,
    pub logs: Vec<String>,
    pub err: Option<Value>,
}

pub fn is_mint_to_log(logs: &[String]) -> bool {
    logs.iter().any(|line| {
        line.contains("Instruction: MintTo") || line.contains("Instruction: MintToChecked")
    })
}

pub fn is_mint_to_type(inst_type: &str) -> bool {
    matches!(inst_type, "mintTo" | "mintToChecked")
}

/// True when this transaction creates the mint's entire on-chain supply so far.
///
/// `preTokenBalances` only covers accounts touched in the tx, so we compare mint
/// account supply: if supply before this tx was zero, `supply_now == minted_in_tx`.
pub fn is_first_supply(supply_now: u64, minted_in_tx: u64) -> bool {
    minted_in_tx > 0 && supply_now.saturating_sub(minted_in_tx) == 0
}

/// NFTs are minted with a total supply of 1 base unit (decimals 0).
pub fn is_likely_nft(supply: u64) -> bool {
    supply == 1
}

pub fn sum_mint_to_amounts(
    message: &TransactionMessage,
    meta: &TransactionMeta,
    mint: &str,
) -> u64 {
    let mut total = 0u64;
    for inst in &message.instructions {
        total = total.saturating_add(mint_to_amount(inst, mint));
    }
    if let Some(groups) = &meta.inner_instructions {
        for group in groups {
            for inst in &group.instructions {
                total = total.saturating_add(mint_to_amount(inst, mint));
            }
        }
    }
    total
}

fn mint_to_amount(inst: &Instruction, mint: &str) -> u64 {
    let Some(parsed) = inst.parsed.as_ref() else {
        return 0;
    };
    if !is_mint_to_type(&parsed.inst_type) {
        return 0;
    }
    let Some(info) = parsed.info.as_ref() else {
        return 0;
    };
    if info.get("mint").and_then(|v| v.as_str()) != Some(mint) {
        return 0;
    }
    parse_amount_field(info)
}

fn parse_amount_field(info: &Value) -> u64 {
    if let Some(amount) = info.get("amount").and_then(|v| v.as_str()) {
        return amount.parse().unwrap_or(0);
    }
    info.pointer("/tokenAmount/amount")
        .and_then(|v| v.as_str())
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod supply_tests {
    use super::*;

    #[test]
    fn first_supply_when_prior_supply_was_zero() {
        assert!(is_first_supply(1_000, 1_000));
        assert!(is_first_supply(500, 500));
    }

    #[test]
    fn likely_nft_when_supply_is_one() {
        assert!(is_likely_nft(1));
        assert!(!is_likely_nft(0));
        assert!(!is_likely_nft(2));
        assert!(!is_likely_nft(1_000_000_000));
    }

    #[test]
    fn not_first_supply_when_mint_already_had_supply() {
        // swap tx: minted 880085237 while supply is already ~10.8B
        assert!(!is_first_supply(10_827_726_644, 880_085_237));
        assert!(!is_first_supply(1_500, 500));
    }

    #[test]
    fn sums_mint_to_amounts_for_mint() {
        let mint = "Mint1";
        let info = |amount: &str| {
            Some(serde_json::json!({
                "mint": mint,
                "amount": amount,
            }))
        };
        let meta = TransactionMeta {
            err: None,
            inner_instructions: Some(vec![InnerInstructionGroup {
                instructions: vec![Instruction {
                    program_id: None,
                    accounts: None,
                    parsed: Some(ParsedInstruction {
                        inst_type: "mintTo".into(),
                        info: info("452726679"),
                    }),
                }],
            }]),
        };
        let message = TransactionMessage {
            account_keys: vec![],
            instructions: vec![Instruction {
                program_id: None,
                accounts: None,
                parsed: Some(ParsedInstruction {
                    inst_type: "mintTo".into(),
                    info: info("427358558"),
                }),
            }],
        };
        assert_eq!(sum_mint_to_amounts(&message, &meta, mint), 880_085_237);
    }
}
