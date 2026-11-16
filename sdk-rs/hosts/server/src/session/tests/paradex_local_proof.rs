//! Paradex's production public facts select the existing one-use Stark challenge.
use super::*;
use safe_cex_crypto::admission::{
    Family,
    identity::Projection,
    wire::{Request, Response},
};
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{KeyRef, Provider};
use safe_cex_native::{HttpRoute, RoutingPlan, WsRoute, WsRoutes};
use std::time::Duration;

fn scalar(byte: u8) -> [u8; 32] {
    let mut scalar = [0; 32];
    scalar[31] = byte;
    scalar
}

fn init() -> InitPayload {
    InitPayload {
        account_name: "paradex-local".into(),
        exchange_name: "paradex".into(),
        key: "0x123".into(),
        paradex_address: Some("0x123".into()),
        subkey_address: Some(format!(
            "0x{}",
            safe_cex_crypto::hex(&safe_cex_crypto::starknet::public_key_bytes(&scalar(7)))
        )),
        signing_mode: Some("cosigned".into()),
        instance_id: Some("paradex-registration".into()),
        ..Default::default()
    }
}

fn registry() -> Arc<Registry> {
    // Even a broken proof strategy cannot reach an external venue from these tests.
    let http = HttpRoute {
        prefix: "in-memory://paradex-local-proof".into(),
        ..Default::default()
    };
    let ws = WsRoute {
        host: Some("in-memory://paradex-local-proof".into()),
        ..Default::default()
    };
    let mut registry = crate::accounts::testing::probe_registry(RoutingPlan {
        http: http.clone(),
        public_http: Some(http.clone()),
        read_http: Some(http.clone()),
        read_public_http: Some(http.clone()),
        public_read_http: Some(http),
        ws: WsRoutes {
            public: ws.clone(),
            private: ws.clone(),
            business: ws,
        },
        ..Default::default()
    });
    crate::accounts::testing::set_join_admission(Arc::get_mut(&mut registry).unwrap(), true);
    registry
}

fn provider(byte: u8) -> (Provider, KeyRef) {
    // Small synthetic scalars only; never exchange credentials.
    let (provider, identities) = LocalSigner::from_json_with_policy(
        json!({"primary":{"key":"synthetic", "secret":safe_cex_crypto::hex(&scalar(byte))}})
            .to_string(),
        serde_json::from_value(json!({"primary":{
            "secretEncoding":"hex", "signingAlgorithm":"starkDigest"
        }}))
        .unwrap(),
    )
    .unwrap();
    (
        Arc::new(provider),
        identities.primary.unwrap().signing_ref().unwrap().clone(),
    )
}

async fn challenge(sent: &mut crate::out::Receiver) -> Request {
    let text = tokio::time::timeout(Duration::from_secs(5), sent.recv())
        .await
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        frame["t"], "sign-req",
        "Paradex must issue a local challenge"
    );
    assert_eq!(frame["d"]["type"], "join-proof");
    Request::from_json(&frame["d"].to_string()).unwrap()
}

fn response(request: &Request, byte: u8) -> String {
    let (provider, reference) = provider(byte);
    let signature = request
        .statement()
        .sign(Family::Stark, &provider, &reference, &[9; 32])
        .unwrap();
    let reply = Response::signed(request.id(), Family::Stark, &signature).unwrap();
    json!({"t":"sign-res", "d":serde_json::to_value(reply).unwrap()}).to_string()
}

#[tokio::test]
async fn paradex_catalog_uses_captured_account_public_key_and_network() {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert!(crate::delegated::probe::cosigned_ready("paradex"));
            for testnet in [false, true] {
                let registry = registry();
                let mut init = init();
                init.testnet = Some(testnet);
                let resident = crate::accounts::testing::trading_account(&registry, "alice", &init);
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init.clone()).await.unwrap();
                assert!(session.joined.is_empty());
                assert!(!registry.holds(&resident, session.socket));
                let request = challenge(&mut sent).await;
                let facts = json!({
                    "venue":"paradex", "network":if testnet { "testnet" } else { "mainnet" },
                    "credential":{"kind":"stark", "account":format!("0x{:0>64}", "123"),
                        "publicKey":init.subkey_address}
                });
                let expected = Projection::from_json("alice", &facts.to_string()).unwrap();
                assert_eq!(request.account_name(), init.account_name);
                assert_eq!(request.instance_id(), init.instance_id.unwrap());
                assert_eq!(
                    serde_json::to_value(request.statement()).unwrap()["identityDigest"],
                    json!(expected.digest())
                );
                session.handle(&response(&request, 7)).await.unwrap();
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert!(session.joined.is_empty());
                session.verification_completed(completed).await.unwrap();
                assert!(Arc::ptr_eq(
                    &session.joined["paradex-local"].account,
                    &resident
                ));
                assert!(registry.holds(&resident, session.socket));
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn paradex_decimal_public_fields_complete_the_same_stark_device_proof() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let mut payload = init();
            payload.paradex_address = Some("291".into());
            payload.key = "0X0123".into();
            payload.subkey_address = Some(
                "874739451078007766457464989774322083649278607533249481151382481072868806602"
                    .into(),
            );
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &payload);
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(payload).await.unwrap();
            let request = challenge(&mut sent).await;
            assert!(session.joined.is_empty());
            session.handle(&response(&request, 1)).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                .await
                .unwrap()
                .unwrap();
            session.verification_completed(completed).await.unwrap();
            assert!(Arc::ptr_eq(
                &session.joined["paradex-local"].account,
                &resident
            ));
            assert!(registry.holds(&resident, session.socket));
            session.close();
        })
        .await;
}

