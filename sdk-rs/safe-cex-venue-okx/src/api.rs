//! OKX REST policy; signing and transmission keep separate request views.
pub fn is_public_endpoint(url: Option<&str>) -> bool {
    url.is_some_and(|url| url.contains("/public/") || url.contains("/market/"))
}

// ts: isOkxSignatureRejection
pub fn is_okx_signature_rejection(
    data: &serde_json::Value,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let field = |key| data.as_object().and_then(|object| object.get(key));
    if safe_cex_num::js_string(field("code"))? == "50113" {
        return Ok(true);
    }
    let message = match field("msg").filter(|value| !value.is_null()) {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    };
    // A non-unicode /i regex never folds a non-ASCII unit onto an ASCII one.
    Ok(message.to_ascii_lowercase().contains("invalid sign"))
}

/// ts: formatOkxApiErrorMessage. A row-level failure names its own message and code.
pub fn format_okx_api_error_message(
    data: &serde_json::Value,
) -> Result<String, safe_cex_num::JsCoercionError> {
    use serde_json::Value;
    let trimmed = |value: Option<&Value>| match value {
        Some(Value::String(text)) => safe_cex_num::js_trim(text).to_owned(),
        _ => String::new(),
    };
    let first = data.get("data").and_then(Value::as_array).and_then(|rows| {
        rows.iter().find(|row| {
            safe_cex_num::js_truthy(Some(row))
                && ((safe_cex_num::js_truthy(row.get("sCode"))
                    && row.get("sCode") != Some(&Value::String("0".into())))
                    || safe_cex_num::js_truthy(row.get("sMsg")))
        })
    });
    let row_message = trimmed(first.and_then(|row| row.get("sMsg")));
    let row_code = match first.and_then(|row| row.get("sCode")) {
        None => String::new(),
        code => safe_cex_num::js_string(code)?,
    };
    if !row_message.is_empty() {
        return Ok(if !row_code.is_empty() && row_code != "0" {
            format!("{row_message} (code {row_code})")
        } else {
            row_message
        });
    }
    let message = trimmed(data.get("msg"));
    if !message.is_empty() {
        return Ok(message);
    }
    let code = match data.get("code") {
        None => "unknown".to_owned(),
        code => safe_cex_num::js_string(code)?,
    };
    Ok(format!("OKX API error (code {code})"))
}

pub(crate) fn check_response(
    response: safe_cex_transport::RestResponse,
    request: safe_cex_transport::RestRequest,
) -> Result<safe_cex_transport::RestResponse, safe_cex_transport::RestFailure> {
    let body = response
        .body
        .json_value()
        .unwrap_or(serde_json::Value::Null);
    let code = body.get("code");
    if safe_cex_num::js_truthy(code) && code.and_then(serde_json::Value::as_str) != Some("0") {
        let message = format_okx_api_error_message(&body).map_err(|error| {
            let mut failure = safe_cex_transport::RestFailure::pre_transmit(
                error.message.into(),
                request.clone(),
            );
            failure.context = Some(Box::new(serde_json::json!({"name":error.name})));
            failure
        })?;
        return Err(safe_cex_transport::RestFailure {
            code: None,
            message,
            response: Some(Box::new(response)),
            request: Box::new(request),
            transport_error: None,
            context: None,
        });
    }
    Ok(response)
}
