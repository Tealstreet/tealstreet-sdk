//! Test-only routes; every caller supplies independent native field assertions.
use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

fn decode<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
    route: usize,
) -> Result<T, serde_json::Error> {
    match route {
        0 => serde_json::from_str(raw),
        1 => serde_json::from_slice(raw.as_bytes()),
        2 => serde_json::from_value(serde_json::from_str::<Value>(raw).unwrap()),
        3 => T::deserialize(&serde_json::from_str::<Value>(raw).unwrap()),
        4 => js_from_str(raw),
        5 => js_from_value(serde_json::from_str::<Value>(raw).unwrap()),
        6 => js_from_value_ref(&serde_json::from_str::<Value>(raw).unwrap()),
        _ => unreachable!(),
    }
}
pub(crate) fn positive<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
    inspect: impl Fn(T),
) {
    for route in 0..7 {
        inspect(decode::<T>(raw, route).unwrap());
    }
}
pub(crate) fn rejects<T: DeserializeOwned + Serialize + JsNumberOutput>(
    complete_array: &str,
    invalid_objects: &[&str],
) {
    for route in 0..7 {
        for raw in [complete_array, "[]", "null", "false", "42", r#""record""#]
            .into_iter()
            .chain(invalid_objects.iter().copied())
        {
            assert!(
                decode::<T>(raw, route).is_err(),
                "complete valid positional array or invalid record accepted: route={route} input={raw}"
            );
        }
    }
}
