use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::Store;
use safe_cex_store::{StoreUpdate, model_patch::ModelPatch};
use safe_cex_types::Ticker;

#[test]
fn ticker_timestamp_survives_store_insert_and_numeric_patch() {
    let ticker: Ticker = serde_json::from_str(r#"{"id":"x","symbol":"Y","bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"openInterest":7,"fundingRate":8,"fundingTime":9,"volume":10,"quoteVolume":17,"timestamp":123.25}"#).unwrap();
    let store = DefaultStore::new();
    store
        .update_full(
            StoreUpdate {
                tickers: Some(vec![ticker]),
                ..StoreUpdate::default()
            },
            false,
        )
        .unwrap();
    assert_eq!(
        store.tickers_by_id().unwrap()["x"]
            .timestamp
            .unwrap()
            .to_bits(),
        123.25_f64.to_bits(),
        "TICKER-TIMESTAMP Store insert identity"
    );
    assert_eq!(
        store.tickers_by_symbol().unwrap()["Y"]
            .timestamp
            .unwrap()
            .to_bits(),
        123.25_f64.to_bits(),
        "TICKER-TIMESTAMP Store symbol insert identity"
    );
    let mut comparisons = 0;
    for timestamp in [-0.0_f64, 456.75, f64::INFINITY] {
        let mut patch = ModelPatch::default();
        patch.set_number("timestamp", timestamp);
        store.update_ticker("x", &patch).unwrap();
        assert_eq!(
            store.tickers_by_id().unwrap()["x"]
                .timestamp
                .unwrap()
                .to_bits(),
            timestamp.to_bits(),
            "TICKER-TIMESTAMP Store numeric patch identity"
        );
        assert_eq!(
            store.tickers_by_id().unwrap()["x"].quote_volume.to_bits(),
            17_f64.to_bits(),
            "TICKER-TIMESTAMP Store unrelated field identity"
        );
        assert_eq!(
            store.tickers_by_symbol().unwrap()["Y"]
                .timestamp
                .unwrap()
                .to_bits(),
            timestamp.to_bits(),
            "TICKER-TIMESTAMP Store symbol numeric patch identity"
        );
        comparisons += 3;
    }
    assert_eq!(comparisons, 9, "TICKER-TIMESTAMP Store comparison count");
    // A full source replacement must clear the old runtime extra in both aliases.
    let replacement: Ticker = serde_json::from_str(r#"{"id":"x","symbol":"Y","bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"openInterest":7,"fundingRate":8,"fundingTime":9,"volume":10,"quoteVolume":17}"#).unwrap();
    store
        .update_full(
            StoreUpdate {
                tickers: Some(vec![replacement]),
                ..StoreUpdate::default()
            },
            false,
        )
        .unwrap();
    assert_eq!(
        store.tickers_by_id().unwrap()["x"].timestamp,
        None,
        "TICKER-TIMESTAMP Store id absence replacement"
    );
    assert_eq!(
        store.tickers_by_symbol().unwrap()["Y"].timestamp,
        None,
        "TICKER-TIMESTAMP Store symbol absence replacement"
    );
}
