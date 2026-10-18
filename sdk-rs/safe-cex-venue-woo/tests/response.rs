use safe_cex_num::JsCoercionError;
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_venue_woo::response::*;
use serde_json::{Value, json};
fn input(v: &Value) -> Option<Value> {
    if v.get("$undefined") == Some(&json!(true)) {
        return None;
    }
    match v.get("$number").and_then(Value::as_str) {
        Some("Infinity") => Some(serde_json::from_str("1e400").unwrap()),
        Some("-Infinity") => Some(serde_json::from_str("-1e400").unwrap()),
        Some("-0") => Some(json!(-0.0)),
        _ => Some(v.clone()),
    }
}
fn number(v: Option<f64>) -> Value {
    v.map_or(json!({"$undefined":true}), |v| {
        if v == 0.0 && v.is_sign_negative() {
            json!({"$number":"-0"})
        } else {
            json!(v)
        }
    })
}
fn text(v: Option<String>) -> Value {
    v.map_or(json!({"$undefined":true}), Value::String)
}
fn same(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        _ => assert_eq!(a, b),
    }
}
#[test]
fn wire_response_helpers_match_live_source() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/response.json")).unwrap();
    let mut outside_json = 0;
    assert_eq!(
        cases.as_array().unwrap().len(),
        487,
        "WOO-CARDINALITY response source total"
    );
    for case in cases.as_array().unwrap() {
        let a = case["args"].as_array().unwrap();
        // CHOSEN JSON boundary cannot carry native NaN. Keep source witnesses visible.
        if a[0].get("$number") == Some(&json!("NaN")) {
            outside_json += 1;
            continue;
        }
        let raw = input(&a[0]);
        let result: Result<Value, JsCoercionError> = match case["name"].as_str().unwrap() {
            "parseTransactionRowsFromResponse" => Ok(json!(parse_transaction_rows_from_response(
                &Input::from(raw.clone().unwrap_or(Value::Null))
            ))),
            "parseFiniteFundingNumber" => parse_finite_funding_number(raw.as_ref()).map(number),
            "normalizeTimestampSeconds" => normalize_timestamp_seconds(raw.as_ref()).map(number),
            "normalizeTimestampMilliseconds" => {
                normalize_timestamp_milliseconds(raw.as_ref()).map(number)
            }
            "getWooResponseMessage" => Ok(json!(get_woo_response_message(
                a[0].as_object().unwrap(),
                a[1].as_str().unwrap()
            ))),
            "normalizeTransactionSymbol" => Ok(text(normalize_transaction_symbol(raw.as_ref()))),
            "normalizeRequestedSymbol" => Ok(text(normalize_requested_symbol(a[0].as_str()))),
            "getTransactionHistorySymbolCandidates" => Ok(json!(
                get_transaction_history_symbol_candidates(a[0].as_str().unwrap())
            )),
            name => panic!("unclaimed source {name}"),
        };
        if let Some(error) = case.get("error") {
            let actual = result.unwrap_err();
            assert_eq!(actual.message, error.as_str().unwrap(), "{case}");
            assert_eq!(actual.name, case["errorName"].as_str().unwrap());
        } else {
            same(
                &result.unwrap_or_else(|e| panic!("{case}: {e:?}")),
                &case["ok"],
            );
        }
    }
    assert_eq!(outside_json, 4, "native-NaN omissions must remain explicit");
}

#[test]
fn selected_transaction_array_is_borrowed_verbatim() {
    let value = Input::from(json!({"data":{"rows":[null,3,"fill",{"id":1}]},"rows":["root"]}));
    let actual = parse_transaction_rows_from_response(&value);
    let source = value
        .get("data")
        .unwrap()
        .get("rows")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(actual.as_ptr(), source.as_ptr());
    assert_eq!(actual.len(), 4);
}

