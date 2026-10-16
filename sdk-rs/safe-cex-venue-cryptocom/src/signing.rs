//! Signature and authentication helpers from cryptocom.signing.ts.
use crate::signing_value::SigningValue;
use safe_cex_num::{JsCoercionError, js_string};
use serde_json::Value;

/// JSON data with stock prototypes; None represents source undefined.
/// Callers retain numeric tokens through shared numeric ingress before this boundary.
/// Omitted level is zero, while explicit NaN/infinity preserve the source comparison.
pub fn params_to_string(
    value: Option<&Value>,
    level: Option<f64>,
) -> Result<String, JsCoercionError> {
    let level = level.unwrap_or(0.0);
    if level >= 3.0 {
        return js_string(value);
    }
    match value {
        Some(Value::Null) => Ok("null".into()),
        Some(Value::Array(values)) => {
            let mut output = String::new();
            for item in values {
                output.push_str(&params_to_string(Some(item), Some(level + 1.0))?);
            }
            Ok(output)
        }
        Some(Value::Object(values)) => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            let mut output = String::new();
            for key in keys {
                output.push_str(key);
                output.push_str(&params_to_string(values.get(key), Some(level + 1.0))?);
            }
            Ok(output)
        }
        _ => js_string(value),
    }
}

/// Build the source preimage and compute shared HMAC synchronously at invocation, like the source.
pub fn generate_signature(
    method: &str,
    id: f64,
    api_key: &str,
    api_secret: &str,
    params: &Value,
    nonce: f64,
) -> Result<String, safe_cex_exchange::runtime::ExchangeError> {
    let param_string = params_to_string(Some(params), None).map_err(coercion_error)?;
    Ok(signature_from_params(
        method,
        id,
        api_key,
        api_secret,
        &param_string,
        nonce,
    ))
}

fn coercion_error(error: JsCoercionError) -> safe_cex_exchange::runtime::ExchangeError {
    safe_cex_exchange::runtime::ExchangeError {
        name: error.name.into(),
        ..safe_cex_exchange::runtime::ExchangeError::new(error.message)
    }
}

fn signature_from_params(
    method: &str,
    id: f64,
    api_key: &str,
    api_secret: &str,
    param_string: &str,
    nonce: f64,
) -> String {
    use safe_cex_crypto::{Encoding, hmac_sha256};
    let payload = format!(
        "{method}{}{api_key}{param_string}{}",
        safe_cex_num::js_number_string(id),
        safe_cex_num::js_number_string(nonce)
    );
    hmac_sha256(api_secret.as_bytes(), payload.as_bytes(), Encoding::Hex)
}

/// Request fields capture primitives by value and params retain shared object/array identity.
#[derive(Debug)]
pub struct AuthenticatedRequest {
    pub id: f64,
    pub method: String,
    pub api_key: String,
    pub params: SigningValue,
    pub sig: String,
    pub nonce: f64,
}

pub fn generate_authenticated_request(
    method: &str,
    params: SigningValue,
    api_key: &str,
    api_secret: &str,
    nonce: Option<f64>,
    clock: &safe_cex_store::virtual_clock::VirtualClock,
) -> Result<AuthenticatedRequest, safe_cex_exchange::runtime::ExchangeError> {
    let id = clock.get_current_time_ms();
    let request_nonce = nonce.unwrap_or_else(|| clock.get_current_time_ms());
    let param_string = params.params_to_string(0).map_err(coercion_error)?;
    let sig = signature_from_params(
        method,
        id,
        api_key,
        api_secret,
        &param_string,
        request_nonce,
    );
    Ok(AuthenticatedRequest {
        id,
        method: method.to_owned(),
        api_key: api_key.to_owned(),
        params,
        sig,
        nonce: request_nonce,
    })
}

#[derive(Debug)]
pub struct WsAuthMessage {
    pub id: f64,
    pub method: &'static str,
    pub api_key: String,
    pub sig: String,
    pub nonce: f64,
}

pub fn generate_ws_auth_message(
    api_key: &str,
    api_secret: &str,
    clock: &safe_cex_store::virtual_clock::VirtualClock,
) -> Result<WsAuthMessage, safe_cex_exchange::runtime::ExchangeError> {
    let id = clock.get_current_time_ms();
    let nonce = clock.get_current_time_ms();
    let sig = generate_signature(
        "public/auth",
        id,
        api_key,
        api_secret,
        &serde_json::json!({}),
        nonce,
    )?;
    Ok(WsAuthMessage {
        id,
        method: "public/auth",
        api_key: api_key.to_owned(),
        sig,
        nonce,
    })
}
