#![cfg(feature = "txflow")]
use safe_cex_engine::signing::{SignFuture, SignRequest, Signer};
use safe_cex_engine::{Engine, ErrorKind, Outcome, Venue};
use safe_cex_venues::{EngineOptions, Visit, build, build_with_signer};
use serde_json::json;
use std::sync::Arc;

struct Mode;
impl Visit for Mode {
    type Output = bool;
    fn visit<V: Venue + 'static>(self, engine: Engine<V>) -> bool {
        engine.account_startup()
    }
}
struct NoIo;
impl Signer for NoIo {
    fn sign(&self, _: SignRequest) -> SignFuture {
        panic!("construction cannot ask the provider to sign");
    }
}
#[test]
fn explicit_private_payload_retains_refusal_before_unused_agent_cleanup() {
    for owner in [None, Some("0x1111111111111111111111111111111111111111")] {
        for remote in [false, true] {
            let value = json!({"key":"configured-key", "testnet":true,
                "applicationId":owner,"payloadSigner":true,"remoteSigning":remote,
                "publicMode":{"requested":false,"payloadSigner":true,"remoteSigner":remote}});
            for injected in [false, true] {
                let options: EngineOptions = serde_json::from_value(value.clone()).unwrap();
                let error = if injected {
                    build_with_signer("txflow", options, None, Some(Arc::new(NoIo)), Mode)
                } else {
                    build("txflow", options, None, Mode)
                }
                .expect_err("unsupported private PayloadSigner");
                assert_eq!(
                    error.kind,
                    Some(if injected {
                        ErrorKind::Unsupported
                    } else {
                        ErrorKind::InvalidInput
                    })
                );
                assert_eq!(error.outcome, Some(Outcome::NotSent));
                assert_eq!(
                    error.message,
                    if injected {
                        "Rust txflow has no REST-only PayloadSigner lifecycle"
                    } else {
                        "Delegated signing requires a configured callback provider"
                    }
                );
            }
        }
    }
}
#[test]
fn explicit_public_payload_still_discards_unused_agent_material() {
    let options = serde_json::from_value(json!({"testnet":true,
        "key":"configured-key","secret":"unused-invalid-scalar","payloadSigner":true,
        "publicMode":{"requested":true,"payloadSigner":true}}))
    .unwrap();
    assert!(
        !build_with_signer(
            "txflow",
            options,
            Some(safe_cex_engine::test_support::seeded_entropy(17)),
            Some(Arc::new(NoIo)),
            Mode
        )
        .unwrap()
    );
}
