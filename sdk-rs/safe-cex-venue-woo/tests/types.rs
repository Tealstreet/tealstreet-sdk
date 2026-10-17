#[path = "support/field_identity.rs"]
mod identity;
use safe_cex_venue_woo::types::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

fn roundtrip<T: DeserializeOwned + Serialize>(value: Value) -> Result<Value, serde_json::Error> {
    serde_json::from_value::<T>(value).and_then(serde_json::to_value)
}

#[test]
fn legacy_declarations_preserve_values_and_reject_missing_or_null_required_fields() {
    let mut cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/types.json")).unwrap();
    assert_eq!(cases.len(), 140, "WOO-CARDINALITY retained declarations");
    cases.extend(serde_json::from_str::<Vec<Value>>(&identity::capture("types")).unwrap());
    for case in cases {
        let input = case["input"].clone();
        let result = match case["name"].as_str().unwrap() {
            "ACCOUNT_RESPONSE" => roundtrip::<AccountResponse>(input.clone()),
            "PLACE_ORDER_RESPONSE" => roundtrip::<PlaceOrderResponse>(input.clone()),
            "PLACE_ALGO_ORDER_RESPONSE" => roundtrip::<PlaceAlgoOrderResponse>(input.clone()),
            "GET_FUTURES_LEVERAGE_RESPONSE" => {
                roundtrip::<GetFuturesLeverageResponse>(input.clone())
            }
            "GET_KLINE_RESPONSE" => roundtrip::<GetKlineResponse>(input.clone()),
            name => panic!("unmapped TS type {name}"),
        };
        assert_eq!(
            result.is_ok(),
            case["accepted"].as_bool().unwrap(),
            "{}: {}: {result:?}",
            case["name"],
            case["label"]
        );
        if let Ok(output) = result {
            assert_eq!(output, input, "{}: {}", case["name"], case["label"]);
        }
    }
}
