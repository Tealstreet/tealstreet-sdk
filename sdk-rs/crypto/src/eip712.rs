//! Dynamic EIP-712 hashing and deterministic secp256k1 signing, without transport.

use alloy_dyn_abi::TypedData;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;

/// Hash an EIP-712 domain separator and message struct hash.
pub fn hash_prehashed(domain_separator: [u8; 32], struct_hash: [u8; 32]) -> [u8; 32] {
    let mut bytes = [0u8; 66];
    bytes[..2].copy_from_slice(&[0x19, 0x01]);
    bytes[2..34].copy_from_slice(&domain_separator);
    bytes[34..].copy_from_slice(&struct_hash);
    crate::ethereum::keccak256(&bytes)
}

fn typed_data(raw: &str) -> Result<TypedData, String> {
    serde_json::from_str(raw).map_err(|error| error.to_string())
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
