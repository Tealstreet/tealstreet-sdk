use super::*;
use safe_cex_transport::query::{QueryArrayFormat, QueryProfile, query_json_projection};
use safe_cex_types::QueryValue;

const INPUT_IDS: [&str; 6] = ["array", "string", "number", "boolean", "null", "absent"];
const QUERIES: [&str; 6] = [
    "0=17&1=23&requestId=id-1",
    "0=a&1=b&requestId=id-1",
    "requestId=id-1",
    "requestId=id-1",
    "requestId=id-1",
    "requestId=id-1",
];
const SIGNATURES: [&str; 6] = [
    "8760a29167829555e1b4ee515b7abe17310e8bc9a6bf562bf3ed434f7822cd2e",
    "6012c672821208b4874110293d28d2cb713ee5469761f8f2b9e96f1ff05167ae",
    GET_ONE,
    GET_ONE,
    GET_ONE,
    GET_ONE,
];

fn legacy_input(id: &str) -> Option<Value> {
    match id {
        "array" => Some(json!([17, 23])),
        "string" => Some(json!("ab")),
        "number" => Some(json!(7)),
        "boolean" => Some(json!(false)),
        "null" => Some(Value::Null),
        "absent" => None,
        _ => panic!("unknown fixed params input"),
    }
}
fn native_input(id: &str) -> QueryValue {
    match id {
        "array" => QueryValue::Array(vec![QueryValue::Number(17.0), QueryValue::Number(23.0)]),
        "string" => QueryValue::String("ab".into()),
        "number" => QueryValue::Number(7.0),
        "boolean" => QueryValue::Bool(false),
        "null" => QueryValue::Null,
        "absent" => QueryValue::Undefined,
        _ => panic!("unknown fixed params input"),
    }
}

