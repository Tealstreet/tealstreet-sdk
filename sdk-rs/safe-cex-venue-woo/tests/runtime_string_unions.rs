use safe_cex_venue_woo::{
    public_parity::WooV3PublicParityRouteId,
    runtime_selector::{WooV3MigrationRouteId, WooV3RuntimeMode},
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
fn reject_shapes<T: DeserializeOwned>(literal: &str, label: &str) {
    let rejected: [Value; 9] = [
        json!({literal: null}),
        json!([literal]),
        Value::Null,
        json!(true),
        json!(42),
        json!(format!(" {literal}")),
        json!(format!("\u{feff}{literal}")),
        json!(literal.to_uppercase()),
        json!("not-a-route"),
    ];
    for (iteration, input) in rejected.into_iter().enumerate() {
        assert!(
            serde_json::from_value::<T>(input).is_err(),
            "native {label} rejects non-string/outside-union case {iteration}"
        );
    }
}
#[test]
fn public_route_string_only() {
    use WooV3PublicParityRouteId::*;
    for (input, expected) in [
        ("markets", Markets),
        ("tickers", Tickers),
        ("orderbook", Orderbook),
        ("recent-klines", RecentKlines),
        ("historical-klines", HistoricalKlines),
    ] {
        assert_eq!(
            serde_json::from_value::<WooV3PublicParityRouteId>(json!(input)).unwrap(),
            expected,
            "native public route discriminant"
        );
    }
    reject_shapes::<WooV3PublicParityRouteId>("markets", "public route");
}
#[test]
fn runtime_mode_string_only() {
    use WooV3RuntimeMode::*;
    for (input, expected) in [("legacy", Legacy), ("shadow-v3", ShadowV3), ("v3", V3)] {
        assert_eq!(
            serde_json::from_value::<WooV3RuntimeMode>(json!(input)).unwrap(),
            expected,
            "native runtime mode discriminant"
        );
    }
    reject_shapes::<WooV3RuntimeMode>("legacy", "runtime mode");
}
#[test]
fn migration_route_string_only() {
    use WooV3MigrationRouteId::*;
    let cases = [
        ("markets", Markets),
        ("tickers", Tickers),
        ("orderbook", Orderbook),
        ("recent-klines", RecentKlines),
        ("historical-klines", HistoricalKlines),
        ("place-order", PlaceOrder),
        ("cancel-order", CancelOrder),
        ("fetch-orders", FetchOrders),
        ("cancel-symbol-orders", CancelSymbolOrders),
        ("cancel-all-orders", CancelAllOrders),
        ("amend-order", AmendOrder),
        ("place-algo-order", PlaceAlgoOrder),
        ("amend-algo-order", AmendAlgoOrder),
        ("cancel-algo-order", CancelAlgoOrder),
        ("fetch-algo-orders", FetchAlgoOrders),
        ("cancel-all-algo-orders", CancelAllAlgoOrders),
        ("cancel-all-after", CancelAllAfter),
        ("recent-trades", RecentTrades),
        ("historical-trades", HistoricalTrades),
        ("transaction-history", TransactionHistory),
        ("account-info", AccountInfo),
        ("balances", Balances),
        ("positions", Positions),
        ("position-mode", PositionMode),
        ("fetch-futures-leverage", FetchFuturesLeverage),
        ("set-futures-leverage", SetFuturesLeverage),
    ];
    assert_eq!(cases.len(), 26, "independent migration route floor");
    for (input, expected) in cases {
        assert_eq!(
            serde_json::from_value::<WooV3MigrationRouteId>(json!(input)).unwrap(),
            expected,
            "native migration discriminant"
        );
    }
    reject_shapes::<WooV3MigrationRouteId>("markets", "migration route");
}
