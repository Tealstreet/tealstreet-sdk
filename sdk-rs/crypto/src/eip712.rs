//! EIP-712 hashing and deterministic secp256k1 signing, without transport.

use alloy_dyn_abi::TypedData;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;

/// Build an EIP-712 signing digest from an already hashed domain and message.
/// Callers with a fixed schema can encode its words directly without loading
/// the dynamic typed-data parser. Both arguments are keccak-256 hashes, not
/// ABI data; this is the message form, not domain-only signing.
pub fn hash_prehashed(domain_separator: [u8; 32], struct_hash: [u8; 32]) -> [u8; 32] {
    let mut bytes = [0u8; 66];
    bytes[..2].copy_from_slice(&[0x19, 0x01]);
    bytes[2..34].copy_from_slice(&domain_separator);
    bytes[34..].copy_from_slice(&struct_hash);
    crate::ethereum::keccak256(&bytes)
}

fn validate_addresses(
    types: &serde_json::Value,
    kind: &str,
    value: &mut serde_json::Value,
    depth: usize,
) -> Result<(), String> {
    if depth > 128 {
        return Err("EIP-712 address nesting limit exceeded".into());
    }
    if kind == "address" {
        let address = value.as_str().ok_or("invalid address")?;
        *value = serde_json::Value::String(crate::ethereum::get_address(address)?);
    } else if kind.ends_with(']') {
        if let Some((element, _)) = kind.rsplit_once('[')
            && let Some(values) = value.as_array_mut()
        {
            for value in values {
                validate_addresses(types, element, value, depth + 1)?;
            }
        }
    } else if let Some(fields) = types.get(kind).and_then(serde_json::Value::as_array) {
        for field in fields {
            if let (Some(name), Some(kind)) = (field["name"].as_str(), field["type"].as_str())
                && let Some(value) = value.get_mut(name)
            {
                validate_addresses(types, kind, value, depth + 1)?;
            }
        }
    }
    Ok(())
}

fn typed_data(raw: &str) -> Result<TypedData, String> {
    let mut value: serde_json::Value =
        serde_json::from_str(raw).map_err(|error| error.to_string())?;
    if !value.is_object() || !value["domain"].is_object() || !value["types"].is_object() {
        return Err("EIP-712 payload, domain and types must be objects".into());
    }
    for (name, field) in value["domain"]
        .as_object()
        .into_iter()
        .flat_map(|domain| domain.iter())
    {
        if !field.is_null()
            && !matches!(
                name.as_str(),
                "name" | "version" | "chainId" | "verifyingContract" | "salt"
            )
        {
            return Err(format!("invalid typed-data domain key: {name}"));
        }
    }
    let types = value["types"].clone();
    for fields in types
        .as_object()
        .into_iter()
        .flat_map(|types| types.values())
    {
        let fields = fields
            .as_array()
            .ok_or("EIP-712 field declaration must be an array")?;
        let mut names = std::collections::HashSet::new();
        for field in fields {
            let name = field
                .get("name")
                .and_then(serde_json::Value::as_str)
                .ok_or("EIP-712 field declaration must be a named object")?;
            if !names.insert(name) {
                return Err(format!("duplicate EIP-712 field name: {name}"));
            }
        }
    }
    if let Some(address) = value["domain"]
        .get_mut("verifyingContract")
        .filter(|address| !address.is_null())
    {
        *address = serde_json::Value::String(crate::ethereum::get_address(
            address.as_str().ok_or("invalid address")?,
        )?);
    }
    let primary = value["primaryType"].as_str().unwrap_or("").to_owned();
    validate_addresses(&types, &primary, &mut value["message"], 0)?;
    let raw = serde_json::to_string(&value).map_err(|error| error.to_string())?;
    serde_json::from_str(&raw).map_err(|error| error.to_string())
}

/// Hash raw EIP-712 JSON using the domain and primary type it declares.
pub fn hash_typed_data(raw: &str) -> Result<String, String> {
    let data = typed_data(raw)?;
    data.eip712_signing_hash()
        .map(|hash| format!("{hash:#x}"))
        .map_err(|error| error.to_string())
}

