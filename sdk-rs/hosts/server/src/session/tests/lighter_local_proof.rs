//! Lighter registration is venue-owned; no test injects a local-proof projection.
use super::*;
use safe_cex_crypto::admission::{
    Family, PublicKey,
    identity::Projection,
    wire::{Request, Response},
};
use safe_cex_crypto::signer::LocalSigner;
use std::time::Duration;

type Observed = Arc<Mutex<crate::delegated_e2e_io::Observed>>;

const VENUES: [&str; 2] = ["lighter", "lighter-robinhood"];
fn scalar(value: u8) -> [u8; 40] {
    let mut bytes = [0; 40];
    bytes[0] = value;
    bytes
}
fn public(value: u8) -> PublicKey {
    PublicKey::new(
        Family::Schnorr,
        &safe_cex_crypto::lighter::public_key(&scalar(value)).unwrap(),
    )
    .unwrap()
}
fn init(venue: &str, testnet: bool) -> InitPayload {
    InitPayload {
        account_name: "lighter-local".into(),
        exchange_name: venue.into(),
        key: "synthetic-public-account".into(),
        application_id: Some("42".into()),
        api_key_index: Some("1".into()),
        testnet: Some(testnet),
        signing_mode: Some("cosigned".into()),
        instance_id: Some("lighter-registration".into()),
        ..Default::default()
    }
}
fn body() -> Value {
    json!({"code":200,"api_keys":[{"account_index":42,"api_key_index":1,
        "public_key":safe_cex_crypto::hex(public(7).as_bytes())}]})
}
fn registry(body: Value, permits: usize) -> (Arc<Registry>, Arc<tokio::sync::Semaphore>, Observed) {
    let gate = Arc::new(tokio::sync::Semaphore::new(permits));
    let io = crate::delegated_e2e_io::MockVenue::registered_key(body, gate.clone());
    let observed = io.observed.clone();
    let mut registry = crate::accounts::testing::e2e_registry("alice", io);
    crate::accounts::testing::set_join_admission(Arc::get_mut(&mut registry).unwrap(), true);
    (registry, gate, observed)
}
async fn challenge(sent: &mut crate::out::Receiver) -> Request {
    let text = tokio::time::timeout(Duration::from_secs(5), sent.recv())
        .await
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        frame["t"], "sign-req",
        "Lighter must issue a local possession challenge"
    );
    assert_eq!(frame["d"]["type"], "join-proof");
    Request::from_json(&frame["d"].to_string()).unwrap()
}
fn response(request: &Request, value: u8) -> String {
    let (signer, identities) = LocalSigner::from_json_with_policy(
        json!({"primary":{"key":"synthetic-public-account","secret":safe_cex_crypto::hex(&scalar(value))}}).to_string(),
        serde_json::from_value(json!({"primary":{"secretEncoding":"hex","signingAlgorithm":"schnorrDigest"}})).unwrap(),
    ).unwrap();
    let provider: safe_cex_engine::signing::Provider = Arc::new(signer);
    let key = identities.primary.as_ref().unwrap().signing_ref().unwrap();
    let signature = request
        .statement()
        .sign(Family::Schnorr, &provider, key, &[29; 32])
        .unwrap();
    let reply = Response::signed(request.id(), Family::Schnorr, &signature).unwrap();
    json!({"t":"sign-res","d":serde_json::to_value(reply).unwrap()}).to_string()
}
async fn finish(
    session: &mut Session,
    done: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
) {
    let result = tokio::time::timeout(Duration::from_secs(5), done.recv())
        .await
        .unwrap()
        .unwrap();
    session.verification_completed(result).await.unwrap();
}

