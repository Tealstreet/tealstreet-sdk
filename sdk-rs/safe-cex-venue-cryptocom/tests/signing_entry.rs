use safe_cex_store::virtual_clock::VirtualClock;
use safe_cex_venue_cryptocom::signing::{generate_authenticated_request, generate_ws_auth_message};
use safe_cex_venue_cryptocom::signing_value::SigningValue;
use serde_json::json;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn counted_clock() -> (VirtualClock, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut clock = VirtualClock::default();
    clock.now = {
        let calls = calls.clone();
        Arc::new(move || 1000.0 + calls.fetch_add(1, Ordering::SeqCst) as f64)
    };
    (clock, calls)
}

#[test]
fn request_clock_reads_happen_before_return() {
    let (clock, calls) = counted_clock();
    let params = SigningValue::from_value(json!({}));
    let result = generate_authenticated_request(
        "private/test",
        params.clone(),
        "key",
        "secret",
        None,
        &clock,
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2, "AUTH-REQUEST-EAGER-CLOCK");
    drop(result);
    let result = generate_authenticated_request(
        "private/test",
        params.clone(),
        "key",
        "secret",
        Some(37.0),
        &clock,
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "AUTH-REQUEST-EAGER-EXPLICIT-NONCE"
    );
    drop(result);
    let invalid = SigningValue::from_value(json!({"a":{"b":{"c":{"toString":0}}}}));
    let result = generate_authenticated_request(
        "private/test",
        invalid.clone(),
        "key",
        "secret",
        Some(37.0),
        &clock,
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        4,
        "AUTH-REQUEST-EAGER-ERROR-CLOCK"
    );
    drop(result);
}

#[test]
fn websocket_clock_reads_happen_before_return() {
    let (clock, calls) = counted_clock();
    let result = generate_ws_auth_message("key", "secret", &clock);
    assert_eq!(calls.load(Ordering::SeqCst), 2, "AUTH-WS-EAGER-CLOCK");
    drop(result);
}
