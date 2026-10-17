//! Real production class members, with native values observed before projection.
use crate::binding_test_harness as binding_harness;
use binding_harness::{Harness, harness, options};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, from_json},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};

fn setup(script: impl FnOnce(&binding_harness::Http)) -> Harness {
    let h = harness(ExchangeName::Woo, options(), script);
    h.runtime.now.set(1234.0);
    Store::update(h.store.as_ref(), StoreUpdate {
        markets: Some(vec![serde_json::from_value(json!({"id":"BTC","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":100}}})).unwrap()]),
        ..Default::default()
    }, false).unwrap();
    h
}
fn partial() -> Value {
    json!({"algoOrderId":7,"rootAlgoOrderId":6,"symbol":"PERP_BTC_USDT","algoType":"STOP","side":"SELL","triggerPrice":90,"triggerPriceType":"MARK_PRICE","quantity":"1.25","totalExecutedQuantity":0.25})
}
fn map(h: &Harness, member: &str, row: Js) -> Result<Js, super::Thrown> {
    h.exchange.rt.this.call_method(member, vec![row])
}

#[test]
fn live_algo_mapper_preserves_partial_stop_and_copied_fields() {
    let h = setup(|_| {});
    let input = from_json(&partial());
    let result =
        map(&h, "mapAlgoOrder", input.clone()).expect("real algo mapper must retain partial STOP");
    let rows = result.array_values().unwrap();
    assert_eq!(rows.len(), 1, "partial STOP remains an order");
    let row = &rows[0];
    assert!(row.has_own("type") && row.get("type").unwrap().is_undefined());
    assert!(row.has_own("reduceOnly") && row.get("reduceOnly").unwrap().is_undefined());
    assert_eq!(
        row.get("amount")
            .unwrap()
            .as_str()
            .map(|s| String::from_utf16_lossy(s)),
        Some("1.25".into())
    );
    assert_eq!(
        row.get("remaining").unwrap().as_f64().unwrap().to_bits(),
        1.0_f64.to_bits()
    );
    assert!(row.get("info").unwrap().strict_equals(&input));
    input.set("afterMapping", Js::Number(17.0)).unwrap();
    assert_eq!(
        row.get("info")
            .unwrap()
            .get("afterMapping")
            .unwrap()
            .as_f64(),
        Some(17.0)
    );
}

#[test]
fn live_algo_fetch_keeps_partial_stop_and_both_bracket_children() {
    let h = setup(|http| {
        http.route("GET", "/v3/algo/orders", 200.0, json!({"success":true,"data":{"rows":[partial(),{"symbol":"PERP_BTC_USDT","rootAlgoOrderId":10,"childOrders":[
            {"algoOrderId":11,"algoType":"TAKE_PROFIT","side":"SELL","triggerPrice":110,"triggerPriceType":"MARK_PRICE","quantity":2,"totalExecutedQuantity":0,"reduceOnly":true},
            {"algoOrderId":12,"algoType":"STOP_LOSS","side":"SELL","triggerPrice":90,"triggerPriceType":"MARK_PRICE","quantity":2,"totalExecutedQuantity":0,"reduceOnly":true}]}],"meta":{"total":2,"records_per_page":100}}}));
    });
    let value = h
        .exchange
        .rt
        .this
        .call_method("fetchAlgoOrders", vec![])
        .unwrap();
    let result = h
        .drive(Box::pin(async move {
            await_js(value).await.map_err(|e| e.to_exchange_error())
        }))
        .unwrap();
    let rows = result.array_values().unwrap();
    assert_eq!(
        rows.len(),
        3,
        "one partial STOP must not discard the complete algo-order set"
    );
    assert_eq!(
        rows.iter()
            .map(|row| row.get("id").unwrap().to_display())
            .collect::<Vec<_>>(),
        ["7", "11", "12"]
    );
    assert!(rows[0].get("type").unwrap().is_undefined());
    assert_eq!(rows[0].get("amount").unwrap().to_display(), "1.25");
    assert!(h.runtime.reports.borrow().is_empty());
    assert_eq!(h.http.calls("/v3/algo/orders").len(), 1);
}

fn observe(actual: &Js, expected: &Value, label: &str) {
    match expected.get("$type").and_then(Value::as_str) {
        Some("undefined") => {
            assert!(actual.is_undefined(), "{label}: own undefined value");
            return;
        }
        Some("function") => {
            assert!(
                actual.is_callable(),
                "{label}: original callable table member"
            );
            assert_eq!(
                actual.get("name").unwrap().to_display(),
                expected["name"].as_str().unwrap(),
                "{label}: native function name"
            );
            return;
        }
        Some("prototype-object") => {
            assert!(
                actual.is_object() && !actual.is_callable(),
                "{label}: native prototype object"
            );
            return;
        }
        _ => {}
    }
    match expected {
        Value::Null => assert!(matches!(actual, Js::Null), "{label}: null"),
        Value::Bool(value) => assert!(matches!(actual, Js::Bool(v) if v == value), "{label}: bool"),
        Value::Number(value) => assert_eq!(
            actual.as_f64().expect(label).to_bits(),
            value.as_f64().unwrap().to_bits(),
            "{label}: native bits"
        ),
        Value::String(value) => assert_eq!(
            actual.as_str().expect(label).as_ref(),
            &value.encode_utf16().collect::<Vec<_>>(),
            "{label}: native UTF16"
        ),
        Value::Array(values) => {
            let rows = actual.array_values().expect(label);
            assert_eq!(rows.len(), values.len(), "{label}: count");
            for (index, (row, value)) in rows.iter().zip(values).enumerate() {
                observe(row, value, &format!("{label}[{index}]"));
            }
        }
        Value::Object(fields) => {
            assert_eq!(
                actual.own_keys().len(),
                fields.len(),
                "{label}: native own-key count"
            );
            for (name, value) in fields {
                assert!(actual.has_own(name), "{label}.{name}: own presence");
                observe(
                    &actual.get(name).unwrap(),
                    value,
                    &format!("{label}.{name}"),
                );
            }
        }
    }
}

