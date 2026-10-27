use crate::JsCoercionError;

/// String.prototype.replace with a string needle and replacement, preserving UTF-16 units.
pub fn js_replace_utf16(text: &[u16], needle: &[u16], replacement: &[u16]) -> Vec<u16> {
    safe_cex_types::js_coercion::js_string_replace_utf16(text, needle, replacement, false)
}

/// String.prototype.replaceAll with a string needle and replacement, including empty needles.
pub fn js_replace_all_utf16(text: &[u16], needle: &[u16], replacement: &[u16]) -> Vec<u16> {
    safe_cex_types::js_coercion::js_string_replace_utf16(text, needle, replacement, true)
}

/// First string match with JavaScript dollar-token expansion, not literal Rust replacen.
pub fn js_replace(text: &str, needle: &str, replacement: &str) -> String {
    let result = js_replace_utf16(
        &text.encode_utf16().collect::<Vec<_>>(),
        &needle.encode_utf16().collect::<Vec<_>>(),
        &replacement.encode_utf16().collect::<Vec<_>>(),
    );
    // Valid strings and a first match never split a surrogate pair; an empty needle matches at zero.
    String::from_utf16(&result).expect("first string replacement preserves well-formed Unicode")
}

/// All string matches with JavaScript dollar-token expansion.
/// Empty needles may split surrogate pairs; report a representation refusal, never replace units.
pub fn js_replace_all(
    text: &str,
    needle: &str,
    replacement: &str,
) -> Result<String, JsCoercionError> {
    let result = js_replace_all_utf16(
        &text.encode_utf16().collect::<Vec<_>>(),
        &needle.encode_utf16().collect::<Vec<_>>(),
        &replacement.encode_utf16().collect::<Vec<_>>(),
    );
    String::from_utf16(&result).map_err(|_| JsCoercionError {
        name: "RepresentationError",
        message: "JavaScript replacement result contains lone UTF-16 surrogates; use js_replace_all_utf16",
    })
}
