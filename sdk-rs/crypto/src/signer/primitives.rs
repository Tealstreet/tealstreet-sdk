//! Local provider arms for non-HMAC algorithms. Exact key, message and result
//! widths follow `safe_cex_engine::signing::Algorithm`; failures never echo input.
use safe_cex_engine::signing::SignError;

/// 32-byte seed, or the 64-byte `seed || public` keypair form whose public half
/// must match the seed; exact message; raw 64-byte signature.
pub(super) fn ed25519(key: &[u8], message: &[u8]) -> Result<Vec<u8>, SignError> {
    #[cfg(feature = "ed25519")]
    {
        let seed = match key.len() {
            32 => key,
            64 => {
                let (seed, public) = key.split_at(32);
                let derived =
                    crate::ed25519::public_key(seed).map_err(|_| SignError::InvalidRequest)?;
                if derived[..] != *public {
                    return Err(SignError::InvalidRequest);
                }
                seed
            }
            _ => return Err(SignError::InvalidRequest),
        };
        crate::ed25519::sign(message, seed)
            .map(|signature| signature.to_vec())
            .map_err(|_| SignError::InvalidRequest)
    }
    #[cfg(not(feature = "ed25519"))]
    {
        let _ = (key, message);
        Err(SignError::Unsupported)
    }
}

/// PKCS8 or SEC1 P-256 DER (trusted PEM decoding yields either); one SHA-256
/// of the exact message; fixed-width r || s.
pub(super) fn es256(key: &[u8], message: &[u8]) -> Result<Vec<u8>, SignError> {
    #[cfg(feature = "es256")]
    {
        use p256::ecdsa::{Signature, SigningKey, signature::Signer};
        use p256::pkcs8::DecodePrivateKey;
        let signing = SigningKey::from_pkcs8_der(key)
            .or_else(|_| p256::SecretKey::from_sec1_der(key).map(SigningKey::from))
            .map_err(|_| SignError::InvalidRequest)?;
        let signature: Signature = signing
            .try_sign(message)
            .map_err(|_| SignError::InvalidRequest)?;
        Ok(signature.to_bytes().to_vec())
    }
    #[cfg(not(feature = "es256"))]
    {
        let _ = (key, message);
        Err(SignError::Unsupported)
    }
}

/// 32-byte secret; exact 32-byte digest; r || s || recovery id (0 or 1).
pub(super) fn secp256k1_digest(key: &[u8], digest: &[u8]) -> Result<Vec<u8>, SignError> {
    #[cfg(feature = "secp256k1")]
    {
        use alloy_primitives::B256;
        use alloy_signer::SignerSync;
        let digest: [u8; 32] = digest.try_into().map_err(|_| SignError::InvalidRequest)?;
        if key.len() != 32 {
            return Err(SignError::InvalidRequest);
        }
        let signer = alloy_signer_local::PrivateKeySigner::from_slice(key)
            .map_err(|_| SignError::InvalidRequest)?;
        let signature = signer
            .sign_hash_sync(&B256::from(digest))
            .map_err(|_| SignError::InvalidRequest)?;
        let mut output = Vec::with_capacity(65);
        output.extend_from_slice(&signature.r().to_be_bytes::<32>());
        output.extend_from_slice(&signature.s().to_be_bytes::<32>());
        output.push(u8::from(signature.v()));
        Ok(output)
    }
    #[cfg(not(feature = "secp256k1"))]
    {
        let _ = (key, digest);
        Err(SignError::Unsupported)
    }
}

/// Stark curve order n; keys must satisfy 1 <= key < n, as in starknet.js.
#[cfg(feature = "stark")]
const STARK_ORDER: [u8; 32] = [
    0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
    0xb7, 0x81, 0x12, 0x6d, 0xca, 0xe7, 0xb2, 0x32, 0x1e, 0x66, 0xa2, 0x41, 0xad, 0xc6, 0x4d, 0x2f,
];

/// 32-byte big-endian Stark key in [1, n); 32-byte big-endian field digest;
/// big-endian r || s. Deterministic RFC 6979, matching starknet.js.
pub(super) fn stark_digest(key: &[u8], digest: &[u8]) -> Result<Vec<u8>, SignError> {
    #[cfg(feature = "stark")]
    {
        if key >= &STARK_ORDER[..] {
            return Err(SignError::InvalidRequest);
        }
        let (r, s) = crate::starknet::sign(&stark_felt(digest)?, &stark_felt(key)?)
            .map_err(|_| SignError::InvalidRequest)?;
        let mut output = Vec::with_capacity(64);
        output.extend_from_slice(&r.to_bytes_be());
        output.extend_from_slice(&s.to_bytes_be());
        Ok(output)
    }
    #[cfg(not(feature = "stark"))]
    {
        let _ = (key, digest);
        Err(SignError::Unsupported)
    }
}

