//! Explicit opaque JSON fields: capture numeric events before Value's visitor.
use serde::Deserializer;
use serde_json::Value;

/// Deserialize a declared opaque JSON field without projecting infinity to null.
/// Pair this with `passthrough { field }` in `js_number_output!` and name both
/// the field and this operation in its parity row. This does not disable the
/// whole-output numeric comparison. NaN is refused, never encoded as JSON.
pub fn js_deserialize_passthrough<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Value, D::Error> {
    safe_cex_types::retained_json::deserialize_retained_json(deserializer)
}

/// Eligibility only for explicitly named opaque fields, never ordinary output
/// eligibility. The retained whole-output guard still checks numeric contents.
pub trait JsNumberPassthrough: private::Sealed {
    /// Check the declared opaque field's supported public representation.
    fn check_number_passthrough(&self);
}
mod private {
    pub trait Sealed {}
    impl Sealed for serde_json::Value {}
    impl Sealed for Option<serde_json::Value> {}
    impl Sealed for safe_cex_types::ModelField<serde_json::Value> {}
}
impl JsNumberPassthrough for Value {
    fn check_number_passthrough(&self) {}
}
impl JsNumberPassthrough for Option<Value> {
    fn check_number_passthrough(&self) {}
}
impl JsNumberPassthrough for safe_cex_types::ModelField<Value> {
    fn check_number_passthrough(&self) {}
}

impl private::Sealed for safe_cex_types::retained_json::RetainedJsonInput {}
impl private::Sealed for Option<safe_cex_types::retained_json::RetainedJsonInput> {}
impl private::Sealed
    for safe_cex_types::ModelField<safe_cex_types::retained_json::RetainedJsonInput>
{
}
impl JsNumberPassthrough for safe_cex_types::retained_json::RetainedJsonInput {
    fn check_number_passthrough(&self) {}
}
impl JsNumberPassthrough for Option<safe_cex_types::retained_json::RetainedJsonInput> {
    fn check_number_passthrough(&self) {}
}
impl JsNumberPassthrough
    for safe_cex_types::ModelField<safe_cex_types::retained_json::RetainedJsonInput>
{
    fn check_number_passthrough(&self) {}
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_nan_passthrough_is_refused_before_json_construction() {
        let error = safe_cex_types::retained_json::RetainedJsonInput::Float(f64::NAN)
            .into_value()
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("numeric passthrough refuses NaN JSON")
        );
    }
}
