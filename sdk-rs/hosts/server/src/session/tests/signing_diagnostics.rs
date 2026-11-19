use super::*;
use safe_cex_engine::signing::{Algorithm, KeyRef, Operation, PayloadPart, SignError, SignRequest};
use safe_cex_remote_signer::SIGN_TIMEOUT;

fn signing(
    link: Arc<safe_cex_remote_signer::SignerLink>,
) -> tokio::task::JoinHandle<Result<(), SignError>> {
    let account = safe_cex_remote_signer::remote_account(
        link,
        "bybit",
        Some("ACCOUNT_CANARY".into()),
        r#"{"key":"KEY_CANARY"}"#,
    )
    .unwrap();
    let options: Value = serde_json::from_str(&account.options_json).unwrap();
    let key = KeyRef(
        options["signing"]["primary"]["signing"]
            .as_str()
            .unwrap()
            .into(),
    );
    tokio::spawn(async move {
        account
            .provider
            .sign(SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![Operation::Sign {
                    algorithm: Algorithm::HmacSha256,
                    key,
                    payload: vec![PayloadPart::Bytes {
                        bytes: b"BODY_CANARY".to_vec(),
                    }],
                }],
            })
            .await
            .map(|_| ())
    })
}

fn events(lines: Vec<String>) -> Vec<Value> {
    lines
        .into_iter()
        .filter_map(|line| {
            let outer: Value = serde_json::from_str(&line).unwrap();
            serde_json::from_str(outer["msg"].as_str().unwrap()).ok()
        })
        .collect()
}

fn session_signer(
    session: &mut Session,
) -> (
    Arc<safe_cex_remote_signer::SignerLink>,
    tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    let (send, frames) = tokio::sync::mpsc::unbounded_channel();
    let link = crate::signing_diagnostics::link(
        session.socket,
        Arc::new(move |text| send.send(text).is_ok()),
    );
    session.pending_verifications.insert(
        "acct".into(),
        PendingVerification {
            generation: 1,
            task: tokio::spawn(async {}),
            out: session.out.clone(),
            cosigned_link: Some(link.clone()),
            _signer_output: None,
        },
    );
    (link, frames)
}

async fn expired_on(
    link: Arc<safe_cex_remote_signer::SignerLink>,
    frames: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
    heard: bool,
) -> (SignError, String) {
    let task = signing(link.clone());
    let frame: Value = serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
    let id = frame["d"]["id"].as_str().unwrap().to_owned();
    if heard {
        link.heard(link.socket());
    }
    tokio::time::advance(SIGN_TIMEOUT).await;
    (task.await.unwrap().unwrap_err(), id)
}

async fn expired(heard: bool) -> (SignError, String) {
    let (send, mut frames) = tokio::sync::mpsc::unbounded_channel();
    let link = crate::signing_diagnostics::link(7, Arc::new(move |text| send.send(text).is_ok()));
    expired_on(link, &mut frames, heard).await
}

#[tokio::test(start_paused = true)]
async fn timeout_records_request_socket_age_and_last_heard_without_payload() {
    let capture = crate::state::log_capture::capture();
    let (failure, id) = expired(true).await;
    assert_eq!(failure, SignError::TimedOut);
    let lines = capture.finish();
    assert!(!lines.join("").contains("KEY_CANARY"));
    assert!(!lines.join("").contains("BODY_CANARY"));
    let rows = events(lines);
    assert_eq!(
        rows.len(),
        1,
        "an actual unanswered sign needs correlated evidence"
    );
    assert_eq!(rows[0]["event"], "sign.timeout");
    assert_eq!(rows[0]["socket_id"], 7);
    assert_eq!(rows[0]["sent_socket_id"], 7);
    assert_eq!(rows[0]["request_age_ms"], 15000);
    assert_eq!(rows[0]["cause"], "unanswered");
    assert_eq!(
        rows[0]["request_id_hash"],
        safe_cex_crypto::hex(&safe_cex_crypto::sha256(id.as_bytes()))
    );
    assert!(rows[0]["attachment_age_ms"].is_u64());
    assert!(rows[0]["last_heard_age_ms"].is_u64());
}

