use safe_cex_venue_bitrue::wire::{BitrueOrderType, FetchOrdersResponseDataItemSide};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::fmt::Debug;

fn string_boundary<T: DeserializeOwned + Serialize + PartialEq + Debug>(
    cases: &[(&str, T)],
    owner: &str,
) {
    for (tag, expected) in cases {
        let value = Value::String((*tag).into());
        let text = serde_json::to_string(tag).unwrap();
        let values = [
            serde_json::from_value::<T>(value.clone()).unwrap(),
            T::deserialize(&value).unwrap(),
            {
                let mut decoder = serde_json::Deserializer::from_str(&text);
                let decoded = <T as Deserialize>::deserialize(&mut decoder).unwrap();
                decoder.end().unwrap();
                decoded
            },
            serde_json::from_str::<T>(&text).unwrap(),
        ];
        for actual in values {
            assert_eq!(&actual, expected, "BITRUE STRING NATIVE {owner} {tag}");
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                value,
                "BITRUE STRING OUTPUT {owner} {tag}"
            );
        }
        let tagged = json!({(*tag): null});
        let tagged_text = serde_json::to_string(&tagged).unwrap();
        assert!(
            serde_json::from_value::<T>(tagged.clone()).is_err(),
            "BITRUE STRING SHAPE {owner} {tag} owned tagged map"
        );
        assert!(
            T::deserialize(&tagged).is_err(),
            "BITRUE STRING SHAPE {owner} {tag} borrowed tagged map"
        );
        let mut decoder = serde_json::Deserializer::from_str(&tagged_text);
        assert!(
            <T as Deserialize>::deserialize(&mut decoder).is_err(),
            "BITRUE STRING SHAPE {owner} {tag} qualified tagged map"
        );
        assert!(
            serde_json::from_str::<T>(&tagged_text).is_err(),
            "BITRUE STRING SHAPE {owner} {tag} text tagged map"
        );
    }
    let mut refused = vec![
        json!(null),
        json!(false),
        json!(1),
        json!([]),
        json!([cases[0].0]),
        json!({}),
        json!("unknown"),
        json!(cases[0].0.to_lowercase()),
        json!(format!(" {}", cases[0].0)),
        json!(format!("{} ", cases[0].0)),
        json!(format!("\u{feff}{}", cases[0].0)),
    ];
    for (tag, _) in cases {
        refused.push(json!({(*tag): false}));
    }
    for value in refused {
        let text = serde_json::to_string(&value).unwrap();
        assert!(
            serde_json::from_value::<T>(value.clone()).is_err(),
            "BITRUE STRING REFUSAL {owner} owned {value}"
        );
        assert!(
            T::deserialize(&value).is_err(),
            "BITRUE STRING REFUSAL {owner} borrowed {value}"
        );
        let mut decoder = serde_json::Deserializer::from_str(&text);
        assert!(
            <T as Deserialize>::deserialize(&mut decoder).is_err(),
            "BITRUE STRING REFUSAL {owner} qualified {value}"
        );
        assert!(
            serde_json::from_str::<T>(&text).is_err(),
            "BITRUE STRING REFUSAL {owner} text {value}"
        );
    }
}

#[test]
fn bitrue_order_type_accepts_only_exact_string_literals() {
    string_boundary(
        &[
            ("LIMIT", BitrueOrderType::Limit),
            ("MARKET", BitrueOrderType::Market),
            ("STOP_MARKET", BitrueOrderType::StopMarket),
            ("TAKE_PROFIT_MARKET", BitrueOrderType::TakeProfitMarket),
        ],
        "BitrueOrderType",
    );
}

#[test]
fn bitrue_order_side_accepts_only_exact_string_literals() {
    string_boundary(
        &[
            ("BUY", FetchOrdersResponseDataItemSide::Buy),
            ("SELL", FetchOrdersResponseDataItemSide::Sell),
        ],
        "FetchOrdersResponseDataItemSide",
    );
}
