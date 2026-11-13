#[path = "../src/history_output.rs"]
pub mod output;
use safe_cex_engine::{ClosedPositionFormat, Cx, VenueError, history, model, num};

#[test]
fn native_closed_output_keeps_missing_zero_fee_and_timestamp_semantics() {
    let output = output::ClosedPnlOutput::new(
        ClosedPositionFormat::Bybit,
        "fixture".into(),
        false,
        None,
        &[],
    );
    let rows = [
        r#"{"symbol":"BTCUSDH25","side":"Sell","closedSize":"1000","avgExitPrice":"100000","openFee":"0.00001","closeFee":"0.00002","createdTime":"1700000000000ms","updatedTime":"0","leverage":" 10x"}"#.to_owned(),
        r#"{"symbol":"SKIP","createdTime":"0"}"#.to_owned(),
        r#"{"symbol":0,"createdTime":"1"}"#.to_owned(),
        r#"{"symbol":false,"createdTime":"1"}"#.to_owned(),
        r#"{"symbol":"BTCUSDT","side":"Buy","closedSize":0,"createdTime":"1700000000001","updatedTime":"1700000001000","closedPnl":null}"#.to_owned(),
    ];
    let result: serde_json::Value = serde_json::from_str(&output.encode(&rows).unwrap()).unwrap();
    assert_eq!(result.as_array().unwrap().len(), 3);
    assert_eq!(result[0]["side"], "short");
    assert_eq!(result[0]["unrealizedPnl"], serde_json::Value::Null);
    assert_eq!(result[1]["contracts"], 0.01);
    assert_eq!(result[1]["notional"], 1000.);
    assert_eq!(result[1]["closedTime"], 0.);
    assert_eq!(result[1]["leverage"], 10.);
}

#[test]
fn absent_market_keeps_the_venue_instrument_and_size() {
    let output =
        output::ClosedPnlOutput::new(ClosedPositionFormat::Okx, String::new(), false, None, &[]);
    let rows = [
        r#"{"instId":"RETIRED-SWAP","cTime":"1","closeTotalPos":"2","closeAvgPx":"0","fee":"-1"}"#
            .to_owned(),
    ];
    let result: serde_json::Value = serde_json::from_str(&output.encode(&rows).unwrap()).unwrap();
    assert_eq!(result[0]["symbol"], "RETIRED-SWAP");
    assert_eq!(result[0]["contracts"], 2.);
    assert_eq!(result[0]["notional"], 0.);
    assert_eq!(result[0]["fees"], 1.);
}

#[test]
fn reported_field_decoding_keeps_native_coercion_and_literal_side_rules() {
    let output = output::ClosedPnlOutput::new(
        ClosedPositionFormat::Bybit,
        "fixture".into(),
        false,
        None,
        &[],
    );
    let rows = [
        r#"{"symbol":["BTCUSDT"],"side":["Sell"],"closedSize":[["2lots"],null],"avgExitPrice":"3x","createdTime":[" 12ms"],"updatedTime":{},"openFee":"-.25fee","closeFee":true,"leverage":1e-7}"#.into(),
        r#"{"symbol":"0","side":"Sell","closedSize":null,"createdTime":1,"closedPnl":{"unused":12}}"#.into(),
        "null".into(),
        "[]".into(),
        "true".into(),
    ];
    let result: serde_json::Value = serde_json::from_str(&output.encode(&rows).unwrap()).unwrap();
    assert_eq!(result.as_array().unwrap().len(), 2);
    assert_eq!(result[0]["symbol"], "BTCUSDT");
    assert_eq!(result[0]["side"], "short");
    assert_eq!(result[0]["contracts"], 2.);
    assert_eq!(result[0]["exitPrice"], 3.);
    assert_eq!(result[0]["closedTime"], serde_json::Value::Null);
    assert_eq!(result[0]["fees"], serde_json::Value::Null);
    assert_eq!(result[0]["leverage"], 1e-7);
    assert_eq!(result[1]["symbol"], "0");
    assert_eq!(result[1]["side"], "long");
    assert_eq!(result[1]["unrealizedPnl"], serde_json::Value::Null);
}

