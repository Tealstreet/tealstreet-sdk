//! Hosted signing is an explicit event, including bounded failure paths.
#![cfg(not(target_arch = "wasm32"))]
use safe_cex_engine::{PayloadSignRequest, RequestId};
use safe_cex_native::{HostEvent, NetworkTransport, PayloadSignerResult, Transport};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

fn payload() -> PayloadSignRequest {
    PayloadSignRequest {
        exchange_name: "paradex",
        key: "public-key-only".into(),
        payload: "0x123".into(),
        scheme: "stark",
        encoding: "felt-pair",
    }
}
#[tokio::test]
async fn native_signer_receives_the_exact_hash_and_returns_opaque_signature_and_headers() {
    let received = Arc::new(Mutex::new(Vec::new()));
    let captured = received.clone();
    let transport = NetworkTransport::new().with_payload_signer(Arc::new(move |request| {
        captured.lock().unwrap().push(request);
        Box::pin(async {
            Ok(PayloadSignerResult {
                signature: "opaque delegated answer".into(),
                headers: vec![("X-Proof".into(), "proof".into())],
            })
        })
    }));
    let (events, mut inbox) = mpsc::unbounded_channel();
    transport.sign_payload(RequestId(7), payload(), 1000, events);
    match inbox.recv().await.unwrap() {
        HostEvent::PayloadSigned(id, result) => {
            assert_eq!(id, RequestId(7));
            assert_eq!(result.signature, "opaque delegated answer");
            assert_eq!(result.headers, vec![("X-Proof".into(), "proof".into())]);
        }
        other => panic!("unexpected signer event: {other:?}"),
    }
    assert_eq!(*received.lock().unwrap(), vec![payload()]);
}
#[tokio::test]
async fn unconfigured_and_rejected_native_signers_fail_closed() {
    for transport in [
        NetworkTransport::new(),
        NetworkTransport::new()
            .with_payload_signer(Arc::new(|_| Box::pin(async { Err("https://venue.test/private?signature=private-signature Authorization: private-token".into()) }))),
    ] {
        let (events, mut inbox) = mpsc::unbounded_channel();
        transport.sign_payload(RequestId(8), payload(), 1000, events);
        match inbox.recv().await.unwrap() {
            HostEvent::PayloadSignFailed(id, error) => {
                assert_eq!(id, RequestId(8));
                assert!(error == "Payload signing failed" || error == "Payload signing is not configured");
            }
            other => panic!("unexpected signer event: {other:?}"),
        }
    }
}
#[tokio::test(start_paused = true)]
async fn native_signing_deadline_cancels_the_unfinished_signer_future() {
    let transport =
        NetworkTransport::new().with_payload_signer(Arc::new(|_| Box::pin(std::future::pending())));
    let (events, mut inbox) = mpsc::unbounded_channel();
    transport.sign_payload(RequestId(9), payload(), 5, events);
    tokio::time::advance(std::time::Duration::from_millis(6)).await;
    match inbox.recv().await.unwrap() {
        HostEvent::PayloadSignFailed(id, error) => {
            assert_eq!(id, RequestId(9));
            assert_eq!(error, "Payload signing timed out");
        }
        other => panic!("unexpected signer event: {other:?}"),
    }
}

