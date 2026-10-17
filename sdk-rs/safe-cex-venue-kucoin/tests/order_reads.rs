use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::provider::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse,
};
use safe_cex_types::{ExchangeName, Order};
use safe_cex_venue_kucoin::order_reads::{fetch_algo_orders, fetch_normal_orders, fetch_orders};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
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
}
impl ExchangeRuntime for Runtime {
    fn monotonic_now_ms(&self) -> f64 {
        1700000000123.0
    }
    fn now_ms(&self) -> f64 {
        1700000000123.0
    }
    fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
        panic!("unexpected sleep")
    }
    fn spawn_local(&self, _: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        panic!("unexpected task")
    }
    fn console(&self, level: &str, message: &str) {
        if level == "error" {
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
fn ready<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    match std::pin::pin!(future).as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("fixture unexpectedly blocked"),
    }
}
struct Adapter {
    case: Value,
    calls: RefCell<Vec<Value>>,
    store: Rc<DefaultStore>,
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        self.calls
            .borrow_mut()
            .push(json!({"method":request.method,"url":request.url,"params":request.params}));
        assert!(request.body.is_none());
        let index = self.calls.borrow().len() - 1;
        Box::pin(async move {
            if index == 1
                && let Some(orders) = self.case.get("duringSecond")
            {
                Store::update(
                    self.store.as_ref(),
                    StoreUpdate {
                        orders: Some(serde_json::from_value(orders.clone()).unwrap()),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            }
            let response = &self.case["responses"][index];
            if let Some(failure) = response.get("failure") {
                Err(RestFailure {
                    code: Some("ECONNRESET".into()),
                    message: failure["message"].as_str().unwrap().into(),
                    response: failure.get("response").map(|r| {
                        Box::new(RestResponse {
                            status: 503,
                            reason: Some("Unavailable".into()),
                            headers: Default::default(),
                            body: safe_cex_transport::RestResponseBody::Encoded(
                                r["raw"]
                                    .as_str()
                                    .map(|raw| raw.as_bytes().to_vec())
                                    .unwrap_or_else(|| serde_json::to_vec(&r["data"]).unwrap()),
                            ),
                        })
                    }),
                    request: Box::new(request),
                    context: Some(Box::new(json!({"retained":true}))),
                    transport_error: None,
                })
            } else {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: safe_cex_transport::RestResponseBody::Encoded(
                        response
                            .get("raw")
                            .and_then(Value::as_str)
                            .map(|s| s.as_bytes().to_vec())
                            .unwrap_or_else(|| serde_json::to_vec(&response["data"]).unwrap()),
                    ),
                })
            }
        })
    }
}
fn oracle(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-reads.cjs"
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
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let captures: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        captures.len(),
        cases.len(),
        "KUCOIN-CARDINALITY order_reads source case count"
    );
    captures
}
fn row(id: &str) -> Value {
    json!({"id":id,"symbol":"XBTUSDTM","side":"buy","type":"limit","status":"open","size":"4","filledSize":"1","price":"2","createdAt":1700000000123_i64})
}
fn existing() -> Vec<Value> {
    ["limit","market","stop_market","take_profit_market","trailing_stop_market"].into_iter().enumerate().map(|(i,kind)|json!({"id":format!("old{i}"),"symbol":"BTCUSDT","status":"open","type":kind,"side":"buy","price":2.0,"amount":4.0,"filled":1.0,"remaining":3.0,"reduceOnly":false,"lastUpdate":1700000000777.0})).collect()
}
fn execute(case: Value) -> (Option<Vec<Order>>, Vec<Value>, Vec<String>, usize) {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            orders: Some(
                serde_json::from_value(case.get("existing").cloned().unwrap_or(json!([]))).unwrap(),
            ),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let runtime = Rc::new(Runtime::default());
    let base = BaseExchange::new(
        ExchangeName::Kucoin,
        serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
        store.clone(),
        runtime.clone(),
        ExchangeSettings::default(),
    );
    let clock_calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut clock = safe_cex_store::virtual_clock::VirtualClock::default();
    let clock_counter = clock_calls.clone();
    clock.now = std::sync::Arc::new(move || {
        clock_counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        1700000000123.0
    });
    let offset = case["clockOffset"].as_f64().unwrap_or(0.0);
    if offset != 0.0 {
        clock.runtime = "browser".into();
        clock.set_server_time_source(Some(std::sync::Arc::new(move || {
            Box::pin(async move { Ok(Some(1700000000123.0 + offset)) })
        })));
        ready(clock.sync_client_server_time());
        clock_calls.store(0, std::sync::atomic::Ordering::SeqCst);
    }
    let adapter = Adapter {
        case,
        calls: RefCell::new(vec![]),
        store,
    };
    let result = match adapter.case["method"].as_str().unwrap() {
        "fetchNormalOrders" => ready(fetch_normal_orders(&base, &adapter, &clock)).unwrap(),
        "fetchAlgoOrders" => ready(fetch_algo_orders(&base, &adapter, &clock)).unwrap(),
        "fetchOrders" => Some(ready(fetch_orders(&base, &adapter, &clock)).unwrap()),
        _ => panic!("unknown method"),
    };
    let calls = adapter.calls.take();
    let errors = runtime.errors.take();
    (
        result,
        calls,
        errors,
        clock_calls.load(std::sync::atomic::Ordering::SeqCst),
    )
}
fn canonical(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(canonical).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), canonical(v))).collect())
        }
        v => v.clone(),
    }
}
#[test]
fn live_order_reads_preserve_failure_categories_and_request_order() {
    let normal = json!({"data":{"code":"200000","data":{"items":[row("new")]}}});
    let empty = json!({"data":{"code":"200000","data":{"items":[]}}});
    let mut cases = vec![];
    for method in ["fetchNormalOrders", "fetchAlgoOrders"] {
        for response in [
            normal.clone(),
            empty.clone(),
            json!({"data":{"code":"200000","data":{}}}),
            json!({"data":{"code":"200000","data":{"items":{}}}}),
            json!({"data":{"code":"200000"}}),
            json!({"data":{"code":"200000","data":null}}),
            json!({"data":null}),
            json!({"data":{"code":200000,"msg":"strict code"}}),
            json!({"data":{"code":"bad","msg":""}}),
            json!({"failure":{"message":"transport","response":{"data":{"msg":"venue body"}}}}),
            json!({"failure":{"message":"transport","response":{"data":{"msg":""}}}}),
        ] {
            cases.push(json!({"method":method,"responses":[response]}));
        }
    }
    for first in [
        normal.clone(),
        empty.clone(),
        json!({"failure":{"message":"normal failed"}}),
    ] {
        for second in [
            normal.clone(),
            empty.clone(),
            json!({"failure":{"message":"algo failed"}}),
        ] {
            for update in [false, true] {
                let mut case = json!({"method":"fetchOrders","responses":[first,second],"existing":existing()});
                if update {
                    let mut next = existing();
                    for order in &mut next {
                        order["id"] = json!(format!("later-{}", order["id"].as_str().unwrap()));
                    }
                    case["duringSecond"] = json!(next);
                }
                cases.push(case);
            }
        }
    }
    for size in ["1e400", "-Infinity", "NaN"] {
        let mut input = row("nonfinite");
        input["size"] = json!(size);
        cases.push(json!({"method":"fetchOrders","responses":[{"data":{"code":"200000","data":{"items":[input]}}},empty],"existing":existing()}));
    }
    for remaining in [0.0, 1.0, 3.0, 4.0] {
        let mut old = existing();
        old[0]["id"] = json!("new");
        old[0]["extra"] = json!({"remainingNative":remaining});
        cases.push(json!({"method":"fetchOrders","responses":[normal,empty],"existing":existing(),"duringSecond":old}));
    }
    let mut missing_time = row("virtual-time");
    missing_time.as_object_mut().unwrap().remove("createdAt");
    cases.push(json!({"method":"fetchOrders","clockOffset":5000,"responses":[{"data":{"code":"200000","data":{"items":[row("known-time"),missing_time]}}},empty]}));
    let expected = oracle(&cases);
    for (case, expected) in cases.into_iter().zip(expected) {
        let (orders, calls, errors, clock_calls) = execute(case.clone());
        assert_eq!(
            canonical(
                &json!({"orders":orders.as_ref().map(|rows|rows.iter().map(tag_order).collect::<Vec<_>>()),"calls":calls,"errors":errors,"clockCalls":clock_calls})
            ),
            canonical(
                &json!({"orders":expected["orders"],"calls":expected["calls"],"errors":expected["errors"],"clockCalls":expected["clockCalls"]})
            ),
            "failed book must preserve current category: {case}"
        );
    }
}
#[test]
fn order_read_source_empty_failure_control() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_order_reads_preserve_failure_categories_and_request_order",
            "--nocapture",
        ])
        .env("KUCOIN_ORDER_READ_MUTATION", "empty-failure")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        text.contains("failed book must preserve current category:"),
        "{text}"
    );
}

