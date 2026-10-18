use safe_cex_transport::RestRequest;
use safe_cex_types::QueryValue;
use serde_json::{Value, json};

pub fn observe(value: &QueryValue) -> Value {
    match value {
        QueryValue::Undefined => json!({"$undefined": true}),
        QueryValue::Null => Value::Null,
        QueryValue::Bool(value) => json!(value),
        QueryValue::Number(value) => json!({"bits": format!("{:016x}", value.to_bits())}),
        QueryValue::String(value) => json!(value),
        QueryValue::Array(values) => Value::Array(values.iter().map(observe).collect()),
        QueryValue::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), observe(value)))
                .collect(),
        ),
    }
}

pub fn params(request: &RestRequest) -> Value {
    assert!(
        request.params.is_none(),
        "history uses only native query parameters"
    );
    assert!(
        request.query_profile.is_some(),
        "history owns its serializer profile"
    );
    observe(
        request
            .query_params
            .as_ref()
            .expect("history query parameters"),
    )
}
