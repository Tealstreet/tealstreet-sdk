//! Shared Ethereum primitives, without venue payload construction or entropy.
use alloy_primitives::B256;
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;

/// Keccak-256 over exact bytes.
pub fn keccak256(payload: &[u8]) -> [u8; 32] {
    alloy_primitives::keccak256(payload).into()
}

/// EIP-191 Ethereum signed-message hash over exact bytes, not a hex spelling.
pub fn ethereum_message_hash(payload: &[u8]) -> [u8; 32] {
    alloy_primitives::eip191_hash_message(payload).into()
}

/// RFC6979 recoverable secp256k1 signature over an already prepared 32-byte digest.
pub fn sign_digest(digest: &[u8], private_key: &str) -> Result<String, String> {
    let digest: &[u8; 32] = digest
        .try_into()
        .map_err(|_| "Ethereum digest must be 32 bytes")?;
    let signer: PrivateKeySigner = private_key
        .parse()
        .map_err(|error: alloy_signer_local::LocalSignerError| error.to_string())?;
    signer
        .sign_hash_sync(&B256::from(*digest))
        .map(|signature| signature.to_string())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn corpus() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/ethereum/primitives.json")).unwrap()
    }
    fn bytes(value: &str) -> Vec<u8> {
        value
            .strip_prefix("0x")
            .unwrap_or(value)
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn hash_and_recoverable_signature_raw_vectors() {
        let corpus = corpus();
        let cases = corpus["cases"].as_array().unwrap();
        for name in [
            "empty",
            "ascii",
            "unicode-nul",
            "word-boundary",
            "word-plus-one",
            "long",
        ] {
            assert!(
                cases.iter().any(|case| case["name"] == name),
                "missing Ethereum primitive property {name}"
            );
        }
        for case in cases {
            let abi = bytes(case["abi"].as_str().unwrap());
            let hash = keccak256(&abi);
            assert_eq!(
                hash.to_vec(),
                bytes(case["keccak"].as_str().unwrap()),
                "Keccak {}",
                case["name"]
            );
            let digest = ethereum_message_hash(&hash);
            assert_eq!(
                digest.to_vec(),
                bytes(case["messageHash"].as_str().unwrap()),
                "EIP-191 {}",
                case["name"]
            );
            let signature = sign_digest(&digest, case["privateKey"].as_str().unwrap()).unwrap();
            assert_eq!(
                signature,
                case["signature"].as_str().unwrap(),
                "recoverable signature {}",
                case["name"]
            );
            let signature: alloy_primitives::Signature = signature.parse().unwrap();
            let recovered = signature
                .recover_address_from_prehash(&B256::from(digest))
                .unwrap();
            assert_eq!(
                recovered.to_checksum(None),
                case["recovered"].as_str().unwrap(),
                "signature recovery {}",
                case["name"]
            );
        }
        assert!(
            sign_digest(&[], "invalid").is_err(),
            "digest length refusal"
        );
        assert!(sign_digest(&[0; 32], "invalid").is_err(), "key refusal");
    }
    #[test]
    fn exact_byte_hashes() {
        for case in corpus()["bytesCases"].as_array().unwrap() {
            let payload = bytes(case["bytes"].as_str().unwrap());
            assert_eq!(
                keccak256(&payload).to_vec(),
                bytes(case["keccak"].as_str().unwrap()),
                "raw Keccak {}",
                case["name"]
            );
            assert_eq!(
                ethereum_message_hash(&payload).to_vec(),
                bytes(case["messageHash"].as_str().unwrap()),
                "raw EIP-191 {}",
                case["name"]
            );
        }
    }
}
