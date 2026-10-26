use safe_cex_types::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Debug)]
struct Inventory(Vec<String>);
impl std::fmt::Display for Inventory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.0)
    }
}
impl std::error::Error for Inventory {}
impl serde::de::Error for Inventory {
    fn custom<T: std::fmt::Display>(message: T) -> Self {
        Self(vec![message.to_string()])
    }
    fn unknown_variant(_variant: &str, expected: &'static [&'static str]) -> Self {
        Self(expected.iter().map(|value| (*value).to_owned()).collect())
    }
}
fn inventory<T: for<'de> Deserialize<'de>>() -> Vec<String> {
    T::deserialize(serde::de::value::StringDeserializer::<Inventory>::new(
        "__inventory_unknown__".into(),
    ))
    .err()
    .expect("unknown literal must fail")
    .0
}
fn reject_shapes<T: for<'de> Deserialize<'de>>(name: &str, values: &[String]) {
    for spelling in values {
        for case in [spelling.to_uppercase(), spelling.to_lowercase()] {
            if !values.contains(&case) {
                assert!(
                    serde_json::from_value::<T>(json!(case)).is_err(),
                    "{name}: unexpected case spelling"
                );
            }
        }
        assert!(
            serde_json::from_value::<T>(json!(spelling)).is_ok(),
            "{name}:{spelling}"
        );
        for wrong in [
            json!({spelling:null}),
            json!([spelling]),
            Value::Null,
            json!(0),
            json!(true),
            json!(format!(" {spelling}")),
            json!(format!("{spelling} ")),
            json!(format!("\u{feff}{spelling}")),
            json!(format!("{spelling}__unknown")),
        ] {
            let text = wrong.to_string();
            let mut input = serde_json::Deserializer::from_str(&text);
            // Public qualified calls must reach the strict trait, not a permissive inherent decoder.
            assert!(
                <T as Deserialize>::deserialize(&mut input).is_err(),
                "{name} accepted {wrong}"
            );
        }
    }
}
#[test]
#[ignore = "compares canonical serde accepted spellings with live TypeScript AST"]
fn shared_string_union_accepted_set_matches_typescript() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/strict_wire_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    macro_rules! check { ($($ty:ident),* $(,)?) => {$({
        let actual=inventory::<$ty>();
        assert_eq!(json!(actual),source[stringify!($ty)],"{} accepted spelling inventory",stringify!($ty));
        reject_shapes::<$ty>(stringify!($ty),&actual);
    })*}; }
    check!(
        OrderStatus,
        OrderType,
        OrderSide,
        OrderTimeInForce,
        OrderStopTriggerType,
        OrderRejectReason,
        PositionSide,
        PositionMode,
        MarginType,
        Timeframe,
        ExchangeName,
        DataProvenance,
        IncomeCategory,
        RequestOutcome,
        MessageHandlerStatsType,
        SecondsTimeframe,
        ExtendedTimeframe,
        ExchangeAccountConfigBybitSiteId,
        ExchangeAccountConfigHealthStatus,
        ExchangeOptionsBybitSiteId,
        ExchangeOptionsExtraAsterdexApiVersion,
        ExchangeOptionsExtraHyrotraderMode,
        ExchangeOptionsExtraLighterBuilderFeeMode,
        ExchangeOptionsExternalPublicDataVariant0Type,
        ExchangeOptionsExternalPublicDataVariant1Type
    );
}
#[test]
fn qualified_decoder_requires_strings() {
    use serde::de::IntoDeserializer;
    assert!(OrderSide::deserialize(json!({"buy":null}).into_deserializer()).is_err());
    assert!(
        OrderStopTriggerType::deserialize(json!({"mark_price":null}).into_deserializer()).is_err()
    );
    assert_eq!(
        OrderSide::deserialize(json!("buy").into_deserializer()).unwrap(),
        OrderSide::Buy
    );
}
#[test]
fn complete_record_sequences_are_rejected_at_nested_nodes() {
    for wrong in [
        json!([false, false, false, false, false]),
        json!([0, 0, 0, 0]),
        json!([1, 2]),
    ] {
        assert!(serde_json::from_value::<StoreDataLoaded>(wrong.clone()).is_err());
        assert!(serde_json::from_value::<Balance>(wrong.clone()).is_err());
        assert!(serde_json::from_value::<MarketPrecision>(wrong).is_err());
    }
    let valid = json!({"used":1,"free":2,"total":3,"upnl":4});
    assert!(serde_json::from_value::<Balance>(valid).is_ok());
    assert!(serde_json::from_value::<MarketLimits>(json!({"amount":[1,2]})).is_err());
}
