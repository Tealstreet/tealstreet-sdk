//! API helpers at the source Array/JSON/String and property-read boundaries.

/// Some represents exactly `typeof value === 'object' && value !== null`.
/// None is the comparison-equivalent class rejected by that source guard.
pub fn is_record<T, E>(
    non_null_object: Option<&T>,
    array_is_array: impl FnOnce(&T) -> Result<bool, E>,
) -> Result<bool, E> {
    match non_null_object {
        Some(value) => Ok(!array_is_array(value)?),
        None => Ok(false),
    }
}

pub fn parse_json_object<T, E>(
    value: &[u16],
    json_parse: impl FnOnce(&[u16]) -> Result<T, E>,
    record: impl FnOnce(&T) -> Result<bool, E>,
) -> Option<T> {
    let parsed = json_parse(value).ok()?;
    if record(&parsed).ok()? {
        Some(parsed)
    } else {
        None
    }
}

pub fn is_cryptocom_authentication_error_code<T, E>(
    code: T,
    string: impl FnOnce(T) -> Result<Vec<u16>, E>,
) -> Result<bool, E> {
    let code = string(code)?;
    Ok(code.iter().copied().eq("40101".encode_utf16()))
}

/// Some is a primitive JS string; all other kinds share the false branch.
pub fn is_cryptocom_authentication_failure_message(message: Option<&[u16]>) -> bool {
    let needle: Vec<_> = "Crypto.com API Error 40101".encode_utf16().collect();
    message.is_some_and(|message| message.windows(needle.len()).any(|part| part == needle))
}

pub fn should_suppress_validation_auth_error_emission<T, E>(
    code: T,
    string: impl FnOnce(T) -> Result<Vec<u16>, E>,
    read_suppress_flag: impl FnOnce() -> Result<Option<bool>, E>,
) -> Result<bool, E> {
    if !is_cryptocom_authentication_error_code(code, string)? {
        return Ok(false);
    }
    // None represents non-boolean values; strict equality accepts only true.
    Ok(read_suppress_flag()? == Some(true))
}
