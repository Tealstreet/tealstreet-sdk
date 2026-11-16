//! Both CDP algorithms prove the claimed opaque key through native venue startup.
use super::*;
use crate::delegated::probe::coinbase_tests::{CoinbaseIo, key, secret};
use crate::delegated_e2e_io::MockVenue;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use safe_cex_venues::{EngineOptions, hosted_signing, local_signer_from_capsule};
use std::time::Duration;

fn init(venue: &str, algorithm: &str, seed: u8) -> InitPayload {
    serde_json::from_value(json!({"accountName":"acct","exchangeName":venue,"key":key(algorithm,seed),
        "signingMode":"cosigned","coinbaseJwtAlgorithm":algorithm,"instanceId":"coinbase-registration"})).unwrap()
}
fn registry(venue: &'static str) -> (Arc<Registry>, CoinbaseIo) {
    let io = CoinbaseIo::new(venue);
    (
        crate::accounts::testing::e2e_registry("alice", MockVenue::for_coinbase(io.clone())),
        io,
    )
}

#[derive(Default)]
struct Device {
    prior: HashMap<String, String>,
    wrong_key: bool,
    replay: bool,
    replayed: usize,
    signed: usize,
}
impl Device {
    fn reply(&mut self, venue: &str, algorithm: &str, seed: u8, frame: &Value) -> String {
        assert_eq!(
            frame["t"], "sign-req",
            "a real native private read must request its server-built JWS"
        );
        assert_eq!(frame["d"]["type"], "ws-auth");
        assert_eq!(frame["d"]["wsAuth"]["outputFormat"], "coinbase-jws");
        let input = frame["d"]["wsAuth"]["toSign"].as_str().unwrap();
        let (header, claims) = input.split_once('.').unwrap();
        let header: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(header).unwrap()).unwrap();
        let claims: Value =
            serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).unwrap()).unwrap();
        assert_eq!(header["kid"], key(algorithm, seed));
        assert_eq!(claims["sub"], header["kid"]);
        assert_eq!(
            claims["exp"].as_u64(),
            claims["nbf"].as_u64().map(|nbf| nbf + 120)
        );
        assert!(!header["nonce"].as_str().unwrap().is_empty());
        let op = format!("{}:{}:{}", header["alg"], header["kid"], claims["uri"]);
        let signature = if self.replay && self.prior.contains_key(&op) {
            self.replayed += 1;
            self.prior[&op].clone()
        } else {
            let scalar = if self.wrong_key {
                if seed == 7 { 8 } else { 7 }
            } else {
                seed
            };
            let (provider, ids) = local_signer_from_capsule(
                venue,
                json!({"primary":{"key":key(algorithm,seed),"secret":secret(algorithm,scalar)}})
                    .to_string(),
                &mut serde_json::from_str::<EngineOptions>("{}").unwrap(),
            )
            .unwrap();
            let (work, kind) = hosted_signing::prepare(
                venue,
                &ids,
                hosted_signing::Input::WsAuth {
                    to_sign: input.into(),
                    output_format: hosted_signing::OutputFormat::CoinbaseJws,
                },
            )
            .unwrap();
            let result = provider.sign_ready(&work).unwrap().unwrap();
            let signature = hosted_signing::finish(kind, &result)
                .unwrap()
                .signature
                .unwrap();
            assert_eq!(URL_SAFE_NO_PAD.decode(&signature).unwrap().len(), 64);
            self.prior.insert(op, signature.clone());
            signature
        };
        self.signed += 1;
        json!({"t":"sign-res","d":{"id":frame["d"]["id"],"wsAuthSignature":signature}}).to_string()
    }
}

async fn first(sent: &mut crate::out::Receiver) -> Value {
    let text = tokio::time::timeout(Duration::from_secs(8), sent.recv())
        .await
        .unwrap()
        .unwrap();
    let frame: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        frame["t"], "sign-req",
        "native JWT join must be available, not silently attached or refused"
    );
    frame
}

async fn drive(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    done: &mut tokio::sync::mpsc::UnboundedReceiver<JoinCompletion>,
    device: &mut Device,
    venue: &str,
    algorithm: &str,
    seed: u8,
) -> JoinCompletion {
    tokio::time::timeout(Duration::from_secs(8), async {
        loop {
            tokio::select! {
                complete=done.recv()=>return complete.unwrap(),
                text=sent.recv()=>{
                    let frame:Value=serde_json::from_str(&text.unwrap()).unwrap();
                    assert_eq!(frame["t"],"sign-req","unproved join produced a non-signing frame");
                    session.handle(&device.reply(venue,algorithm,seed,&frame)).await.unwrap();
                }
            }
        }
    })
    .await
    .unwrap()
}