#[tokio::test(start_paused = true)]
async fn silent_socket_keeps_unavailable_classification_and_records_suspension() {
    let capture = crate::state::log_capture::capture();
    let (failure, _) = expired(false).await;
    assert_eq!(failure, SignError::Unavailable);
    let rows = events(capture.finish());
    assert_eq!(rows.len(), 1, "a silent browser needs evidence too");
    assert_eq!(rows[0]["cause"], "suspended");
}

#[tokio::test(start_paused = true)]
async fn unknown_and_expired_replies_are_counted_without_trusting_their_contents() {
    let (mut session, _sent) = user_session(crate::accounts::tests::registry(), "USER_CANARY");
    let (link, mut frames) = session_signer(&mut session);
    let (_, expired_id) = expired_on(link, &mut frames, true).await;
    let capture = crate::state::log_capture::capture();
    for id in [expired_id.as_str(), "KEY_CANARY\nBODY_CANARY"] {
        session.handle(&json!({"t":"sign-res", "d":{"id":id,"wsAuthSignature":"SIGNATURE_CANARY", "error":"ERROR_CANARY"}}).to_string()).await.unwrap();
        tokio::time::advance(std::time::Duration::from_secs(1)).await;
    }
    let lines = capture.finish();
    for canary in [
        "KEY_CANARY",
        "BODY_CANARY",
        "SIGNATURE_CANARY",
        "ERROR_CANARY",
        "USER_CANARY",
    ] {
        assert!(!lines.join("").contains(canary));
    }
    let rows = events(lines);
    assert_eq!(rows.len(), 2, "unmatched replies must not disappear");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["event"], "sign.response_unmatched");
        assert_eq!(row["socket_id"], session.socket);
        assert_eq!(row["unmatched_count"], i + 1);
        assert!(row["socket_age_ms"].is_u64());
        assert_eq!(row["request_id_hash"].as_str().unwrap().len(), 64);
        assert_eq!(row.as_object().unwrap().len(), 7);
    }
    assert_eq!(
        rows[0]["request_id_hash"],
        safe_cex_crypto::hex(&safe_cex_crypto::sha256(expired_id.as_bytes()))
    );
}

#[tokio::test]
async fn valid_reply_resolves_once_without_timeout_or_extra_send() {
    let capture = crate::state::log_capture::capture();
    let (send, mut frames) = tokio::sync::mpsc::unbounded_channel();
    let link = crate::signing_diagnostics::link(7, Arc::new(move |text| send.send(text).is_ok()));
    let task = signing(link.clone());
    let frame: Value = serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
    let id = frame["d"]["id"].clone();
    let response = json!({"id":id,"wsAuthSignature":"00".repeat(32)});
    assert!(!link.resolve(8, response.clone()));
    assert!(link.resolve(7, response.clone()));
    assert!(task.await.unwrap().is_ok());
    assert!(!link.resolve(7, response));
    assert!(frames.try_recv().is_err());
    assert!(capture.finish().is_empty());
}

#[tokio::test]
async fn takeover_keeps_the_original_socket_fence_and_one_resend() {
    let (send, mut frames) = tokio::sync::mpsc::unbounded_channel();
    let link = crate::signing_diagnostics::link(7, Arc::new(move |text| send.send(text).is_ok()));
    let task = signing(link.clone());
    let original = frames.recv().await.unwrap();
    let frame: Value = serde_json::from_str(&original).unwrap();
    let id = frame["d"]["id"].clone();
    let (send, mut resumed) = tokio::sync::mpsc::unbounded_channel();
    link.suspend();
    link.attach(8, Arc::new(move |text| send.send(text).is_ok()));
    assert_eq!(resumed.recv().await.unwrap(), original);
    assert!(!link.resolve(7, json!({"id":id,"wsAuthSignature":"00".repeat(32)})));
    assert!(link.resolve(8, json!({"id":id,"wsAuthSignature":"00".repeat(32)})));
    assert!(task.await.unwrap().is_ok());
    assert!(resumed.try_recv().is_err());
}