async fn lighter_registered_key_proof_attaches_exact_venue_and_network_using(
    sign: fn(&Request, u8) -> String,
) {
    tokio::task::LocalSet::new().run_until(async {
        for venue in VENUES { for testnet in [false, true] {
            assert!(crate::delegated::probe::cosigned_ready(venue));
            let payload = init(venue, testnet);
            let (registry, _, observed) = registry(body(), 8);
            let resident = crate::accounts::testing::registered_lighter_account(&registry, "alice", &payload, &public(7));
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            let mut done = session.take_verified();
            session.init(payload.clone()).await.unwrap();
            let request = challenge(&mut sent).await;
            assert!(session.joined.is_empty());
            assert!(!registry.holds(&resident, session.socket));
            let facts = json!({"venue":venue,"network":if testnet {"testnet"} else {"mainnet"},"credential":{
                "kind":"schnorr","accountIndex":"42","apiKeyIndex":1,
                "registeredPublicKey":format!("0x{}",safe_cex_crypto::hex(public(7).as_bytes()))}});
            let expected = Projection::from_json("alice", &facts.to_string()).unwrap();
            assert_eq!(serde_json::to_value(request.statement()).unwrap()["identityDigest"], json!(expected.digest()));
            assert_eq!(request.account_name(), payload.account_name);
            assert_eq!(request.instance_id(), payload.instance_id.unwrap());
            session.handle(&sign(&request, 7)).await.unwrap();
            finish(&mut session, &mut done).await;
            assert!(Arc::ptr_eq(&session.joined["lighter-local"].account, &resident));
            assert!(registry.holds(&resident, session.socket));
            let io = observed.lock().unwrap();
            assert_eq!(io.reads, 1);
            assert_eq!(io.sockets, 0);
            assert!(io.writes.is_empty());
            let origin = safe_cex_venues::lighter_registration::Network::from_options(
                venue == "lighter-robinhood", testnet,
            ).origin();
            assert_eq!(io.registration_urls, [format!("{origin}/api/v1/apikeys?account_index=42&api_key_index=1")]);
            drop(io);
            session.close();
        }}
    }).await;
}

async fn lighter_other_valid_key_preserves_resident_and_original_signer_using(
    sign: fn(&Request, u8) -> String,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let payload = init(venue, false);
                let (registry, _, observed) = registry(body(), 8);
                let resident = crate::accounts::testing::registered_lighter_account(
                    &registry,
                    "alice",
                    &payload,
                    &public(7),
                );
                let original = resident.link.clone().unwrap();
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(payload).await.unwrap();
                let request = challenge(&mut sent).await;
                session.handle(&sign(&request, 8)).await.unwrap();
                finish(&mut session, &mut done).await;
                assert!(session.joined.is_empty());
                assert!(!registry.holds(&resident, session.socket));
                assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original));
                assert_eq!(registry.account_count(), 1);
                assert!(observed.lock().unwrap().writes.is_empty());
                session.close();
            }
        })
        .await;
}

async fn lighter_replay_refuses_new_attempt_then_fresh_signature_succeeds_using(
    sign: fn(&Request, u8) -> String,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let payload = init(venue, false);
                let (registry, _, _) = registry(body(), 8);
                let resident = crate::accounts::testing::registered_lighter_account(
                    &registry,
                    "alice",
                    &payload,
                    &public(7),
                );
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(payload.clone()).await.unwrap();
                let first = challenge(&mut sent).await;
                let mut replay: Value = serde_json::from_str(&sign(&first, 7)).unwrap();
                session.init(payload.clone()).await.unwrap();
                let second = challenge(&mut sent).await;
                assert_ne!(first.statement(), second.statement());
                replay["d"]["id"] = second.id().into();
                session.handle(&replay.to_string()).await.unwrap();
                finish(&mut session, &mut done).await;
                assert!(session.joined.is_empty());
                assert!(!registry.holds(&resident, session.socket));
                while sent.try_recv().is_ok() {}
                session.init(payload).await.unwrap();
                let third = challenge(&mut sent).await;
                let fresh = sign(&third, 7);
                session.handle(&fresh).await.unwrap();
                finish(&mut session, &mut done).await;
                assert!(registry.holds(&resident, session.socket));
                session.handle(&fresh).await.unwrap();
                tokio::task::yield_now().await;
                assert!(
                    done.try_recv().is_err(),
                    "a consumed reply cannot create another admission"
                );
                assert!(session.pending_verifications.is_empty());
                assert_eq!(registry.account_count(), 1);
                session.close();
            }
        })
        .await;
}