async fn venue_cases(venue: &'static str, algorithm: &str) {
    for seed in [7, 8] {
        let (registry, io) = registry(venue);
        let resident = crate::accounts::testing::trading_account(
            &registry,
            "alice",
            &init(venue, algorithm, seed),
        );
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        let mut done = session.take_verified();
        let mut device = Device::default();
        session.init(init(venue, algorithm, seed)).await.unwrap();
        assert!(session.joined.is_empty());
        assert!(!registry.holds(&resident, session.socket));
        let completed = drive(
            &mut session,
            &mut sent,
            &mut done,
            &mut device,
            venue,
            algorithm,
            seed,
        )
        .await;
        assert!(completed.probe.as_ref().unwrap().permit.is_ok());
        assert!(session.joined.is_empty());
        session.verification_completed(completed).await.unwrap();
        assert!(Arc::ptr_eq(&session.joined["acct"].account, &resident));
        assert!(registry.holds(&resident, session.socket));
        let seen = io.seen.lock().unwrap();
        assert!(seen.signed_reads >= 2);
        assert!(seen.private_reads > 0);
        if venue == "coinbase" {
            assert!(seen.auth_exchanges > 0);
            assert!(seen.private_reads >= 3);
        }
        assert_eq!(seen.refused, 0);
        drop(seen);
        session.close();
    }
    let (registry, io) = registry(venue);
    let resident =
        crate::accounts::testing::trading_account(&registry, "alice", &init(venue, algorithm, 7));
    let original_link = resident.link.clone().unwrap();
    let (mut session, mut sent) = user_session(registry.clone(), "alice");
    let mut done = session.take_verified();
    let mut device = Device {
        wrong_key: true,
        ..Default::default()
    };
    session.init(init(venue, algorithm, 7)).await.unwrap();
    let completed = drive(
        &mut session,
        &mut sent,
        &mut done,
        &mut device,
        venue,
        algorithm,
        7,
    )
    .await;
    assert!(completed.probe.as_ref().unwrap().permit.is_err());
    session.verification_completed(completed).await.unwrap();
    assert!(session.joined.is_empty());
    assert!(!registry.holds(&resident, session.socket));
    assert!(
        io.seen.lock().unwrap().refused > 0,
        "wrong key must reach and fail venue verification"
    );
    assert!(Arc::ptr_eq(resident.link.as_ref().unwrap(), &original_link));
    while let Ok(text) = sent.try_recv() {
        let frame: Value = serde_json::from_str(&text).unwrap();
        assert_ne!(frame["d"]["op"], "frame");
    }
    device.wrong_key = false;
    session.init(init(venue, algorithm, 7)).await.unwrap();
    let first_proof = drive(
        &mut session,
        &mut sent,
        &mut done,
        &mut device,
        venue,
        algorithm,
        7,
    )
    .await;
    assert!(first_proof.probe.as_ref().unwrap().permit.is_ok());
    drop(first_proof);
    device.replay = true;
    session.init(init(venue, algorithm, 7)).await.unwrap();
    let replay = drive(
        &mut session,
        &mut sent,
        &mut done,
        &mut device,
        venue,
        algorithm,
        7,
    )
    .await;
    assert!(device.replayed > 0);
    assert!(
        replay.probe.as_ref().unwrap().permit.is_err(),
        "old signature must not prove fresh server claims"
    );
    session.verification_completed(replay).await.unwrap();
    assert!(session.joined.is_empty());
    assert!(!registry.holds(&resident, session.socket));
    while sent.try_recv().is_ok() {}
    device.replay = false;
    session.init(init(venue, algorithm, 7)).await.unwrap();
    let frame = first(&mut sent).await;
    let reply = device.reply(venue, algorithm, 7, &frame);
    for owner in ["alice", "bob"] {
        let (mut foreign, _) = user_session(registry.clone(), owner);
        foreign.handle(&reply).await.unwrap();
        tokio::task::yield_now().await;
        assert!(done.try_recv().is_err());
        assert!(!registry.holds(&resident, foreign.socket));
        foreign.close();
    }
    session.handle(&reply).await.unwrap();
    session.handle(&reply).await.unwrap();
    let completed = drive(
        &mut session,
        &mut sent,
        &mut done,
        &mut device,
        venue,
        algorithm,
        7,
    )
    .await;
    assert!(completed.probe.as_ref().unwrap().permit.is_ok());
    session.verification_completed(completed).await.unwrap();
    assert!(registry.holds(&resident, session.socket));
    assert_eq!(registry.account_count(), 1);
    session.close();
}

