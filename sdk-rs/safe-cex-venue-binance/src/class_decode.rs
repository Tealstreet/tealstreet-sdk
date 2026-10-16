//! Decode declared DTOs through the shared native typed-value boundary.
use crate::private_event_value::EventValue;
use serde::de;
pub fn decode<T: de::DeserializeOwned>(value: EventValue) -> Result<T, de::value::Error> {
    safe_cex_exchange::js_runtime::value::to_typed_preserving_undefined(&value)
        .map_err(|error| de::Error::custom(error.message()))
}
