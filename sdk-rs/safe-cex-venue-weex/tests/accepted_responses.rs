//! Native accepted-domain observations before any owner serialization.
use safe_cex_venue_weex::response_types::*;
use serde::Deserialize;

fn qualified<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, serde_json::Error> {
    <T as Deserialize>::deserialize(&mut serde_json::Deserializer::from_str(text))
}

#[test]
fn order_result_object_shape_and_native_fields() {
    let raw = r#"{"orderId":"order-alpha","clientOrderId":"client-beta","success":false,"errorCode":"E7","errorMessage":"denied"}"#;
    for value in [
        serde_json::from_str::<WeexOrderResult>(raw).unwrap(),
        qualified::<WeexOrderResult>(raw).unwrap(),
    ] {
        assert_eq!(value.order_id.as_deref(), Some("order-alpha"));
        assert_eq!(value.client_order_id.as_deref(), Some("client-beta"));
        assert_eq!(value.success, Some(false));
        assert_eq!(value.error_code.as_deref(), Some("E7"));
        assert_eq!(value.error_message.as_deref(), Some("denied"));
    }
    let missing: WeexOrderResult = serde_json::from_str("{}").unwrap();
    assert_eq!(
        (
            missing.order_id,
            missing.client_order_id,
            missing.success,
            missing.error_code,
            missing.error_message
        ),
        (None, None, None, None, None)
    );
    let partial: WeexOrderResult =
        serde_json::from_str(r#"{"clientOrderId":"second-client","success":true,"ignored":8}"#)
            .unwrap();
    assert!(partial.order_id.is_none());
    assert_eq!(partial.client_order_id.as_deref(), Some("second-client"));
    assert_eq!(partial.success, Some(true));
    for raw in [
        r#"["order-alpha","client-beta",false,"E7","denied"]"#,
        "[]",
        "null",
        "1",
        "true",
        r#"{"orderId":null}"#,
        r#"{"success":"false"}"#,
    ] {
        assert!(
            serde_json::from_str::<WeexOrderResult>(raw).is_err(),
            "accepted_order_result::object_guard {raw}"
        );
        assert!(
            qualified::<WeexOrderResult>(raw).is_err(),
            "accepted_order_result::qualified_guard {raw}"
        );
    }
}

#[test]
fn exchange_info_object_shape_and_native_records() {
    let raw = r#"{"assets":[{"negativeZero":-0,"wide":9007199254740993}],"symbols":[[-3.25]]}"#;
    for value in [
        serde_json::from_str::<WeexExchangeInfoResponse>(raw).unwrap(),
        qualified::<WeexExchangeInfoResponse>(raw).unwrap(),
    ] {
        let assets = value.assets.unwrap();
        assert_eq!(assets.len(), 1);
        let WeexExchangeInfoRecord::Object(record) = &assets[0] else {
            panic!("native asset record")
        };
        assert_eq!(
            record["negativeZero"]
                .clone()
                .decode::<f64>()
                .unwrap()
                .to_bits(),
            0x8000000000000000
        );
        assert_eq!(
            record["wide"].clone().decode::<f64>().unwrap().to_bits(),
            0x4340000000000000
        );
        let symbols = value.symbols.unwrap();
        assert_eq!(symbols.len(), 1);
        let WeexExchangeInfoRecord::Array(record) = &symbols[0] else {
            panic!("native symbol array")
        };
        assert_eq!(
            record[0].clone().decode::<f64>().unwrap().to_bits(),
            (-3.25f64).to_bits()
        );
    }
    let missing: WeexExchangeInfoResponse = serde_json::from_str("{}").unwrap();
    assert!(missing.assets.is_none() && missing.symbols.is_none());
    let empty: WeexExchangeInfoResponse =
        serde_json::from_str(r#"{"assets":[],"symbols":[]}"#).unwrap();
    assert!(empty.assets.unwrap().is_empty() && empty.symbols.unwrap().is_empty());
    for raw in [
        r#"[[{"asset":"USD"}],[["ticker"]]]"#,
        "[]",
        "null",
        "17",
        r#"{"assets":null}"#,
        r#"{"symbols":{}}"#,
    ] {
        assert!(
            serde_json::from_str::<WeexExchangeInfoResponse>(raw).is_err(),
            "accepted_exchange_info::object_guard {raw}"
        );
        assert!(
            qualified::<WeexExchangeInfoResponse>(raw).is_err(),
            "accepted_exchange_info::qualified_guard {raw}"
        );
    }
}

#[test]
fn record_union_retains_native_shape_cardinality_and_number_bits() {
    for value in [
        serde_json::from_str::<WeexExchangeInfoRecord>(r#"{"alpha":-0}"#).unwrap(),
        qualified::<WeexExchangeInfoRecord>(r#"{"alpha":-0}"#).unwrap(),
    ] {
        let WeexExchangeInfoRecord::Object(record) = value else {
            panic!("native object variant")
        };
        assert_eq!(record.len(), 1);
        assert_eq!(
            record["alpha"].clone().decode::<f64>().unwrap().to_bits(),
            0x8000000000000000
        );
    }
    let value: WeexExchangeInfoRecord = serde_json::from_str("[-3.25]").unwrap();
    let WeexExchangeInfoRecord::Array(record) = value else {
        panic!("native array variant")
    };
    assert_eq!(record.len(), 1);
    assert_eq!(
        record[0].clone().decode::<f64>().unwrap().to_bits(),
        (-3.25f64).to_bits()
    );
    assert!(
        matches!(serde_json::from_str::<WeexExchangeInfoRecord>("{}").unwrap(),WeexExchangeInfoRecord::Object(x) if x.is_empty())
    );
    assert!(
        matches!(serde_json::from_str::<WeexExchangeInfoRecord>("[]").unwrap(),WeexExchangeInfoRecord::Array(x) if x.is_empty())
    );
    for raw in ["null", "true", "17", r#""text""#] {
        assert!(qualified::<WeexExchangeInfoRecord>(raw).is_err());
    }
}

#[test]
fn number_or_string_has_native_variants_and_preserves_binary64() {
    for (raw, bits) in [
        ("-0", 0x8000000000000000),
        ("9007199254740993", 0x4340000000000000),
        ("1e400", f64::INFINITY.to_bits()),
        ("-3.25", (-3.25f64).to_bits()),
    ] {
        for value in [
            serde_json::from_str::<WeexNumberOrString>(raw).unwrap(),
            qualified::<WeexNumberOrString>(raw).unwrap(),
        ] {
            let WeexNumberOrString::Number(number) = value else {
                panic!("native Number variant")
            };
            assert_eq!(number.to_bits(), bits);
        }
    }
    for raw in [r#""alpha""#, r#""9007199254740993""#] {
        let WeexNumberOrString::String(value) = qualified::<WeexNumberOrString>(raw).unwrap()
        else {
            panic!("native String variant")
        };
        assert_eq!(value, raw.trim_matches('"'));
    }
    for raw in ["null", "true", "[]", "{}"] {
        assert!(qualified::<WeexNumberOrString>(raw).is_err());
    }
}
