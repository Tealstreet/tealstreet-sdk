#[path = "support/order_field_identity.rs"]
mod native_identity;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_venue_woo::orders::{map_algo_order, map_limit_order};
use serde_json::{Value, json};
fn store() -> DefaultStore {
    let store = DefaultStore::new();
    let market = serde_json::from_value(json!({"id":"BTC","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":100}}})).unwrap();
    Store::update(
        &store,
        StoreUpdate {
            markets: Some(vec![market]),
            ..Default::default()
        },
        false,
    )
    .expect("test market initialization must succeed");
    store
}
fn omission(v: &Value) -> bool {
    v.get("$type").and_then(Value::as_str) == Some("undefined")
}
fn project(value: &Value) -> Value {
    match value {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(_, v)| !omission(v))
                .map(|(k, v)| (k.clone(), project(v)))
                .collect(),
        ),
        Value::Array(a) => Value::Array(a.iter().map(project).collect()),
        _ => value.clone(),
    }
}
fn compare(actual: &Value, expected: &Value, label: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{label} numeric bits"
        ),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{label} keys: {actual} vs {expected}");
            for (k, v) in b {
                compare(&a[k], v, label)
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{label} count");
            for (a, b) in a.iter().zip(b) {
                compare(a, b, label)
            }
        }
        _ => assert_eq!(actual, expected, "{label}"),
    }
}
fn refusal(order: &Value) -> bool {
    if order.is_null() {
        return false;
    }
    ["type", "side"].iter().any(|k| !order[*k].is_string())
        || ["price", "amount", "filled"]
            .iter()
            .any(|k| !order[*k].is_number())
        || !order["reduceOnly"].is_boolean()
        || ["slTriggerType", "tpTriggerType"].iter().any(|k| {
            order
                .get(*k)
                .is_some_and(|v| !v.is_string() && !omission(v))
        })
}
#[test]
fn legacy_mappers_match_source_and_pin_whole_record_refusals() {
    let store = store();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/order-output.json")).unwrap();
    let mut matched = 0;
    let mut refused = 0;
    for case in &cases {
        let row = case["input"].as_object().unwrap();
        let label = format!("{} {} {}", case["method"], case["field"], case["key"]);
        let result = if case["method"] == "mapLimitOrder" {
            map_limit_order(row, &store, || 1234.0).map(|x| {
                if let Some(order) = &x {
                    native_identity::check(order, &case["result"]);
                }
                serde_json::to_value(x).unwrap()
            })
        } else {
            map_algo_order(row, &store, || 1234.0).map(|x| {
                let expected = case["result"].as_array().unwrap();
                assert_eq!(x.len(), expected.len());
                for (order, input) in x.iter().zip(expected) {
                    native_identity::check(order, input);
                }
                serde_json::to_value(x).unwrap()
            })
        };
        if case.get("error").is_some() {
            assert_eq!(
                result.unwrap_err(),
                case["error"].as_str().unwrap(),
                "{label}"
            );
            continue;
        }
        let expected = &case["result"];
        let cannot_carry = if let Some(rows) = expected.as_array() {
            rows.iter().any(refusal)
        } else {
            refusal(expected)
        };
        if cannot_carry {
            let error = result.unwrap_err();
            assert!(error.starts_with("typed Order."), "{label}: {error}");
            if [
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
            ]
            .contains(&case["key"].as_str().unwrap())
            {
                assert!(
                    error.contains("refuses inherited"),
                    "{label}: preserve inherited identity"
                );
            }
            refused += 1;
        } else {
            assert!(
                result.is_ok(),
                "{label}: source mapping succeeds: {result:?}"
            );
            compare(&result.unwrap(), &project(expected), &label);
            matched += 1;
        }
    }
    assert_eq!(cases.len(), 103);
    assert_eq!(
        (matched, refused),
        (25, 77),
        "projection coverage and refusal count"
    );
}
#[test]
fn mapper_info_snapshots_parent_and_clock_runs_per_emitted_order() {
    let store = store();
    let aliases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/order-alias.json")).unwrap();
    assert_eq!(aliases.len(), 2, "WOO-CARDINALITY alias cases");
    for case in aliases {
        let mut input = case["before"].as_object().unwrap().clone();
        let mut ticks = 0;
        let orders = if case["method"] == "mapLimitOrder" {
            vec![
                map_limit_order(&input, &store, || {
                    ticks += 1;
                    1234.0
                })
                .unwrap()
                .unwrap(),
            ]
        } else {
            map_algo_order(&input, &store, || {
                ticks += 1;
                1234.0
            })
            .unwrap()
        };
        input.insert("afterMapping".into(), json!("visible"));
        assert_eq!(orders.len(), 1, "WOO-CARDINALITY alias native orders");
        assert_eq!(ticks, 1, "WOO-CARDINALITY alias clock calls");
        assert_eq!(orders[0].info.as_ref().unwrap(), &case["before"]);
        assert_eq!(case["after"]["afterMapping"], "visible");
        assert!(
            orders[0]
                .info
                .as_ref()
                .unwrap()
                .get("afterMapping")
                .is_none()
        );
    }
}

