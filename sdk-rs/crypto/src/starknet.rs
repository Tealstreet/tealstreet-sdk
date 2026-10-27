//! Stark-curve public keys and deterministic signing.
use crate::{eip712, ethereum};
use alloy_primitives::U256;
use starknet_crypto::Felt;
use zeroize::Zeroizing;

pub fn public_key(private_key: &Felt) -> Felt {
    starknet_crypto::get_public_key(private_key)
}

/// The public x of a big-endian private scalar, big-endian.
pub fn public_key_bytes(private_key: &[u8; 32]) -> [u8; 32] {
    public_key(&Felt::from_bytes_be(private_key)).to_bytes_be()
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

const CURVE_ORDER: &str =
    "3618502788666131213697322783095070105526743751716087489154079457884512865583";

fn unsigned(input: &str) -> Result<U256, String> {
    let (digits, radix) = if let Some(hex) = input
        .strip_prefix("0x")
        .or_else(|| input.strip_prefix("0X"))
    {
        (hex, 16)
    } else {
        (input, 10)
    };
    if digits.is_empty()
        || !digits.bytes().all(|b| {
            if radix == 16 {
                b.is_ascii_hexdigit()
            } else {
                b.is_ascii_digit()
            }
        })
    {
        return Err("Invalid Paradex field value".into());
    }
    U256::from_str_radix(digits, radix).map_err(|_| "Invalid Paradex field value".into())
}

/// A Paradex Stark private key as the venue always accepted it: `0x`/`0X` hex
/// or decimal digits, `0 < k < n`. Big-endian.
pub fn parse_stark_key(input: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    let value = unsigned(input).map_err(|_| "Invalid Paradex private key")?;
    let order = U256::from_str_radix(CURVE_ORDER, 10).expect("curve order");
    if value == U256::ZERO || value >= order {
        return Err("Invalid Paradex private key".into());
    }
    Ok(Zeroizing::new(value.to_be_bytes::<32>()))
}

/// Paradex's EIP-712 `Constant("STARK Key")` digest for an Ethereum chain ID.
pub fn ethereum_key_digest(chain: &str) -> Result<[u8; 32], String> {
    let chain = unsigned(chain).map_err(|_| "Invalid Paradex Ethereum chain ID")?;
    // EIP712Domain(string name,string version,uint256 chainId), with no
    // verifyingContract field. Each string occupies its hashed 32-byte word.
    let mut domain = [0u8; 128];
    domain[..32].copy_from_slice(&ethereum::keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId)",
    ));
    domain[32..64].copy_from_slice(&ethereum::keccak256(b"Paradex"));
    domain[64..96].copy_from_slice(&ethereum::keccak256(b"1"));
    domain[96..].copy_from_slice(&chain.to_be_bytes::<32>());
    let mut constant = [0u8; 64];
    constant[..32].copy_from_slice(&ethereum::keccak256(b"Constant(string action)"));
    constant[32..].copy_from_slice(&ethereum::keccak256(b"STARK Key"));
    Ok(eip712::hash_prehashed(
        ethereum::keccak256(&domain),
        ethereum::keccak256(&constant),
    ))
}

/// The Stark scalar Paradex derives from an Ethereum key on `chain`: the
/// `STARK Key` signature's r, ground below the curve order. Big-endian.
pub fn derive_from_ethereum(key: &str, chain: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    let digest = ethereum_key_digest(chain)?;
    let seed = ethereum::sign_digest(&digest, key).map_err(|_| "Invalid Paradex Ethereum key")?;
    let r = seed
        .strip_prefix("0x")
        .unwrap_or(&seed)
        .get(..64)
        .ok_or("Invalid Paradex Ethereum signature")?;
    let mut r_bytes = [0u8; 32];
    for (byte, pair) in r_bytes.iter_mut().zip(r.as_bytes().as_chunks::<2>().0) {
        *byte = u8::from_str_radix(
            std::str::from_utf8(pair).map_err(|_| "Invalid Paradex Ethereum signature")?,
            16,
        )
        .map_err(|_| "Invalid Paradex Ethereum signature")?;
    }
    let order = U256::from_str_radix(CURVE_ORDER, 10).expect("curve order");
    // 2^256 - (2^256 mod order), represented without overflowing U256.
    let limit = U256::MAX - (U256::MAX % order);
    let private = (0u32..)
        .find_map(|index| {
            let index_bytes = index.to_be_bytes();
            let index_start = index_bytes.iter().position(|b| *b != 0).unwrap_or(3);
            let mut material = r_bytes.to_vec();
            material.extend_from_slice(&index_bytes[index_start..]);
            let digest = U256::from_be_bytes(crate::sha256(&material));
            (digest < limit).then_some(digest % order)
        })
        .ok_or("Paradex key derivation failed")?;
    if private == U256::ZERO || private >= order {
        return Err("Invalid Paradex private key".into());
    }
    Ok(Zeroizing::new(private.to_be_bytes::<32>()))
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
