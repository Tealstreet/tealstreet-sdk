//! Exercises the wire handover, native probe, account admission and manual write together.
use super::*;
use crate::delegated::{KeyOffer, Sealed};
use crate::delegated_e2e_io::MockVenue;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hpke::{
    Deserializable, OpModeR, aead::ChaCha20Poly1305, kdf::HkdfSha256, kem::X25519HkdfSha256,
};
use std::time::Duration;
use tokio::sync::mpsc;

const USER: &str = "synthetic-tenant:alice";
const ACCOUNT: &str = "synthetic-e2e";
const INSTANCE: &str = "e2e-registration";

fn vectors() -> Vec<Value> {
    let data: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/delegated-handover-vectors.json"
    ))
    .unwrap();
    data["vectors"].as_array().unwrap().clone()
}

/// Use the S5 fixture bytes before sealing the same plaintext to a fresh server offer.
fn plaintext(vector: &Value) -> Vec<u8> {
    let input = &vector["input"];
    let sealed: Sealed = serde_json::from_value(vector["sealed"].clone()).unwrap();
    let private = <X25519HkdfSha256 as hpke::Kem>::PrivateKey::from_bytes(
        &[vector["recipientSeedByte"].as_u64().unwrap() as u8; 32],
    )
    .unwrap();
    let enc = <X25519HkdfSha256 as hpke::Kem>::EncappedKey::from_bytes(
        &STANDARD.decode(&sealed.enc).unwrap(),
    )
    .unwrap();
    let mut receiver = hpke::setup_receiver::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
        &OpModeR::Base,
        &private,
        &enc,
        crate::delegated::INFO,
    )
    .unwrap();
    let aad = format!(
        "{}|{}|{}|{}",
        input["attemptId"].as_str().unwrap(),
        input["bootId"].as_str().unwrap(),
        input["exchangeName"].as_str().unwrap(),
        input["key"].as_str().unwrap()
    );
    let bytes = receiver
        .open(
            &STANDARD.decode(&sealed.ciphertext).unwrap(),
            aad.as_bytes(),
        )
        .unwrap();
    let fields: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        fields["key"] == input["key"]
            && fields["secret"] == input["secret"]
            && fields["passphrase"] == input["passphrase"],
        "S5 fixture fields match without formatting material"
    );
    bytes
}

fn connection(registry: Arc<Registry>) -> (Session, crate::out::Receiver) {
    let (out, sent, _) = Out::channel(1024);
    (
        Session::new(
            registry,
            Arc::default(),
            Identity {
                user: Some(USER.into()),
                chat_handle: None,
                token: Some(("synthetic-proxy-token".into(), u64::MAX)),
                resume_from: None,
                resumable: false,
            },
            out,
        ),
        sent,
    )
}

async fn wire(session: &mut Session, message: Value) {
    session.handle(&message.to_string()).await.unwrap();
}

async fn next_text(sent: &mut crate::out::Receiver, op: &str) -> String {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(text) = sent.recv().await {
            let value: Value = serde_json::from_str(&text).unwrap();
            assert_ne!(
                value["t"], "sign-req",
                "DELEGATED uses the resident provider"
            );
            if value["d"]["op"] == op {
                return text;
            }
        }
        panic!("missing test reply")
    })
    .await
    .expect("bounded test reply")
}

async fn next(sent: &mut crate::out::Receiver, op: &str) -> Value {
    serde_json::from_str(&next_text(sent, op).await).unwrap()
}

async fn snapshot(session: &mut Session, sent: &mut crate::out::Receiver) -> Value {
    let mut loaded = session.take_loaded();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            tokio::select! {
                Some((name, client)) = loaded.recv() => session.account_loaded(&name, client).await,
                Some(text) = sent.recv() => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(value["t"], "sign-req");
                    if value["d"]["op"] == "frame" { return value; }
                }
            }
        }
    })
    .await
    .expect("loaded snapshot reaches the connection")
}

async fn sealed_init(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    vector: &Value,
    bytes: &[u8],
    venue: &str,
) -> Value {
    wire(
        session,
        json!({"t":"msg","d":{"op":"delegated-key-req","accountName":ACCOUNT,"data":{"exchangeName":"bybit"}}}),
    )
    .await;
    let response = next(sent, "delegated-key").await;
    assert_eq!(response["d"]["accountName"], ACCOUNT);
    assert_eq!(response["d"]["bootId"], crate::state::boot_id());
    let offer = KeyOffer {
        attempt_id: response["d"]["attemptId"].as_str().unwrap().into(),
        public_key: response["d"]["publicKey"].as_str().unwrap().into(),
        boot_id: crate::state::boot_id(),
        expires_at_ms: response["d"]["expiresAtMs"].as_u64().unwrap(),
    };
    assert!(offer.expires_at_ms > crate::accounts::now_ms());
    let key = vector["input"]["key"].as_str().unwrap();
    let sealed = crate::delegated::tests::seal(&offer, venue, key, bytes);
    json!({"t":"init","d":{"accountName":ACCOUNT,"exchangeName":venue,"key":key,"instanceId":INSTANCE,"signingMode":"delegated","sealed":sealed}})
}

