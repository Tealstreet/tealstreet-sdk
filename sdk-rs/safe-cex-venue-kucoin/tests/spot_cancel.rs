use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};
use safe_cex_types::ExchangeName;
use safe_cex_venue_kucoin::spot_writes::{cancel_all_orders, cancel_orders, cancel_symbol_orders};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    pin::Pin,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct Runtime {
    errors: std::cell::RefCell<Vec<String>>,
    events: Rc<RefCell<Vec<String>>>,
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1.0
    }
    fn now_ms(&self) -> f64 {
        1.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console(&self, level: &str, message: &str) {
        if level == "error" {
            self.events.borrow_mut().push("emit".into());
            self.errors
                .borrow_mut()
                .push(message.strip_prefix("exchange error ").unwrap().to_owned());
        }
    }
    fn unique_id(&self) -> String {
        "fixture-id".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(&self, _: &serde_json::Value, _: &serde_json::Value) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, _: &ExchangeError, _: &Value) {}
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
struct Adapter {
    case: Value,
    events: Rc<RefCell<Vec<String>>>,
    calls: RefCell<Vec<Value>>,
    active: Cell<bool>,
    store: Rc<DefaultStore>,
}
struct Guard<'a>(&'a Cell<bool>);
impl Drop for Guard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            assert!(request.body.is_none());
            // Assert the native request before the JSON capture used by the oracle.
            if let Some(expected) = self.case.get("expectedNativeRequests") {
                let index = self.calls.borrow().len();
                assert_eq!(request.method, "DELETE", "KUCOIN-SEQUENTIAL native method");
                assert_eq!(
                    request.url,
                    expected[index]["url"].as_str().unwrap(),
                    "KUCOIN-SEQUENTIAL native order id"
                );
                assert_eq!(
                    request.params.as_ref().unwrap()["symbol"],
                    expected[index]["symbol"],
                    "KUCOIN-SEQUENTIAL fresh symbol identity"
                );
            }
            if self.case["method"] != "cancelSymbolOrders" && self.case["method"] != "cancelOrders"
            {
                assert!(request.params.is_none());
            }
            self.events.borrow_mut().push("request".into());
            let mut call = json!({"method":request.method,"url":request.url});
            if let Some(params) = &request.params {
                call["params"] = params.clone();
            }
            self.calls.borrow_mut().push(call);
            let descriptor = self
                .case
                .get("responses")
                .map(|rows| &rows[self.calls.borrow().len() - 1])
                .unwrap_or(&self.case);
            assert!(!descriptor.is_null(), "missing response fixture");
            if let Some(markets) = descriptor.get("marketsAfter") {
                Store::update(
                    self.store.as_ref(),
                    StoreUpdate {
                        markets: Some(serde_json::from_value(markets.clone()).unwrap()),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            }
            if descriptor["pending"] == true {
                self.active.set(true);
                let _guard = Guard(&self.active);
                std::future::pending::<()>().await;
            }
            let response = |r: &Value| RestResponse {
                status: 400,
                reason: Some("fixture".into()),
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    r["raw"]
                        .as_str()
                        .map(|s| s.as_bytes().to_vec())
                        .unwrap_or_else(|| serde_json::to_vec(&r["data"]).unwrap()),
                ),
            };
            if let Some(failure) = descriptor.get("failure") {
                let mut error =
                    RestFailure::pre_transmit(failure["message"].as_str().unwrap().into(), request);
                error.code = Some("ECONNRESET".into());
                error.context = Some(Box::new(json!({"retained":true})));
                error.response = failure.get("response").map(|r| Box::new(response(r)));
                return Err(error);
            }
            Ok(response(descriptor))
        })
    }
}
fn setup(case: Value) -> (BaseExchange, Adapter, Rc<Runtime>) {
    let events = Rc::new(RefCell::new(vec![]));
    let runtime = Rc::new(Runtime {
        events: events.clone(),
        ..Default::default()
    });
    let store = Rc::new(DefaultStore::new());
    if let Some(markets) = case.get("markets") {
        Store::update(
            store.as_ref(),
            StoreUpdate {
                markets: Some(serde_json::from_value(markets.clone()).unwrap()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    }
    let base = BaseExchange::new(
        ExchangeName::Kucoinspot,
        serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
        store.clone(),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    (
        base,
        Adapter {
            case,
            events,
            calls: RefCell::new(vec![]),
            active: Cell::new(false),
            store,
        },
        runtime,
    )
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture pending"),
    }
}
fn source(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-cancel.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let captures: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        captures.len(),
        cases.len(),
        "KUCOIN-CARDINALITY spot_cancel source case count"
    );
    captures
}
fn run(case: Value) -> Value {
    let (base, adapter, runtime) = setup(case);
    let response = if adapter.case["method"] == "cancelSymbolOrders" {
        ready(cancel_symbol_orders(
            &base,
            &adapter,
            adapter.case["symbol"].as_str().unwrap(),
        ))
    } else if adapter.case["method"] == "cancelOrders" {
        let orders =
            serde_json::from_value::<Vec<safe_cex_types::Order>>(adapter.case["orders"].clone())
                .unwrap();
        ready(cancel_orders(&base, &adapter, &orders))
    } else {
        ready(cancel_all_orders(&base, &adapter))
    };
    let result = match response {
        Ok(()) => {
            runtime.events.borrow_mut().push("resolved".into());
            json!({"ok":true})
        }
        Err(error) => {
            runtime.events.borrow_mut().push("rejected".into());
            if let Some(error) = error.downcast_ref::<RestFailure>() {
                assert_eq!(error.code.as_deref(), Some("ECONNRESET"));
                assert_eq!(error.context.as_deref(), Some(&json!({"retained":true})));
                assert_eq!(error.request.method, "DELETE");
                if adapter.case["failure"].get("response").is_some() {
                    assert!(error.response.is_some());
                }
                json!({"ok":false,"message":error.message,"name":null,"sameFailure":true})
            } else {
                let error = error.downcast_ref::<ExchangeError>().unwrap();
                json!({"ok":false,"message":error.message,"name":error.name,"sameFailure":false})
            }
        }
    };
    json!({"result":result,"errors":runtime.errors.borrow().clone(),"events":runtime.events.borrow().clone(),"calls":adapter.calls.borrow().clone()})
}
#[test]
fn live_spot_cancel_preserves_emit_then_rethrow_and_failure_payload() {
    let mut cases = vec![
        json!({"data":{"code":"200000"}}),
        json!({"data":null}),
        json!({"failure":{"message":"network","response":{"data":{"msg":"venue text"}}}}),
        json!({"failure":{"message":"network"}}),
    ];
    for msg in [
        Value::Null,
        json!(false),
        json!(0),
        json!(""),
        json!(true),
        json!(77),
        json!({"why":"no"}),
        json!([]),
        json!([1, 2]),
        json!({"toString":null}),
        json!("é😀"),
    ] {
        cases.push(json!({"data":{"code":"bad","msg":msg}}));
    }
    for code in [json!(200000), json!(false), json!("200000 ")] {
        cases.push(json!({"data":{"code":code}}));
    }
    let expected = source(&cases);
    assert_eq!(cases.len(), 18);
    for (c, e) in cases.into_iter().zip(expected) {
        assert_eq!(
            run(c.clone()),
            e,
            "live Spot cancel emission and rethrow: {c}"
        );
    }
}
#[test]
fn cancel_response_and_emitter_findings_are_separate() {
    let cases = vec![
        json!({"raw":""}),
        json!({"raw":"not-json"}),
        json!({"failure":{"message":"network","response":{"raw":"not-json"}}}),
        json!({"failure":{"message":"network","response":{"data":{"msg":{"why":"no"}}}}}),
    ];
    let expected = source(&cases);
    for (c, e) in cases.into_iter().zip(expected) {
        let actual = run(c.clone());
        assert_eq!(actual["events"], e["events"]);
        assert_eq!(actual["calls"], e["calls"]);
        assert_ne!(actual["errors"], e["errors"]);
        if c.get("failure").is_some() {
            assert_eq!(
                actual["result"], e["result"],
                "original failure survives emitter refusal"
            );
        }
    }
    let raw = r#"{"code":"bad","msg":"\ud800"}"#;
    let c = json!({"raw":raw,"surrogate":true});
    let expected = source(std::slice::from_ref(&c));
    assert!(
        expected[0]["resultJson"]
            .as_str()
            .unwrap()
            .contains(r"\ud800")
    );
    let actual = run(c);
    assert!(
        actual["result"]["message"]
            .as_str()
            .unwrap()
            .contains("representation refusal")
    );
}
#[test]
fn cancellation_drops_pending_delete_twice() {
    for _ in 0..2 {
        let (base, adapter, runtime) = setup(json!({"pending":true}));
        let mut future = Box::pin(cancel_all_orders(&base, &adapter));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(adapter.active.get());
        drop(future);
        assert!(!adapter.active.get());
        assert!(runtime.errors.borrow().is_empty());
        assert_eq!(*runtime.events.borrow(), ["request"]);
    }
}
#[test]
fn cancel_source_rethrow_control_fails_by_name() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_spot_cancel_preserves_emit_then_rethrow_and_failure_payload",
            "--nocapture",
        ])
        .env("KUCOIN_CANCEL_MUTATION", "swallow")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(101));
    assert!(String::from_utf8_lossy(&out.stderr).contains("live Spot cancel emission and rethrow"));
}
#[test]
fn cancel_runtime_and_compiler_fail_loudly() {
    for (key, value) in [
        ("KUCOIN_CANCEL_MISSING_RUNTIME", "1"),
        (
            "SAFE_CEX_TYPESCRIPT_PATH",
            "missing-kucoin-typescript-control",
        ),
    ] {
        let mut child = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-cancel.cjs"
            ))
            .env(key, value)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"[]").unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("MODULE_NOT_FOUND"));
    }
}

