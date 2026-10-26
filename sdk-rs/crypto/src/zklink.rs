//! zkLink's deterministic Rescue/MuSig primitive, without wallet/provider adapters.
//!
//! Algorithm adapted from zkLinkProtocol/zklink_sdk at
//! 3152525e2615ecbbdb56fd6728ce265ad2dde485, signers/src/zklink_signer/
//! (Apache-2.0). The franklin-crypto fork supplies the curve and Rescue implementation.
use franklin_crypto::{
    alt_babyjubjub::{
        AltJubjubBn256, FixedGenerators,
        fs::{Fs, FsRepr},
    },
    bellman::{PrimeField, PrimeFieldRepr, pairing::bn256::Bn256},
    circuit::multipack,
    eddsa::{PrivateKey, PublicKey, Seed},
    rescue::{bn256::Bn256RescueParams, rescue_hash},
};
use sha2::{Digest, Sha256};

#[cfg(test)]
thread_local! {
    static EMPTY_RESCUE_OUTPUT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// Keep output cardinality a checked boundary even if the pinned fork changes.
fn rescue_output<T: Copy>(outputs: &[T]) -> Result<T, &'static str> {
    #[cfg(test)]
    let outputs = if EMPTY_RESCUE_OUTPUT.get() {
        &[]
    } else {
        outputs
    };
    outputs
        .first()
        .copied()
        .ok_or("zkLink Rescue returned no hash output")
}

/// Packed public key and packed MuSig signature, as returned by the npm WASM.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ZkLinkSignature {
    pub public_key: [u8; 32],
    pub signature: [u8; 64],
}

