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

async fn expired(heard: bool) -> (Arc<safe_cex_remote_signer::SignerLink>, SignError, String) {
    let (send, mut frames) = tokio::sync::mpsc::unbounded_channel();
    let link = crate::signing_diagnostics::link(7, Arc::new(move |text| send.send(text).is_ok()));
    let task = signing(link.clone());
    let frame: Value = serde_json::from_str(&frames.recv().await.unwrap()).unwrap();
    let id = frame["d"]["id"].as_str().unwrap().to_owned();
    if heard {
        link.heard(7);
    }
    tokio::time::advance(SIGN_TIMEOUT).await;
    (link, task.await.unwrap().unwrap_err(), id)
}

#[tokio::test(start_paused = true)]
async fn timeout_records_request_socket_age_and_last_heard_without_payload() {
    let capture = crate::state::log_capture::capture();
    let (_, failure, id) = expired(true).await;
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
    let (_, failure, _) = expired(false).await;
    assert_eq!(failure, SignError::Unavailable);
    let rows = events(capture.finish());
    assert_eq!(rows.len(), 1, "a silent browser needs evidence too");
    assert_eq!(rows[0]["cause"], "suspended");
}

#[tokio::test(start_paused = true)]
async fn unknown_and_expired_replies_are_counted_without_trusting_their_contents() {
    let (mut session, _sent) = user_session(crate::accounts::tests::registry(), "user");
    let (_, _, expired_id) = expired(true).await;
    let capture = crate::state::log_capture::capture();
    for id in [expired_id.as_str(), "KEY_CANARY\nBODY_CANARY"] {
        session.handle(&json!({"t":"sign-res", "d":{"id":id,"wsAuthSignature":"SIGNATURE_CANARY", "error":"ERROR_CANARY"}}).to_string()).await.unwrap();
    }
    let lines = capture.finish();
    for canary in [
        "KEY_CANARY",
        "BODY_CANARY",
        "SIGNATURE_CANARY",
        "ERROR_CANARY",
        "user",
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
        assert_eq!(row.as_object().unwrap().len(), 5);
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
