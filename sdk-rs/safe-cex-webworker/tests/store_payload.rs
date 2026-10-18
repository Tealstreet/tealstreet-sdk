use safe_cex_exchange::store_payload::*;
use safe_cex_types::Ticker;
use serde_json::{Value, json};
fn number(v: &Value) -> f64 {
    match v.as_str() {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        _ => v.as_f64().unwrap(),
    }
}
fn encoded(n: f64) -> Value {
    if n.is_nan() {
        json!("NaN")
    } else if n == f64::INFINITY {
        json!("Infinity")
    } else if n == f64::NEG_INFINITY {
        json!("-Infinity")
    } else if n == 0.0 && n.is_sign_negative() {
        json!("-0")
    } else {
        json!(n)
    }
}
fn normalized(v: Value) -> Value {
    match v {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalized).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, normalized(v))).collect()),
        v => v,
    }
}
fn ticker(v: &Value) -> Ticker {
    Ticker {
        id: v["id"].as_str().unwrap().into(),
        symbol: v["symbol"].as_str().unwrap().into(),
        bid: number(&v["bid"]),
        ask: number(&v["ask"]),
        last: number(&v["last"]),
        mark: number(&v["mark"]),
        index: number(&v["index"]),
        percentage: number(&v["percentage"]),
        open_interest: number(&v["openInterest"]),
        funding_rate: number(&v["fundingRate"]),
        funding_time: number(&v["fundingTime"]),
        volume: number(&v["volume"]),
        quote_volume: number(&v["quoteVolume"]),
        mid_price: v.get("midPrice").map(number),
        timestamp: v.get("timestamp").map(number),
        info: v.get("info").cloned(),
    }
}
#[test]
fn snapshot_bookkeeping_matches_live_versions_and_ticker_transitions() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/store_payload_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        data["fields"],
        json!(safe_cex_store::store::PACKED_TICKER_FIELDS)
    );
    let versions = data["versions"].as_array().unwrap();
    assert_eq!(versions.len(), 243);
    for (i, v) in versions.iter().enumerate() {
        let update = v.get("update").map(number);
        let previous = (!v["previous"].is_null()).then(|| number(&v["previous"]));
        assert_eq!(
            should_send_update_id_backed_snapshot(
                update,
                previous,
                v.get("force").map(|x| x.as_bool().unwrap())
            ),
            v["send"].as_bool().unwrap(),
            "version decision {i}"
        );
        assert_eq!(
            record_sent_update_id_backed_snapshot(update)
                .map(encoded)
                .unwrap_or(Value::Null),
            normalized(v["record"].clone()),
            "record version {i}"
        );
    }
    let scenarios = data["scenarios"].as_array().unwrap();
    assert_eq!(scenarios.len(), 70, "TICKER-CARDINALITY scenario total");
    let mut comparisons = 0;
    for (i, s) in scenarios.iter().enumerate() {
        let mut baseline = TickerBaseline::default();
        let count = match i {
            0..66 => 5,
            66 | 67 => 8,
            68 => 2,
            69 => 6,
            _ => unreachable!("TICKER-CARDINALITY unexpected scenario"),
        };
        let actions = s["actions"].as_array().unwrap();
        let expected = s["expected"].as_array().unwrap();
        assert_eq!(
            actions.len(),
            count,
            "TICKER-CARDINALITY actions per scenario"
        );
        assert_eq!(
            expected.len(),
            count,
            "TICKER-CARDINALITY expected per scenario"
        );
        for j in 0..count {
            let action = actions
                .get(j)
                .expect("TICKER-CARDINALITY missing fixed action");
            let tickers: Vec<_> = action["tickers"]
                .as_array()
                .unwrap()
                .iter()
                .map(ticker)
                .collect();
            let actual = if action["op"] == "copy" {
                baseline.copy_from(&tickers);
                Value::Null
            } else {
                baseline
                    .pack_deltas(&tickers)
                    .map(|deltas| {
                        Value::Array(
                            deltas
                                .into_iter()
                                .map(|delta| {
                                    Value::Array(
                                        [encoded(delta.index), encoded(delta.mask)]
                                            .into_iter()
                                            .chain(delta.values.into_iter().map(encoded))
                                            .collect(),
                                    )
                                })
                                .collect(),
                        )
                    })
                    .unwrap_or(Value::Null)
            };
            let expected = expected
                .get(j)
                .expect("TICKER-CARDINALITY missing fixed expectation");
            assert_eq!(
                actual.as_array().map(Vec::len),
                expected.as_array().map(Vec::len),
                "TICKER-CARDINALITY expected/native deltas"
            );
            assert_eq!(
                actual,
                normalized(expected.clone()),
                "TICKER-CARDINALITY complete output scenario {i}, action {j}"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 354, "TICKER-CARDINALITY native comparisons");
}
