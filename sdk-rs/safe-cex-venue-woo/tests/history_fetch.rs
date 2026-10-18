#[path = "support/query_observer.rs"]
mod query_observer;

use safe_cex_transport::{
    RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse, RestResponseBody,
};
use safe_cex_venue_woo::{
    history_fetch::fetch_transaction_history_rows, legacy_fills::WooLegacyFillsQuery,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    process::Command,
    task::{Context, Poll, Waker},
};

struct Script {
    scenario: String,
    calls: RefCell<Vec<Value>>,
    attempts: RefCell<[usize; 2]>,
}
impl RestAdapter for Script {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            assert_eq!(request.method, "GET", "HISTORY native method");
            let primary = request.url == "/v3/trade/transactionHistory";
            let phase = if primary { "primary" } else { "legacy" };
            let slot = usize::from(!primary);
            let ordinal = self.attempts.borrow()[slot];
            self.attempts.borrow_mut()[slot] += 1;
            self.calls.borrow_mut().push(
                json!({"phase":phase,"ordinal":ordinal,"url":request.url,"params":query_observer::params(&request)}),
            );
            let error = ordinal == 0
                && match self.scenario.as_str() {
                    "primary-error" | "primary-error-legacy-rows" => primary,
                    "both-errors" => true,
                    "legacy-error" => !primary,
                    _ => false,
                };
            if error {
                let mut failure =
                    RestFailure::pre_transmit(format!("{phase}-error-{ordinal}"), request);
                failure.context = Some(Box::new(
                    json!({"phase":phase,"ordinal":ordinal,"message":format!("{phase}-error-{ordinal}")}),
                ));
                failure.code = Some(format!("{phase}-{ordinal}"));
                return Err(failure);
            }
            let populated = match self.scenario.as_str() {
                "primary-first" => primary && ordinal == 0,
                "primary-second" => primary && ordinal == 1,
                "primary-unqualified" => primary && ordinal == 4,
                "legacy-first" | "primary-error-legacy-rows" => !primary && ordinal == 0,
                "legacy-last" => !primary && ordinal == 4,
                _ => false,
            };
            let rows = if populated {
                json!([{"phase":phase,"id":"first","qty":3.25},null,{"phase":phase,"id":"last","qty":7.5}])
            } else {
                json!([])
            };
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: RestResponseBody::Transformed(
                    safe_cex_types::retained_json::RetainedJsonInput::from_value(
                        json!({"data":{"rows":rows},"rows":[{"id":"wrong-container"}]}),
                    )
                    .unwrap(),
                ),
            })
        })
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    match std::pin::pin!(future)
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("HISTORY unexpected yield"),
    }
}
fn observe(value: &Value) -> Value {
    match value {
        Value::Number(_) => {
            json!({"bits":format!("{:016x}",safe_cex_num::js_number(Some(value)).unwrap().to_bits())})
        }
        Value::Array(v) => Value::Array(v.iter().map(observe).collect()),
        Value::Object(v) => Value::Object(v.iter().map(|(k, v)| (k.clone(), observe(v))).collect()),
        other => other.clone(),
    }
}
#[test]
fn history_fetch_matches_source_order_and_error_precedence() {
    let run = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-fetch-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let source: Value = serde_json::from_slice(&run.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 72, "HISTORY source total");
    let (mut compared, mut excluded, mut requests, mut rows_total, mut errors) = (0, 0, 0, 0, 0);
    for index in 0..72 {
        let case = cases.get(index).expect("HISTORY required source case");
        let scenario = case["scenario"].as_str().unwrap();
        if matches!(
            scenario,
            "primary-null"
                | "primary-false"
                | "primary-zero"
                | "primary-string"
                | "primary-undefined"
                | "primary-error-null"
                | "primary-error-legacy-null"
                | "mutated-options"
        ) {
            excluded += 1;
            continue;
        }
        let query: WooLegacyFillsQuery = serde_json::from_value(case["opts"].clone()).unwrap();
        let adapter = Script {
            scenario: scenario.into(),
            calls: RefCell::new(Vec::new()),
            attempts: RefCell::new([0, 0]),
        };
        let result = ready(fetch_transaction_history_rows(&adapter, &query));
        assert_eq!(
            observe(&json!(*adapter.calls.borrow())),
            observe(&case["calls"]),
            "HISTORY native request sequence {index}"
        );
        requests += adapter.calls.borrow().len();
        if case["outcome"]["threw"] == true {
            let error = result.expect_err("HISTORY expected retained error");
            assert_eq!(
                error.context.as_deref(),
                Some(&case["outcome"]["error"]),
                "HISTORY native error precedence"
            );
            let phase = case["outcome"]["error"]["phase"].as_str().unwrap();
            assert_eq!(
                error.code,
                Some(format!("{phase}-0")),
                "HISTORY original error code"
            );
            errors += 1;
        } else {
            let rows = result.unwrap();
            let expected = case["outcome"]["rows"].as_array().unwrap();
            assert_eq!(rows.len(), expected.len(), "HISTORY native row cardinality");
            assert_eq!(
                observe(&json!(rows)),
                observe(&json!(expected)),
                "HISTORY native rows"
            );
            rows_total += rows.len();
        }
        compared += 1;
    }
    assert_eq!(
        (compared, excluded),
        (40, 32),
        "HISTORY comparison inventory"
    );
    assert_eq!(
        (requests, rows_total, errors),
        (160, 42, 12),
        "HISTORY native observation totals"
    );
}

