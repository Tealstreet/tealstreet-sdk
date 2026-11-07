//! L1 phantom-agent signing. Struct declaration order is part of the hash.
pub mod dispatch;
pub mod statement;

use safe_cex_crypto::{eip712, ethereum};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct Signature {
    pub r: String,
    pub s: String,
    pub v: u8,
}

pub fn action_hash(
    action: &impl Serialize,
    vault: Option<&str>,
    nonce: u64,
) -> Result<[u8; 32], String> {
    let encoded = rmp_serde::to_vec_named(action).map_err(|e| e.to_string())?;
    action_hash_encoded(&encoded, vault, nonce)
}

fn action_hash_encoded(
    encoded: &[u8],
    vault: Option<&str>,
    nonce: u64,
) -> Result<[u8; 32], String> {
    let mut bytes = encoded.to_vec();
    bytes.extend_from_slice(&nonce.to_be_bytes());
    bytes.push(u8::from(vault.is_some()));
    if let Some(vault) = vault {
        let address = ethereum::get_address(vault)?;
        for pair in address.as_bytes()[2..].as_chunks::<2>().0 {
            bytes.push(
                u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16)
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    Ok(ethereum::keccak256(&bytes))
}
pub fn sign(
    action: &impl Serialize,
    vault: Option<&str>,
    nonce: u64,
    mainnet: bool,
    secret: &str,
) -> Result<Signature, String> {
    let connection_id = action_hash(action, vault, nonce)?;
    let signature = ethereum::sign_digest(&agent_digest(connection_id, mainnet), secret)?;
    let signature = signature.strip_prefix("0x").unwrap_or(&signature);
    if signature.len() != 130 {
        return Err("Invalid Ethereum signature length".into());
    }
    Ok(Signature {
        r: format!("0x{}", &signature[..64]),
        s: format!("0x{}", &signature[64..128]),
        v: u8::from_str_radix(&signature[128..], 16).map_err(|e| e.to_string())?,
    })
}

/// Reserve the agent's nonce in the domain Hyperliquid shares with TxFlow.
pub fn reserve_agent_nonce<P>(
    cx: &safe_cex_engine::Cx<'_, P>,
    secret: &str,
) -> Result<u64, String> {
    let address = ethereum::private_key_address(secret)?;
    cx.reserve_nonce(
        &safe_cex_engine::nonce::HYPERLIQUID_AGENT,
        &safe_cex_engine::nonce::NonceKey::ethereum_address(&address),
    )
    .map_err(|error| error.message().into())
}

// The phantom agent has a fixed EIP-712 schema. Encode its ABI words here so
// the venue does not pull the general JSON typed-data parser into the host.
pub(crate) fn agent_digest(connection_id: [u8; 32], mainnet: bool) -> [u8; 32] {
    let mut domain = [0u8; 160];
    domain[..32].copy_from_slice(&ethereum::keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    ));
    domain[32..64].copy_from_slice(&ethereum::keccak256(b"Exchange"));
    domain[64..96].copy_from_slice(&ethereum::keccak256(b"1"));
    domain[120..128].copy_from_slice(&1337u64.to_be_bytes());
    // The fifth word is the zero verifying contract.
    let mut agent = [0u8; 96];
    agent[..32].copy_from_slice(&ethereum::keccak256(
        b"Agent(string source,bytes32 connectionId)",
    ));
    agent[32..64].copy_from_slice(&ethereum::keccak256(if mainnet { b"a" } else { b"b" }));
    agent[64..].copy_from_slice(&connection_id);
    eip712::hash_prehashed(ethereum::keccak256(&domain), ethereum::keccak256(&agent))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_key_hex_case_does_not_create_a_new_nonce_domain() {
        let key = "00000000000000000000000000000000000000000000000000000000000000ab";
        let upper = format!("0x{}", key.to_uppercase());
        let checksummed = ethereum::private_key_address(key).unwrap();
        assert_eq!(checksummed, ethereum::private_key_address(&upper).unwrap());
        assert_eq!(
            safe_cex_engine::nonce::NonceKey::ethereum_address(&checksummed),
            safe_cex_engine::nonce::NonceKey::ethereum_address(&checksummed.to_ascii_lowercase())
        );
    }
    #[test]
    fn fixed_agent_words_match_dynamic_eip712_for_both_networks() {
        for connection_id in [[0u8; 32], [0xff; 32], core::array::from_fn(|i| i as u8)] {
            for mainnet in [false, true] {
                let typed = serde_json::json!({
                    "domain": { "name": "Exchange", "version": "1", "chainId": 1337,
                        "verifyingContract": "0x0000000000000000000000000000000000000000" },
                    "types": { "Agent": [ { "name": "source", "type": "string" },
                        { "name": "connectionId", "type": "bytes32" } ] },
                    "primaryType": "Agent",
                    "message": { "source": if mainnet { "a" } else { "b" },
                        "connectionId": format!("0x{}", safe_cex_crypto::hex(&connection_id)) },
                });
                assert_eq!(
                    format!(
                        "0x{}",
                        safe_cex_crypto::hex(&agent_digest(connection_id, mainnet))
                    ),
                    format!(
                        "{:#x}",
                        serde_json::from_value::<alloy_dyn_abi::TypedData>(typed)
                            .unwrap()
                            .eip712_signing_hash()
                            .unwrap()
                    )
                );
            }
        }
    }

    // Official SDK signing vectors, including vault domain separation.
    // https://github.com/hyperliquid-dex/hyperliquid-python-sdk/blob/master/tests/signing_test.py
    #[test]
    fn official_mainnet_testnet_and_vault_signatures() {
        #[derive(Serialize)]
        struct Dummy {
            r#type: &'static str,
            num: u64,
        }
        let a = Dummy {
            r#type: "dummy",
            num: 100_000_000_000,
        };
        let key = "0x0123456789012345678901234567890123456789012345678901234567890123";
        for (vault, mainnet, r, s, v) in [
            (
                None,
                true,
                "0x53749d5b30552aeb2fca34b530185976545bb22d0b3ce6f62e31be961a59298",
                "0x755c40ba9bf05223521753995abb2f73ab3229be8ec921f350cb447e384d8ed8",
                27,
            ),
            (
                None,
                false,
                "0x542af61ef1f429707e3c76c5293c80d01f74ef853e34b76efffcb57e574f9510",
                "0x17b8b32f086e8cdede991f1e2c529f5dd5297cbe8128500e00cbaf766204a613",
                28,
            ),
            (
                Some("0x1719884eb866cb12b2287399b15f7db5e7d775ea"),
                true,
                "0x3c548db75e479f8012acf3000ca3a6b05606bc2ec0c29c50c515066a326239",
                "0x4d402be7396ce74fbba3795769cda45aec00dc3125a984f2a9f23177b190da2c",
                28,
            ),
        ] {
            let sig = sign(&a, vault, 0, mainnet, key).unwrap();
            let integer_hex = |s: &str| {
                s.strip_prefix("0x")
                    .unwrap_or(s)
                    .trim_start_matches('0')
                    .to_owned()
            };
            assert_eq!(
                (integer_hex(&sig.r), integer_hex(&sig.s), sig.v),
                (integer_hex(r), integer_hex(s), v)
            );
        }
    }
}