pub fn collect_native() -> Vec<Value> {
    assert_eq!(INPUT_IDS.len(), 6, "factory_params_native_declared_inputs");
    let mut cases = Vec::new();
    for carrier in ["legacy", "native"] {
        for signed in [false, true] {
            for (index, input) in INPUT_IDS.into_iter().enumerate() {
                let id = format!("{}-{input}", if signed { "signed" } else { "unsigned" });
                let options: ExchangeOptions = serde_json::from_value(if signed {
                    json!({"key":"key","secret":"secret"})
                } else {
                    json!({"key":"","secret":""})
                })
                .unwrap();
                let f = fixture_with_options(
                    vec![response(r#"{"retCode":0,"retMsg":"OK"}"#)],
                    true,
                    options,
                );
                let mut request = get("/private");
                if carrier == "legacy" {
                    request.params = legacy_input(input);
                } else {
                    request.query_params = Some(native_input(input));
                }
                let execution = ready(f.api.execute_with_receipt(request)).unwrap();
                let requests = f.wire.requests.borrow();
                assert_eq!(
                    requests.len(),
                    1,
                    "factory_params_native_attempts {carrier}/{id}"
                );
                let keys = if index < 2 {
                    vec!["0", "1", "requestId"]
                } else {
                    vec!["requestId"]
                };
                let params = if carrier == "legacy" {
                    assert!(
                        execution.config.query_params.is_none(),
                        "factory_params_native_legacy_exclusive {id}"
                    );
                    let params = execution
                        .config
                        .params
                        .as_ref()
                        .unwrap()
                        .as_object()
                        .unwrap();
                    assert_eq!(
                        params.keys().map(String::as_str).collect::<Vec<_>>(),
                        keys,
                        "factory_params_native_legacy_keys {id}"
                    );
                    assert_eq!(
                        params["requestId"], "id-1",
                        "factory_params_native_legacy_request_id {id}"
                    );
                    if input == "array" {
                        assert_eq!(
                            (&params["0"], &params["1"]),
                            (&json!(17), &json!(23)),
                            "factory_params_native_legacy_array_values {id}"
                        );
                    } else if input == "string" {
                        assert_eq!(
                            (&params["0"], &params["1"]),
                            (&json!("a"), &json!("b")),
                            "factory_params_native_legacy_string_values {id}"
                        );
                    }
                    Value::Object(params.clone())
                } else {
                    assert!(
                        execution.config.params.is_none(),
                        "factory_params_native_query_exclusive {id}"
                    );
                    let Some(QueryValue::Object(params)) = execution.config.query_params.as_ref()
                    else {
                        panic!("factory_params_native_object_variant {id}");
                    };
                    assert_eq!(
                        params.keys().map(String::as_str).collect::<Vec<_>>(),
                        keys,
                        "factory_params_native_query_keys {id}"
                    );
                    assert_eq!(
                        params.get("requestId"),
                        Some(&QueryValue::String("id-1".into())),
                        "factory_params_native_query_request_id {id}"
                    );
                    if input == "array" {
                        assert_eq!(
                            params.get("0"),
                            Some(&QueryValue::Number(17.0)),
                            "factory_params_native_query_array_first {id}"
                        );
                        assert_eq!(
                            params.get("1"),
                            Some(&QueryValue::Number(23.0)),
                            "factory_params_native_query_array_second {id}"
                        );
                    } else if input == "string" {
                        assert_eq!(
                            params.get("0"),
                            Some(&QueryValue::String("a".into())),
                            "factory_params_native_query_string_first {id}"
                        );
                        assert_eq!(
                            params.get("1"),
                            Some(&QueryValue::String("b".into())),
                            "factory_params_native_query_string_second {id}"
                        );
                    }
                    query_json_projection(execution.config.query_params.as_ref().unwrap()).unwrap()
                };
                let wire = &requests[0];
                assert_eq!(
                    wire.url,
                    format!("/private?{}", QUERIES[index]),
                    "factory_params_native_wire_query {carrier}/{id}"
                );
                assert!(
                    wire.params.is_none() && wire.query_params.is_none(),
                    "factory_params_native_wire_consumed {carrier}/{id}"
                );
                assert_eq!(
                    wire.query_profile,
                    Some(QueryProfile {
                        key_order: safe_cex_transport::query::QueryKeyOrder::Preserve,
                        arrays: QueryArrayFormat::Indexed,
                        encode: true
                    }),
                    "factory_params_native_default_profile {carrier}/{id}"
                );
                assert_eq!(
                    wire.method, "get",
                    "factory_params_native_method {carrier}/{id}"
                );
                assert!(
                    wire.body.is_none() && execution.config.body.is_none(),
                    "factory_params_native_body_absent {carrier}/{id}"
                );
                assert_eq!(
                    wire.headers.get("X-BAPI-SIGN").map(String::as_str),
                    signed.then_some(SIGNATURES[index]),
                    "factory_params_native_hmac {carrier}/{id}"
                );
                assert_eq!(
                    wire.headers.get("X-BAPI-API-KEY").map(String::as_str),
                    signed.then_some("key"),
                    "factory_params_native_key_header {carrier}/{id}"
                );
                assert_eq!(
                    wire.headers.get("X-BAPI-TIMESTAMP").map(String::as_str),
                    signed.then_some("1000"),
                    "factory_params_native_timestamp_header {carrier}/{id}"
                );
                assert_eq!(
                    f.owner.accesses.borrow().len(),
                    usize::from(signed),
                    "factory_params_native_auth_calls {carrier}/{id}"
                );
                assert_eq!(
                    f.owner.base.auth_failure_streak(),
                    0,
                    "factory_params_native_no_auth_failure {carrier}/{id}"
                );
                assert_eq!(
                    f.clock_fetches.load(Ordering::SeqCst),
                    0,
                    "factory_params_native_no_clock_fetch {carrier}/{id}"
                );
                assert_eq!(
                    f.host.failures.get(),
                    0,
                    "factory_params_native_no_failure_hook {carrier}/{id}"
                );
                assert_eq!(f.host.responses.borrow().len(), 1);
                assert_native_json(
                    &f.host.responses.borrow()[0],
                    &json!({"retCode":0,"retMsg":"OK"}),
                    &format!("factory_params_native_common_response {carrier}/{id}"),
                );
                assert_native_json(
                    &execution.response.body.clone().into_data(),
                    &json!({"retCode":0,"retMsg":"OK"}),
                    &format!("factory_params_native_returned_response {carrier}/{id}"),
                );
                assert_eq!(
                    f.emissions.get(),
                    0,
                    "factory_params_native_no_emit {carrier}/{id}"
                );
                assert!(
                    f.runtime.console.borrow().is_empty(),
                    "factory_params_native_no_console {carrier}/{id}"
                );
                let mut trace = vec!["start", "wire", "common-response"];
                if signed {
                    trace.push("owner-base");
                }
                assert_eq!(
                    f.trace
                        .borrow()
                        .iter()
                        .map(String::as_str)
                        .collect::<Vec<_>>(),
                    trace,
                    "factory_params_native_hook_order {carrier}/{id}"
                );
                cases.push(json!({"id":id,"carrier":carrier,"params":params,"wireQuery":wire.url.split('?').nth(1).unwrap(),
                    "signature":wire.headers.get("X-BAPI-SIGN"),"authSuccesses":f.owner.accesses.borrow().len()}));
            }
        }
    }
    assert_eq!(cases.len(), 24, "factory_params_native_executed_cases");
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["carrier"] == "legacy")
            .count(),
        12,
        "factory_params_native_legacy_floor"
    );
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["carrier"] == "native")
            .count(),
        12,
        "factory_params_native_query_floor"
    );
    cases
}

pub fn compare_source(native: &[Value]) {
    assert_eq!(native.len(), 24, "factory_params_native_before_source");
    let source = source("create-api-params-spread-boundaries.cjs", 12);
    for (index, actual) in native.iter().enumerate() {
        let expected = &source["cases"][index % 12];
        assert_eq!(
            actual["id"], expected["id"],
            "factory_params_source_id {index}"
        );
        for field in ["params", "wireQuery", "signature", "authSuccesses"] {
            assert_eq!(
                actual[field], expected["source"][field],
                "factory_params_source_value {index}/{field}"
            );
        }
    }
}
