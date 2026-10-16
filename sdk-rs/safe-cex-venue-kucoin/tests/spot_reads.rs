#[path = "support/numeric_bits.rs"]
mod numeric_bits;
use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};
use safe_cex_venue_kucoin::spot_reads::fetch_active_order_symbols;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    task::{Context, Poll, Waker},
};
struct Adapter {
    case: Value,
    calls: RefCell<Vec<Value>>,
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
            assert!(request.params.is_none());
            self.calls
                .borrow_mut()
                .push(json!({"method":request.method,"url":request.url}));
            if self.case["pending"] == true {
                self.active.set(true);
                let _guard = Pending(&self.active);
                std::future::pending::<()>().await;
            }
            if self.case.get("failure").is_some() {
                return Err(RestFailure::pre_transmit("fixture".into(), request));
            }
            let body = self.case["raw"]
                .as_str()
                .map(str::as_bytes)
                .map(Vec::from)
                .unwrap_or_else(|| serde_json::to_vec(&self.case["data"]).unwrap());
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(body),
            })
        })
    }
}
fn adapter(case: Value) -> Adapter {
    Adapter {
        case,
        calls: RefCell::new(vec![]),
        active: Cell::new(false),
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected pending"),
    }
}
fn source(cases: &[Value]) -> Vec<Value> {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-reads.cjs"
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
        "KUCOIN-CARDINALITY spot_reads source case count"
    );
    captures
}
fn tagged(value: Value) -> Value {
    match value {
        Value::Array(v) => Value::Array(v.into_iter().map(tagged).collect()),
        Value::Object(v) => Value::Object(v.into_iter().map(|(k, v)| (k, tagged(v))).collect()),
        Value::Number(n) => {
            let n = safe_cex_num::number_from_string(&n.to_string());
            if n.is_infinite() {
                json!({"$number":if n.is_sign_negative(){"-Infinity"}else{"Infinity"}})
            } else {
                json!(if n == 0.0 { 0.0 } else { n })
            }
        }
        v => v,
    }
}
#[test]
fn live_spot_symbol_discovery_keeps_array_entries_and_catches_failures() {
    let mut cases = vec![
        json!({"data":{"code":"200000","data":{"symbols":["BTC-USDT",null,1,false,{},[],"constructor","__proto__","é","😀","\u{feff}","\u{85}"]}}}),
        json!({"failure":{"message":"network"}}),
    ];
    for data in [
        Value::Null,
        json!(false),
        json!(1),
        json!("x"),
        json!([]),
        json!({}),
        json!({"symbols":["x"]}),
    ] {
        cases.push(json!({"data":data}));
        cases.push(json!({"data":{"code":"200000","data":data}}));
        cases.push(json!({"data":{"code":"200000","data":{"symbols":data}}}));
    }
    for code in [
        json!(200000),
        json!(true),
        Value::Null,
        json!("bad"),
        json!("200000 "),
    ] {
        cases.push(json!({"data":{"code":code,"data":{"symbols":["x"]}}}));
    }
    cases.push(json!({"raw":r#"{"code":"200000","data":{"symbols":[1e400,-1e400,-0]}}"#}));
    let expected = source(&cases);
    assert_eq!(cases.len(), 29);
    for (case, expected) in cases.into_iter().zip(expected) {
        let xhr = adapter(case.clone());
        let actual = ready(fetch_active_order_symbols(&xhr));
        assert_eq!(
            tagged(json!(actual)),
            tagged(expected["result"].clone()),
            "live spot active-symbol array boundary: {case}"
        );
        assert_eq!(json!(*xhr.calls.borrow()), expected["calls"]);
    }
}
#[test]
fn symbol_array_is_live_in_source_and_owned_in_rust() {
    let case = json!({"data":{"code":"200000","data":{"symbols":["original"]}},"alias":true});
    let expected = source(std::slice::from_ref(&case));
    assert_eq!(
        expected[0]["alias"],
        json!({"same":true,"before":"[\"original\"]","result":["later","added"],"gateway":["later","added"]})
    );
    let mut xhr = adapter(case);
    let actual = ready(fetch_active_order_symbols(&xhr));
    xhr.case["data"]["data"]["symbols"] = json!(["later", "added"]);
    assert_eq!(actual, vec![json!("original")]);
    assert_eq!(
        xhr.case["data"]["data"]["symbols"],
        json!(["later", "added"])
    );
}
#[test]
fn valid_lone_surrogate_json_is_a_representation_refusal() {
    let case = json!({"raw":r#"{"code":"200000","data":{"symbols":["\ud800"]}}"#,"rawOnly":true});
    let expected = source(std::slice::from_ref(&case));
    assert_eq!(expected[0]["json"], r#"["\ud800"]"#);
    assert!(ready(fetch_active_order_symbols(&adapter(case))).is_empty());
}
#[test]
fn discovery_drop_releases_transport_wait_without_latching() {
    let xhr = adapter(json!({"pending":true}));
    for calls in 1..=2 {
        let mut future = Box::pin(fetch_active_order_symbols(&xhr));
        assert!(
            future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        assert!(xhr.active.get());
        drop(future);
        assert!(!xhr.active.get());
        assert_eq!(xhr.calls.borrow().len(), calls);
    }
}
#[test]
fn source_symbol_coercion_control_fails_by_name() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_spot_symbol_discovery_keeps_array_entries_and_catches_failures",
            "--nocapture",
        ])
        .env("KUCOIN_SPOT_READ_MUTATION", "coerce-symbols")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("live spot active-symbol array boundary")
    );
}

#[test]
fn discovery_keeps_overflow_and_negative_zero_bits_before_json_projection() {
    let case = json!({"raw":r#"{"code":"200000","data":{"symbols":[-0,-0.0,-0e300,0,1e400,-1e400,1.5]}}"#,"bits":true});
    let expected = source(std::slice::from_ref(&case));
    let actual = ready(fetch_active_order_symbols(&adapter(case)));
    let bits: Vec<String> = actual
        .iter()
        .map(
            |value| match safe_cex_num::JsJsonValue::from(value.clone()) {
                safe_cex_num::JsJsonValue::Number(number) => numeric_bits::bits(number),
                _ => panic!("numeric entry changed shape"),
            },
        )
        .collect();
    assert_eq!(
        json!(bits),
        expected[0]["bits"],
        "raw overflow and signed-zero bit preservation"
    );
    assert_eq!(
        &bits[..3],
        &["8000000000000000", "8000000000000000", "8000000000000000"]
    );
    assert_eq!(&bits[4..6], &["7ff0000000000000", "fff0000000000000"]);
}

#[test]
fn discovery_actual_axios_raw_strings_still_return_empty() {
    let cases = [json!({"raw":""}), json!({"raw":"not-json"})];
    for (case, expected) in cases.iter().zip(source(&cases)) {
        let xhr = adapter(case.clone());
        let actual = ready(fetch_active_order_symbols(&xhr));
        assert_eq!(
            expected["result"],
            json!([]),
            "actual Axios discovery fallback"
        );
        assert!(actual.is_empty());
        assert_eq!(json!(*xhr.calls.borrow()), expected["calls"]);
    }
}

#[test]
fn discovery_cardinality_supplement_retains_each_native_value() {
    let case = json!({"data":{"code":"200000","data":{"symbols":["BTC-USDT","ETH-USDT"]}}});
    let captured = source(std::slice::from_ref(&case));
    assert_eq!(
        captured.len(),
        1,
        "KUCOIN-CARDINALITY discovery supplement cases"
    );
    assert_eq!(
        captured[0]["result"].as_array().unwrap().len(),
        2,
        "KUCOIN-CARDINALITY discovery source output total"
    );
    let xhr = adapter(case);
    let result = ready(fetch_active_order_symbols(&xhr));
    assert_eq!(
        result.len(),
        2,
        "KUCOIN-CARDINALITY discovery native retained rows"
    );
    let mut comparisons = 0;
    for index in 0..2 {
        assert_eq!(
            result[index],
            ["BTC-USDT", "ETH-USDT"][index],
            "KUCOIN-CARDINALITY discovery native index coverage"
        );
        assert_eq!(result[index], captured[0]["result"][index]);
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 2,
        "KUCOIN-CARDINALITY discovery native comparisons"
    );
}
