//! Stark-curve public keys and deterministic signing.
use starknet_crypto::Felt;

pub fn public_key(private_key: &Felt) -> Felt {
    starknet_crypto::get_public_key(private_key)
}

/// Stark curve `y^2 = x^3 + x + BETA` (alpha = 1), as starknet-curve.
const BETA: Felt =
    Felt::from_hex_unchecked("0x6f21413efbe40de150e596d72f7a8c5609ad26c15c915c1f4cdfcb99cee9e89");

/// A 32-byte big-endian public x: canonical, nonzero, and on the curve.
pub fn is_public_key(bytes: &[u8]) -> bool {
    let Ok(bytes) = <[u8; 32]>::try_from(bytes) else {
        return false;
    };
    let x = Felt::from_bytes_be(&bytes);
    x.to_bytes_be() == bytes && x != Felt::ZERO && (x.square() * x + x + BETA).sqrt().is_some()
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
            assert!(is_public_key(&felt(&case.public_key).to_bytes_be()));
        }
    }

    #[test]
    fn public_key_check_agrees_with_starknet_crypto_and_refuses_non_canonical() {
        let mut off_curve = 0;
        for x in 1u64..=64 {
            let x = Felt::from(x);
            let refused = matches!(
                starknet_crypto::verify(&x, &Felt::ONE, &Felt::ONE, &Felt::ONE),
                Err(starknet_crypto::VerifyError::InvalidPublicKey)
            );
            assert_eq!(is_public_key(&x.to_bytes_be()), !refused, "{x}");
            off_curve += usize::from(refused);
        }
        assert!(off_curve > 0);
        assert!(!is_public_key(&[0; 32]));
        assert!(!is_public_key(&[0xff; 32]));
        assert!(!is_public_key(&[1; 31]));
    }
}
