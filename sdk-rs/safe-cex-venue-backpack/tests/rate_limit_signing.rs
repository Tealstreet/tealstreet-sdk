//! The constructed native client must sign queued requests at dequeue time.
#[allow(dead_code)]
#[path = "support/adapter_runtime.rs"]
mod support;
use base64::Engine;
use safe_cex_num::JsJsonValue;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use serde_json::json;
use std::task::Poll;

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
            runtime.poll_checkpoint(pending.as_mut()),
            Poll::Ready(Ok(_))
        ));
    }
    let mut queued = request();
    assert!(runtime.poll_checkpoint(queued.as_mut()).is_pending());
    assert_eq!(wire.requests.lock().unwrap().len(), 10);
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 80_000.0);
    let after_dequeue = VIRTUAL_CLOCK.get_current_time();
    runtime.now.set(2000.0);
    assert!(matches!(
        runtime.poll_checkpoint(queued.as_mut()),
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

#[test]
fn constructed_http_settles_on_runtime_checkpoint_without_advancing_rate_clock() {
    use safe_cex_venue_backpack::native_value::NativeValue;
    let (venue, runtime, _, wire) = support::fixture();
    runtime.now.set(1000.0);
    let before = runtime.now.get().to_bits();
    let mut request = venue.xhr.request(
        "GET",
        "/api/v1/time",
        JsJsonValue::Undefined,
        JsJsonValue::Undefined,
    );
    assert!(
        request
            .as_mut()
            .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
            .is_pending()
    );
    assert!(
        wire.requests.lock().unwrap().is_empty(),
        "the runtime owns the scheduled HTTP execution"
    );
    let value = match runtime.poll_checkpoint(request.as_mut()) {
        Poll::Ready(Ok(value)) => value,
        _ => panic!("the real constructed HTTP client must settle after its runtime checkpoint"),
    };
    assert_eq!(value.field("id").unwrap().utf8().unwrap(), "native-id");
    assert_eq!(wire.requests.lock().unwrap().len(), 1);
    assert_eq!(
        runtime.now.get().to_bits(),
        before,
        "checkpoint draining must not open another rate window"
    );
    venue.dispose().unwrap();
}
