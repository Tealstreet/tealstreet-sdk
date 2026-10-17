//! Observe timestamp acquisition through the constructed venue's private read.
#[allow(dead_code)]
#[path = "support/venue_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use serde_json::json;
use std::task::{Context, Poll, Waker};

#[test]
fn queued_bitunix_request_signs_after_the_rate_window_opens() {
    let (venue, runtime, _, wire) = support::fixture_with(false);
    wire.bodies.lock().unwrap().insert(
        "/api/v1/futures/account".into(),
        json!({"code":1,"msg":"fixture denial"}),
    );
    runtime.now.set(1000.0);
    for _ in 0..5 {
        let mut pending = venue.validate_account();
        assert!(matches!(
            pending
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop())),
            Poll::Ready(Ok(_))
        ));
    }
    let mut queued = venue.validate_account();
    assert!(
        queued
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert_eq!(wire.requests.lock().unwrap().len(), 5);
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
    assert_eq!(sent.len(), 6);
    let timestamp = sent[5].headers["timestamp"].parse::<f64>().unwrap();
    assert!(
        timestamp >= after_dequeue,
        "queued request signs from the synced clock after the rate wait: {timestamp} < {after_dequeue}"
    );
}