/// Exact 32-byte big-endian felt below the field modulus. The byte copy is
/// wiped; starknet-crypto's Felt type cannot be.
#[cfg(feature = "stark")]
fn stark_felt(bytes: &[u8]) -> Result<starknet_crypto::Felt, SignError> {
    let bytes = zeroize::Zeroizing::new(
        <[u8; 32]>::try_from(bytes).map_err(|_| SignError::InvalidRequest)?,
    );
    // Reject values at or above the field modulus instead of reducing them.
    let felt = starknet_crypto::Felt::from_bytes_be(&bytes);
    if felt.to_bytes_be() != *bytes {
        return Err(SignError::InvalidRequest);
    }
    Ok(felt)
}

/// Same key form as `secp256k1_digest`; the 20-byte Ethereum address.
pub(super) fn secp256k1_address(key: &[u8]) -> Result<Vec<u8>, SignError> {
    #[cfg(feature = "secp256k1")]
    {
        if key.len() != 32 {
            return Err(SignError::InvalidRequest);
        }
        let signer = alloy_signer_local::PrivateKeySigner::from_slice(key)
            .map_err(|_| SignError::InvalidRequest)?;
        Ok(signer.address().to_vec())
    }
    #[cfg(not(feature = "secp256k1"))]
    {
        let _ = key;
        Err(SignError::Unsupported)
    }
}

/// Same key form as `ed25519`; the 32-byte public key.
pub(super) fn ed25519_public(key: &[u8]) -> Result<Vec<u8>, SignError> {
    #[cfg(feature = "ed25519")]
    {
        crate::ed25519::public_key(key)
            .map(|public| public.to_vec())
            .map_err(|_| SignError::InvalidRequest)
    }
    #[cfg(not(feature = "ed25519"))]
    {
        let _ = key;
        Err(SignError::Unsupported)
    }
}

