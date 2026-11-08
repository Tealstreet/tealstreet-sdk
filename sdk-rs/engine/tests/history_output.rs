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
