use safe_cex_transport::{
    NetworkFailure, RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse,
    TransportError, TransportFuture, rate_limit::RateLimitClock, retry::is_hyperliquid_read,
};
use safe_cex_types::ExchangeOptions;
use safe_cex_venue_hyperliquid::{
    api::{create_exchange_api, create_info_api},
    order_response::*,
    rate_limit::{RateLimiter, get_hyperliquid_rest_request_weight},
    signing::*,
};
use serde_json::Value;
use std::{
    future::Future,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};
fn corpus() -> Value {
    serde_json::from_str(include_str!("fixtures/api.json")).unwrap()
}
fn raw(v: &Value, key: &str) -> Value {
    serde_json::from_str(v[key].as_str().unwrap()).unwrap()
}
fn normalize(v: Value) -> Value {
    match v {
        Value::Number(n) => {
            serde_json::from_str(&safe_cex_num::js_number_string(n.as_f64().unwrap())).unwrap()
        }
        Value::Array(a) => Value::Array(a.into_iter().map(normalize).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalize(v))).collect()),
        v => v,
    }
}
fn run<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected pending test future"),
    }
}
#[derive(Default)]
struct Clock {
    now: AtomicU64,
    sleeps: Mutex<Vec<u64>>,
}
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
    fn sleep_ms(&self, ms: u64) -> TransportFuture<'_, ()> {
        Box::pin(async move {
            self.sleeps.lock().unwrap().push(ms);
            self.now.fetch_add(ms, Ordering::SeqCst);
            Ok(())
        })
    }
}
struct Adapter {
    calls: AtomicUsize,
    fail: bool,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            if self.fail {
                Err(RestFailure::from_transport(
                    TransportError::Network(NetworkFailure::from_code(
                        "reset".into(),
                        Some("ECONNRESET".into()),
                    )),
                    request,
                ))
            } else {
                Ok(RestResponse {
                    reason: None,
                    status: 200,
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Encoded(b"{}".to_vec()),
                })
            }
        })
    }
}
struct TerminalAdapter {
    clock: Arc<Clock>,
    calls: AtomicUsize,
    response: Option<RestResponse>,
}
impl RestAdapter for TerminalAdapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.clock.now.fetch_add(10, Ordering::SeqCst);
            if let Some(response) = &self.response {
                Ok(response.clone())
            } else {
                Err(RestFailure::from_transport(
                    TransportError::Network(NetworkFailure::from_code(
                        "terminal reset".into(),
                        Some("ECONNRESET".into()),
                    )),
                    request,
                ))
            }
        })
    }
}
#[test]
fn retry_budget_uses_the_injected_limiter_clock() {
    let clock = Arc::new(Clock::default());
    clock.now.store(100, Ordering::SeqCst);
    let adapter = Arc::new(TerminalAdapter {
        clock: clock.clone(),
        calls: AtomicUsize::new(0),
        response: None,
    });
    let client = create_info_api(
        options("", false, false),
        adapter.clone(),
        Rc::new(RateLimiter::new(clock.clone(), 1200.0, 60000.0)),
        Arc::new(|_| {}),
    );
    let mut request = client
        .build_request(
            "/info",
            &serde_json::from_str(r#"{"type":"allMids"}"#).unwrap(),
        )
        .unwrap();
    request.timeout_ms = Some(5);
    let failure = run(client.execute(request, false)).unwrap_err();
    assert_eq!(
        adapter.calls.load(Ordering::SeqCst),
        1,
        "expired read budget must not retry"
    );
    assert_eq!(failure.message, "terminal reset");
    assert_eq!(failure.code.as_deref(), Some("ECONNRESET"));
    assert!(clock.sleeps.lock().unwrap().is_empty());
}
#[test]
fn venue_refusal_retains_response_and_signed_request() {
    let clock = Arc::new(Clock::default());
    let response = RestResponse {
        status: 422,
        reason: Some("venue reason".into()),
        headers: [("x-request-id".into(), "r1".into())].into(),
        body: safe_cex_transport::RestResponseBody::Encoded(
            br#"{"status":"err","response":"venue rejection"}"#.to_vec(),
        ),
    };
    let adapter = Arc::new(TerminalAdapter {
        clock: clock.clone(),
        calls: AtomicUsize::new(0),
        response: Some(response.clone()),
    });
    let client = create_exchange_api(
        options("", false, false),
        adapter.clone(),
        Rc::new(RateLimiter::new(clock, 1200.0, 60000.0)),
        Arc::new(|_| {}),
    );
    let body =
        serde_json::from_str(r#"{"action":{"type":"cancel","cancels":[]},"nonce":1}"#).unwrap();
    let request = client.build_request("/exchange", &body).unwrap();
    let failure = run(client.execute(request.clone(), false)).unwrap_err();
    assert_eq!(failure.message, "venue rejection (HTTP 422)");
    assert_eq!(
        failure.response.as_deref(),
        Some(&response),
        "venue refusal must retain terminal response"
    );
    assert_eq!(*failure.request, request);
    assert!(!failure.is_retryable_network());
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 1);
}
#[test]
fn transformed_string_replay_keeps_source_signing_guard() {
    let adapter = Arc::new(Recorder(Mutex::new(Vec::new())));
    let client = create_exchange_api(
        options(&format!("0x{}", "11".repeat(32)), false, false),
        adapter.clone(),
        Rc::new(RateLimiter::new(
            Arc::new(Clock::default()),
            1200.0,
            60000.0,
        )),
        Arc::new(|_| {}),
    );
    let body: Value =
        serde_json::from_str(r#"{"action":{"type":"order","orders":[]},"nonce":1}"#).unwrap();
    let mut request = client.build_request("/exchange", &body).unwrap();
    let original = request
        .body
        .as_ref()
        .unwrap()
        .transform(Some("application/json"))
        .unwrap();
    assert!(matches!(original, RestBody::Transformed(_)));
    let wire = original.wire_bytes().unwrap();
    request.body = Some(original.clone());
    let replay = run(client.prepare_request(request, true)).unwrap();
    assert_eq!(
        replay.body.as_ref(),
        Some(&original),
        "transformed data stays a string, so TS interceptor must skip signing"
    );
    assert_eq!(replay.body.unwrap().wire_bytes().unwrap(), wire);
    let mut encoded = client.build_request("/exchange", &body).unwrap();
    encoded.body = Some(RestBody::Encoded(wire));
    let failure = run(client.execute(encoded, true)).unwrap_err();
    assert_eq!(
        failure.message,
        "Signing requires untransformed request data"
    );
    assert_eq!(
        adapter.0.lock().unwrap().len(),
        0,
        "opaque encoded inputs must not transmit an invented signature"
    );
}
fn options(secret: &str, testnet: bool, public_only: bool) -> ExchangeOptions {
    serde_json::from_str(&format!(r#"{{"key":"wallet","secret":"{secret}","testnet":{testnet},"publicDataOnly":{public_only},"extra":{{"recvWindow":5000}}}}"#)).unwrap()
}
#[test]
fn actual_ts_l1_signatures_bind_composition_and_network() {
    let data = corpus();
    let cases = data["l1"].as_array().unwrap();
    assert!(cases.iter().any(|v| v["vault"] == "0x12"));
    assert!(cases.iter().any(|v| v["nonce"] == -1));
    assert!(cases.iter().any(|v| v["mainnet"] == false));
    for case in cases {
        let action = raw(case, "action");
        let vault = case["vault"].as_str();
        assert_eq!(
            action_hash(&action, vault, &case["nonce"]).unwrap(),
            case["hash"].as_str().unwrap(),
            "actionHash exact msgpack/nonce/vault"
        );
        let sig = sign_l1_action(
            case["key"].as_str().unwrap(),
            &action,
            vault,
            &case["nonce"],
            case["mainnet"].as_bool().unwrap(),
        )
        .unwrap();
        assert_eq!(
            sig.r,
            case["signature"]["r"].as_str().unwrap(),
            "API native signature r identity"
        );
        assert_eq!(
            sig.s,
            case["signature"]["s"].as_str().unwrap(),
            "API native signature s identity"
        );
        assert_eq!(
            normalize(serde_json::to_value(sig).unwrap()),
            case["signature"],
            "actual TS L1 signature"
        );
    }
}
#[test]
fn actual_ts_numeric_and_order_wire_edges() {
    let data = corpus();
    for case in data["floats"].as_array().unwrap() {
        let x = case["raw"].as_str().unwrap().parse::<f64>().unwrap();
        match float_to_wire(x) {
            Ok(wire) => assert_eq!(
                Value::String(wire),
                case["wire"],
                "JS toFixed and trailing zeros"
            ),
            Err(e) => assert_eq!(Value::String(e.to_string()), case["error"]),
        }
    }
    for case in data["wire"].as_array().unwrap() {
        let order = raw(case, "raw");
        match order_request_to_order_wire(&order, 110002.0) {
            Ok(value) => {
                assert_eq!(
                    normalize(serde_json::to_value(&value).unwrap()),
                    case["value"]
                );
                let builder = Some(serde_json::from_str(r#"{"b":"builder","f":10}"#).unwrap());
                let orders = vec![value];
                let actions = [
                    order_wires_to_order_action(orders.clone(), builder.clone()),
                    order_wires_to_normal_tpsl_action(orders.clone(), builder.clone()),
                    order_wires_to_positional_tpsl_action(orders, builder),
                ];
                assert_eq!(
                    normalize(serde_json::to_value(actions).unwrap()),
                    case["actions"]
                );
            }
            Err(e) => assert_eq!(Value::String(e.to_string()), case["error"]),
        }
    }
}
#[test]
fn url_predicates_do_not_conflate_retry_weight_or_signing() {
    for case in corpus()["weights"].as_array().unwrap() {
        let url = case["url"].as_str().unwrap();
        let request = RestRequest {
            method: "POST".into(),
            url: url.into(),
            ..Default::default()
        };
        assert_eq!(
            is_hyperliquid_read(&request),
            case["read"].as_bool().unwrap()
        );
        assert_eq!(
            get_hyperliquid_rest_request_weight(url, &raw(case, "raw")).unwrap(),
            case["weight"].as_f64().unwrap()
        );
    }
}
#[test]
fn actual_ts_relay_allowlist_and_unsigned_info_skip() {
    for case in corpus()["requests"].as_array().unwrap() {
        let emitted = Arc::new(Mutex::new(Vec::<String>::new()));
        let output = emitted.clone();
        let emit = Arc::new(move |s: &str| output.lock().unwrap().push(s.to_owned()));
        let clock = Arc::new(Clock::default());
        let limiter = Rc::new(RateLimiter::new(clock, 1200.0, 60000.0));
        let adapter = Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        });
        let opts = options(
            &format!("0x{}", "11".repeat(32)),
            case["testnet"].as_bool().unwrap(),
            case["publicOnly"].as_bool().unwrap(),
        );
        let client = if case["client"] == "exchange" {
            create_exchange_api(opts, adapter, limiter, emit)
        } else {
            create_info_api(opts, adapter, limiter, emit)
        };
        let request = client
            .build_request(case["url"].as_str().unwrap(), &raw(case, "raw"))
            .unwrap();
        let prepared = run(client.prepare_request(request, false)).unwrap();
        let value: Value =
            serde_json::from_slice(&prepared.body.as_ref().unwrap().wire_bytes().unwrap()).unwrap();
        assert_eq!(normalize(value), case["prepared"], "relay prepared body");
        assert_eq!(
            prepared.skip_remote_signing,
            case["skip"].as_bool().unwrap(),
            "exact URL skip independent of credentials"
        );
        assert_eq!(
            serde_json::to_value(&*emitted.lock().unwrap()).unwrap(),
            case["emitted"]
        );
    }
}
#[test]
fn actual_ts_response_interceptor_and_capture_decisions() {
    for case in corpus()["responses"].as_array().unwrap() {
        let emitted = Arc::new(Mutex::new(Vec::<String>::new()));
        let output = emitted.clone();
        let emit = Arc::new(move |s: &str| output.lock().unwrap().push(s.to_owned()));
        let client = create_exchange_api(
            options("", false, false),
            Arc::new(Adapter {
                calls: AtomicUsize::new(0),
                fail: false,
            }),
            Rc::new(RateLimiter::new(
                Arc::new(Clock::default()),
                1200.0,
                60000.0,
            )),
            emit,
        );
        let request = client
            .build_request(case["url"].as_str().unwrap(), &raw(case, "request"))
            .unwrap();
        let response = RestResponse {
            reason: None,
            status: case["status"].as_u64().unwrap() as u16,
            headers: Default::default(),
            body: safe_cex_transport::RestResponseBody::Encoded(
                case["raw"].as_str().unwrap().as_bytes().to_vec(),
            ),
        };
        let result = client.check_response(&request, response, case["capture"].as_bool().unwrap());
        if let Some(e) = case.get("error") {
            assert_eq!(result.unwrap_err().to_string(), e.as_str().unwrap())
        } else {
            assert!(result.is_ok())
        };
        assert_eq!(
            serde_json::to_value(&*emitted.lock().unwrap()).unwrap(),
            case["emitted"]
        );
    }
}
#[test]
fn actual_ts_order_response_errors_and_terminal_cancel_strings() {
    let data = corpus();
    for case in data["parser"].as_array().unwrap() {
        let body = raw(case, "raw");
        assert_eq!(
            serde_json::to_value(extract_hyperliquid_response_error(&body)).unwrap(),
            case["extracted"]
        );
        assert_eq!(
            hyperliquid_order_response_has_waiting_for_trigger(&body),
            case["waiting"].as_bool().unwrap()
        );
        assert_eq!(
            hyperliquid_order_response_has_deferred_order_status(&body),
            case["deferred"].as_bool().unwrap()
        );
        match parse_hyperliquid_order_response(&body) {
            Ok(parsed) => {
                assert_eq!(
                    serde_json::to_value(parsed.order_ids).unwrap(),
                    case["parsed"]["orderIds"]
                );
                assert_eq!(
                    parsed.waiting_for_trigger,
                    case["parsed"]["waitingForTrigger"].as_bool().unwrap()
                );
            }
            Err(error) => {
                let (message, unreadable) = match error {
                    HyperliquidOrderResponseError::Venue(message) => (message, false),
                    HyperliquidOrderResponseError::Coercion(error) => {
                        (error.message.to_owned(), false)
                    }
                    HyperliquidOrderResponseError::Unreadable(e) => (e.message, true),
                };
                assert_eq!(Value::String(message), case["error"]);
                assert_eq!(unreadable, case["unreadable"].as_bool().unwrap());
            }
        }
    }
    for case in data["terminal"].as_array().unwrap() {
        assert_eq!(
            is_only_hyperliquid_terminal_cancel_error(case["message"].as_str().unwrap()),
            case["only"].as_bool().unwrap(),
            "JS whitespace/ASCII digit terminal predicate"
        )
    }
}
#[test]
fn lost_write_response_never_retries_but_posted_read_does() {
    for (url, expected) in [
        ("/exchange", 1),
        ("/exchange?x=1", 1),
        ("/info", 4),
        ("https://api.hyperliquid.xyz/exchange", 4),
    ] {
        let adapter = Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: true,
        });
        let client = create_info_api(
            options("", false, false),
            adapter.clone(),
            Rc::new(RateLimiter::new(
                Arc::new(Clock::default()),
                1200.0,
                60000.0,
            )),
            Arc::new(|_| {}),
        );
        let request = client
            .build_request(url, &serde_json::from_str(r#"{"type":"allMids"}"#).unwrap())
            .unwrap();
        assert!(run(client.execute(request, false)).is_err());
        assert_eq!(
            adapter.calls.load(Ordering::SeqCst),
            expected,
            "all-POST read retry shape"
        )
    }
}
#[test]
fn shared_weighted_pacing_and_capture_bypass_are_observable() {
    let clock = Arc::new(Clock::default());
    let limiter = Rc::new(RateLimiter::new(clock.clone(), 1200.0, 60000.0));
    run(limiter.wait_for_token(20.0)).unwrap();
    run(limiter.wait_for_token(60.0)).unwrap();
    run(limiter.wait_for_token(2.0)).unwrap();
    assert_eq!(*clock.sleeps.lock().unwrap(), [1000, 3000]);
    let client = create_exchange_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        limiter,
        Arc::new(|_| {}),
    );
    let request = client
        .build_request(
            "/info",
            &serde_json::from_str(r#"{"type":"userRole"}"#).unwrap(),
        )
        .unwrap();
    run(client.prepare_request(request, true)).unwrap();
    assert_eq!(*clock.sleeps.lock().unwrap(), [1000, 3000]);
}
#[test]
fn live_ts_api_oracle_runs_in_normal_cargo_test() {
    assert!(
        std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/api-oracle.cjs"
            ))
            .arg("--check")
            .status()
            .unwrap()
            .success(),
        "live Hyperliquid API/signing oracle drift"
    )
}

#[test]
fn live_ts_numeric_coercion_and_user_signatures() {
    let data: Value = serde_json::from_str(include_str!("fixtures/api-extra.json")).unwrap();
    let limits = data["coercions"].as_array().unwrap();
    for required in ["[60]", "\"0x3c\"", "\"0b111100\"", "\"0o74\"", "1e400"] {
        assert!(
            limits.iter().any(|v| v["raw"] == required),
            "coercion property missing {required}"
        );
    }
    for case in limits {
        let limit = raw(case, "raw");
        let mut body: Value = serde_json::from_str(r#"{"type":"userFills"}"#).unwrap();
        body["limit"] = limit;
        assert_eq!(
            get_hyperliquid_rest_request_weight("/info", &body).unwrap(),
            case["weight"].as_f64().unwrap()
        );
    }
    let nonces = data["nonces"].as_array().unwrap();
    for required in ["\"-0x1\"", "\"+0x1\"", "[1]", "null", "1.5"] {
        assert!(nonces.iter().any(|v| v["raw"] == required));
    }
    for case in nonces {
        let action: Value = serde_json::from_str(r#"{"type":"scheduleCancel"}"#).unwrap();
        let result = action_hash(&action, None, &raw(case, "raw"));
        match result {
            Ok(hash) => assert_eq!(hash, case["hash"].as_str().unwrap()),
            Err(error) => assert_eq!(error, case["error"].as_str().unwrap()),
        }
    }
    let cases = data["user"].as_array().unwrap();
    for scheme in [
        "signUsdTransferAction",
        "signWithdrawFromBridgeAction",
        "signAgent",
    ] {
        for network in [true, false] {
            assert!(
                cases
                    .iter()
                    .any(|v| v["scheme"] == scheme && v["mainnet"] == network)
            );
        }
    }
    for case in cases {
        let mut action = raw(case, "raw");
        let key = case["key"].as_str().unwrap();
        let mainnet = case["mainnet"].as_bool().unwrap();
        let signature = match case["scheme"].as_str().unwrap() {
            "signUsdTransferAction" => sign_usd_transfer_action(key, &mut action, mainnet),
            "signWithdrawFromBridgeAction" => {
                sign_withdraw_from_bridge_action(key, &mut action, mainnet)
            }
            "signAgent" => sign_agent(key, &mut action, mainnet),
            _ => panic!("unknown oracle scheme"),
        }
        .unwrap();
        assert_eq!(
            normalize(serde_json::to_value(signature).unwrap()),
            case["signature"]
        );
        assert_eq!(action, case["mutated"]);
    }
    for case in data["ints"].as_array().unwrap() {
        let x = case["raw"].as_str().unwrap().parse().unwrap();
        let result = if case["scheme"] == "floatToUsdInt" {
            float_to_usd_int(x)
        } else {
            float_to_int_for_hashing(x)
        };
        match result {
            Ok(n) => assert_eq!(
                safe_cex_num::js_number_string(n),
                case["result"].as_str().unwrap()
            ),
            Err(e) => assert_eq!(e, case["error"].as_str().unwrap()),
        }
    }
}

#[test]
fn live_ts_diagnostic_json_and_utf16_bound() {
    // The final split-surrogate source row cannot be decoded into Value.
    // Its native/source outcomes are asserted separately below without rewriting
    // source D83D into the native replacement character.
    let raw_cases: Vec<Box<serde_json::value::RawValue>> =
        serde_json::from_str(include_str!("fixtures/diagnostics.json")).unwrap();
    assert_eq!(
        raw_cases.len(),
        6,
        "DIAGNOSTIC independent source fixture floor"
    );
    let cases: Vec<Value> = raw_cases
        .iter()
        .take(5)
        .map(|raw| serde_json::from_str(raw.get()).unwrap())
        .collect();
    assert!(
        cases
            .iter()
            .any(|v| v["raw"].as_str().unwrap().contains("1e400"))
    );
    assert!(raw_cases[5].get().contains('😀'));
    for case in &cases {
        let body = raw(case, "raw");
        assert_eq!(
            serde_json::to_value(extract_hyperliquid_response_error(&body)).unwrap(),
            case["extracted"]
        );
        match parse_hyperliquid_order_response(&body) {
            Ok(parsed) => assert_eq!(
                serde_json::to_value(parsed.order_ids).unwrap(),
                case["parsed"]["orderIds"]
            ),
            Err(e) => {
                let text = match e {
                    HyperliquidOrderResponseError::Venue(v) => v,
                    HyperliquidOrderResponseError::Coercion(error) => error.message.to_owned(),
                    HyperliquidOrderResponseError::Unreadable(v) => v.message,
                };
                assert_eq!(text, case["parseError"].as_str().unwrap());
            }
        }
        let emitted = Arc::new(Mutex::new(Vec::<String>::new()));
        let output = emitted.clone();
        let client = create_exchange_api(
            options("", false, false),
            Arc::new(Adapter {
                calls: AtomicUsize::new(0),
                fail: false,
            }),
            Rc::new(RateLimiter::new(
                Arc::new(Clock::default()),
                1200.0,
                60000.0,
            )),
            Arc::new(move |s| output.lock().unwrap().push(s.to_owned())),
        );
        let request = client
            .build_request(
                "/exchange",
                &serde_json::from_str(r#"{"action":{"type":"order"}}"#).unwrap(),
            )
            .unwrap();
        let response = RestResponse {
            reason: None,
            status: 422,
            headers: Default::default(),
            body: safe_cex_transport::RestResponseBody::Encoded(
                case["raw"].as_str().unwrap().as_bytes().to_vec(),
            ),
        };
        assert_eq!(
            client
                .check_response(&request, response, false)
                .unwrap_err()
                .to_string(),
            case["httpError"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::to_value(&*emitted.lock().unwrap()).unwrap(),
            case["emitted"]
        );
    }
}

#[derive(Default)]
struct PendingClock {
    now: AtomicU64,
    sleeps: Mutex<Vec<u64>>,
    released: std::sync::atomic::AtomicBool,
}
impl RateLimitClock for PendingClock {
    fn now_ms(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }
    fn sleep_ms(&self, ms: u64) -> TransportFuture<'_, ()> {
        Box::pin(std::future::poll_fn(move |_| {
            if self.released.load(Ordering::SeqCst) {
                self.now.fetch_add(ms, Ordering::SeqCst);
                Poll::Ready(Ok(()))
            } else {
                self.sleeps.lock().unwrap().push(ms);
                Poll::Pending
            }
        }))
    }
}
#[derive(Default)]
struct WakeCounter(AtomicUsize);
impl Wake for WakeCounter {
    fn wake(self: Arc<Self>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}
#[test]
fn cancellation_removes_only_its_ticket_and_preserves_reserved_budget() {
    let clock = Arc::new(PendingClock::default());
    let limiter = RateLimiter::new(clock.clone(), 1200.0, 60000.0);
    // Unpolled construction owns no ticket or budget.
    drop(limiter.wait_for_token(100.0));
    run(limiter.wait_for_token(20.0)).unwrap();
    let wakes = Arc::new(WakeCounter::default());
    let waker = Waker::from(wakes.clone());
    let mut cx = Context::from_waker(&waker);
    let mut front = limiter.wait_for_token(60.0);
    assert!(front.as_mut().poll(&mut cx).is_pending());
    let mut abandoned = limiter.wait_for_token(500.0);
    assert!(abandoned.as_mut().poll(&mut cx).is_pending());
    let mut follower = limiter.wait_for_token(2.0);
    assert!(follower.as_mut().poll(&mut cx).is_pending());
    drop(abandoned);
    drop(front);
    assert_eq!(
        wakes.0.load(Ordering::SeqCst),
        1,
        "drop must wake queued follower on a real executor"
    );
    assert!(follower.as_mut().poll(&mut cx).is_pending());
    assert_eq!(
        *clock.sleeps.lock().unwrap(),
        [1000, 4000],
        "dropped front retains reservation, queued cancellation charges nothing"
    );
    let mut next = limiter.wait_for_token(1.0);
    assert!(next.as_mut().poll(&mut cx).is_pending());
    clock.released.store(true, Ordering::SeqCst);
    assert!(follower.as_mut().poll(&mut cx).is_ready());
    assert_eq!(
        wakes.0.load(Ordering::SeqCst),
        2,
        "completion releases/wakes next exactly once"
    );
    drop(follower);
    assert_eq!(wakes.0.load(Ordering::SeqCst), 2);
    assert!(next.as_mut().poll(&mut cx).is_ready());
    drop(next);
    assert_eq!(
        clock.now.load(Ordering::SeqCst),
        4100,
        "completed ticket removed exactly once"
    );
}
#[test]
fn corpus_properties_pin_signing_and_response_branches() {
    let data = corpus();
    let requests = data["requests"].as_array().unwrap();
    for kind in [
        "order",
        "cancel",
        "cancelByCloid",
        "modify",
        "batchModify",
        "updateLeverage",
        "updateIsolatedMargin",
        "scheduleCancel",
        "usdSend",
        "withdraw3",
        "vaultTransfer",
    ] {
        for client in ["info", "exchange"] {
            for public in [true, false] {
                for testnet in [true, false] {
                    assert!(
                        requests.iter().any(|v| v["client"] == client
                            && v["publicOnly"] == public
                            && v["testnet"] == testnet
                            && raw(v, "raw")["action"]["type"] == kind),
                        "relay property {kind}/{client}/{public}/{testnet}"
                    );
                }
            }
        }
    }
    let cases = data["responses"].as_array().unwrap();
    for url in ["/exchange", "/info", "/exchange?x=1"] {
        for status in [200, 422] {
            for capture in [true, false] {
                for raw in ["null", "{}", "\"\""] {
                    assert!(cases.iter().any(|v| v["url"] == url
                        && v["status"] == status
                        && v["capture"] == capture
                        && v["raw"] == raw));
                }
            }
        }
    }
    let l1 = data["l1"].as_array().unwrap();
    assert!(
        l1.iter()
            .any(|v| raw(v, "action").as_object().unwrap().keys().next().unwrap() == "grouping")
    );
    let terminal = data["terminal"].as_array().unwrap();
    for token in ['\u{feff}', '\u{0085}', '١'] {
        assert!(
            terminal
                .iter()
                .any(|v| v["message"].as_str().unwrap().contains(token))
        );
    }
}

#[test]
fn order_error_telemetry_is_optional_and_terminal_cancel_stays_quiet() {
    let events = Arc::new(Mutex::new(Vec::<(String, Option<Value>)>::new()));
    let output = events.clone();
    let client = create_exchange_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::new(
            Arc::new(Clock::default()),
            1200.0,
            60000.0,
        )),
        Arc::new(|_| panic!("telemetry emitter should handle venue error")),
    )
    .with_order_error_telemetry(
        Arc::new(|request, message| {
            assert_eq!(request["action"]["type"], "cancel");
            assert_eq!(message, "bad price");
            Some(serde_json::from_str(r#"{"category":"order"}"#).unwrap())
        }),
        Arc::new(move |message, context| {
            output
                .lock()
                .unwrap()
                .push((message.to_owned(), context.cloned()))
        }),
    );
    let request = client
        .build_request(
            "/exchange",
            &serde_json::from_str(r#"{"action":{"type":"cancel"}}"#).unwrap(),
        )
        .unwrap();
    for body in [
        r#"{"status":"err","response":"Order was never placed, already canceled, or filled. asset=0"}"#,
        r#"{"status":"err","response":"bad price"}"#,
    ] {
        assert!(
            client
                .check_response(
                    &request,
                    RestResponse {
                        reason: None,
                        status: 200,
                        headers: Default::default(),
                        body: safe_cex_transport::RestResponseBody::Encoded(
                            body.as_bytes().to_vec()
                        )
                    },
                    false
                )
                .is_err()
        );
    }
    assert_eq!(events.lock().unwrap().len(), 1);
    assert_eq!(
        events.lock().unwrap()[0].1.as_ref().unwrap()["category"],
        "order"
    );
}
struct Recorder(Mutex<Vec<RestRequest>>);
impl RestAdapter for Recorder {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            self.0.lock().unwrap().push(request);
            Ok(RestResponse {
                reason: None,
                status: 200,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(b"{}".to_vec()),
            })
        })
    }
}
#[test]
fn signed_action_order_is_exactly_the_sent_action_order() {
    let adapter = Arc::new(Recorder(Mutex::new(Vec::new())));
    let client = create_exchange_api(
        options(&format!("0x{}", "11".repeat(32)), false, false),
        adapter.clone(),
        Rc::new(RateLimiter::new(
            Arc::new(Clock::default()),
            1200.0,
            60000.0,
        )),
        Arc::new(|_| {}),
    );
    let body: Value =
        serde_json::from_str(r#"{"action":{"z":1,"type":"order","a":2},"nonce":1700000000000}"#)
            .unwrap();
    run(client.execute(client.build_request("/exchange", &body).unwrap(), false)).unwrap();
    let requests = adapter.0.lock().unwrap();
    let bytes = requests[0].body.as_ref().unwrap().wire_bytes().unwrap();
    let sent: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(std::str::from_utf8(&bytes).unwrap().starts_with(
        r#"{"action":{"z":1,"type":"order","a":2},"nonce":1700000000000,"signature":"#
    ));
    let expected = sign_l1_action(
        &format!("0x{}", "11".repeat(32)),
        &body["action"],
        None,
        &body["nonce"],
        true,
    )
    .unwrap();
    assert_eq!(
        normalize(serde_json::to_value(expected).unwrap()),
        normalize(sent["signature"].clone())
    );
    assert_eq!(requests[0].method, "POST");
    assert_eq!(requests[0].timeout_ms, Some(5000));
    assert_eq!(
        requests[0].headers["Content-Type"],
        "application/json, charset=utf-8"
    );
}

#[test]
fn sign_only_bigint_strings_are_rejected_but_blank_strings_hash_zero() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/nonce-refusals.json")).unwrap();
    for required in ["+", "-", " + ", "", " "] {
        assert!(
            cases
                .as_array()
                .unwrap()
                .iter()
                .any(|v| raw(v, "raw") == required)
        );
    }
    for case in cases.as_array().unwrap() {
        let action: Value = serde_json::from_str(r#"{"type":"scheduleCancel"}"#).unwrap();
        match action_hash(&action, None, &raw(case, "raw")) {
            Ok(hash) => assert_eq!(hash, case["hash"].as_str().unwrap()),
            Err(error) => assert_eq!(error, case["error"].as_str().unwrap()),
        }
    }
}

#[test]
fn order_numeric_fields_refuse_coercion_as_typescript_tofixed_does() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/wire-refusals.json")).unwrap();
    for field in ["sz", "limit_px", "triggerPx"] {
        assert!(
            cases
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["field"] == field)
        );
    }
    for case in cases.as_array().unwrap() {
        assert_eq!(
            order_request_to_order_wire(&raw(case, "raw"), 1.0)
                .unwrap_err()
                .to_string(),
            case["error"].as_str().unwrap()
        );
    }
}

#[test]
fn source_bound_relay_guards_preserve_existing_signatures_and_capture_signing() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/request-guards.json")).unwrap();
    for key in ["signature", "nonce", "action"] {
        assert!(
            cases
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["raw"].as_str().unwrap().contains(key))
        );
    }
    for case in cases.as_array().unwrap() {
        let emitted = Arc::new(Mutex::new(Vec::<String>::new()));
        let output = emitted.clone();
        let client = create_exchange_api(
            options(case["secret"].as_str().unwrap(), false, false),
            Arc::new(Adapter {
                calls: AtomicUsize::new(0),
                fail: false,
            }),
            Rc::new(RateLimiter::new(
                Arc::new(Clock::default()),
                1200.0,
                60000.0,
            )),
            Arc::new(move |s| output.lock().unwrap().push(s.to_owned())),
        );
        let request = client
            .build_request("/exchange", &raw(case, "raw"))
            .unwrap();
        match run(client.prepare_request(request, case["capture"].as_bool().unwrap())) {
            Ok(prepared) => assert_eq!(
                normalize(
                    serde_json::from_slice(&prepared.body.as_ref().unwrap().wire_bytes().unwrap())
                        .unwrap()
                ),
                case["prepared"]
            ),
            Err(e) => assert_eq!(e.to_string(), case["error"].as_str().unwrap()),
        }
        assert_eq!(
            serde_json::to_value(&*emitted.lock().unwrap()).unwrap(),
            case["emitted"]
        );
    }
}
#[test]
fn injected_timestamp_and_cancel_response_shapes() {
    let clock = Clock::default();
    clock.now.store(1700000000000, Ordering::SeqCst);
    assert_eq!(get_timestamp_ms(&clock), 1700000000000.0);
    let value: CancelOrderResponse = serde_json::from_str(
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    )
    .unwrap();
    assert_eq!(value.response.data.statuses, ["success"]);
}

#[test]
fn existing_signature_truthiness_prevents_resigning() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/signature-guards.json")).unwrap();
    for kind in ["string", "object", "array", "bool", "zero", "null"] {
        assert!(
            cases.as_array().unwrap().iter().any(|v| {
                let body = raw(v, "raw");
                let sig = &body["signature"];
                match kind {
                    "string" => sig.is_string(),
                    "object" => sig.is_object(),
                    "array" => sig.is_array(),
                    "bool" => sig.is_boolean(),
                    "zero" => sig == 0,
                    "null" => sig.is_null(),
                    _ => false,
                }
            }),
            "missing truthiness property {kind}"
        );
    }
    for case in cases.as_array().unwrap() {
        let client = create_exchange_api(
            options(&format!("0x{}", "11".repeat(32)), false, false),
            Arc::new(Adapter {
                calls: AtomicUsize::new(0),
                fail: false,
            }),
            Rc::new(RateLimiter::new(
                Arc::new(Clock::default()),
                1200.0,
                60000.0,
            )),
            Arc::new(|_| {}),
        );
        let request = client
            .build_request("/exchange", &raw(case, "raw"))
            .unwrap();
        let prepared = run(client.prepare_request(request, false)).unwrap();
        assert_eq!(
            normalize(
                serde_json::from_slice(&prepared.body.as_ref().unwrap().wire_bytes().unwrap())
                    .unwrap()
            ),
            case["prepared"]
        );
    }
}

