//! Canonical retained JSON admission, observed before any Value projection.
use safe_cex_types::retained_json::RetainedJsonInput as Input;
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn retained_json_admits_valid_lone_surrogate_values() {
    let result = serde_json::from_str::<Input>(r#"{"label":"tp \ud800","rows":[1,2]}"#)
        .expect("valid source JSON must enter the retained carrier without UTF-8 projection");
    let Input::Object(fields) = &result else {
        panic!("object")
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(
        fields[0],
        (units("label"), Input::Text(vec![116, 112, 32, 0xd800]))
    );
    assert_eq!(
        fields[1],
        (
            units("rows"),
            Input::Array(vec![Input::Lexeme("1".into()), Input::Lexeme("2".into())])
        )
    );
    assert!(
        result.into_value().is_err(),
        "typed Value cannot represent a lone unit"
    );
}
#[test]
fn retained_json_admits_valid_lone_surrogate_keys() {
    let result = serde_json::from_str::<Input>(r#"{"\udfff":"kept","rows":[1,2]}"#)
        .expect("valid source UTF-16 own key must enter the retained carrier");
    let Input::Object(fields) = &result else {
        panic!("object")
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0], (vec![0xdfff], Input::Text(units("kept"))));
    assert_eq!(
        fields[1],
        (
            units("rows"),
            Input::Array(vec![Input::Lexeme("1".into()), Input::Lexeme("2".into())])
        )
    );
    assert!(
        result.into_value().is_err(),
        "typed Value must refuse, never replace a key"
    );
}
#[test]
fn native_parse_keeps_number_bits_and_lexical_capture_keeps_spelling() {
    let text = r#"[-0,1e400,1e-400,9007199254740993]"#;
    let Input::Array(native) = Input::parse_json(&units(text)).unwrap() else {
        panic!("array")
    };
    let expected = [-0.0, f64::INFINITY, 0.0, 9007199254740992.0];
    assert_eq!(native.len(), expected.len());
    for (value, expected) in native.iter().zip(expected) {
        let Input::Float(value) = value else {
            panic!("native binary64")
        };
        assert_eq!(value.to_bits(), expected.to_bits());
    }
    assert_eq!(
        serde_json::from_str::<Input>(text).unwrap(),
        Input::Array(
            ["-0", "1e400", "1e-400", "9007199254740993"]
                .map(|v| Input::Lexeme(v.into()))
                .to_vec()
        )
    );
}
#[test]
fn reserved_keys_are_data_and_duplicates_keep_first_position() {
    let value: Input = serde_json::from_str(r#"{"__proto__":1,"$serde_json::private::Number":"2","__proto__":3,"$serde_json::private::RawValue":"4"}"#).unwrap();
    assert_eq!(
        value,
        Input::Object(vec![
            (units("__proto__"), Input::Lexeme("3".into())),
            (
                units("$serde_json::private::Number"),
                Input::Text(units("2"))
            ),
            (
                units("$serde_json::private::RawValue"),
                Input::Text(units("4"))
            ),
        ])
    );
}
#[test]
fn native_parser_preserves_raw_and_escaped_units_without_typed_depth_budget() {
    for unit in 0u16..=u16::MAX {
        let escaped = format!("\"\\u{unit:04x}\"");
        assert_eq!(
            Input::parse_json(&units(&escaped)).unwrap(),
            Input::Text(vec![unit])
        );
        if unit >= 32 && unit != 34 && unit != 92 {
            assert_eq!(
                Input::parse_json(&[34, unit, 34]).unwrap(),
                Input::Text(vec![unit])
            );
        }
    }
    let text = format!("{}0{}", "[".repeat(140), "]".repeat(140));
    assert!(Input::parse_json(&units(&text)).is_ok());
    assert!(serde_json::from_str::<Input>(&text).is_err());
}
