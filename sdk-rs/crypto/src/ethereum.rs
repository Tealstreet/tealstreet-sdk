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
    fn fixed_abi_preimage_hash_and_recoverable_signature_vectors() {
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
            // Independent ethers-generated ABI bytes remain the signing
            // preimage oracle; venue payload construction belongs to venues.
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
    fn actual_typescript_aster_signing_preimages() {
        let corpus: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/ethereum/aster-preimages.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        for name in ["complete", "omitted", "unicode", "keys"] {
            assert!(
                cases.iter().any(|case| case["name"] == name),
                "missing live Aster TS preimage property {name}"
            );
        }
        for case in cases {
            let abi = bytes(case["abi"].as_str().unwrap());
            let hash = keccak256(&abi);
            assert_eq!(
                hash.to_vec(),
                bytes(case["keccak"].as_str().unwrap()),
                "live TS Keccak {}",
                case["name"]
            );
            let digest = ethereum_message_hash(&hash);
            assert_eq!(
                digest.to_vec(),
                bytes(case["signDigest"].as_str().unwrap()),
                "live TS digest preimage {}",
                case["name"]
            );
            assert_eq!(
                sign_digest(&digest, case["key"].as_str().unwrap()).unwrap(),
                case["signature"].as_str().unwrap(),
                "live TS signature {}",
                case["name"]
            );
        }
    }

    #[test]
    fn ethers_exact_byte_hashes() {
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
    fn ethers_oversized_icap_reference_bug_is_preserved() {
        let case: Value =
            serde_json::from_str(include_str!("../tests/fixtures/ethereum/icap-maximum.json"))
                .unwrap();
        assert!(
            case["address"].as_str().unwrap().len() > 42,
            "oversized reference case required"
        );
        assert_eq!(
            get_address(case["input"].as_str().unwrap()).unwrap(),
            case["address"].as_str().unwrap(),
            "ethers oversized ICAP spelling"
        );
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