#[test]
fn missing_order_fields_match_actual_ts_preimage_and_transmitted_json() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/missing-fields.json")).unwrap();
    for fields in [
        vec![],
        vec!["is_buy"],
        vec!["reduce_only"],
        vec!["is_buy", "reduce_only"],
    ] {
        assert!(
            cases
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["missing"] == serde_json::to_value(&fields).unwrap()),
            "missing-field corpus property {fields:?}"
        );
    }
    let nonce: Value = serde_json::from_str("0").unwrap();
    let mut prepared = Vec::new();
    for case in cases.as_array().unwrap() {
        let wire = order_request_to_order_wire(&raw(case, "raw"), 1.0).unwrap();
        assert_eq!(
            serde_json::to_string(&wire).unwrap(),
            case["wireJson"].as_str().unwrap(),
            "transmitted wire JSON"
        );
        let action = order_wires_to_order_action(vec![wire], None);
        assert_eq!(
            serde_json::to_string(&action).unwrap(),
            case["actionJson"].as_str().unwrap(),
            "transmitted action JSON"
        );
        let hex = |bytes: Vec<u8>| {
            bytes
                .into_iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        assert_eq!(
            hex(signing_preimage(&action, None, &nonce).unwrap()),
            case["preimage"].as_str().unwrap(),
            "actual TS keccak input bytes"
        );
        assert_eq!(
            action_hash(&action, None, &nonce).unwrap(),
            case["hash"].as_str().unwrap()
        );
        let sig = sign_l1_action(
            &format!("0x{}", "11".repeat(32)),
            &action,
            None,
            &nonce,
            true,
        )
        .unwrap();
        assert_eq!(
            sig.r,
            case["signature"]["r"].as_str().unwrap(),
            "API native signature r identity"
        );
        assert_eq!(
            sig.s,
            case["signature"]["s"].as_str().unwrap(),
            "API native signature s identity"
        );
        assert_eq!(
            normalize(serde_json::to_value(sig).unwrap()),
            case["signature"]
        );
        let reparsed: Value = serde_json::from_str(case["actionJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            hex(signing_preimage(&reparsed, None, &nonce).unwrap()),
            case["reparsedPreimage"].as_str().unwrap()
        );
        if !case["missing"].as_array().unwrap().is_empty() {
            assert_ne!(
                case["hash"], case["reparsedHash"],
                "pin TS signed-vs-serialized quirk"
            );
        }
        prepared.push((action, case["preimage"].as_str().unwrap().to_owned()));
    }
    // Retained payloads must not inherit another order's encoding view.
    for (action, expected) in prepared.into_iter().rev() {
        let bytes = signing_preimage(&action, None, &nonce).unwrap();
        let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex, expected);
    }
}

