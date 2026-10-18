//! Typed HTTP receipts feed the existing process-local diagnostics adapter.
use crate::js_runtime::value::{Js, Thrown};
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_store::venue_rest_diagnostics::{
    self as diagnostics, DiagnosticHeaders, RestDiagnosticConfig, RestDiagnosticError,
    RestDiagnosticResponse,
};
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_transport::{RestFailure, RestRequest, RestResponse};
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
use safe_cex_types::{ExchangeName, retained_json::RetainedJsonInput as Input};
use serde_json::Value;

#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn exchange_key(exchange: &ExchangeName) -> String {
    serde_json::to_value(exchange)
        .expect("exchange name")
        .as_str()
        .expect("exchange string")
        .into()
}
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn headers(headers: &std::collections::BTreeMap<String, String>) -> DiagnosticHeaders {
    DiagnosticHeaders {
        values: headers
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), Value::String(v.clone())))
            .collect(),
        get: None,
    }
}
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn config(request: &RestRequest) -> RestDiagnosticConfig {
    let mut config = RestDiagnosticConfig {
        url: Some(request.url.clone()),
        base_url: request.base_url.clone(),
        headers: DiagnosticHeaders {
            values: request
                .headers
                .iter()
                .map(|(k, v)| (k.to_ascii_lowercase(), Value::String(v.clone())))
                .collect(),
            get: None,
        },
        ..Default::default()
    };
    for (key, value) in &request.config_extensions {
        if let Input::Float(value) = value {
            config.numbers.insert(key.clone(), *value);
        }
        if key == "safeCexRestDiagProxyHeader"
            && let Input::Text(value) = value
        {
            config.proxy_header = Some(String::from_utf16_lossy(value));
        }
    }
    if let Some(sign_ms) = request.sign_ms {
        config
            .numbers
            .insert("safeCexRestDiagSignMs".into(), sign_ms as f64);
    }
    config
}
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
fn response(
    response: &RestResponse,
    request: &RestRequest,
    include_body: bool,
) -> RestDiagnosticResponse {
    RestDiagnosticResponse {
        config: Some(config(request)),
        status: Some(f64::from(response.status)),
        headers: headers(&response.headers),
        data: include_body
            .then(|| {
                match &response.body {
                    safe_cex_transport::RestResponseBody::Encoded(_) => {
                        response.body.clone().transform_axios_response().ok()
                    }
                    _ => Some(response.body.clone()),
                }
                .and_then(|data| {
                    body(&crate::js_runtime::value::from_retained_input(
                        data.into_data(),
                    ))
                    .ok()
                })
                .flatten()
            })
            .flatten(),
    }
}
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn success(
    exchange: &ExchangeName,
    reply: &RestResponse,
    request: &RestRequest,
    now: f64,
) {
    diagnostics::note_rest_success(
        &exchange_key(exchange),
        Some(&response(reply, request, false)),
        now,
    );
}
#[cfg(all(feature = "native", not(target_arch = "wasm32")))]
pub(crate) fn failure(exchange: &ExchangeName, error: &RestFailure, now: f64) {
    let error = RestDiagnosticError {
        config: Some(config(&error.request)),
        response: error
            .response
            .as_ref()
            .map(|reply| response(reply, &error.request, true)),
        code: error.code.clone(),
        message: Some(error.message.clone()),
        name: error
            .context
            .as_ref()
            .and_then(|context| context.get("name"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| {
                Some(match error.transport_error.as_deref() {
                    Some(safe_cex_transport::TransportError::Rejected(origin)) => {
                        crate::runtime::ExchangeError::from(RestFailure::from_transport(
                            safe_cex_transport::TransportError::Rejected(origin.clone()),
                            (*error.request).clone(),
                        ))
                        .name
                    }
                    Some(safe_cex_transport::TransportError::JavaScript(cause)) => {
                        cause.name.into()
                    }
                    Some(safe_cex_transport::TransportError::Network(_)) => "AxiosError".into(),
                    _ if error.response.is_some() => "AxiosError".into(),
                    _ => "Error".into(),
                })
            }),
    };
    diagnostics::note_rest_failure(&exchange_key(exchange), Some(&error), now);
}

pub(crate) fn body(data: &Js) -> Result<Option<Value>, Thrown> {
    if data.is_undefined() {
        return Ok(None);
    }
    if matches!(data, Js::Null) {
        return Ok(Some(Value::Null));
    }
    if data.type_of() == "object" {
        let mut body = serde_json::Map::new();
        for key in ["retCode", "retMsg", "code"] {
            if crate::js_runtime::library::in_operator(data, key)? {
                body.insert(key.into(), Value::Null);
                break;
            }
        }
        return Ok(Some(Value::Object(body)));
    }
    let text = if matches!(data, Js::SymbolIterator) {
        "Symbol(Symbol.iterator)".into()
    } else {
        String::from_utf16_lossy(&data.to_js_string()?)
    };
    Ok(Some(Value::String(text)))
}
