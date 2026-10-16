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
        let _: std::cell::Ref<'_, [safe_cex_types::Market]> = store.markets();
        let _: std::cell::Ref<'_, [safe_cex_types::Ticker]> = store.tickers();
        let _: std::cell::Ref<'_, [safe_cex_types::Position]> = store.positions();
        let _: std::cell::Ref<'_, safe_cex_types::Balance> = store.balance();
        let _: std::cell::Ref<'_, safe_cex_types::StoreDataLoaded> = store.loaded();
        let _: safe_cex_types::StoreData = store.serialize();
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

#[test]
fn snapshot_health_flags_and_leverage_keep_native_identity() {
    use safe_cex_store::store::DefaultStore;
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
