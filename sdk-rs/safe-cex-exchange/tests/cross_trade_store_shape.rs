use safe_cex_exchange::cross_trade::CrossTradeStore;
use serde_json::json;

#[test]
fn store_snapshot_requires_an_object_and_retains_native_fields() {
    let snapshot: CrossTradeStore = serde_json::from_value(json!({
        "positionsLoaded": true,
        "positions": [],
        "markets": [],
        "tickers": [],
        "tickersBySymbol": {},
        "balance": null,
        "activeChasers": []
    }))
    .expect("a native store snapshot object");
    assert!(snapshot.positions_loaded);
    assert!(snapshot.positions.is_empty());
    assert!(snapshot.balance.is_none());

    let positional = json!([true, [], [], [], {}, null, []]);
    assert!(serde_json::from_value::<CrossTradeStore>(positional.clone()).is_err());
    assert!(serde_json::from_str::<CrossTradeStore>(&positional.to_string()).is_err());
}
