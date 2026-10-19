//! Audit probes reach the configured class and its signed terminal HTTP path.
use crate::binding_test_harness::{harness, options};
use safe_cex_exchange::js_runtime::value::Js;
use safe_cex_types::{ExchangeName, ExecutionsOptions};
use serde_json::json;
use std::process::Command;

const HISTORY: &str = "/v3/trade/transactionHistory";

#[test]
fn history_nan_limit_reaches_the_signed_wire_before_empty_slice() {
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("GET", HISTORY, json!({"rows":[]}))
    });
    let mut opts: ExecutionsOptions = serde_json::from_value(json!({})).unwrap();
    opts.limit = Some(f64::NAN);
    let fills = h.drive(h.venue().fetch_historical_fills(opts)).unwrap();
    assert!(fills.is_empty());
    let requests = h.http.calls(HISTORY);
    assert_eq!(
        requests.len(),
        1,
        "source sends NaN limit before slice(0,NaN)"
    );
    assert_eq!(requests[0].params["limit"], "NaN");
    assert_source_signatures(&h, "fills");
    assert!(
        !requests[0].headers["x-api-signature"]
            .as_str()
            .unwrap()
            .is_empty()
    );
}

fn assert_source_signatures(h: &crate::binding_test_harness::Harness, kind: &str) {
    let requests = h.http.requests.borrow();
    let timestamps: Vec<_> = requests
        .iter()
        .map(|request| request.headers["x-api-timestamp"].clone())
        .collect();
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/history-query-source.cjs"
        ))
        .arg(json!({"kind":kind,"timestamps":timestamps}).to_string())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(requests.len(), expected.as_array().unwrap().len());
    for (request, expected) in requests.iter().zip(expected.as_array().unwrap()) {
        assert_eq!(
            format!("{}?{}", request.url, request.query.as_ref().unwrap()),
            expected["url"]
        );
        assert_eq!(
            request.headers["x-api-signature"], expected["signature"],
            "maintained signV3 exact HMAC"
        );
    }
}

#[test]
fn internal_and_legacy_history_keep_nan_until_their_actual_signers() {
    for (kind, method) in [
        ("internal", "fetchHistoricalTradesInternal"),
        ("legacy", "fetchLegacyTransactionHistoryRows"),
    ] {
        let h = harness(ExchangeName::Woo, options(), |http| {
            http.ok("GET", HISTORY, json!({"rows":[]}));
            http.ok("GET", "/v1/client/trades", json!({"rows":[]}));
        });
        let opts = if kind == "legacy" {
            Js::object(vec![("limit", Js::Number(f64::NAN))])
        } else {
            Js::object(vec![
                ("symbol", Js::str("BTCUSDT")),
                ("limit", Js::Number(f64::NAN)),
            ])
        };
        let rows: Vec<serde_json::Value> = h.drive(h.exchange.invoke(method, vec![opts])).unwrap();
        assert!(rows.is_empty());
        assert_eq!(
            h.http.requests.borrow().len(),
            if kind == "legacy" { 1 } else { 10 }
        );
        assert_source_signatures(&h, kind);
    }
}

