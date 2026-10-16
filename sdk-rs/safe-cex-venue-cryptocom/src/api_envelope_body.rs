//! Envelope validation and response stringification at native host boundaries.
use crate::api_guards::is_record;

/// Complete comparison classes used by isAuthenticatedEnvelope. Other contains
/// every remaining typeof/non-null class; those values are rejected uniformly.
#[derive(Debug)]
pub enum EnvelopeValue<T> {
    Other,
    Object(T),
    String(Vec<u16>),
    Number(f64),
}

pub fn is_authenticated_envelope<T, E>(
    value: &EnvelopeValue<T>,
    method: &[u16],
    api_key: &[u16],
    mut array_is_array: impl FnMut(&T) -> Result<bool, E>,
    mut get: impl FnMut(&T, &'static str) -> Result<EnvelopeValue<T>, E>,
) -> Result<bool, E> {
    let EnvelopeValue::Object(value) = value else {
        return Ok(false);
    };
    if !is_record(Some(value), &mut array_is_array)? {
        return Ok(false);
    }
    if !matches!(get(value, "method")?, EnvelopeValue::String(text) if text == method) {
        return Ok(false);
    }
    if !matches!(get(value, "api_key")?, EnvelopeValue::String(text) if text == api_key) {
        return Ok(false);
    }
    let EnvelopeValue::Object(params) = get(value, "params")? else {
        return Ok(false);
    };
    if !is_record(Some(&params), &mut array_is_array)? {
        return Ok(false);
    }
    if !matches!(get(value, "sig")?, EnvelopeValue::String(_)) {
        return Ok(false);
    }
    Ok(matches!(get(value, "nonce")?, EnvelopeValue::Number(_)))
}

/// Primitive strings bypass both builtins. All other values retain their native
/// carrier, including values for which JSON.stringify returns undefined.
#[derive(Debug)]
pub enum ResponseBody<T> {
    String(Vec<u16>),
    Other(T),
}

pub fn stringify_response_body<T, E>(
    body: ResponseBody<T>,
    json_stringify: impl FnOnce(&T) -> Result<Option<Vec<u16>>, E>,
    string: impl FnOnce(&T) -> Result<Vec<u16>, E>,
) -> Result<Option<Vec<u16>>, E> {
    match body {
        ResponseBody::String(body) => Ok(Some(body)),
        ResponseBody::Other(body) => match json_stringify(&body) {
            Ok(value) => Ok(value),
            Err(_) => string(&body).map(Some),
        },
    }
}
