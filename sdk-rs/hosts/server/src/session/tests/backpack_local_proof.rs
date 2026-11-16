//! Backpack readiness through the production binding, without injected proof facts.
use super::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_crypto::admission::{
    Family,
    wire::{Request, Response},
};
use std::time::Duration;

fn init() -> InitPayload {
    InitPayload {
        account_name: "backpack-local".into(),
        exchange_name: "backpack".into(),
        key: STANDARD.encode(safe_cex_crypto::ed25519::public_key(&[7; 32]).unwrap()),
        signing_mode: Some("cosigned".into()),
        instance_id: Some("backpack-registration".into()),
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
        "production Backpack must issue a local challenge"
    );
    assert_eq!(frame["d"]["type"], "join-proof");
    Request::from_json(&frame["d"].to_string()).unwrap()
}

fn response(request: &Request, seed: u8) -> String {
    let signature =
        safe_cex_crypto::ed25519::sign(&request.statement().message(Family::Ed25519), &[seed; 32])
            .unwrap();
    let reply = Response::signed(request.id(), Family::Ed25519, &signature).unwrap();
    json!({"t":"sign-res", "d":serde_json::to_value(reply).unwrap()}).to_string()
}

async fn backpack_catalog_selects_local_proof_before_attachment_using(
    sign: fn(&Request, u8) -> String,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            assert!(crate::delegated::probe::cosigned_ready("backpack"));
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            assert!(session.joined.is_empty());
            assert!(!registry.holds(&resident, session.socket));
            let request = challenge(&mut sent).await;
            assert_eq!(request.account_name(), init().account_name);
            assert_eq!(request.instance_id(), init().instance_id.unwrap());
            let facts = json!({"venue":"backpack", "network":"mainnet", "credential":{
                "kind":"ed25519", "publicKey":format!("0x{}", safe_cex_crypto::hex(
                    &safe_cex_crypto::ed25519::public_key(&[7; 32]).unwrap()))
            }});
            let expected = safe_cex_crypto::admission::identity::Projection::from_json(
                "alice",
                &facts.to_string(),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(request.statement()).unwrap()["identityDigest"],
                json!(expected.digest())
            );
            session.handle(&sign(&request, 7)).await.unwrap();
            let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(session.joined.is_empty());
            session.verification_completed(completed).await.unwrap();
            assert!(Arc::ptr_eq(
                &session.joined["backpack-local"].account,
                &resident
            ));
            assert!(registry.holds(&resident, session.socket));
            session.close();
        })
        .await;
}

async fn backpack_different_valid_key_keeps_resident_and_signer_untouched_using(
    sign: fn(&Request, u8) -> String,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let original_link = resident.link.clone().unwrap();
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let request = challenge(&mut sent).await;
            session.handle(&sign(&request, 8)).await.unwrap();
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

async fn backpack_foreign_socket_cannot_resolve_original_challenge_using(
    sign: fn(&Request, u8) -> String,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            for owner in ["alice", "bob"] {
                let registry = registry();
                let resident =
                    crate::accounts::testing::trading_account(&registry, "alice", &init());
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let (mut foreign, _) = user_session(registry.clone(), owner);
                let mut done = session.take_verified();
                session.init(init()).await.unwrap();
                let request = challenge(&mut sent).await;
                let reply = sign(&request, 7);
                foreign.handle(&reply).await.unwrap();
                tokio::task::yield_now().await;
                assert!(done.try_recv().is_err());
                assert!(session.joined.is_empty() && foreign.joined.is_empty());
                session.handle(&reply).await.unwrap();
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                session.verification_completed(completed).await.unwrap();
                assert!(registry.holds(&resident, session.socket));
                assert!(!registry.holds(&resident, foreign.socket));
                session.close();
                foreign.close();
            }
        })
        .await;
}

async fn backpack_prior_attempt_signature_refuses_then_fresh_attempt_succeeds_using(
    sign: fn(&Request, u8) -> String,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            let registry = registry();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &init());
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(init()).await.unwrap();
            let first = challenge(&mut sent).await;
            let mut old: Value = serde_json::from_str(&sign(&first, 7)).unwrap();
            session.init(init()).await.unwrap();
            let second = challenge(&mut sent).await;
            assert_ne!(first.statement(), second.statement());
            old["d"]["id"] = second.id().into();
            session.handle(&old.to_string()).await.unwrap();
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
            session.handle(&old.to_string()).await.unwrap();
            tokio::task::yield_now().await;
            assert!(
                done.try_recv().is_err(),
                "consumed response cannot settle a fresh wait"
            );
            session.handle(&sign(&third, 7)).await.unwrap();
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
async fn backpack_malformed_public_keys_never_fall_back_to_startup_reads() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for key in [
                "",
                "not-base64",
                "AA==",
                "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
                " 7E==",
            ] {
                let registry = registry();
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                let mut payload = init();
                payload.key = key.into();
                session.init(payload).await.unwrap();
                if !session.pending_verifications.is_empty() {
                    let completed = tokio::time::timeout(Duration::from_secs(5), done.recv())
                        .await
                        .unwrap()
                        .unwrap();
                    assert!(completed.probe.as_ref().unwrap().permit.is_err());
                    session.verification_completed(completed).await.unwrap();
                }
                assert!(session.pending_verifications.is_empty());
                assert!(session.joined.is_empty());
                assert_eq!(registry.account_count(), 0);
                let mut refused = false;
                while let Ok(text) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(frame["t"], "sign-req");
                    assert_ne!(frame["d"]["op"], "frame");
                    refused |= frame["d"]["data"][1]["initRefused"] == true;
                }
                assert!(refused, "the account must receive a terminal init refusal");
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn backpack_catalog_selects_local_proof_before_attachment() {
    backpack_catalog_selects_local_proof_before_attachment_using(response).await;
}

