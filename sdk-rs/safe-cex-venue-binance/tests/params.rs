use safe_cex_num::js_number_string;
use safe_cex_types::HistoricalTradeOptions;
use safe_cex_venue_binance::utils::{
    USER_TRADES_MAX_LIMIT, USER_TRADES_MAX_WINDOW_MS, build_user_trades_params,
};
use serde_json::{Value, json};

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn user_trade_params_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!("{}/scripts/params.mjs", env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(corpus["maxLimit"].as_f64().unwrap(), USER_TRADES_MAX_LIMIT);
    assert_eq!(
        corpus["maxWindow"].as_f64().unwrap(),
        USER_TRADES_MAX_WINDOW_MS
    );
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 4161, "SOURCE-CARDINALITY params total");
    let mut completed_comparisons = 0;
    for (index, row) in rows.iter().enumerate() {
        if let Some(special) = row["special"].as_str() {
            let mut opts: HistoricalTradeOptions =
                serde_json::from_value(json!({"symbol":"BTCUSDT"})).unwrap();
            opts.limit = Some(match special {
                "NaN" => f64::NAN,
                "Infinity" => f64::INFINITY,
                "-Infinity" => f64::NEG_INFINITY,
                _ => unreachable!(),
            });
            assert_eq!(
                js_number_string(build_user_trades_params(&opts).limit),
                row["expected"].as_str().unwrap()
            );
            completed_comparisons += 1;
            continue;
        }
        let mut input = row["input"].clone();
        // Source nullish options have the same behavior as absent typed options.
        input.as_object_mut().unwrap().retain(|_, v| !v.is_null());
        let opts: HistoricalTradeOptions = serde_json::from_value(input).unwrap();
        let native = build_user_trades_params(&opts);
        let expected = &row["expected"];
        assert_eq!(
            native.symbol,
            expected["symbol"].as_str().unwrap(),
            "native params symbol case {index}"
        );
        assert_eq!(
            native.limit,
            expected["limit"].as_f64().unwrap(),
            "native params limit case {index}"
        );
        assert_eq!(
            native.order_id,
            expected["orderId"].as_str(),
            "native params orderId case {index}"
        );
        assert_eq!(
            native.start_time,
            expected["startTime"].as_f64(),
            "native params startTime case {index}"
        );
        assert_eq!(
            native.end_time,
            expected["endTime"].as_f64(),
            "native params endTime case {index}"
        );
        assert_eq!(
            native.from_id,
            expected["fromId"].as_f64(),
            "native params fromId case {index}"
        );
        let actual = serde_json::to_value(native).unwrap();
        fn normalize(value: Value) -> Value {
            match value {
                Value::Number(n) => json!(n.as_f64().unwrap()),
                Value::Object(o) => o.into_iter().map(|(k, v)| (k, normalize(v))).collect(),
                other => other,
            }
        }
        assert_eq!(
            normalize(actual),
            normalize(row["expected"].clone()),
            "case {index}: {row}"
        );
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 4161,
        "NATIVE-CARDINALITY params completed comparisons"
    );
    eprintln!("{} live params cases", rows.len());
}
