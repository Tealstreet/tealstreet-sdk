use super::*;
fn encode(value: f64) -> Value {
    if value.is_nan() {
        "NaN".into()
    } else if value == f64::INFINITY {
        "Infinity".into()
    } else if value == f64::NEG_INFINITY {
        "-Infinity".into()
    } else if value == 0. && value.is_sign_negative() {
        "-0".into()
    } else {
        json!(value)
    }
}
fn book(value: &OrderBook) -> Value {
    let levels = |rows: &[OrderBookOrders]| {
        rows.iter().map(|r|json!({"price":encode(r.price),"amount":encode(r.amount),"total":encode(r.total)})).collect::<Vec<_>>()
    };
    json!({"bids":levels(&value.bids),"asks":levels(&value.asks)})
}
fn normalize(value: &Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.iter().map(normalize).collect()),
        Value::Object(m) => {
            Value::Object(m.iter().map(|(k, v)| (k.clone(), normalize(v))).collect())
        }
        _ => value.clone(),
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn book_state_matches_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/book-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    let source_total: usize = cases
        .iter()
        .map(|case| case["outcomes"].as_array().unwrap().len())
        .sum();
    assert_eq!(source_total, 1755, "BOOK-CARDINALITY source total");
    let mut native_total = 0;
    let mut comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let mut state = BookState::new();
        let inputs = case["frames"].as_array().unwrap();
        let expected = case["outcomes"].as_array().unwrap();
        assert_eq!(
            inputs.len(),
            expected.len(),
            "BOOK-CARDINALITY source case {index}"
        );
        let mut native = Vec::new();
        for frame in inputs {
            let result = if frame == "timer" {
                state.resyncing = false;
                Ok(Outcome::Suppress)
            } else {
                state.apply(frame)
            };
            let mut actual = serde_json::Map::new();
            actual.insert(
                "newResync".into(),
                json!(result.as_ref().is_ok_and(|r| *r == Outcome::Resync)),
            );
            match result {
                Ok(Outcome::Publish) => {
                    actual.insert("emitted".into(), book(&state.book));
                }
                Ok(_) => {}
                Err(error) => {
                    actual.insert("error".into(), error.into());
                }
            }
            actual.insert("state".into(),json!({"book":book(&state.book),"lastUpdate":state.last_update,"resyncing":state.resyncing}));
            let actual = normalize(&Value::Object(actual));
            native.push(actual);
        }
        assert_eq!(
            native.len(),
            expected.len(),
            "BOOK-CARDINALITY expected/native case {index}"
        );
        native_total += native.len();
        for (step, (actual, expected)) in native.iter().zip(expected).enumerate() {
            comparisons += 1;
            assert_eq!(actual, &normalize(expected), "case{index} step{step}");
        }
    }
    assert_eq!(comparisons, 1755, "BOOK-CARDINALITY comparisons");
    assert_eq!(native_total, 1755, "BOOK-CARDINALITY native total");
    assert_eq!(cases.len(), 996);
}