#[test]
fn access_errors_match_ts_error_identity_and_message() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/access-errors.json")).unwrap();
    for (op, raw) in [
        ("orderTypeToWire", "null"),
        ("orderTypeToWire", "{}"),
        ("orderRequestToOrderWire", "null"),
        ("orderRequestToOrderWire", r#"{"limit_px":1,"sz":1}"#),
        (
            "orderRequestToOrderWire",
            r#"{"limit_px":1,"sz":1,"order_type":null}"#,
        ),
    ] {
        assert!(
            cases
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["operation"] == op && v["raw"] == raw)
        );
    }
    for case in cases.as_array().unwrap() {
        let input = raw(case, "raw");
        let error = if case["operation"] == "orderTypeToWire" {
            order_type_to_wire(&input).unwrap_err()
        } else {
            order_request_to_order_wire(&input, 1.0).unwrap_err()
        };
        assert_eq!(error.name, case["errorName"].as_str().unwrap());
        assert_eq!(error.message, case["error"].as_str().unwrap());
    }
}

#[test]
fn api_deferred_and_waiting_identities_are_distinct() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/api-field-identities.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual source field identity supplement {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value =
        serde_json::from_str(include_str!("fixtures/api-field-identities.json")).unwrap();
    let cases = corpus["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4, "API-IDENTITY source total");
    let mut comparisons = 0;
    for index in 0..4 {
        let case = cases
            .get(index)
            .expect("API-IDENTITY native index coverage");
        comparisons += 1;
        let input: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        assert_eq!(
            hyperliquid_order_response_has_waiting_for_trigger(&input),
            case["waiting"].as_bool().unwrap(),
            "API waiting identity"
        );
        assert_eq!(
            hyperliquid_order_response_has_deferred_order_status(&input),
            case["deferred"].as_bool().unwrap(),
            "API deferred identity"
        );
        let result = parse_hyperliquid_order_response(&input).unwrap();
        let expected_ids = [0, 0, 2, 2][index];
        assert_eq!(
            case["parsed"]["orderIds"].as_array().unwrap().len(),
            expected_ids,
            "API-IDENTITY source order ID count"
        );
        assert_eq!(
            result.order_ids.len(),
            expected_ids,
            "API-IDENTITY independent native order ID count"
        );
        assert_eq!(
            result
                .order_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            case["parsed"]["orderIds"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap())
                .collect::<Vec<_>>(),
            "API native resting and filled order IDs"
        );
        assert_eq!(
            result.waiting_for_trigger,
            case["parsed"]["waitingForTrigger"].as_bool().unwrap(),
            "API parsed waiting identity"
        );
    }
    assert_eq!(comparisons, 4, "API-IDENTITY native comparison total");
}

#[test]
fn earlier_parser_throwing_id_coercion_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/earlier-response-probes.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for case in corpus["cases"].as_array().unwrap() {
        let input: Value = serde_json::from_str(case["raw"].as_str().unwrap()).unwrap();
        let actual = parse_hyperliquid_order_response(&input);
        if case.get("error").is_some() {
            match actual {
                Err(HyperliquidOrderResponseError::Coercion(error)) => {
                    assert_eq!(
                        error.name,
                        case["error"]["name"].as_str().unwrap(),
                        "old parser source error class"
                    );
                    assert_eq!(
                        error.message,
                        case["error"]["message"].as_str().unwrap(),
                        "old parser source error message"
                    );
                }
                other => panic!("old parser must propagate source TypeError: {other:?}"),
            }
        } else {
            assert_eq!(
                serde_json::json!(actual.unwrap().order_ids),
                case["result"]["orderIds"]
            );
        }
    }
}

