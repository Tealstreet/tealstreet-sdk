use safe_cex_store::{PackedTickerDelta, Store, StoreUpdate};
use serde_json::json;
#[test]
fn flat_packed_tuple_and_partial_update_preserve_wire_shape() {
    let tuple = PackedTickerDelta {
        index: 2.0,
        mask: 5.0,
        values: vec![99.0, 3.0],
    };
    assert_eq!(
        serde_json::to_value(&tuple).unwrap(),
        json!([2.0, 5.0, 99.0, 3.0])
    );
    assert_eq!(
        serde_json::from_value::<PackedTickerDelta>(json!([2.0, 5.0, 99.0, 3.0])).unwrap(),
        tuple
    );
    assert!(serde_json::from_value::<PackedTickerDelta>(json!([2])).is_err());
    let update: StoreUpdate = serde_json::from_value(
        json!({"latency":7.0,"userData":null,"tickerDeltas":[[2.0,5.0,99.0,3.0]]}),
    )
    .unwrap();
    assert_eq!(
        update.latency,
        Some(7.0),
        "STORE-NATIVE partial latency identity"
    );
    let packed = &update.ticker_deltas.as_ref().unwrap()[0];
    assert_eq!(
        (packed.index, packed.mask),
        (2.0, 5.0),
        "STORE-NATIVE packed index and mask identity"
    );
    assert_eq!(
        packed.values,
        [99.0, 3.0],
        "STORE-NATIVE packed values identity"
    );
    assert_eq!(
        serde_json::to_value(update).unwrap(),
        json!({"latency":7.0,"userData":null,"tickerDeltas":[[2.0,5.0,99.0,3.0]]})
    );
    assert_eq!(
        serde_json::to_value(StoreUpdate::default()).unwrap(),
        json!({})
    );
    assert!(serde_json::from_value::<StoreUpdate>(json!({"markets":null})).is_err());
}
#[test]
fn partial_snapshot_dictionaries_retain_source_insertion_order() {
    let update: StoreUpdate = serde_json::from_str(r#"{"ordersRisk":{"z":[],"a":[]}}"#).unwrap();
    assert_eq!(
        serde_json::to_string(&update).unwrap(),
        r#"{"ordersRisk":{"z":[],"a":[]}}"#
    );
}

#[test]
fn host_contract_is_object_safe_and_uses_typed_models() {
    fn read_contract(store: std::rc::Rc<dyn Store>) {
        store.update(StoreUpdate::default(), false).unwrap();
        let _: Result<Vec<safe_cex_types::Market>, safe_cex_types::StoreReadError> =
            store.markets();
        let _: Result<Vec<safe_cex_types::Ticker>, safe_cex_types::StoreReadError> =
            store.tickers();
        let _: Result<Vec<safe_cex_types::Position>, safe_cex_types::StoreReadError> =
            store.positions();
        let _: Result<safe_cex_types::Balance, safe_cex_types::StoreReadError> = store.balance();
        let _: Result<safe_cex_types::StoreDataLoaded, safe_cex_types::StoreReadError> =
            store.loaded();
        let _: Result<safe_cex_types::StoreData, safe_cex_types::StoreReadError> =
            store.serialize();
    }
    let _ = read_contract as fn(std::rc::Rc<dyn Store>);
}

#[test]
fn worker_counter_patch_does_not_invent_a_sibling_counter() {
    for wire in [
        r#"{"updateIds":{"orders":7.0}}"#,
        r#"{"updateIds":{"positions":0.0}}"#,
        r#"{"updateIds":{}}"#,
    ] {
        let update: StoreUpdate = serde_json::from_str(wire).unwrap();
        assert_eq!(serde_json::to_string(&update).unwrap(), wire);
    }
}
