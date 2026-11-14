//! Admission through the production Session path, independent of DELEGATED rollout.
use super::*;
use crate::delegated::probe::venue_tests::{Seen, operation};
use crate::delegated_e2e_io::MockVenue;
use std::time::Duration;

const READY: &[&str] = &["bybit", "binance", "okx", "bitget", "blofin", "kucoin"];
const KEY: &str = "synthetic-key";
const MATERIAL: &str = "synthetic-material";

#[tokio::test]
async fn cosigned_catalog_does_not_gate_delegated_wallet_proof() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for delegated in [false, true] {
                let registry = crate::accounts::tests::registry();
                let (mut session, mut sent) = user_session(registry.clone(), "fixture");
                let mut completed = session.take_verified();
                let mut init = InitPayload {
                    account_name: "acct".into(),
                    exchange_name: "hyperliquidspot".into(),
                    key: KEY.into(),
                    ..Default::default()
                };
                if delegated {
                    init.signing_mode = Some("delegated".into());
                    let offer = session
                        .handovers
                        .issue("acct", "hyperliquidspot", crate::accounts::now_ms())
                        .unwrap();
                    init.sealed = Some(crate::delegated::tests::seal(
                        &offer,
                        "hyperliquidspot",
                        KEY,
                        br#"{"key":"synthetic-key","secret":"synthetic-material"}"#,
                    ));
                }
                // Admission schedules the proof without polling it. No private IO or
                // account attachment is authorized merely by starting verification.
                session.admission_init(init);
                assert!(session.joined.is_empty());
                assert_eq!(registry.account_count(), 0);
                if delegated {
                    assert!(session.pending_verifications.contains_key("acct"));
                    assert!(sent.try_recv().is_err());
                } else {
                    assert!(session.pending_verifications.is_empty());
                    assert_eq!(
                        sent.try_recv().unwrap(),
                        envelope::init_proof_unavailable("acct")
                    );
                }
                session.close();
                tokio::task::yield_now().await;
                assert!(completed.try_recv().is_err());
                assert_eq!(registry.account_count(), 0);
            }
        })
        .await;
}

#[derive(Clone, Copy)]
enum Answer {
    Correct,
    OtherKey,
    Replay,
}

#[derive(Default)]
struct Device {
    prior: HashMap<String, String>,
    replayed: usize,
    other_key: bool,
}
impl Device {
    async fn answer(&mut self, venue: &str, session: &mut Session, frame: Value, mode: Answer) {
        let request = &frame["d"];
        let preimage = request["wsAuth"]["toSign"].as_str().unwrap();
        let socket_auth =
            preimage.contains("/users/self/verify") || preimage.contains("/user/verify");
        let use_other_key = self.other_key || !socket_auth && matches!(mode, Answer::OtherKey);
        let signing_key = if self.other_key {
            "synthetic-other-key"
        } else {
            KEY
        };
        let material = if use_other_key {
            "synthetic-other-material"
        } else {
            MATERIAL
        };
        let mut options = serde_json::from_value(
            json!({"key":signing_key,"secret":material,"passphrase":"synthetic-passphrase"}),
        )
        .unwrap();
        let (provider, identities) =
            safe_cex_venues::prepare_local_signer_for(venue, &mut options).unwrap();
        let input = serde_json::from_value(json!({"kind":"wsAuth","toSign":preimage,"outputFormat":request["wsAuth"]["outputFormat"]})).unwrap();
        let (work, output) =
            safe_cex_venues::hosted_signing::prepare(venue, &identities, input).unwrap();
        let signed = provider.sign(work).await.unwrap();
        let fresh = safe_cex_venues::hosted_signing::finish(output, &signed)
            .unwrap()
            .signature
            .unwrap();
        let signature = if socket_auth {
            fresh
        } else {
            let key = operation(venue, preimage);
            if matches!(mode, Answer::Replay) {
                let prior = self
                    .prior
                    .get(&key)
                    .expect("positive proof signed this operation");
                assert_ne!(prior, &fresh, "new proof must use fresh signing bytes");
                self.replayed += 1;
                prior.clone()
            } else {
                if matches!(mode, Answer::Correct) {
                    self.prior.insert(key, fresh.clone());
                }
                fresh
            }
        };
        let response = json!({"id":request["id"],"wsAuthSignature":signature});
        let link = session
            .pending_verifications
            .get("acct")
            .and_then(|p| p.cosigned_link.clone())
            .or_else(|| {
                session
                    .joined
                    .get("acct")
                    .and_then(|j| j.account.link.clone())
            })
            .unwrap();
        assert!(
            !link.resolve(session.socket + 1000, response.clone()),
            "foreign socket cannot answer"
        );
        session
            .handle(&json!({"t":"sign-res","d":response}).to_string())
            .await
            .unwrap();
        assert!(
            !link.resolve(session.socket, response.clone()),
            "response consumed once"
        );
        session
            .handle(&json!({"t":"sign-res","d":response}).to_string())
            .await
            .unwrap();
    }
}

