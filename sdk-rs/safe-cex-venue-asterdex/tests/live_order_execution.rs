#[path = "support/subscription_runtime.rs"]
mod host;
use host::Runtime;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::ExchangeError,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::{
    RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_types::{ExchangeName, Order, Ticker};
use safe_cex_venue_asterdex::order_execution::{ExecutionOrder, OpenInterestMap, OrderExecution};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};
// Independent expectation, iteration 1. Stops at these 40 scenarios.
const NAMES: [&str; 40] = [
    "limit-rounded-builder-maker",
    "hedge-close-stop",
    "oneway-reduce-takeprofit",
    "bracket-partial-success",
    "oversize-three-chunks",
    "trailing-rejected-before-rest",
    "caller-id-preflight",
    "algo-capacity-reject",
    "stp-v1-rejected",
    "stp-v3-and-postonly-fee",
    "missing-market",
    "missing-id",
    "null-order-response",
    "batch-continues",
    "batch-all-fail",
    "batch-preflight-no-write",
    "batch-empty",
    "cancel-partial-unknown-success",
    "cancel-all-fail",
    "cancel-empty",
    "replace-stop-trigger",
    "replace-cancel-failure-no-post",
    "validate-v1",
    "validate-key-missing",
    "validate-secret-missing",
    "validate-v3-missing-user",
    "validate-v3-bad-private-key",
    "validate-v3-success",
    "validate-null-account",
    "validate-rest-fails",
    "interest-direct-unchanged-missing",
    "interest-bulk",
    "interest-rotating-wrap",
    "interest-empty-finally-timer",
    "interest-start-and-repeat",
    "interest-prototype-id-coercion",
    "interest-live-map-mutation",
    "interest-native-number-boundary",
    "interest-warning-log-throws-aborts-fallback",
    "interest-disposed-no-work",
];
const REQUESTS: [usize; 40] = [
    1, 1, 1, 3, 3, 0, 0, 0, 0, 1, 0, 1, 1, 2, 2, 0, 0, 3, 2, 0, 2, 1, 1, 0, 0, 0, 0, 1, 1, 1, 0, 1,
    27, 0, 2, 0, 0, 0, 1, 0,
];
const TIMERS: [usize; 40] = [
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    1, 1, 1, 0, 0, 0, 1, 0,
];
fn run<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    let w = Waker::noop().clone();
    for _ in 0..32 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(&w)) {
            return v;
        }
    }
    panic!("32-poll fixture floor exceeded")
}
struct Shared {
    queue: RefCell<VecDeque<Value>>,
    requests: RefCell<Vec<(&'static str, RestRequest)>>,
}
struct Adapter(&'static str, Rc<Shared>);
impl RestAdapter for Adapter {
    fn execute<'a>(&'a self, r: RestRequest) -> RestFuture<'a, RestResponse> {
        self.1.requests.borrow_mut().push((self.0, r.clone()));
        let next = self
            .1
            .queue
            .borrow_mut()
            .pop_front()
            .expect("unexpected native request");
        Box::pin(async move {
            if let Some(e) = next["error"].as_str() {
                return Err(RestFailure::pre_transmit(e.into(), r));
            }
            Ok(RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: RestResponseBody::Untransformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        next["data"].clone(),
                    )
                    .unwrap(),
                ),
            })
        })
    }
}
fn assert_json(a: &Value, b: &Value, name: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{name} native numeric value"
        ),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{name} native field count");
            for (k, v) in a {
                assert_json(
                    v,
                    b.get(k).unwrap_or_else(|| panic!("missing {k} {name}")),
                    name,
                );
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{name} native array count");
            for (a, b) in a.iter().zip(b) {
                assert_json(a, b, name);
            }
        }
        _ => assert_eq!(a, b, "{name} native value"),
    }
}
#[test]
#[ignore = "requires live TypeScript oracle"]
fn order_execution_matches_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-execution-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 40, "independent scenario floor");
    let mut wire = Vec::new();
    for (i, c) in cases.iter().enumerate() {
        let name = c["name"].as_str().unwrap();
        assert_eq!(name, NAMES[i], "independent ordered scenario floor");
        let rt = Rc::new(Runtime::default());
        let store = Rc::new(DefaultStore::new());
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(serde_json::from_value(c["initial"]["markets"].clone()).unwrap()),
                orders: c["initial"]
                    .get("orders")
                    .map(|v| serde_json::from_value(v.clone()).unwrap()),
                tickers: c["initial"]
                    .get("tickers")
                    .map(|v| serde_json::from_value(v.clone()).unwrap()),
                options: c["initial"]
                    .get("options")
                    .map(|v| serde_json::from_value(v.clone()).unwrap()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let notifications = Rc::new(Cell::new(0));
        let n = notifications.clone();
        let _ = Store::subscribe(
            store.as_ref(),
            Box::new(move |_, _| {
                n.set(n.get() + 1);
                Ok(())
            }),
        );
        let base = Rc::new(BaseExchange::new(
            ExchangeName::Asterdex,
            serde_json::from_value(c["credentials"].clone()).unwrap(),
            store.clone(),
            rt.clone(),
            ExchangeSettings::default(),
        ));
        let errors = Rc::new(RefCell::new(Vec::new()));
        let e = errors.clone();
        let error_store = store.clone();
        let mutate_error = c["mutateErrorStore"] == true;
        base.on(
            "error",
            Rc::new(move |_, value| {
                let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
                e.borrow_mut().push(value.as_str().unwrap().to_owned());
                if mutate_error {
                    error_store.set_setting("isHedged", true).unwrap();
                }
                Ok(())
            }),
        );
        if c["throwLog"] == true {
            let failed = Cell::new(false);
            base.on(
                "log",
                Rc::new(move |_, v| {
                    let v = safe_cex_exchange::js_runtime::value::to_json(v).unwrap();
                    if v.as_str().unwrap().contains("Bulk open-interest") && !failed.replace(true) {
                        return Err(ExchangeError::new("log down"));
                    }
                    Ok(())
                }),
            );
        }
        let shared = Rc::new(Shared {
            queue: RefCell::new(c["queue"].as_array().unwrap().iter().cloned().collect()),
            requests: RefCell::new(Vec::new()),
        });
        let execution = OrderExecution::new(
            base.clone(),
            Rc::new(Adapter("primary", shared.clone())),
            Rc::new(Adapter("secondary", shared.clone())),
            Rc::new(Adapter("public", shared.clone())),
            serde_json::from_value(c["version"].clone()).unwrap(),
        );
        execution
            .open_interest_cursor
            .set(c["cursor"].as_u64().unwrap_or(0) as usize);
        // Oracle cursor is post-call; setup uses the explicit fixture's starting cursor.
        if name == "interest-rotating-wrap" {
            execution.open_interest_cursor.set(25);
        }
        if c["disposed"] == true {
            base.dispose().unwrap();
        }
        let args = c["args"].as_array().unwrap();
        let result: Result<Option<Vec<String>>, ExchangeError> = match c["method"].as_str().unwrap()
        {
            "placeOrder" => {
                let opts: ExecutionOrder = serde_json::from_value(args[0].clone()).unwrap();
                run(execution.place_order(&opts, false)).map(Some)
            }
            "placeOrders" => {
                let opts: Vec<ExecutionOrder> = serde_json::from_value(args[0].clone()).unwrap();
                run(execution.place_orders(&opts, false)).map(Some)
            }
            "updateOrder" => {
                let opts = serde_json::from_value(args[0].clone()).unwrap();
                run(execution.update_order(&opts)).map(Some)
            }
            "cancelOrders" => {
                let opts: Vec<Order> = serde_json::from_value(args[0].clone()).unwrap();
                run(execution.cancel_orders(&opts)).map(|_| None)
            }
            "validateAccount" => run(execution.validate_account()).map(|v| {
                assert_eq!(
                    v.error.as_value(),
                    &c["result"]["error"],
                    "{name} native validation result"
                );
                assert_eq!(
                    v.error.as_value(),
                    &json!(""),
                    "native empty validation error"
                );
                assert!(v.balance.is_none());
                None
            }),
            "updateStoreOpenInterest" => {
                let map = OpenInterestMap::default();
                for v in args[0].as_array().unwrap() {
                    map.insert(
                        v[0].as_str().unwrap().into(),
                        if c["numbers"] == true {
                            match v[1].as_str().unwrap() {
                                "NaN" => f64::NAN,
                                "Infinity" => f64::INFINITY,
                                "-0" => -0.0,
                                _ => panic!("numeric fixture"),
                            }
                        } else {
                            v[1].as_f64().unwrap()
                        },
                    );
                }
                if c["mutateMap"] == true {
                    let changes = map.clone();
                    let done = Cell::new(false);
                    let _ = Store::subscribe(
                        store.as_ref(),
                        Box::new(move |_, _| {
                            if !done.replace(true) {
                                changes.delete("ETHUSDT");
                                changes.insert("ETHUSDT".into(), 9.0);
                                changes.insert("SOLUSDT".into(), 7.0);
                            }
                            Ok(())
                        }),
                    );
                }
                execution.update_store_open_interest(&map).map(|_| None)
            }
            "pollOpenInterest" => run(execution.poll_open_interest()).map(|_| None),
            "startOpenInterestPolling" => {
                execution.start_open_interest_polling();
                rt.pump();
                rt.mono.set(14999.0);
                rt.pump();
                assert_eq!(
                    shared.requests.borrow().len(),
                    1,
                    "native repeat waits until deadline"
                );
                rt.mono.set(15000.0);
                rt.pump();
                Ok(None)
            }
            _ => panic!("unknown method"),
        };
        rt.pump();
        assert_eq!(
            result.is_ok(),
            c["error"].is_null(),
            "{name} native success"
        );
        match result {
            Ok(Some(ids)) => {
                let expected: Vec<String> = serde_json::from_value(c["result"].clone()).unwrap();
                assert_eq!(ids, expected, "{name} native returned IDs");
            }
            Ok(None) => {
                if c["method"] == "validateAccount" {
                    assert_eq!(c["result"]["error"], "", "validation source success field");
                }
            }
            Err(e) => {
                assert_eq!(
                    e.message, c["error"]["message"],
                    "{name} native error message"
                );
                if name == "validate-rest-fails" {
                    assert_eq!(
                        c["error"]["name"], "AxiosError",
                        "source AxiosError name witness"
                    );
                    assert_eq!(e.name, "Error", "native REST error-name boundary");
                    assert!(e.is_axios_error);
                } else {
                    assert_eq!(e.name, c["error"]["name"], "{name} native error name");
                }
            }
        }
        if c["numbers"] == true {
            let actual = store.tickers();
            assert!(actual[0].open_interest.is_nan(), "native NaN OI");
            assert_eq!(actual[1].open_interest, f64::INFINITY, "native infinite OI");
            assert_eq!(
                actual[2].open_interest.to_bits(),
                0_f64.to_bits(),
                "native JS equality skips signed-zero patch"
            );
            assert_eq!(c["tickers"][0]["openInterest"]["number"], "NaN");
            assert_eq!(c["tickers"][1]["openInterest"]["number"], "Infinity");
        } else {
            let tickers: Vec<Ticker> = serde_json::from_value(c["tickers"].clone()).unwrap();
            assert_eq!(*store.tickers(), tickers, "{name} native ticker state");
        }
        assert_eq!(
            store.options().is_hedged,
            c["hedged"].as_bool().unwrap(),
            "{name} native hedge setting"
        );
        assert_eq!(
            execution.open_interest_cursor.get(),
            c["cursor"].as_u64().unwrap() as usize,
            "{name} native rotation cursor"
        );
        assert_eq!(
            notifications.get(),
            c["notifications"].as_u64().unwrap(),
            "{name} native notifications"
        );
        let deadlines: Vec<f64> = serde_json::from_value(c["timers"].clone()).unwrap();
        assert_eq!(rt.deadlines(), deadlines, "{name} native timer deadlines");
        assert_eq!(rt.timers(), TIMERS[i], "{name} native owned timers");
        assert_eq!(
            c["timers"].as_array().unwrap().len(),
            TIMERS[i],
            "source timer floor"
        );
        assert_eq!(
            *errors.borrow(),
            serde_json::from_value::<Vec<String>>(c["errors"].clone()).unwrap(),
            "{name} native emitted errors"
        );
        let logs: Vec<_> = rt
            .logs
            .borrow()
            .iter()
            .filter(|(level, _)| level != "trace")
            .filter(|(_, msg)| !msg.starts_with("exchange error "))
            .map(|(level, msg)| json!({"message":msg,"severity":level}))
            .collect();
        assert_eq!(
            logs,
            c["logs"].as_array().unwrap().clone(),
            "{name} native venue logs"
        );
        assert_eq!(
            shared.queue.borrow().len(),
            0,
            "{name} native unconsumed responses"
        );
        assert_eq!(c["unconsumed"], 0, "source unconsumed floor");
        wire.push((
            i,
            name.to_owned(),
            shared.requests.borrow().clone(),
            c["requests"].as_array().unwrap().clone(),
        ));
    }
    // Every native result/state assertion above precedes ALL request assertions.
    for (i, name, requests, expected) in wire {
        assert_eq!(
            requests.len(),
            REQUESTS[i],
            "{name} independent request floor"
        );
        assert_eq!(
            expected.len(),
            REQUESTS[i],
            "{name} independent source request floor"
        );
        for ((kind, r), e) in requests.iter().zip(expected) {
            assert_eq!(*kind, e["kind"], "{name} adapter selection");
            assert_eq!(r.method, e["method"], "{name} method");
            assert_eq!(r.url, e["url"], "{name} url");
            match &r.body {
                Some(RestBody::Untransformed(v)) => {
                    assert_json(&v.clone().into_value().unwrap(), &e["body"], &name)
                }
                None => assert!(e.get("body").is_none()),
                _ => panic!("unexpected transformed request"),
            };
            assert_eq!(r.params, e.get("params").cloned(), "{name} params");
            assert_eq!(
                r.timeout_ms,
                e.get("timeout").and_then(Value::as_i64),
                "{name} timeout"
            );
            assert!(
                r.query_params.is_none() && r.query_profile.is_none(),
                "legacy request path"
            );
            if r.method == "DELETE" {
                assert_eq!(
                    r.config_extensions
                        .get("__suppressAsterUnknownOrderError")
                        .cloned()
                        .map(|value| value.into_value().unwrap()),
                    e.get("__suppressAsterUnknownOrderError").cloned(),
                    "native request preserves source Axios suppression"
                );
            }
        }
    }
}

