//! Generic Session local-proof boundary; fixture facts do not activate a venue.
use super::*;
use safe_cex_crypto::admission::{
    Family,
    identity::Projection,
    wire::{Request, Response},
};
use std::time::Duration;

fn init() -> InitPayload {
    InitPayload {
        account_name: "acct".into(),
        exchange_name: "backpack".into(),
        key: safe_cex_crypto::hex(&safe_cex_crypto::ed25519::public_key(&[7; 32]).unwrap()),
        signing_mode: Some("cosigned".into()),
        instance_id: Some("local-proof-registration".into()),
        ..Default::default()
    }
}

fn facts() -> String {
    json!({"venue":"backpack", "network":"mainnet", "credential":{
        "kind":"ed25519", "publicKey":format!("0x{}", init().key)
    }})
    .to_string()
}

fn registry(bound: bool) -> Arc<Registry> {
    let mut registry = crate::accounts::tests::registry();
    let mutable = Arc::get_mut(&mut registry).unwrap();
    crate::accounts::testing::set_join_admission(mutable, true);
    if bound {
        crate::accounts::testing::local_proof_binding(mutable, "alice", &init(), &facts());
    }
    registry
}

async fn challenge(sent: &mut crate::out::Receiver) -> Request {
    let text = tokio::time::timeout(Duration::from_secs(5), sent.recv())
        .await
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        frame["t"], "sign-req",
        "a server-owned local binding must issue a challenge, never fall back to startup reads"
    );
    Request::from_json(&frame["d"].to_string()).unwrap()
}

fn response(request: &Request, seed: u8) -> String {
    let signature =
        safe_cex_crypto::ed25519::sign(&request.statement().message(Family::Ed25519), &[seed; 32])
            .unwrap();
    let response = Response::signed(request.id(), Family::Ed25519, &signature).unwrap();
    json!({"t":"sign-res", "d":serde_json::to_value(response).unwrap()}).to_string()
}

#[tokio::test]
async fn local_proof_correct_key_attaches_only_after_verification() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry(true);
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            assert!(session.joined.is_empty());
            assert!(registry.held("alice", "acct", session.socket).is_none());
            let request = challenge(&mut sent).await;
            assert_eq!(request.account_name(), "acct");
            assert_eq!(request.instance_id(), "local-proof-registration");
            assert_eq!(
                serde_json::to_value(request.statement()).unwrap()["identityDigest"],
                json!(Projection::from_json("alice", &facts()).unwrap().digest())
            );
            assert!(done.try_recv().is_err());
            session.handle(&response(&request, 7)).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(
                session.joined.is_empty(),
                "verification alone cannot publish state"
            );
            session.verification_completed(completed).await.unwrap();
            assert!(Arc::ptr_eq(&session.joined["acct"].account, &resident));
            assert!(registry.holds(&resident, session.socket));
            session.close();
        })
        .await;
}

#[tokio::test]
async fn local_proof_other_valid_key_cannot_touch_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry(true);
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
                assert_ne!(
                    frame["d"]["op"], "frame",
                    "refused account gets no snapshot"
                );
            }
            session.close();
        })
        .await;
}

#[tokio::test]
async fn local_proof_unactivated_venue_stays_unavailable_without_fixture_binding() {
    // SDK #3578 moves this unactivated-venue negative from Hyperliquid Spot to
    // TxFlow, which stays excluded while the M32 venue proofs activate.
    assert!(!crate::delegated::probe::cosigned_ready(&InitPayload {
        exchange_name: "txflow".into(),
        ..init()
    }));
    for owner in ["alice", "bob"] {
        let registry = registry(owner == "bob");
        let (mut session, mut sent) = user_session(registry.clone(), owner);
        let mut payload = init();
        payload.exchange_name = "txflow".into();
        // TxFlow's beta roster refuses wire init earlier; exercise the independent proof gate.
        session.admission_init(payload);
        let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(frame["d"]["data"][1]["kind"], "proof-unavailable");
        assert!(session.pending_verifications.is_empty());
        assert!(session.joined.is_empty());
        assert_eq!(registry.account_count(), 0);
        session.close();
    }
}

#[tokio::test]
async fn local_proof_txflow_wire_init_keeps_its_earlier_beta_roster_refusal() {
    assert!(!crate::delegated::probe::cosigned_ready("txflow"));
    for owner in ["alice", "bob"] {
        let registry = registry(owner == "bob");
        let (mut session, mut sent) = user_session(registry.clone(), owner);
        let mut payload = init();
        payload.exchange_name = "txflow".into();
        session.init(payload).await.unwrap();
        let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
        assert_eq!(frame["d"]["data"][1]["kind"], "unsupported");
        assert_eq!(frame["d"]["data"][1]["outcome"], "notSent");
        assert!(session.pending_verifications.is_empty());
        assert!(session.joined.is_empty());
        assert_eq!(registry.account_count(), 0);
        session.close();
    }
}

