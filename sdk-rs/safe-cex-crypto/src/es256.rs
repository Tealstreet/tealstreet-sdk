//! P-256 ECDSA/SHA-256 over caller-owned bytes, returning WebCrypto/JWS r||s.
//! RFC 6979 nonces are deterministic; WebCrypto's random nonce choice produces
//! different valid signatures. No JWT, PEM normalization or host entropy lives here.
use p256::{
    ecdsa::{Signature, SigningKey, signature::Signer},
    pkcs8::DecodePrivateKey,
};

/// Imported, validated P-256 key. Keep this behind an Rc when caching CryptoKeys;
/// it intentionally exposes no serialization, debug output or private-key export.
pub struct Es256SigningKey(SigningKey);

impl Es256SigningKey {
    /// Import the PKCS8 DER bytes supplied to WebCrypto's ECDSA/P-256 importer.
    /// SEC1/PEM conversion belongs to the venue's existing source-mapped helper.
    pub fn from_pkcs8_der(der: &[u8]) -> Result<Self, String> {
        SigningKey::from_pkcs8_der(der)
            .map(Self)
            .map_err(|_| "Invalid P-256 PKCS8 private key".to_owned())
    }

    /// Hash the exact message once with SHA-256 and sign with RFC 6979.
    /// The 64-byte result is fixed-width big-endian r||s, never ASN.1 DER.
    pub fn sign(&self, payload: &[u8]) -> Result<[u8; 64], String> {
        let signature: Signature = self
            .0
            .try_sign(payload)
            .map_err(|_| "P-256 SHA-256 signing failed".to_owned())?;
        Ok(signature.to_bytes().into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{VerifyingKey, signature::Verifier};
    use serde::{Deserialize, Serialize};
    use std::{
        io::Write,
        process::{Command, Stdio},
    };

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Corpus {
        public_key: Vec<u8>,
        cases: Vec<Case>,
        invalid_imports: Vec<Vec<u8>>,
    }
    #[derive(Deserialize)]
    struct Case {
        label: String,
        pkcs8: Vec<u8>,
        payload: Vec<u8>,
        signature: Vec<u8>,
    }
    #[derive(Serialize)]
    struct NativeSignature {
        payload: Vec<u8>,
        signature: Vec<u8>,
    }
    fn oracle(input: &serde_json::Value) -> Vec<u8> {
        let mut child = Command::new("node")
            .arg("tests/es256-oracle.cjs")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Node required for the live Coinbase/WebCrypto signing oracle");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }
    fn hex_bytes(text: &str) -> Vec<u8> {
        text.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn rfc6979_known_answer_and_live_coinbase_webcrypto_signatures() {
        // Enter through raw JSON, preserving the exact byte arrays emitted by the
        // actual TypeScript signer before any native model conversion.
        let corpus: Corpus = serde_json::from_slice(&oracle(&serde_json::json!({}))).unwrap();
        assert_eq!(
            corpus
                .cases
                .iter()
                .map(|c| c.label.as_str())
                .collect::<Vec<_>>(),
            [
                "raw-0",
                "raw-1",
                "raw-2",
                "raw-3",
                "pkcs8-ws",
                "pkcs8-GET",
                "pkcs8-POST",
                "sec1-ws",
                "sec1-GET",
                "sec1-POST"
            ]
        );
        let verifier = VerifyingKey::from_sec1_bytes(&corpus.public_key).unwrap();
        assert_eq!(corpus.invalid_imports.len(), 3);
        for der in &corpus.invalid_imports {
            assert!(
                Es256SigningKey::from_pkcs8_der(der).is_err(),
                "WebCrypto-rejected import must fail natively"
            );
        }
        let mut native = Vec::new();
        for case in &corpus.cases {
            let key = Es256SigningKey::from_pkcs8_der(&case.pkcs8).unwrap();
            assert_eq!(
                key.0.verifying_key().to_encoded_point(false).as_bytes(),
                corpus.public_key,
                "{} key identity",
                case.label
            );
            assert_eq!(case.signature.len(), 64, "{} TS r||s width", case.label);
            let ts_signature = Signature::from_slice(&case.signature).unwrap();
            assert!(
                verifier.verify(&case.payload, &ts_signature).is_ok(),
                "{} TS signature",
                case.label
            );
            let mut changed = case.payload.clone();
            changed.push(32);
            assert!(
                verifier.verify(&changed, &ts_signature).is_err(),
                "{} payload control",
                case.label
            );
            let signature = key.sign(&case.payload).unwrap();
            assert_eq!(
                key.sign(&case.payload).unwrap(),
                signature,
                "{} RFC6979 determinism",
                case.label
            );
            assert!(
                verifier
                    .verify(&case.payload, &Signature::from_slice(&signature).unwrap())
                    .is_ok(),
                "{} native signature",
                case.label
            );
            if case.label == "raw-0" {
                assert_eq!(case.payload, b"sample");
                // RFC 6979 A.2.5, NIST P-256 + SHA-256, including its high S.
                assert_eq!(
                    signature.as_slice(),
                    hex_bytes(concat!(
                        "EFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716",
                        "F7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8"
                    ))
                );
            }
            if case.label == "raw-3" {
                assert_eq!(case.payload, br#"{"zeta":1e400,"alpha":-0,"nonce":"7"}"#);
            }
            native.push(NativeSignature {
                payload: case.payload.clone(),
                signature: signature.to_vec(),
            });
        }
        let results: Vec<serde_json::Value> =
            serde_json::from_slice(&oracle(&serde_json::json!({"verify": native}))).unwrap();
        assert_eq!(results.len(), 10, "all native signatures reach WebCrypto");
        for result in results {
            assert_eq!(result, serde_json::json!({"valid": true, "changed": false}));
        }
    }

    #[test]
    fn malformed_pkcs8_is_rejected_without_key_material_in_errors() {
        let inputs: Vec<Vec<u8>> =
            serde_json::from_str("[[],[48,0],[0,1,2,3],[48,129,255]]").unwrap();
        for input in inputs {
            assert!(
                matches!(Es256SigningKey::from_pkcs8_der(&input), Err(error) if error == "Invalid P-256 PKCS8 private key")
            );
        }
    }
}
