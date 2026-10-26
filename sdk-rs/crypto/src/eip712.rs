//! EIP-712 digest construction from venue-owned domain and struct hashes.
//! Dynamic ABI/JSON encoding belongs only to the independent test oracle.

/// Hash an EIP-712 domain separator and message struct hash.
pub fn hash_prehashed(domain_separator: [u8; 32], struct_hash: [u8; 32]) -> [u8; 32] {
    let mut bytes = [0u8; 66];
    bytes[..2].copy_from_slice(&[0x19, 0x01]);
    bytes[2..34].copy_from_slice(&domain_separator);
    bytes[34..].copy_from_slice(&struct_hash);
    crate::ethereum::keccak256(&bytes)
}

#[cfg(test)]
mod tests {
    use super::hash_prehashed;
    use alloy_dyn_abi::TypedData;
    use alloy_signer::SignerSync;
    use alloy_signer_local::PrivateKeySigner;
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
            let data: TypedData = serde_json::from_str(&case.typed_data).unwrap();
            assert_eq!(
                format!("{:#x}", data.eip712_signing_hash().unwrap()),
                case.hash,
                "EIP-712 payload hash: {}",
                case.name
            );
            let signer: PrivateKeySigner = case.private_key.parse().unwrap();
            assert_eq!(
                signer
                    .sign_dynamic_typed_data_sync(&data)
                    .unwrap()
                    .to_string(),
                case.signature,
                "secp256k1 signature: {}",
                case.name
            );
        }
    }

    #[test]
    fn prehashed_digest_matches_dynamic_hash_and_ethers_signature_corpus() {
        for case in corpus() {
            let data: TypedData = serde_json::from_str(&case.typed_data).unwrap();
            let digest = hash_prehashed(
                data.domain.separator().into(),
                data.hash_struct().unwrap().into(),
            );
            assert_eq!(
                format!("0x{}", crate::hex(&digest)),
                format!("{:#x}", data.eip712_signing_hash().unwrap()),
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
            let data: TypedData = serde_json::from_str(&changed.typed_data).unwrap();
            let digest = data.eip712_signing_hash().unwrap();
            assert_ne!(
                crate::ethereum::sign_digest(digest.as_slice(), &changed.private_key).unwrap(),
                base.signature,
                "{mutation} mutation must change secp256k1 signature"
            );
        }
    }

    #[test]
    fn malformed_oracle_and_invalid_signing_key_are_errors() {
        assert!(serde_json::from_str::<TypedData>("{").is_err());
        let case = corpus().remove(0);
        let data: TypedData = serde_json::from_str(&case.typed_data).unwrap();
        let digest = data.eip712_signing_hash().unwrap();
        assert!(crate::ethereum::sign_digest(digest.as_slice(), "0x00").is_err());
        assert!(
            crate::ethereum::sign_digest(digest.as_slice(), &format!("0x{}", "00".repeat(32)))
                .is_err()
        );
    }
}