#[test]
fn native_field_names_remain_distinct_across_projection_formats() {
    let row = r#"{"symbol":"BTCUSDT","instId":"BTC-USDT-SWAP","createdTime":"1","cTime":"2","createTime":"3","ctime":"4","closedSize":"5","closeTotalPos":"6","closePositions":"7","qty":"8","realizedPnl":"9","realizedPNL":"10","closedPnl":"0","leverage":"11","lever":"12"}"#;
    for (format, symbol, time, size, pnl, leverage) in [
        (ClosedPositionFormat::Bybit, "BTCUSDT", 1., 5., 0., 11.),
        (ClosedPositionFormat::Okx, "BTC-USDT-SWAP", 2., 6., 9., 12.),
        (
            ClosedPositionFormat::Blofin,
            "BTC-USDT-SWAP",
            3.,
            7.,
            9.,
            11.,
        ),
        (ClosedPositionFormat::Bitunix, "BTCUSDT", 4., 8., 10., 11.),
    ] {
        let output = output::ClosedPnlOutput::new(format, String::new(), false, None, &[]);
        let result: serde_json::Value =
            serde_json::from_str(&output.encode(&[row.into()]).unwrap()).unwrap();
        assert_eq!(result[0]["symbol"], symbol);
        assert_eq!(result[0]["time"], time);
        assert_eq!(result[0]["contracts"], size);
        assert_eq!(result[0]["unrealizedPnl"], pnl);
        assert_eq!(result[0]["leverage"], leverage);
    }
}

