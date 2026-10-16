#[path = "support/numeric_bits.rs"]
mod numeric_bits;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};
use safe_cex_types::{ExchangeName, Ticker};
use safe_cex_venue_kucoin::spot_reads::fetch_tickers;
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
    active: Cell<bool>,
}
struct Pending<'a>(&'a Cell<bool>);
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
impl RestAdapter for Adapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            assert!(request.params.is_none() && request.body.is_none());
            self.calls
                .borrow_mut()
                .push(json!({"method":request.method,"url":request.url}));
            if self.case["pending"] == true {
                self.active.set(true);
                let _guard = Pending(&self.active);
                std::future::pending::<()>().await;
            }
            if let Some(tickers) = self.case.get("duringResponse") {
                Store::update(
                    self.store.as_ref(),
                    StoreUpdate {
                        tickers: Some(serde_json::from_value(tickers.clone()).unwrap()),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            }
            if let Some(error) = self.case.get("failure") {
                let mut failure =
                    RestFailure::pre_transmit(error["message"].as_str().unwrap().into(), request);
                failure.response = error.get("response").map(|r| {
                    Box::new(RestResponse {
                        status: 503,
                        reason: None,
                        headers: Default::default(),
                        body: safe_cex_transport::RestResponseBody::Encoded(
                            r["raw"]
                                .as_str()
                                .map(|raw| raw.as_bytes().to_vec())
                                .unwrap_or_else(|| serde_json::to_vec(&r["data"]).unwrap()),
                        ),
                    })
                });
                return Err(failure);
            }
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    self.case["raw"]
                        .as_str()
                        .map(|s| s.as_bytes().to_vec())
                        .unwrap_or_else(|| serde_json::to_vec(&self.case["data"]).unwrap()),
                ),
            })
        })
    }
}
fn setup(case: Value) -> (BaseExchange, Adapter, Rc<Runtime>) {
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(
                serde_json::from_value(case.get("markets").cloned().unwrap_or(json!([]))).unwrap(),
            ),
            tickers: Some(
                serde_json::from_value(case.get("existing").cloned().unwrap_or(json!([]))).unwrap(),
            ),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let runtime = Rc::new(Runtime::default());
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
            calls: RefCell::new(vec![]),
            store,
            active: Cell::new(false),
        },
        runtime,
    )
}
fn source(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-tickers.cjs"
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
        "KUCOIN-CARDINALITY spot_tickers source case count"
    );
    captures
}
fn market(symbol: &str) -> Value {
    json!({"id":format!("market:{symbol}"),"symbol":symbol,"base":"A","quote":"B","active":true,"precision":{"amount":1,"price":0.25},"limits":{"amount":{"min":0,"max":9},"leverage":{"min":1,"max":7}}})
}
fn existing() -> Value {
    json!([{"id":"old","symbol":"OLD","bid":1,"ask":2,"last":3,"mark":7,"index":11,"percentage":13,"openInterest":17,"fundingRate":19,"fundingTime":23,"volume":4,"quoteVolume":5}])
}
fn row() -> Value {
    json!({"symbol":"BTC-USDT","last":"12.5tail","buy":"12","sell":"13","changeRate":"-0.25","vol":"4","volValue":"50","marker":"before"})
}
fn success(row: Value) -> Value {
    json!({"markets":[market("BTC-USDT")],"data":{"code":"200000","data":{"ticker":[row]}}})
}
fn bits(rows: &[Ticker]) -> Value {
    json!(
        rows.iter()
            .map(|t| [
                t.bid,
                t.ask,
                t.last,
                t.mark,
                t.index,
                t.percentage,
                t.funding_rate,
                t.funding_time,
                t.open_interest,
                t.volume,
                t.quote_volume
            ]
            .map(numeric_bits::bits))
            .collect::<Vec<_>>()
    )
}
fn wire(value: Value) -> Value {
    serde_json::from_str(
        &safe_cex_num::js_json_stringify(&value.into())
            .unwrap()
            .unwrap(),
    )
    .unwrap()
}
#[test]
fn live_spot_tickers_numeric_bits_and_fallback_order() {
    let mut cases = vec![success(row())];
    for field in ["last", "buy", "sell", "changeRate", "vol", "volValue"] {
        for value in [
            Value::Null,
            json!(false),
            json!(true),
            json!(0),
            json!(-2),
            json!(""),
            json!("-0"),
            json!("12abc"),
            json!("Infinity"),
            json!("-Infinity"),
            json!("bad"),
            json!([]),
            json!([1, 2]),
            json!({}),
        ] {
            let mut r = row();
            r[field] = value;
            cases.push(success(r));
        }
        let mut r = row();
        r.as_object_mut().unwrap().remove(field);
        cases.push(success(r));
    }
    for data in [
        Value::Null,
        json!({}),
        json!({"code":200000}),
        json!({"code":"bad","msg":"gateway"}),
        json!({"code":"200000"}),
        json!({"code":"200000","data":{"ticker":null}}),
        json!({"code":"200000","data":{"ticker":[null]}}),
    ] {
        cases.push(json!({"data":data,"existing":[],"duringResponse":existing()}));
    }
    for msg in [json!("venue body"), json!(""), json!(false)] {
        cases.push(json!({"failure":{"message":"network","response":{"data":{"msg":msg}}},"duringResponse":existing()}));
    }
    for symbol in ["unknown", " café 🚀 \u{feff}\t"] {
        let mut r = row();
        r["symbol"] = json!(symbol);
        let mut c = success(r);
        c["markets"] = json!([market(symbol)]);
        cases.push(c);
    }
    cases.push(json!({"data":{"code":"200000","data":{"ticker":[row()]}}}));
    for lexeme in ["-0", "-0.0", "1e400", "-1e400"] {
        let mut r = row();
        r["last"] = json!("TOKEN");
        let raw = serde_json::to_string(&json!({"code":"200000","data":{"ticker":[r]}}))
            .unwrap()
            .replace("\"TOKEN\"", lexeme);
        cases.push(json!({"raw":raw,"markets":[market("BTC-USDT")]}));
    }
    assert_eq!(cases.len(), 108);
    for (case, expected) in cases.iter().zip(source(&cases)) {
        let (base, adapter, runtime) = setup(case.clone());
        let result = ready(fetch_tickers(&base, &adapter)).unwrap();
        assert_eq!(
            bits(&result),
            expected["bits"],
            "Spot ticker native bits: {case}"
        );
        assert_eq!(
            wire(serde_json::to_value(&result).unwrap()),
            wire(expected["tickers"].clone()),
            "live Spot ticker output: {case}"
        );
        assert_eq!(
            json!(adapter.calls.take()),
            expected["calls"],
            "Spot ticker request"
        );
        assert_eq!(
            json!(runtime.errors.take()),
            expected["errors"],
            "Spot ticker errors: {case}"
        );
    }
}
#[test]
fn ticker_cardinality_supplement_retains_each_native_row() {
    let mut second = row();
    second["symbol"] = json!("ETH-USDT");
    let case = json!({"markets":[market("BTC-USDT"),market("ETH-USDT")],
        "data":{"code":"200000","data":{"ticker":[row(),second]}}});
    let captured = source(std::slice::from_ref(&case));
    assert_eq!(
        captured.len(),
        1,
        "KUCOIN-CARDINALITY ticker supplement cases"
    );
    assert_eq!(
        captured[0]["tickers"].as_array().unwrap().len(),
        2,
        "KUCOIN-CARDINALITY ticker source output total"
    );
    let (base, adapter, _) = setup(case);
    let result = ready(fetch_tickers(&base, &adapter)).unwrap();
    assert_eq!(
        result.len(),
        2,
        "KUCOIN-CARDINALITY ticker native retained rows"
    );
    let mut comparisons = 0;
    for index in 0..2 {
        assert_eq!(
            result[index].symbol,
            ["BTC-USDT", "ETH-USDT"][index],
            "KUCOIN-CARDINALITY ticker native index coverage"
        );
        assert_eq!(
            result[index].symbol,
            captured[0]["tickers"][index]["symbol"]
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 2,
        "KUCOIN-CARDINALITY ticker native comparisons"
    );
}

#[test]
fn ticker_source_numeric_mutation_fails_by_name() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_spot_tickers_numeric_bits_and_fallback_order",
            "--nocapture",
        ])
        .env("KUCOIN_TICKERS_MUTATION", "number")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(String::from_utf8_lossy(&output.stderr).contains("Spot ticker native bits"));
}
#[test]
fn inherited_market_outputs_are_not_silently_skipped() {
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
    let cases = names
        .iter()
        .map(
            |name| json!({"data":{"code":"200000","data":{"ticker":[{"symbol":name,"last":"2"}]}}}),
        )
        .collect::<Vec<_>>();
    for (case, expected) in cases.iter().zip(source(&cases)) {
        assert_eq!(expected["tickers"].as_array().unwrap().len(), 1);
        assert_eq!(
            expected["fields"],
            json!([{"id":"undefined","symbol":"undefined"}])
        );
        assert!(expected["tickers"][0].get("symbol").is_none());
        assert_eq!(expected["errors"], json!([]));
        let (base, adapter, runtime) = setup(case.clone());
        assert!(ready(fetch_tickers(&base, &adapter)).unwrap().is_empty());
        assert!(runtime.errors.take()[0].contains("inherited market"));
    }
}

