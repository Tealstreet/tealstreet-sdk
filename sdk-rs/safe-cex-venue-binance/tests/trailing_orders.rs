use safe_cex_num::number_from_string;
use safe_cex_types::*;
use safe_cex_venue_binance::trailing_orders::*;
use serde_json::{Value, json};

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn trailing_order_payloads_match_live_typescript() {
    let output = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/trailing-orders.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        cases.len(),
        3794,
        "SOURCE-CARDINALITY trailing_orders total"
    );
    let mut completed_comparisons = 0;
    for (index, c) in cases.iter().enumerate() {
        let n = |key: &str| number_from_string(c[key].as_str().unwrap());
        let mut opts: PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":c["type"],"side":c["side"],"amount":999}),
        )
        .unwrap();
        opts.price = if c["price"] == "undefined" {
            None
        } else {
            Some(n("price"))
        };
        opts.reduce_only = c["reduceOnly"].as_bool();
        let mut market:Market=serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"price":1,"amount":1},"limits":{"amount":{"min":0,"max":1000},"leverage":{"min":1,"max":100}}})).unwrap();
        market.precision.price = n("step");
        let mut ticker:Ticker=serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","bid":0,"ask":0,"last":1,"mark":0,"index":0,"percentage":0,"volume":0,"quoteVolume":0,"fundingRate":0,"fundingTime":0,"openInterest":0})).unwrap();
        ticker.last = n("last");
        let mut p:Position=serde_json::from_value(json!({"id":"p","symbol":"BTCUSDT","side":if opts.side==OrderSide::Buy {"short"}else{"long"},"entryPrice":0,"notional":0,"leverage":0,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0})).unwrap();
        p.contracts = n("contracts");
        let positions = match c["positions"].as_str().unwrap() {
            "empty" => vec![],
            "wrong-symbol" => {
                p.symbol = "OTHER".into();
                vec![p]
            }
            "wrong-side" => {
                p.side = if opts.side == OrderSide::Buy {
                    safe_cex_types::ModelField::Value(PositionSide::Long)
                } else {
                    safe_cex_types::ModelField::Value(PositionSide::Short)
                };
                vec![p]
            }
            "duplicates" => {
                let mut second = p.clone();
                second.contracts = 999.0;
                vec![p, second]
            }
            _ => vec![p],
        };
        let mut minted = 0;
        let mint = || {
            minted += 1;
            "uuid-test".into()
        };
        let formatter = if c["variant"] == "binance" {
            format_create_trailing_stop_loss_order
        } else {
            format_pm_create_trailing_stop_loss_order
        };
        let result = formatter(
            &opts,
            c["market"].as_bool().unwrap().then_some(&market),
            c["ticker"].as_bool().unwrap().then_some(&ticker),
            &positions,
            c["hedged"].as_bool().unwrap(),
            mint,
        );
        let actual = match result {
            Ok(value) => json!({"value":value}),
            Err(error) => json!({"error":error}),
        };
        assert_eq!(actual, c["expected"], "case {index}: {c}");
        assert_eq!(json!(minted), c["minted"], "mint count {index}: {c}");
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 3794,
        "NATIVE-CARDINALITY trailing_orders completed comparisons"
    );
    eprintln!("{} live trailing-order comparisons", cases.len());
}
