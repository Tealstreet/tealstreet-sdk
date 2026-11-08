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
    assert_eq!(result.as_array().unwrap().len(), 2);
    assert_eq!(result[0]["side"], "short");
    assert_eq!(result[0]["unrealizedPnl"], 0.);
    assert_eq!(result[1]["contracts"], 0.01);
    assert_eq!(result[1]["notional"], 1000.);
    assert_eq!(result[1]["closedTime"], 1700000000000.);
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
    assert_eq!(result[0]["closedTime"], 12.);
    assert_eq!(result[0]["fees"], -0.25);
    assert_eq!(result[0]["leverage"], 1e-7);
    assert_eq!(result[1]["symbol"], "0");
    assert_eq!(result[1]["side"], "long");
    assert_eq!(result[1]["unrealizedPnl"], 0.);
}

#[test]
fn native_field_names_remain_distinct_across_projection_formats() {
    let row = r#"{"symbol":"BTCUSDT","instId":"BTC-USDT-SWAP","createdTime":"1","cTime":"2","createTime":"3","ctime":"4","closedSize":"5","closeTotalPos":"6","closePositions":"7","qty":"8","realizedPnl":"9","realizedPNL":"10","leverage":"11","lever":"12"}"#;
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