async fn lighter_foreign_socket_cannot_consume_original_proof_using(
    sign: fn(&Request, u8) -> String,
) {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let payload = init(venue, false);
                let (registry, _, _) = registry(body(), 8);
                let resident = crate::accounts::testing::registered_lighter_account(
                    &registry,
                    "alice",
                    &payload,
                    &public(7),
                );
                let (mut original, mut sent) = user_session(registry.clone(), "alice");
                let (mut foreign, _) = user_session(registry.clone(), "alice");
                let mut done = original.take_verified();
                original.init(payload).await.unwrap();
                let request = challenge(&mut sent).await;
                let reply = sign(&request, 7);
                foreign.handle(&reply).await.unwrap();
                tokio::task::yield_now().await;
                assert!(done.try_recv().is_err());
                original.handle(&reply).await.unwrap();
                finish(&mut original, &mut done).await;
                assert!(registry.holds(&resident, original.socket));
                assert!(!registry.holds(&resident, foreign.socket));
                original.close();
                foreign.close();
            }
        })
        .await;
}

#[tokio::test]
async fn lighter_invalid_registration_refuses_without_startup_fallback() {
    tokio::task::LocalSet::new().run_until(async {
        for venue in VENUES { for invalid in [json!({"code":200,"api_keys":[]}), json!({"code":200,"api_keys":[{
            "account_index":43,"api_key_index":1,"public_key":safe_cex_crypto::hex(public(7).as_bytes())}]}),
            json!({"code":200,"api_keys":[{"account_index":42,"api_key_index":1,"public_key":"00"}]})] {
            let (registry, _, observed) = registry(invalid,8);
            let (mut session,mut sent) = user_session(registry.clone(),"alice");
            let mut done=session.take_verified();
            session.init(init(venue,false)).await.unwrap();
            if !session.pending_verifications.is_empty() { finish(&mut session,&mut done).await; }
            assert!(session.joined.is_empty()); assert_eq!(registry.account_count(),0);
            while let Ok(text)=sent.try_recv() { let frame: Value=serde_json::from_str(&text).unwrap(); assert_ne!(frame["t"],"sign-req"); }
            let io=observed.lock().unwrap(); assert_eq!(io.sockets,0); assert!(io.writes.is_empty()); drop(io);
            session.close();
        }}
    }).await;
}

