#[path = "support/query_observer.rs"]
mod query_observer;

use safe_cex_transport::{RestAdapter, RestFailure, RestFuture, RestRequest, RestResponse};
use safe_cex_venue_woo::legacy_fills::{
    WooLegacyFillsQuery, fetch_legacy_transaction_history_rows,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    process::Command,
    task::{Context, Poll, Waker},
};

fn observe(value: &Value) -> Value {
    match value {
        Value::Number(_) => {
            let n = safe_cex_num::js_number(Some(value)).unwrap();
            json!({"bits":format!("{:016x}", n.to_bits())})
        }
        Value::Array(values) => Value::Array(values.iter().map(observe).collect()),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(k, v)| (k.clone(), observe(v)))
                .collect(),
        ),
        other => other.clone(),
    }
}
struct Script {
    scenario: String,
    calls: RefCell<Vec<Value>>,
}
impl RestAdapter for Script {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            assert_eq!(request.method, "GET", "LEGACY request method");
            let index = self.calls.borrow().len();
            self.calls
                .borrow_mut()
                .push(json!({"url":request.url,"params":query_observer::params(&request)}));
            let error = match self.scenario.as_str() {
                "error" | "error-empty" | "error-rows" => index == 0,
                "two-errors" => index < 2,
                _ => false,
            };
            if error {
                let mut failure = RestFailure::pre_transmit(format!("failure-{index}"), request);
                failure.context = Some(Box::new(
                    json!({"message":format!("failure-{index}"),"marker":41+index}),
                ));
                failure.code = Some(format!("code-{index}"));
                return Err(failure);
            }
            let populated = match self.scenario.as_str() {
                "first" => index == 0,
                "second" | "error-rows" => index == 1,
                "last" => index == 4,
                _ => false,
            };
            let rows = if populated {
                json!([{"id":format!("first-{index}"),"qty":3.25},null,{"id":format!("last-{index}"),"qty":7.5}])
            } else {
                json!([])
            };
            let data = if self.scenario == "null-data" && index == 0 {
                Value::Null
            } else {
                json!({"data":{"rows":rows},"rows":[{"id":"wrong-container"}]})
            };
            Ok(RestResponse {
                status: 200,
                reason: None,
                headers: BTreeMap::new(),
                body: safe_cex_transport::RestResponseBody::Encoded(
                    serde_json::to_vec(&data).unwrap(),
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
        Poll::Pending => panic!("LEGACY fixture yielded"),
    }
}
#[test]
fn legacy_history_matches_source_requests_and_rows() {
    let run = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/legacy-history-oracle.cjs"
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
    assert_eq!(cases.len(), 90, "LEGACY source total");
    let mut compared = 0;
    let mut excluded = 0;
    let mut request_count = 0;
    let mut row_count = 0;
    let mut error_count = 0;
    for index in 0..90 {
        let case = cases.get(index).expect("LEGACY required source case");
        let scenario = case["scenario"].as_str().unwrap();
        if matches!(
            scenario,
            "error-null"
                | "error-false"
                | "error-zero"
                | "error-string"
                | "null-response"
                | "mutated-options"
        ) {
            excluded += 1;
            continue;
        }
        let query: WooLegacyFillsQuery = serde_json::from_value(case["opts"].clone()).unwrap();
        let adapter = Script {
            scenario: scenario.into(),
            calls: RefCell::new(Vec::new()),
        };
        let result = ready(fetch_legacy_transaction_history_rows(&adapter, &query));
        assert_eq!(
            observe(&Value::Array(adapter.calls.borrow().clone())),
            observe(&case["calls"]),
            "LEGACY native request sequence {index}"
        );
        request_count += adapter.calls.borrow().len();
        if let Some(expected) = case["outcome"].get("error") {
            error_count += 1;
            let error = result.expect_err("LEGACY expected retained error");
            assert_eq!(
                error.context.as_deref(),
                Some(expected),
                "LEGACY retained last error {index}"
            );
            assert_eq!(
                error.code.as_deref(),
                Some(
                    if scenario == "two-errors"
                        && query.symbol.as_deref().is_some_and(|s| !s.is_empty())
                    {
                        "code-1"
                    } else {
                        "code-0"
                    }
                ),
                "LEGACY original error code"
            );
        } else {
            let rows = result.unwrap();
            row_count += rows.len();
            let expected = case["outcome"]["rows"].as_array().unwrap();
            assert_eq!(
                rows.len(),
                expected.len(),
                "LEGACY native row cardinality {index}"
            );
            assert_eq!(
                observe(&json!(rows)),
                observe(&Value::Array(expected.clone())),
                "LEGACY native rows {index}"
            );
        }
        compared += 1;
    }
    assert_eq!(request_count, 158, "LEGACY native request total");
    assert_eq!(row_count, 54, "LEGACY native row total");
    assert_eq!(error_count, 20, "LEGACY native error total");
    assert_eq!(compared, 54, "LEGACY compared cases");
    assert_eq!(excluded, 36, "LEGACY named ABI boundary cases");
}

#[test]
fn legacy_native_parameter_bits_include_nan() {
    let mut observations = 0;
    for field in ["size", "start_t", "end_t", "page"] {
        for value in [-0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
            let mut query = WooLegacyFillsQuery {
                symbol: None,
                from_milliseconds: Some(13.25),
                to_milliseconds: Some(29.5),
                limit: 37.75,
                page: Some(7.5),
            };
            match field {
                "size" => query.limit = value,
                "start_t" => query.from_milliseconds = Some(value),
                "end_t" => query.to_milliseconds = Some(value),
                _ => query.page = Some(value),
            }
            let adapter = Script {
                scenario: "empty".into(),
                calls: RefCell::new(Vec::new()),
            };
            let result = ready(fetch_legacy_transaction_history_rows(&adapter, &query));
            assert!(result.unwrap().is_empty());
            let calls = adapter.calls.borrow();
            assert_eq!(calls.len(), 1, "LEGACY numeric single attempt {field}");
            assert_eq!(
                calls[0]["params"][field]["bits"].as_str().unwrap(),
                format!("{:016x}", value.to_bits()),
                "LEGACY native parameter bits {field}"
            );
            observations += 1;
        }
    }
    assert_eq!(
        observations, 16,
        "LEGACY numeric parameter observation total"
    );
}

#[test]
fn legacy_response_phase_preserves_native_rows() {
    use safe_cex_transport::RestResponseBody;
    struct Phase(RestResponseBody);
    impl RestAdapter for Phase {
        fn execute(&self, _: RestRequest) -> RestFuture<'_, RestResponse> {
            Box::pin(async move {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: BTreeMap::new(),
                    body: self.0.clone(),
                })
            })
        }
    }
    let data = json!({"rows":[{"id":"first"},null,{"id":"last"}]});
    let phases = [
        RestResponseBody::Encoded(serde_json::to_vec(&data).unwrap()),
        RestResponseBody::Untransformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(data.clone()).unwrap(),
        ),
        RestResponseBody::Transformed(
            safe_cex_types::retained_json::RetainedJsonInput::from_value(data.clone()).unwrap(),
        ),
    ];
    let query = WooLegacyFillsQuery {
        symbol: None,
        limit: 3.0,
        from_milliseconds: None,
        to_milliseconds: None,
        page: None,
    };
    let mut count = 0;
    for phase in phases {
        let result = ready(fetch_legacy_transaction_history_rows(&Phase(phase), &query)).unwrap();
        assert_eq!(result.len(), 3, "LEGACY response phase row count");
        assert_eq!(
            result,
            data["rows"]
                .as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(safe_cex_types::retained_json::RetainedJsonInput::from)
                .collect::<Vec<_>>(),
            "LEGACY response phase native members"
        );
        count += 1;
    }
    assert_eq!(count, 3, "LEGACY response phase inventory");
}