#[tokio::test]
async fn paradex_different_valid_key_leaves_resident_and_signer_untouched() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let original_link = resident.link.clone().unwrap();
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let request = challenge(&mut sent).await;
            session.handle(&response(&request, 8)).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                .await
                .unwrap()
                .unwrap();
            session.verification_completed(completed).await.unwrap();
            assert!(session.joined.is_empty());
            assert!(!registry.holds(&resident, session.socket));
            assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original_link));
            assert_eq!(registry.account_count(), 1);
            while let Ok(text) = sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(frame["d"]["op"], "frame");
            }
            session.close();
        })
        .await;
}

#[tokio::test]
async fn paradex_foreign_socket_cannot_complete_the_joining_socket_proof() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for owner in ["alice", "bob"] {
                let registry = registry();
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let (mut original, mut sent) = user_session(registry.clone(), "alice");
                let (mut foreign, _) = user_session(registry.clone(), owner);
                let mut done = original.take_verified();
                original.init(init()).await.unwrap();
                let request = challenge(&mut sent).await;
                let reply = response(&request, 7);
                foreign.handle(&reply).await.unwrap();
                tokio::task::yield_now().await;
                assert!(done.try_recv().is_err());
                assert!(original.joined.is_empty() && foreign.joined.is_empty());
                original.handle(&reply).await.unwrap();
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                original.verification_completed(completed).await.unwrap();
                assert!(registry.holds(&resident, original.socket));
                assert!(!registry.holds(&resident, foreign.socket));
                original.close();
                foreign.close();
            }
        })
        .await;
}

#[tokio::test]
async fn paradex_previous_attempt_and_consumed_reply_are_not_reused() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut first, mut first_sent) = user_session(registry.clone(), "alice");
            let mut first_done = first.take_verified();
            first.init(init()).await.unwrap();
            let old = challenge(&mut first_sent).await;
            let old_reply = response(&old, 7);
            first.handle(&old_reply).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), first_done.recv())
                .await
                .unwrap()
                .unwrap();
            first.verification_completed(completed).await.unwrap();
            first.handle(&old_reply).await.unwrap();
            tokio::task::yield_now().await;
            assert!(
                first_done.try_recv().is_err(),
                "a consumed response creates no second completion"
            );
            assert!(registry.holds(&resident, first.socket));
            let original_link = resident.link.clone().unwrap();

            let (mut second, mut second_sent) = user_session(registry.clone(), "alice");
            let mut second_done = second.take_verified();
            second.init(init()).await.unwrap();
            let current = challenge(&mut second_sent).await;
            assert_ne!(old.id(), current.id());
            let mut replay: Value = serde_json::from_str(&old_reply).unwrap();
            replay["d"]["id"] = current.id().into();
            second.handle(&replay.to_string()).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), second_done.recv())
                .await
                .unwrap()
                .unwrap();
            second.verification_completed(completed).await.unwrap();
            assert!(second.joined.is_empty());
            assert!(!registry.holds(&resident, second.socket));
            assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original_link));

            while let Ok(text) = second_sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(frame["d"]["op"], "frame");
            }
            second.init(init()).await.unwrap();
            let fresh = challenge(&mut second_sent).await;
            second.handle(&response(&fresh, 7)).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), second_done.recv())
                .await
                .unwrap()
                .unwrap();
            second.verification_completed(completed).await.unwrap();
            assert!(Arc::ptr_eq(
                &second.joined["paradex-local"].account,
                &resident
            ));
            assert!(registry.holds(&resident, second.socket));
            first.close();
            second.close();
        })
        .await;
}