#[tokio::test]
async fn lighter_cold_correct_key_verifies_before_any_account_startup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let (registry, _, observed) = registry(body(), 8);
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init(venue, false)).await.unwrap();
                let request = challenge(&mut sent).await;
                session.handle(&response(&request, 7)).await.unwrap();
                let result = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert!(result.probe.as_ref().unwrap().permit.is_ok());
                assert!(session.joined.is_empty());
                assert_eq!(registry.account_count(), 0);
                let io = observed.lock().unwrap();
                assert_eq!(io.reads, 1);
                assert_eq!(io.sockets, 0);
                assert!(io.writes.is_empty());
                drop(io);
                drop(result);
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn lighter_cold_wrong_key_never_starts_an_account() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let (registry, _, observed) = registry(body(), 8);
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init(venue, false)).await.unwrap();
                let request = challenge(&mut sent).await;
                session.handle(&response(&request, 8)).await.unwrap();
                finish(&mut session, &mut done).await;
                assert!(session.joined.is_empty());
                assert_eq!(registry.account_count(), 0);
                let io = observed.lock().unwrap();
                assert_eq!(io.reads, 1);
                assert_eq!(io.sockets, 0);
                assert!(io.writes.is_empty());
                drop(io);
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn lighter_rotated_registration_cannot_attach_to_the_old_resident() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let payload = init(venue, false);
                let mut changed = body();
                changed["api_keys"][0]["public_key"] =
                    safe_cex_crypto::hex(public(8).as_bytes()).into();
                let (registry, _, _) = registry(changed, 8);
                let resident = crate::accounts::testing::registered_lighter_account(
                    &registry,
                    "alice",
                    &payload,
                    &public(7),
                );
                let original = resident.link.clone().unwrap();
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(payload).await.unwrap();
                // SDK #3576 captures the actor pin before lookup; rotation refuses before signing.
                finish(&mut session, &mut done).await;
                while let Ok(text) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(frame["t"], "sign-req");
                }
                assert!(session.joined.is_empty());
                assert!(!registry.holds(&resident, session.socket));
                assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original));
                assert_eq!(registry.account_count(), 1);
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn lighter_registration_rotation_between_proof_and_cold_build_refuses_before_startup() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let state = Arc::new(Mutex::new(body()));
                let io = crate::delegated_e2e_io::MockVenue::registered_key_state(
                    state.clone(),
                    Arc::new(tokio::sync::Semaphore::new(8)),
                );
                let observed = io.observed.clone();
                let mut registry = crate::accounts::testing::e2e_registry("alice", io);
                crate::accounts::testing::set_join_admission(
                    Arc::get_mut(&mut registry).unwrap(),
                    true,
                );
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(init(venue, false)).await.unwrap();
                let request = challenge(&mut sent).await;
                session.handle(&response(&request, 7)).await.unwrap();
                let result = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert!(result.probe.as_ref().unwrap().permit.is_ok());
                state.lock().unwrap()["api_keys"][0]["public_key"] =
                    safe_cex_crypto::hex(public(8).as_bytes()).into();
                session.verification_completed(result).await.unwrap();
                assert!(session.joined.is_empty());
                assert_eq!(registry.account_count(), 0);
                let io = observed.lock().unwrap();
                assert_eq!(io.reads, 2);
                assert_eq!(io.sockets, 0);
                assert!(io.writes.is_empty());
                drop(io);
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn lighter_retired_session_cannot_publish_a_delayed_registration() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                assert!(crate::delegated::probe::cosigned_ready(venue));
                let (registry, gate, observed) = registry(body(), 0);
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                session.init(init(venue, false)).await.unwrap();
                tokio::time::timeout(Duration::from_secs(5), async {
                    while observed.lock().unwrap().preparations == 0 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                assert!(session.joined.is_empty());
                session.close();
                gate.add_permits(1);
                for _ in 0..10 {
                    tokio::task::yield_now().await;
                }
                assert_eq!(registry.account_count(), 0);
                assert_eq!(observed.lock().unwrap().requests, 0);
                while let Ok(text) = sent.try_recv() {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(frame["t"], "sign-req");
                }
            }
        })
        .await;
}

#[tokio::test]
async fn lighter_invalid_slot_facts_refuse_before_public_io() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                for (account, index) in [
                    ("-1", "1"),
                    ("42", "255"),
                    ("42", "+1"),
                    ("281474976710656", "1"),
                ] {
                    let (registry, _, observed) = registry(body(), 8);
                    let (mut session, mut sent) = user_session(registry.clone(), "alice");
                    let mut done = session.take_verified();
                    let mut payload = init(venue, false);
                    payload.application_id = Some(account.into());
                    payload.api_key_index = Some(index.into());
                    session.init(payload).await.unwrap();
                    if !session.pending_verifications.is_empty() {
                        finish(&mut session, &mut done).await;
                    }
                    assert!(session.joined.is_empty());
                    assert_eq!(registry.account_count(), 0);
                    assert_eq!(observed.lock().unwrap().preparations, 0);
                    while let Ok(text) = sent.try_recv() {
                        let frame: Value = serde_json::from_str(&text).unwrap();
                        assert_ne!(frame["t"], "sign-req");
                    }
                    session.close();
                }
            }
        })
        .await;
}

