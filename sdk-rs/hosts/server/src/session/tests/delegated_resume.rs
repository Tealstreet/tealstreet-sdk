//! AGT-54: a DELEGATED reconnect inside the resume grace re-attaches the resident
//! account and signer without a fresh handover or venue probe.
use super::delegated_e2e::{
    ACCOUNT, INSTANCE, USER, completed, mock, next, plaintext, sealed_init, snapshot, vectors, wire,
};
use super::*;
use std::time::{Duration, Instant};

/// Simulated venue round trip for every REST read, so the probe's serial reads cost time.
const VENUE_RTT: Duration = Duration::from_millis(40);

fn resumable(
    registry: Arc<Registry>,
    user: &str,
    from: Option<String>,
) -> (Session, crate::out::Receiver) {
    let (out, sent, _) = Out::channel(1024);
    (
        Session::new(
            registry,
            Arc::default(),
            Identity {
                user: Some(user.into()),
                chat_handle: None,
                token: Some(("synthetic-proxy-token".into(), u64::MAX)),
                resume_from: from,
                resumable: true,
            },
            out,
        ),
        sent,
    )
}

async fn session_frame(sent: &mut crate::out::Receiver) -> Value {
    tokio::time::timeout(Duration::from_secs(5), async {
        while let Some(text) = sent.recv().await {
            let value: Value = serde_json::from_str(&text).unwrap();
            if value["t"] == "session" {
                return value["d"].clone();
            }
        }
        panic!("missing session frame")
    })
    .await
    .expect("bounded session frame")
}

fn resume_init(key: &str, proof: &str) -> Value {
    json!({"t":"init","d":{"accountName":ACCOUNT,"exchangeName":"bybit","key":key,
        "instanceId":INSTANCE,"signingMode":"delegated","resume":true,"resumeProof":proof}})
}

async fn refused_resume(session: &mut Session, sent: &mut crate::out::Receiver, init: Value) {
    wire(session, init).await;
    let refusal = next(sent, "error").await;
    let meta = &refusal["d"]["data"][1];
    assert_eq!(meta["reason"], "resume_unavailable");
    assert_eq!(meta["transient"], true);
    assert_eq!(meta["initRefused"], true);
    assert!(session.pending_verifications.is_empty(), "no probe starts");
}

/// The attachment confirmation and the first snapshot, in whichever order they arrive.
async fn mode_and_snapshot(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
) -> (Value, Value) {
    let mut loaded = session.take_loaded();
    let (mut mode, mut frame) = (None, None);
    tokio::time::timeout(Duration::from_secs(5), async {
        while mode.is_none() || frame.is_none() {
            tokio::select! {
                Some((name, client)) = loaded.recv() => session.account_loaded(&name, client).await,
                Some(text) = sent.recv() => {
                    let value: Value = serde_json::from_str(&text).unwrap();
                    assert_ne!(value["t"], "sign-req", "DELEGATED uses the resident provider");
                    assert_ne!(value["d"]["op"], "delegated-key", "no handover offer");
                    match value["d"]["op"].as_str() {
                        Some("account-signing-mode") => mode = Some(value),
                        Some("frame") if frame.is_none() => frame = Some(value),
                        _ => {}
                    }
                }
            }
        }
    })
    .await
    .expect("resumed account confirms and snapshots");
    (mode.unwrap(), frame.unwrap())
}

/// Ordinary sealed handover through the native probe; returns init-to-snapshot time.
async fn handover(
    session: &mut Session,
    sent: &mut crate::out::Receiver,
    vector: &Value,
    bytes: &[u8],
) -> Duration {
    let mut results = session.take_verified();
    let started = Instant::now();
    let init = sealed_init(session, sent, vector, bytes, "bybit").await;
    wire(session, init).await;
    assert!(session.joined.is_empty(), "probe precedes admission");
    session
        .verification_completed(completed(&mut results).await)
        .await
        .unwrap();
    assert!(session.joined.contains_key(ACCOUNT));
    snapshot(session, sent).await;
    started.elapsed()
}

