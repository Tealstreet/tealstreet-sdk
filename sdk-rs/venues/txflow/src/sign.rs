//! TxFlow's agent schema is distinct from Hyperliquid; only the preimage is shared.
use safe_cex_crypto::{eip712, ethereum};
pub use safe_cex_venue_hyperliquid::sign::{Signature, action_hash};
use serde::Serialize;

pub fn domain_separator() -> [u8; 32] {
    let mut words = [0u8; 160];
    words[..32].copy_from_slice(&ethereum::keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    ));
    words[32..64].copy_from_slice(&ethereum::keccak256(b"TxFlow-Testnet"));
    words[64..96].copy_from_slice(&ethereum::keccak256(b"1"));
    words[120..128].copy_from_slice(&2511u64.to_be_bytes());
    ethereum::keccak256(&words)
}

pub fn agent_digest(connection_id: [u8; 32]) -> [u8; 32] {
    let mut words = [0u8; 160];
    words[..32].copy_from_slice(&ethereum::keccak256(
        b"Agent(string txflowNetwork,uint32 chainId,uint32 apiVersion,bytes32 connectionId)",
    ));
    words[32..64].copy_from_slice(&ethereum::keccak256(b"TxFlow-Testnet"));
    words[88..96].copy_from_slice(&2511u64.to_be_bytes());
    words[127] = 1;
    words[128..].copy_from_slice(&connection_id);
    eip712::hash_prehashed(domain_separator(), ethereum::keccak256(&words))
}

/// `action` contains named fields in documented order, without its dispatch `type`.
pub fn sign(
    action: &impl Serialize,
    vault: Option<&str>,
    nonce: u64,
    secret: &str,
) -> Result<Signature, String> {
    let hash = action_hash(action, vault, nonce)?;
    let signature = ethereum::sign_digest(&agent_digest(hash), secret)?;
    let signature = signature.strip_prefix("0x").unwrap_or(&signature);
    if signature.len() != 130 {
        return Err("Invalid TxFlow signature length".into());
    }
    Ok(Signature {
        r: format!("0x{}", &signature[..64]),
        s: format!("0x{}", &signature[64..128]),
        v: u8::from_str_radix(&signature[128..], 16).map_err(|e| e.to_string())?,
    })
}