#[test]
fn ticker_store_prototype_replacement_is_a_separate_finding() {
    let case = json!({"markets":[market("__proto__")],"data":{"code":"200000","data":{"ticker":[{"symbol":"id","last":"2"}]}}});
    let expected = source(std::slice::from_ref(&case)).pop().unwrap();
    assert_eq!(expected["tickers"].as_array().unwrap().len(), 1);
    assert_eq!(
        expected["fields"],
        json!([{"id":"undefined","symbol":"undefined"}])
    );
    let (base, adapter, runtime) = setup(case);
    assert!(ready(fetch_tickers(&base, &adapter)).unwrap().is_empty());
    assert!(runtime.errors.take().is_empty());
}
#[test]
fn ticker_aliases_remain_owned_snapshots() {
    let mut info = success(row());
    info["alias"] = json!("info");
    let fallback = json!({"data":{"code":"bad"},"existing":existing(),"alias":"store"});
    for (case, expected) in [&info, &fallback]
        .into_iter()
        .zip(source(&[info.clone(), fallback.clone()]))
    {
        let (base, adapter, _) = setup(case.clone());
        let result = ready(fetch_tickers(&base, &adapter)).unwrap();
        if case["alias"] == "info" {
            assert_eq!(expected["before"][0]["info"]["marker"], "before");
            assert_eq!(expected["tickers"][0]["info"]["marker"], "later");
            assert_eq!(expected["source"]["data"]["ticker"][0]["marker"], "later");
            let mut source_row = row();
            source_row["marker"] = json!("later");
            assert_eq!(source_row["marker"], "later");
            assert_eq!(result[0].info.as_ref().unwrap()["marker"], "before");
        } else {
            assert_eq!(expected["before"][0]["last"], 3);
            assert_eq!(expected["tickers"][0]["last"], 77);
            assert_eq!(expected["store"][0]["last"], 77);
            let mut changed = result.clone();
            changed[0].last = 77.0;
            Store::update(
                adapter.store.as_ref(),
                StoreUpdate {
                    tickers: Some(changed),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            assert_eq!(base.store.tickers()[0].last, 77.0);
            assert_eq!(result[0].last, 3.0);
        }
    }
}
#[test]
fn ticker_response_and_string_refusals_are_visible() {
    let cases = vec![
        json!({"raw":"not-json"}),
        json!({"raw":""}),
        json!({"data":{"code":"bad","msg":77}}),
        json!({"data":{"code":"200000","data":{"ticker":{}}}}),
        json!({"raw":r#"{"code":"bad","msg":"\ud800"}"#,"surrogate":true}),
        json!({"failure":{"message":"network","response":{"raw":r#"{"msg":"\udfff"}"#}},"surrogate":true}),
    ];
    for (case, expected) in cases.iter().zip(source(&cases)) {
        let (base, adapter, runtime) = setup(case.clone());
        assert!(ready(fetch_tickers(&base, &adapter)).unwrap().is_empty());
        let errors = runtime.errors.take();
        assert_eq!(errors.len(), 1);
        if case["surrogate"] == true {
            assert!(expected["errorsJson"].as_str().unwrap().contains("\\ud"));
            assert!(errors[0].contains("representation refusal"));
            assert_ne!(errors[0], "network");
        } else {
            assert_ne!(json!(errors), expected["errors"]);
        }
    }
}
#[test]
fn ticker_drop_releases_transport_wait_twice() {
    let (base, adapter, runtime) = setup(json!({"pending":true}));
    for _ in 0..2 {
        let mut future = Box::pin(fetch_tickers(&base, &adapter));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(adapter.active.get());
        drop(future);
        assert!(!adapter.active.get());
    }
    assert!(runtime.errors.take().is_empty());
}

#[test]
fn ticker_oracle_missing_runtime_and_compiler_fail_loudly() {
    for (name, value) in [
        ("KUCOIN_TICKERS_MISSING_RUNTIME", "1"),
        (
            "SAFE_CEX_TYPESCRIPT_PATH",
            "/missing-kucoin-compiler-control",
        ),
    ] {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-tickers.cjs"
            ))
            .env(name, value)
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
    }
}

#[test]
fn ticker_inherited_output_refusal_follows_numeric_coercion() {
    let mut cases = Vec::new();
    for symbol in [
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
    ] {
        for field in ["last", "buy", "sell", "changeRate", "vol", "volValue"] {
            let mut r = row();
            r["symbol"] = json!(symbol);
            r[field] = json!({"toString":null});
            cases.push(
                json!({"data":{"code":"200000","data":{"ticker":[r]}},"existing":existing()}),
            );
        }
    }
    for (case, expected) in cases.iter().zip(source(&cases)) {
        assert_eq!(
            expected["errors"],
            json!(["Cannot convert object to primitive value"])
        );
        let (base, adapter, runtime) = setup(case.clone());
        let result = ready(fetch_tickers(&base, &adapter)).unwrap();
        assert_eq!(
            json!(runtime.errors.take()),
            expected["errors"],
            "ticker coercion precedes inherited output refusal: {case}"
        );
        assert_eq!(
            wire(serde_json::to_value(result).unwrap()),
            wire(expected["tickers"].clone())
        );
    }
}

#[test]
fn ticker_batch_coercion_precedes_inherited_output_refusal() {
    let mut cases = Vec::new();
    for symbol in [
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
    ] {
        for later in [
            Value::Null,
            json!({"symbol":symbol,"buy":{"toString":null}}),
        ] {
            cases.push(json!({"data":{"code":"200000","data":{"ticker":[{"symbol":symbol,"last":"2"},later]}},"existing":existing()}));
        }
    }
    for (case, expected) in cases.iter().zip(source(&cases)) {
        let (base, adapter, runtime) = setup(case.clone());
        let result = ready(fetch_tickers(&base, &adapter)).unwrap();
        assert_eq!(
            json!(runtime.errors.take()),
            expected["errors"],
            "ticker later-row failure precedes output refusal: {case}"
        );
        assert_eq!(
            wire(serde_json::to_value(result).unwrap()),
            wire(expected["tickers"].clone())
        );
    }
}