#[tokio::test]
async fn accepted_session_reply_is_not_counted_as_unmatched() {
    let (mut session, _sent) = user_session(crate::accounts::tests::registry(), "USER_CANARY");
    let (send, mut frames) = tokio::sync::mpsc::unbounded_channel();
    let link = crate::signing_diagnostics::link(
        session.socket,
        Arc::new(move |text| send.send(text).is_ok()),
    );
    session.pending_verifications.insert(
        "acct".into(),
        PendingVerification {
            generation: 1,
            task: tokio::spawn(async {}),
            out: session.out.clone(),
            cosigned_link: Some(link.clone()),
            _signer_output: None,
        },
    );
    let task = signing(link);
    let frame: Value = serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
    let capture = crate::state::log_capture::capture();
    session
        .handle(
            &json!({"t":"sign-res", "d":{"id":frame["d"]["id"],"wsAuthSignature":"00".repeat(32)}})
                .to_string(),
        )
        .await
        .unwrap();
    assert!(task.await.unwrap().is_ok());
    assert!(capture.finish().is_empty());
    assert_eq!(session.sign_responses.count(), 0);
}

#[tokio::test(start_paused = true)]
async fn timeout_after_takeover_names_both_original_and_current_socket() {
    let capture = crate::state::log_capture::capture();
    let (send, mut frames) = tokio::sync::mpsc::unbounded_channel();
    let link = crate::signing_diagnostics::link(7, Arc::new(move |text| send.send(text).is_ok()));
    let task = signing(link.clone());
    let original = frames.recv().await.unwrap();
    let (send, mut resumed) = tokio::sync::mpsc::unbounded_channel();
    tokio::time::advance(std::time::Duration::from_secs(8)).await;
    link.suspend();
    link.attach(8, Arc::new(move |text| send.send(text).is_ok()));
    assert_eq!(resumed.recv().await.unwrap(), original);
    link.heard(8);
    tokio::time::advance(std::time::Duration::from_secs(7)).await;
    assert_eq!(task.await.unwrap(), Err(SignError::TimedOut));
    let rows = events(capture.finish());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["sent_socket_id"], 7);
    assert_eq!(rows[0]["socket_id"], 8);
    assert_eq!(rows[0]["request_age_ms"], 15000);
}

