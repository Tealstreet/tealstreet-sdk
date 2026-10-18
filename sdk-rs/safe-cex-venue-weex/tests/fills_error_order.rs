use safe_cex_num::numeric_ingress::{js_from_str, js_from_value};
use safe_cex_venue_weex::{
    fills::{normalize_execution, normalize_historical_trade},
    utils::{WeexUtilityError, WeexUtilityInput},
};
use serde_json::{Value, json};
use std::{path::Path, process::Command};

fn source_profile(script: &str, profile: &str, ids: &[&str], assertion: &str) -> Vec<Value> {
    let run = |control| {
        let mut command = Command::new("node");
        command
            .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join(script))
            .arg(profile);
        if control {
            command.arg("--control");
        }
        command
            .output()
            .expect("Node must execute maintained TypeScript")
    };
    let baseline = run(false);
    assert!(
        baseline.status.success(),
        "WEEX source baseline: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );
    let mutant = run(true);
    assert_eq!(mutant.status.code(), Some(1), "WEEX source control exit");
    assert!(
        String::from_utf8_lossy(&mutant.stderr).contains(assertion),
        "WEEX source named control: {}",
        String::from_utf8_lossy(&mutant.stderr)
    );
    let restored = run(false);
    assert!(
        restored.status.success(),
        "WEEX source restored: {}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let mut results = Vec::new();
    for output in [baseline, restored] {
        let envelope: Value = serde_json::from_slice(&output.stdout).unwrap();
        let observations = envelope["observations"].as_array().unwrap();
        assert_eq!(
            observations.len(),
            ids.len(),
            "WEEX independent source profile floor"
        );
        for (observation, id) in observations.iter().zip(ids) {
            assert_eq!(observation["id"], *id, "WEEX source observation identity");
        }
        results.extend(observations.iter().cloned());
    }
    results
}

fn record() -> Value {
    json!({"symbol":"B/Q","side":"BUY","price":3,"qty":5,
        "time":1700000000123_u64,"tradeId":"fill-a","orderId":"order-b"})
}

fn retained(value: Value) -> WeexUtilityInput {
    js_from_value(value).unwrap()
}

fn conversion_error(error: WeexUtilityError, label: &str) {
    assert_eq!(error.name, "TypeError", "{label} class");
    assert_eq!(
        error.message, "Cannot convert object to primitive value",
        "{label} cause"
    );
}

#[test]
fn commission_conversion_precedes_model_projection() {
    let mut raw = record();
    raw["maker"] = Value::Null;
    raw["commissionAsset"] = json!(7);
    raw["commission"] = json!({"toString":null});
    let raw = retained(raw);
    conversion_error(
        normalize_execution(&raw, None, |v| v).unwrap_err(),
        "WEEX execution commission before maker admission",
    );
    conversion_error(
        normalize_historical_trade(&raw, None, |v| v).unwrap_err(),
        "WEEX historical commission before maker admission",
    );
    source_profile(
        "tests/fills-order-oracle.cjs",
        "commission",
        &["normalizeExecution", "normalizeHistoricalTrade"],
        "WEEX SOURCE commission before maker",
    );
}

#[test]
fn historical_order_guard_precedes_model_projection() {
    let mut guarded = 0;
    for order_id in [None, Some(json!(0)), Some(json!([]))] {
        let mut raw = record();
        raw.as_object_mut().unwrap().remove("orderId");
        if let Some(order_id) = order_id {
            raw["orderId"] = order_id;
        }
        raw["maker"] = Value::Null;
        raw["commissionAsset"] = json!(7);
        raw["buyer"] = Value::Null;
        raw["quoteQty"] = json!({"toString":null});
        assert!(
            matches!(
                normalize_historical_trade(&retained(raw.clone()), None, |v| v),
                Ok(None)
            ),
            "WEEX historical order guard before model admission"
        );
        raw["commission"] = json!({"toString":null});
        conversion_error(
            normalize_historical_trade(&retained(raw), None, |v| v).unwrap_err(),
            "WEEX execution conversion before historical order guard",
        );
        guarded += 1;
    }
    assert_eq!(guarded, 3, "WEEX historical order guard floor");
    let mut raw = record();
    raw["maker"] = Value::Null;
    raw["quoteQty"] = json!({"toString":null});
    conversion_error(
        normalize_historical_trade(&retained(raw), None, |v| v).unwrap_err(),
        "WEEX historical quote conversion before maker admission",
    );
    source_profile(
        "tests/fills-order-oracle.cjs",
        "historical",
        &[
            "missing",
            "missing-commission",
            "zero",
            "zero-commission",
            "array",
            "array-commission",
            "quote",
        ],
        "WEEX SOURCE historical order guard",
    );
}

#[test]
fn historical_timestamp_precedes_model_projection() {
    let mut compared = 0;
    for (field, invalid, message) in [
        (
            "buyer",
            Value::Null,
            "HistoricalTrade.buyer requires a boolean",
        ),
        ("maker", Value::Null, "Execution.maker requires a boolean"),
        (
            "commissionAsset",
            json!(7),
            "Execution.commissionAsset requires a string",
        ),
    ] {
        let mut raw = record();
        raw["time"] = json!("2024-01-02T03:04:05");
        raw[field] = invalid;
        let mut calls = Vec::new();
        let result = normalize_historical_trade(&retained(raw), None, |v| {
            calls.push(v);
            v
        });
        assert_eq!(
            calls,
            [1704164645000.0, 1704164645000.0],
            "WEEX historical second timestamp before {field} admission"
        );
        let error = result.unwrap_err();
        assert_eq!(error.name, "TypeError");
        assert_eq!(error.message, message);
        compared += 1;
    }
    assert_eq!(compared, 3, "WEEX typed projection timestamp floor");
    source_profile(
        "tests/fills-order-oracle.cjs",
        "clock",
        &[
            "buyer",
            "maker",
            "commissionAsset",
            "distinct",
            "zero",
            "nan",
        ],
        "WEEX SOURCE historical second timestamp",
    );
}

#[test]
fn scalar_work_precedes_the_cold_guard() {
    let mut raw = record();
    raw["symbol"] = json!("");
    raw["time"] = json!("2024-01-02T03:04:05");
    raw["maker"] = Value::Null;
    raw["commission"] = json!({"toString":null});
    let mut calls = Vec::new();
    assert!(
        normalize_execution(&retained(raw.clone()), None, |v| {
            calls.push(v);
            v
        })
        .unwrap()
        .is_none()
    );
    assert_eq!(
        calls,
        [1704164645000.0],
        "WEEX execution timestamp before cold guard"
    );
    calls.clear();
    assert!(
        normalize_historical_trade(&retained(raw.clone()), None, |v| {
            calls.push(v);
            v
        })
        .unwrap()
        .is_none()
    );
    assert_eq!(
        calls,
        [1704164645000.0],
        "WEEX historical timestamp before cold guard"
    );
    raw["price"] = json!({"toString":null});
    conversion_error(
        normalize_execution(&retained(raw.clone()), None, |_| {
            panic!("timestamp follows throwing price")
        })
        .unwrap_err(),
        "WEEX execution eager scalar conversion",
    );
    conversion_error(
        normalize_historical_trade(&retained(raw), None, |_| {
            panic!("timestamp follows throwing price")
        })
        .unwrap_err(),
        "WEEX historical eager scalar conversion",
    );
    source_profile(
        "tests/fills-order-oracle.cjs",
        "guards",
        &[
            "normalizeExecution-cold",
            "normalizeExecution-eager",
            "normalizeHistoricalTrade-cold",
            "normalizeHistoricalTrade-eager",
        ],
        "WEEX SOURCE cold guard scalar work",
    );
}

#[test]
fn historical_recomputes_time_without_an_extra_validity_guard() {
    let mut compared = 0;
    for second_time in [1704164645987.0, 0.0, f64::NAN] {
        let mut raw = record();
        raw["time"] = json!("2024-01-02T03:04:05");
        let mut calls = Vec::new();
        let trade = normalize_historical_trade(&retained(raw), None, |v| {
            calls.push(v);
            if calls.len() == 1 {
                v + 123.0
            } else {
                second_time
            }
        })
        .unwrap()
        .expect("WEEX second timestamp adds no invalid-record guard");
        assert_eq!(trade.symbol, "B/Q");
        assert_eq!(trade.id.as_deref(), Some("fill-a"));
        assert_eq!(trade.order_id, "order-b");
        assert_eq!(trade.price, 3.0);
        assert_eq!(trade.qty, 5.0);
        let expected_time = if second_time.is_nan() {
            0.0
        } else {
            second_time
        };
        assert_eq!(
            trade.time.to_bits(),
            expected_time.to_bits(),
            "WEEX historical independently recomputed milliseconds"
        );
        assert_eq!(calls, [1704164645000.0, 1704164645000.0]);
        compared += 1;
    }
    assert_eq!(compared, 3, "WEEX independent second timestamp floor");
}

#[test]
fn fills_preserve_native_strings_and_refuse_whole_utf16_records() {
    let mut compared = 0;
    for key in ["A", "😀", "\u{feff}", "�", "\\ud800"] {
        let symbol = format!("{key}/Q");
        let order_id = format!("order:{key}");
        let mut raw = record();
        raw["symbol"] = json!(symbol);
        raw["orderId"] = json!(order_id);
        let text = raw.to_string();
        for raw in [
            serde_json::from_str::<WeexUtilityInput>(&text).unwrap(),
            js_from_str(&text).unwrap(),
            retained(raw),
        ] {
            let execution = normalize_execution(&raw, None, |v| v).unwrap().unwrap();
            assert_eq!(execution.symbol, symbol, "WEEX native execution symbol");
            assert_eq!(execution.order_id.as_deref(), Some(order_id.as_str()));
            let trade = normalize_historical_trade(&raw, None, |v| v)
                .unwrap()
                .unwrap();
            assert_eq!(trade.symbol, symbol);
            assert_eq!(trade.order_id, order_id, "WEEX native historical order ID");
            compared += 1;
        }
    }
    assert_eq!(compared, 15, "WEEX independent native Unicode route floor");
    for raw in [
        r#"{"symbol":"\ud800","side":"BUY","price":3,"qty":5,"time":1700000000123,"orderId":"ordinary"}"#,
        r#"{"symbol":"B/Q","side":"BUY","price":3,"qty":5,"time":1700000000123,"orderId":"\ud800"}"#,
    ] {
        for input in [
            serde_json::from_str::<WeexUtilityInput>(raw).unwrap(),
            js_from_str::<WeexUtilityInput>(raw).unwrap(),
        ] {
            let field = if raw.contains("ordinary") {
                "symbol"
            } else {
                "orderId"
            };
            assert_eq!(
                input.as_value().get(field).unwrap().as_text(),
                Some([0xd800].as_slice())
            );
            assert!(
                normalize_execution(&input, None, |v| v).is_err(),
                "closed execution String field {field}"
            );
            assert!(
                normalize_historical_trade(&input, None, |v| v).is_err(),
                "closed historical String field {field}"
            );
        }
    }
    for (profile, field) in [
        ("weex-execution-unit", "symbol"),
        ("weex-historical-unit", "orderId"),
    ] {
        for observation in source_profile(
            "../parity/evidence/okx-kraken-weex-audit/full-source-observations.mjs",
            profile,
            &[profile],
            &format!("FULL-{} returned field", profile.to_uppercase()),
        ) {
            assert_eq!(observation["field"], field);
            assert_eq!(
                observation["units"],
                json!([0xd800]),
                "WEEX source materialized returned unit"
            );
        }
    }
}

#[test]
fn retained_whole_record_costs_are_explicit() {
    let mut admitted = 0;
    let mut shallow = record();
    shallow["ignored"] = json!([[null]]);
    let text = shallow.to_string();
    for raw in [
        serde_json::from_str::<WeexUtilityInput>(&text).unwrap(),
        js_from_str(&text).unwrap(),
    ] {
        let execution = normalize_execution(&raw, None, |v| v).unwrap().unwrap();
        assert_eq!(execution.symbol, "B/Q");
        assert_eq!(execution.order_id.as_deref(), Some("order-b"));
        let trade = normalize_historical_trade(&raw, None, |v| v)
            .unwrap()
            .unwrap();
        assert_eq!(trade.symbol, "B/Q");
        assert_eq!(trade.order_id, "order-b");
        admitted += 1;
    }
    assert_eq!(admitted, 2, "WEEX shallow ignored-field route floor");
    let mut deep = record();
    deep["ignored"] = (0..150).fold(Value::Null, |value, _| json!([value]));
    let mut refused = 0;
    let mut retained_routes = 0;
    for (id, text) in [
        ("depth", deep.to_string()),
        ("unit", r#"{"symbol":"B/Q","side":"BUY","price":3,"qty":5,"time":1700000000123,"orderId":"order-b","ignored":"\ud800"}"#.to_owned()),
        ("cold-unit", r#"{"symbol":"","side":"BUY","price":3,"qty":5,"time":1700000000123,"orderId":"order-b","ignored":"\ud800"}"#.to_owned()),
    ] {
        let routes = [serde_json::from_str::<WeexUtilityInput>(&text), js_from_str::<WeexUtilityInput>(&text)];
        for route in routes {
            if id == "depth" {
                assert!(route.is_err(), "WEEX retained depth refusal");
                refused += 1;
                continue;
            }
            let input = route.unwrap();
            assert_eq!(input.as_value().get("ignored").unwrap().as_text(), Some([0xd800].as_slice()));
            let execution = normalize_execution(&input, None, |v| v).unwrap();
            let trade = normalize_historical_trade(&input, None, |v| v).unwrap();
            if id == "cold-unit" {
                assert!(execution.is_none() && trade.is_none(), "cold guard precedes ignored field");
            } else {
                let execution = execution.unwrap();
                let trade = trade.unwrap();
                assert_eq!(execution.symbol, "B/Q");
                assert_eq!(execution.order_id.as_deref(), Some("order-b"));
                assert_eq!(execution.price.to_bits(), 3.0_f64.to_bits());
                assert_eq!(trade.symbol, "B/Q");
                assert_eq!(trade.order_id, "order-b");
                assert_eq!(trade.price.to_bits(), 3.0_f64.to_bits());
            }
            retained_routes += 1;
        }
    }
    assert_eq!(refused, 2, "WEEX depth refusal route floor");
    assert_eq!(
        retained_routes, 4,
        "WEEX ignored UTF16 admission route floor"
    );
    source_profile(
        "tests/fills-order-oracle.cjs",
        "bounds",
        &[
            "normalizeExecution-depth",
            "normalizeExecution-unit",
            "normalizeExecution-cold-unit",
            "normalizeHistoricalTrade-depth",
            "normalizeHistoricalTrade-unit",
            "normalizeHistoricalTrade-cold-unit",
        ],
        "WEEX SOURCE ignored input admitted",
    );
}
