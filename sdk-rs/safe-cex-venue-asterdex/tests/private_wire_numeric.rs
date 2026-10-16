//! Independent raw-text witnesses for the six flattened response owners.
use safe_cex_venue_asterdex::types::*;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Map, Value};

fn exercise<T: DeserializeOwned>(
    name: &str,
    number: impl Fn(&T) -> f64,
    tag: impl Fn(&T) -> &str,
    extra: impl Fn(&T) -> &Map<String, Value>,
) {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/private-wire-numeric.json")).unwrap();
    assert_eq!(fixtures.as_object().unwrap().len(), 6);
    let row = &fixtures[name];
    let template = row["input"].to_string();
    assert_eq!(template.matches("\"__number__\"").count(), 1);
    for (literal, bits) in [
        ("1", 0x3ff0_0000_0000_0000),
        ("1.25", 0x3ff4_0000_0000_0000),
        ("-0", 0x8000_0000_0000_0000),
    ] {
        let text = template.replace("\"__number__\"", literal);
        let raw: Value = serde_json::from_str(&text).unwrap();
        for (path, result) in [
            ("text", serde_json::from_str::<T>(&text)),
            ("slice", serde_json::from_slice::<T>(text.as_bytes())),
            ("owned", serde_json::from_value::<T>(raw.clone())),
            ("qualified-borrowed", <T as Deserialize>::deserialize(&raw)),
        ] {
            let decoded = result
                .unwrap_or_else(|error| panic!("ASTER PUBLIC {name} {path} {literal}: {error}"));
            assert_eq!(
                number(&decoded).to_bits(),
                bits,
                "ASTER PUBLIC {name} {path} {literal} native bits"
            );
            assert_eq!(tag(&decoded), row["tagExpected"].as_str().unwrap());
            assert_eq!(extra(&decoded)["opaqueTag"], row["input"]["opaqueTag"]);
        }
    }
    for invalid in ["[]", "null", "true", "1.25", "\"record\""] {
        assert!(
            serde_json::from_str::<T>(invalid).is_err(),
            "{name} map shape {invalid}"
        );
    }
}

#[test]
fn filters_raw_numeric() {
    exercise::<AsterDEXMarketResponseFiltersItem>(
        "AsterDEXMarketResponseFiltersItem",
        |value| value.limit.unwrap(),
        |value| &value.filter_type,
        |value| &value.extra,
    );
}

#[test]
fn market_raw_numeric() {
    exercise::<AsterDEXMarketResponse>(
        "AsterDEXMarketResponse",
        |value| value.price_precision,
        |value| &value.symbol,
        |value| &value.extra,
    );
}

#[test]
fn position_raw_numeric() {
    exercise::<AsterDEXPositionResponse>(
        "AsterDEXPositionResponse",
        |value| value.update_time,
        |value| &value.symbol,
        |value| &value.extra,
    );
}

#[test]
fn order_raw_numeric() {
    exercise::<AsterDEXOrderResponse>(
        "AsterDEXOrderResponse",
        |value| value.order_id,
        |value| &value.symbol,
        |value| &value.extra,
    );
}

#[test]
fn ticker_raw_numeric() {
    exercise::<AsterDEXTickerResponse>(
        "AsterDEXTickerResponse",
        |value| value.open_time,
        |value| &value.symbol,
        |value| &value.extra,
    );
}

#[test]
fn websocket_raw_numeric() {
    exercise::<AsterDEXWebSocketMessage>(
        "AsterDEXWebSocketMessage",
        |value| value.capital_e,
        |value| &value.e,
        |value| &value.extra,
    );
}
