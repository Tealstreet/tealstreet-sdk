//! Exact GRVT production strategy; no synthetic local-proof binding or venue I/O.
use super::*;
use safe_cex_crypto::admission::{
    Family,
    identity::Projection,
    wire::{Request, Response},
};
use std::time::Duration;

const SIGNER: &str = "0x7e5f4552091a69125d5dfcb7b8c2659029395bdf";
fn init() -> InitPayload {
    InitPayload {
        account_name: "acct".into(),
        exchange_name: "grvt".into(),
        key: "synthetic-grvt-api-key".into(),
        grvt_signer_address: Some(SIGNER.into()),
        grvt_trading_account_id: Some("42".into()),
        signing_mode: Some("cosigned".into()),
        instance_id: Some("grvt-proof-registration".into()),
        ..Default::default()
    }
}
fn registry() -> Arc<Registry> {
    let mut registry = crate::accounts::tests::registry();
    crate::accounts::testing::set_join_admission(Arc::get_mut(&mut registry).unwrap(), true);
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
        "GRVT must prove its scalar, never API-key login"
    );
    let request = Request::from_json(&frame["d"].to_string()).unwrap();
    assert_eq!(request.account_name(), "acct");
    assert_eq!(request.instance_id(), "grvt-proof-registration");
    request
}
fn response(request: &Request, scalar: u8) -> String {
    use safe_cex_crypto::signer::LocalSigner;
    use safe_cex_engine::signing::{KeyRef, Provider};
    let mut material = vec![0; 32];
    material[31] = scalar;
    let mut signer = LocalSigner::new();
    let key = KeyRef("fixture-grvt-scalar".into());
    signer
        .insert_declared(key.clone(), material, Family::Ethereum.algorithm())
        .unwrap();
    let provider: Provider = Arc::new(signer);
    let signature = request
        .statement()
        .sign(Family::Ethereum, &provider, &key, &[0; 32])
        .unwrap();
    let reply = Response::signed(request.id(), Family::Ethereum, &signature).unwrap();
    json!({"t":"sign-res","d":serde_json::to_value(reply).unwrap()}).to_string()
}
async fn complete(
    session: &mut Session,
    done: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
) {
    let completion = tokio::time::timeout(Duration::from_secs(5), done.recv())
        .await
        .unwrap()
        .unwrap();
    session.verification_completed(completion).await.unwrap();
}
#[tokio::test]
async fn grvt_correct_scalar_uses_production_strategy_before_attachment() {
    tokio::task::LocalSet::new().run_until(async {
        let registry=registry();
        let resident=crate::accounts::testing::trading_account(&registry,"alice",&init());
        let (mut session,mut sent)=user_session(registry.clone(),"alice");
        let mut done=session.take_verified();
        session.init(init()).await.unwrap();
        assert!(session.pending_verifications.contains_key("acct"),"GRVT must enter fresh possession proof");
        assert!(session.joined.is_empty());
        assert!(!registry.holds(&resident,session.socket));
        let request=challenge(&mut sent).await;
        let facts=json!({"venue":"grvt","network":"mainnet","credential":{"kind":"grvt","tradingAccount":"42","signer":SIGNER}});
        let digest=Projection::from_json("alice",&facts.to_string()).unwrap().digest();
        assert_eq!(serde_json::to_value(request.statement()).unwrap()["identityDigest"],json!(digest));
        session.handle(&response(&request,1)).await.unwrap();
        complete(&mut session,&mut done).await;
        assert!(Arc::ptr_eq(&session.joined["acct"].account,&resident));
        assert!(registry.holds(&resident,session.socket));
        session.close(); resident.handle.dispose();
    }).await;
}