fn payload(venue: &str) -> InitPayload {
    InitPayload {
        account_name: "acct".into(),
        exchange_name: venue.into(),
        key: KEY.into(),
        passphrase: Some("synthetic-passphrase".into()),
        signing_mode: Some("cosigned".into()),
        instance_id: Some("proof-registration".into()),
        ..Default::default()
    }
}

async fn proof(
    venue: &str,
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    device: &mut Device,
    answer: Answer,
) {
    let mut done = session.take_verified();
    let mut init = payload(venue);
    if device.other_key {
        init.key = "synthetic-other-key".into();
    }
    session.init(init).await.unwrap();
    assert!(
        session.pending_verifications.contains_key("acct"),
        "{venue}: every fresh connection must prove its own key"
    );
    assert!(!session.joined.contains_key("acct"));
    assert!(
        session
            .registry
            .held(session.user(), "acct", session.socket)
            .is_none()
    );
    session
        .handle(r#"{"t":"msg","d":{"op":"sendSnapshot","accountName":"acct"}}"#)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(30),async {
        loop { tokio::select! {
            completed=done.recv()=>{ session.verification_completed(completed.unwrap()).await.unwrap(); break; }
            text=sent.recv()=>{ let frame:Value=serde_json::from_str(&text.unwrap()).unwrap();
                if frame["t"]=="sign-req" { device.answer(venue,session,frame,answer).await; }
                else { assert_ne!(frame["d"]["op"],"frame", "pending connection must not receive state"); }
            }
        }}
    }).await.expect("native proof settles");
}

async fn load(
    venue: &str,
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    device: &mut Device,
) {
    let account = session.joined["acct"].account.clone();
    tokio::time::timeout(Duration::from_secs(30),async {
        loop {
            if account.handle.read(|s|{let l=s.loaded();l.markets&&l.balance&&l.orders&&l.positions}).await==Some(true){break;}
            tokio::select! {
                text=sent.recv()=>{ let frame:Value=serde_json::from_str(&text.unwrap()).unwrap(); if frame["t"]=="sign-req"{device.answer(venue,session,frame,Answer::Correct).await;} }
                _=tokio::time::sleep(Duration::from_millis(5))=>{}
            }
        }
    }).await.expect("positive resident fully loaded");
}

