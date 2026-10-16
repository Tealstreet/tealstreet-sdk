use safe_cex_venue_bitget::account_predicates::ApiCode;
use serde::Deserialize;

fn number(value: ApiCode) -> u64 {
    match value {
        ApiCode::Number(value) => value.to_bits(),
        ApiCode::Text(_) => panic!("numeric API code changed variant"),
    }
}

#[test]
fn api_code_direct_decimal_and_signed_zero_keep_native_number_bits() {
    // Independent binary64 expectations, before any owner serialization.
    for (raw, expected) in [
        ("40085", 0x40e3_92a0_0000_0000),
        ("1.25", 0x3ff4_0000_0000_0000),
        ("-0", 0x8000_0000_0000_0000),
        ("-0.0", 0x8000_0000_0000_0000),
        ("9007199254740993", 0x4340_0000_0000_0000),
        ("1e400", 0x7ff0_0000_0000_0000),
        ("-1e400", 0xfff0_0000_0000_0000),
        ("-1e-400", 0x8000_0000_0000_0000),
    ] {
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        assert_eq!(
            number(serde_json::from_str(raw).unwrap()),
            expected,
            "direct {raw}"
        );
        assert_eq!(
            number(serde_json::from_value(value.clone()).unwrap()),
            expected,
            "owned {raw}"
        );
        assert_eq!(
            number(ApiCode::deserialize(&value).unwrap()),
            expected,
            "borrowed {raw}"
        );
        let mut decoder = serde_json::Deserializer::from_str(raw);
        assert_eq!(
            number(ApiCode::deserialize(&mut decoder).unwrap()),
            expected,
            "qualified {raw}"
        );
    }
}

#[test]
fn api_code_keeps_text_identity_and_refuses_other_json_shapes() {
    for raw in ["\"40085\"", "\"synthetic-denial\""] {
        let expected: String = serde_json::from_str(raw).unwrap();
        let value: serde_json::Value = serde_json::from_str(raw).unwrap();
        for actual in [
            serde_json::from_str::<ApiCode>(raw).unwrap(),
            serde_json::from_value::<ApiCode>(value.clone()).unwrap(),
            ApiCode::deserialize(&value).unwrap(),
        ] {
            let ApiCode::Text(actual) = actual else {
                panic!("text API code changed variant")
            };
            assert_eq!(actual, expected);
        }
    }
    for raw in [
        "null",
        "true",
        "false",
        "[]",
        "{}",
        "[1.25]",
        "{\"Number\":1.25}",
        "{\"Text\":\"40085\"}",
        "{\"$serde_json::private::Number\":\"1.25\"}",
    ] {
        // Value's parser can itself promote this reserved object into a Number.
        // Construct the object explicitly when testing an owned/borrowed map.
        let value = if raw.contains("$serde_json::private::Number") {
            serde_json::json!({"$serde_json::private::Number":"1.25"})
        } else {
            serde_json::from_str(raw).unwrap()
        };
        assert!(
            serde_json::from_str::<ApiCode>(raw).is_err(),
            "direct shape {raw}"
        );
        assert!(
            serde_json::from_value::<ApiCode>(value.clone()).is_err(),
            "owned shape {raw}"
        );
        assert!(
            ApiCode::deserialize(&value).is_err(),
            "borrowed shape {raw}"
        );
    }
}