#[test]
fn ignored_native_values_preserve_numeric_and_depth_refusals() {
    let nested =
        |value: &str, depth| format!("{}{}{}", "[".repeat(depth), value, "]".repeat(depth));
    let mut values = vec![
        ("1e309".to_owned(), true),
        ("-1e309".into(), true),
        (r#"{"x":1e309}"#.into(), true),
        ("[1e309]".into(), true),
        ("1e308".into(), false),
        (r#"{"x":[null,true,"1e309",1e308]}"#.into(), false),
    ];
    for depth in [126, 127, 128, 150] {
        values.push((nested("0", depth), depth >= 127));
    }
    for format in [
        ClosedPositionFormat::Bybit,
        ClosedPositionFormat::Okx,
        ClosedPositionFormat::Blofin,
        ClosedPositionFormat::Bitunix,
    ] {
        let output = output::ClosedPnlOutput::new(format, String::new(), false, None, &[]);
        for (value, refuses) in &values {
            for row in [
                format!(r#"{{"extra":{value}}}"#),
                format!(r#"{{"closedPnl":{value}}}"#),
                value.clone(),
            ] {
                // The original parser validates the whole row, even when it has no projected fields.
                let prior = serde_json::from_str::<serde_json::Value>(&row);
                let result = output.encode(std::slice::from_ref(&row));
                assert_eq!(result.is_err(), prior.is_err(), "{format:?}: {row}");
                if row.starts_with('{') && row.contains("\"extra\"") {
                    assert_eq!(prior.is_err(), *refuses, "depth/range control: {row}");
                }
                if let Err(error) = result {
                    assert_eq!(error.message, "invalid closed-position output");
                    assert_eq!(error.kind, None);
                }
            }
        }
    }
}

fn projected(format: ClosedPositionFormat, row: &serde_json::Value) -> serde_json::Value {
    let output = output::ClosedPnlOutput::new(format, "fixture".into(), false, None, &[]);
    serde_json::from_str::<serde_json::Value>(&output.encode(&[row.to_string()]).unwrap()).unwrap()
        [0]
    .clone()
}

#[test]
fn every_history_number_preserves_unknown_zero_and_valid_values() {
    use serde_json::{Value, json};
    for (format, symbol, opened, closed, mappings) in [
        (
            ClosedPositionFormat::Bybit,
            "symbol",
            "createdTime",
            "updatedTime",
            vec![
                ("closedSize", "contracts"),
                ("cumExitValue", "notional"),
                ("avgEntryPrice", "entryPrice"),
                ("avgExitPrice", "exitPrice"),
                ("closedPnl", "unrealizedPnl"),
                ("openFee", "fees"),
                ("closeFee", "fees"),
                ("leverage", "leverage"),
            ],
        ),
        (
            ClosedPositionFormat::Okx,
            "instId",
            "cTime",
            "uTime",
            vec![
                ("closeTotalPos", "contracts"),
                ("openAvgPx", "entryPrice"),
                ("closeAvgPx", "exitPrice"),
                ("realizedPnl", "unrealizedPnl"),
                ("fee", "fees"),
                ("lever", "leverage"),
            ],
        ),
        (
            ClosedPositionFormat::Blofin,
            "instId",
            "createTime",
            "updateTime",
            vec![
                ("closePositions", "contracts"),
                ("openAveragePrice", "entryPrice"),
                ("closeAveragePrice", "exitPrice"),
                ("realizedPnl", "unrealizedPnl"),
                ("fee", "fees"),
                ("leverage", "leverage"),
            ],
        ),
        (
            ClosedPositionFormat::Bitunix,
            "symbol",
            "ctime",
            "mtime",
            vec![
                ("qty", "contracts"),
                ("entryPrice", "entryPrice"),
                ("closePrice", "exitPrice"),
                ("realizedPNL", "unrealizedPnl"),
                ("fee", "fees"),
                ("leverage", "leverage"),
            ],
        ),
    ] {
        let mut base = json!({symbol: "BTCUSDT", opened: "100", closed: "200", "openFee": "0", "closeFee": "0"});
        for (input, _) in &mappings {
            base[*input] = json!("0");
        }
        for (input, output) in mappings
            .into_iter()
            .chain([(opened, "time"), (closed, "closedTime")])
        {
            for value in [
                None,
                Some(Value::Null),
                Some(json!("not-a-number")),
                Some(json!("NaN")),
                Some(json!("Infinity")),
                Some(json!("-Infinity")),
                Some(json!("1e309")),
            ] {
                let mut row = base.clone();
                if let Some(value) = value {
                    row[input] = value;
                } else {
                    row.as_object_mut().unwrap().remove(input);
                }
                assert_eq!(
                    projected(format, &row)[output],
                    Value::Null,
                    "{format:?} {input} {row}"
                );
            }
            for value in [json!("0"), json!(0), json!("12.5"), json!(12.5)] {
                let mut row = base.clone();
                row[input] = value.clone();
                let expected = if value == json!("0") || value == json!(0) {
                    0.
                } else if output == "time" || output == "closedTime" {
                    12.
                } else {
                    12.5
                };
                assert_eq!(
                    projected(format, &row)[output].as_f64(),
                    Some(expected),
                    "{format:?} {input} {value}"
                );
            }
        }
    }
}

#[test]
fn derived_history_values_require_all_inputs_and_finite_results() {
    use serde_json::{Value, json};
    for format in [
        ClosedPositionFormat::Okx,
        ClosedPositionFormat::Blofin,
        ClosedPositionFormat::Bitunix,
    ] {
        let row = json!({"symbol":"BTCUSDT","instId":"BTCUSDT","createdTime":"1","cTime":"1","createTime":"1","ctime":"1","closeTotalPos":"2","closePositions":"2","qty":"2"});
        assert_eq!(projected(format, &row)["notional"], Value::Null);
    }
    let inverse = json!({"symbol":"BTCUSD","createdTime":"1","closedSize":"10"});
    assert_eq!(
        projected(ClosedPositionFormat::Bybit, &inverse)["contracts"],
        Value::Null
    );
    assert_eq!(
        projected(ClosedPositionFormat::Bybit, &inverse)["notional"],
        10.
    );
    let overflow = json!({"symbol":"BTCUSDT","ctime":"1","qty":"1e308","closePrice":"1e308"});
    assert_eq!(
        projected(ClosedPositionFormat::Bitunix, &overflow)["notional"],
        Value::Null
    );
    let fee = json!({"symbol":"BTCUSDT","createdTime":"1","openFee":"1e308","closeFee":"1e308"});
    assert_eq!(
        projected(ClosedPositionFormat::Bybit, &fee)["fees"],
        Value::Null
    );
}

#[test]
fn native_unknown_fixture_is_the_projections_actual_serialized_output() {
    let output = output::ClosedPnlOutput::new(
        ClosedPositionFormat::Bybit,
        "fixture".into(),
        false,
        None,
        &[],
    );
    let rows = [
        r#"{"symbol":"BTCUSDT","side":"Sell"}"#.to_owned(),
        r#"{"symbol":"BTCUSDT","side":"Sell","createdTime":0,"updatedTime":0,"closedSize":"0","cumExitValue":"0","avgEntryPrice":"0","avgExitPrice":"0","closedPnl":"0","openFee":"0","closeFee":"0","leverage":"0"}"#.to_owned(),
    ];
    let actual: serde_json::Value = serde_json::from_str(&output.encode(&rows).unwrap()).unwrap();
    let expected: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/history-unknown-output.json")).unwrap();
    assert_eq!(actual, expected);
}
