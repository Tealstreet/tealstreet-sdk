use serde::{Deserialize, Serialize};
use serde_json::Value;

// ts: SerializableWorkerError
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SerializableWorkerError {
    pub message: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

#[derive(Deserialize)]
#[serde(remote = "SerializableWorkerError")]
struct SerializableWorkerErrorWire {
    message: String,
    name: String,
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    stack: Option<String>,
}

impl<'de> Deserialize<'de> for SerializableWorkerError {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let record = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        SerializableWorkerErrorWire::deserialize(serde_json::Value::Object(record))
            .map_err(serde::de::Error::custom)
    }
}

// ts: readStringProperty
fn read_string_property<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key)?.as_str().filter(|value| !value.is_empty())
}

// ts: serializeWorkerErrorForSentry
pub fn serialize_worker_error_for_sentry(
    error: &Value,
    fallback_message: &str,
    fallback_name: Option<&str>,
) -> SerializableWorkerError {
    SerializableWorkerError {
        message: read_string_property(error, "message")
            .unwrap_or(fallback_message)
            .to_owned(),
        name: read_string_property(error, "name")
            .unwrap_or(fallback_name.unwrap_or("Error"))
            .to_owned(),
        stack: read_string_property(error, "stack").map(str::to_owned),
    }
}

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