#[tokio::test]
async fn local_proof_foreign_session_cannot_settle_original_wait() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for foreign_owner in ["alice", "bob"] {
                let registry = registry(true);
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let (mut original, mut sent) = user_session(registry.clone(), "alice");
                let (mut foreign, _) = user_session(registry.clone(), foreign_owner);
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
async fn local_proof_replayed_signature_refuses_new_attempt_then_fresh_proof_works() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry(true);
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let first = challenge(&mut sent).await;
            let old: Value = serde_json::from_str(&response(&first, 7)).unwrap();
            session.init(init()).await.unwrap();
            let second = challenge(&mut sent).await;
            assert_ne!(first.statement(), second.statement());
            let mut replay = old.clone();
            replay["d"]["id"] = second.id().into();
            session.handle(&replay.to_string()).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                .await
                .unwrap()
                .unwrap();
            session.verification_completed(completed).await.unwrap();
            assert!(session.joined.is_empty());
            assert!(!registry.holds(&resident, session.socket));
            while sent.try_recv().is_ok() {}
            session.init(init()).await.unwrap();
            let third = challenge(&mut sent).await;
            session.handle(&response(&third, 7)).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                .await
                .unwrap()
                .unwrap();
            session.verification_completed(completed).await.unwrap();
            assert!(registry.holds(&resident, session.socket));
            session.close();
        })
        .await;
}

#[tokio::test]
async fn local_proof_raw_duplicate_and_mixed_outputs_refuse_before_attach() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for invalid in [
                "duplicate-version",
                "duplicate-id",
                "mixed",
                "refused",
                "unavailable",
                "short",
            ] {
                let registry = registry(true);
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init()).await.unwrap();
                let request = challenge(&mut sent).await;
                let valid = response(&request, 7);
                let mut value: Value = serde_json::from_str(&valid).unwrap();
                let malformed = match invalid {
                    "duplicate-version" => {
                        valid.replace("\"version\":1", "\"version\":1,\"version\":1")
                    }
                    "duplicate-id" => {
                        valid.replace("\"id\":", &format!("\"id\":{:?},\"id\":", request.id()))
                    }
                    "mixed" => {
                        value["d"]["wsAuthSignature"] = "synthetic".into();
                        value.to_string()
                    }
                    "refused" | "unavailable" => {
                        let reason = if invalid == "refused" {
                            "Join proof refused"
                        } else {
                            "Join proof unavailable"
                        };
                        json!({"t":"sign-res", "d":{"id":request.id(), "error":reason}}).to_string()
                    }
                    _ => {
                        value["d"]["joinProof"]["signature"] = "AA==".into();
                        value.to_string()
                    }
                };
                assert_ne!(malformed, valid);
                session.handle(&malformed).await.unwrap();
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                session.verification_completed(completed).await.unwrap();
                assert!(session.joined.is_empty(), "{invalid}");
                assert!(!registry.holds(&resident, session.socket));
                assert_eq!(registry.account_count(), 1);
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn local_proof_completed_success_cannot_survive_replacement_or_expiry() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for retire in ["replacement", "expired", "dispose"] {
                let registry = registry(true);
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init()).await.unwrap();
                let request = challenge(&mut sent).await;
                session.handle(&response(&request, 7)).await.unwrap();
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                match retire {
                    "replacement" => {
                        session.init(init()).await.unwrap();
                    }
                    "expired" => session.identity.token.as_mut().unwrap().1 = now_ms(),
                    _ => {
                        session
                            .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
                            .await
                            .unwrap();
                    }
                }
                session.verification_completed(completed).await.unwrap();
                assert!(session.joined.is_empty(), "{retire}");
                assert!(!registry.holds(&resident, session.socket));
                assert_eq!(registry.account_count(), 1);
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn local_proof_old_owner_completion_cannot_publish_through_successor_session() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for successor_owner in ["bob", "alice"] {
                let mut registry = registry(true);
                crate::accounts::testing::local_proof_binding(
                    Arc::get_mut(&mut registry).unwrap(),
                    "bob",
                    &init(),
                    &facts(),
                );
                let alice = crate::accounts::testing::trading_account(&registry, "alice", &init());
                let bob = crate::accounts::testing::trading_account(&registry, "bob", &init());
                let (mut retired, mut retired_sent) = user_session(registry.clone(), "alice");
                let mut old_done = retired.take_verified();
                retired.init(init()).await.unwrap();
                let old_request = challenge(&mut retired_sent).await;
                retired.handle(&response(&old_request, 7)).await.unwrap();
                let old_completion = tokio::time::timeout(Duration::from_secs(5), old_done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                retired.close();
                let (intervening_b, _) = user_session(registry.clone(), "bob");
                intervening_b.close();
                let (mut successor, mut sent) = user_session(registry.clone(), successor_owner);
                let mut done = successor.take_verified();
                successor.init(init()).await.unwrap();
                let current_request = challenge(&mut sent).await;
                successor
                    .verification_completed(old_completion)
                    .await
                    .unwrap();
                assert!(successor.joined.is_empty());
                assert!(!registry.holds(&alice, successor.socket));
                assert!(!registry.holds(&bob, successor.socket));
                successor
                    .handle(&response(&current_request, 7))
                    .await
                    .unwrap();
                let completion = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                successor.verification_completed(completion).await.unwrap();
                let expected = if successor_owner == "alice" {
                    &alice
                } else {
                    &bob
                };
                assert!(Arc::ptr_eq(&successor.joined["acct"].account, expected));
                successor.close();
            }
        })
        .await;
}

