//! Test-side categorical observation after the canonical value cutover.
pub fn category<T: serde::de::DeserializeOwned>(
    value: &safe_cex_exchange::js_runtime::value::Js,
) -> T {
    let safe_cex_exchange::js_runtime::value::Js::String(units) = value else {
        panic!("native categorical value must be a JS string")
    };
    let text = String::from_utf16(units).expect("categorical UTF-16 units");
    serde_json::from_value(serde_json::Value::String(text)).expect("declared categorical value")
}