#[tokio::test]
async fn delegated_resume_skips_handover_and_probe_on_reconnect() {
    let vector = vectors().remove(0);
    let bytes = plaintext(&vector);
    let key = vector["input"]["key"].as_str().unwrap().to_owned();
    let venue = mock(&vector).with_latency(VENUE_RTT);
    let registry = crate::accounts::testing::e2e_registry(USER, venue.clone());

    let (mut first, mut sent) = resumable(registry.clone(), USER, None);
    first.open().await;
    assert!(
        session_frame(&mut sent)
            .await
            .get("delegatedResume")
            .is_none()
    );
    let cold = handover(&mut first, &mut sent, &vector, &bytes).await;
    let account = registry.held(USER, ACCOUNT, first.socket).unwrap();
    let parked_id = first.resume_id.clone();
    first.close();

    // Before: today's client re-hands-over the key and the box re-runs the probe.
    let (mut second, mut sent) = resumable(registry.clone(), USER, Some(parked_id));
    second.open().await;
    assert_eq!(
        session_frame(&mut sent).await["delegatedResume"],
        json!([ACCOUNT])
    );
    let reads = venue.observed.lock().unwrap().reads;
    let rehandover = handover(&mut second, &mut sent, &vector, &bytes).await;
    assert!(
        venue.observed.lock().unwrap().reads > reads,
        "the handover path probes the venue again"
    );
    let held = registry.held(USER, ACCOUNT, second.socket).unwrap();
    assert!(
        Arc::ptr_eq(&account, &held),
        "handover joins the same resident"
    );
    let (parked_id, proof) = (second.resume_id.clone(), second.resume_proof.clone());
    second.close();

    // After: the resume init claims the parked label with no handover or probe.
    let (mut third, mut sent) = resumable(registry.clone(), USER, Some(parked_id));
    third.open().await;
    assert_eq!(
        session_frame(&mut sent).await["delegatedResume"],
        json!([ACCOUNT])
    );
    let started = Instant::now();
    wire(&mut third, resume_init(&key, &proof)).await;
    assert!(
        third.joined.contains_key(ACCOUNT) && third.pending_verifications.is_empty(),
        "joined synchronously: no key offer, sealed init or venue probe"
    );
    let (mode, frame) = mode_and_snapshot(&mut third, &mut sent).await;
    let resumed = started.elapsed();
    assert_eq!(mode["d"]["instanceId"], INSTANCE);
    assert_eq!(mode["d"]["data"]["actualMode"], "delegated");
    assert_eq!(frame["d"]["instanceId"], INSTANCE);
    assert_eq!(frame["d"]["snapshot"], true);
    let held = registry.held(USER, ACCOUNT, third.socket).unwrap();
    assert!(Arc::ptr_eq(&account, &held), "the same resident account");
    assert!(held.delegated().is_some() && held.link.is_none());
    println!(
        "AGT-54 reconnect init->snapshot (venue RTT {VENUE_RTT:?}): cold handover {cold:?}, \
         re-handover {rehandover:?}, resume {resumed:?}"
    );
    assert!(resumed < rehandover, "{resumed:?} >= {rehandover:?}");

    // Account-originated notices (algo errors, stop replay) reach the resumed socket.
    held.notify(json!({"t":"msg","d":{"op":"agt54-notice","accountName":ACCOUNT}}).to_string());
    let notice = next(&mut sent, "agt54-notice").await;
    assert_eq!(notice["d"]["accountName"], ACCOUNT);

    // The resident signer still writes after the resume.
    wire(&mut third, json!({"t":"msg","d":{"op":"callMethod","accountName":ACCOUNT,"data":{"requestId":"after-resume","method":"placeOrder","args":[{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":50000}]}}})).await;
    let reply = next(&mut sent, "callMethod:done").await;
    assert_eq!(reply["d"]["instanceId"], INSTANCE);
    assert_eq!(reply["d"]["data"]["success"], true);
    assert_eq!(venue.observed.lock().unwrap().writes.len(), 1);
    third.close();
}

#[tokio::test]
async fn delegated_resume_binds_socket_owner_label_venue_and_identity() {
    let vector = vectors().remove(0);
    let bytes = plaintext(&vector);
    let key = vector["input"]["key"].as_str().unwrap().to_owned();
    let registry = crate::accounts::testing::e2e_registry(USER, mock(&vector));
    let (mut first, mut sent) = resumable(registry.clone(), USER, None);
    first.open().await;
    session_frame(&mut sent).await;
    handover(&mut first, &mut sent, &vector, &bytes).await;
    let account = registry.held(USER, ACCOUNT, first.socket).unwrap();
    let (parked_id, proof) = (first.resume_id.clone(), first.resume_proof.clone());
    first.close();

    // A socket that did not present the id cannot claim it.
    let (mut stranger, mut sent) = resumable(registry.clone(), USER, None);
    stranger.open().await;
    session_frame(&mut sent).await;
    refused_resume(&mut stranger, &mut sent, resume_init(&key, &proof)).await;
    stranger.close();

    // Another user presenting the id is neither offered nor admitted.
    let mallory = "synthetic-tenant:mallory";
    let (mut other, mut sent) = resumable(registry.clone(), mallory, Some(parked_id.clone()));
    other.open().await;
    assert!(
        session_frame(&mut sent)
            .await
            .get("delegatedResume")
            .is_none()
    );
    refused_resume(&mut other, &mut sent, resume_init(&key, &proof)).await;
    other.close();

    let (mut owner, mut sent) = resumable(registry.clone(), USER, Some(parked_id.clone()));
    owner.open().await;
    let frame = session_frame(&mut sent).await;
    assert_eq!(frame["delegatedResume"], json!([ACCOUNT]));
    assert_eq!(frame["resumeProof"], owner.resume_proof.as_str());
    assert_ne!(
        frame["resumeProof"],
        proof.as_str(),
        "a fresh proof per socket"
    );
    // A missing or wrong in-band proof, identity digest (key), label, venue and
    // settings mismatches, and a sealed resume each fall back without consuming
    // the parked label.
    let mut wrong_label = resume_init(&key, &proof);
    wrong_label["d"]["accountName"] = json!("synthetic-other");
    let mut wrong_venue = resume_init(&key, &proof);
    wrong_venue["d"]["exchangeName"] = json!("bybitinverse");
    let mut wrong_settings = resume_init(&key, &proof);
    wrong_settings["d"]["testnet"] = json!(true);
    let mut sealed = resume_init(&key, &proof);
    sealed["d"]["sealed"] = json!({"attemptId":"synthetic","enc":"AA","ciphertext":"AA"});
    let mut no_proof = resume_init(&key, &proof);
    no_proof["d"].as_object_mut().unwrap().remove("resumeProof");
    let wrong_proof = resume_init(&key, &owner.resume_proof.clone());
    for init in [
        no_proof,
        wrong_proof,
        resume_init("synthetic-rotated-key", &proof),
        wrong_venue,
        wrong_settings,
        sealed,
    ] {
        refused_resume(&mut owner, &mut sent, init).await;
        assert!(owner.joined.is_empty());
    }
    wire(&mut owner, wrong_label).await;
    let refusal = next(&mut sent, "error").await;
    assert_eq!(refusal["d"]["accountName"], "synthetic-other");
    assert_eq!(refusal["d"]["data"][1]["reason"], "resume_unavailable");
    wire(&mut owner, resume_init(&key, &proof)).await;
    let held = registry.held(USER, ACCOUNT, owner.socket).unwrap();
    assert!(Arc::ptr_eq(&account, &held));
    owner.close();

    // A second socket presenting the already-consumed id gets nothing.
    let (mut late, mut sent) = resumable(registry.clone(), USER, Some(parked_id));
    late.open().await;
    assert!(
        session_frame(&mut sent)
            .await
            .get("delegatedResume")
            .is_none()
    );
    refused_resume(&mut late, &mut sent, resume_init(&key, &proof)).await;
    late.close();
}

#[tokio::test]
async fn delegated_resume_after_grace_requires_a_handover() {
    let vector = vectors().remove(0);
    let bytes = plaintext(&vector);
    let key = vector["input"]["key"].as_str().unwrap().to_owned();
    let registry = crate::accounts::testing::e2e_registry_with_grace(
        USER,
        mock(&vector),
        Duration::from_millis(200),
    );
    let (mut first, mut sent) = resumable(registry.clone(), USER, None);
    first.open().await;
    session_frame(&mut sent).await;
    handover(&mut first, &mut sent, &vector, &bytes).await;
    let account = registry.held(USER, ACCOUNT, first.socket).unwrap();
    let (parked_id, proof) = (first.resume_id.clone(), first.resume_proof.clone());
    first.close();
    tokio::time::sleep(Duration::from_millis(400)).await;

    let (mut late, mut sent) = resumable(registry.clone(), USER, Some(parked_id));
    late.open().await;
    assert!(
        session_frame(&mut sent)
            .await
            .get("delegatedResume")
            .is_none()
    );
    refused_resume(&mut late, &mut sent, resume_init(&key, &proof)).await;
    // Retention kept the account; the fallback handover joins the same resident.
    handover(&mut late, &mut sent, &vector, &bytes).await;
    let held = registry.held(USER, ACCOUNT, late.socket).unwrap();
    assert!(Arc::ptr_eq(&account, &held));
    late.close();
}
