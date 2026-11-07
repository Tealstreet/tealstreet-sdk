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

#[test]
fn native_payload_stream_preserves_all_units_and_rejects_malformed_units() {
    use safe_cex_types::retained_json::deserialize_native_payload;
    let units: Vec<u16> = (0..=u16::MAX).collect();
    let decoded =
        deserialize_native_payload(Input::Text(units.clone()).into_native_payload()).unwrap();
    assert_eq!(decoded, Input::Text(units.clone()));
    let keyed = Input::Object(vec![(units.clone(), Input::Text(units))]);
    let decoded = deserialize_native_payload(keyed.clone().into_native_payload()).unwrap();
    assert_eq!(
        decoded, keyed,
        "all UTF-16 keys and values survive packed native capture"
    );
    for (raw, expected) in [
        (r#"["Text",[0,65535,3.0,-0.0]]"#, Some(vec![0, 65535, 3, 0])),
        (r#"[{"Text":null},[0]]"#, None),
        (r#"[3,[0]]"#, None),
        (r#"[false,[0]]"#, None),
        (r#"["Text",[-1]]"#, None),
        (r#"["Text",[65536]]"#, None),
        (r#"["Text",[1.5]]"#, None),
        (r#"["Text",[null]]"#, None),
        (r#"["Text",[true]]"#, None),
        (r#"["Text",["1"]]"#, None),
        (r#"["Text",[1e400]]"#, None),
        (r#"["Text",[1],false]"#, None),
        (r#"["Object",[[[-1],["Null",null]]]]"#, None),
    ] {
        let mut decoder = serde_json::Deserializer::from_str(raw);
        let actual = deserialize_native_payload(&mut decoder);
        match expected {
            Some(units) => assert_eq!(actual.unwrap(), Input::Text(units), "{raw}"),
            None => assert!(actual.is_err(), "invalid payload accepted: {raw}"),
        }
    }
}

#[test]
fn native_payload_stream_retains_numeric_bits_presence_and_serde_wrappers() {
    use safe_cex_types::retained_json::deserialize_native_payload;
    use serde::Deserialize;
    let original = Input::Object(vec![(
        vec![0xd800],
        Input::Array(vec![
            Input::Undefined,
            Input::Float(f64::from_bits(0x7ff8000000000042)),
            Input::Float(-0.0),
            Input::Float(f64::INFINITY),
            Input::Lexeme("-1e400".into()),
        ]),
    )]);
    let decoded = deserialize_native_payload(original.into_native_payload()).unwrap();
    let Input::Object(entries) = decoded else {
        panic!("object")
    };
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, [0xd800]);
    let Input::Array(values) = &entries[0].1 else {
        panic!("array")
    };
    assert_eq!(values.len(), 5);
    assert_eq!(values[0], Input::Undefined);
    for (value, bits) in
        values[1..4]
            .iter()
            .zip([0x7ff8000000000042, 0x8000000000000000, 0x7ff0000000000000])
    {
        let Input::Float(value) = value else {
            panic!("number")
        };
        assert_eq!(value.to_bits(), bits);
    }
    assert_eq!(values[4], Input::Lexeme("-1e400".into()));
    let optional =
        Option::<Vec<serde_json::Value>>::deserialize(Input::Null.into_native_payload()).unwrap();
    assert_eq!(
        optional.unwrap(),
        [serde_json::json!("Null"), serde_json::Value::Null]
    );
    #[derive(Deserialize)]
    struct Wrapped(Vec<serde_json::Value>);
    let wrapped = Wrapped::deserialize(Input::Null.into_native_payload()).unwrap();
    assert_eq!(
        wrapped.0,
        [serde_json::json!("Null"), serde_json::Value::Null]
    );
}

#[test]
fn native_payload_tags_keep_string_and_utf8_byte_admission() {
    use safe_cex_types::retained_json::deserialize_native_payload;
    use serde::Deserializer;
    use serde::de::{self, IntoDeserializer, Visitor};
    enum Atom {
        Text(&'static str),
        Owned(String),
        Bytes(&'static [u8]),
        ByteBuf(Vec<u8>),
        Unit,
    }
    impl<'de> IntoDeserializer<'de, serde_json::Error> for Atom {
        type Deserializer = Self;
        fn into_deserializer(self) -> Self {
            self
        }
    }
    impl<'de> Deserializer<'de> for Atom {
        type Error = serde_json::Error;
        fn deserialize_any<V: Visitor<'de>>(self, v: V) -> Result<V::Value, Self::Error> {
            match self {
                Self::Text(s) => v.visit_str(s),
                Self::Owned(s) => v.visit_string(s),
                Self::Bytes(b) => v.visit_bytes(b),
                Self::ByteBuf(b) => v.visit_byte_buf(b),
                Self::Unit => v.visit_unit(),
            }
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
            option unit unit_struct newtype_struct seq tuple tuple_struct map struct enum identifier ignored_any
        }
    }
    for tag in [
        Atom::Text("Null"),
        Atom::Owned("Null".into()),
        Atom::Bytes(b"Null"),
        Atom::ByteBuf(b"Null".to_vec()),
    ] {
        let decoded = deserialize_native_payload(de::value::SeqDeserializer::new(
            [tag, Atom::Unit].into_iter(),
        ))
        .unwrap();
        assert_eq!(decoded, Input::Null);
    }
    for tag in [
        Atom::Text("unknown"),
        Atom::Bytes(b"unknown"),
        Atom::Bytes(b"\xff"),
        Atom::ByteBuf(vec![0xff]),
    ] {
        assert!(
            deserialize_native_payload(de::value::SeqDeserializer::new(
                [tag, Atom::Unit].into_iter()
            ))
            .is_err()
        );
    }
}

#[test]
fn native_field_identifiers_preserve_map_key_admission_and_utf16_projection() {
    use serde::{
        Deserialize,
        de::{self},
    };
    use std::{collections::BTreeMap, fmt::Debug};
    #[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
    struct Newtype(String);
    #[derive(Deserialize, Debug, PartialEq, Eq, PartialOrd, Ord)]
    enum UnitKey {
        A,
    }
    fn compare<K: de::DeserializeOwned + Ord + Debug>(key: Vec<u16>)
    where
        BTreeMap<K, bool>: PartialEq,
    {
        let expected = BTreeMap::<K, bool>::deserialize(de::value::MapDeserializer::<
            _,
            serde_json::Error,
        >::new(
            [(String::from_utf16_lossy(&key), true)].into_iter(),
        ))
        .map_err(|e| e.to_string());
        let actual = Input::Object(vec![(key, Input::Bool(true))])
            .decode_native::<BTreeMap<K, bool>>(false)
            .map_err(|e| e.to_string());
        assert_eq!(actual, expected);
    }
    for key in [
        units("A"),
        units("1"),
        units("a"),
        units("é"),
        vec![0xd800],
        units(&"x".repeat(65)),
    ] {
        compare::<String>(key.clone());
        compare::<char>(key.clone());
        compare::<bool>(key.clone());
        compare::<u8>(key.clone());
        compare::<Option<String>>(key.clone());
        compare::<Newtype>(key.clone());
        compare::<UnitKey>(key);
    }
    #[derive(Deserialize)]
    struct Record {
        short: f64,
        #[serde(rename = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")]
        bound: f64,
        #[serde(rename = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx")]
        long: f64,
        #[serde(rename = "é")]
        unicode: f64,
    }
    let decoded: Record = Input::Object(vec![
        (units("short"), Input::Float(-0.0)),
        (units(&"x".repeat(64)), Input::Float(64.0)),
        (units(&"x".repeat(65)), Input::Float(65.0)),
        (units("é"), Input::Float(f64::INFINITY)),
        (vec![0xd800], Input::Bool(true)),
    ])
    .decode_native(false)
    .unwrap();
    assert_eq!(decoded.short.to_bits(), (-0.0f64).to_bits());
    assert_eq!(decoded.bound, 64.0);
    assert_eq!(decoded.long, 65.0);
    assert_eq!(decoded.unicode, f64::INFINITY);
}
