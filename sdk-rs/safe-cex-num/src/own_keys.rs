//! OrdinaryOwnPropertyKeys for string keys; JSON has no Symbol keys.
// Lowest-layer backend is shared with typed Balance dictionaries; types cannot depend on num.
pub fn js_array_index(key: &str) -> Option<u32> {
    safe_cex_types::js_coercion::js_array_index(key)
}
pub fn js_own_keys<'a>(keys: impl IntoIterator<Item = &'a str>) -> Vec<&'a str> {
    safe_cex_types::js_coercion::js_own_keys(keys)
}
pub fn normalize_object_order(value: &mut serde_json::Map<String, serde_json::Value>) {
    safe_cex_types::js_coercion::normalize_object_order(value);
}
pub fn normalize_json_order(value: &mut serde_json::Value) {
    safe_cex_types::js_coercion::normalize_json_order(value);
}

/// Read a JSON object's member or a JSON array's element by its string property key.
/// This is a data-entry lookup, excluding prototypes, array length and boxed scalar properties.
pub fn js_object_or_array_entry<'a>(
    value: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    match value {
        serde_json::Value::Object(object) => object.get(key),
        serde_json::Value::Array(array) => {
            js_array_index(key).and_then(|index| array.get(index as usize))
        }
        _ => None,
    }
}
