use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_num::numeric_ingress::js_from_value;
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch, store::pack_ticker_deltas};
use serde_json::{Value, json};

fn nullable_index_ticker_fixture(index: Value) -> Value {
    json!({"id":"SOLV","symbol":"SOLVUSDT","bid":1.0,"ask":2.0,"last":3.0,"mark":4.0,
        "index":index,"percentage":6.0,"openInterest":7.0,"fundingRate":8.0,"fundingTime":9.0,
        "volume":10.0,"quoteVolume":11.0})
}

#[test]
fn store_null_index_survives_patches_and_aliases() {
    let store = DefaultStore::new();
    store
        .update_full(
            StoreUpdate {
                tickers: Some(vec![
                    js_from_value(nullable_index_ticker_fixture(json!(5.0))).unwrap(),
                ]),
                ..StoreUpdate::default()
            },
            false,
        )
        .unwrap();
    let mut patch = ModelPatch::default();
    patch.set_json("index", Value::Null);
    store.update_ticker("SOLV", &patch).unwrap();
    assert_eq!(store.tickers_by_id().unwrap()["SOLV"].index, None);
    assert_eq!(store.tickers_by_symbol().unwrap()["SOLVUSDT"].index, None);
    // The source static packer intentionally omits non-number values.
    let packed = pack_ticker_deltas(&[(0.0, patch)]).unwrap();
    assert!(packed.packed.is_empty());
    let mut patch = ModelPatch::default();
    patch.set_number("index", 17.0);
    store.update_ticker("SOLV", &patch).unwrap();
    assert_eq!(store.tickers_by_id().unwrap()["SOLV"].index, Some(17.0));
    assert_eq!(store.tickers_by_symbol().unwrap()["SOLVUSDT"].index, Some(17.0));
    assert_eq!(store.tickers_by_id().unwrap()["SOLV"].mark, 4.0);
    for index in [None, Some(-0.0_f64), Some(23.5)] {
        let mut patch = ModelPatch::default();
        patch.set_native("index", index);
        store.update_ticker("SOLV", &patch).unwrap();
        assert_eq!(
            store.tickers_by_id().unwrap()["SOLV"].index.map(f64::to_bits),
            index.map(f64::to_bits)
        );
        let packed = pack_ticker_deltas(&[(0.0, patch)]).unwrap();
        if let Some(index) = index {
            assert_eq!(packed.packed.len(), 1);
            assert_eq!(packed.packed[0].mask, 16.0);
            assert_eq!(packed.packed[0].values[0].to_bits(), index.to_bits());
        } else {
            assert!(packed.packed.is_empty());
        }
    }
}