#[test]
fn rest_adapter_preserves_every_native_query_kind_before_signing() {
    use safe_cex_exchange::js_runtime::client::JsRestAdapter;
    use safe_cex_transport::{RestAdapter, RestRequest};
    use safe_cex_types::QueryValue as Q;
    use std::{cell::RefCell, rc::Rc};
    let h = harness(ExchangeName::Woo, options(), |http| {
        http.ok("GET", HISTORY, json!({"rows":[]}))
    });
    let xhr = h.exchange.rt.this.get("xhr").unwrap();
    let seen = Rc::new(RefCell::new(None));
    let capture = seen.clone();
    let observer = Js::function("observeQuery", move |_, args| {
        let config = args[0].clone();
        *capture.borrow_mut() = Some(config.get("params")?);
        Ok(config)
    });
    xhr.get("interceptors")
        .unwrap()
        .get("request")
        .unwrap()
        .call_method("use", vec![observer])
        .unwrap();
    let adapter = JsRestAdapter::new(h.runtime.clone(), xhr);
    let query = Q::Object(
        [
            ("undefined".into(), Q::Undefined),
            ("null".into(), Q::Null),
            ("bool".into(), Q::Bool(true)),
            ("nan".into(), Q::Number(f64::NAN)),
            ("infinity".into(), Q::Number(f64::INFINITY)),
            ("zero".into(), Q::Number(-0.0)),
            ("string".into(), Q::String("two words".into())),
            (
                "array".into(),
                Q::Array(vec![
                    Q::Undefined,
                    Q::Number(f64::NEG_INFINITY),
                    Q::Null,
                    Q::Number(-0.0),
                ]),
            ),
            (
                "object".into(),
                Q::Object([("x".into(), Q::Bool(false))].into_iter().collect()),
            ),
        ]
        .into_iter()
        .collect(),
    );
    let request = RestRequest {
        method: "GET".into(),
        url: HISTORY.into(),
        query_params: Some(query),
        ..Default::default()
    };
    h.drive(Box::pin(async {
        adapter.execute(request).await.map_err(Into::into)
    }))
    .unwrap();
    let params = seen
        .borrow()
        .clone()
        .expect("native query reaches real request interceptors");
    assert_eq!(params.own_keys().len(), 9);
    assert!(params.has_own("undefined") && params.get("undefined").unwrap().is_undefined());
    assert!(matches!(params.get("null").unwrap(), Js::Null));
    assert!(params.get("bool").unwrap().strict_equals(&Js::Bool(true)));
    assert!(matches!(params.get("nan").unwrap(),Js::Number(value) if value.is_nan()));
    for (key, expected) in [("zero", -0.0f64), ("infinity", f64::INFINITY)] {
        let Js::Number(actual) = params.get(key).unwrap() else {
            panic!("native number {key}");
        };
        assert_eq!(actual.to_bits(), expected.to_bits());
    }
    assert!(
        params
            .get("string")
            .unwrap()
            .strict_equals(&Js::str("two words"))
    );
    let array = params.get("array").unwrap();
    assert!(array.is_array());
    assert_eq!(array.get("length").unwrap().to_number().unwrap(), 4.0);
    assert!(array.has_own("0") && array.get("0").unwrap().is_undefined());
    assert!(
        array
            .get("1")
            .unwrap()
            .strict_equals(&Js::Number(f64::NEG_INFINITY))
    );
    assert!(matches!(array.get("2").unwrap(), Js::Null));
    let Js::Number(zero) = array.get("3").unwrap() else {
        panic!("nested native zero");
    };
    assert_eq!(zero.to_bits(), (-0.0f64).to_bits());
    assert!(
        params
            .get("object")
            .unwrap()
            .get("x")
            .unwrap()
            .strict_equals(&Js::Bool(false))
    );
    assert_eq!(h.http.calls(HISTORY).len(), 1);
    assert_source_signatures(&h, "tree");
}

#[test]
fn rest_adapter_refuses_conflicting_query_carriers_before_any_http() {
    use safe_cex_exchange::js_runtime::client::JsRestAdapter;
    use safe_cex_transport::{RestAdapter, RestRequest};
    let h = harness(ExchangeName::Woo, options(), |_| {});
    let adapter = JsRestAdapter::new(h.runtime.clone(), h.exchange.rt.this.get("xhr").unwrap());
    let request = RestRequest {
        method: "GET".into(),
        url: HISTORY.into(),
        params: Some(json!({"limit":1})),
        query_params: Some(safe_cex_types::QueryValue::Number(f64::NAN)),
        ..Default::default()
    };
    let error = h
        .drive(Box::pin(async { Ok(adapter.execute(request).await) }))
        .unwrap()
        .unwrap_err();
    assert_eq!(
        error.message,
        "Cannot combine legacy and native query parameters"
    );
    let thrown = adapter.thrown(error);
    assert!(
        thrown
            .value
            .get("name")
            .unwrap()
            .strict_equals(&Js::str("TypeError"))
    );
    assert!(h.http.requests.borrow().is_empty());
}