fn symbol_market(symbol: &str, id: &str) -> Value {
    json!({"id":id,"symbol":symbol,"base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0.01,"max":10000},"leverage":{"min":1,"max":1}}})
}
#[test]
fn live_symbol_cancel_selects_wire_id_and_preserves_rejection_order() {
    let mut cases = vec![];
    for symbol in [
        "BTC/USDT",
        "UNKNOWN",
        "",
        "é🚀",
        "constructor",
        "toString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toLocaleString",
        "__proto__",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
    ] {
        for response in [
            json!({"data":{"code":"200000"}}),
            json!({"data":{"code":"bad"}}),
            json!({"failure":{"message":"network","response":{"data":{"msg":"wire rejection"}}}}),
        ] {
            let mut case = response;
            case["method"] = json!("cancelSymbolOrders");
            case["symbol"] = json!(symbol);
            case["markets"] = json!([symbol_market("BTC/USDT", "BTC-USDT")]);
            cases.push(case);
        }
    }
    cases.push(json!({"method":"cancelSymbolOrders","symbol":"EMPTY","markets":[symbol_market("EMPTY", "")],"data":{"code":"200000"}}));
    let expected = source(&cases);
    assert_eq!(expected.len(), cases.len());
    for (case, expected) in cases.into_iter().zip(expected) {
        assert_eq!(
            run(case.clone()),
            expected,
            "live symbol cancel wire identity and error ordering: {case}"
        );
    }
}
#[test]
fn symbol_cancel_store_prototype_replacement_remains_a_finding() {
    let mut market = symbol_market("__proto__", "prototype-market");
    market["info"] = json!({"id":"poison-wire"});
    let case = json!({"method":"cancelSymbolOrders","symbol":"info","markets":[market],"data":{"code":"200000"}});
    let expected = source(std::slice::from_ref(&case)).remove(0);
    assert_eq!(
        expected["calls"][0]["params"]["symbol"], "poison-wire",
        "source Store prototype replacement redirects cancellation"
    );
    assert_eq!(
        run(case)["calls"][0]["params"]["symbol"],
        "info",
        "typed Store refuses prototype traversal"
    );
}

#[test]
fn symbol_cancellation_drops_pending_delete_twice() {
    for _ in 0..2 {
        let (base, adapter, runtime) =
            setup(json!({"method":"cancelSymbolOrders","symbol":"BTC-USDT","pending":true}));
        let mut future = Box::pin(cancel_symbol_orders(&base, &adapter, "BTC-USDT"));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(adapter.active.get());
        Store::update(
            base.store.as_ref(),
            StoreUpdate {
                markets: Some(vec![]),
                ..Default::default()
            },
            false,
        )
        .expect("symbol cancel releases Store borrow before transport await");
        drop(future);
        assert!(
            !adapter.active.get(),
            "symbol cancel releases transport wait"
        );
        assert!(runtime.errors.borrow().is_empty());
        assert_eq!(*runtime.events.borrow(), ["request"]);
    }
}

#[test]
fn symbol_cancel_lone_surrogate_query_boundary() {
    let cases: Vec<_> = [r#""\ud800""#, r#""\udfff""#]
        .into_iter()
        .map(|raw| json!({"method":"cancelSymbolOrders","symbolRaw":raw,"data":{"code":"200000"}}))
        .collect();
    let captures = source(&cases);
    assert_eq!(
        captures.len(),
        cases.len(),
        "symbol surrogate capture count"
    );
    for (case, capture) in cases.iter().zip(captures) {
        let raw = case["symbolRaw"].as_str().unwrap();
        assert!(
            serde_json::from_str::<String>(raw).is_err(),
            "Rust symbol ingress representation refusal"
        );
        assert_eq!(
            capture["resultJson"], r#"{"ok":true}"#,
            "source accepts lone-surrogate cancel symbol"
        );
        let expected = [
            r#"[{"method":"DELETE","url":"/api/v1/hf/orders","params":{"symbol":"#,
            raw,
            "}}]",
        ]
        .concat();
        assert_eq!(
            capture["callsJson"], expected,
            "source adapter params retain lone-surrogate symbol"
        );
    }
}

fn sequential_order(id: &str, symbol: &str) -> Value {
    json!({"id":id,"symbol":symbol,"status":"open","type":"limit","side":"buy",
        "price":1,"amount":2,"filled":0,"remaining":2,"reduceOnly":false,"lastUpdate":3})
}
#[test]
fn sequential_cancel_pins_request_identity_and_stops_at_first_error() {
    let orders = json!([
        sequential_order("first/17", "BTC-USDT"),
        sequential_order("second-29", "ETH-USDT")
    ]);
    let success = json!({"data":{"code":"200000"}});
    let rejected = json!({"data":{"code":"bad","msg":"denied"}});
    let transport = json!({"failure":{"message":"network"}});
    let mut cases = vec![
        json!({"method":"cancelOrders","orders":[],"responses":[]}),
        json!({"method":"cancelOrders","orders":orders,"responses":[success,success]}),
        json!({"method":"cancelOrders","orders":orders,"responses":[rejected,success]}),
        json!({"method":"cancelOrders","orders":orders,"responses":[transport,success]}),
        json!({"method":"cancelOrders","orders":orders,"responses":[success,rejected]}),
        json!({"method":"cancelOrders","orders":orders,"responses":[success,transport]}),
        json!({"method":"cancelOrders","orders":orders,"markets":[symbol_market("ETH-USDT","old-id")],
            "responses":[{"data":{"code":"200000"},"marketsAfter":[symbol_market("ETH-USDT","new-id")]},success]}),
        json!({"method":"cancelOrders","orders":orders,"markets":[symbol_market("BTC-USDT","")],"responses":[success,success]}),
    ];
    let counts = [0, 2, 1, 1, 2, 2, 2, 2];
    for (index, case) in cases.iter_mut().enumerate() {
        case["expectedNativeRequests"] = Value::Array((0..counts[index]).map(|request| {
            json!({
                "url": (["/api/v1/hf/orders/first/17", "/api/v1/hf/orders/second-29"][request]),
                "symbol": if request == 0 { "BTC-USDT" } else if index == 6 { "new-id" } else { "ETH-USDT" }
            })
        }).collect());
    }
    let expected = source(&cases);
    assert_eq!(expected.len(), 8, "KUCOIN-SEQUENTIAL source total");
    let mut compared = 0;
    for index in 0..8 {
        let actual = run(cases[index].clone());
        assert_eq!(
            expected[index]["calls"].as_array().unwrap().len(),
            counts[index],
            "KUCOIN-SEQUENTIAL source request count {index}"
        );
        assert_eq!(
            actual["calls"].as_array().unwrap().len(),
            counts[index],
            "KUCOIN-SEQUENTIAL native request count {index}"
        );
        for request in 0..counts[index] {
            assert_eq!(
                actual["calls"][request]["url"],
                ["/api/v1/hf/orders/first/17", "/api/v1/hf/orders/second-29"][request],
                "KUCOIN-SEQUENTIAL native order id"
            );
            let symbol = if request == 0 {
                "BTC-USDT"
            } else if index == 6 {
                "new-id"
            } else {
                "ETH-USDT"
            };
            assert_eq!(
                actual["calls"][request]["params"]["symbol"], symbol,
                "KUCOIN-SEQUENTIAL fresh symbol identity"
            );
            compared += 1;
        }
        assert_eq!(
            actual, expected[index],
            "KUCOIN-SEQUENTIAL full ordered effect identity {index}"
        );
    }
    assert_eq!(compared, 12, "KUCOIN-SEQUENTIAL native comparisons");
}
#[test]
fn dropping_sequential_cancel_releases_first_request_without_sending_second() {
    for pending_request in 1..=2 {
        for _ in 0..2 {
            let responses = if pending_request == 1 {
                json!([{"pending":true}, {"data":{"code":"200000"}}])
            } else {
                json!([{"data":{"code":"200000"}}, {"pending":true}])
            };
            let (base, adapter, _) = setup(json!({"method":"cancelOrders","responses":responses}));
            let orders = serde_json::from_value::<Vec<safe_cex_types::Order>>(json!([
                sequential_order("first", "BTC-USDT"),
                sequential_order("second", "ETH-USDT")
            ]))
            .unwrap();
            let mut future = Box::pin(cancel_orders(&base, &adapter, &orders));
            assert!(
                future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                    .is_pending()
            );
            assert!(adapter.active.get());
            assert_eq!(
                adapter.calls.borrow().len(),
                pending_request,
                "KUCOIN-SEQUENTIAL pending request count"
            );
            drop(future);
            assert!(!adapter.active.get());
            assert_eq!(
                adapter.calls.borrow().len(),
                pending_request,
                "KUCOIN-SEQUENTIAL dropped request count"
            );
        }
    }
}

#[test]
fn sequential_cancel_live_input_alias_and_store_prototype_findings() {
    let mut market = symbol_market("__proto__", "prototype-market");
    market["info"] = json!({"id":"poison-wire"});
    let cases = vec![
        json!({"method":"cancelOrders","orders":[sequential_order("first", "BTC-USDT")],
            "responses":[{"data":{"code":"200000"},"appendOrder":sequential_order("appended", "ETH-USDT")},{"data":{"code":"200000"}}]}),
        json!({"method":"cancelOrders","orders":[sequential_order("first", "info")],"markets":[market],"data":{"code":"200000"}}),
        json!({"method":"cancelOrders","orders":[sequential_order("first", "BTC-USDT")],"raw":"not-json"}),
        json!({"method":"cancelOrders","orders":[sequential_order("first", "BTC-USDT")],"failure":{"message":"network","response":{"data":{"msg":77}}}}),
    ];
    let expected = source(&cases);
    assert_eq!(expected.len(), 4, "KUCOIN-SEQUENTIAL finding source total");
    let actual: Vec<_> = cases.into_iter().map(run).collect();
    assert_eq!(
        expected[0]["calls"].as_array().unwrap().len(),
        2,
        "KUCOIN-SEQUENTIAL source live array grows"
    );
    assert_eq!(
        expected[0]["calls"][1]["url"], "/api/v1/hf/orders/appended",
        "KUCOIN-SEQUENTIAL appended order identity"
    );
    assert_eq!(
        actual[0]["calls"].as_array().unwrap().len(),
        1,
        "KUCOIN-SEQUENTIAL Rust borrowed slice stays fixed"
    );
    assert_eq!(
        expected[1]["calls"][0]["params"]["symbol"], "poison-wire",
        "KUCOIN-SEQUENTIAL source prototype target"
    );
    assert_eq!(
        actual[1]["calls"][0]["params"]["symbol"], "info",
        "KUCOIN-SEQUENTIAL typed Store target"
    );
    assert_eq!(
        expected[2]["errors"],
        json!(["Failed to cancel KuCoin Spot order first"])
    );
    assert!(
        actual[2]["errors"][0]
            .as_str()
            .unwrap()
            .contains("expected"),
        "KUCOIN-SEQUENTIAL response parse finding"
    );
    assert_eq!(expected[3]["errors"], json!([77]));
    assert_eq!(
        actual[3]["errors"],
        json!(["KuCoin Spot cancel error emitter requires a string representation"])
    );
    assert_eq!(
        actual[3]["result"], expected[3]["result"],
        "KUCOIN-SEQUENTIAL raw emission keeps original failure"
    );
}

#[test]
fn sequential_cancel_lone_surrogate_order_id_is_a_string_boundary() {
    let cases = [
        json!({"method":"cancelOrders","ordersRaw":r#"[{"id":"\ud800","symbol":"BTC-USDT"}]"#,"data":{"code":"200000"}}),
    ];
    let expected = source(&cases);
    assert_eq!(expected.len(), 1);
    assert_eq!(
        expected[0]["callsJson"],
        r#"[{"method":"DELETE","url":"/api/v1/hf/orders/\ud800","params":{"symbol":"BTC-USDT"}}]"#,
        "KUCOIN-SEQUENTIAL source surrogate URL retained"
    );
    assert!(
        serde_json::from_str::<Value>(cases[0]["ordersRaw"].as_str().unwrap()).is_err(),
        "KUCOIN-SEQUENTIAL Rust UTF-8 input refusal"
    );
}