fn unchanged(registry: &Registry, resident: &Arc<crate::accounts::Account>, session: &Session) {
    assert!(session.joined.is_empty());
    assert!(!registry.holds(resident, session.socket));
    assert!(resident.handle.disposal_cause().is_none());
    assert!(resident.delegated().is_none());
    assert_eq!(registry.account_count(), 1);
}
fn no_snapshot(sent: &mut crate::out::Receiver) {
    while let Ok(text) = sent.try_recv() {
        let frame: Value = serde_json::from_str(&text).unwrap();
        assert_ne!(
            frame["d"]["op"], "frame",
            "unverified socket receives no state"
        );
    }
}
#[tokio::test]
async fn grvt_different_valid_scalar_cannot_borrow_api_key_or_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let original = resident.link.clone().unwrap();
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let request = challenge(&mut sent).await;
            session.handle(&response(&request, 2)).await.unwrap();
            complete(&mut session, &mut done).await;
            unchanged(&registry, &resident, &session);
            assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original));
            no_snapshot(&mut sent);
            session.close();
            resident.handle.dispose();
        })
        .await;
}
#[tokio::test]
async fn grvt_second_scalar_is_valid_for_its_own_captured_signer() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut own = init();
            own.grvt_signer_address = Some(
                safe_cex_crypto::ethereum::private_key_address(&format!("{:064x}", 2)).unwrap(),
            );
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &own);
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(own).await.unwrap();
            let request = challenge(&mut sent).await;
            session.handle(&response(&request, 2)).await.unwrap();
            complete(&mut session, &mut done).await;
            assert!(registry.holds(&resident, session.socket));
            session.close();
            resident.handle.dispose();
        })
        .await;
}
#[tokio::test]
async fn grvt_replayed_signature_refuses_fresh_attempt_then_fresh_signature_passes() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let first = challenge(&mut sent).await;
            let mut replay: Value = serde_json::from_str(&response(&first, 1)).unwrap();
            session.init(init()).await.unwrap();
            let second = challenge(&mut sent).await;
            assert_ne!(first.statement(), second.statement());
            replay["d"]["id"] = second.id().into();
            session.handle(&replay.to_string()).await.unwrap();
            complete(&mut session, &mut done).await;
            unchanged(&registry, &resident, &session);
            no_snapshot(&mut sent);
            session.init(init()).await.unwrap();
            let fresh = challenge(&mut sent).await;
            session.handle(&response(&fresh, 1)).await.unwrap();
            complete(&mut session, &mut done).await;
            assert!(registry.holds(&resident, session.socket));
            session.close();
            resident.handle.dispose();
        })
        .await;
}
#[tokio::test]
async fn grvt_foreign_socket_and_consumed_reply_cannot_settle_proof() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let (mut foreign, _) = user_session(registry.clone(), "bob");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let request = challenge(&mut sent).await;
            let reply = response(&request, 1);
            foreign.handle(&reply).await.unwrap();
            tokio::task::yield_now().await;
            assert!(done.try_recv().is_err());
            unchanged(&registry, &resident, &session);
            let link = session.pending_verifications["acct"]
                .cosigned_link
                .clone()
                .unwrap();
            let data: Value = serde_json::from_str(&reply).unwrap();
            assert!(!link.resolve_join_proof(foreign.socket, &data["d"].to_string()));
            session.handle(&reply).await.unwrap();
            assert!(!link.resolve_join_proof(session.socket, &data["d"].to_string()));
            complete(&mut session, &mut done).await;
            assert!(registry.holds(&resident, session.socket));
            session.close();
            foreign.close();
            resident.handle.dispose();
        })
        .await;
}
#[tokio::test]
async fn grvt_missing_or_invalid_captured_facts_refuse_without_challenge() {
    for field in ["signer", "account", "zero", "bad-checksum", "empty-key"] {
        let registry = registry();
        let mut payload = init();
        match field {
            "signer" => payload.grvt_signer_address = None,
            "account" => payload.grvt_trading_account_id = None,
            "zero" => payload.grvt_signer_address = Some(format!("0x{}", "00".repeat(20))),
            "bad-checksum" => {
                payload.grvt_signer_address =
                    Some("0x7e5f4552091a69125d5dFCb7b8c2659029395bdf".into())
            }
            _ => payload.key = String::new(),
        }
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        let mut done = session.take_verified();
        session.init(payload).await.unwrap();
        if !session.pending_verifications.is_empty() {
            complete(&mut session, &mut done).await;
        }
        assert!(session.joined.is_empty(), "{field}");
        assert_eq!(registry.account_count(), 0);
        while let Ok(text) = sent.try_recv() {
            let frame: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(
                frame["t"], "sign-req",
                "{field}: missing facts cannot fall back to login or device metadata"
            );
            assert_ne!(frame["d"]["op"], "frame");
        }
        session.close();
    }
}
#[tokio::test]
async fn grvt_changing_captured_account_after_proof_cannot_replace_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for field in ["account", "signer", "api-key"] {
                let registry = registry();
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let original = resident.link.clone().unwrap();
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                let mut payload = init();
                let scalar = if field == "signer" { 2 } else { 1 };
                match field {
                    "account" => payload.grvt_trading_account_id = Some("43".into()),
                    "signer" => {
                        payload.grvt_signer_address = Some(
                            safe_cex_crypto::ethereum::private_key_address(&format!(
                                "{:064x}",
                                scalar
                            ))
                            .unwrap(),
                        )
                    }
                    _ => payload.key = "synthetic-other-api-key".into(),
                }
                session.init(payload).await.unwrap();
                let request = challenge(&mut sent).await;
                session.handle(&response(&request, scalar)).await.unwrap();
                complete(&mut session, &mut done).await;
                unchanged(&registry, &resident, &session);
                assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original));
                no_snapshot(&mut sent);
                session.close();
                resident.handle.dispose();
            }
        })
        .await;
}
#[tokio::test]
async fn grvt_owner_aba_cannot_publish_a_deferred_completion() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut old, mut old_sent) = user_session(registry.clone(), "alice");
            let mut old_done = old.take_verified();
            old.init(init()).await.unwrap();
            let first = challenge(&mut old_sent).await;
            old.handle(&response(&first, 1)).await.unwrap();
            let stale = tokio::time::timeout(Duration::from_secs(5), old_done.recv())
                .await
                .unwrap()
                .unwrap();
            old.close();
            let (bob, _) = user_session(registry.clone(), "bob");
            bob.close();
            let (mut successor, mut sent) = user_session(registry.clone(), "alice");
            let mut done = successor.take_verified();
            successor.init(init()).await.unwrap();
            let current = challenge(&mut sent).await;
            successor.verification_completed(stale).await.unwrap();
            unchanged(&registry, &resident, &successor);
            successor.handle(&response(&current, 1)).await.unwrap();
            complete(&mut successor, &mut done).await;
            assert!(registry.holds(&resident, successor.socket));
            successor.close();
            resident.handle.dispose();
        })
        .await;
}
#[tokio::test(start_paused = true)]
async fn grvt_original_deadline_fences_signing_and_completed_proof() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for after_verify in [false, true] {
                let registry = registry();
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init()).await.unwrap();
                let request = challenge(&mut sent).await;
                if after_verify {
                    session.handle(&response(&request, 1)).await.unwrap();
                    let completed = done.recv().await.unwrap();
                    tokio::time::advance(Duration::from_secs(36)).await;
                    session.verification_completed(completed).await.unwrap();
                } else {
                    tokio::time::advance(Duration::from_millis(15_001)).await;
                    session.handle(&response(&request, 1)).await.unwrap();
                    complete(&mut session, &mut done).await;
                }
                unchanged(&registry, &resident, &session);
                session.close();
                resident.handle.dispose();
            }
        })
        .await;
}