#[test]
fn divergent_response_boundaries_are_observed_before_source_projection() {
    use safe_cex_venue_woo::utils::{normalize_symbol, reverse_symbol};
    assert_eq!(
        normalize_symbol("PERP_BTC_USDT"),
        "BTCUSDT",
        "WOO-RESPONSE-AUDIT native normalize"
    );
    assert_eq!(
        reverse_symbol("BTCUSDT"),
        "PERP_BTC_USDT",
        "WOO-RESPONSE-AUDIT native reverse"
    );
    assert_eq!(
        normalize_transaction_symbol(Some(&json!("PERP/BTC/USDT"))).as_deref(),
        Some("BTCUSDT"),
        "WOO-RESPONSE-AUDIT native transaction"
    );
    assert_eq!(
        normalize_requested_symbol(Some("PERP_BTC_USDT.P")).as_deref(),
        Some("BTCUSDT"),
        "WOO-RESPONSE-AUDIT native requested"
    );
    assert_eq!(
        get_transaction_history_symbol_candidates("BTCUSDT"),
        ["PERP_BTC_USDT", "BTC_USDT_PERP", "BTC-USDT-PERP", "BTCUSDT"],
        "WOO-RESPONSE-AUDIT native candidate order"
    );
    let record = json!({"valueOf": 1234.75});
    assert_eq!(
        parse_finite_funding_number(Some(&record)).unwrap(),
        None,
        "WOO-RESPONSE-AUDIT native funding rejects object without coercion"
    );
    assert!(
        normalize_timestamp_seconds(Some(&record))
            .unwrap()
            .is_none(),
        "WOO-RESPONSE-AUDIT native JSON object has no executable hook"
    );
    assert!(
        normalize_timestamp_milliseconds(Some(&record))
            .unwrap()
            .is_none(),
        "WOO-RESPONSE-AUDIT native JSON milliseconds has no executable hook"
    );
    assert_eq!(
        parse_finite_funding_number(Some(&json!(" "))).unwrap(),
        None,
        "WOO-RESPONSE-AUDIT native trim stays deterministic"
    );
    let negative_zero = json!(-0.0);
    for actual in [
        parse_finite_funding_number(Some(&negative_zero)),
        normalize_timestamp_seconds(Some(&negative_zero)),
        normalize_timestamp_milliseconds(Some(&negative_zero)),
    ] {
        assert_eq!(
            actual.unwrap().unwrap().to_bits(),
            (-0.0_f64).to_bits(),
            "WOO-RESPONSE-AUDIT native signed-zero bits"
        );
    }
    let sentinel = std::rc::Rc::new("original method failure");
    let mut arguments = Vec::new();
    let value = std::rc::Rc::new(json!({"arbitrary": 97}));
    let actual = normalize_requested_symbol_with(Some("x:btcusdt.P"), |raw| {
        arguments.push(raw.to_owned());
        Ok::<_, std::rc::Rc<&str>>(value.clone())
    })
    .unwrap()
    .unwrap();
    assert!(
        std::rc::Rc::ptr_eq(&actual, &value),
        "WOO-RESPONSE-AUDIT native overridden return identity"
    );
    assert_eq!(
        arguments,
        ["BTCUSDT"],
        "WOO-RESPONSE-AUDIT native overridden argument"
    );
    let failed = normalize_requested_symbol_with(Some("btc.p"), |_| Err::<(), _>(sentinel.clone()));
    assert!(
        std::rc::Rc::ptr_eq(&failed.unwrap_err(), &sentinel),
        "WOO-RESPONSE-AUDIT native overridden error identity"
    );
    let mut calls = Vec::new();
    for raw in [None, Some(""), Some("PERP")] {
        normalize_requested_symbol_with(raw, |text| {
            calls.push(text.to_owned());
            Ok::<_, ()>(())
        })
        .unwrap();
    }
    assert_eq!(
        calls,
        [""],
        "WOO-RESPONSE-AUDIT native empty suffix still calls override"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/response-oracle.cjs"
        ))
        .arg("--audit")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        observed.len(),
        24,
        "WOO-RESPONSE-AUDIT source observation floor stops at requested-empty-suffix"
    );
    assert_eq!(observed[0]["value"], "injected-normalize");
    assert_eq!(observed[1]["value"], "injected-reverse");
    assert_eq!(observed[17]["value"], "object");
}
