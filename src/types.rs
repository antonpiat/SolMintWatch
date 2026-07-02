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

pub fn is_initialize_mint_log(logs: &[String]) -> bool {
    logs.iter().any(|line| {
        line.contains("Instruction: InitializeMint")
            || line.contains("Instruction: InitializeMint2")
    })
}

pub fn is_initialize_mint_type(inst_type: &str) -> bool {
    matches!(inst_type, "initializeMint" | "initializeMint2")
}