struct SignedVenue;
impl safe_cex_engine::Venue for SignedVenue {
    type Pending = ();
    fn on_request_failed(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: (), error: &str) {
        panic!("unexpected request failure: {error}");
    }
    fn start(&mut self, cx: &mut safe_cex_engine::Cx<'_, ()>) {
        let id = cx.request(
            safe_cex_engine::Method::Post,
            "https://venue.invalid/orders".into(),
            vec![("Authorization", "Bearer venue-jwt".into())],
            Some(r#"{"signature":"0x123"}"#.into()),
            (),
        );
        cx.sign_request(
            id,
            payload(),
            safe_cex_engine::SignatureTarget::JsonField(r#""signature":"0x123""#.into()),
        )
        .unwrap();
    }
    fn on_open(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::SocketId) {}
    fn on_message(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: safe_cex_engine::SocketId,
        _: &str,
    ) {
    }
    fn on_close(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: safe_cex_engine::SocketId,
        _: u16,
    ) {
    }
    fn on_response(
        &mut self,
        _: &mut safe_cex_engine::Cx<'_, ()>,
        _: (),
        _: safe_cex_engine::Response<'_>,
    ) {
    }
    fn on_timer(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::TimerId) {}
    fn on_command(&mut self, _: &mut safe_cex_engine::Cx<'_, ()>, _: safe_cex_engine::Command) {}
    fn book(&self, _: &str) -> Option<&safe_cex_engine::book::Book> {
        None
    }
    fn books(&self) -> Box<dyn Iterator<Item = (&str, &safe_cex_engine::book::Book)> + '_> {
        Box::new(std::iter::empty())
    }
}
struct SignedTransport {
    signer: NetworkTransport,
    outgoing: mpsc::UnboundedSender<safe_cex_engine::HttpRequest>,
}
impl Transport for SignedTransport {
    fn connect(
        &self,
        _: safe_cex_engine::SocketId,
        _: String,
        _: safe_cex_native::Events,
    ) -> mpsc::UnboundedSender<String> {
        mpsc::unbounded_channel().0
    }
    fn sign_payload(
        &self,
        request: RequestId,
        payload: PayloadSignRequest,
        timeout_ms: u32,
        events: safe_cex_native::Events,
    ) {
        self.signer
            .sign_payload(request, payload, timeout_ms, events);
    }
    fn request(&self, request: safe_cex_engine::HttpRequest, _: safe_cex_native::Events) {
        self.outgoing.send(request).unwrap();
    }
}
#[tokio::test]
async fn native_host_resumes_http_only_after_the_signature_event_preserving_jwt() {
    let signer = NetworkTransport::new().with_payload_signer(Arc::new(|request| {
        Box::pin(async move {
            assert_eq!(request, payload());
            Ok(PayloadSignerResult {
                signature: "opaque \"answer\"".into(),
                headers: vec![
                    ("Authorization".into(), "must-not-replace-jwt".into()),
                    ("X-Proof".into(), "proof".into()),
                ],
            })
        })
    }));
    let (outgoing, mut requests) = mpsc::unbounded_channel();
    let (commands, inbox) = mpsc::unbounded_channel();
    let host = tokio::spawn(
        safe_cex_native::Host::new(
            safe_cex_engine::Engine::new(SignedVenue),
            SignedTransport { signer, outgoing },
        )
        .run(inbox, std::time::Duration::from_secs(1), |_| {}, |_| {}),
    );
    let request = tokio::time::timeout(std::time::Duration::from_secs(1), requests.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(request.url, "https://venue.invalid/orders");
    assert_eq!(
        request.body.as_deref(),
        Some(r#"{"signature":"opaque \"answer\""}"#)
    );
    assert_eq!(
        request.headers,
        vec![("Authorization", "Bearer venue-jwt".into())]
    );
    assert_eq!(
        request.signer_headers,
        vec![("X-Proof".into(), "proof".into())]
    );
    assert!(
        request
            .timeout_ms
            .is_some_and(|remaining| remaining <= 20_000)
    );
    drop(commands);
    host.await.unwrap();
}

#[tokio::test]
async fn native_host_disposal_cancels_signing_before_or_during_the_callback() {
    use std::sync::atomic::{AtomicBool, Ordering};
    struct Dropped(Arc<AtomicBool>);
    impl Drop for Dropped {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    for closed_before_start in [true, false] {
        let called = Arc::new(AtomicBool::new(false));
        let dropped = Arc::new(AtomicBool::new(false));
        let callback_called = called.clone();
        let callback_dropped = dropped.clone();
        let transport = NetworkTransport::new().with_payload_signer(Arc::new(move |_| {
            callback_called.store(true, Ordering::SeqCst);
            let guard = Dropped(callback_dropped.clone());
            Box::pin(async move {
                let _guard = guard;
                std::future::pending().await
            })
        }));
        let (events, mut inbox) = mpsc::unbounded_channel();
        if closed_before_start {
            inbox.close();
        }
        transport.sign_payload(RequestId(11), payload(), 20_000, events);
        tokio::task::yield_now().await;
        if closed_before_start {
            assert!(!called.load(Ordering::SeqCst));
        } else {
            assert!(called.load(Ordering::SeqCst));
            drop(inbox);
            tokio::task::yield_now().await;
            assert!(dropped.load(Ordering::SeqCst));
        }
    }
}
