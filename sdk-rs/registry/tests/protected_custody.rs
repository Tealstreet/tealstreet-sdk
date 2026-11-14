#![cfg(all(feature = "dispatch-signing", not(target_arch = "wasm32")))]
use safe_cex_engine::signing::*;
use safe_cex_venues::{
    EngineOptions, prepare_local_signer_for, prepare_protected_local_signer_for,
};
use serde_json::json;
use std::task::{Context, Poll, Waker};

fn options() -> EngineOptions {
    serde_json::from_value(json!({"key":"synthetic-public", "secret":"synthetic-private", "passphrase":"synthetic-passphrase"})).unwrap()
}
fn work(operations: Vec<Operation>) -> SignRequest {
    SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations,
    }
}
fn hmac_custody(venue: &str) {
    let (plain, ids) = prepare_local_signer_for(venue, &mut options()).unwrap();
    let id = ids.primary.unwrap();
    let signing = id.signing_ref().unwrap().clone();
    let present = work(vec![Operation::Present {
        key: signing.clone(),
    }]);
    // Legacy LOCAL policy is unchanged; the stricter custody boundary is opt-in.
    assert!(plain.sign_ready(&present).unwrap().is_ok());
    let public = work(vec![Operation::Present {
        key: id.credential.clone(),
    }]);
    let expected_public = plain.sign_ready(&public).unwrap().unwrap();
    let signature = work(vec![Operation::Sign {
        algorithm: Algorithm::HmacSha256,
        key: signing.clone(),
        payload: vec![PayloadPart::Bytes {
            bytes: b"synthetic-payload".to_vec(),
        }],
    }]);
    let expected_signature = plain.sign_ready(&signature).unwrap().unwrap();
    let owner = plain.protect().unwrap();
    let provider = owner.provider();
    let mut escaped = provider.sign_ready(&present).unwrap();
    let rejected = escaped.is_err();
    owner.erase();
    // Keep any returned copy live across erasure. Do not print its contents.
    if let Ok(result) = &mut escaped {
        result.values.clear();
    }
    assert!(
        rejected,
        "{venue}: protected signing material cannot be presented"
    );

    let (owner, ids) = prepare_protected_local_signer_for(venue, &mut options()).unwrap();
    let provider = owner.provider();
    let id = ids.primary.unwrap();
    for operation in [
        Operation::Sha256 {
            payload: vec![PayloadPart::Credential {
                key: signing.clone(),
            }],
        },
        Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key: signing,
            payload: vec![PayloadPart::Credential {
                key: id.signing_ref().unwrap().clone(),
            }],
        },
    ] {
        assert!(
            provider
                .sign_ready(&work(vec![operation]))
                .unwrap()
                .is_err(),
            "{venue}: private-slot interpolation refused"
        );
    }
    let result = provider.sign_ready(&public).unwrap().unwrap();
    assert_eq!(result, expected_public);
    assert_eq!(
        provider.sign_ready(&signature).unwrap().unwrap(),
        expected_signature
    );
    if let Some(key) = id.passphrase {
        assert!(
            provider
                .sign_ready(&work(vec![Operation::Present { key }]))
                .unwrap()
                .is_ok()
        );
    }
    let mut deferred = provider.sign(present);
    owner.erase();
    assert_eq!(
        provider.sign_ready(&public),
        Some(Err(SignError::Unavailable))
    );
    assert_eq!(
        deferred
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Err(SignError::Unavailable))
    );
    assert_eq!(
        result, expected_public,
        "only permitted output survives erasure"
    );
}
macro_rules! hmac_family {
    ($name:ident, $feature:literal, $venue:literal) => {
        #[cfg(feature=$feature)]
        #[test]
        fn $name() {
            hmac_custody($venue);
        }
    };
}
hmac_family!(binance, "binance", "binance");
hmac_family!(bybit, "bybit", "bybit");
hmac_family!(bitget, "bitget", "bitget");
hmac_family!(bingx, "bingx", "bingx");
hmac_family!(bitrue, "bitrue", "bitrue");
hmac_family!(bitunix, "bitunix", "bitunix");
hmac_family!(blofin, "blofin", "blofin");
hmac_family!(cryptocom, "cryptocom", "cryptocom");
hmac_family!(kucoin, "kucoin", "kucoin");
hmac_family!(kucoinspot, "kucoinspot", "kucoinspot");
hmac_family!(mexc, "mexc", "mexc");
hmac_family!(okx, "okx", "okx");
hmac_family!(phemex, "phemex", "phemex");
hmac_family!(weex, "weex", "weex");
hmac_family!(woo, "woo", "woo");