/// Sign raw EIP-712 JSON; returns Ethereum's 65-byte hex signature (v = 27/28).
/// The message's nonce is part of the typed data; RFC6979 chooses ECDSA's nonce.
pub fn sign_typed_data(raw: &str, private_key: &str) -> Result<String, String> {
    let data = typed_data(raw)?;
    let signer: PrivateKeySigner = private_key
        .parse()
        .map_err(|error: alloy_signer_local::LocalSignerError| error.to_string())?;
    signer
        .sign_dynamic_typed_data_sync(&data)
        .map(|signature| signature.to_string())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{hash_prehashed, hash_typed_data, sign_typed_data, typed_data};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        name: String,
        private_key: String,
        typed_data: String,
        hash: String,
        signature: String,
    }

    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }

    fn corpus() -> Vec<Case> {
        serde_json::from_str::<Corpus>(include_str!("../tests/fixtures/ethereum/vectors.json"))
            .expect("raw ethers oracle corpus")
            .cases
    }

    #[test]
    fn ethers_hash_and_signature_vectors() {
        let cases = corpus();
        for required in ["base", "key", "payload", "nonce", "empty", "uint64-max"] {
            assert!(
                cases.iter().any(|case| case.name == required),
                "missing {required} oracle"
            );
        }
        for case in cases {
            assert_eq!(
                hash_typed_data(&case.typed_data).unwrap(),
                case.hash,
                "EIP-712 payload hash: {}",
                case.name
            );
            assert_eq!(
                sign_typed_data(&case.typed_data, &case.private_key).unwrap(),
                case.signature,
                "secp256k1 signature: {}",
                case.name
            );
        }
    }

    #[test]
    fn prehashed_digest_matches_dynamic_hash_and_ethers_signature_corpus() {
        for case in corpus() {
            let data = typed_data(&case.typed_data).unwrap();
            let digest = hash_prehashed(
                data.domain.separator().into(),
                data.hash_struct().unwrap().into(),
            );
            assert_eq!(
                format!("0x{}", crate::hex(&digest)),
                hash_typed_data(&case.typed_data).unwrap(),
                "dynamic EIP-712 equivalence: {}",
                case.name
            );
            assert_eq!(
                format!("0x{}", crate::hex(&digest)),
                case.hash,
                "ethers digest: {}",
                case.name
            );
            assert_eq!(
                crate::ethereum::sign_digest(&digest, &case.private_key).unwrap(),
                case.signature,
                "ethers signature through prehashed digest: {}",
                case.name
            );
        }
    }

    #[test]
    fn key_payload_and_nonce_each_change_signature() {
        let cases = corpus();
        let base = cases.iter().find(|case| case.name == "base").unwrap();
        for mutation in ["key", "payload", "nonce"] {
            let changed = cases.iter().find(|case| case.name == mutation).unwrap();
            assert_ne!(
                sign_typed_data(&changed.typed_data, &changed.private_key).unwrap(),
                base.signature,
                "{mutation} mutation must change secp256k1 signature"
            );
        }
    }

    #[test]
    fn malformed_raw_input_and_invalid_key_are_errors() {
        assert!(hash_typed_data("{").is_err());
        let cases = corpus();
        assert!(sign_typed_data(&cases[0].typed_data, "0x00").is_err());
        assert!(sign_typed_data(&cases[0].typed_data, &format!("0x{}", "00".repeat(32))).is_err());
    }
}

#[cfg(test)]
mod validation_tests {
    use super::*;
    #[test]
    fn ethers_declared_domain_and_nested_address_validation() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum/typed-validation.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        for name in [
            "unknown-domain",
            "duplicate-field",
            "bad-domain-checksum",
            "wrong-checksum",
            "nested-wrong-checksum",
            "nested-bytes-string",
            "salt-only",
            "empty",
        ] {
            assert!(
                cases.iter().any(|case| case["name"] == name),
                "missing EIP-712 validation property {name}"
            );
        }
        for case in cases {
            let raw = case["raw"].as_str().unwrap();
            let hash = hash_typed_data(raw);
            if let Some(expected) = case.get("hash") {
                assert_eq!(
                    hash.as_deref(),
                    Ok(expected.as_str().unwrap()),
                    "ethers hash {}",
                    case["name"]
                );
                assert_eq!(
                    sign_typed_data(raw, case["key"].as_str().unwrap()).unwrap(),
                    case["signature"].as_str().unwrap(),
                    "ethers signature {}",
                    case["name"]
                );
            } else {
                assert!(
                    hash.is_err(),
                    "ethers refuses but Rust hashes {}",
                    case["name"]
                );
                assert!(
                    sign_typed_data(raw, case["key"].as_str().unwrap()).is_err(),
                    "ethers refuses but Rust signs {}",
                    case["name"]
                );
            }
        }
    }
}
