use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use spl_token_2022_interface::{
    extension::{
        metadata_pointer::MetadataPointer,
        BaseStateWithExtensions, StateWithExtensions,
    },
    state::Mint,
};
use spl_token_metadata_interface::{
    solana_address::Address, solana_nullable::MaybeNull, state::TokenMetadata,
};
use spl_type_length_value::state::{TlvState, TlvStateBorrowed};

use crate::config::METAPLEX_METADATA_PROGRAM;
use crate::rpc::HeliusRpc;
use crate::types::TokenProgram;

/// Resolve token name/symbol using the program-appropriate metadata source.
pub async fn resolve(
    rpc: &HeliusRpc,
    mint: &str,
    program: TokenProgram,
) -> Result<(Option<String>, Option<String>)> {
    match program {
        TokenProgram::Spl => fetch_metaplex_metadata(rpc, mint).await,
        TokenProgram::Token2022 => fetch_token2022_metadata(rpc, mint).await,
    }
}

async fn fetch_token2022_metadata(
    rpc: &HeliusRpc,
    mint: &str,
) -> Result<(Option<String>, Option<String>)> {
    let Some(bytes) = rpc.get_account_bytes(mint).await? else {
        return fetch_metaplex_metadata(rpc, mint).await;
    };

    let mut parsed = parse_token2022_account(&bytes);

    if !parsed.is_complete()
        && let Some(target) = parsed.metadata_address.clone()
        && target != mint
        && let Some(target_bytes) = rpc.get_account_bytes(&target).await?
    {
        let target_parsed = parse_token2022_account(&target_bytes);
        parsed.merge(target_parsed);
    }

    if parsed.is_complete() {
        return Ok((parsed.name, parsed.symbol));
    }

    fetch_metaplex_metadata(rpc, mint).await
}

#[derive(Debug, Clone, Default)]
struct ParsedMetadata {
    name: Option<String>,
    symbol: Option<String>,
    metadata_address: Option<String>,
}

impl ParsedMetadata {
    fn is_complete(&self) -> bool {
        self.name.as_ref().is_some_and(|s| !s.is_empty())
            && self.symbol.as_ref().is_some_and(|s| !s.is_empty())
    }

    fn merge(&mut self, other: Self) {
        if self.name.is_none() {
            self.name = other.name;
        }
        if self.symbol.is_none() {
            self.symbol = other.symbol;
        }
        if self.metadata_address.is_none() {
            self.metadata_address = other.metadata_address;
        }
    }
}

/// Parse Token-2022 metadata from raw account bytes using the official SPL interfaces.
///
/// Follows the approach in the Solana docs:
/// `StateWithExtensions::<Mint>::unpack`, then `get_extension::<MetadataPointer>()`
/// and `get_variable_len_extension::<TokenMetadata>()`.
fn parse_token2022_account(bytes: &[u8]) -> ParsedMetadata {
    if let Ok(state) = StateWithExtensions::<Mint>::unpack(bytes) {
        let mut parsed = ParsedMetadata::default();

        if let Ok(metadata) = state.get_variable_len_extension::<TokenMetadata>() {
            parsed.name = non_empty(metadata.name);
            parsed.symbol = non_empty(metadata.symbol);
        }

        if let Ok(pointer) = state.get_extension::<MetadataPointer>() {
            parsed.metadata_address = maybe_address_string(&pointer.metadata_address);
        }

        return parsed;
    }

    if let Ok(tlv) = TlvStateBorrowed::unpack(bytes)
        && let Ok(metadata) = tlv.get_first_variable_len_value::<TokenMetadata>()
    {
        return ParsedMetadata {
            name: non_empty(metadata.name),
            symbol: non_empty(metadata.symbol),
            metadata_address: None,
        };
    }

    ParsedMetadata::default()
}

async fn fetch_metaplex_metadata(
    rpc: &HeliusRpc,
    mint: &str,
) -> Result<(Option<String>, Option<String>)> {
    let metadata_pda = derive_metadata_pda(mint)?;
    let Some(bytes) = rpc.get_account_bytes(&metadata_pda).await? else {
        return Ok((None, None));
    };

    Ok(parse_metaplex_metadata(&bytes))
}

fn maybe_address_string(address: &MaybeNull<Address>) -> Option<String> {
    address.as_ref().map(ToString::to_string)
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_empty_rejects_whitespace() {
        assert_eq!(non_empty("  ".to_string()), None);
        assert_eq!(non_empty("PUMP".to_string()).as_deref(), Some("PUMP"));
    }
}
