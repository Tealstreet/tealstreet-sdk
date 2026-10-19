//! L1 phantom-agent signing. Struct declaration order is part of the hash.
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
    let mut bytes = rmp_serde::to_vec_named(action).map_err(|e| e.to_string())?;
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
    #[derive(Serialize)]
    struct Field {
        name: &'static str,
        r#type: &'static str,
    }
    #[derive(Serialize)]
    struct Types {
        #[serde(rename = "Agent")]
        agent: [Field; 2],
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Domain {
        name: &'static str,
        version: &'static str,
        chain_id: u32,
        verifying_contract: &'static str,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Agent {
        source: &'static str,
        connection_id: String,
    }
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Typed {
        domain: Domain,
        types: Types,
        primary_type: &'static str,
        message: Agent,
    }
    let hash = action_hash(action, vault, nonce)?;
    let typed = Typed {
        domain: Domain {
            name: "Exchange",
            version: "1",
            chain_id: 1337,
            verifying_contract: "0x0000000000000000000000000000000000000000",
        },
        types: Types {
            agent: [
                Field {
                    name: "source",
                    r#type: "string",
                },
                Field {
                    name: "connectionId",
                    r#type: "bytes32",
                },
            ],
        },
        primary_type: "Agent",
        message: Agent {
            source: if mainnet { "a" } else { "b" },
            connection_id: format!("0x{}", hex(&hash)),
        },
    };
    let signature = eip712::sign_typed_data(
        &serde_json::to_string(&typed).map_err(|e| e.to_string())?,
        secret,
    )?;
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
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
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
