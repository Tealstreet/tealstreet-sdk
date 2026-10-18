use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use serde::{Deserialize, Serialize};

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
fn read_string_property(value: &Js, key: &str) -> Result<Option<String>, Thrown> {
    if value.is_nullish() || value.type_of() != "object" {
        return Ok(None);
    }
    Ok(match value.get(key)? {
        Js::String(text) if !text.is_empty() => Some(String::from_utf16_lossy(&text)),
        _ => None,
    })
}

// ts: serializeWorkerErrorForSentry
pub fn serialize_worker_error_for_sentry(
    error: &Js,
    fallback_message: &str,
    fallback_name: Option<&str>,
) -> Result<SerializableWorkerError, Thrown> {
    Ok(SerializableWorkerError {
        message: read_string_property(error, "message")?
            .unwrap_or_else(|| fallback_message.to_owned()),
        name: read_string_property(error, "name")?
            .unwrap_or_else(|| fallback_name.unwrap_or("Error").to_owned()),
        stack: read_string_property(error, "stack")?,
    })
}

fn present<'de, D: serde::Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