#[test]
fn inherited_order_output_refuses_whole_book_and_preserves_old_category() {
    let names = [
        "constructor",
        "toString",
        "toLocaleString",
        "valueOf",
        "hasOwnProperty",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "__proto__",
        "__defineGetter__",
        "__defineSetter__",
        "__lookupGetter__",
        "__lookupSetter__",
    ];
    let mut cases = vec![];
    for method in ["fetchNormalOrders", "fetchAlgoOrders", "fetchOrders"] {
        for name in names {
            for field in ["status", "type", "timeInForce"] {
                let mut input = row("inherited");
                input[field] = json!(name);
                let mut responses = vec![
                    json!({"data":{"code":"200000","data":{"items":[row("good-before"),input,row("good-after")]}}}),
                ];
                if method == "fetchOrders" {
                    responses.push(json!({"data":{"code":"200000","data":{"items":[]}}}));
                }
                cases.push(json!({"method":method,"responses":responses,"existing":existing(),"inherited":name,"field":field}));
            }
        }
    }
    let references = oracle(&cases);
    for (case, reference) in cases.into_iter().zip(references) {
        let field = case["field"].as_str().unwrap();
        assert_eq!(
            reference["types"][1][field],
            if case["inherited"] == "__proto__" {
                "object"
            } else {
                "function"
            },
            "source typeof before JSON"
        );
        assert_eq!(reference["orders"].as_array().unwrap().len(), 3);
        if case["inherited"] == "__proto__" {
            assert_eq!(reference["orders"][1][field], json!({}));
        } else {
            assert!(reference["orders"][1].get(field).is_none());
        }
        assert_eq!(reference["errors"], json!([]));
        let (orders, _, errors, _) = execute(case.clone());
        assert!(errors[0].contains("whole record refused"));
        if case["method"] == "fetchOrders" {
            let ids: Vec<_> = orders.unwrap().into_iter().map(|o| o.id).collect();
            assert_eq!(
                ids,
                ["old0", "old1"],
                "closed mapper refusal preserves entire existing normal category"
            );
        } else {
            assert!(
                orders.is_none(),
                "typed refusal loses even valid siblings in the same book"
            );
        }
    }
}

