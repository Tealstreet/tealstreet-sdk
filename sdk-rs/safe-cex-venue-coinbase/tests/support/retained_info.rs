use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::Value;

pub fn assert_info(actual: Option<&Input>, expected: &Value, label: &str) {
    if actual.is_none_or(|v| matches!(v, Input::Undefined)) {
        assert_eq!(
            expected.get("$undefined").and_then(Value::as_bool),
            Some(true),
            "{label}: undefined"
        );
        return;
    }
    let actual = actual.unwrap();
    match actual {
        Input::Float(_) | Input::Lexeme(_) => {
            let number = match actual {
                Input::Float(value) => *value,
                Input::Lexeme(value) => value.parse::<f64>().unwrap(),
                _ => unreachable!(),
            };
            let expected = expected
                .get("$bits")
                .and_then(Value::as_str)
                .map(|bits| f64::from_bits(u64::from_str_radix(bits, 16).unwrap()))
                .unwrap_or_else(|| expected.as_f64().unwrap());
            assert!(
                number.is_nan() && expected.is_nan() || number.to_bits() == expected.to_bits(),
                "{label}: native number {number:?} vs {expected:?}"
            );
        }
        Input::Text(units) if expected.get("$utf16").is_some() => {
            let expected: Vec<u16> = serde_json::from_value(expected["$utf16"].clone()).unwrap();
            assert_eq!(units, &expected, "{label}: native UTF16");
        }
        Input::Array(values) => {
            let expected = expected.as_array().unwrap();
            assert_eq!(values.len(), expected.len(), "{label}: array count");
            for (value, expected) in values.iter().zip(expected) {
                assert_info(Some(value), expected, label);
            }
        }
        Input::Object(fields) => {
            if let Some(entries) = expected.get("$entries") {
                let entries = entries.as_array().unwrap();
                assert_eq!(
                    fields.len(),
                    entries.len(),
                    "{label}: own UTF16 field count"
                );
                for entry in entries {
                    let key: Vec<u16> = serde_json::from_value(entry[0].clone()).unwrap();
                    let value = fields
                        .iter()
                        .find(|(name, _)| name == &key)
                        .map(|(_, value)| value);
                    assert_info(value, &entry[1], label);
                }
                return;
            }
            let expected = expected.as_object().unwrap();
            assert_eq!(fields.len(), expected.len(), "{label}: own field count");
            for (key, expected) in expected {
                let key: Vec<u16> = key.encode_utf16().collect();
                let value = fields
                    .iter()
                    .find(|(name, _)| name == &key)
                    .map(|(_, value)| value);
                assert_info(value, expected, label);
            }
        }
        _ => assert_eq!(
            actual,
            &Input::from_value(expected.clone()).unwrap(),
            "{label}: native scalar"
        ),
    }
}
