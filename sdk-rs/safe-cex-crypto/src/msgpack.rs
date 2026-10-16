//! MessagePack encoding of JSON as JavaScript's default @msgpack/msgpack encoder.

use serde::ser::{SerializeMap, SerializeSeq};
use serde::{Serialize, Serializer};

/// Encode raw JSON, preserving string-key insertion order and JS Number semantics.
pub fn encode_json(raw: &str) -> Result<Vec<u8>, String> {
    encode_js_value(&json_input(raw)?)
}

fn json_input(raw: &str) -> Result<safe_cex_num::JsJsonValue, String> {
    serde_json::from_str::<safe_cex_types::retained_json::RetainedJsonInput>(raw)
        .and_then(safe_cex_types::retained_json::RetainedJsonInput::into_value)
        .map(Into::into)
        .map_err(|error| error.to_string())
}

/// Encode a native prepared value without a JSON projection of binary64 values.
pub fn encode_js_value(value: &safe_cex_num::JsJsonValue) -> Result<Vec<u8>, String> {
    rmp_serde::to_vec(&JsValue(value, 1)).map_err(|error| error.to_string())
}

struct JsValue<'a>(&'a safe_cex_num::JsJsonValue, usize);

impl Serialize for JsValue<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use safe_cex_num::JsJsonValue as J;
        if self.1 > 100 {
            return Err(serde::ser::Error::custom(format!(
                "Too deep objects in depth {}",
                self.1
            )));
        }
        match self.0 {
            J::Undefined | J::Null => serializer.serialize_unit(),
            J::Bool(value) => serializer.serialize_bool(*value),
            J::String(value) => serializer.serialize_str(value),
            J::Number(number) => {
                if number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0 {
                    if *number >= 0.0 {
                        serializer.serialize_u64(*number as u64)
                    } else {
                        serializer.serialize_i64(*number as i64)
                    }
                } else {
                    serializer.serialize_f64(*number)
                }
            }
            J::Array(values) => {
                let mut sequence = serializer.serialize_seq(Some(values.len()))?;
                for value in values {
                    sequence.serialize_element(&JsValue(value, self.1 + 1))?;
                }
                sequence.end()
            }
            J::Object(values) => {
                let mut keys = Vec::new();
                let mut selected = std::collections::HashMap::new();
                for (key, value) in values {
                    if selected.insert(key.as_str(), value).is_none() {
                        keys.push(key.as_str());
                    }
                }
                let mut map = serializer.serialize_map(Some(selected.len()))?;
                for key in safe_cex_num::js_own_keys(keys) {
                    map.serialize_entry(key, &JsValue(selected[key], self.1 + 1))?;
                }
                map.end()
            }
            J::BigInt(_) => Err(serde::ser::Error::custom(
                "Unrecognized object: [object BigInt]",
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::encode_json;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Case {
        name: String,
        raw: String,
        hex: String,
    }
    #[derive(Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }
    fn corpus() -> Vec<Case> {
        serde_json::from_str::<Corpus>(include_str!("../tests/fixtures/msgpack/vectors.json"))
            .expect("raw JS msgpack corpus")
            .cases
    }
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn js_msgpack_exact_bytes() {
        let cases = corpus();
        for required in [
            "two-key",
            "reversed",
            "signature-last",
            "nested",
            "numeric-boundaries",
            "integer-number",
            "index-keys",
            "overflow",
            "empty",
        ] {
            assert!(
                cases.iter().any(|case| case.name == required),
                "missing {required} msgpack oracle"
            );
        }
        for case in cases {
            assert_eq!(
                hex(&encode_json(&case.raw).unwrap()),
                case.hex,
                "MessagePack exact wire bytes: {}",
                case.name
            );
        }
    }

    #[test]
    fn two_key_body_preserves_insertion_order_and_signature_last() {
        let body = r#"{"z":1,"a":2}"#;
        let mut value: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            body,
            "signed body must not re-key"
        );
        value
            .as_object_mut()
            .unwrap()
            .insert("signature".into(), "fixture".into());
        let wire = serde_json::to_string(&value).unwrap();
        assert_eq!(
            wire, r#"{"z":1,"a":2,"signature":"fixture"}"#,
            "signature must be last on wire"
        );
        let case = corpus()
            .into_iter()
            .find(|case| case.name == "signature-last")
            .unwrap();
        assert_eq!(
            hex(&encode_json(&wire).unwrap()),
            case.hex,
            "wire encoding must match signed order"
        );
    }

    #[test]
    fn malformed_raw_input_is_error() {
        assert!(encode_json("{").is_err());
    }
    #[test]
    fn native_raw_messagepack_input_retains_reserved_object_before_encoding() {
        let raw = r#"{"$serde_json::private::Number":"120"}"#;
        let native = super::json_input(raw).unwrap();
        let safe_cex_num::JsJsonValue::Object(fields) = native else {
            panic!("MSGPACK native raw input must retain literal marker object before encoding");
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].0, "$serde_json::private::Number");
        assert_eq!(
            fields[0].1,
            safe_cex_num::JsJsonValue::String("120".into()),
            "MSGPACK native marker member"
        );
        assert_eq!(
            hex(&encode_json(raw).unwrap()),
            "81bc2473657264655f6a736f6e3a3a707269766174653a3a4e756d626572a3313230",
            "MSGPACK source literal object bytes"
        );
    }
}
