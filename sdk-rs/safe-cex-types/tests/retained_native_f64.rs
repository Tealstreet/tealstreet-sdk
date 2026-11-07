use safe_cex_types::retained_json::{
    RetainedJsonInput, deserialize_present_retained_json, deserialize_retained_json,
};
use serde::{Deserialize, de::value::F64Deserializer};

fn input(value: f64) -> F64Deserializer<serde_json::Error> {
    F64Deserializer::new(value)
}

#[test]
fn stock_f64_events_retain_native_bits_before_json_projection() {
    let bits = [
        0x0000_0000_0000_0000,
        0x8000_0000_0000_0000,
        0x3ff4_0000_0000_0000,
        0xc004_0000_0000_0000,
        0x0000_0000_0000_0001,
        0x7fef_ffff_ffff_ffff,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_002a,
    ];
    for expected in bits {
        let value = f64::from_bits(expected);
        let captured = RetainedJsonInput::deserialize(input(value))
            .expect("stock F64Deserializer must reach retained numeric capture");
        let RetainedJsonInput::Float(native) = captured else {
            panic!("native floating-point events must retain the Float variant");
        };
        assert_eq!(native.to_bits(), expected, "capture preserves event bits");

        for projected in [
            deserialize_retained_json(input(value)),
            deserialize_present_retained_json(input(value)).map(Option::unwrap),
        ] {
            if value.is_nan() {
                assert!(
                    projected
                        .unwrap_err()
                        .to_string()
                        .contains("refuses NaN JSON"),
                    "NaN is refused at JSON projection, after capture"
                );
            } else {
                let number = projected.unwrap();
                let native = number.as_number().expect("JSON number, never null");
                assert_eq!(
                    native.to_string().parse::<f64>().unwrap().to_bits(),
                    expected,
                    "JSON projection preserves finite, signed-zero and overflow bits"
                );
            }
        }
    }
}

#[test]
fn native_event_admission_keeps_raw_token_map_protections() {
    for raw in [
        r#"{"$serde_json::private::Number":"1e400"}"#,
        r#"{"$serde_json::private::RawValue":"-0"}"#,
    ] {
        let captured: RetainedJsonInput = serde_json::from_str(raw).unwrap();
        let RetainedJsonInput::Object(entries) = &captured else {
            panic!("reserved-looking literal JSON must remain an object");
        };
        assert_eq!(entries.len(), 1);
        assert!(matches!(entries[0].1, RetainedJsonInput::Text(_)));
        assert_eq!(captured.into_value().unwrap().to_string(), raw);
    }

    for entries in [
        vec![],
        vec![("wrong", "-0")],
        vec![("$serde_json::private::RawValue", "-0"), ("extra", "1")],
    ] {
        let map =
            serde::de::value::MapDeserializer::<_, serde_json::Error>::new(entries.into_iter());
        assert!(RetainedJsonInput::deserialize(map).is_err());
    }
}
