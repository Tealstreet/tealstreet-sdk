//! Shared Ethereum primitives, without venue payload construction or entropy.
use alloy_primitives::{B256, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;

fn checksum_hex(body: &str) -> String {
    let mut bytes = body.to_ascii_lowercase().into_bytes();
    // ref-bug: ethers.getAddress accepts oversized ICAP and checks only its first 40 hex chars.
    let hash = alloy_primitives::keccak256(&bytes[..40]);
    for (index, byte) in bytes.iter_mut().take(40).enumerate() {
        let nibble = if index % 2 == 0 {
            hash[index / 2] >> 4
        } else {
            hash[index / 2] & 15
        };
        if nibble >= 8 {
            byte.make_ascii_uppercase();
        }
    }
    format!("0x{}", String::from_utf8_lossy(&bytes))
}

/// ethers.getAddress: normalize hex/direct-mode ICAP and enforce mixed-case EIP-55.
pub fn get_address(value: &str) -> Result<String, String> {
    let body = value.strip_prefix("0x").unwrap_or(value);
    if body.len() == 40 && body.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let checked = checksum_hex(body);
        if body.bytes().any(|byte| matches!(byte, b'a'..=b'f'))
            && body.bytes().any(|byte| matches!(byte, b'A'..=b'F'))
            && checked != format!("0x{body}")
        {
            return Err("bad address checksum".into());
        }
        return Ok(checked);
    }
    let bytes = value.as_bytes();
    if matches!(bytes.len(), 34 | 35)
        && bytes.starts_with(b"XE")
        && bytes[2..4].iter().all(u8::is_ascii_digit)
        && bytes[4..].iter().all(u8::is_ascii_alphanumeric)
    {
        let mut remainder = 0u32;
        for byte in bytes[4..].iter().copied().chain(b"XE00".iter().copied()) {
            let digit = byte.to_ascii_uppercase();
            let number = if digit.is_ascii_digit() {
                u32::from(digit - b'0')
            } else {
                u32::from(digit - b'A') + 10
            };
            remainder = (remainder * if number < 10 { 10 } else { 100 } + number) % 97;
        }
        if value[2..4] != format!("{:02}", 98 - remainder) {
            return Err("bad icap checksum".into());
        }
        let number = U256::from_str_radix(&value[4..], 36).map_err(|_| "invalid address")?;
        return Ok(checksum_hex(&format!("{number:040x}")));
    }
    Err("invalid address".into())
}

/// Derive the Ethereum address of a local signing key.
pub fn private_key_address(private_key: &str) -> Result<String, String> {
    let signer: PrivateKeySigner = private_key
        .parse()
        .map_err(|error: alloy_signer_local::LocalSignerError| error.to_string())?;
    Ok(signer.address().to_checksum(None))
}

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

/// Pre-release check for a host/remote signature (P4): true only when the
/// 65-byte low-s r || s || v (v 0/1 or 27/28) over `digest` recovers to the
/// bound actual signer. Callers never pass expected venue config here.
pub fn signed_by(digest: &[u8], signature: &[u8], signer: &str) -> bool {
    // secp256k1 n / 2: recovery normalizes s, but venues enforcing low-s would
    // reject a malleated signature only after it was sent.
    const HALF_ORDER: [u8; 32] = [
        0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
        0xff, 0x5d, 0x57, 0x6e, 0x73, 0x57, 0xa4, 0x50, 0x1d, 0xdf, 0xe9, 0x2f, 0x46, 0x68, 0x1b,
        0x20, 0xa0,
    ];
    let (Ok(digest), Ok(signer)) = (
        <[u8; 32]>::try_from(digest),
        signer.parse::<alloy_primitives::Address>(),
    ) else {
        return false;
    };
    let [body @ .., v] = signature else {
        return false;
    };
    let parity = match v {
        0 | 27 => false,
        1 | 28 => true,
        _ => return false,
    };
    body.len() == 64
        && body[32..] <= HALF_ORDER[..]
        && alloy_primitives::Signature::from_bytes_and_parity(body, parity)
            .recover_address_from_prehash(&B256::from(digest))
            .is_ok_and(|recovered| recovered == signer)
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
            let raw = bytes(&signature);
            let signer = case["recovered"].as_str().unwrap();
            assert!(signed_by(&digest, &raw, signer), "{}", case["name"]);
            assert!(signed_by(&digest, &raw, &signer.to_ascii_lowercase()));
            let mut parity = raw.clone();
            parity[64] -= 27;
            assert!(signed_by(&digest, &parity, signer));
            let mut changed = digest;
            changed[0] ^= 1;
            assert!(!signed_by(&changed, &raw, signer));
            assert!(!signed_by(&digest, &raw[..64], signer));
            assert!(!signed_by(
                &digest,
                &raw,
                "0x0000000000000000000000000000000000000001"
            ));
            assert!(!signed_by(&digest, &raw, "not an address"));
            let mut invalid_v = raw.clone();
            invalid_v[64] = 29;
            assert!(!signed_by(&digest, &invalid_v, signer));
            // The malleated twin (n - s, flipped parity) recovers the same signer.
            let order = U256::from_str_radix(
                "fffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141",
                16,
            )
            .unwrap();
            let high = order - U256::from_be_slice(&raw[32..64]);
            let mut malleated = raw.clone();
            malleated[32..64].copy_from_slice(&high.to_be_bytes::<32>());
            malleated[64] = 55 - raw[64];
            assert!(!signed_by(&digest, &malleated, signer));
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

    #[test]
    fn ethers_address_accepted_set_and_checksum() {
        let corpus = corpus();
        let cases = corpus["addresses"].as_array().unwrap();
        for name in [
            "lower",
            "upper",
            "checksum",
            "no-prefix",
            "invalid-checksum",
            "short",
            "upper-prefix",
            "bom",
            "leading-space",
            "trailing-space",
            "invalid-hex",
            "icap",
            "icap-lower",
            "invalid-icap-checksum",
        ] {
            assert!(
                cases.iter().any(|case| case["name"] == name),
                "missing address property {name}"
            );
        }
        for case in cases {
            let input = case["input"].as_str().unwrap();
            let actual = get_address(input);
            if let Some(expected) = case.get("address") {
                assert_eq!(
                    actual.as_deref(),
                    Ok(expected.as_str().unwrap()),
                    "getAddress {}",
                    case["name"]
                );
            } else {
                assert_eq!(
                    actual.expect_err(&format!("ethers refuses address {}", case["name"])),
                    case["error"]["shortMessage"].as_str().unwrap(),
                    "address refusal {}",
                    case["name"]
                );
            }
        }
    }
}
