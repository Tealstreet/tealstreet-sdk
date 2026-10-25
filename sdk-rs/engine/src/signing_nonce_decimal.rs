//! Exact counter nonces use decimal strings independently of clock stamps.
use serde::{Deserialize, Deserializer, Serializer, de::Error};

pub(super) fn serialize<S: Serializer>(
    value: &Option<u64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match value {
        Some(value) => serializer.serialize_str(&value.to_string()),
        None => serializer.serialize_none(),
    }
}

pub(super) fn deserialize<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(D::Error::custom("Invalid signing nonce"));
    }
    value
        .parse()
        .map(Some)
        .map_err(|_| D::Error::custom("Invalid signing nonce"))
}