#[tokio::test]
async fn lighter_slot_queue_is_shared_across_untrusted_key_labels() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                assert!(crate::delegated::probe::cosigned_ready(venue));
                let (registry, gate, observed) = registry(body(), 0);
                let (mut first, mut first_sent) = user_session(registry.clone(), "alice");
                let (mut second, mut second_sent) = user_session(registry.clone(), "alice");
                let mut second_done = second.take_verified();
                first.init(init(venue, false)).await.unwrap();
                tokio::time::timeout(Duration::from_secs(5), async {
                    while observed.lock().unwrap().preparations == 0 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                let mut renamed = init(venue, false);
                renamed.account_name = "another-label".into();
                renamed.key = "another-public-label".into();
                second.init(renamed).await.unwrap();
                for _ in 0..10 {
                    tokio::task::yield_now().await;
                }
                assert_eq!(
                    observed.lock().unwrap().preparations,
                    1,
                    "labels cannot create a second active proof for one venue slot"
                );
                first.close();
                gate.add_permits(2);
                let request = challenge(&mut second_sent).await;
                second.handle(&response(&request, 7)).await.unwrap();
                let result = tokio::time::timeout(Duration::from_secs(5), second_done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert!(result.probe.as_ref().unwrap().permit.is_ok());
                assert_eq!(observed.lock().unwrap().preparations, 2);
                assert!(second.joined.is_empty());
                assert_eq!(registry.account_count(), 0);
                while let Ok(text) = first_sent.try_recv() {
                    let frame: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(frame["t"], "sign-req");
                }
                drop(result);
                second.close();
            }
        })
        .await;
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn lighter_delegated_pair_and_sealed_replay_preserve_original_provider() {
    tokio::task::LocalSet::new().run_until(async {
        for venue in VENUES {for mode in ["correct","wrong-key","replay"] {
            let mut payload=init(venue,false); payload.signing_mode=Some("delegated".into());
            let (registry,_,observed)=registry(body(),8);
            let original:crate::delegated::Credentials=serde_json::from_str(&json!({
                "key":payload.key,"secret":safe_cex_crypto::hex(&scalar(7))}).to_string()).unwrap();
            let retained=crate::delegated::prepare_delegated(&payload,original.pair()).unwrap().resident;
            let provider=retained.provider();
            let reference=retained.identities().primary.as_ref().unwrap().signing_ref().unwrap().clone();
            let resident=crate::accounts::testing::registered_lighter_account(&registry,"alice",&payload,&public(7));
            resident.signing.set_test_resident(retained);
            let (mut session,mut sent)=user_session(registry.clone(),"alice"); let mut done=session.take_verified();
            let offer=session.handovers.issue(&payload.account_name,venue,now_ms()).unwrap();
            let candidate=json!({"key":payload.key,"secret":safe_cex_crypto::hex(&scalar(if mode=="wrong-key" {8} else {7}))}).to_string();
            payload.sealed=Some(crate::delegated::tests::seal(&offer,venue,&payload.key,candidate.as_bytes()));
            if mode=="replay" {session.handovers.issue(&payload.account_name,venue,now_ms()).unwrap();}
            session.init(payload).await.unwrap();
            if mode!="replay" {
                let result=tokio::time::timeout(Duration::from_secs(5),done.recv()).await.unwrap().unwrap();
                assert_eq!(result.probe.as_ref().unwrap().permit.is_ok(),mode=="correct");
                session.verification_completed(result).await.unwrap();
            }
            assert_eq!(registry.holds(&resident,session.socket),mode=="correct");
            assert_eq!(session.joined.is_empty(),mode!="correct"); assert_eq!(registry.account_count(),1);
            assert!(public(7).matches_provider(&provider,&reference).unwrap());
            assert!(observed.lock().unwrap().writes.is_empty());
            while let Ok(text)=sent.try_recv() {
                let frame:Value=serde_json::from_str(&text).unwrap(); assert_ne!(frame["t"],"sign-req");
                if mode!="correct" {assert_ne!(frame["d"]["op"],"frame");}
            }
            session.close();
        }}
    }).await;
}

#[tokio::test]
async fn lighter_registered_key_proof_attaches_exact_venue_and_network() {
    lighter_registered_key_proof_attaches_exact_venue_and_network_using(response).await;
}

#[tokio::test]
async fn lighter_other_valid_key_preserves_resident_and_original_signer() {
    lighter_other_valid_key_preserves_resident_and_original_signer_using(response).await;
}

#[tokio::test]
async fn lighter_replay_refuses_new_attempt_then_fresh_signature_succeeds() {
    lighter_replay_refuses_new_attempt_then_fresh_signature_succeeds_using(response).await;
}

#[tokio::test]
async fn lighter_foreign_socket_cannot_consume_original_proof() {
    lighter_foreign_socket_cannot_consume_original_proof_using(response).await;
}