#[test]
fn signing_validates_wallet_before_invalid_vault_and_encoded_cancel_retains_action() {
    let emitted = Arc::new(Mutex::new(Vec::new()));
    let logs = emitted.clone();
    let client = create_exchange_api(
        options("0x12", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::with_defaults(
            Arc::new(Clock::default()),
            None,
            None,
        )),
        Arc::new(move |message| logs.lock().unwrap().push(message.to_owned())),
    );
    let request = client
        .build_request(
            "/exchange",
            &serde_json::json!({"action":{"type":"cancel"},"nonce":1,"vaultAddress":"0xzz"}),
        )
        .unwrap();
    let error = run(client.prepare_request(request, true)).unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid private key",
        "KEY native key failure precedes invalid vault"
    );
    let mut request = client.build_request("/exchange", &Value::Null).unwrap();
    request.body = Some(RestBody::Encoded(
        br#"{"action":{"type":"cancel"},"unused":"\ud800"}"#.to_vec(),
    ));
    let response = RestResponse {
        status: 200,
        reason: None,
        headers: Default::default(),
        body: safe_cex_transport::RestResponseBody::Encoded(
            br#"{"status":"err","response":"Order was never placed, already canceled, or filled"}"#
                .to_vec(),
        ),
    };
    let error = client
        .check_response(&request, response, false)
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "Order was never placed, already canceled, or filled",
        "ACTION native original terminal cancellation error"
    );
    assert_eq!(
        emitted.lock().unwrap().len(),
        0,
        "ACTION native encoded cancellation suppresses terminal toast despite ignored surrogate"
    );
}

