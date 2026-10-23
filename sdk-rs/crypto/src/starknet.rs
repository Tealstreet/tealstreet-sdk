//! Stark-curve public keys and deterministic signing.
use starknet_crypto::Felt;

pub fn public_key(private_key: &Felt) -> Felt {
    starknet_crypto::get_public_key(private_key)
}

/// Verify r, s over a field-element message hash against a Stark public key.
pub fn verify(message_hash: &Felt, public_key: &Felt, r: &Felt, s: &Felt) -> bool {
    starknet_crypto::verify(public_key, message_hash, r, s).unwrap_or(false)
}

/// Deterministic RFC6979 signing, matching the Starknet JS SDK.
pub fn sign(message_hash: &Felt, private_key: &Felt) -> Result<(Felt, Felt), String> {
    if *private_key == Felt::ZERO {
        return Err("private key must be non-zero".into());
    }
    let mut seed = None;
    loop {
        let nonce = starknet_crypto::rfc6979_generate_k(message_hash, private_key, seed.as_ref());
        match starknet_crypto::sign(private_key, message_hash, &nonce) {
            Ok(signature) => return Ok((signature.r, signature.s)),
            Err(starknet_crypto::SignError::InvalidK) => {
                seed = Some(seed.unwrap_or(Felt::ZERO) + Felt::ONE)
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[test]
    fn verify_accepts_only_the_signing_key_and_hash() {
        let key = Felt::from(5u8);
        let hash = Felt::from(0x1234u32);
        let (r, s) = sign(&hash, &key).unwrap();
        assert!(verify(&hash, &public_key(&key), &r, &s));
        assert!(!verify(&(hash + Felt::ONE), &public_key(&key), &r, &s));
        assert!(!verify(&hash, &public_key(&Felt::from(6u8)), &r, &s));
        assert!(!verify(&hash, &public_key(&key), &r, &(s + Felt::ONE)));
    }

    #[derive(Deserialize)]
    struct Fixture<T> {
        cases: Vec<T>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct SignCase {
        key: String,
        public_key: String,
        message_hash: String,
        r: String,
        s: String,
    }

    fn felt(text: &str) -> Felt {
        Felt::from_hex(text).unwrap()
    }

    #[test]
    fn signatures_match_starknet_js() {
        let fixture: Fixture<SignCase> =
            serde_json::from_str(include_str!("../tests/fixtures/starknet/sdk.json")).unwrap();
        for case in fixture.cases {
            let key = felt(&case.key);
            assert_eq!(public_key(&key), felt(&case.public_key));
            let signature = sign(&felt(&case.message_hash), &key).unwrap();
            assert_eq!(signature, (felt(&case.r), felt(&case.s)));
        }
    }
}