#[tokio::test]
async fn coinbase_es256_native_proof_and_negatives() {
    tokio::task::LocalSet::new()
        .run_until(venue_cases("coinbase", "es256"))
        .await;
}
#[tokio::test]
async fn coinbase_eddsa_native_proof_and_negatives() {
    tokio::task::LocalSet::new()
        .run_until(venue_cases("coinbase", "edDsa"))
        .await;
}
#[tokio::test]
async fn coinbasespot_es256_native_proof_and_negatives() {
    tokio::task::LocalSet::new()
        .run_until(venue_cases("coinbasespot", "es256"))
        .await;
}
#[tokio::test]
async fn coinbasespot_eddsa_native_proof_and_negatives() {
    tokio::task::LocalSet::new()
        .run_until(venue_cases("coinbasespot", "edDsa"))
        .await;
}

#[tokio::test]
async fn coinbase_old_owner_completion_cannot_publish_on_another_users_session() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in ["coinbase", "coinbasespot"] {
                for algorithm in ["es256", "edDsa"] {
                    let (registry, _) = registry(venue);
                    let alice = crate::accounts::testing::trading_account(
                        &registry,
                        "alice",
                        &init(venue, algorithm, 7),
                    );
                    let bob = crate::accounts::testing::trading_account(
                        &registry,
                        "bob",
                        &init(venue, algorithm, 7),
                    );
                    let (mut old, mut old_sent) = user_session(registry.clone(), "alice");
                    let mut old_done = old.take_verified();
                    let mut device = Device::default();
                    old.init(init(venue, algorithm, 7)).await.unwrap();
                    let completion = drive(
                        &mut old,
                        &mut old_sent,
                        &mut old_done,
                        &mut device,
                        venue,
                        algorithm,
                        7,
                    )
                    .await;
                    assert!(completion.probe.as_ref().unwrap().permit.is_ok());
                    old.close();
                    let (mut successor, mut sent) = user_session(registry.clone(), "bob");
                    let mut done = successor.take_verified();
                    successor.init(init(venue, algorithm, 7)).await.unwrap();
                    let pending = first(&mut sent).await;
                    successor.verification_completed(completion).await.unwrap();
                    assert!(successor.joined.is_empty());
                    assert!(!registry.holds(&alice, successor.socket));
                    assert!(!registry.holds(&bob, successor.socket));
                    successor
                        .handle(&device.reply(venue, algorithm, 7, &pending))
                        .await
                        .unwrap();
                    let completed = drive(
                        &mut successor,
                        &mut sent,
                        &mut done,
                        &mut device,
                        venue,
                        algorithm,
                        7,
                    )
                    .await;
                    successor.verification_completed(completed).await.unwrap();
                    assert!(registry.holds(&bob, successor.socket));
                    assert!(!registry.holds(&alice, successor.socket));
                    successor.close();
                }
            }
        })
        .await;
}

#[tokio::test]
async fn coinbase_authentication_without_private_census_never_admits_and_disposal_retires_the_wait()
{
    tokio::task::LocalSet::new()
        .run_until(async {
            for venue in ["coinbase", "coinbasespot"] {
                for algorithm in ["es256", "edDsa"] {
                    let (registry, io) = registry(venue);
                    io.seen.lock().unwrap().block_private = true;
                    let resident = crate::accounts::testing::trading_account(
                        &registry,
                        "alice",
                        &init(venue, algorithm, 7),
                    );
                    let (mut session, mut sent) = user_session(registry.clone(), "alice");
                    let mut done = session.take_verified();
                    let mut device = Device::default();
                    session.init(init(venue, algorithm, 7)).await.unwrap();
                    tokio::time::timeout(Duration::from_secs(8),async {
                loop {
                    if io.seen.lock().unwrap().blocked_reads>0 {break;}
                    tokio::select! {
                        frame=sent.recv()=>{
                            let frame:Value=serde_json::from_str(&frame.unwrap()).unwrap();
                            session.handle(&device.reply(venue,algorithm,7,&frame)).await.unwrap();
                        },
                        _=tokio::time::sleep(Duration::from_millis(2))=>{},
                    }
                }
            }).await.unwrap();
                    assert!(done.try_recv().is_err());
                    assert!(session.joined.is_empty());
                    assert!(!registry.holds(&resident, session.socket));
                    session
                        .handle(r#"{"t":"dispose","d":{"accountName":"acct"}}"#)
                        .await
                        .unwrap();
                    io.gate.add_permits(100);
                    tokio::task::yield_now().await;
                    if let Ok(completed) = done.try_recv() {
                        session.verification_completed(completed).await.unwrap();
                    }
                    assert!(session.joined.is_empty());
                    assert!(!registry.holds(&resident, session.socket));
                    session.close();
                }
            }
        })
        .await;
}