async fn venue_case(venue: &'static str) {
    assert!(
        crate::delegated::enabled(venue),
        "shared hosted eligibility does not replace a fresh COSIGNED proof"
    );
    let (io, seen): (MockVenue, Arc<Mutex<Seen>>) = MockVenue::for_admission(venue);
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    // The other credential is valid under its own public key. It cannot prove KEY.
    let (mut other, mut other_sent) = user_session(registry.clone(), "other-valid-key");
    other.identity.resumable = false;
    let mut other_device = Device {
        other_key: true,
        ..Default::default()
    };
    proof(
        venue,
        &mut other,
        &mut other_sent,
        &mut other_device,
        Answer::Correct,
    )
    .await;
    load(venue, &mut other, &mut other_sent, &mut other_device).await;
    other.close();
    let (mut resident, mut output) = user_session(registry.clone(), "alice");
    resident.identity.resumable = false;
    let mut device = Device::default();
    proof(
        venue,
        &mut resident,
        &mut output,
        &mut device,
        Answer::Correct,
    )
    .await;
    load(venue, &mut resident, &mut output, &mut device).await;
    let account = resident.joined["acct"].account.clone();
    let provider = account.link.clone().unwrap();
    assert!(seen.lock().unwrap().signed_reads > 0);
    let original_socket = provider.socket();
    for mode in [Answer::OtherKey, Answer::Replay] {
        let before = seen.lock().unwrap().refused;
        let (mut candidate, mut sent) = user_session(registry.clone(), "alice");
        candidate.identity.resumable = false;
        proof(venue, &mut candidate, &mut sent, &mut device, mode).await;
        assert!(candidate.joined.is_empty());
        assert!(registry.held("alice", "acct", candidate.socket).is_none());
        assert!(Arc::ptr_eq(
            &account,
            &registry.held("alice", "acct", resident.socket).unwrap()
        ));
        assert_eq!(provider.socket(), original_socket);
        assert!(
            seen.lock().unwrap().refused > before,
            "venue actually checked wrong signature"
        );
        let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
            .map(|s| serde_json::from_str(&s).unwrap())
            .collect();
        assert!(
            frames
                .iter()
                .any(|f| f["d"]["data"][1]["kind"] == "unverified"
                    && f["d"]["instanceId"] == "proof-registration")
        );
        assert!(!frames.iter().any(|f| f["d"]["op"] == "frame"));
        candidate.close();
    }
    assert!(device.replayed > 0);
    resident.close();
}

#[tokio::test]
async fn cosigned_binance_requires_connection_proof() {
    venue_case("binance").await;
}

#[tokio::test]
async fn cosigned_okx_requires_connection_proof() {
    venue_case("okx").await;
}

#[tokio::test]
async fn cosigned_bitget_requires_connection_proof() {
    venue_case("bitget").await;
}

#[tokio::test]
async fn cosigned_blofin_requires_connection_proof() {
    venue_case("blofin").await;
}

#[tokio::test]
async fn cosigned_kucoin_requires_connection_proof() {
    venue_case("kucoin").await;
}