fn wasm_response(request: &Request, seed: u8) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let value = serde_json::to_value(request).unwrap();
    let venue = value["exchangeName"].as_str().unwrap();
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/lighter/join-proof-device.mjs"
        ))
        .arg(seed.to_string())
        .arg(venue)
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
        "real WASM device failed (exit {:?})",
        result.status.code()
    );
    String::from_utf8(result.stdout).unwrap()
}

#[tokio::test]
#[ignore = "requires Node22 and SAFE_CEX_LIGHTER_JOIN_WASM pointing to a fresh activated Lighter module"]
async fn lighter_actual_wasm_session_proof_and_negatives() {
    lighter_registered_key_proof_attaches_exact_venue_and_network_using(wasm_response).await;
    lighter_other_valid_key_preserves_resident_and_original_signer_using(wasm_response).await;
    lighter_replay_refuses_new_attempt_then_fresh_signature_succeeds_using(wasm_response).await;
    lighter_foreign_socket_cannot_consume_original_proof_using(wasm_response).await;
}

#[tokio::test]
async fn lighter_actor_aba_during_registration_cannot_transfer_pending_authority() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let payload = init(venue, false);
                let (registry, gate, observed) = registry(body(), 0);
                let original = crate::accounts::testing::registered_lighter_account(
                    &registry,
                    "alice",
                    &payload,
                    &public(7),
                );
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(payload.clone()).await.unwrap();
                tokio::time::timeout(Duration::from_secs(5), async {
                    while observed.lock().unwrap().preparations == 0 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
                original.handle.dispose();
                let successor = crate::accounts::testing::registered_lighter_account(
                    &registry,
                    "alice",
                    &payload,
                    &public(7),
                );
                assert!(!Arc::ptr_eq(&original, &successor));
                gate.add_permits(1);
                tokio::time::timeout(Duration::from_secs(5), async {
                    tokio::select! {
                        result=done.recv() => {
                            let result=result.unwrap();
                            assert!(result.probe.as_ref().unwrap().permit.is_err());
                            session.verification_completed(result).await.unwrap();
                        },
                        text=sent.recv() => {
                            let frame:Value=serde_json::from_str(&text.unwrap()).unwrap();
                            assert_eq!(frame["t"],"sign-req");
                            let request=Request::from_json(&frame["d"].to_string()).unwrap();
                            session.handle(&response(&request,7)).await.unwrap();
                            finish(&mut session,&mut done).await;
                        },
                    }
                })
                .await
                .unwrap();
                assert!(
                    session.joined.is_empty(),
                    "old registration authority cannot attach a replacement actor"
                );
                assert!(!registry.holds(&successor, session.socket));
                assert_eq!(
                    observed.lock().unwrap().requests,
                    0,
                    "retired actor cannot dispatch held registration IO"
                );
                session.close();
            }
        })
        .await;
}

#[tokio::test]
async fn lighter_actor_aba_after_proof_cannot_reuse_a_completed_receipt() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in VENUES {
                let payload = init(venue, false);
                let (registry, _, _) = registry(body(), 8);
                let original = crate::accounts::testing::registered_lighter_account(
                    &registry,
                    "alice",
                    &payload,
                    &public(7),
                );
                let (mut session, mut sent) = user_session(registry.clone(), "alice");
                let mut done = session.take_verified();
                session.init(payload.clone()).await.unwrap();
                let request = challenge(&mut sent).await;
                session.handle(&response(&request, 7)).await.unwrap();
                let result = tokio::time::timeout(Duration::from_secs(5), done.recv())
                    .await
                    .unwrap()
                    .unwrap();
                assert!(result.probe.as_ref().unwrap().permit.is_ok());
                original.handle.dispose();
                let successor = crate::accounts::testing::registered_lighter_account(
                    &registry,
                    "alice",
                    &payload,
                    &public(7),
                );
                session.verification_completed(result).await.unwrap();
                assert!(
                    session.joined.is_empty(),
                    "a completed receipt belongs to its captured actor incarnation"
                );
                assert!(!registry.holds(&successor, session.socket));
                assert_eq!(registry.account_count(), 1);
                session.close();
            }
        })
        .await;
}