#[tokio::test]
async fn local_proof_queue_is_shared_across_labels_and_cancel_releases_next_waiter() {
    let registry = registry(true);
    let mut sessions = Vec::new();
    let mut first = None;
    for index in 0..6 {
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        let mut done = session.take_verified();
        let mut payload = init();
        payload.account_name = format!("pending-{index}");
        payload.instance_id = Some(format!("registration-{index}"));
        session.init(payload).await.unwrap();
        if index == 0 {
            first = Some(challenge(&mut sent).await);
        }
        tokio::task::yield_now().await;
        if index == 5 {
            let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                .await
                .unwrap()
                .unwrap();
            session.verification_completed(completed).await.unwrap();
            assert!(session.joined.is_empty());
            let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
            assert_eq!(frame["d"]["data"][1]["transient"], true);
            session.close();
        } else {
            sessions.push((session, sent));
        }
    }
    let first = first.unwrap();
    for (_, sent) in sessions.iter_mut().skip(1) {
        assert!(
            sent.try_recv().is_err(),
            "only the active reservation can issue a challenge"
        );
    }
    let (first_session, _) = sessions.remove(0);
    first_session.close();
    let next = challenge(&mut sessions[0].1).await;
    assert_ne!(first.statement(), next.statement());
    assert_eq!(
        registry.account_count(),
        0,
        "unproven connections create no accounts"
    );
    for (session, _) in sessions {
        session.close();
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn local_proof_delegated_candidate_uses_temporary_provider_and_preserves_resident() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    tokio::task::LocalSet::new().run_until(async {
        for correct in [true, false] {
            let mut payload = init();
            payload.signing_mode = Some("delegated".into());
            let mut registry = registry(false);
            crate::accounts::testing::local_proof_binding(Arc::get_mut(&mut registry).unwrap(), "alice", &payload, &facts());
            let existing: crate::delegated::Credentials = serde_json::from_str(&json!({"key":payload.key, "secret":STANDARD.encode([7;32])}).to_string()).unwrap();
            let retained = crate::delegated::prepare_delegated(&payload, existing.pair()).unwrap().resident;
            let retained_provider = retained.provider();
            let retained_ref = retained.identities().primary.as_ref().unwrap().signing_ref().unwrap().clone();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &payload);
            resident.signing.set_test_resident(retained);
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            let offer = session.handovers.issue("acct", "backpack", now_ms()).unwrap();
            let credentials = json!({"key":payload.key, "secret":STANDARD.encode([if correct {7} else {8};32])}).to_string();
            payload.sealed = Some(crate::delegated::tests::seal(&offer, "backpack", &payload.key, credentials.as_bytes()));
            session.init(payload).await.unwrap();
            assert!(session.joined.is_empty());
            let completed = tokio::time::timeout(Duration::from_secs(5), done.recv()).await.unwrap().unwrap();
            assert_eq!(completed.probe.as_ref().unwrap().permit.is_ok(), correct);
            session.verification_completed(completed).await.unwrap();
            assert_eq!(registry.holds(&resident, session.socket), correct);
            let projection = Projection::from_json("alice", &facts()).unwrap();
            assert!(projection.public_key().matches_provider(&retained_provider, &retained_ref).unwrap(), "the retained key must remain usable after candidate cleanup");
            while let Ok(text) = sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(frame["t"], "sign-req", "DELEGATED never asks the device to sign");
                if !correct { assert_ne!(frame["d"]["op"], "frame"); }
            }
            session.close();
        }
    }).await;
}

