use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Balance, object_map::JsObjectMap};
use serde_json::Value;

#[test]
fn production_balance_enumerates_recursive_keys_like_live_javascript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/balance_key_order.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 196);
    for row in rows {
        let balance: Balance = serde_json::from_value(row["balance"].clone()).unwrap();
        let real = DefaultStore::new();
        let host: &dyn Store = &real;
        host.update(
            StoreUpdate {
                balance: Some(balance),
                ..Default::default()
            },
            false,
        )
        .expect("fixture Store mutation must succeed");
        let balance = host.balance().unwrap();
        let currencies = balance.currencies.as_ref().unwrap();
        assert_eq!(
            serde_json::to_value(currencies.keys().collect::<Vec<_>>()).unwrap(),
            row["outer"],
            "live outer key order"
        );
        let nested = currencies["USDT"]
            .as_balance()
            .unwrap()
            .currencies
            .as_ref()
            .unwrap();
        assert_eq!(
            serde_json::to_value(nested.keys().collect::<Vec<_>>()).unwrap(),
            row["nested"],
            "live nested key order"
        );
        assert_eq!(
            serde_json::to_value(balance.coins.as_ref().unwrap().keys().collect::<Vec<_>>())
                .unwrap(),
            row["coins"],
            "live coin key order"
        );
        let encoded: Value =
            serde_json::from_str(&serde_json::to_string(&balance).unwrap()).unwrap();
        assert_eq!(
            encoded["currencies"]
                .as_object()
                .unwrap()
                .keys()
                .collect::<Vec<_>>(),
            currencies.keys().collect::<Vec<_>>(),
            "serialized key order"
        );
    }
}

#[test]
fn typed_writes_preserve_order_and_first_suffix_selection() {
    let mut fields: JsObjectMap<u32> = [("USDT".into(), 1), ("USDC".into(), 2)]
        .into_iter()
        .collect();
    assert_eq!(
        fields.keys().find(|key| key.ends_with('T')),
        Some(&"USDT".to_owned())
    );
    let assets: JsObjectMap<u32> = [("ZUSDT".into(), 1), ("AUSDT".into(), 2)]
        .into_iter()
        .collect();
    assert_eq!(
        assets
            .iter()
            .find(|(key, _)| key.ends_with("USDT"))
            .map(|(_, value)| *value),
        Some(1),
        "first suffix-match selects insertion-first currency"
    );
    fields.insert("USDT".into(), 3);
    fields.insert("10".into(), 10);
    fields.insert("2".into(), 2);
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        ["2", "10", "USDT", "USDC"],
        "typed insertion order"
    );
    fields.remove("USDT");
    fields.insert("USDT".into(), 4);
    assert_eq!(
        fields.keys().map(String::as_str).collect::<Vec<_>>(),
        ["2", "10", "USDC", "USDT"],
        "delete/reinsert order"
    );
    for input in ["[]", "[[\"USDT\", 1]]", "null", "false", "2"] {
        assert!(
            serde_json::from_str::<JsObjectMap<u32>>(input).is_err(),
            "map-only {input}"
        );
    }
}