/// Derive a key from seed bytes and sign an already serialized message.
///
/// zkLink's transaction hash pads to 736 bits. Reject longer inputs instead of
/// reaching the upstream assertion; callers own transaction serialization.
pub fn sign(seed: &[u8], payload: &[u8]) -> Result<ZkLinkSignature, &'static str> {
    if seed.len() < 32 {
        return Err("zkLink seed must contain at least 32 bytes");
    }
    if payload.len() > 92 {
        return Err("zkLink payload exceeds 92 bytes");
    }
    // Derived seed material is wiped; the franklin scalar type cannot be.
    let mut effective_seed = zeroize::Zeroizing::new(Sha256::digest(seed).to_vec());
    let scalar = loop {
        let raw = Sha256::digest(&effective_seed);
        let mut repr = FsRepr::default();
        repr.read_be(&raw[..])
            .map_err(|_| "zkLink scalar encoding failed")?;
        match Fs::from_repr(repr) {
            Ok(value) => break value,
            Err(_) => effective_seed.copy_from_slice(&raw),
        }
    };
    let private_key = PrivateKey::<Bn256>(scalar);
    let jubjub = AltJubjubBn256::new();
    let rescue = Bn256RescueParams::new_checked_2_into_1();
    let mut bits: Vec<bool> = payload
        .iter()
        .flat_map(|byte| (0..8).map(move |bit| byte & (0x80 >> bit) != 0))
        .collect();
    bits.resize(736, false);
    let packed = multipack::compute_multipacking::<Bn256>(&bits);
    let hash = rescue_output(&rescue_hash::<Bn256>(&rescue, &packed))?;
    let mut hashed = [0u8; 32];
    hash.into_repr()
        .write_le(&mut hashed[..])
        .map_err(|_| "zkLink hash encoding failed")?;
    let nonce = Seed::deterministic_seed(&private_key, &hashed);
    let signed = private_key.musig_rescue_sign(
        &hashed,
        &nonce,
        FixedGenerators::SpendingKeyGenerator,
        &rescue,
        &jubjub,
    );
    let public =
        PublicKey::from_private(&private_key, FixedGenerators::SpendingKeyGenerator, &jubjub);
    let mut result = ZkLinkSignature {
        public_key: [0; 32],
        signature: [0; 64],
    };
    public
        .write(&mut result.public_key[..])
        .map_err(|_| "zkLink public key encoding failed")?;
    signed
        .r
        .write(&mut result.signature[..32])
        .map_err(|_| "zkLink R encoding failed")?;
    signed
        .s
        .into_repr()
        .write_le(&mut result.signature[32..])
        .map_err(|_| "zkLink S encoding failed")?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rescue_output_empty_is_an_error_not_a_signer_panic() {
        assert_eq!(
            rescue_output::<u8>(&[]),
            Err("zkLink Rescue returned no hash output")
        );
        assert_eq!(rescue_output(&[7u8]), Ok(7));
    }
    #[test]
    fn public_sign_propagates_empty_rescue_output() {
        struct Reset;
        impl Drop for Reset {
            fn drop(&mut self) {
                EMPTY_RESCUE_OUTPUT.set(false);
            }
        }
        EMPTY_RESCUE_OUTPUT.set(true);
        let _reset = Reset;
        assert_eq!(
            sign(&[7; 32], &[]),
            Err("zkLink Rescue returned no hash output"),
            "public sign must propagate the checked Rescue output boundary"
        );
    }
    use serde::Deserialize;
    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }
    #[derive(Deserialize)]
    struct Case {
        name: String,
        seed: String,
        payload: String,
        signed: Signed,
    }
    #[derive(Deserialize)]
    struct Signed {
        signature: Expected,
    }
    #[derive(Deserialize)]
    struct Expected {
        #[serde(rename = "pubKey")]
        public_key: String,
        signature: String,
    }
    fn decode(value: &str) -> Vec<u8> {
        value
            .trim_start_matches("0x")
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
    fn corpus() -> Corpus {
        serde_json::from_str(include_str!("../tests/fixtures/zklink/npm-wasm.json")).unwrap()
    }
    #[test]
    fn independent_native_sdk_pins_empty_and_maximum_payloads() {
        let corpus: serde_json::Value = serde_json::from_str(include_str!(
            "../tests/fixtures/zklink/native-sdk-boundaries.json"
        ))
        .unwrap();
        let cases = corpus["cases"].as_array().unwrap();
        assert!(
            cases
                .iter()
                .any(|c| decode(c["payload"].as_str().unwrap()).is_empty()),
            "empty native SDK payload required"
        );
        assert!(
            cases.iter().any(|c| {
                let bytes = decode(c["payload"].as_str().unwrap());
                bytes.len() == 92 && bytes.last().is_some_and(|last| *last != 0)
            }),
            "maximum native SDK payload with nonzero final byte required"
        );
        for case in cases {
            let seed = decode(case["seed"].as_str().unwrap());
            let payload = decode(case["payload"].as_str().unwrap());
            let signed = sign(&seed, &payload).unwrap();
            assert_eq!(
                signed.public_key.to_vec(),
                decode(case["publicKey"].as_str().unwrap()),
                "independent native SDK boundary public key"
            );
            assert_eq!(
                signed.signature.to_vec(),
                decode(case["signature"].as_str().unwrap()),
                "independent native SDK boundary signature"
            );
            assert!(
                sign(&seed, &[1u8; 93]).is_err(),
                "93-byte payload refused before upstream assertion"
            );
        }
    }
    #[test]
    fn npm_wasm_vectors() {
        let corpus = corpus();
        for name in ["base", "mutated-key", "mutated-payload", "mutated-nonce"] {
            assert!(
                corpus.cases.iter().any(|case| case.name == name),
                "missing zkLink oracle property {name}"
            );
        }
        for case in corpus.cases {
            let signature = sign(&decode(&case.seed), &decode(&case.payload)).unwrap();
            assert_eq!(
                signature.public_key.as_slice(),
                decode(&case.signed.signature.public_key),
                "{} public key",
                case.name
            );
            assert_eq!(
                signature.signature.as_slice(),
                decode(&case.signed.signature.signature),
                "{} signature",
                case.name
            );
        }
    }
    #[test]
    fn key_payload_nonce_controls() {
        let corpus = corpus();
        let base = &corpus.cases[0];
        let expected = decode(&base.signed.signature.signature);
        for case in &corpus.cases[1..] {
            assert_ne!(
                sign(&decode(&case.seed), &decode(&case.payload))
                    .unwrap()
                    .signature
                    .as_slice(),
                expected,
                "{} mutation was not observed",
                case.name
            );
        }
        assert_eq!(
            sign(&[0; 31], &[]).unwrap_err(),
            "zkLink seed must contain at least 32 bytes"
        );
        assert_eq!(
            sign(&[0; 32], &[0; 93]).unwrap_err(),
            "zkLink payload exceeds 92 bytes"
        );
    }
}