#[tokio::test(start_paused = true)]
async fn local_proof_signing_and_final_attachment_keep_the_original_deadlines() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for attach_after_verification in [false, true] {
                let registry = registry(true);
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init()).await.unwrap();
                let request = challenge(&mut sent).await;
                if attach_after_verification {
                    session.handle(&response(&request, 7)).await.unwrap();
                    let completed = done.recv().await.unwrap();
                    tokio::time::advance(Duration::from_secs(36)).await;
                    session.verification_completed(completed).await.unwrap();
                } else {
                    tokio::time::advance(Duration::from_millis(15_001)).await;
                    session.handle(&response(&request, 7)).await.unwrap();
                    let completed = done.recv().await.unwrap();
                    session.verification_completed(completed).await.unwrap();
                }
                assert!(session.joined.is_empty());
                assert!(!registry.holds(&resident, session.socket));
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn local_proof_captured_token_lease_and_drain_retire_deferred_success() {
    use crate::router_lease::{Assertion, Config, Heartbeat};
    tokio::task::LocalSet::new()
        .run_until(async {
            for guard in ["token", "lease", "drain"] {
                let registry = Arc::new(
                    Arc::try_unwrap(registry(true))
                        .ok()
                        .unwrap()
                        .with_router_lease(Config {
                            box_id: Some("01".into()),
                            enforce: true,
                            ..Default::default()
                        }),
                );
                let heartbeat = |epoch| Heartbeat {
                    r#box: "01".into(),
                    issued_at: now_ms(),
                    users: vec![Assertion {
                        user_id: "alice".into(),
                        owner_epoch: epoch,
                        sequence: 1,
                        not_after: now_ms() + 30_000,
                    }],
                };
                registry.router_leases.accept(&heartbeat(1)).unwrap();
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                if guard == "token" {
                    session.identity.token.as_mut().unwrap().1 = now_ms() + 100;
                }
                session.init(init()).await.unwrap();
                let request = challenge(&mut sent).await;
                if guard == "token" {
                    session.identity.token.as_mut().unwrap().1 = u64::MAX;
                    tokio::time::sleep(Duration::from_millis(125)).await;
                }
                session.handle(&response(&request, 7)).await.unwrap();
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                match guard {
                    "lease" => {
                        registry.router_leases.accept(&heartbeat(2)).unwrap();
                    }
                    "drain" => {
                        let _ = registry.break_glass(Duration::ZERO).await;
                    }
                    _ => {}
                }
                let result = session.verification_completed(completed).await;
                if guard == "drain" {
                    assert!(matches!(result, Err(Close::Moved)));
                } else {
                    result.unwrap();
                }
                assert!(session.joined.is_empty(), "{guard}");
                assert!(!registry.holds(&resident, session.socket));
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn local_proof_device_without_handler_refuses_as_unavailable() {
    let registry = registry(true);
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    let mut done = session.take_verified();
    session.init(init()).await.unwrap();
    let request = challenge(&mut sent).await;
    let response = Response::refused(
        request.id(),
        safe_cex_crypto::admission::wire::Refusal::Unavailable,
    )
    .unwrap();
    session
        .handle(&json!({"t":"sign-res", "d":serde_json::to_value(response).unwrap()}).to_string())
        .await
        .unwrap();
    let completion = tokio::time::timeout(Duration::from_secs(5), done.recv())
        .await
        .unwrap()
        .unwrap();
    session.verification_completed(completion).await.unwrap();
    let frame: Value = serde_json::from_str(&sent.try_recv().unwrap()).unwrap();
    assert_eq!(frame["d"]["data"][1]["kind"], "proof-unavailable");
    assert!(session.joined.is_empty());
    assert_eq!(registry.account_count(), 0);
    session.close();
}

async fn retired_link_after_response(reattach: bool) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry(true);
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let request = challenge(&mut sent).await;
            let link = session.pending_verifications["acct"]
                .cosigned_link
                .clone()
                .unwrap();
            session.handle(&response(&request, 7)).await.unwrap();
            if reattach {
                link.attach(
                    session.socket + 100,
                    Arc::new(|_| panic!("a consumed local proof must never replay")),
                );
            } else {
                link.mark_unavailable();
            }
            let completion = tokio::time::timeout(Duration::from_secs(5), done.recv())
                .await
                .unwrap()
                .unwrap();
            session.verification_completed(completion).await.unwrap();
            assert!(
                session.joined.is_empty(),
                "a reply delivered before link retirement cannot grant later admission"
            );
            assert!(!registry.holds(&resident, session.socket));
            session.close();
        })
        .await;
}

#[tokio::test]
async fn local_proof_link_retirement_after_delivery_refuses_deferred_verification() {
    retired_link_after_response(false).await;
}

#[tokio::test]
async fn local_proof_link_replacement_after_delivery_refuses_deferred_verification() {
    retired_link_after_response(true).await;
}
