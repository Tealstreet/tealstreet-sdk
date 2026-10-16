#[path = "support/numeric_bits.rs"]
mod numeric_bits;
use safe_cex_exchange::{
    exchange::{BaseExchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};
use safe_cex_types::{ExchangeName, Market};
use safe_cex_venue_kucoin::spot_reads::fetch_markets;
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
            if let Some(markets) = self.case.get("duringResponse") {
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
            "/scripts/spot-markets.cjs"
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
        "KUCOIN-CARDINALITY spot_markets source case count"
    );
    captures
}
fn scalar(n: f64) -> Value {
    if n.is_finite() {
        json!(if n == 0.0 { 0.0 } else { n })
    } else {
        json!({"$number":if n.is_nan(){"NaN"}else if n.is_sign_negative(){"-Infinity"}else{"Infinity"}})
    }
}
fn tagged(v: Value) -> Value {
    match v {
        Value::Number(n) => scalar(safe_cex_num::number_from_string(&n.to_string())),
        Value::Array(a) => Value::Array(a.into_iter().map(tagged).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, tagged(v))).collect()),
        v => v,
    }
}
fn snapshot(markets: &[Market]) -> (Value, Value) {
    let mut values = serde_json::to_value(markets).unwrap();
    let mut bits = vec![];
    for (market, v) in markets.iter().zip(values.as_array_mut().unwrap()) {
        let numbers = [
            market.precision.amount,
            market.precision.price,
            market.limits.amount.min,
            market.limits.amount.max,
        ];
        v["precision"]["amount"] = scalar(numbers[0]);
        v["precision"]["price"] = scalar(numbers[1]);
        v["limits"]["amount"]["min"] = scalar(numbers[2]);
        v["limits"]["amount"]["max"] = scalar(numbers[3]);
        bits.push(numbers.map(numeric_bits::bits));
    }
    (tagged(values), json!(bits))
}
fn row() -> Value {
    json!({"symbol":"BTC-USDT","baseCurrency":"BTC","quoteCurrency":"USDT","enableTrading":true,"baseIncrement":"0.01","priceIncrement":"0.1","baseMinSize":"2","baseMaxSize":"999","marker":"before"})
}
fn success(row: Value) -> Value {
    json!({"data":{"code":"200000","data":[row]}})
}
fn existing() -> Value {
    json!([{"id":"old","symbol":"OLD","base":"A","quote":"B","active":true,"precision":{"amount":1,"price":0.25},"limits":{"amount":{"min":0,"max":9},"leverage":{"min":1,"max":7}}}])
}
#[test]
fn live_spot_markets_preserve_numeric_bits_and_current_store_fallback() {
    let mut cases = vec![success(row()), json!({"data":{"code":"200000","data":[]}})];
    for field in [
        "baseIncrement",
        "priceIncrement",
        "baseMinSize",
        "baseMaxSize",
    ] {
        for value in [
            Value::Null,
            json!(false),
            json!(true),
            json!(0),
            json!(-2),
            json!(""),
            json!("1.5tail"),
            json!("-0"),
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
    for enabled in [json!(1), json!("true"), json!(false), Value::Null] {
        let mut r = row();
        r["enableTrading"] = enabled;
        cases.push(success(r));
    }
    for response in [
        Value::Null,
        json!({}),
        json!({"code":200000}),
        json!({"code":"bad","msg":"gateway"}),
        json!({"code":"bad","msg":false}),
        json!({"code":"200000"}),
        json!({"code":"200000","data":null}),
        json!({"code":"200000","data":{}}),
        json!({"code":"200000","data":[null]}),
    ] {
        cases.push(json!({"data":response,"existing":existing(),"duringResponse":[]}));
    }
    for msg in [json!("venue body"), json!(""), json!(false)] {
        cases.push(json!({"failure":{"message":"network","response":{"data":{"msg":msg}}},"existing":[],"duringResponse":existing()}));
    }
    for lexeme in ["-0", "-0.0", "1e400", "-1e400"] {
        let mut r = row();
        r["baseIncrement"] = json!("TOKEN");
        let raw = serde_json::to_string(&json!({"code":"200000","data":[r]}))
            .unwrap()
            .replace("\"TOKEN\"", lexeme);
        cases.push(json!({"raw":raw}));
    }
    let expected = source(&cases);
    assert_eq!(cases.len(), 82);
    for (case, expected) in cases.into_iter().zip(expected) {
        let (base, adapter, runtime) = setup(case.clone());
        let result = ready(fetch_markets(&base, &adapter)).unwrap();
        let (markets, bits) = snapshot(&result);
        assert_eq!(
            markets,
            tagged(expected["markets"].clone()),
            "live Spot market mapping: {case}"
        );
        assert_eq!(
            bits, expected["bits"],
            "Spot market pre-serialization bits: {case}"
        );
        assert_eq!(
            adapter.calls.take(),
            expected["calls"].as_array().unwrap().clone(),
            "Spot market request"
        );
        assert_eq!(
            json!(runtime.errors.take()),
            expected["errors"],
            "Spot market errors: {case}"
        );
    }
}
#[test]
fn market_source_numeric_mutation_fails_by_name() {
    let out = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_spot_markets_preserve_numeric_bits_and_current_store_fallback",
            "--nocapture",
        ])
        .env("KUCOIN_SPOT_MARKETS_MUTATION", "number")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(101));
    assert!(String::from_utf8_lossy(&out.stderr).contains("live Spot market mapping"));
}
#[test]
fn dropping_market_read_releases_transport_wait() {
    let (base, adapter, _) = setup(json!({"pending":true}));
    for _ in 0..2 {
        let mut future = Box::pin(fetch_markets(&base, &adapter));
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
}

#[test]
fn market_owned_snapshots_pin_both_alias_paths() {
    let mut corpus = Vec::new();
    for alias in ["info", "fallback"] {
        let mut case = if alias == "info" {
            success(row())
        } else {
            json!({"data":{"code":"bad"},"existing":existing()})
        };
        case["alias"] = json!(alias);
        corpus.push((alias, case));
    }
    let inputs: Vec<_> = corpus.iter().map(|row| row.1.clone()).collect();
    let captured = source(&inputs);
    assert_eq!(captured.len(), corpus.len());
    for ((alias, case), expected) in corpus.into_iter().zip(captured) {
        let (base, mut adapter, _) = setup(case);
        let result = ready(fetch_markets(&base, &adapter)).unwrap();
        assert_eq!(
            snapshot(&result).0,
            tagged(expected["alias"]["before"].clone())
        );
        if alias == "info" {
            adapter.case["data"]["data"][0]["marker"] = json!("later");
            assert_eq!(expected["markets"][0]["info"]["marker"], "later");
            assert_eq!(result[0].info.as_ref().unwrap()["marker"], "before");
            assert_eq!(adapter.case["data"], expected["alias"]["source"]);
        } else {
            let mut changed = base.store.markets().to_vec();
            changed[0].symbol = "LATER".into();
            Store::update(
                adapter.store.as_ref(),
                StoreUpdate {
                    markets: Some(changed),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
            assert_eq!(expected["markets"][0]["symbol"], "LATER");
            assert_eq!(result[0].symbol, "OLD");
            assert_eq!(base.store.markets()[0].symbol, "LATER");
        }
    }
}
#[test]
fn market_typed_field_and_error_payload_findings_are_explicit() {
    let mut corpus = Vec::new();
    for field in ["symbol", "baseCurrency", "quoteCurrency"] {
        for value in [None, Some(Value::Null), Some(json!(7)), Some(json!(false))] {
            let mut r = row();
            match value {
                Some(value) => {
                    r[field] = value;
                }
                None => {
                    r.as_object_mut().unwrap().remove(field);
                }
            }
            let mut case = success(r);
            case["existing"] = existing();
            corpus.push((case,));
        }
    }
    let inputs: Vec<_> = corpus.iter().map(|row| row.0.clone()).collect();
    let captured = source(&inputs);
    assert_eq!(captured.len(), corpus.len());
    for ((case,), expected) in corpus.into_iter().zip(captured) {
        assert_eq!(expected["markets"].as_array().unwrap().len(), 1);
        assert_eq!(expected["errors"], json!([]));
        let (base, adapter, runtime) = setup(case);
        let result = ready(fetch_markets(&base, &adapter)).unwrap();
        assert_eq!(result[0].symbol, "OLD");
        assert_eq!(runtime.errors.borrow().len(), 1);
        assert!(runtime.errors.borrow()[0].contains("requires a string representation"));
    }
    let case = json!({"data":{"code":"bad","msg":77},"existing":existing()});
    let expected = source(std::slice::from_ref(&case)).remove(0);
    let (base, adapter, runtime) = setup(case);
    let result = ready(fetch_markets(&base, &adapter)).unwrap();
    assert_eq!(snapshot(&result).0, tagged(expected["markets"].clone()));
    assert_eq!(expected["errors"], json!([77]));
    assert!(runtime.errors.borrow()[0].contains("error requires a string representation"));
}
#[test]
fn market_utf16_refusal_does_not_widen_to_scalar_unicode() {
    let mut corpus = Vec::new();
    for text in ["é", "😀", " \t\n", "constructor", "__proto__"] {
        let mut r = row();
        r["symbol"] = json!(text);
        let case = success(r);
        corpus.push((case,));
    }
    let inputs: Vec<_> = corpus.iter().map(|row| row.0.clone()).collect();
    let captured = source(&inputs);
    assert_eq!(captured.len(), corpus.len());
    for ((case,), expected) in corpus.into_iter().zip(captured) {
        let (base, adapter, _) = setup(case);
        assert_eq!(
            snapshot(&ready(fetch_markets(&base, &adapter)).unwrap()).0,
            tagged(expected["markets"].clone())
        );
    }
    let raw = r#"{"code":"200000","data":[{"symbol":"\ud800","baseCurrency":"BTC","quoteCurrency":"USDT"}]}"#;
    let case = json!({"raw":raw,"surrogate":true});
    let expected = source(std::slice::from_ref(&case)).remove(0);
    assert!(expected["markets"].as_str().unwrap().contains("\\ud800"));
    assert_eq!(expected["errors"], json!([]));
    let (base, adapter, runtime) = setup(case);
    assert!(ready(fetch_markets(&base, &adapter)).unwrap().is_empty());
    assert!(runtime.errors.borrow()[0].contains("representation refusal"));
}

#[test]
fn market_failure_body_surrogate_is_an_explicit_representation_refusal() {
    let case = json!({"failure":{"message":"network fallback","response":{"raw":r#"{"msg":"\ud800"}"#}},"existing":existing(),"failureSurrogate":true});
    let expected = source(std::slice::from_ref(&case)).remove(0);
    assert_eq!(expected["errorJson"], r#"["\ud800"]"#);
    let (base, adapter, runtime) = setup(case);
    let result = ready(fetch_markets(&base, &adapter)).unwrap();
    assert_eq!(snapshot(&result).0, tagged(expected["markets"].clone()));
    assert_eq!(runtime.errors.borrow().len(), 1);
    assert!(runtime.errors.borrow()[0].contains("representation refusal"));
    assert!(!runtime.errors.borrow()[0].contains("network fallback"));
    let mut corpus = Vec::new();
    for message in ["é", "😀", " \t\n"] {
        let case = json!({"failure":{"message":"network fallback","response":{"raw":serde_json::to_string(&json!({"msg":message})).unwrap()}},"existing":existing()});
        corpus.push((case,));
    }
    let inputs: Vec<_> = corpus.iter().map(|row| row.0.clone()).collect();
    let captured = source(&inputs);
    assert_eq!(captured.len(), corpus.len());
    for ((case,), expected) in corpus.into_iter().zip(captured) {
        let (base, adapter, runtime) = setup(case);
        let result = ready(fetch_markets(&base, &adapter)).unwrap();
        assert_eq!(snapshot(&result).0, tagged(expected["markets"].clone()));
        assert_eq!(json!(runtime.errors.take()), expected["errors"]);
    }
}

#[test]
fn market_coercions_and_later_rows_precede_typed_output_refusal() {
    let mut cases = Vec::new();
    for invalid in ["symbol", "baseCurrency", "quoteCurrency"] {
        for numeric in [
            "baseIncrement",
            "priceIncrement",
            "baseMinSize",
            "baseMaxSize",
        ] {
            let mut r = row();
            r[invalid] = Value::Null;
            r[numeric] = json!({"toString":null});
            cases.push(success(r));
        }
        let mut r = row();
        r[invalid] = Value::Null;
        for later in [
            Value::Null,
            json!({"baseCurrency":{"toString":null}}),
            json!({"baseCurrency":"A","quoteCurrency":"B","baseIncrement":{"toString":null}}),
        ] {
            cases.push(json!({"data":{"code":"200000","data":[r,later]}}));
        }
    }
    // Template interpolation can throw before any parseFloat runs.
    for field in ["baseCurrency", "quoteCurrency"] {
        let mut r = row();
        r[field] = json!({"toString":null});
        cases.push(success(r));
    }
    for c in &mut cases {
        c["existing"] = existing();
    }
    let expected = source(&cases);
    for (c, expected) in cases.into_iter().zip(expected) {
        let (base, adapter, runtime) = setup(c.clone());
        let result = ready(fetch_markets(&base, &adapter)).unwrap();
        assert_eq!(result[0].symbol, "OLD");
        assert_eq!(
            json!(runtime.errors.take()),
            expected["errors"],
            "market source error precedes typed output refusal: {c}"
        );
    }
}
#[test]
fn market_actual_axios_raw_body_staging_finding() {
    let mut corpus = Vec::new();
    for raw in ["", "not-json"] {
        let c = json!({"raw":raw,"existing":existing()});
        corpus.push((c,));
    }
    let inputs: Vec<_> = corpus.iter().map(|row| row.0.clone()).collect();
    let captured = source(&inputs);
    assert_eq!(captured.len(), corpus.len());
    for ((c,), expected) in corpus.into_iter().zip(captured) {
        assert_eq!(
            expected["errors"],
            json!(["Failed to fetch KuCoin Spot markets"])
        );
        let (base, adapter, runtime) = setup(c);
        let result = ready(fetch_markets(&base, &adapter)).unwrap();
        assert_eq!(result[0].symbol, "OLD");
        assert!(runtime.errors.borrow()[0].contains("representation refusal"));
    }
}

#[test]
fn market_runtime_and_compiler_fail_loudly() {
    for (key, value) in [
        ("KUCOIN_MARKETS_MISSING_RUNTIME", "1"),
        (
            "SAFE_CEX_TYPESCRIPT_PATH",
            "missing-kucoin-typescript-control",
        ),
    ] {
        let mut child = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/spot-markets.cjs"
            ))
            .env(key, value)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"[]").unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
    }
}

#[test]
fn market_cardinality_supplement_retains_each_native_row() {
    let mut second = row();
    second["symbol"] = json!("ETH-USDT");
    let case = json!({"data":{"code":"200000","data":[row(),second]}});
    let captured = source(std::slice::from_ref(&case));
    assert_eq!(
        captured.len(),
        1,
        "KUCOIN-CARDINALITY market supplement cases"
    );
    assert_eq!(
        captured[0]["markets"].as_array().unwrap().len(),
        2,
        "KUCOIN-CARDINALITY market source output total"
    );
    let (base, adapter, _) = setup(case);
    let result = ready(fetch_markets(&base, &adapter)).unwrap();
    assert_eq!(
        result.len(),
        2,
        "KUCOIN-CARDINALITY market native retained rows"
    );
    let mut comparisons = 0;
    for index in 0..2 {
        assert_eq!(
            result[index].symbol,
            ["BTC-USDT", "ETH-USDT"][index],
            "KUCOIN-CARDINALITY market native index coverage"
        );
        assert_eq!(
            result[index].symbol,
            captured[0]["markets"][index]["symbol"]
        );
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 2,
        "KUCOIN-CARDINALITY market native comparisons"
    );
}