#[test]
fn live_mappers_execute_all_103_source_records_without_typed_refusal_exemptions() {
    let h = setup(|_| {});
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/order-output.json")).unwrap();
    assert_eq!(cases.len(), 103);
    for case in cases {
        let label = format!("{} {} {}", case["method"], case["field"], case["key"]);
        let input = from_json(&case["input"]);
        let result = map(&h, case["method"].as_str().unwrap(), input.clone());
        if let Some(error) = case.get("error") {
            let actual = result.unwrap_err();
            let message = error.as_str().unwrap();
            assert_eq!(
                actual.value.get("name").unwrap().to_display(),
                "TypeError",
                "{label}: native error name"
            );
            assert_eq!(
                actual.message(),
                message.strip_prefix("TypeError: ").unwrap_or(message),
                "{label}: native error message"
            );
            continue;
        }
        let result = result
            .unwrap_or_else(|error| panic!("{label}: actual mapper refused {}", error.message()));
        observe(&result, &case["result"], &label);
        for row in result.array_values().unwrap_or_else(|| {
            if matches!(result, Js::Null) {
                vec![]
            } else {
                vec![result]
            }
        }) {
            assert!(
                row.get("info").unwrap().strict_equals(&input),
                "{label}: original parent identity"
            );
        }
    }
}

#[test]
fn live_mapper_numeric_fields_keep_all_36_source_bit_witnesses() {
    let h = setup(|_| {});
    let ordinary: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/order-output.json")).unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/order-numbers.json")).unwrap();
    assert_eq!(cases.len(), 36);
    for case in cases {
        let method = case["method"].as_str().unwrap();
        let mut row = ordinary
            .iter()
            .find(|c| c["method"] == method && c["key"] == "BUY" && c["field"] == "side")
            .unwrap()["input"]
            .clone();
        row["algoType"] = json!("TRAILING_STOP");
        for (key, value) in [
            ("order_quantity", 7),
            ("order_price", 12),
            ("extremePrice", 15),
            ("callbackValue", 2),
        ] {
            row[key] = json!(value);
        }
        row[case["field"].as_str().unwrap()] =
            serde_json::from_str(case["lexeme"].as_str().unwrap()).unwrap();
        let result = map(&h, method, from_json(&row)).unwrap();
        let order = if method == "mapAlgoOrder" {
            result.array_values().unwrap().remove(0)
        } else {
            result
        };
        for name in ["price", "amount", "filled", "remaining", "lastUpdate"] {
            let value = order.get(name).unwrap().as_f64().unwrap();
            let bits = if value.is_nan() {
                "NaN".to_owned()
            } else {
                format!("{:016x}", value.to_bits())
            };
            assert_eq!(
                bits,
                case["bits"][name].as_str().unwrap(),
                "{method} {} {} native {name}",
                case["field"],
                case["lexeme"]
            );
        }
    }
}

#[test]
fn live_algo_filter_error_and_clock_order_match_all_12_source_cases() {
    let h = setup(|_| {});
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("../fixtures/algo-timing.json")).unwrap();
    assert_eq!(cases.len(), 12);
    for case in cases {
        h.runtime.clock_reads.set(0);
        let result = map(&h, "mapAlgoOrder", from_json(&case["input"]));
        assert_eq!(
            h.runtime.clock_reads.get(),
            case["clockCalls"].as_u64().unwrap() as usize,
            "{}: actual native clock calls",
            case["label"]
        );
        if let Some(error) = case.get("error") {
            let actual = result.unwrap_err();
            assert_eq!(
                actual.message(),
                error["message"].as_str().unwrap(),
                "{}: source error priority",
                case["label"]
            );
        } else {
            observe(
                &result.unwrap(),
                &case["result"],
                case["label"].as_str().unwrap(),
            );
        }
    }
}

#[test]
fn real_client_id_rule_matches_source_safe_integer_limits() {
    let h = setup(|_| {});
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/client-id-rule-source.cjs"
        ))
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source["rows"].as_array().unwrap().len(), 26);
    let rule = h.exchange.rt.this.get("clientOrderIdRule").unwrap();
    assert_eq!(
        rule.get("label").unwrap().to_display(),
        source["label"].as_str().unwrap()
    );
    assert_eq!(
        rule.get("requirement").unwrap().to_display(),
        source["requirement"].as_str().unwrap()
    );
    for row in source["rows"].as_array().unwrap() {
        let actual = rule
            .call_method("isValid", vec![Js::str(row["input"].as_str().unwrap())])
            .expect(
                "real constructed WOO client-ID rule must call the shared safe-integer builtin",
            );
        assert!(
            actual.strict_equals(&Js::Bool(row["valid"].as_bool().unwrap())),
            "source client-ID rule {row}"
        );
    }
    h.exchange.dispose().unwrap();
}