#[test]
fn audit_split_surrogate_response_refuses_before_emitting_substituted_error() {
    let emitted = Arc::new(Mutex::new(Vec::<String>::new()));
    let output = emitted.clone();
    let client = create_exchange_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::new(Arc::new(Clock::default()), 1200., 60000.)),
        Arc::new(move |text| output.lock().unwrap().push(text.to_owned())),
    );
    let request = client
        .build_request("/exchange", &serde_json::json!({"action":{"type":"order"}}))
        .unwrap();
    let response = RestResponse {
        reason: None,
        status: 422,
        headers: Default::default(),
        body: safe_cex_transport::RestResponseBody::Encoded(
            serde_json::to_vec(&"😀".repeat(200)).unwrap(),
        ),
    };
    let TransportError::Host(message) = client
        .check_response(&request, response, false)
        .unwrap_err()
    else {
        panic!("DIAGNOSTIC required native TransportError::Host(String)")
    };
    assert_eq!(
        message, "Hyperliquid response diagnostic exceeds native UTF-8 representation",
        "AUDIT native response cap refuses a split surrogate"
    );
    assert_eq!(
        emitted.lock().unwrap().len(),
        0,
        "AUDIT native representation refusal emits no replacement payload"
    );
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finding-http-health-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "DIAGNOSTIC actual source Error/emission/HealthMonitor getters: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let receipt: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(receipt["cases"], 1);
    assert_eq!(receipt["getter_observations"], 2);
}

