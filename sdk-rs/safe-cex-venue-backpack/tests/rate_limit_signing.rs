//! The constructed native client must sign queued requests at dequeue time.
#[allow(dead_code)]
#[path = "support/adapter_runtime.rs"]
mod support;
use base64::Engine;
use safe_cex_num::JsJsonValue;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use serde_json::json;
use std::task::{Context, Poll, Waker};

#[test]
fn queued_backpack_request_signs_after_the_rate_window_opens() {
    let secret = base64::engine::general_purpose::STANDARD.encode([1; 32]);
    let (venue, runtime, _, wire) = support::fixture_options(
        serde_json::from_value(json!({"key":"fixture-key","secret":secret})).unwrap(),
    );
    runtime.now.set(1000.0);
    let request = || {
        venue.xhr.request(
            "GET",
            "/api/v1/capital",
            JsJsonValue::Undefined,
            JsJsonValue::Undefined,
        )
    };
    for _ in 0..10 {
        let mut pending = request();
        assert!(matches!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Ok(_))
        ));
    }
    let mut queued = request();
    assert!(
        queued
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(wire.requests.lock().unwrap().len(), 10);
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 80_000.0);
    let after_dequeue = VIRTUAL_CLOCK.get_current_time();
    runtime.now.set(2000.0);
    assert!(matches!(
        queued
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop())),
        Poll::Ready(Ok(_))
    ));
    let sent = wire.requests.lock().unwrap();
    assert_eq!(sent.len(), 11);
    let first = sent[0].headers["X-Timestamp"].parse::<f64>().unwrap();
    let queued = sent[10].headers["X-Timestamp"].parse::<f64>().unwrap();
    assert!(first < after_dequeue);
    assert!(
        queued >= after_dequeue,
        "queued request signs from the synced clock after its rate slot opens: {queued} < {after_dequeue}"
    );
}
