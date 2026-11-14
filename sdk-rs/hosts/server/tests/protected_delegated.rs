use safe_cex_engine::signing::*;
use safe_cex_exchange_server::delegated::{DelegatedPair, prepare_delegated};
use safe_cex_exchange_server::envelope::InitPayload;

fn resident(venue: &str, name: &str) -> safe_cex_exchange_server::delegated::PreparedDelegated {
    let init = InitPayload {
        account_name: name.into(),
        exchange_name: venue.into(),
        key: "synthetic-api-key".into(),
        ..Default::default()
    };
    prepare_delegated(
        &init,
        DelegatedPair::new(init.key.clone(), "synthetic-secret".into(), None),
    )
    .unwrap_or_else(|_| panic!("{venue}: protected delegated construction"))
}
fn request() -> SignRequest {
    SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key: KeyRef("primary.signing".into()),
            payload: vec![PayloadPart::Bytes {
                bytes: b"synthetic-canonical-request".to_vec(),
            }],
        }],
    }
}
#[test]
fn queued_original_account_cannot_sign_after_erase_or_adopt_replacement_account() {
    for venue in ["bybit", "binance"] {
        let original = resident(venue, "original-owner");
        let provider = original.resident.provider();
        let mut deferred = provider.sign(request());
        let replacement = resident(venue, "replacement-owner");
        original.resident.erase();
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert_eq!(
            deferred.as_mut().poll(&mut cx),
            std::task::Poll::Ready(Err(SignError::Unavailable))
        );
        assert_eq!(
            provider.sign_ready(&request()),
            Some(Err(SignError::Unavailable))
        );
        assert!(
            replacement
                .resident
                .provider()
                .sign_ready(&request())
                .unwrap()
                .is_ok()
        );
    }
}
#[test]
fn resident_clones_share_one_erasure_authority_but_engine_handles_do_not_retain_it() {
    let prepared = resident("binance", "same-owner");
    let shared = prepared.resident.clone();
    let engine = shared.provider();
    drop(prepared);
    assert!(engine.sign_ready(&request()).unwrap().is_ok());
    drop(shared);
    assert_eq!(
        engine.sign_ready(&request()),
        Some(Err(SignError::Unavailable))
    );
}