#[tokio::test]
async fn grvt_initial_unproven_connection_never_creates_an_actor_or_logs_in() {
    let registry = registry();
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    let mut done = session.take_verified();
    session.init(init()).await.unwrap();
    let request = challenge(&mut sent).await;
    assert_eq!(registry.account_count(), 0);
    assert!(session.joined.is_empty());
    session.handle(&response(&request, 2)).await.unwrap();
    complete(&mut session, &mut done).await;
    assert!(session.joined.is_empty());
    assert_eq!(registry.account_count(), 0);
    no_snapshot(&mut sent);
    session.close();
}

#[tokio::test]
async fn grvt_identity_digest_binds_owner_network_and_trading_account() {
    let registry = registry();
    let mut identities = Vec::new();
    for (owner, account, testnet) in [
        ("alice", "42", false),
        ("bob", "42", false),
        ("alice", "43", false),
        ("alice", "42", true),
    ] {
        let mut payload = init();
        payload.grvt_trading_account_id = Some(account.into());
        payload.testnet = Some(testnet);
        let (mut session, mut sent) = user_session(registry.clone(), owner);
        session.init(payload).await.unwrap();
        let request = challenge(&mut sent).await;
        let identity = serde_json::to_value(request.statement()).unwrap()["identityDigest"].clone();
        assert!(!identities.contains(&identity));
        identities.push(identity);
        assert_eq!(registry.account_count(), 0);
        session.close();
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn grvt_delegated_candidate_checks_its_own_scalar_without_borrowing_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for correct in [true, false] {
                let mut payload = init();
                payload.signing_mode = Some("delegated".into());
                let registry = registry();
                let existing: crate::delegated::Credentials = serde_json::from_str(
                    &json!({"key":payload.key,"secret":format!("{:064x}",1)}).to_string(),
                )
                .unwrap();
                let retained = crate::delegated::prepare_delegated(&payload, existing.pair())
                    .unwrap()
                    .resident;
                let retained_provider = retained.provider();
                let retained_ref = retained
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
                let offer = session.handovers.issue("acct", "grvt", now_ms()).unwrap();
                let credentials =
                    json!({"key":payload.key,"secret":format!("{:064x}",if correct {1} else {2})})
                        .to_string();
                payload.sealed = Some(crate::delegated::tests::seal(
                    &offer,
                    "grvt",
                    &payload.key,
                    credentials.as_bytes(),
                ));
                session.init(payload).await.unwrap();
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(completed.probe.as_ref().unwrap().permit.is_ok(), correct);
                session.verification_completed(completed).await.unwrap();
                assert_eq!(registry.holds(&resident, session.socket), correct);
                let public = safe_cex_crypto::admission::PublicKey::from_provider(
                    Family::Ethereum,
                    &retained_provider,
                    &retained_ref,
                )
                .unwrap();
                assert_eq!(
                    safe_cex_crypto::hex(public.as_bytes()),
                    &SIGNER[2..],
                    "retained scalar survives candidate cleanup"
                );
                assert!(resident.handle.disposal_cause().is_none());
                while let Ok(text) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(
                        frame["t"], "sign-req",
                        "DELEGATED never asks a device for the proof"
                    );
                    if !correct {
                        assert_ne!(frame["d"]["op"], "frame");
                    }
                }
                session.close();
                resident.handle.dispose();
            }
        })
        .await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn grvt_legacy_resident_key_mismatch_refuses_before_holding_the_actor() {
    tokio::task::LocalSet::new()
        .run_until(async {
            // Model a pre-local-proof DELEGATED actor: login could succeed with a
            // scalar different from the signer declared in the old init metadata.
            let registry = registry();
            let credentials: crate::delegated::Credentials = serde_json::from_str(
                &json!({"key":init().key,"secret":format!("{:064x}",2)}).to_string(),
            )
            .unwrap();
            let installed = crate::delegated::prepare_delegated(&init(), credentials.pair())
                .unwrap()
                .resident;
            let provider = installed.provider();
            let reference = installed
                .identities()
                .primary
                .as_ref()
                .unwrap()
                .signing_ref()
                .unwrap()
                .clone();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            resident.signing.set_test_resident(installed);
            let original_link = resident.link.clone().unwrap();
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let request = challenge(&mut sent).await;
            session.handle(&response(&request, 1)).await.unwrap();
            complete(&mut session, &mut done).await;
            assert!(
                session.joined.is_empty(),
                "metadata signer cannot prove a different installed scalar"
            );
            assert!(!registry.holds(&resident, session.socket));
            assert!(resident.delegated().is_some());
            assert!(resident.handle.disposal_cause().is_none());
            assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original_link));
            let actual = safe_cex_crypto::admission::PublicKey::from_provider(
                Family::Ethereum,
                &provider,
                &reference,
            )
            .unwrap();
            assert_ne!(safe_cex_crypto::hex(actual.as_bytes()), &SIGNER[2..]);
            no_snapshot(&mut sent);
            session.close();
            resident.handle.dispose();
        })
        .await;
}
