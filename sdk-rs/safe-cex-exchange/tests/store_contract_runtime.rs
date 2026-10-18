use safe_cex_store::{Store, StoreUpdate};
use serde_json::json;
#[test]
fn snapshot_health_flags_and_leverage_keep_native_identity() {
    use safe_cex_exchange::native_store::DefaultStore;
    let flags = [
        "publicHealthy",
        "privateHealthy",
        "publicInitialized",
        "privateInitialized",
    ];
    for selected in flags {
        let real = DefaultStore::new();
        let mut health = serde_json::Map::new();
        for flag in flags {
            health.insert(flag.into(), json!(flag == selected));
        }
        let update: StoreUpdate=serde_json::from_value(json!({"socketHealth":health,"fetchedLeverage":{"identity":{"buyLeverage":11,"sellLeverage":23}}})).unwrap();
        Store::update(&real, update, false).unwrap();
        let actual = Store::socket_health(&real).unwrap();
        for flag in flags {
            assert_eq!(
                actual[flag],
                json!(flag == selected),
                "STORE-NATIVE one-hot socket flag {flag}"
            );
        }
        let leverage = real.fetched_leverage();
        assert_eq!(
            (
                leverage["identity"].buy_leverage,
                leverage["identity"].sell_leverage
            ),
            (11.0, 23.0),
            "STORE-NATIVE leverage side identity"
        );
    }
}
