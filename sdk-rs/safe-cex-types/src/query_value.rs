//! Query values are not JSON: undefined and non-finite numbers remain distinct.
//! Rendering belongs to the transport's selected serializer, not this carrier.
//! ```compile_fail
//! let query = safe_cex_types::QueryValue::Number(f64::NAN);
//! let _ = serde_json::to_value(query); // No accidental JSON projection path.
//! ```
//! ```compile_fail
//! let query: safe_cex_types::QueryValue = serde_json::json!("-0").into();
//! ```
use crate::object_map::JsObjectMap;

#[derive(Debug, Clone, PartialEq)]
pub enum QueryValue {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<QueryValue>),
    Object(JsObjectMap<QueryValue>),
}