#[test]
fn portable_crypto_errors_price_the_versioned_ethers_metadata_before_source_observation() {
    let mut completed = 0;
    for input in ["0xzz", "0x1", "12"] {
        assert_eq!(
            address_to_bytes(input).unwrap_err(),
            "invalid BytesLike value",
            "CRYPTO native stable address diagnostic"
        );
        assert_eq!(
            action_hash(&serde_json::json!({}), Some(input), &serde_json::json!(1)).unwrap_err(),
            "invalid BytesLike value",
            "CRYPTO native hash propagates stable diagnostic"
        );
        assert_eq!(
            sign_l1_action(
                &format!("0x{}", "11".repeat(32)),
                &serde_json::json!({}),
                Some(input),
                &serde_json::json!(1),
                true
            )
            .unwrap_err(),
            "invalid BytesLike value",
            "CRYPTO native sign propagates stable diagnostic"
        );
        completed += 3;
    }
    assert_eq!(completed, 9, "CRYPTO independent native error floor");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finding-crypto-diagnostics-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CRYPTO actual source diagnostic: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["source_cases"], 9);
    assert_eq!(receipt["stop"], "unprefixed signL1Action error");
}

#[test]
fn audit_raw_weight_retains_reserved_number_object_before_coercion() {
    let raw = Value::String(
        r#"{"type":"candleSnapshot","limit":{"$serde_json::private::Number":"120"}}"#.into(),
    );
    let parsed = safe_cex_venue_hyperliquid::rate_limit::parse_request_data(&raw);
    assert!(
        parsed["limit"].is_object(),
        "AUDIT native limit stays an object before weight coercion"
    );
    assert_eq!(
        parsed["limit"]["$serde_json::private::Number"].as_str(),
        Some("120")
    );
    assert_eq!(
        get_hyperliquid_rest_request_weight("/info", &raw).unwrap(),
        20.0,
        "AUDIT native marker object weight"
    );
}