#[cfg(feature = "paradex")]
#[path = "../../venues/paradex/tests/support/mod.rs"]
mod support;

#[cfg(feature = "paradex")]
mod paradex {
    use super::*;

    fn options() -> EngineOptions {
        serde_json::from_value(
            json!({"key":support::STARK_KEY, "paradexAddress":"0x0123", "subkeyAddress":"0x456"}),
        )
        .unwrap()
    }
    fn setup(provider: &Provider, ids: &Identities) -> Box<dyn OwnedSetupExecution> {
        assert!(
            provider.incarnation().is_none(),
            "protected wrapper must not revive a seal"
        );
        let id = ids.primary.as_ref().unwrap();
        assert!(
            id.public_key_sealed_by(Algorithm::StarkDigest, provider.incarnation())
                .is_err()
        );
        provider
            .create_public_address_setup(work(vec![Operation::PublicKey {
                algorithm: Algorithm::StarkDigest,
                key: id.signing_ref().unwrap().clone(),
            }]))
            .expect("protected Paradex needs an owned public-key setup path")
    }
    #[test]
    fn owned_public_key_setup_is_guarded_at_start_and_delivery_and_account_bound() {
        let (plain, ids) = prepare_local_signer_for("paradex", &mut options()).unwrap();
        let expected = ids
            .primary
            .as_ref()
            .unwrap()
            .public_key_sealed_by(Algorithm::StarkDigest, plain.incarnation())
            .unwrap();
        let owner = plain.protect().unwrap();
        let provider = owner.provider();
        let mut dormant = setup(&provider, &ids);
        let mut started = setup(&provider, &ids);
        started.start().unwrap();
        let (replacement, replacement_ids) =
            prepare_protected_local_signer_for("paradex", &mut options()).unwrap();
        let mut replacement_work = setup(&replacement.provider(), &replacement_ids);
        owner.erase();
        let mut cx = Context::from_waker(Waker::noop());
        assert_eq!(dormant.start(), Err(SignError::Unavailable));
        for execution in [&mut dormant, &mut started] {
            assert_eq!(
                execution.poll_result(&mut cx),
                Poll::Ready(Err(SignError::Unavailable))
            );
            assert_eq!(execution.poll_joined(&mut cx), Poll::Ready(Ok(())));
        }
        replacement_work.start().unwrap();
        let Poll::Ready(Ok(result)) = replacement_work.poll_result(&mut cx) else {
            panic!("replacement must remain usable")
        };
        assert_eq!(result.values, vec![expected.to_vec()]);
        assert_eq!(replacement_work.poll_joined(&mut cx), Poll::Ready(Ok(())));
    }
    #[test]
    fn protected_paradex_private_startup_reaches_signed_auth() {
        use safe_cex_engine::{SocketId, test_support::seeded_engine};
        use safe_cex_venue_paradex::{Options, Paradex};
        let (owner, ids) = prepare_protected_local_signer_for("paradex", &mut options()).unwrap();
        let mut e = seeded_engine(Paradex::new(Options {
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            credentials: ids.primary.clone(),
            ..Default::default()
        }));
        e.set_signer(Some(owner.provider())).unwrap();
        e.set_identities(ids).unwrap();
        e.initialize_address_nonce(1_700_000_000_000.).unwrap();
        e.poll_address_setup(1_700_000_000_000., &mut Context::from_waker(Waker::noop()));
        assert!(e.address_nonce_setup_ready());
        e.set_frame_fed().unwrap();
        e.start(1_700_000_000_000.);
        let mut h = support::Harness {
            e,
            pending: vec![],
            actions: vec![],
            failures: vec![],
            receipt_trace: vec![],
            public: SocketId(0),
            now: 1_700_000_000_000.,
        };
        h.collect();
        h.reply("/markets", support::MARKETS);
        h.reply(
            "/system/config",
            r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
        );
        assert!(
            h.pending.iter().any(|r| r.url.contains("/auth/")),
            "private startup must construct credentials and sign authentication"
        );
        h.reply("/auth/", r#"{"jwt_token":"synthetic-jwt"}"#);
        assert!(h.failures.is_empty());
        owner.erase();
    }
}