#[tokio::test]
async fn malformed_reply_id_is_counted_but_never_serialized() {
    let (mut session, _sent) = user_session(crate::accounts::tests::registry(), "USER_CANARY");
    let capture = crate::state::log_capture::capture();
    session.handle(r#"{"t":"sign-res","d":{"id":{"secret":"KEY_CANARY"},"wsAuthSignature":"SIGNATURE_CANARY"}}"#).await.unwrap();
    let lines = capture.finish();
    assert!(!lines.join("").contains("CANARY"));
    let rows = events(lines);
    assert_eq!(rows.len(), 1);
    assert!(rows[0]["request_id_hash"].is_null());
    assert_eq!(rows[0]["unmatched_count"], 1);
}

#[tokio::test(start_paused = true)]
async fn unmatched_flood_is_coalesced_and_final_count_survives_socket_close() {
    let (mut session, _sent) = user_session(crate::accounts::tests::registry(), "USER_CANARY");
    let capture = crate::state::log_capture::capture();
    for _ in 0..1000 {
        session.handle(r#"{"t":"sign-res","d":{"id":"SECRET_CANARY","wsAuthSignature":"SIGNATURE_CANARY"}}"#).await.unwrap();
    }
    assert_eq!(session.sign_responses.count(), 1000);
    drop(session);
    let lines = capture.finish();
    assert!(!lines.join("").contains("CANARY"));
    let rows = events(lines);
    assert_eq!(
        rows.len(),
        2,
        "one sample and one close summary, not one line per frame"
    );
    assert_eq!(rows[0]["unmatched_count"], 1);
    assert_eq!(rows[1]["unmatched_count"], 1000);
    assert_eq!(rows[1]["coalesced_count"], 999);
    assert_eq!(rows[1]["summary"], true);
    assert!(rows[1]["request_id_hash"].is_null());
}

#[tokio::test(start_paused = true)]
async fn old_socket_late_reply_does_not_count_against_the_new_owner() {
    let registry = crate::accounts::tests::registry();
    let (mut old, _old_sent) = user_session(registry.clone(), "USER_CANARY");
    let (mut new, _new_sent) = user_session(registry, "USER_CANARY");
    let (link, mut frames) = session_signer(&mut old);
    let (send, mut resumed) = tokio::sync::mpsc::unbounded_channel();
    link.attach(new.socket, Arc::new(move |text| send.send(text).is_ok()));
    let pending = old.pending_verifications.remove("acct").unwrap();
    new.pending_verifications.insert("acct".into(), pending);
    let task = signing(link);
    let frame: Value = serde_json::from_str(&resumed.recv().await.unwrap()).unwrap();
    let response =
        json!({"t":"sign-res","d":{"id":frame["d"]["id"],"wsAuthSignature":"00".repeat(32)}})
            .to_string();
    let capture = crate::state::log_capture::capture();
    old.handle(&response).await.unwrap();
    assert!(!task.is_finished());
    new.handle(&response).await.unwrap();
    assert!(task.await.unwrap().is_ok());
    assert_eq!(old.sign_responses.count(), 1);
    assert_eq!(new.sign_responses.count(), 0);
    let rows = events(capture.finish());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["socket_id"], old.socket);
    assert!(frames.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn join_proof_timeout_and_late_reply_never_log_proof_or_identity_canaries() {
    let (mut session, _sent) = user_session(crate::accounts::tests::registry(), "USER_CANARY");
    let (link, mut frames) = session_signer(&mut session);
    let running = link.clone();
    let task = tokio::spawn(async move {
        running
            .join_proof(
                "JOIN_ID_CANARY",
                &json!({"id":"JOIN_ID_CANARY","key":"KEY_CANARY","body":"BODY_CANARY"}),
            )
            .await
    });
    frames.recv().await.unwrap();
    link.heard(session.socket);
    let capture = crate::state::log_capture::capture();
    tokio::time::advance(SIGN_TIMEOUT).await;
    assert!(task.await.unwrap().is_err());
    session.handle(r#"{"t":"sign-res","d":{"id":"JOIN_ID_CANARY","signature":"SIGNATURE_CANARY","key":"KEY_CANARY","body":"BODY_CANARY","user":"USER_CANARY"}}"#).await.unwrap();
    let lines = capture.finish();
    assert!(!lines.join("").contains("CANARY"));
    let rows = events(lines);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["request_id_hash"], rows[1]["request_id_hash"]);
    assert_eq!(rows[0]["event"], "sign.timeout");
    assert_eq!(rows[1]["event"], "sign.response_unmatched");
}

#[tokio::test]
async fn accepted_join_proof_is_not_reported_or_counted() {
    let (mut session, _sent) = user_session(crate::accounts::tests::registry(), "USER_CANARY");
    let (link, mut frames) = session_signer(&mut session);
    let task = tokio::spawn(async move {
        link.join_proof("JOIN_ID_CANARY", &json!({"id":"JOIN_ID_CANARY"}))
            .await
    });
    frames.recv().await.unwrap();
    let capture = crate::state::log_capture::capture();
    session.handle(r#"{"t":"sign-res","d":{"id":"JOIN_ID_CANARY","signature":"SIGNATURE_CANARY","body":"BODY_CANARY"}}"#).await.unwrap();
    assert!(task.await.unwrap().is_ok());
    assert_eq!(session.sign_responses.count(), 0);
    assert!(capture.finish().is_empty());
}
