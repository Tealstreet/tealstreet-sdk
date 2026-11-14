pub use safe_cex_types::{
    MarketLimits, MarketPrecision, MarketRuntime, OptionTerms, retained_json,
};
#[path = "../src/market.rs"]
pub mod market;

#[test]
fn slice_model_has_the_complete_canonical_market_declaration() {
    use ts_rs::TS;
    let config = ts_rs::Config::default();
    assert_eq!(
        market::Market::decl(&config),
        safe_cex_types::Market::decl(&config)
    );
}

#[test]
fn native_zero_limits_and_missing_option_metadata_remain_distinct_from_null() {
    let source = r#"{"id":"native-id","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":false,"precision":{"amount":0.001,"price":0.1},"limits":{"amount":{"min":0,"max":0},"leverage":{"min":0,"max":0}},"contractSize":0,"takerFee":0}"#;
    let market: safe_cex_types::Market = serde_json::from_str(source).unwrap();
    assert_eq!(market.id, "native-id");
    assert_eq!(market.contract_size, Some(0.));
    assert_eq!(market.taker_fee, Some(0.));
    assert!(market.option.is_none());
    assert!(market.limits.price.is_none());
    let value = serde_json::to_value(&market).unwrap();
    assert_eq!(value["limits"]["amount"]["max"].as_f64(), Some(0.));
    assert!(value.get("option").is_none());
    assert!(value.get("makerFee").is_none());
    for field in ["option", "contractSize", "makerFee"] {
        let mut invalid = value.clone();
        invalid[field] = serde_json::Value::Null;
        assert!(
            serde_json::from_value::<safe_cex_types::Market>(invalid).is_err(),
            "{field}"
        );
    }
}
