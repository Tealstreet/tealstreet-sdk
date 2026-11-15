use super::*;
use crate::link::Failure;
use std::{future::Future, task::Poll};
use tokio::sync::mpsc;

fn link() -> (Arc<SignerLink>, mpsc::UnboundedReceiver<String>) {
    let (send, receive) = mpsc::unbounded_channel();
    (
        SignerLink::new(11, Arc::new(move |frame| send.send(frame).is_ok())),
        receive,
    )
}

async fn pending<F: Future>(mut work: std::pin::Pin<&mut F>) {
    assert!(
        std::future::poll_fn(|cx| Poll::Ready(work.as_mut().poll(cx)))
            .await
            .is_pending()
    );
}

fn request() -> Value {
    json!({"id":"proof", "type":"join-proof"})
}

#[tokio::test]
async fn join_proof_keeps_receiving_socket_and_raw_duplicate_fields() {
    let (link, mut sent) = link();
    let request = request();
    let work = link.join_proof("proof", &request);
    tokio::pin!(work);
    pending(work.as_mut()).await;
    assert!(
        sent.try_recv()
            .unwrap()
            .starts_with(r#"{"t":"sign-req","d":"#)
    );
    let raw = r#"{"id":"proof","joinProof":{"version":1,"version":1,"signature":"synthetic"}}"#;
    assert!(
        !link.resolve(11, serde_json::from_str(raw).unwrap()),
        "normal Value routing must never settle a local proof"
    );
    assert!(
        !link.resolve_join_proof(12, raw),
        "foreign socket cannot consume the real wait"
    );
    assert!(link.resolve_join_proof(11, raw));
    assert_eq!(work.await.unwrap(), (11, raw.to_owned()));
    assert!(!link.resolve_join_proof(11, raw), "reply is consumed once");
}

#[tokio::test]
async fn join_proof_never_migrates_or_replays_on_attach_or_suspend() {
    for operation in ["attach", "suspend", "unavailable"] {
        let (link, mut sent) = link();
        let request = request();
        let work = link.join_proof("proof", &request);
        tokio::pin!(work);
        pending(work.as_mut()).await;
        sent.try_recv().unwrap();
        let (send, mut replays) = mpsc::unbounded_channel();
        match operation {
            "attach" => link.attach(12, Arc::new(move |frame| send.send(frame).is_ok())),
            "suspend" => {
                link.suspend();
                link.attach(12, Arc::new(move |frame| send.send(frame).is_ok()));
            }
            _ => link.mark_unavailable(),
        }
        assert_eq!(work.await, Err(Failure::Aborted), "{operation}");
        assert!(replays.try_recv().is_err(), "join proof cannot be resumed");
    }
}

#[tokio::test]
async fn join_proof_and_ordinary_signing_share_the_same_id_reservation() {
    for proof_first in [true, false] {
        let (link, _sent) = link();
        let request = request();
        if proof_first {
            let work = link.join_proof("proof", &request);
            tokio::pin!(work);
            pending(work.as_mut()).await;
            assert_eq!(
                link.round_trip("proof", &request).await,
                Err(Failure::Duplicate)
            );
            link.mark_unavailable();
            assert_eq!(work.await, Err(Failure::Aborted));
        } else {
            let work = link.round_trip("proof", &request);
            tokio::pin!(work);
            pending(work.as_mut()).await;
            assert_eq!(
                link.join_proof("proof", &request).await,
                Err(Failure::Duplicate)
            );
            assert!(!link.resolve_join_proof(11, r#"{"id":"proof"}"#));
            link.mark_unavailable();
            assert_eq!(work.await, Err(Failure::Aborted));
        }
    }
}

#[tokio::test(start_paused = true)]
async fn join_proof_timeout_and_drop_remove_pending_work() {
    let (link, _sent) = link();
    let request = request();
    {
        let work = link.join_proof("proof", &request);
        tokio::pin!(work);
        pending(work.as_mut()).await;
        tokio::time::advance(SIGN_TIMEOUT + std::time::Duration::from_millis(1)).await;
        assert!(matches!(
            work.await,
            Err(Failure::TimedOut | Failure::Suspended)
        ));
    }
    {
        let work = link.join_proof("proof", &request);
        tokio::pin!(work);
        pending(work.as_mut()).await;
    }
    assert!(!link.resolve_join_proof(11, r#"{"id":"proof"}"#));
    assert!(
        link.promote_to_server_owned(),
        "no canceled proof may retain a pending signer job"
    );
}

#[tokio::test]
async fn join_proof_refuses_a_parked_link_without_waiting_for_resume() {
    let (link, _sent) = link();
    link.suspend();
    assert_eq!(
        link.join_proof("proof", &request()).await,
        Err(Failure::Suspended)
    );
}