#[test]
fn algo_clock_follows_emitted_children_and_parent_info() {
    let store = store();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/order-output.json")).unwrap();
    let case = cases
        .iter()
        .find(|c| c["key"] == "parent id and ordered children")
        .unwrap();
    let row = case["input"].as_object().unwrap();
    let mut ticks = 100.0;
    let orders = map_algo_order(row, &store, || {
        ticks += 1.0;
        ticks
    })
    .unwrap();
    assert_eq!(
        orders.iter().map(|o| o.id.as_str()).collect::<Vec<_>>(),
        ["first", "last"]
    );
    assert_eq!(
        orders
            .iter()
            .map(|o| o.last_update.to_bits())
            .collect::<Vec<_>>(),
        [101.0_f64.to_bits(), 102.0_f64.to_bits()]
    );
    for order in orders {
        assert_eq!(order.parent_id.as_deref(), Some("42"));
        assert_eq!(order.info.as_ref(), Some(&case["input"]));
    }
}

#[test]
fn mapper_numeric_bits_are_observed_before_wire_projection() {
    let store = store();
    let ordinary: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/order-output.json")).unwrap();
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/order-numbers.json")).unwrap();
    for case in &cases {
        let method = case["method"].as_str().unwrap();
        let mut row = ordinary
            .iter()
            .find(|c| c["method"] == method && c["key"] == "BUY" && c["field"] == "side")
            .unwrap()["input"]
            .as_object()
            .unwrap()
            .clone();
        row.insert("algoType".into(), json!("TRAILING_STOP"));
        for (key, value) in [
            ("order_quantity", 7),
            ("order_price", 12),
            ("extremePrice", 15),
            ("callbackValue", 2),
        ] {
            row.insert(key.into(), json!(value));
        }
        let field = case["field"].as_str().unwrap();
        let lexeme = case["lexeme"].as_str().unwrap();
        row.insert(field.into(), serde_json::from_str(lexeme).unwrap());
        let order = if method == "mapLimitOrder" {
            map_limit_order(&row, &store, || 1234.0).unwrap().unwrap()
        } else {
            map_algo_order(&row, &store, || 1234.0).unwrap().remove(0)
        };
        for (name, value) in [
            ("price", order.price.to_number()),
            ("amount", order.amount),
            ("filled", order.filled),
            ("remaining", order.remaining),
            ("lastUpdate", order.last_update),
        ] {
            let observed = if value.is_nan() {
                "NaN".to_owned()
            } else {
                format!("{:016x}", value.to_bits())
            };
            assert_eq!(
                observed,
                case["bits"][name].as_str().unwrap(),
                "{method}.{field} {lexeme} -> {name} pre-serialization bits"
            );
        }
    }
    assert_eq!(cases.len(), 36);
}

#[test]
fn algo_batch_filter_precedes_mapping_source_finding() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/algo-batch-priority.json")).unwrap();
    assert_eq!(cases.len(), 2);
    for (index, case) in cases.iter().enumerate() {
        let mut clocks = 0;
        let error = map_algo_order(case["input"].as_object().unwrap(), &store(), || {
            clocks += 1;
            1234.0
        })
        .unwrap_err();
        if index == 0 {
            assert_eq!(
                error,
                "typed Order.type refuses inherited function Object() { [native code] }"
            );
            assert_eq!(clocks, 1, "closed output refusal follows source clock");
            assert_eq!(case["result"][0]["type"]["$type"], "function");
            assert_eq!(case["clockCalls"], 1);
            assert!(case.get("error").is_none());
        } else {
            assert_eq!(
                error, case["error"]["message"],
                "ALGO-FILTER primitive RHS exact source message"
            );
            assert_eq!(clocks, 0, "full filter failure precedes mapping");
            assert_eq!(case["error"]["name"], "TypeError");
            assert_eq!(
                case["error"]["message"],
                "Cannot use 'in' operator to search for 'triggerPrice' in 1"
            );
            assert_eq!(case["clockCalls"], 0);
        }
    }
}

#[test]
fn algo_refusal_preserves_source_error_priority_and_clock_counts() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/algo-timing.json")).unwrap();
    assert_eq!(cases.len(), 12);
    for case in cases {
        let mut clocks = 0;
        let error = map_algo_order(case["input"].as_object().unwrap(), &store(), || {
            clocks += 1;
            1234.0
        })
        .unwrap_err();
        assert_eq!(
            json!(clocks),
            case["clockCalls"],
            "algo source clock count {}",
            case["label"]
        );
        if let Some(source_error) = case.get("error") {
            if source_error["message"]
                .as_str()
                .unwrap()
                .starts_with("Cannot use 'in' operator")
            {
                assert_eq!(
                    error, source_error["message"],
                    "ALGO-FILTER primitive RHS exact source message"
                );
            } else {
                assert_eq!(source_error["name"], "TypeError");
                assert_eq!(
                    error, source_error["message"],
                    "algo later source error wins {}",
                    case["label"]
                );
            }
        } else {
            assert_eq!(
                error, "typed Order.type refuses inherited function Object() { [native code] }",
                "algo whole batch refusal {}",
                case["label"]
            );
        }
    }
}