async fn completed(results: &mut mpsc::UnboundedReceiver<JoinCompletion>) -> JoinCompletion {
    tokio::time::timeout(Duration::from_secs(5), results.recv())
        .await
        .expect("native probe settles")
        .unwrap()
}

async fn complete(session: &mut Session, results: &mut mpsc::UnboundedReceiver<JoinCompletion>) {
    session
        .verification_completed(completed(results).await)
        .await
        .unwrap();
}

async fn assert_empty(session: &Session, registry: &Registry) {
    assert!(session.joined.is_empty());
    assert!(session.pending_verifications.is_empty());
    assert!(registry.held(USER, ACCOUNT, session.socket).is_none());
    assert!(!registry.live_here(USER));
    tokio::time::timeout(Duration::from_secs(5), async {
        while crate::accounts::testing::engine_count(registry) != 0 {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("temporary probe is released");
}

fn mock(vector: &Value) -> MockVenue {
    MockVenue::new(
        vector["input"]["key"].as_str().unwrap().into(),
        vector["input"]["secret"].as_str().unwrap().into(),
    )
}

#[tokio::test]
async fn delegated_e2e_sealed_init_probe_admission_and_resident_signed_manual_write() {
    for vector in vectors() {
        let bytes = plaintext(&vector);
        let venue = mock(&vector);
        let registry = crate::accounts::testing::e2e_registry(USER, venue.clone());
        let (mut session, mut sent) = connection(registry.clone());
        let mut results = session.take_verified();
        let init = sealed_init(&mut session, &mut sent, &vector, &bytes, "bybit").await;
        wire(&mut session, init).await;
        assert!(session.joined.is_empty(), "probe precedes admission");
        let result = completed(&mut results).await;
        assert!(result.candidate.is_ok());
        assert!(result.probe.as_ref().unwrap().permit.is_ok());
        assert!(
            venue.observed.lock().unwrap().reads > 0,
            "native private probe ran before admission"
        );
        assert!(registry.held(USER, ACCOUNT, session.socket).is_none());
        session.verification_completed(result).await.unwrap();
        let account = registry
            .held(USER, ACCOUNT, session.socket)
            .expect("verified handover admitted");
        assert!(account.delegated.is_some() && account.link.is_none());
        let snapshot = snapshot(&mut session, &mut sent).await;
        assert_eq!(snapshot["d"]["instanceId"], INSTANCE);
        assert_eq!(snapshot["d"]["snapshot"], true);
        tokio::time::timeout(Duration::from_secs(5), async {
            while account
                .handle
                .read(|s| s.loaded().markets && s.loaded().orders && s.loaded().positions)
                .await
                != Some(true)
            {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("resident startup finishes");
        assert!(venue.observed.lock().unwrap().reads > 0);
        assert!(venue.observed.lock().unwrap().writes.is_empty());
        wire(&mut session, json!({"t":"msg","d":{"op":"callMethod","accountName":ACCOUNT,"data":{"requestId":"manual","method":"placeOrder","args":[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":50000}]}}})).await;
        let reply = next(&mut sent, "callMethod:done").await;
        assert_eq!(reply["d"]["instanceId"], INSTANCE);
        assert_eq!(
            reply["d"]["data"],
            json!({"requestId":"manual","success":true,"result":["synthetic-order"]})
        );
        {
            let observed = venue.observed.lock().unwrap();
            assert_eq!(observed.refused, 0);
            assert_eq!(observed.writes.len(), 1);
            assert_eq!(observed.writes[0]["symbol"], "BTCUSDT");
            assert_eq!(observed.writes[0]["side"], "Buy");
        }
        session.close();
    }
}

#[tokio::test]
async fn delegated_e2e_wrong_pair_replay_and_disabled_venue_leave_no_account() {
    let vector = vectors().remove(0);
    for case in ["wrong-pair", "replay", "other-venue"] {
        let mut bytes = plaintext(&vector);
        let venue = mock(&vector);
        let registry = crate::accounts::testing::e2e_registry(USER, venue.clone());
        let (mut session, mut sent) = connection(registry.clone());
        let mut results = session.take_verified();
        if case == "wrong-pair" {
            let mut fields: Value = serde_json::from_slice(&bytes).unwrap();
            fields["secret"] = "synthetic-other-material".into();
            bytes = serde_json::to_vec(&fields).unwrap();
        }
        let name = if case == "other-venue" {
            "hyrotrader"
        } else {
            "bybit"
        };
        assert_eq!(crate::delegated::enabled(name), case != "other-venue");
        let init = sealed_init(&mut session, &mut sent, &vector, &bytes, name).await;
        if case == "replay" {
            // Consume this exact envelope once through the actual opener/probe,
            // then explicitly release the admitted account before replaying it.
            wire(&mut session, init.clone()).await;
            complete(&mut session, &mut results).await;
            assert!(registry.held(USER, ACCOUNT, session.socket).is_some());
            wire(
                &mut session,
                json!({"t":"dispose","d":{"accountName":ACCOUNT}}),
            )
            .await;
            while sent.try_recv().is_ok() {}
            assert_empty(&session, &registry).await;
        }
        let before = venue.observed.lock().unwrap().reads;
        let sealed: Sealed = serde_json::from_value(init["d"]["sealed"].clone()).unwrap();
        wire(&mut session, init).await;
        if session.pending_verifications.contains_key(ACCOUNT) {
            complete(&mut session, &mut results).await;
        }
        let refusal = next_text(&mut sent, "error").await;
        let expected =
            envelope::with_instance_id(envelope::init_unverified(ACCOUNT, false), INSTANCE);
        assert_eq!(refusal, expected);
        assert_empty(&session, &registry).await;
        assert!(
            session
                .handovers
                .take(ACCOUNT, &sealed, crate::accounts::now_ms())
                .is_err()
        );
        let observed = venue.observed.lock().unwrap();
        assert!(observed.writes.is_empty());
        if case == "wrong-pair" {
            assert!(observed.refused > 0);
        } else {
            assert_eq!(observed.reads, before);
            assert_eq!(observed.refused, 0);
        }
        drop(observed);
        session.close();
    }
}

#[tokio::test]
async fn delegated_e2e_bybit_declaration_off_refuses_valid_handover_without_io() {
    let vector = vectors().remove(0);
    let bytes = plaintext(&vector);
    let venue = mock(&vector);
    let registry = crate::accounts::testing::e2e_registry(USER, venue.clone());
    let (mut session, mut sent) = connection(registry.clone());
    let mut results = session.take_verified();
    // Obtain a valid Bybit handover while the declaration is enabled.
    let init = sealed_init(&mut session, &mut sent, &vector, &bytes, "bybit").await;
    assert_eq!(init["d"]["exchangeName"], "bybit");
    let sealed: Sealed = serde_json::from_value(init["d"]["sealed"].clone()).unwrap();
    let mut declaration: Value = serde_json::from_str(include_str!(
        "../../../../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
    ))
    .unwrap();
    assert_eq!(
        declaration["venues"]["bybit"]["remoteSigning"]["enabled"],
        true
    );
    declaration["venues"]["bybit"]["remoteSigning"]["enabled"] = false.into();
    crate::delegated::with_test_declaration(declaration, async {
        wire(&mut session, init).await;
        assert_empty(&session, &registry).await;
        let expected =
            envelope::with_instance_id(envelope::init_unverified(ACCOUNT, false), INSTANCE);
        assert_eq!(next_text(&mut sent, "error").await, expected);
        assert!(
            session
                .handovers
                .take(ACCOUNT, &sealed, crate::accounts::now_ms())
                .is_err()
        );
        assert!(results.try_recv().is_err());
        let observed = venue.observed.lock().unwrap();
        assert_eq!(observed.preparations, 0);
        assert_eq!(observed.requests, 0);
        assert_eq!(observed.sockets, 0);
        assert_eq!(observed.reads, 0);
        assert_eq!(observed.refused, 0);
        assert!(observed.writes.is_empty());
    })
    .await;
    // The task-local fixture is gone: the same connection and pair can use a
    // fresh offer through the normal probe and resident construction paths.
    let init = sealed_init(&mut session, &mut sent, &vector, &bytes, "bybit").await;
    wire(&mut session, init).await;
    complete(&mut session, &mut results).await;
    let account = registry
        .held(USER, ACCOUNT, session.socket)
        .expect("enabled declaration admits the valid pair");
    assert!(account.delegated.is_some() && account.link.is_none());
    assert!(venue.observed.lock().unwrap().reads > 0);
    session.close();
}
