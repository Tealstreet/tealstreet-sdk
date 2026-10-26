use safe_cex_types::{
    OrderFillEvent, OrderSide, READ_ONLY_ORDER_STATUSES, TIMEFRAME_TO_SECONDS, WritableStoreData,
};
use serde_json::{Value, json};
#[test]
#[ignore = "runs current exported TypeScript value constants"]
fn constants_match_current_source_in_value_and_order() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_constants_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected = value["timeframeToSeconds"].as_object().unwrap();
    assert_eq!(TIMEFRAME_TO_SECONDS.len(), 27);
    assert_eq!(
        TIMEFRAME_TO_SECONDS.keys().copied().collect::<Vec<_>>(),
        expected.keys().map(String::as_str).collect::<Vec<_>>()
    );
    for (k, v) in TIMEFRAME_TO_SECONDS.iter() {
        assert_eq!(v.to_bits(), expected[*k].as_f64().unwrap().to_bits());
    }
    assert_eq!(
        serde_json::to_value(READ_ONLY_ORDER_STATUSES).unwrap(),
        value["readOnlyOrderStatuses"]
    );
}
#[test]
fn fill_and_writable_store_preserve_their_own_projection() {
    let fill = OrderFillEvent {
        id: "fill".into(),
        amount: 2.0,
        price: 3.0,
        side: OrderSide::Buy,
        symbol: "BTCUSDT".into(),
        order_id: Some("order".into()),
        realized_pnl: None,
        fee: Some(0.1),
    };
    let encoded = serde_json::to_value(&fill).unwrap();
    assert_eq!(encoded["orderId"], "order");
    assert!(!encoded.as_object().unwrap().contains_key("realizedPnl"));
    assert_eq!(
        serde_json::from_value::<OrderFillEvent>(encoded).unwrap(),
        fill
    );
    assert!(
        serde_json::from_value::<OrderFillEvent>(
            json!({"id":"fill","amount":2,"price":3,"side":"buy","symbol":"BTCUSDT","fee":null})
        )
        .is_err()
    );
    let mut projected:WritableStoreData=serde_json::from_value(json!({"latency":1,"balance":{"used":0,"free":0,"total":0,"upnl":0},"markets":[],"tickers":[],"orders":[],"positions":[],"loaded":{"balance":false,"orders":false,"markets":false,"tickers":false,"positions":false},"options":{"isHedged":false},"fetchedLeverage":{}})).unwrap();
    projected.balance.total = 4.0;
    assert_eq!(projected.balance.total, 4.0);
    let encoded = serde_json::to_value(projected).unwrap();
    assert_eq!(encoded.as_object().unwrap().len(), 9);
    assert!(!encoded.as_object().unwrap().contains_key("updateIds"));
}
