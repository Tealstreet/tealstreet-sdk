//! Starknet SNIP-12 typed-data hashing and Stark-curve signing (Paradex).
use starknet_core::types::{Felt, TypedData};

pub fn parse_felt(input: &str) -> Result<Felt, String> {
    let parsed = if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        Felt::from_hex(&format!("0x{hex}"))
    } else {
        Felt::from_dec_str(input)
    };
    parsed.map_err(|error| format!("invalid felt {input:?}: {error}"))
}

/// Hash SNIP-12 typed data (revision 0 or 1) for an account address.
pub fn hash_typed_data(raw: &str, address: &Felt) -> Result<Felt, String> {
    let typed: TypedData = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    typed
        .message_hash(*address)
        .map_err(|error| error.to_string())
}

pub fn public_key(private_key: &Felt) -> Felt {
    starknet_crypto::get_public_key(private_key)
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
    struct TypedCase {
        raw: String,
        address: String,
        key: String,
        hash: String,
        public_key: String,
        r: String,
        s: String,
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
        parse_felt(text).unwrap()
    }

    #[test]
    fn typed_data_hashes_and_signatures_match_starknet_js() {
        let fixture: Fixture<TypedCase> =
            serde_json::from_str(include_str!("../tests/fixtures/starknet/typed-data.json"))
                .unwrap();
        assert!(!fixture.cases.is_empty());
        for case in fixture.cases {
            let hash = hash_typed_data(&case.raw, &felt(&case.address)).unwrap();
            assert_eq!(hash, felt(&case.hash), "{}", case.raw);
            let key = felt(&case.key);
            assert_eq!(public_key(&key), felt(&case.public_key));
            assert_eq!(sign(&hash, &key).unwrap(), (felt(&case.r), felt(&case.s)));
        }
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
