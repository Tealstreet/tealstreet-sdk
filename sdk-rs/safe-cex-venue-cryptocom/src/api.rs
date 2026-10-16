//! Endpoint classifiers from cryptocom.api.ts; generic transport/retry bodies are separate.
use crate::types::{ENDPOINTS, PUBLIC_ENDPOINTS};

/// Source `some(url.includes)` matching, including embedded paths and query strings.
pub fn is_public_endpoint(url: &str) -> bool {
    is_public_endpoint_units(&url.encode_utf16().collect::<Vec<_>>())
}

/// Lossless string ABI for the source UTF-16 substring predicate.
pub fn is_public_endpoint_units(url: &[u16]) -> bool {
    PUBLIC_ENDPOINTS.contains_units(url)
}

/// None mirrors the source's omitted/undefined URL default of an empty string.
pub fn is_non_retryable_private_write(url: Option<&str>) -> bool {
    let units = url.map(|value| value.encode_utf16().collect::<Vec<_>>());
    is_non_retryable_private_write_units(units.as_deref())
}

/// Lossless source string input; None is omitted/undefined, not JS null.
pub fn is_non_retryable_private_write_units(url: Option<&[u16]>) -> bool {
    let url = url.unwrap_or(&[]);
    [
        "CREATE_ORDER",
        "ADVANCED_CREATE_ORDER",
        "AMEND_ORDER",
        "ADVANCED_AMEND_ORDER",
        "CANCEL_ORDER",
        "ADVANCED_CANCEL_ORDER",
        "CANCEL_ALL_ORDERS",
        "CLOSE_POSITION",
        "CHANGE_LEVERAGE",
    ]
    .into_iter()
    .any(|key| {
        let endpoint = ENDPOINTS
            .iter()
            .find(|(name, _)| *name == key)
            .expect("source private-write endpoint exists")
            .1;
        let endpoint = endpoint.encode_utf16().collect::<Vec<_>>();
        url.windows(endpoint.len()).any(|part| part == endpoint)
    })
}

/// ts: isCryptocomSignatureRejection, `Number(data?.code) === 40101`.
pub fn is_cryptocom_signature_rejection(
    data: &serde_json::Value,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let code = data.as_object().and_then(|object| object.get("code"));
    Ok(safe_cex_num::js_number(code)? == 40101.0)
}