#[test]
fn primary_parameter_presence_and_native_numbers() {
    let mut query = WooLegacyFillsQuery {
        symbol: Some("btcusdt".into()),
        from_milliseconds: None,
        to_milliseconds: None,
        limit: 37.0,
        page: Some(5.0),
    };
    let adapter = Script {
        scenario: "empty".into(),
        calls: RefCell::new(Vec::new()),
        attempts: RefCell::new([0, 0]),
    };
    assert!(
        ready(fetch_transaction_history_rows(&adapter, &query))
            .unwrap()
            .is_empty()
    );
    let calls = adapter.calls.borrow();
    assert_eq!(calls.len(), 10, "HISTORY page request count");
    for index in 0..10 {
        let params = calls[index]["params"].as_object().unwrap();
        for key in ["startTime", "endTime", "start_t", "end_t"] {
            assert!(!params.contains_key(key), "HISTORY missing bounds omitted");
        }
        if index < 5 {
            assert!(!params.contains_key("page"), "HISTORY primary omits page");
        } else {
            assert_eq!(
                params["page"]["bits"].as_str().unwrap(),
                format!("{:016x}", 5.0f64.to_bits()),
                "HISTORY legacy retains page"
            );
        }
    }
    query.symbol = None;
    let mut observations = 0;
    for field in 0..3 {
        for number in [-0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let mut input = query.clone();
            match field {
                0 => input.limit = number,
                1 => input.from_milliseconds = Some(number),
                _ => input.to_milliseconds = Some(number),
            }
            let adapter = Script {
                scenario: "empty".into(),
                calls: RefCell::new(Vec::new()),
                attempts: RefCell::new([0, 0]),
            };
            let result = ready(fetch_transaction_history_rows(&adapter, &input));
            assert!(result.unwrap().is_empty());
            let calls = adapter.calls.borrow();
            assert_eq!(calls.len(), 1, "HISTORY numeric direct request count");
            let key = ["limit", "startTime", "endTime"][field];
            assert_eq!(
                calls[0]["params"][key]["bits"].as_str().unwrap(),
                format!("{:016x}", number.to_bits()),
                "HISTORY primary numeric bits {key}"
            );
            observations += 1;
        }
    }
    assert_eq!(observations, 12, "HISTORY primary numeric inventory");
}
