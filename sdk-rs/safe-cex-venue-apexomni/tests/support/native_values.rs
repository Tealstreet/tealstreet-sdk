use safe_cex_num::JsJsonValue as J;
use serde_json::Value;

pub fn number(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
pub fn input(value: &Value) -> J {
    match value["kind"].as_str().unwrap() {
        "undefined" => J::Undefined,
        "null" => J::Null,
        "boolean" => J::Bool(value["value"].as_bool().unwrap()),
        "number" => J::Number(number(&value["bits"])),
        "string" => J::String(value["value"].as_str().unwrap().to_owned()),
        "bigint" => J::BigInt(value["value"].as_str().unwrap().parse().unwrap()),
        "array" => J::Array(
            value["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(input)
                .collect(),
        ),
        "object" => J::Object(
            value["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| (v[0].as_str().unwrap().to_owned(), input(&v[1])))
                .collect(),
        ),
        _ => panic!("unclassified source native kind"),
    }
}
pub fn check(actual: &J, expected: &Value, label: &str) {
    let expected = input(expected);
    native_eq(actual, &expected, label);
}
pub fn check_fields(actual: &J, expected: &Value, label: &str) {
    fn order(value: J) -> J {
        match value {
            J::Object(mut fields) => {
                fields.sort_by(|a, b| a.0.cmp(&b.0));
                J::Object(fields.into_iter().map(|(k, v)| (k, order(v))).collect())
            }
            J::Array(values) => J::Array(values.into_iter().map(order).collect()),
            value => value,
        }
    }
    native_eq(&order(actual.clone()), &order(input(expected)), label);
}
pub fn native_eq(actual: &J, expected: &J, label: &str) {
    match (actual, expected) {
        (J::Number(a), J::Number(b)) => assert_eq!(
            if a.is_nan() {
                0x7ff8000000000000
            } else {
                a.to_bits()
            },
            if b.is_nan() {
                0x7ff8000000000000
            } else {
                b.to_bits()
            },
            "{label} native bits"
        ),
        (J::Array(a), J::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{label} native length");
            for (a, b) in a.iter().zip(b) {
                native_eq(a, b, label);
            }
        }
        (J::Object(a), J::Object(b)) => {
            assert_eq!(
                a.iter().map(|(k, _)| k).collect::<Vec<_>>(),
                b.iter().map(|(k, _)| k).collect::<Vec<_>>(),
                "{label} native keys"
            );
            for ((_, a), (_, b)) in a.iter().zip(b) {
                native_eq(a, b, label);
            }
        }
        _ => assert_eq!(actual, expected, "{label} native value"),
    }
}

pub fn retained(value: &safe_cex_types::retained_json::RetainedJsonInput) -> J {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    match value {
        Input::Undefined => J::Undefined,
        Input::Null => J::Null,
        Input::Bool(value) => J::Bool(*value),
        Input::Float(_) | Input::Lexeme(_) => J::Number(value.as_f64().unwrap()),
        Input::Text(units) => {
            J::String(String::from_utf16(units).expect("source native observer UTF8 text"))
        }
        Input::Array(values) => J::Array(values.iter().map(retained).collect()),
        Input::Object(values) => J::Object(
            values
                .iter()
                .map(|(key, value)| {
                    (
                        String::from_utf16(key).expect("source native observer UTF8 key"),
                        retained(value),
                    )
                })
                .collect(),
        ),
    }
}