#[test]
fn order_read_drop_releases_pending_request_without_second_send_or_store_change() {
    use std::cell::Cell;
    struct Guard(Rc<Cell<usize>>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    struct Pending {
        calls: Cell<usize>,
        drops: Rc<Cell<usize>>,
        fail_first: bool,
    }
    impl RestAdapter for Pending {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.calls.set(self.calls.get() + 1);
            if self.fail_first && self.calls.get() == 1 {
                return Box::pin(async move {
                    Err(RestFailure::pre_transmit("first refused".into(), request))
                });
            }
            let guard = Guard(self.drops.clone());
            Box::pin(async move {
                let _guard = guard;
                std::future::pending().await
            })
        }
    }
    for fail_first in [false, true] {
        let store = Rc::new(DefaultStore::new());
        Store::update(
            store.as_ref(),
            StoreUpdate {
                orders: Some(serde_json::from_value(json!(existing())).unwrap()),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let base = BaseExchange::new(
            ExchangeName::Kucoin,
            serde_json::from_value(json!({"key":"fixture","secret":"fixture"})).unwrap(),
            store.clone(),
            Rc::new(Runtime::default()),
            ExchangeSettings::default(),
        );
        let adapter = Pending {
            calls: Cell::new(0),
            drops: Rc::new(Cell::new(0)),
            fail_first,
        };
        let clock = safe_cex_store::virtual_clock::VirtualClock::default();
        let mut future = Box::pin(fetch_orders(&base, &adapter, &clock));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        let calls = if fail_first { 2 } else { 1 };
        assert_eq!(adapter.calls.get(), calls);
        drop(future);
        assert_eq!(adapter.drops.get(), 1);
        assert_eq!(adapter.calls.get(), calls);
        assert_eq!(store.orders().len(), 5);
        let mut retry = Box::pin(fetch_orders(&base, &adapter, &clock));
        assert!(
            retry
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert_eq!(adapter.calls.get(), calls + 1, "no cancellation latch");
        drop(retry);
        assert_eq!(adapter.drops.get(), 2);
    }
}

#[test]
fn order_read_json_string_and_malformed_row_boundaries_are_explicit() {
    let mut corpus = Vec::new();
    for method in ["fetchNormalOrders", "fetchAlgoOrders", "fetchOrders"] {
        for response in [
            json!({"data":{"code":"bad","msg":42}}),
            json!({"data":{"code":"200000","data":{"items":[null]}}}),
            json!({"raw":r#"{"code":"200000","data":{"items":[{"id":"\ud800","symbol":"XBTUSDTM","type":"limit","size":"1"}]}}"#}),
        ] {
            let mut responses = vec![response.clone()];
            if method == "fetchOrders" {
                responses.push(json!({"data":{"code":"200000","data":{"items":[]}}}));
            }
            let case = json!({"method":method,"responses":responses,"existing":existing(),"rawOnly":response.get("raw").is_some()});
            corpus.push((method, response, case));
        }
    }
    let inputs: Vec<_> = corpus.iter().map(|row| row.2.clone()).collect();
    let captured = oracle(&inputs);
    assert_eq!(captured.len(), corpus.len());
    for ((method, response, case), reference) in corpus.into_iter().zip(captured) {
        let (orders, _, errors, _) = execute(case.clone());
        assert_eq!(orders.is_some(), method == "fetchOrders");
        if response.get("raw").is_some() {
            assert!(
                reference["ordersJSON"]
                    .as_str()
                    .unwrap()
                    .contains(r#"\ud800"#)
            );
            assert_eq!(reference["errors"], json!([]));
            assert!(errors[0].contains("representation refusal"));
        } else if response["data"]["msg"] == 42 {
            assert_eq!(reference["errors"], json!([42]));
            assert!(errors[0].contains("error message requires"));
        } else {
            assert_eq!(
                reference["errors"],
                json!(["Cannot read properties of null (reading 'updateTime')"])
            );
            assert!(errors[0].contains("order row requires an object"));
        }
    }
}

fn tag_number(n: f64) -> Value {
    if n.is_finite() {
        json!(n)
    } else {
        json!({"$number":if n.is_nan(){"NaN"}else if n.is_sign_positive(){"Infinity"}else{"-Infinity"}})
    }
}
fn tag_order(order: &safe_cex_types::Order) -> Value {
    let mut value = json!(order);
    for (key, n) in [
        ("price", order.price.to_number()),
        ("amount", order.amount),
        ("filled", order.filled),
        ("remaining", order.remaining),
        ("lastUpdate", order.last_update),
    ] {
        value[key] = tag_number(n);
    }
    if let Some(extra) = order.extra.as_ref() {
        if let Some(n) = extra.remaining_native {
            value["extra"]["remainingNative"] = tag_number(n);
        }
        if let Some(n) = extra.filled_in_placement_units {
            value["extra"]["filledInPlacementUnits"] = tag_number(n);
        }
    }
    value
}

#[test]
fn order_info_live_alias_differs_from_owned_snapshot() {
    let mut corpus = Vec::new();
    for method in ["fetchNormalOrders", "fetchAlgoOrders", "fetchOrders"] {
        let mut responses = vec![json!({"data":{"code":"200000","data":{"items":[row("alias")]}}})];
        if method == "fetchOrders" {
            responses.push(json!({"data":{"code":"200000","data":{"items":[]}}}));
        }
        let case = json!({"method":method,"responses":responses,"aliasProbe":true});
        corpus.push((case,));
    }
    let inputs: Vec<_> = corpus.iter().map(|row| row.0.clone()).collect();
    let captured = oracle(&inputs);
    assert_eq!(captured.len(), corpus.len());
    for ((mut case,), reference) in corpus.into_iter().zip(captured) {
        assert_eq!(reference["alias"]["same"], true);
        assert_eq!(reference["alias"]["label"], "later-label");
        assert_eq!(reference["alias"]["infoPrice"], "NaN");
        assert_eq!(reference["alias"]["gatewayPrice"], "NaN");
        assert_eq!(reference["alias"]["mappedPrice"], 2);
        let (orders, _, _, _) = execute(case.clone());
        let order = orders.unwrap().remove(0);
        let before: Value =
            serde_json::from_str(reference["alias"]["before"].as_str().unwrap()).unwrap();
        assert_eq!(order.info.as_ref().unwrap(), &before);
        case["responses"][0]["data"]["data"]["items"][0]["id"] = json!("later-label");
        assert_eq!(order.info.as_ref().unwrap()["id"], "alias");
        assert_eq!(order.info.as_ref().unwrap()["price"], "2");
        assert_eq!(order.price, 2.0);
    }
}

#[test]
fn order_reads_actual_axios_raw_response_staging_findings() {
    let mut corpus = Vec::new();
    for method in ["fetchNormalOrders", "fetchAlgoOrders", "fetchOrders"] {
        for raw in ["", "not-json"] {
            for failed in [false, true] {
                let response = if failed {
                    json!({"failure":{"message":"network message","response":{"raw":raw}}})
                } else {
                    json!({"raw":raw})
                };
                let responses = if method == "fetchOrders" {
                    vec![response.clone(), response]
                } else {
                    vec![response]
                };
                let case = json!({"method":method,"responses":responses,"existing":existing()});
                corpus.push((method, failed, case));
            }
        }
    }
    let inputs: Vec<_> = corpus.iter().map(|row| row.2.clone()).collect();
    let captured = oracle(&inputs);
    assert_eq!(captured.len(), corpus.len());
    for ((method, failed, case), expected) in corpus.into_iter().zip(captured) {
        let messages = if failed {
            vec!["network message"; if method == "fetchOrders" { 2 } else { 1 }]
        } else {
            match method {
                "fetchNormalOrders" => vec!["Failed to fetch orders"],
                "fetchAlgoOrders" => vec!["Failed to fetch algo orders"],
                _ => vec!["Failed to fetch orders", "Failed to fetch algo orders"],
            }
        };
        assert_eq!(
            expected["errors"],
            json!(messages),
            "actual Axios order error boundary"
        );
        let (orders, calls, errors, clocks) = execute(case);
        assert_eq!(calls.len(), messages.len());
        assert_eq!(clocks, 0);
        assert_eq!(errors.len(), messages.len());
        assert!(errors.iter().all(|e| e.contains("representation refusal")));
        if method == "fetchOrders" {
            assert_eq!(orders.unwrap().len(), existing().len());
        } else {
            assert!(orders.is_none());
        }
    }
}

#[test]
fn order_book_cardinality_supplement_retains_each_category() {
    let normal =
        json!({"data":{"code":"200000","data":{"items":[row("normal-1"),row("normal-2")]}}});
    let algo = json!({"data":{"code":"200000","data":{"items":[row("algo-1"),row("algo-2")]}}});
    let bad = json!({"data":{"code":"bad","msg":"retain category"}});
    let cases = [
        json!({"method":"fetchNormalOrders","responses":[normal]}),
        json!({"method":"fetchAlgoOrders","responses":[algo]}),
        json!({"method":"fetchOrders","responses":[normal,algo]}),
        json!({"method":"fetchOrders","responses":[bad,bad],"existing":existing()}),
    ];
    let expected_ids: [&[&str]; 4] = [
        &["normal-1", "normal-2"],
        &["algo-1", "algo-2"],
        &["normal-1", "normal-2", "algo-1", "algo-2"],
        &["old0", "old1", "old2", "old3", "old4"],
    ];
    let source = oracle(&cases);
    assert_eq!(source.len(), 4, "KUCOIN-CARDINALITY book source cases");
    assert_eq!(
        source
            .iter()
            .map(|c| c["orders"].as_array().unwrap().len())
            .sum::<usize>(),
        13,
        "KUCOIN-CARDINALITY book source total"
    );
    let mut comparisons = 0;
    for index in 0..4 {
        assert_eq!(
            source[index]["orders"].as_array().unwrap().len(),
            [2, 2, 4, 5][index],
            "KUCOIN-CARDINALITY book source category {index}"
        );
        let (native, _, _, _) = execute(cases[index].clone());
        let native = native.unwrap();
        assert_eq!(
            native.len(),
            [2, 2, 4, 5][index],
            "KUCOIN-CARDINALITY book native category {index}"
        );
        for row in 0..[2, 2, 4, 5][index] {
            assert_eq!(native[row].id, expected_ids[index][row]);
            assert_eq!(native[row].id, source[index]["orders"][row]["id"]);
            comparisons += 1;
        }
    }
    assert_eq!(
        comparisons, 13,
        "KUCOIN-CARDINALITY book native comparisons"
    );
}