#[test]
fn audit_encoded_response_retains_reserved_number_object_before_error_extraction() {
    let body = safe_cex_transport::RestResponseBody::Encoded(
        br#"{"error":{"$serde_json::private::Number":"120"}}"#.to_vec(),
    );
    let data = body.json_value().unwrap();
    assert!(
        data["error"].is_object(),
        "AUDIT native response error stays an object before extraction"
    );
    assert_eq!(
        data["error"]["$serde_json::private::Number"].as_str(),
        Some("120")
    );
    let clock = Arc::new(Clock::default());
    let client = create_exchange_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::new(clock, 1200., 60000.)),
        Arc::new(|_| {}),
    );
    let request = client
        .build_request("/exchange", &serde_json::json!({"action":{"type":"order"}}))
        .unwrap();
    let error = client
        .check_response(
            &request,
            RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body,
            },
            false,
        )
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        r#"{"$serde_json::private::Number":"120"}"#,
        "AUDIT native response error object description"
    );
}

#[test]
fn audit_encoded_request_marker_uses_object_weight_in_actual_pacing() {
    let clock = Arc::new(Clock::default());
    let client = create_info_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::new(clock.clone(), 1200., 60000.)),
        Arc::new(|_| {}),
    );
    let mut first = client.build_request("/info", &Value::Null).unwrap();
    first.body = Some(RestBody::Encoded(
        br#"{"type":"candleSnapshot","limit":{"$serde_json::private::Number":"120"}}"#.to_vec(),
    ));
    run(client.prepare_request(first, false)).unwrap();
    let second = client
        .build_request("/info", &serde_json::json!({"type":"allMids"}))
        .unwrap();
    run(client.prepare_request(second, false)).unwrap();
    assert_eq!(
        *clock.sleeps.lock().unwrap(),
        [1000],
        "AUDIT native encoded request reserves twenty tokens"
    );
}

#[test]
fn audit_poisoned_action_type_throws_before_unknown_action_emission() {
    let clock = Arc::new(Clock::default());
    let emitted = Arc::new(Mutex::new(Vec::<String>::new()));
    let output = emitted.clone();
    let adapter = Arc::new(Adapter {
        calls: AtomicUsize::new(0),
        fail: false,
    });
    let client = create_exchange_api(
        options("not-a-valid-wallet", false, false),
        adapter.clone(),
        Rc::new(RateLimiter::new(clock, 1200., 60000.)),
        Arc::new(move |s| output.lock().unwrap().push(s.into())),
    );
    let request = client
        .build_request(
            "/exchange",
            &serde_json::json!({"action":{"type":{"toString":0}},"nonce":1}),
        )
        .unwrap();
    let result = run(client.prepare_request(request, false));
    let Err(TransportError::JavaScript(error)) = result else {
        panic!("AUDIT native action type coercion must throw TypeError before emitter or wallet");
    };
    assert_eq!(error.name, "TypeError");
    assert_eq!(error.message, "Cannot convert object to primitive value");
    assert_eq!(
        emitted.lock().unwrap().len(),
        0,
        "AUDIT native failed coercion emits nothing"
    );
    assert_eq!(adapter.calls.load(Ordering::SeqCst), 0);
    let unknown = client
        .build_request(
            "/exchange",
            &serde_json::json!({"action":{"type":"unknown"},"nonce":1}),
        )
        .unwrap();
    assert!(run(client.prepare_request(unknown, false)).is_ok());
    assert_eq!(
        *emitted.lock().unwrap(),
        ["Refusing to sign unexpected Hyperliquid action \"unknown\""]
    );
}

