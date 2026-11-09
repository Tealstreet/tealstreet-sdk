//! Compile and exercise the slice module without changing integrator-owned lib.rs.
pub use safe_cex_types::{model_field, object_map, retained_json, strict_decode};
#[path = "../src/balance.rs"]
mod balance;
pub use balance::{Balance, CoinBalance, CurrencyBalance};

#[test]
fn distinct_fields_omission_and_zero_survive_the_canonical_codec() {
    let input = r#"{"used":11.25,"free":22.5,"total":33.75,"upnl":-4.5,"coins":{"BTC":{"base":"BTC","amount":0,"notional":7}},"currencies":{"USD":{"used":1,"free":2,"total":3,"upnl":4}}}"#;
    let value: Balance = serde_json::from_str(input).unwrap();
    assert_eq!(
        (value.used, value.free, value.total, value.upnl),
        (11.25, 22.5, 33.75, -4.5)
    );
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        serde_json::to_string(&serde_json::from_str::<safe_cex_types::Balance>(input).unwrap())
            .unwrap()
    );
    assert_eq!(
        serde_json::to_string(&balance::empty_balance()).unwrap(),
        r#"{"used":0.0,"free":0.0,"total":0.0,"upnl":0.0}"#
    );
}

#[test]
fn optional_maps_reject_null_and_required_aggregates_never_default() {
    for input in [
        r#"{"free":0,"total":0,"upnl":0}"#,
        r#"{"used":0,"free":null,"total":0,"upnl":0}"#,
        r#"{"used":0,"free":0,"total":0,"upnl":0,"coins":null}"#,
        r#"{"used":0,"free":0,"total":0,"upnl":0,"currencies":null}"#,
        r#"[0,0,0,0]"#,
    ] {
        assert!(serde_json::from_str::<Balance>(input).is_err(), "{input}");
    }
    assert!(serde_json::from_str::<CoinBalance>(r#"["BTC",1,2]"#).is_err());
}

#[test]
fn retained_info_and_js_dictionary_key_order_are_preserved() {
    let input = r#"{"used":1,"free":2,"total":3,"upnl":4,"info":null,"coins":{"BTC":{"base":"BTC","amount":5,"notional":6},"10":{"base":"10","amount":7,"notional":8},"2":{"base":"2","amount":9,"notional":10},"__proto__":{"base":"__proto__","amount":11,"notional":12}}}"#;
    let value: Balance = serde_json::from_str(input).unwrap();
    assert_eq!(
        value
            .coins
            .as_ref()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["2", "10", "BTC", "__proto__"]
    );
    assert!(matches!(
        value.info,
        Some(retained_json::RetainedJsonInput::Null)
    ));
    let mut value = balance::empty_balance();
    value.info = Some(retained_json::RetainedJsonInput::Undefined);
    assert!(!serde_json::to_string(&value).unwrap().contains("info"));
}

#[test]
fn source_currency_shape_is_not_coerced_into_aggregate_fields() {
    let input =
        r#"{"used":1,"free":2,"total":3,"upnl":4,"currencies":{"CRO":{"available":7,"locked":9}}}"#;
    let value: Balance = serde_json::from_str(input).unwrap();
    assert!(matches!(
        value.currencies.as_ref().unwrap().get("CRO"),
        Some(CurrencyBalance::AvailableLocked {
            available: 7.0,
            locked: 9.0
        })
    ));
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        serde_json::to_string(&serde_json::from_str::<safe_cex_types::Balance>(input).unwrap())
            .unwrap()
    );
}
