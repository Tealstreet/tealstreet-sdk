use safe_cex_types::store_model::{
    DisplayedMargin, FetchedLeverage, StoreData, StoreDataLoaded, StoreOptions, StoreUpdateIds,
    initial_store_data,
};
use serde_json::{Value, json};

#[test]
fn initial_store_is_complete_and_nullable_latency_is_required() {
    let initial = initial_store_data();
    let mut value = serde_json::to_value(&initial).unwrap();
    assert_eq!(value["latency"], Value::Null);
    assert_eq!(value["updateIds"], json!({"orders":0.0,"positions":0.0}));
    assert_eq!(
        value["loaded"],
        json!({"balance":false,"orders":false,"markets":false,"tickers":false,"positions":false})
    );
    assert!(value.get("userData").is_none());
    assert_eq!(
        serde_json::from_value::<StoreData>(value.clone()).unwrap(),
        initial
    );
    value["latency"] = json!(0.0);
    assert_eq!(
        serde_json::from_value::<StoreData>(value.clone())
            .unwrap()
            .latency,
        Some(0.0)
    );
    value.as_object_mut().unwrap().remove("latency");
    assert!(serde_json::from_value::<StoreData>(value).is_err());
}

#[test]
fn scalar_store_records_preserve_zero_false_and_optional_presence() {
    let leverage: FetchedLeverage =
        serde_json::from_value(json!({"buyLeverage":0.0,"sellLeverage":30.0,"isFetching":false}))
            .unwrap();
    assert_eq!(leverage.buy_leverage, 0.0);
    assert_eq!(leverage.sell_leverage, 30.0);
    assert_eq!(leverage.is_fetching, Some(false));
    assert_eq!(
        serde_json::to_value(leverage).unwrap(),
        json!({"buyLeverage":0.0,"sellLeverage":30.0,"isFetching":false})
    );
    for invalid in [
        json!({"buyLeverage":null,"sellLeverage":0}),
        json!({"buyLeverage":0}),
        json!({"buyLeverage":0,"sellLeverage":0,"isFetching":null}),
        json!({"buyLeverage":0,"sellLeverage":0,"marginType":"invalid"}),
    ] {
        assert!(serde_json::from_value::<FetchedLeverage>(invalid).is_err());
    }
    assert!(
        serde_json::from_value::<StoreDataLoaded>(json!([false, false, false, false, false]))
            .is_err()
    );
    assert!(serde_json::from_value::<StoreOptions>(json!([false])).is_err());
    assert!(serde_json::from_value::<StoreUpdateIds>(json!([0, 0])).is_err());
    assert!(serde_json::from_value::<DisplayedMargin>(json!([0, 0, 0])).is_err());
}