#[test]
fn audit_limiter_omitted_weight_defaults_to_one_token() {
    let clock = Arc::new(Clock::default());
    let limiter = RateLimiter::new(clock.clone(), 1200., 60000.);
    run(limiter.wait_for_token(None)).unwrap();
    run(limiter.wait_for_token(1.)).unwrap();
    assert_eq!(
        *clock.sleeps.lock().unwrap(),
        [50],
        "AUDIT native omitted weight reserves one token"
    );
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence-weight-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "AUDIT actual weight/default source: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let receipt: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(receipt["cases"], 10);
}

#[test]
fn audit_encoded_weight_parses_once_and_ignores_unused_native_strings() {
    let clock = Arc::new(Clock::default());
    let client = create_info_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::new(clock.clone(), 1200., 60000.)),
        Arc::new(|_| {}),
    );
    let cases = [
        (br#"{"type":"userRole","unused":"\ud800"}"#.to_vec(), 60.),
        (
            format!(
                "{{\"type\":\"userRole\",\"unused\":{}0{}}}",
                "[".repeat(140),
                "]".repeat(140)
            )
            .into_bytes(),
            60.,
        ),
        (br#""{\"type\":\"userRole\"}""#.to_vec(), 20.),
        (br#"{"type":"allMids"}"#.to_vec(), 2.),
    ];
    let mut expected_start = 0.;
    for (index, (bytes, weight)) in cases.into_iter().enumerate() {
        let mut request = client.build_request("/info", &Value::Null).unwrap();
        request.body = Some(RestBody::Encoded(bytes));
        run(client.prepare_request(request, false)).unwrap();
        assert_eq!(
            clock.now.load(Ordering::SeqCst) as f64,
            expected_start,
            "AUDIT native encoded weight start {index}"
        );
        expected_start += weight * 50.;
    }
    assert_eq!(
        *clock.sleeps.lock().unwrap(),
        [3000, 3000, 1000],
        "AUDIT native raw weights reserve sixty, sixty then twenty before fourth request"
    );
}

#[test]
fn audit_raw_limit_coercion_visits_later_poison_after_surrogate() {
    for raw in [
        r#"{"type":"userRole","limit":["\ud800",{"toString":0}]}"#,
        r#"{"type":"not-extra-weighted","limit":{"toString":0}}"#,
    ] {
        let error =
            get_hyperliquid_rest_request_weight("/info", &Value::String(raw.into())).unwrap_err();
        assert_eq!(
            error.name, "TypeError",
            "AUDIT native consumed limit error name"
        );
        assert_eq!(
            error.message, "Cannot convert object to primitive value",
            "AUDIT native consumed limit visits poison"
        );
    }
    assert_eq!(
        get_hyperliquid_rest_request_weight(
            "/info",
            &Value::String(r#"{"type":"userRole","limit":"\ud800"}"#.into())
        )
        .unwrap(),
        60.
    );
    assert_eq!(
        get_hyperliquid_rest_request_weight(
            "/exchange",
            &Value::String(format!(
                "{{\"action\":{{\"type\":\"order\",\"orders\":[{}]}}}}",
                vec![r#""\ud800""#; 40].join(",")
            ))
        )
        .unwrap(),
        2.
    );
}

#[test]
fn audit_response_cap_accepts_complete_scalar_prefix() {
    let emitted = Arc::new(Mutex::new(Vec::<String>::new()));
    let output = emitted.clone();
    let client = create_exchange_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::new(Arc::new(Clock::default()), 1200., 60000.)),
        Arc::new(move |s| output.lock().unwrap().push(s.into())),
    );
    let request = client.build_request("/exchange", &Value::Null).unwrap();
    let body = format!("a{}", "😀".repeat(200));
    let error = client
        .check_response(
            &request,
            RestResponse {
                status: 422,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    serde_json::to_vec(&body).unwrap(),
                ),
            },
            false,
        )
        .unwrap_err();
    let expected = format!("Hyperliquid API error (HTTP 422): \"a{}", "😀".repeat(149));
    assert_eq!(
        error.to_string(),
        expected,
        "AUDIT native complete three-hundred-unit prefix"
    );
    assert_eq!(*emitted.lock().unwrap(), [expected]);
}

#[test]
fn audit_encoded_string_literal_weight_is_not_parsed_again() {
    let clock = Arc::new(Clock::default());
    let client = create_info_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::new(clock.clone(), 1200., 60000.)),
        Arc::new(|_| {}),
    );
    let mut request = client.build_request("/info", &Value::Null).unwrap();
    request.body = Some(RestBody::Encoded(br#""{\"type\":\"userRole\"}""#.to_vec()));
    run(client.prepare_request(request, false)).unwrap();
    run(client.prepare_request(
        client
            .build_request("/info", &serde_json::json!({"type":"allMids"}))
            .unwrap(),
        false,
    ))
    .unwrap();
    assert_eq!(
        *clock.sleeps.lock().unwrap(),
        [1000],
        "AUDIT native physical JSON string parses once and reserves twenty"
    );
}

#[test]
fn audit_encoded_request_telemetry_preserves_current_data_phase() {
    let observations = Arc::new(AtomicUsize::new(0));
    let observed = observations.clone();
    let emitted = Arc::new(AtomicUsize::new(0));
    let output = emitted.clone();
    let client = create_exchange_api(
        options("", false, false),
        Arc::new(Adapter {
            calls: AtomicUsize::new(0),
            fail: false,
        }),
        Rc::new(RateLimiter::new(Arc::new(Clock::default()), 1200., 60000.)),
        Arc::new(|_| {}),
    )
    .with_order_error_telemetry(
        Arc::new(move |data, message| {
            assert_eq!(
                data.as_str(),
                Some(
                    r#"{"action":{"type":"order"},"unused":{"$serde_json::private::Number":"120"}}"#
                ),
                "AUDIT native encoded request telemetry preserves source data string before parsing"
            );
            assert_eq!(message, "rejected");
            observed.fetch_add(1, Ordering::SeqCst);
            None
        }),
        Arc::new(move |message, _| {
            assert_eq!(message, "rejected");
            output.fetch_add(1, Ordering::SeqCst);
        }),
    );
    let mut request = client.build_request("/exchange", &Value::Null).unwrap();
    request.body = Some(RestBody::Encoded(
        br#"{"action":{"type":"order"},"unused":{"$serde_json::private::Number":"120"}}"#.to_vec(),
    ));
    let error = client
        .check_response(
            &request,
            RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    br#"{"error":"rejected"}"#.to_vec(),
                ),
            },
            false,
        )
        .unwrap_err();
    assert_eq!(error.to_string(), "rejected");
    assert_eq!(
        observations.load(Ordering::SeqCst),
        1,
        "AUDIT native telemetry provider called once"
    );
    assert_eq!(
        emitted.load(Ordering::SeqCst),
        1,
        "AUDIT native telemetry emission once"
    );
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/divergence-telemetry-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "AUDIT actual response interceptor/provider phase: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let receipt: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(receipt["cases"], 2);
}
