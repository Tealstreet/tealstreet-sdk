//! Goldilocks Poseidon2 and ECgFp5 Schnorr, bound to checked-in Go-WASM vectors.
use goldilocks_crypto::{Point, ScalarField};
use poseidon_hash::{
    Goldilocks, hash_no_pad, hash_out_from_bytes_le, hash_out_to_bytes_le,
    hash_to_quintic_extension,
};
use zeroize::Zeroize;

fn words(input: &[u8]) -> Result<Vec<Goldilocks>, String> {
    if !input.len().is_multiple_of(8) {
        return Err("input length must be multiple of 8 bytes".into());
    }
    Ok(input
        .as_chunks::<8>()
        .0
        .iter()
        .map(|bytes| {
            // Go accepts raw u64 limbs, including noncanonical values, then reduces field arithmetic.
            let value = u64::from_le_bytes(*bytes);
            Goldilocks::from_canonical_u64(value % Goldilocks::MODULUS)
        })
        .collect())
}
fn scalar(bytes: &[u8]) -> Result<[u8; 40], String> {
    let value = ScalarField::from_bytes_le(bytes)?;
    Ok(ScalarField::from_non_canonical_limbs(value.limbs()).to_bytes_le())
}
/// Poseidon2 HashNoPad; raw words and digest words are little endian.
pub fn poseidon2_hash(input: &[u8]) -> Result<[u8; 32], String> {
    Ok(hash_out_to_bytes_le(hash_no_pad(&words(input)?)))
}
/// Poseidon2 hash to the quintic extension used by Schnorr.
pub fn poseidon2_hash_fp5(input: &[u8]) -> Result<[u8; 40], String> {
    Ok(hash_to_quintic_extension(&words(input)?).to_bytes_le())
}
/// Two-to-one compression of two canonical 32-byte hash outputs.
pub fn poseidon2_two_to_one(left: &[u8], right: &[u8]) -> Result<[u8; 32], String> {
    Ok(hash_out_to_bytes_le(poseidon_hash::hash_n_to_one(&[
        hash_out_from_bytes_le(left)?,
        hash_out_from_bytes_le(right)?,
    ])))
}
/// Derive the compressed public key, including Go's scalar reduction semantics.
pub fn public_key(key: &[u8]) -> Result<[u8; 40], String> {
    let key = ScalarField::from_bytes_le(&scalar(key)?)?;
    Ok(Point::generator().mul(&key).encode().to_bytes_le())
}
/// A canonical, nonzero secret nonce, consumed by signing and never Clone/Copy.
/// Hosts must supply fresh CSPRNG output; construction rejects broken all-zero entropy.
/// ```compile_fail
/// use safe_cex_crypto::lighter::SchnorrNonce;
/// let forged = SchnorrNonce([0u8; 40]);
/// ```
/// ```compile_fail
/// use safe_cex_crypto::lighter::SchnorrNonce;
/// let nonce = SchnorrNonce::from_bytes(&[1u8; 40]).unwrap();
/// let reused = nonce.clone();
/// ```
/// ```compile_fail
/// use safe_cex_crypto::lighter::{SchnorrNonce, schnorr_sign};
/// let nonce = SchnorrNonce::from_bytes(&[1u8; 40]).unwrap();
/// let _ = schnorr_sign(&[], &[1u8; 40], nonce);
/// let _ = schnorr_sign(&[], &[1u8; 40], nonce);
/// ```
#[repr(transparent)]
pub struct SchnorrNonce([u8; 40]);
impl Drop for SchnorrNonce {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}
impl SchnorrNonce {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, String> {
        let reduced = scalar(bytes)?;
        // ref-bug: Go accepts an all-zero random nonce and leaks the scalar key.
        // Deliberately refuse signing it; verification still mirrors Go rejection.
        if ScalarField::from_bytes_le(&reduced)?.is_zero() {
            return Err("nonce must be nonzero".into());
        }
        if bytes != reduced {
            return Err("nonce must be below the scalar order".into());
        }
        Ok(Self(reduced))
    }
}
/// Sign raw word-aligned bytes with a validated nonce, consumed exactly once.
/// Hosts own fresh, unpredictable nonce generation; re-constructing a reused nonce reveals the key.
pub fn schnorr_sign(input: &[u8], key: &[u8], nonce: SchnorrNonce) -> Result<Vec<u8>, String> {
    goldilocks_crypto::sign_hashed_message(&scalar(key)?, &poseidon2_hash_fp5(input)?, &nonce.0)
        .map_err(|error| error.to_string())
}
/// Verify against Go's hash-to-Fp5 preimage, rejecting a neutral nonce commitment.
pub fn schnorr_verify(input: &[u8], key: &[u8], signature: &[u8]) -> Result<bool, String> {
    let message = poseidon2_hash_fp5(input)?;
    if signature.len() != 80 {
        return Err("signature must be 80 bytes".into());
    }
    let s = ScalarField::from_bytes_le(&signature[..40])?;
    let e = ScalarField::from_bytes_le(&signature[40..])?;
    let encoded_key = poseidon_hash::Fp5Element::from_bytes_le(key)?;
    let public_point = Point::decode(&encoded_key).ok_or("invalid public key")?;
    let commitment = Point::generator().mul(&s).add(&public_point.mul(&e));
    if commitment.is_neutral() {
        return Ok(false);
    }
    goldilocks_crypto::verify_signature(signature, &message, key).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    fn decode(text: &str) -> Vec<u8> {
        text.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
    fn bytes(case: &Value, key: &str) -> Vec<u8> {
        decode(case[key].as_str().unwrap())
    }
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
    #[test]
    fn zero_nonce_key_recovery_signature_is_refused_like_go() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/lighter/go-wasm-security.json"
        ))
        .unwrap();
        let cases = fixture["signatures"].as_array().unwrap();
        assert!(
            cases
                .iter()
                .any(|case| bytes(case, "nonce") == [0u8; 40] && case["verified"] == false),
            "zero-nonce rejection oracle required"
        );
        assert!(
            cases
                .iter()
                .any(|case| bytes(case, "nonce") != [0u8; 40] && case["verified"] == true),
            "nonzero-nonce verification control required"
        );
        for case in cases {
            let nonce = bytes(case, "nonce");
            let payload = bytes(case, "payload");
            let key = bytes(case, "key");
            let signature = bytes(case, "signature");
            assert_eq!(
                schnorr_verify(&payload, &bytes(case, "publicKey"), &signature).unwrap(),
                case["verified"].as_bool().unwrap(),
                "Go zero-commitment verification parity"
            );
            if nonce == [0u8; 40] {
                assert!(
                    SchnorrNonce::from_bytes(&nonce).is_err(),
                    "zero nonce must never reach signing"
                );
                let leaked_s = ScalarField::from_bytes_le(&signature[..40]).unwrap();
                let leaked_e = ScalarField::from_bytes_le(&signature[40..]).unwrap();
                let recovered = leaked_s
                    .neg()
                    .mul_canonical(&leaked_e.inverse().unwrap())
                    .to_bytes_le();
                assert_eq!(
                    recovered,
                    scalar(&key).unwrap(),
                    "public zero-nonce signature exposes the complete scalar key"
                );
            } else {
                assert_eq!(
                    hex(
                        &schnorr_sign(&payload, &key, SchnorrNonce::from_bytes(&nonce).unwrap())
                            .unwrap()
                    ),
                    case["signature"]
                );
            }
        }
        let mut order = decode(
            "7ffffffd800000077ffffff1000000167fffffe6cfb80639e8885c39d724a09ce80fd996948bffe1",
        );
        order.reverse();
        assert!(
            SchnorrNonce::from_bytes(&order).is_err(),
            "order reducing to zero is refused"
        );
        assert!(
            SchnorrNonce::from_bytes(&[255u8; 40]).is_err(),
            "over-order nonce refused"
        );
        assert!(
            SchnorrNonce::from_bytes(&[1u8; 39]).is_err(),
            "nonce size checked"
        );
    }
    #[test]
    fn nonce_drop_scrubs_owned_storage() {
        let raw = Box::into_raw(Box::new(SchnorrNonce::from_bytes(&[1u8; 40]).unwrap()));
        // Retain the allocation while dropping its value; inspect only initialized u8 storage.
        unsafe {
            let bytes = raw.cast::<u8>();
            std::ptr::drop_in_place(raw);
            assert_eq!(
                std::slice::from_raw_parts(bytes, 40),
                [0u8; 40],
                "nonce drop must zeroize its owned storage"
            );
            std::alloc::dealloc(raw.cast(), std::alloc::Layout::new::<SchnorrNonce>());
        }
    }
    #[test]
    fn malformed_raw_lengths_are_rejected_without_truncating_signed_bytes() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/lighter/go-wasm-lengths.json"
        ))
        .unwrap();
        let cases = fixture["invalidInputs"].as_array().unwrap();
        for required in [4, 9, 15] {
            assert!(
                cases
                    .iter()
                    .any(|case| bytes(case, "input").len() == required),
                "Go invalid length {required} required"
            );
        }
        let mut nonce = [0u8; 40];
        nonce[0] = 7;
        for case in cases {
            let input = bytes(case, "input");
            for operation in ["hash", "fp5", "signature", "verified"] {
                assert!(
                    case[operation]["error"].is_string(),
                    "Go refusal oracle missing for {operation}"
                );
            }
            assert!(
                poseidon2_hash(&input).is_err(),
                "unaligned hash must not discard trailing bytes"
            );
            assert!(
                poseidon2_hash_fp5(&input).is_err(),
                "unaligned Fp5 must not discard trailing bytes"
            );
            assert!(
                schnorr_sign(
                    &input,
                    &bytes(&fixture, "key"),
                    SchnorrNonce::from_bytes(&nonce).unwrap()
                )
                .is_err(),
                "unaligned signer must not discard trailing bytes"
            );
            assert!(
                schnorr_verify(
                    &input,
                    &bytes(&fixture, "publicKey"),
                    &bytes(&fixture, "validSignature")
                )
                .is_err(),
                "unaligned verifier must not discard trailing bytes"
            );
        }
    }
    #[test]
    fn two_to_one_noncanonical_reference_bound_is_explicit() {
        let fixture: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/lighter/go-wasm-two-noncanonical.json"
        ))
        .unwrap();
        let cases = fixture["twoToOne"].as_array().unwrap();
        let noncanonical = |bytes: &[u8]| {
            bytes
                .as_chunks::<8>()
                .0
                .iter()
                .any(|word| u64::from_le_bytes(*word) >= Goldilocks::MODULUS)
        };
        for required in [(false, false), (true, false), (false, true), (true, true)] {
            assert!(
                cases.iter().any(|case| (
                    noncanonical(&bytes(case, "left")),
                    noncanonical(&bytes(case, "right"))
                ) == required),
                "two-to-one canonicality coverage missing {required:?}"
            );
        }
        for case in cases {
            let left = bytes(case, "left");
            let right = bytes(case, "right");
            assert_eq!(
                bytes(case, "hash").len(),
                32,
                "Go cryptographic output required"
            );
            let actual = poseidon2_two_to_one(&left, &right);
            if noncanonical(&left) || noncanonical(&right) {
                assert!(
                    actual.is_err(),
                    "documented noncanonical two-to-one refusal"
                );
            } else {
                assert_eq!(
                    hex(&actual.unwrap()),
                    case["hash"],
                    "canonical two-to-one control"
                );
            }
        }
    }
    #[test]
    fn go_wasm_hash_and_scalar_vectors() {
        for (index, raw) in [
            include_str!("../tests/fixtures/lighter/go-wasm.json"),
            include_str!("../tests/fixtures/lighter/go-wasm-edges.json"),
            include_str!("../tests/fixtures/lighter/go-wasm-noncanonical.json"),
        ]
        .into_iter()
        .enumerate()
        {
            let fixture: Value = serde_json::from_str(raw).unwrap();
            let hashes = fixture["hashes"].as_array().unwrap();
            assert!(!hashes.is_empty(), "Go hash vectors required");
            if index == 1 {
                assert!(
                    hashes.iter().any(|case| bytes(case, "input").len() == 40),
                    "Go five-element vectors required"
                );
                let keys = fixture["keys"].as_array().unwrap();
                assert!(
                    keys.iter().any(|case| bytes(case, "key") == [0u8; 40]),
                    "Go zero-scalar key required"
                );
                assert!(
                    keys.iter().any(|case| bytes(case, "key") == [255u8; 40]),
                    "Go over-order scalar reduction required"
                );
            }
            if index == 2 {
                assert!(
                    hashes.iter().any(|case| bytes(case, "input")
                        .as_chunks::<8>()
                        .0
                        .iter()
                        .any(|word| u64::from_le_bytes(*word) >= Goldilocks::MODULUS)),
                    "Go noncanonical word vectors required"
                );
            }
            if index > 0 {
                assert!(
                    fixture["signature"].is_object(),
                    "Go specialized signature required"
                );
            }
            for case in hashes {
                assert_eq!(
                    hex(&poseidon2_hash(&bytes(case, "input")).unwrap()),
                    case["hash"],
                    "Go HashNoPad mismatch"
                );
                assert_eq!(
                    hex(&poseidon2_hash_fp5(&bytes(case, "input")).unwrap()),
                    case["fp5"],
                    "Go Fp5 mismatch"
                );
            }
            if let Some(cases) = fixture["keys"].as_array() {
                for case in cases {
                    assert_eq!(
                        hex(&public_key(&bytes(case, "key")).unwrap()),
                        case["publicKey"],
                        "Go reduced public key mismatch"
                    );
                }
            }
            if fixture["signature"].is_object() {
                assert_signature(&fixture["signature"]);
            }
        }
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/lighter/go-wasm.json")).unwrap();
        let sizes: std::collections::BTreeSet<_> = fixture["hashes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| bytes(case, "input").len())
            .collect();
        assert!(
            sizes.contains(&0)
                && sizes.contains(&64)
                && sizes.contains(&72)
                && sizes.contains(&136),
            "Go hash corpus must cover empty, rate boundary, and multiblock inputs"
        );
        assert!(
            !fixture["twoToOne"].as_array().unwrap().is_empty(),
            "Go two-to-one corpus required"
        );
        for case in fixture["twoToOne"].as_array().unwrap() {
            assert_eq!(
                hex(&poseidon2_two_to_one(&bytes(case, "left"), &bytes(case, "right")).unwrap()),
                case["hash"],
                "Go two-to-one mismatch"
            );
        }
    }
    fn assert_signature(case: &Value) {
        let key = bytes(case, "key");
        let payload = bytes(case, "payload");
        let nonce = bytes(case, "nonce");
        assert_eq!(
            hex(&public_key(&key).unwrap()),
            case["publicKey"],
            "Go public key mismatch"
        );
        assert_eq!(
            hex(&schnorr_sign(&payload, &key, SchnorrNonce::from_bytes(&nonce).unwrap()).unwrap()),
            case["signature"],
            "Go Schnorr mismatch"
        );
        assert!(
            schnorr_verify(
                &payload,
                &bytes(case, "publicKey"),
                &bytes(case, "signature")
            )
            .unwrap(),
            "Go signature rejected"
        );
    }
    #[test]
    fn go_wasm_schnorr_and_mutation_controls() {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/lighter/go-wasm.json")).unwrap();
        let cases = fixture["signatures"].as_array().unwrap();
        assert!(cases.iter().any(|case| bytes(case, "payload").len() > 64));
        assert!(
            cases
                .iter()
                .any(|case| bytes(case, "key")[8..].iter().any(|b| *b != 0))
        );
        for case in cases {
            assert_signature(case);
            assert!(
                !schnorr_verify(
                    &bytes(case, "changedPayload"),
                    &bytes(case, "publicKey"),
                    &bytes(case, "signature")
                )
                .unwrap(),
                "mutated payload accepted"
            );
            assert!(
                !schnorr_verify(
                    &bytes(case, "payload"),
                    &public_key(&bytes(case, "changedKey")).unwrap(),
                    &bytes(case, "signature")
                )
                .unwrap(),
                "mutated key accepted"
            );
            let mut nonce = bytes(case, "nonce");
            nonce[0] += 1;
            let changed = schnorr_sign(
                &bytes(case, "payload"),
                &bytes(case, "key"),
                SchnorrNonce::from_bytes(&nonce).unwrap(),
            )
            .unwrap();
            assert_eq!(
                hex(&changed),
                case["changedNonceSignature"],
                "Go mutated nonce mismatch"
            );
            assert_ne!(
                hex(&changed),
                case["signature"],
                "mutated nonce failed to change signature"
            );
        }
    }
}
