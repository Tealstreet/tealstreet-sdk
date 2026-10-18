use super::*;
use crate::runtime::{ExchangeError, ExchangeFuture};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, pin::Pin};

struct Host(RefCell<VirtualClock>);
impl ProxyTokenHost for Host {
    fn is_main_window(&self) -> bool {
        false
    }
    fn has_self(&self) -> bool {
        false
    }
    fn origin(&self) -> Option<String> {
        None
    }
    fn virtual_clock(&self) -> VirtualClock {
        self.0.borrow().clone()
    }
    fn initialize_broadcast(
        &self,
        _: &str,
        _: Rc<dyn Fn(BroadcastMessage)>,
    ) -> Result<bool, String> {
        unreachable!()
    }
    fn broadcast(&self, _: BroadcastMessage) -> Result<(), String> {
        unreachable!()
    }
    fn fetch<'a>(&'a self, _: String, _: bool) -> ExchangeFuture<'a, Option<String>> {
        unreachable!()
    }
}
struct Runtime;
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        0.0
    }
    fn now_ms(&self) -> f64 {
        0.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        unreachable!()
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        unreachable!()
    }
    fn console_values(&self, _: &str, _: &[crate::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        unreachable!()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &crate::js_runtime::value::Js, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {
        unreachable!()
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        unreachable!()
    }
}
fn token(payload: &str) -> String {
    format!(
        "{}.{}.signature",
        URL_SAFE_NO_PAD.encode(r#"{"alg":"HS256"}"#),
        URL_SAFE_NO_PAD.encode(payload)
    )
}

#[test]
fn expiration_retains_javascript_coercion_and_throw_boundaries() {
    for (payload, expected) in [
        (json!({"exp":"0x10"}), 16000.0),
        (json!({"exp":[2]}), 2000.0),
        (json!({"exp":"\u{FEFF}2\u{FEFF}"}), 2000.0),
        (json!({"exp":{"toString":null}}), 0.0),
    ] {
        assert_eq!(
            ProxyTokenClient::get_token_expiration(&token(&payload.to_string())),
            expected
        );
    }
    assert!(ProxyTokenClient::get_token_expiration(&token(r#"{"exp":"\u00852"}"#)).is_nan());
    assert!(ProxyTokenClient::get_token_expiration(&token(r#"{"exp":"inf"}"#)).is_nan());
    assert_eq!(js_number_text("0x200000000000011"), 144115188075855900.0);
}

#[test]
#[ignore = "live TS expiry function oracle; CI runs all core tests with --include-ignored"]
fn token_expiry_matches_live_typescript_functions() {
    assert_token_oracle(false);
}
#[test]
#[ignore = "actual jsonwebtoken and TS codec oracle; CI runs include-ignored"]
fn jwt_codec_matches_live_jsonwebtoken_and_typescript() {
    assert_token_oracle(true);
}
fn assert_token_oracle(codec: bool) {
    let mut command = std::process::Command::new("node");
    command.arg("scripts/proxy-token-oracle.cjs");
    if codec {
        command.arg("--codec");
    }
    let output = command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("Node is required for the live token oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let host = Rc::new(Host(RefCell::new(VirtualClock::default())));
    let client = ProxyTokenClient::new(host.clone(), Rc::new(Runtime));
    assert_eq!(
        oracle["cases"].as_array().unwrap().len(),
        if codec { 2817 } else { 486 },
        "TOKEN-CARDINALITY source cases"
    );
    for case in oracle["cases"].as_array().unwrap() {
        let token = case["token"].as_str().unwrap();
        let expected: f64 = case["expiration"].as_str().unwrap().parse().unwrap();
        let actual = ProxyTokenClient::get_token_expiration(token);
        assert!(
            actual == expected || actual.is_nan() && expected.is_nan(),
            "expiration case={case} actual={actual}"
        );
        if actual == 0.0 {
            assert_eq!(
                actual.is_sign_negative(),
                case["negativeZero"].as_bool().unwrap(),
                "zero sign case={case}"
            );
        }
        let now = case["now"].as_f64().unwrap();
        host.0.borrow_mut().now = std::sync::Arc::new(move || now);
        assert_eq!(
            is_token_expired(token, &host.virtual_clock()),
            case["expired"].as_bool().unwrap(),
            "shared helper case={case}"
        );
        assert_eq!(
            client.token_expired(token),
            case["expired"].as_bool().unwrap(),
            "expiry case={case}"
        );
    }
}

#[test]
fn shared_expiry_uses_service_clock_offset_and_centralizes_malformed_tokens() {
    let mut clock = VirtualClock::default();
    clock.now = std::sync::Arc::new(|| 1000.0);
    let jwt = token(r#"{"exp":2}"#);
    assert!(!is_token_expired(&jwt, &clock));
    clock.sync_from_server_time(3000.0);
    assert!(is_token_expired(&jwt, &clock));
    let client =
        ProxyTokenClient::new(Rc::new(Host(RefCell::new(clock.clone()))), Rc::new(Runtime));
    assert!(client.token_expired(&jwt));
    for malformed in ["", "garbage", "a.b.c", "a.e30.c"] {
        assert!(is_token_expired(malformed, &clock));
        assert!(client.token_expired(malformed));
    }
}

#[test]
fn jwt_decoder_preserves_jws_alphabet_and_payload_shapes() {
    let mut clock = VirtualClock::default();
    clock.now = std::sync::Arc::new(|| -1.0);
    for payload in ["[]", "true", r#""hello""#, r#""{\"exp\":100}""#] {
        assert!(!is_token_expired(&token(payload), &clock), "{payload}");
    }
    for payload in ["null", "false", "0", r#""""#] {
        assert!(is_token_expired(&token(payload), &clock));
    }
    let valid = token(r#"{"exp":100}"#);
    let invalid = format!("{}.bad!", valid.rsplit_once('.').unwrap().0);
    assert!(is_token_expired(&invalid, &clock));
    assert_eq!(ProxyTokenClient::get_token_expiration(&invalid), 0.0);
    assert!(is_token_expired(&format!("{valid}\n"), &clock));
    let padded = valid.replacen('.', "=.", 1);
    assert!(is_token_expired(&padded, &clock));
}

#[test]
fn dropped_proxy_waiter_releases_waker_while_shared_flight_stays_pending() {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        task::{Context, Wake, Waker},
    };
    struct Counter(AtomicUsize);
    impl Wake for Counter {
        fn wake(self: Arc<Self>) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    let flight = Rc::new(RefCell::new(Flight {
        result: None,
        waiters: crate::waiters::Waiters::default(),
    }));
    let sentinel = Arc::new(Counter(AtomicUsize::new(0)));
    let weak = Arc::downgrade(&sentinel);
    let waker = Waker::from(sentinel);
    let mut waiter = wait_for_flight(flight.clone());
    assert!(
        waiter
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    drop(waker);
    drop(waiter);
    assert!(
        weak.upgrade().is_none(),
        "proxy waiter drop must release its waker before task settlement"
    );
    assert!(
        flight.borrow().result.is_none(),
        "caller cancellation must not settle shared proxy work"
    );
    let survivor = Arc::new(Counter(AtomicUsize::new(0)));
    let waker = Waker::from(survivor.clone());
    let mut waiter = wait_for_flight(flight.clone());
    assert!(
        waiter
            .as_mut()
            .poll(&mut Context::from_waker(&waker))
            .is_pending()
    );
    let waiters = {
        let mut state = flight.borrow_mut();
        state.result = Some("token".into());
        state.waiters.clone()
    };
    waiters.wake_all();
    assert_eq!(survivor.0.load(Ordering::SeqCst), 1);
    assert!(
        matches!(waiter.as_mut().poll(&mut Context::from_waker(&waker)),Poll::Ready(Ok(value)) if value=="token")
    );
}