#[test]
fn history_reports_retained_response_messages_like_the_source() {
    assert_history_reports(false);
}

#[test]
fn history_reports_retained_utf16_messages_like_the_source() {
    assert_history_reports(true);
}

fn assert_history_reports(surrogates: bool) {
    use std::{cell::RefCell, rc::Rc};
    let cases = [
        r#"{"message":7}"#,
        r#"{"message":true}"#,
        r#"{"message":{"venue":"busy","attempt":2}}"#,
        r#"{"message":["busy",7]}"#,
        r#"{"message":{"message":"nested reason"}}"#,
        r#"{"message":"prefix\ud800suffix\udc00"}"#,
        r#"{"message":["\udc00",7]}"#,
        r#"{"message":{"message":"\ud800"}}"#,
        r#"{"message":0}"#,
        r#"{"message":false}"#,
        r#"{"message":""}"#,
        r#"{"message":null}"#,
        r#"{}"#,
    ];
    for method in ["fetchHistoricalFills", "fetchHistoricalTradesInternal"] {
        for raw in cases
            .into_iter()
            .filter(|raw| raw.contains("\\ud") == surrogates)
        {
            let source = Command::new("node")
                .args(["--import", "tsx"])
                .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/history-query-source.cjs"))
                .arg(json!({"kind":"report", "method":method, "raw":raw, "timestamps":vec![1700000000000_i64; 20]}).to_string())
                .output().unwrap();
            assert!(
                source.status.success(),
                "source {method} {raw}: {}",
                String::from_utf8_lossy(&source.stderr)
            );
            let expected: serde_json::Value = serde_json::from_slice(&source.stdout).unwrap();
            let h = harness(ExchangeName::Woo, options(), |http| {
                let input: safe_cex_types::retained_json::RetainedJsonInput =
                    serde_json::from_str(raw).unwrap();
                let body = safe_cex_exchange::js_runtime::value::from_retained_input(input);
                http.route_native("GET", HISTORY, 500.0, body.clone());
                http.route_native("GET", "/v1/client/trades", 500.0, body);
            });
            let seen = Rc::new(RefCell::new(Vec::new()));
            let record = seen.clone();
            h.venue().base().on(
                "error",
                Rc::new(move |_, value| {
                    let text = value.as_str().expect("native error string");
                    record.borrow_mut().push(text.to_vec());
                    Ok(())
                }),
            );
            let args = if method == "fetchHistoricalFills" {
                Js::object(vec![])
            } else {
                Js::object(vec![("symbol", Js::str("BTCUSDT"))])
            };
            let result: Vec<serde_json::Value> = h
                .drive(h.exchange.invoke(method, vec![args]))
                .unwrap_or_else(|error| {
                    panic!("source reports then resolves []: {method} {raw}: {error:?}")
                });
            assert!(result.is_empty());
            let units: Vec<u16> = serde_json::from_value(expected["units"].clone()).unwrap();
            assert_eq!(
                &*seen.borrow(),
                &[units],
                "native UTF16 message: {method} {raw}"
            );
            let requests = h.http.requests.borrow();
            assert_eq!(
                requests.len(),
                expected["calls"].as_u64().unwrap() as usize,
                "request count: {method} {raw}"
            );
            for request in requests.iter() {
                assert_eq!(request.method, "GET");
                assert!(
                    !request.headers["x-api-signature"]
                        .as_str()
                        .unwrap()
                        .is_empty()
                );
            }
        }
    }
}
