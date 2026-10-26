use safe_cex_types::CurrencyBalance;
use serde::Deserialize;

#[test]
fn currency_rows_are_named_objects_without_rest_normalization() {
    let declared = r#"{"used":2,"free":9,"total":11,"upnl":-3}"#;
    let alternate = r#"{"available":9,"locked":6}"#;
    for raw in [declared, alternate] {
        let text: CurrencyBalance = serde_json::from_str(raw).unwrap();
        let qualified =
            CurrencyBalance::deserialize(serde_json::from_str::<serde_json::Value>(raw).unwrap())
                .unwrap();
        for row in [text, qualified] {
            match row {
                CurrencyBalance::Declared(balance) => {
                    assert_eq!(balance.used.to_bits(), 2f64.to_bits());
                    assert_eq!(balance.free.to_bits(), 9f64.to_bits());
                    assert_eq!(balance.total.to_bits(), 11f64.to_bits());
                    assert_eq!(balance.upnl.to_bits(), (-3f64).to_bits());
                    assert!(balance.info.is_none());
                    assert!(balance.currencies.is_none());
                    assert!(balance.coins.is_none());
                }
                CurrencyBalance::AvailableLocked { available, locked } => {
                    assert_eq!(available.to_bits(), 9f64.to_bits());
                    assert_eq!(locked.to_bits(), 6f64.to_bits());
                    assert!(row.as_balance().is_none());
                }
            }
        }
    }
    // The alternate row array is complete and follows its declaration order.
    for raw in ["[9,6]", "[]", "null", "true", "3", "\"x\""] {
        assert!(
            serde_json::from_str::<CurrencyBalance>(raw).is_err(),
            "raw currency shape {raw}"
        );
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert!(
            CurrencyBalance::deserialize(value).is_err(),
            "qualified currency shape {raw}"
        );
    }
}
