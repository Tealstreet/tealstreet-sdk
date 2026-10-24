//! P4 setup-time public fetch: public-only, bounded, inline providers only,
//! and results held to exact widths and to the curve, whatever the provider.
#![cfg(feature = "dispatch-signing")]

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use safe_cex_engine::signing::{
    Algorithm, KeyRef, Operation, Provider, SignError, SignFuture, SignRequest, SignResult, Signer,
};
use safe_cex_venues::fetch_public;

/// Answers every operation with `value` and counts the calls it sees.
struct Fixed {
    immediate: bool,
    value: Vec<u8>,
    calls: AtomicUsize,
}
impl Signer for Fixed {
    fn immediate(&self) -> bool {
        self.immediate
    }
    fn sign_ready(&self, request: &SignRequest) -> Option<Result<SignResult, SignError>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.immediate.then(|| {
            Ok(SignResult::new(
                vec![self.value.clone(); request.operations.len()],
                request.receipt,
            ))
        })
    }
    fn sign(&self, _request: SignRequest) -> SignFuture {
        panic!("setup fetch never schedules provider work")
    }
}
fn provider(immediate: bool, value: Vec<u8>) -> (Arc<Fixed>, Provider) {
    let fixed = Arc::new(Fixed {
        immediate,
        value,
        calls: AtomicUsize::new(0),
    });
    (fixed.clone(), fixed)
}
fn key() -> KeyRef {
    KeyRef("primary.signing".into())
}
fn address() -> Operation {
    Operation::PublicAddress { key: key() }
}
fn public_key(algorithm: Algorithm) -> Operation {
    Operation::PublicKey {
        algorithm,
        key: key(),
    }
}
fn hex32(value: &str) -> Vec<u8> {
    let value = format!("{:0>64}", value.trim_start_matches("0x"));
    (0..64)
        .step_by(2)
        .map(|i| u8::from_str_radix(&value[i..i + 2], 16).unwrap())
        .collect()
}
/// Independent public keys: Node/OpenSSL Ed25519 and starknet.js.
fn vector(path: &str) -> Vec<u8> {
    let corpus: serde_json::Value = serde_json::from_str(path).unwrap();
    hex32(corpus["cases"][0]["publicKey"].as_str().unwrap())
}
fn ed25519_vector() -> Vec<u8> {
    vector(include_str!(
        "../../crypto/tests/fixtures/ed25519/node.json"
    ))
}
fn stark_vector() -> Vec<u8> {
    vector(include_str!(
        "../../crypto/tests/fixtures/starknet/sdk.json"
    ))
}

#[test]
fn inline_provider_answers_exact_valid_public_values() {
    let (_, signer) = provider(true, vec![1; 20]);
    assert_eq!(
        fetch_public(&signer, vec![address()]),
        Ok(vec![vec![1; 20]])
    );
    let mut canonical_y3 = vec![0; 32];
    canonical_y3[0] = 3;
    for (algorithm, value) in [
        (Algorithm::Ed25519, ed25519_vector()),
        (Algorithm::Ed25519, canonical_y3),
        (Algorithm::StarkDigest, stark_vector()),
    ] {
        let (_, signer) = provider(true, value.clone());
        assert_eq!(
            fetch_public(&signer, vec![public_key(algorithm)]),
            Ok(vec![value])
        );
    }
}

#[test]
fn malformed_provider_results_are_invalid() {
    let mut identity = vec![0; 32];
    identity[0] = 1;
    // y = p + 3: decodes under ZIP-215 to the same non-weak point as y = 3,
    // but is not the canonical encoding.
    let mut non_canonical_y3 = vec![0xff; 32];
    non_canonical_y3[0] = 0xf0;
    non_canonical_y3[31] = 0x7f;
    for (operation, value) in [
        (address(), vec![1; 32]),
        (public_key(Algorithm::Ed25519), vec![1; 31]),
        // The identity point decodes but has small order.
        (public_key(Algorithm::Ed25519), identity),
        (public_key(Algorithm::Ed25519), non_canonical_y3),
        (public_key(Algorithm::StarkDigest), vec![0; 32]),
        // At or above the field modulus: not a canonical felt.
        (public_key(Algorithm::StarkDigest), vec![0xff; 32]),
    ] {
        let (_, signer) = provider(true, value);
        assert_eq!(
            fetch_public(&signer, vec![operation.clone()]),
            Err(SignError::InvalidResult),
            "{operation:?}"
        );
    }
    // An x with no curve point; crypto's tests pin the predicate against
    // starknet-crypto's own refusal.
    let off_curve = (1u8..=64)
        .map(|x| hex32(&format!("{x:x}")))
        .find(|x| !safe_cex_crypto::starknet::is_public_key(x))
        .unwrap();
    let (_, signer) = provider(true, off_curve);
    assert_eq!(
        fetch_public(&signer, vec![public_key(Algorithm::StarkDigest)]),
        Err(SignError::InvalidResult)
    );
}

#[test]
fn host_or_remote_provider_is_unavailable_without_any_provider_call() {
    let (fixed, signer) = provider(false, vec![1; 20]);
    assert_eq!(
        fetch_public(&signer, vec![address()]),
        Err(SignError::Unavailable)
    );
    assert_eq!(fixed.calls.load(Ordering::SeqCst), 0);
}

#[test]
fn only_bounded_valid_public_only_requests_reach_any_provider() {
    for immediate in [true, false] {
        let (fixed, signer) = provider(immediate, vec![1; 20]);
        let present = Operation::Present {
            key: KeyRef("primary.credential".into()),
        };
        for operations in [
            vec![],
            vec![address(); 5],
            vec![address(), present],
            vec![public_key(Algorithm::Es256)],
            vec![Operation::PublicAddress {
                key: KeyRef(String::new()),
            }],
        ] {
            assert_eq!(
                fetch_public(&signer, operations),
                Err(SignError::InvalidRequest),
                "immediate={immediate}"
            );
        }
        assert_eq!(fixed.calls.load(Ordering::SeqCst), 0);
    }
}

/// P6b: the L1 wallet address through the real provider and its extra ref.
/// The `asterdex` feature is what enables the crypto secp256k1 arm here.
#[cfg(feature = "asterdex")]
#[test]
fn l1_wallet_address_comes_from_its_extra_ref() {
    use safe_cex_engine::signing::{CredentialSetupPolicy, ExtraKey};
    let wallet = "11".repeat(32);
    let policy: CredentialSetupPolicy = serde_json::from_value(serde_json::json!({
        "primary": {"extra": {"key": "l1Wallet", "field": "requiredForPrivate", "encoding": "hex"}}
    }))
    .unwrap();
    let setup = serde_json::json!({"primary": {
        "extra": {"l1Wallet": {"value": wallet, "encoding": "hex"}}
    }});
    let (signer, ids) =
        safe_cex_crypto::signer::LocalSigner::from_json_with_policy(setup.to_string(), policy)
            .unwrap();
    let role = ids.primary.unwrap().extra[&ExtraKey::L1Wallet].clone();
    let provider: Provider = Arc::new(signer);
    let values = fetch_public(&provider, vec![Operation::PublicAddress { key: role }]).unwrap();
    let expected = safe_cex_crypto::ethereum::private_key_address(&wallet).unwrap();
    let expected = &expected.to_ascii_lowercase()[2..];
    let actual: String = values[0].iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(actual, expected);
}
