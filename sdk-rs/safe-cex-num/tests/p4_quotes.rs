use safe_cex_num::sizing::*;
use safe_cex_types::{Market, Ticker};
use serde_json::{Value, json};
fn number(v: &Value) -> f64 {
    match v.as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "-0" => -0.0,
        v => v.parse().unwrap(),
    }
}
#[test]
#[ignore = "requires root TypeScript dependencies"]
fn current_source_contracts_and_quotes() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_quotes_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 1216);
    for row in rows {
        let mut market:Market=serde_json::from_value(json!({"id":"m","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}})).unwrap();
        let actual = if row["op"] == "baseToNativeContracts" {
            market.contract_size = Some(number(&row["size"]));
            base_to_native_contracts(&market, number(&row["base"]))
        } else {
            let mut ticker:Ticker=serde_json::from_value(json!({"id":"t","symbol":"BTCUSDT","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
            ticker.last = number(&row["last"]);
            ticker.bid = number(&row["quote"]);
            ticker.ask = ticker.bid;
            let m = if row["price"].is_null() {
                None
            } else {
                market.precision.price = number(&row["price"]);
                Some(&market)
            };
            if row["op"] == "getTickerPredictedBestAsk" {
                get_ticker_predicted_best_ask(&ticker, m)
            } else {
                get_ticker_predicted_best_bid(&ticker, m)
            }
        };
        let expected = number(&row["expected"]);
        assert!(
            actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
            "{row}: actual {actual:?}"
        );
    }
}

#[test]
#[ignore = "requires root TypeScript dependencies"]
fn quotes_field_identity_supplement() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_quotes_field_identity.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 162);
    let decode = |v: &Value| f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap());
    for row in rows {
        let mut ticker: Ticker = serde_json::from_value(json!({"id":"id","symbol":"symbol","bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"openInterest":7,"fundingRate":8,"fundingTime":9,"volume":10,"quoteVolume":11})).unwrap();
        ticker.last = decode(&row["last"]);
        ticker.bid = decode(&row["bid"]);
        ticker.ask = decode(&row["ask"]);
        let market: Option<Market> = row["price"].as_f64().map(|price| serde_json::from_value(json!({"id":"id","symbol":"symbol","base":"B","quote":"Q","active":true,"precision":{"amount":0.01,"price":price},"limits":{"amount":{"min":0,"max":10},"leverage":{"min":1,"max":100}}})).unwrap());
        let actual = if row["op"] == "getTickerPredictedBestAsk" {
            get_ticker_predicted_best_ask(&ticker, market.as_ref())
        } else {
            get_ticker_predicted_best_bid(&ticker, market.as_ref())
        };
        let expected = decode(&row["expected"]);
        assert!(
            actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
            "QUOTE FIELD IDENTITY {}: {row}",
            row["op"]
        );
    }
}