#[tokio::test]
async fn backpack_different_valid_key_keeps_resident_and_signer_untouched() {
    backpack_different_valid_key_keeps_resident_and_signer_untouched_using(response).await;
}

#[tokio::test]
async fn backpack_foreign_socket_cannot_resolve_original_challenge() {
    backpack_foreign_socket_cannot_resolve_original_challenge_using(response).await;
}

#[tokio::test]
async fn backpack_prior_attempt_signature_refuses_then_fresh_attempt_succeeds() {
    backpack_prior_attempt_signature_refuses_then_fresh_attempt_succeeds_using(response).await;
}

fn wasm_response(request: &Request, seed: u8) -> String {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    assert!(
        std::env::var_os("SAFE_CEX_BACKPACK_JOIN_WASM").is_some(),
        "set the absolute shipped WASM module path"
    );
    let script = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/backpack/join-proof-device.mjs"
    );
    let mut child = Command::new("node")
        .args([script, &seed.to_string()])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(request).unwrap())
        .unwrap();
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "real WASM fixture failed (exit {:?})",
        result.status.code()
    );
    String::from_utf8(result.stdout).unwrap()
}

#[tokio::test]
#[ignore = "requires Node22 and SAFE_CEX_BACKPACK_JOIN_WASM pointing to the built Backpack module"]
async fn backpack_actual_wasm_production_session_join_and_negatives() {
    backpack_catalog_selects_local_proof_before_attachment_using(wasm_response).await;
    backpack_different_valid_key_keeps_resident_and_signer_untouched_using(wasm_response).await;
    backpack_foreign_socket_cannot_resolve_original_challenge_using(wasm_response).await;
    backpack_prior_attempt_signature_refuses_then_fresh_attempt_succeeds_using(wasm_response).await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn backpack_delegated_correct_wrong_key_and_handover_replay_preserve_resident() {
    tokio::task::LocalSet::new().run_until(async {
        for mode in ["correct", "wrong-key", "replay"] {
            let mut payload = init();
            payload.signing_mode = Some("delegated".into());
            let registry = registry();
            let original: crate::delegated::Credentials = serde_json::from_str(&json!({
                "key":payload.key, "secret":STANDARD.encode([7; 32])
            }).to_string()).unwrap();
            let retained = crate::delegated::prepare_delegated(&payload, original.pair()).unwrap().resident;
            let provider = retained.provider();
            let reference = retained.identities().primary.as_ref().unwrap().signing_ref().unwrap().clone();
            let resident = crate::accounts::testing::trading_account(&registry, "alice", &payload);
            resident.signing.set_test_resident(retained);
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            let offer = session.handovers.issue(&payload.account_name, "backpack", now_ms()).unwrap();
            let candidate = json!({"key":payload.key, "secret":STANDARD.encode([if mode == "wrong-key" {8} else {7}; 32])}).to_string();
            payload.sealed = Some(crate::delegated::tests::seal(&offer, "backpack", &payload.key, candidate.as_bytes()));
            if mode == "replay" {
                session.handovers.issue(&payload.account_name, "backpack", now_ms()).unwrap();
            }
            session.init(payload).await.unwrap();
            if mode != "replay" {
                let completed = tokio::time::timeout(Duration::from_secs(5), done.recv()).await.unwrap().unwrap();
                assert_eq!(completed.probe.as_ref().unwrap().permit.is_ok(), mode == "correct");
                session.verification_completed(completed).await.unwrap();
            }
            assert_eq!(registry.holds(&resident, session.socket), mode == "correct");
            assert_eq!(session.joined.is_empty(), mode != "correct");
            assert_eq!(registry.account_count(), 1);
            let public = safe_cex_crypto::admission::PublicKey::new(
                Family::Ed25519, &safe_cex_crypto::ed25519::public_key(&[7; 32]).unwrap(),
            ).unwrap();
            assert!(public.matches_provider(&provider, &reference).unwrap());
            while let Ok(text) = sent.try_recv() {
                let frame: Value = serde_json::from_str(&text).unwrap();
                assert_ne!(frame["t"], "sign-req");
                if mode != "correct" { assert_ne!(frame["d"]["op"], "frame"); }
            }
            session.close();
        }
    }).await;
}
