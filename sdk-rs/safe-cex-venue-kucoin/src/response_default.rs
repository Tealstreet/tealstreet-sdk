use safe_cex_transport::RestResponseBody;
#[cfg(test)]
use serde_json::Map;
use serde_json::Value;

pub(crate) fn decode(body: &RestResponseBody) -> Result<Value, String> {
    if let RestResponseBody::Transformed(value) = body {
        return value
            .clone()
            .into_value()
            .map_err(|error| error.to_string());
    }
    body.clone()
        .transform_axios_response()
        .map_err(|error| error.to_string())?
        .json_value()
        .map_err(|error| {
            format!(
                "KuCoin account response representation refusal at JSON/UTF-8 boundary: {error}"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn raw(text: &str) -> Value {
        decode(&RestResponseBody::Encoded(text.as_bytes().to_vec())).unwrap()
    }
    #[test]
    fn native_axios_transform_floor() {
        assert_eq!(raw(""), Value::String("".into()));
        assert_eq!(raw("not-json"), Value::String("not-json".into()));
        assert_eq!(
            raw(r#"{"msg":"\ud800",oops}"#),
            Value::String(r#"{"msg":"\ud800",oops}"#.into())
        );
        assert_eq!(raw("null"), Value::Null);
        assert_eq!(raw("[]"), Value::Array(vec![]));
        assert_eq!(raw("{}"), Value::Object(Map::new()));
        assert_eq!(
            raw(r#""quote\"slash\\""#),
            Value::String("quote\"slash\\".into())
        );
        assert_eq!(raw("null true"), Value::String("null true".into()));
        assert_eq!(
            raw("9007199254740993").as_u64(),
            Some(9007199254740992),
            "KUCOIN-STAGING native number rounding"
        );
        assert_eq!(
            raw("-1e-400")
                .as_number()
                .unwrap()
                .to_string()
                .parse::<f64>()
                .unwrap()
                .to_bits(),
            (-0.0f64).to_bits()
        );
        assert_eq!(
            raw("1e400")
                .as_number()
                .unwrap()
                .to_string()
                .parse::<f64>()
                .unwrap()
                .to_bits(),
            f64::INFINITY.to_bits()
        );
        let object = raw(
            r#"{"z":1,"2":2,"01":3,"4294967295":4,"1":5,"z":6,"$serde_json::private::Number":"42"}"#,
        );
        let object = object.as_object().unwrap();
        assert_eq!(
            object.keys().map(String::as_str).collect::<Vec<_>>(),
            [
                "1",
                "2",
                "z",
                "01",
                "4294967295",
                "$serde_json::private::Number"
            ]
        );
        assert_eq!(object["z"].as_u64(), Some(6));
        assert_eq!(object["$serde_json::private::Number"].as_str(), Some("42"));
        let text = format!("{}0{}", "[".repeat(170), "]".repeat(170));
        let deep = raw(&text);
        let mut value = &deep;
        for _ in 0..170 {
            value = &value.as_array().unwrap()[0];
        }
        assert_eq!(value.as_u64(), Some(0), "KUCOIN-STAGING native depth 170");
        assert!(
            decode(&RestResponseBody::Encoded(br#"{"msg":"\ud800"}"#.to_vec()))
                .unwrap_err()
                .contains("representation refusal")
        );
        let quoted = Value::String("{\"code\":\"200000\"}".into());
        assert_eq!(
            decode(&RestResponseBody::Untransformed(quoted.clone())).unwrap()["code"],
            "200000"
        );
        assert_eq!(
            decode(&RestResponseBody::Transformed(quoted.clone())).unwrap(),
            quoted,
            "KUCOIN-STAGING transformed strings stay unchanged"
        );
    }
}