#[test]
fn nonprogressing_split_refuses_before_io() {
    let runtime = Rc::new(Runtime::default());
    let store = Rc::new(DefaultStore::new());
    let market=serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"price":0.01,"amount":1},"limits":{"amount":{"min":0,"max":0.1},"leverage":{"min":1,"max":20}}})).unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let owner = Rc::new(BaseExchange::new(
        ExchangeName::Asterdex,
        serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
        store,
        runtime,
        ExchangeSettings::default(),
    ));
    let shared = Rc::new(Shared {
        queue: RefCell::new(VecDeque::new()),
        requests: RefCell::new(Vec::new()),
    });
    let adapter = Rc::new(Adapter("secondary", shared.clone()));
    let execution = OrderExecution::new(
        owner,
        adapter.clone(),
        adapter.clone(),
        adapter,
        safe_cex_venue_asterdex::types::AsterDEXAPIVersion::V1,
    );
    let opts: ExecutionOrder = serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10}),
    )
    .unwrap();
    let error = run(execution.place_order(&opts, false)).unwrap_err();
    assert_eq!(
        error.name, "RepresentationError",
        "native nonprogress refusal name"
    );
    assert_eq!(
        error.message, "AsterDEX order split cannot make finite progress",
        "native nonprogress refusal message"
    );
    assert_eq!(
        shared.requests.borrow().len(),
        0,
        "independent no-request refusal floor"
    );
}