#[tokio::test]
async fn cosigned_unready_venues_fail_closed_with_a_typed_refusal() {
    let rules: Value = serde_json::from_str(include_str!(
        "../../../../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
    let io = MockVenue::new(KEY.into(), MATERIAL.into());
    let observed = io.observed.clone();
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    for (venue, rule) in rules["venues"].as_object().unwrap() {
        if rule["remoteSigning"]["enabled"] != true || READY.contains(&venue.as_str()) {
            continue;
        }
        let (mut session, mut sent) = user_session(registry.clone(), "alice");
        session.identity.resumable = false;
        session.init(payload(venue)).await.unwrap();
        assert!(
            session.joined.is_empty(),
            "{venue}: no proof-ready admission"
        );
        assert!(
            session.pending_verifications.is_empty(),
            "{venue}: unavailable proof cannot start"
        );
        assert!(registry.held("alice", "acct", session.socket).is_none());
        let frames: Vec<Value> = std::iter::from_fn(|| sent.try_recv().ok())
            .map(|s| serde_json::from_str(&s).unwrap())
            .collect();
        // Compiled codecs do not imply account admission. The beta roster
        // refuses excluded venues before the COSIGNED proof-readiness gate;
        // admitted venues still require their own proof or refuse unavailable.
        let expected_kind = if rule["betaAdmitted"] == false || !safe_cex_venues::supported(venue) {
            "unsupported"
        } else {
            "proof-unavailable"
        };
        assert!(
            frames
                .iter()
                .any(|f| f["d"]["data"][1]["kind"] == expected_kind
                    && f["d"]["data"][1]["initRefused"] == true
                    && f["d"]["instanceId"] == "proof-registration"),
            "{venue}: {frames:?}"
        );
        assert!(
            !frames
                .iter()
                .any(|f| f["t"] == "sign-req" || f["d"]["op"] == "frame")
        );
        assert!(
            frames
                .iter()
                .any(|f| f["d"]["data"][1]["outcome"] == "notSent")
        );
        if expected_kind == "proof-unavailable" {
            assert!(
                frames
                    .iter()
                    .any(|f| f["d"]["data"][1]["transient"] == false)
            );
        }
        let seen = observed.lock().unwrap();
        assert_eq!(
            (seen.preparations, seen.requests, seen.sockets),
            (0, 0, 0),
            "unready venue must not reach IO"
        );
        drop(seen);
        session.close();
    }
}

#[tokio::test]
async fn cosigned_login_switch_during_new_family_signing_await_is_isolated() {
    let (io, _) = MockVenue::for_admission("binance");
    let registry = crate::accounts::testing::e2e_registry("alice", io);
    let (mut alice, mut a_sent) = user_session(registry.clone(), "alice");
    let mut a_done = alice.take_verified();
    alice.init(payload("binance")).await.unwrap();
    assert!(alice.pending_verifications.contains_key("acct"));
    // Withhold Alice's proof signature: her native proof is genuinely deferred.
    let request = tokio::time::timeout(Duration::from_secs(30), a_sent.recv())
        .await
        .unwrap()
        .unwrap();
    let request: Value = serde_json::from_str(&request).unwrap();
    assert_eq!(request["t"], "sign-req");
    let old_socket = alice.socket;
    alice.close();
    let (mut bob, mut b_sent) = user_session(registry.clone(), "bob");
    bob.identity.resumable = false;
    let mut device = Device::default();
    proof(
        "binance",
        &mut bob,
        &mut b_sent,
        &mut device,
        Answer::Correct,
    )
    .await;
    assert_eq!(bob.joined["acct"].account.user, "bob");
    assert!(registry.held("alice", "acct", old_socket).is_none());
    assert!(registry.held("bob", "acct", bob.socket).is_some());
    assert!(a_done.try_recv().is_err());
    assert!(a_sent.try_recv().is_err());
    bob.close();
}

#[test]
fn cosigned_kucoin_probe_setup_uses_the_existing_futures_origin() {
    assert_eq!(
        safe_cex_native::resolve::rest_base_url("kucoin", false).unwrap(),
        "https://api-futures.kucoin.com"
    );
    assert!(safe_cex_native::resolve::rest_base_url("kucoin", true).is_err());
    let link = safe_cex_remote_signer::SignerLink::new(1, Arc::new(|_| true));
    assert!(crate::accounts::prepare(&payload("kucoin"), false, Some(link)).is_ok());
}

#[tokio::test]
async fn cosigned_bybit_proof_ignores_obsolete_delegated_rollout_field() {
    crate::delegated::with_test_declaration(
        json!({"venues":{"bybit":{"remoteSigning":{"enabled":true},"delegated":{"enabled":false}}}}),
        async {
            // Preserve the production hosted/identity policy across the spawned
            // proof task; the obsolete delegated flag must not select admission.
            assert!(crate::delegated::enabled("bybit"));
            let registry = crate::accounts::testing::e2e_registry(
                "alice",
                MockVenue::new(KEY.into(), MATERIAL.into()),
            );
            let (mut session, mut sent) = user_session(registry.clone(), "alice");
            session.identity.resumable = false;
            session.init(payload("bybit")).await.unwrap();
            assert!(session.pending_verifications.contains_key("acct"));
            let request = tokio::time::timeout(Duration::from_secs(30), sent.recv())
                .await
                .unwrap()
                .unwrap();
            let request: Value = serde_json::from_str(&request).unwrap();
            assert_eq!(
                request["t"], "sign-req",
                "native proof actually requested the joining signature"
            );
            assert!(session.joined.is_empty());
            assert!(registry.held("alice", "acct", session.socket).is_none());
            session.close();
        },
    )
    .await;
}
