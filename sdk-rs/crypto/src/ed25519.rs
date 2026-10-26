//! Ed25519 over exact caller-supplied bytes. Deterministic signing has no nonce
//! parameter: any application nonce must be included in those bytes.
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};

/// Derive a public key from a 32-byte Ed25519 seed.
pub fn public_key(seed: &[u8]) -> Result<[u8; 32], String> {
    let seed: &[u8; 32] = seed
        .try_into()
        .map_err(|_| "Ed25519 seed must be 32 bytes")?;
    Ok(SigningKey::from_bytes(seed).verifying_key().to_bytes())
}

/// Sign the exact bytes; no JSON serialization or prehash occurs here.
pub fn sign(payload: &[u8], seed: &[u8]) -> Result<[u8; 64], String> {
    let seed: &[u8; 32] = seed
        .try_into()
        .map_err(|_| "Ed25519 seed must be 32 bytes")?;
    Ok(SigningKey::from_bytes(seed).sign(payload).to_bytes())
}

/// Strict verification rejects malformed signatures and weak public keys.
pub fn verify(payload: &[u8], public_key: &[u8], signature: &[u8]) -> Result<bool, String> {
    let key: &[u8; 32] = public_key
        .try_into()
        .map_err(|_| "Ed25519 public key must be 32 bytes")?;
    let key = VerifyingKey::from_bytes(key).map_err(|error| error.to_string())?;
    let signature = Signature::from_slice(signature).map_err(|error| error.to_string())?;
    Ok(key.verify_strict(payload, &signature).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Vector>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Vector {
        seed: String,
        public_key: String,
        payload: String,
        signature: String,
        changed_payload: String,
        nonce_payload: String,
    }
    fn bytes(hex: &str) -> Vec<u8> {
        hex.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn node_raw_string_vectors_and_mutations() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../tests/fixtures/ed25519/node.json")).unwrap();
        assert!(
            corpus.cases.iter().any(|v| v.payload.is_empty()),
            "empty-message coverage"
        );
        assert!(
            corpus
                .cases
                .iter()
                .any(|v| !v.payload.is_ascii() && v.payload.contains("\\u0000")),
            "Unicode and escaped NUL coverage"
        );
        assert!(
            corpus.cases.iter().any(|v| v.payload.contains("\"nonce\"")),
            "application nonce coverage"
        );
        for v in corpus.cases {
            let seed = bytes(&v.seed);
            let key = bytes(&v.public_key);
            let signature = bytes(&v.signature);
            assert_eq!(
                public_key(&seed).unwrap().as_slice(),
                key,
                "Node Ed25519 public key"
            );
            assert_eq!(
                sign(v.payload.as_bytes(), &seed).unwrap().as_slice(),
                signature,
                "Node Ed25519 signature"
            );
            assert!(verify(v.payload.as_bytes(), &key, &signature).unwrap());
            let mut changed = seed.clone();
            changed[0] ^= 1;
            assert!(
                !verify(
                    v.payload.as_bytes(),
                    &public_key(&changed).unwrap(),
                    &signature
                )
                .unwrap(),
                "mutated key must reject"
            );
            assert!(
                !verify(v.changed_payload.as_bytes(), &key, &signature).unwrap(),
                "mutated payload must reject"
            );
            assert!(
                !verify(v.nonce_payload.as_bytes(), &key, &signature).unwrap(),
                "mutated application nonce must reject"
            );
        }
    }
    #[test]
    fn malformed_raw_seed_returns_error() {
        let seed: Vec<u8> = serde_json::from_str("[1,2,3]").unwrap();
        assert_eq!(
            sign(b"payload", &seed).unwrap_err(),
            "Ed25519 seed must be 32 bytes"
        );
    }
}
