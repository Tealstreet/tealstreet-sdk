use safe_cex_venue_woo::legacy_fills::*;
use serde::Deserialize;
use serde_json::{Value, json};
#[test]
fn legacy_fills_request_names_and_zero_presence_match_live_source() {
    let cases: Vec<Value> = serde_json::from_str(include_str!("fixtures/fills.json")).unwrap();
    assert_eq!(cases.len(), 30, "WOO-CARDINALITY legacy_fills source total");
    for case in cases {
        let query: WooLegacyFillsQuery = serde_json::from_value(case["input"].clone()).unwrap();
        let actual = serde_json::to_value(build_woo_legacy_fills_params(&query)).unwrap();
        let expected = &case["output"];
        assert_eq!(
            actual.as_object().unwrap().len(),
            expected.as_object().unwrap().len()
        );
        for (key, value) in actual.as_object().unwrap() {
            if value.is_number() {
                assert_eq!(value.as_f64(), expected[key].as_f64(), "{case} {key}");
            } else {
                assert_eq!(value, &expected[key], "{case} {key}");
            }
        }
    }
}
#[test]
fn query_rejects_positional_arrays_and_optional_nulls_on_qualified_path() {
    let full = json!({"symbol":"X","fromMilliseconds":0,"toMilliseconds":1,"limit":2,"page":0});
    assert!(WooLegacyFillsQuery::deserialize(full.clone()).is_ok());
    let _: WooLegacyFillsQuery = Deserialize::deserialize(full.clone()).unwrap();
    assert!(
        WooLegacyFillsQuery::deserialize(json!(["X", 0, 1, 2, 0])).is_err(),
        "WooLegacyFillsQuery positional arrays accepted"
    );
    for key in [
        "symbol",
        "fromMilliseconds",
        "toMilliseconds",
        "limit",
        "page",
    ] {
        let mut input = full.clone();
        input[key] = Value::Null;
        assert!(WooLegacyFillsQuery::deserialize(input).is_err(), "{key}");
        let mut input = full.clone();
        input[key] = json!({"wrong":true});
        assert!(WooLegacyFillsQuery::deserialize(input).is_err(), "{key}");
    }
}

#[test]
fn query_presence_and_types_are_source_derived() {
    let fields: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/fills-schema.json")).unwrap();
    let input: serde_json::Map<String, Value> = fields
        .iter()
        .map(|f| {
            (
                f["name"].as_str().unwrap().into(),
                match f["type"].as_str().unwrap() {
                    "string" => json!("X"),
                    "number" => json!(1),
                    kind => panic!("unhandled source type {kind}"),
                },
            )
        })
        .collect();
    assert!(WooLegacyFillsQuery::deserialize(Value::Object(input.clone())).is_ok());
    for field in fields {
        let mut without = input.clone();
        without.remove(field["name"].as_str().unwrap());
        assert_eq!(
            WooLegacyFillsQuery::deserialize(Value::Object(without)).is_ok(),
            field["optional"].as_bool().unwrap(),
            "{field}"
        );
    }
}

#[test]
fn query_and_params_native_field_identity() {
    let query: WooLegacyFillsQuery = serde_json::from_value(json!({
        "symbol":"FIELD_SYMBOL", "fromMilliseconds":11.25,
        "toMilliseconds":22.5, "limit":33.75, "page":44.125
    }))
    .unwrap();
    assert_eq!(
        query.symbol.as_deref(),
        Some("FIELD_SYMBOL"),
        "query symbol identity"
    );
    assert_eq!(query.from_milliseconds, Some(11.25), "query from identity");
    assert_eq!(query.to_milliseconds, Some(22.5), "query to identity");
    assert_eq!(query.limit, 33.75, "query limit identity");
    assert_eq!(query.page, Some(44.125), "query page identity");
    let params = build_woo_legacy_fills_params(&query);
    assert_eq!(
        params.symbol.as_deref(),
        Some("FIELD_SYMBOL"),
        "params symbol identity"
    );
    assert_eq!(params.start_t, Some(11.25), "params start identity");
    assert_eq!(params.end_t, Some(22.5), "params end identity");
    assert_eq!(params.size, 33.75, "params size identity");
    assert_eq!(params.page, Some(44.125), "params page identity");
}
