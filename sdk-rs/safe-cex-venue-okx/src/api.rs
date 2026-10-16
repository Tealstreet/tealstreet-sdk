//! OKX REST policy; signing and transmission keep separate request views.
pub fn is_public_endpoint(url: Option<&str>) -> bool {
    url.is_some_and(|url| url.contains("/public/") || url.contains("/market/"))
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
        let message = if let Some(message) = body
            .get("msg")
            .filter(|value| safe_cex_num::js_truthy(Some(value)))
        {
            safe_cex_num::js_string(Some(message))
        } else {
            safe_cex_num::js_string(code).map(|code| format!("OKX API error (code {code})"))
        }
        .map_err(|error| {
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