/// Same key form as `stark_digest`, in [1, n); the 32-byte big-endian public x.
pub(super) fn stark_public(key: &[u8]) -> Result<Vec<u8>, SignError> {
    #[cfg(feature = "stark")]
    {
        if key >= &STARK_ORDER[..] || key.iter().all(|byte| *byte == 0) {
            return Err(SignError::InvalidRequest);
        }
        Ok(crate::starknet::public_key(&stark_felt(key)?)
            .to_bytes_be()
            .to_vec())
    }
    #[cfg(not(feature = "stark"))]
    {
        let _ = key;
        Err(SignError::Unsupported)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(
        feature = "ed25519",
        feature = "es256",
        feature = "secp256k1",
        feature = "stark"
    ))]
    fn bytes(value: &str) -> Vec<u8> {
        let value = value.trim_start_matches("0x");
        let value = if value.len() % 2 == 1 {
            format!("0{value}")
        } else {
            value.to_owned()
        };
        (0..value.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
            .collect()
    }
    #[cfg(feature = "stark")]
    fn word(value: &str) -> Vec<u8> {
        let raw = bytes(value);
        let mut padded = vec![0; 32 - raw.len()];
        padded.extend(raw);
        padded
    }

    #[cfg(feature = "ed25519")]
    #[test]
    fn ed25519_matches_node_vectors_and_exact_seed_width() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/ed25519/node.json")).unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let seed = bytes(case["seed"].as_str().unwrap());
            let payload = case["payload"].as_str().unwrap().as_bytes();
            assert_eq!(
                ed25519(&seed, payload).unwrap(),
                bytes(case["signature"].as_str().unwrap())
            );
            assert_eq!(
                ed25519(&seed[..31], payload),
                Err(SignError::InvalidRequest)
            );
        }
    }

    #[cfg(feature = "ed25519")]
    #[test]
    fn ed25519_keypair_form_signs_as_its_seed_and_refuses_a_foreign_public_half() {
        // RFC 8032 test 1 seed and its published public key.
        let seed = bytes("9d61b19deffd5a60ba844af492ec2cc44449c5697b326919703bac031cae7f60");
        let public = bytes("d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a");
        let keypair = [seed.clone(), public.clone()].concat();
        assert_eq!(
            ed25519(&keypair, b"payload"),
            Ok(ed25519(&seed, b"payload").unwrap())
        );
        let mut foreign = keypair.clone();
        foreign[63] ^= 1;
        assert_eq!(
            ed25519(&foreign, b"payload"),
            Err(SignError::InvalidRequest)
        );
        assert_eq!(
            ed25519(&keypair[..63], b"payload"),
            Err(SignError::InvalidRequest)
        );
    }

    #[cfg(feature = "es256")]
    #[test]
    fn es256_matches_rfc6979_p256_sha256_sample_from_sec1_der() {
        // RFC 6979 A.2.5, P-256 / SHA-256, message "sample".
        let mut der = vec![0x30, 0x31, 0x02, 0x01, 0x01, 0x04, 0x20];
        der.extend(bytes(
            "C9AFA9D845BA75166B5C215767B1D6934E50C3DB36E89B127B8A622B120F6721",
        ));
        der.extend([
            0xa0, 0x0a, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
        ]);
        assert_eq!(
            es256(&der, b"sample").unwrap(),
            bytes(concat!(
                "EFD48B2AACB6A8FD1140DD9CD45E81D69D2C877B56AAF991C34D0EA84EAF3716",
                "F7CB1C942D657C41D436C7A1B6E29F65F3E900DBB9AFF4064DC4AB2F843ACDA8"
            ))
        );
        assert_eq!(es256(&der[..20], b"sample"), Err(SignError::InvalidRequest));
        // The same key in a PKCS8 envelope takes the first import path.
        let mut pkcs8 = vec![
            0x30, 0x4d, 0x02, 0x01, 0x00, 0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d,
            0x02, 0x01, 0x06, 0x08, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07, 0x04, 0x33,
        ];
        pkcs8.extend(&der);
        assert!(
            p256::SecretKey::from_sec1_der(&pkcs8).is_err(),
            "PKCS8 path exercised"
        );
        assert_eq!(
            es256(&pkcs8, b"sample").unwrap(),
            es256(&der, b"sample").unwrap()
        );
    }

    #[cfg(feature = "secp256k1")]
    #[test]
    fn secp256k1_digest_matches_ethers_recoverable_signatures() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ethereum/primitives.json"
        ))
        .unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let key = bytes(case["privateKey"].as_str().unwrap());
            let digest = bytes(case["messageHash"].as_str().unwrap());
            let mut expected = bytes(case["signature"].as_str().unwrap());
            // ethers encodes v as 27/28; the provider contract is recovery 0/1.
            expected[64] -= 27;
            assert_eq!(secp256k1_digest(&key, &digest).unwrap(), expected);
            assert_eq!(
                secp256k1_digest(&key, &digest[..31]),
                Err(SignError::InvalidRequest)
            );
            assert_eq!(
                secp256k1_digest(&key[..31], &digest),
                Err(SignError::InvalidRequest)
            );
        }
    }

    #[cfg(feature = "stark")]
    #[test]
    fn stark_digest_matches_starknet_js_and_refuses_non_canonical_felts() {
        let corpus: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/starknet/sdk.json")).unwrap();
        for case in corpus["cases"].as_array().unwrap() {
            let key = word(case["key"].as_str().unwrap());
            let digest = word(case["messageHash"].as_str().unwrap());
            let mut expected = word(case["r"].as_str().unwrap());
            expected.extend(word(case["s"].as_str().unwrap()));
            assert_eq!(stark_digest(&key, &digest).unwrap(), expected);
        }
        let one = word("0x1");
        assert_eq!(
            stark_digest(&STARK_ORDER, &one),
            Err(SignError::InvalidRequest)
        );
        assert_eq!(
            stark_digest(&[0xff; 32], &one),
            Err(SignError::InvalidRequest)
        );
        assert_eq!(stark_digest(&[0; 32], &one), Err(SignError::InvalidRequest));
        assert_eq!(
            stark_digest(&one, &one[..31]),
            Err(SignError::InvalidRequest)
        );
    }

    #[test]
    fn disabled_arms_refuse_as_unsupported() {
        let key = [1u8; 32];
        #[cfg(not(feature = "ed25519"))]
        assert_eq!(ed25519(&key, b""), Err(SignError::Unsupported));
        #[cfg(not(feature = "es256"))]
        assert_eq!(es256(&key, b""), Err(SignError::Unsupported));
        #[cfg(not(feature = "secp256k1"))]
        assert_eq!(secp256k1_digest(&key, &key), Err(SignError::Unsupported));
        #[cfg(not(feature = "stark"))]
        assert_eq!(stark_digest(&key, &key), Err(SignError::Unsupported));
        let _ = key;
    }
}
