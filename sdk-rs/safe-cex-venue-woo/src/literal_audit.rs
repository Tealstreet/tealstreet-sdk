use super::*;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
#[derive(Default)]
struct EnumInventory(Option<&'static [&'static str]>);
impl<'de> serde::Deserializer<'de> for &mut EnumInventory {
    type Error = serde::de::value::Error;
    fn deserialize_enum<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0 = Some(variants);
        Err(serde::de::Error::custom(
            "inventory captured; no value requested",
        ))
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        Err(serde::de::Error::custom("expected derived enum metadata"))
    }
    serde::forward_to_deserialize_any! {
        bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
        option unit unit_struct newtype_struct seq tuple tuple_struct map struct
        identifier ignored_any
    }
}

fn check_inputs<T: DeserializeOwned>(expected: &[String], label: &str, failures: &mut Vec<String>) {
    for value in expected {
        assert!(
            serde_json::from_value::<T>(json!(value)).is_ok(),
            "{label}: {value}"
        );
        for rejected in [
            json!({value:null}),
            json!({value:[]}),
            json!([value]),
            json!(value.to_lowercase()),
            json!(value.to_uppercase()),
            json!(format!("{value} ")),
            json!(format!("\u{feff}{value}")),
        ] {
            if rejected
                .as_str()
                .is_some_and(|s| expected.iter().any(|v| v == s))
            {
                continue;
            }
            if serde_json::from_value::<T>(rejected.clone()).is_ok() {
                failures.push(format!("{label}: {rejected}"));
            }
        }
    }
    for rejected in [
        Value::Null,
        json!(true),
        json!(1),
        json!({}),
        json!([]),
        json!("unknown-control"),
        json!("constructor"),
        json!("__proto__"),
    ] {
        if serde_json::from_value::<T>(rejected.clone()).is_ok() {
            failures.push(format!("{label}: {rejected}"));
        }
    }
}
#[test]
fn literal_union_actual_serde_metadata_and_rejected_shapes_match_typescript() {
    let fixture: Value =
        serde_json::from_str(include_str!("../tests/fixtures/contracts.json")).unwrap();
    let sets = fixture["literalSets"].as_object().unwrap();
    let mut seen = Vec::new();
    let mut failures = Vec::new();
    macro_rules! audit {
        ($ty:ident, $wire:ident, $key:literal) => {{
            let expected: Vec<String> = serde_json::from_value(sets[$key].clone()).unwrap();
            let mut inventory = EnumInventory::default();
            let _ = $wire::deserialize(&mut inventory);
            assert_eq!(
                inventory
                    .0
                    .unwrap()
                    .iter()
                    .copied()
                    .collect::<std::collections::BTreeSet<_>>(),
                expected
                    .iter()
                    .map(String::as_str)
                    .collect::<std::collections::BTreeSet<_>>(),
                "{} metadata",
                $key
            );
            check_inputs::<$ty>(&expected, $key, &mut failures);
            seen.push($key);
        }};
    }
    audit!(WooV3Endpoint, WooV3EndpointWire, "WooV3Endpoint");
    audit!(WooV3HttpMethod, WooV3HttpMethodWire, "WooV3HttpMethod");
    audit!(
        WooV3MigrationGate,
        WooV3MigrationGateWire,
        "WooV3MigrationGate"
    );
    audit!(
        WooV3MigrationRouteRuntimeLifecycle,
        WooV3MigrationRouteRuntimeLifecycleWire,
        "WooV3MigrationRoute.runtimeLifecycle"
    );
    audit!(
        WooV3PositionPositionSide,
        WooV3PositionPositionSideWire,
        "WooV3Position.positionSide"
    );
    audit!(
        WooV3AccountInfoPositionMode,
        WooV3AccountInfoPositionModeWire,
        "WooV3AccountInfo.positionMode"
    );
    audit!(
        WooV3PlaceOrderRequestSide,
        WooV3PlaceOrderRequestSideWire,
        "WooV3PlaceOrderRequest.side"
    );
    audit!(
        WooV3PlaceOrderRequestType,
        WooV3PlaceOrderRequestTypeWire,
        "WooV3PlaceOrderRequest.type"
    );
    audit!(
        WooV3PlaceOrderRequestPositionSide,
        WooV3PlaceOrderRequestPositionSideWire,
        "WooV3PlaceOrderRequest.positionSide"
    );
    audit!(
        WooV3PlaceOrderRequestMarginMode,
        WooV3PlaceOrderRequestMarginModeWire,
        "WooV3PlaceOrderRequest.marginMode"
    );
    audit!(
        WooV3PlaceOrderRequestBidAskLevel,
        WooV3PlaceOrderRequestBidAskLevelWire,
        "WooV3PlaceOrderRequest.bidAskLevel"
    );
    audit!(
        WooV3ListenKeyRequestType,
        WooV3ListenKeyRequestTypeWire,
        "WooV3ListenKeyRequest.type"
    );
    assert_eq!(
        seen.len(),
        sets.len(),
        "new TS literal union needs auditing"
    );
    assert!(
        failures.is_empty(),
        "accepted rejected TS inputs: {failures:?}"
    );
}

#[test]
fn number_or_string_siblings_reject_other_json_kinds() {
    fn check<T: DeserializeOwned>() {
        for value in [json!("text"), json!(0), json!(-1.25)] {
            assert!(serde_json::from_value::<T>(value).is_ok());
        }
        for value in [
            Value::Null,
            json!(false),
            json!([]),
            json!({}),
            json!({"String":"text"}),
            json!({"Number":1}),
        ] {
            assert!(
                serde_json::from_value::<T>(value.clone()).is_err(),
                "accepted {value}"
            );
        }
    }
    check::<crate::types::PlaceAlgoOrderResponseDataRowsItemOrderId>();
    check::<WooV3InstrumentBaseAssetMultiplier>();
    check::<WooV3InstrumentBaseMin>();
    check::<WooV3InstrumentBaseMax>();
    check::<WooV3InstrumentBaseTick>();
    check::<WooV3InstrumentQuoteTick>();
    check::<WooV3InstrumentMinNotional>();
    check::<WooV3InstrumentBaseIMR>();
}
