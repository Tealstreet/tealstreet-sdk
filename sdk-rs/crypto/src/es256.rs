//! P-256 ECDSA/SHA-256 over caller-owned bytes, returning WebCrypto/JWS r||s.
//! RFC 6979 nonces are deterministic; WebCrypto's random nonce choice produces
//! different valid signatures. No JWT, PEM normalization or host entropy lives here.
use p256::{
    ecdsa::{
        Signature, SigningKey, VerifyingKey,
        signature::{Signer, Verifier},
    },
    pkcs8::DecodePrivateKey,
};

/// Verify a fixed-width r||s P-256/SHA-256 signature against a SEC1 public key.
pub fn verify(payload: &[u8], public_key: &[u8], signature: &[u8]) -> bool {
    let (Ok(key), Ok(signature)) = (
        VerifyingKey::from_sec1_bytes(public_key),
        Signature::from_slice(signature),
    ) else {
        return false;
    };
    key.verify(payload, &signature).is_ok()
}

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

    #[test]
    fn verify_accepts_only_the_exact_payload_key_and_signature() {
        let key = SigningKey::from_slice(&[7; 32]).unwrap();
        let signature: Signature = key.sign(b"payload");
        let signature = signature.to_bytes();
        for compressed in [false, true] {
            let public = VerifyingKey::from(&key).to_encoded_point(compressed);
            assert!(verify(b"payload", public.as_bytes(), &signature));
            assert!(!verify(b"payload!", public.as_bytes(), &signature));
        }
        let other =
            VerifyingKey::from(&SigningKey::from_slice(&[8; 32]).unwrap()).to_encoded_point(false);
        assert!(!verify(b"payload", other.as_bytes(), &signature));
        let public = VerifyingKey::from(&key).to_encoded_point(false);
        assert!(!verify(b"payload", public.as_bytes(), &signature[..63]));
        assert!(!verify(b"payload", &public.as_bytes()[..10], &signature));
    }

    #[test]
    fn signs_deterministically_and_verifies() {
        let secret = SigningKey::from_slice(&[7u8; 32]).unwrap();
        let key = Es256SigningKey(secret.clone());
        let first = key.sign(b"GET/api/v3/brokerage/accounts").unwrap();
        assert_eq!(first, key.sign(b"GET/api/v3/brokerage/accounts").unwrap());
        let verifier = VerifyingKey::from(&secret);
        let signature = Signature::from_slice(&first).unwrap();
        assert!(
            verifier
                .verify(b"GET/api/v3/brokerage/accounts", &signature)
                .is_ok()
        );
    }

    #[test]
    fn malformed_pkcs8_is_rejected_without_key_material_in_errors() {
        for input in [&[][..], &[48, 0], &[0, 1, 2, 3], &[48, 129, 255]] {
            assert!(
                matches!(Es256SigningKey::from_pkcs8_der(input), Err(error) if error == "Invalid P-256 PKCS8 private key")
            );
        }
    }
}
