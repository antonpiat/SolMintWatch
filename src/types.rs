use serde::Deserialize;
use serde_json::Value;

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
            crate::constants::SPL_TOKEN_PROGRAM => Some(Self::Spl),
            crate::constants::TOKEN_2022_PROGRAM => Some(Self::Token2022),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MintEvent {
    pub mint: String,
    pub creator: String,
    pub signature: String,
    #[allow(dead_code)]
    pub slot: u64,
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
    pub slot: u64,
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
    #[serde(rename = "preTokenBalances")]
    pub pre_token_balances: Option<Vec<TokenBalance>>,
    #[serde(rename = "postTokenBalances")]
    pub post_token_balances: Option<Vec<TokenBalance>>,
}

#[derive(Debug, Deserialize)]
pub struct TokenBalance {
    pub mint: String,
    #[serde(rename = "uiTokenAmount")]
    pub ui_token_amount: Option<UiTokenAmount>,
}

#[derive(Debug, Deserialize)]
pub struct UiTokenAmount {
    #[serde(rename = "uiAmount")]
    pub ui_amount: Option<f64>,
    pub amount: String,
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
    Parsed {
        pubkey: String,
        #[allow(dead_code)]
        signer: Option<bool>,
        #[allow(dead_code)]
        writable: Option<bool>,
    },
    Raw(String),
}

impl AccountKey {
    pub fn pubkey(&self) -> &str {
        match self {
            Self::Parsed { pubkey, .. } => pubkey,
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
    pub data: AccountData,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum AccountData {
    Tuple(Vec<String>),
    #[allow(dead_code)]
    Parsed(Value),
}

impl AccountData {
    pub fn base64_data(&self) -> Option<&str> {
        match self {
            Self::Tuple(parts) => parts.first().map(String::as_str),
            Self::Parsed(_) => None,
        }
    }
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

/// True when this transaction is the first time tokens are minted for `mint`
/// (no pre-tx token accounts held a balance for this mint).
pub fn is_first_supply_mint(meta: &TransactionMeta, mint: &str) -> bool {
    let pre = meta.pre_token_balances.as_deref().unwrap_or(&[]);
    let had_supply = pre
        .iter()
        .filter(|b| b.mint == mint)
        .any(|b| token_amount_positive(&b.ui_token_amount));

    if had_supply {
        return false;
    }

    let post = meta.post_token_balances.as_deref().unwrap_or(&[]);
    post.iter()
        .filter(|b| b.mint == mint)
        .any(|b| token_amount_positive(&b.ui_token_amount))
}

fn token_amount_positive(amount: &Option<UiTokenAmount>) -> bool {
    match amount {
        Some(a) => a
            .ui_amount
            .map(|v| v > 0.0)
            .unwrap_or_else(|| a.amount != "0"),
        None => false,
    }
}

#[cfg(test)]
mod supply_tests {
    use super::*;

    fn balance(mint: &str, amount: &str, ui: f64) -> TokenBalance {
        TokenBalance {
            mint: mint.to_string(),
            ui_token_amount: Some(UiTokenAmount {
                ui_amount: Some(ui),
                amount: amount.to_string(),
            }),
        }
    }

    #[test]
    fn first_supply_when_post_has_tokens_and_pre_is_empty() {
        let meta = TransactionMeta {
            err: None,
            inner_instructions: None,
            pre_token_balances: Some(vec![]),
            post_token_balances: Some(vec![balance("Mint1", "1000", 1000.0)]),
        };
        assert!(is_first_supply_mint(&meta, "Mint1"));
    }

    #[test]
    fn not_first_supply_when_pre_already_had_tokens() {
        let meta = TransactionMeta {
            err: None,
            inner_instructions: None,
            pre_token_balances: Some(vec![balance("Mint1", "500", 500.0)]),
            post_token_balances: Some(vec![balance("Mint1", "1500", 1500.0)]),
        };
        assert!(!is_first_supply_mint(&meta, "Mint1"));
    }
}
