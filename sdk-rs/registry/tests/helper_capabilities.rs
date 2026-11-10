use safe_cex_venues::EngineOptions;
use serde_json::json;

#[test]
fn native_helper_capabilities_match_the_shared_phemex_declaration() {
    let options = serde_json::from_str::<EngineOptions>("{}").unwrap();
    let phemex = serde_json::to_value(options.construction("phemex").bridge).unwrap();
    assert_eq!(phemex["helpers"], json!({"resolveLeverageContext":true}));
    for venue in ["okx", "bybit", "binance", "unknown"] {
        let other = serde_json::to_value(options.construction(venue).bridge).unwrap();
        assert!(other.get("helpers").is_none(), "{venue}");
    }
}