#[tokio::test]
async fn paradex_malformed_or_mismatched_public_facts_never_fall_back_to_private_startup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for change in [
                "missing-account",
                "wrong-account",
                "zero-account",
                "out-of-range-account",
                "missing-public",
                "zero-public",
                "out-of-range",
                "oversized-public",
                "invalid-hex",
                "whitespace",
            ] {
                let registry = registry();
                let mut init = init();
                match change {
                    "missing-account" => init.paradex_address = None,
                    "wrong-account" => init.paradex_address = Some("0x456".into()),
                    "zero-account" => {
                        init.key = "0x0".into();
                        init.paradex_address = Some("0x0".into());
                    }
                    "out-of-range-account" => {
                        init.key = format!("0x{}", "f".repeat(64));
                        init.paradex_address = Some(init.key.clone());
                    }
                    "missing-public" => init.subkey_address = None,
                    "zero-public" => init.subkey_address = Some("0x0".into()),
                    "out-of-range" => init.subkey_address = Some(format!("0x{}", "f".repeat(64))),
                    "oversized-public" => {
                        init.subkey_address = Some(format!("0x{}", "1".repeat(65)))
                    }
                    "invalid-hex" => init.subkey_address = Some("0xnot-hex".into()),
                    "whitespace" => init.subkey_address.as_mut().unwrap().push(' '),
                    _ => unreachable!(),
                }
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init).await.unwrap();
                if !session.pending_verifications.is_empty() {
                    let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                        .await
                        .unwrap()
                        .unwrap();
                    session.verification_completed(completed).await.unwrap();
                }
                let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
                assert_eq!(frame["t"], "msg");
                assert_eq!(frame["d"]["data"][1]["initRefused"], true);
                assert!(session.pending_verifications.is_empty());
                assert!(session.joined.is_empty());
                assert_eq!(registry.account_count(), 0);
                assert!(sent.try_recv().is_err());
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn paradex_sealed_handover_replay_into_a_new_attempt_preserves_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let mut payload = init();
            payload.signing_mode = Some("delegated".into());
            payload.key = payload.subkey_address.clone().unwrap();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &payload);
            let original_link = resident.link.clone().unwrap();
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let offer = session
                .handovers
                .issue(&payload.account_name, "paradex", now_ms())
                .unwrap();
            let credentials = json!({
                "key":payload.key, "secret":"",
                "signingKey":format!("0x{}", safe_cex_crypto::hex(&scalar(7)))
            })
            .to_string();
            let sealed = crate::delegated::tests::seal(
                &offer,
                "paradex",
                &payload.key,
                credentials.as_bytes(),
            );
            let opened = session
                .handovers
                .take(&payload.account_name, &sealed, now_ms())
                .unwrap()
                .open("paradex", &payload.key, &sealed, now_ms());
            assert!(opened.is_ok(), "the original sealed control must open");
            drop(opened);
            let next_offer = session
                .handovers
                .issue(&payload.account_name, "paradex", now_ms())
                .unwrap();
            assert_ne!(next_offer.attempt_id, sealed.attempt_id);
            payload.sealed = Some(sealed);
            session.init(payload).await.unwrap();
            assert!(session.joined.is_empty());
            assert!(session.pending_verifications.is_empty());
            assert!(!registry.holds(&resident, session.socket));
            assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original_link));
            assert_eq!(registry.account_count(), 1);
            let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
                .map(|text| serde_json::from_str(&text).unwrap())
                .collect();
            assert!(
                frames
                    .iter()
                    .any(|frame| frame["d"]["data"][1]["initRefused"] == true)
            );
            assert!(
                !frames
                    .iter()
                    .any(|frame| frame["t"] == "sign-req" || frame["d"]["op"] == "frame")
            );
            session.close();
        })
        .await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn paradex_delegated_candidate_matches_captured_public_key_without_device_signing() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for correct in [true, false] {
                let registry = registry();
                let mut payload = init();
                payload.signing_mode = Some("delegated".into());
                payload.key = payload.subkey_address.clone().unwrap();
                let credentials = |byte| {
                    json!({
                        "key":payload.key, "secret":"",
                        "signingKey":format!("0x{}", safe_cex_crypto::hex(&scalar(byte)))
                    })
                    .to_string()
                };
                let original: crate::delegated::Credentials =
                    serde_json::from_str(&credentials(7)).unwrap();
                let retained = crate::delegated::prepare_delegated(&payload, original.pair())
                    .unwrap()
                    .resident;
                let retained_provider = retained.provider();
                let retained_reference = retained
                    .identities()
                    .primary
                    .as_ref()
                    .unwrap()
                    .signing_ref()
                    .unwrap()
                    .clone();
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &payload);
                resident.signing.set_test_resident(retained);
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                let offer = session
                    .handovers
                    .issue(&payload.account_name, "paradex", now_ms())
                    .unwrap();
                payload.sealed = Some(crate::delegated::tests::seal(
                    &offer,
                    "paradex",
                    &payload.key,
                    credentials(if correct { 7 } else { 8 }).as_bytes(),
                ));
                session.init(payload).await.unwrap();
                assert!(session.joined.is_empty());
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(completed.probe.as_ref().unwrap().permit.is_ok(), correct);
                session.verification_completed(completed).await.unwrap();
                assert_eq!(registry.holds(&resident, session.socket), correct);
                let public = safe_cex_crypto::admission::PublicKey::new(
                    Family::Stark,
                    &safe_cex_crypto::starknet::public_key_bytes(&scalar(7)),
                )
                .unwrap();
                assert!(
                    public
                        .matches_provider(&retained_provider, &retained_reference)
                        .unwrap(),
                    "candidate cleanup must not erase the retained provider"
                );
                while let Ok(text) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(frame["t"], "sign-req");
                    if !correct {
                        assert_ne!(frame["d"]["op"], "frame");
                    }
                }
                session.close();
            }
        })
        .await;
}
