//! Account-mode response decisions, projected from the exchange receiver.
use safe_cex_num::js_number_string;
use serde::Deserialize;
use std::cell::Cell;

// ts: BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE
pub const BITGET_UTA_VARIANT_UNSUPPORTED_MESSAGE: &str = "Bitget UTA routing is available for USDT-M and USDC-M only; this market variant is not enabled for UTA yet.";

/// The source payload's declared string-or-number code.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ApiCode {
    Text(String),
    Number(f64),
}

// ts: isBitgetUtaClassicUnsupported
pub fn is_bitget_uta_classic_unsupported(code: Option<&ApiCode>, message: Option<&str>) -> bool {
    let code = match code {
        Some(ApiCode::Text(value)) => value.clone(),
        Some(ApiCode::Number(value)) => js_number_string(*value),
        None => String::new(),
    };
    let message = message.unwrap_or_default().to_lowercase();
    code == "40085"
        || (message.contains("unified account mode")
            && message.contains("classic account api is not supported"))
}

// ts: getBitgetApiErrorMessage
pub fn get_bitget_api_error_message<'a>(message: Option<&'a str>, fallback: &'a str) -> &'a str {
    message
        .filter(|message| !message.is_empty())
        .unwrap_or(fallback)
}

// ts: isBitgetEnvironmentMismatch
pub fn is_bitget_environment_mismatch(message: Option<&str>) -> bool {
    message.is_some_and(|message| message.to_lowercase().contains("environment is incorrect"))
}

// ts: BitgetExchange.promoteUtaRoutingFromClassicUnsupported
// The existing predicate receives the original payload. Receiver getter reads
// remain lazy and may fail; the predicate's own mapping/domain is unchanged.
pub fn promote_uta_routing_from_classic_unsupported<P, E>(
    payload: Option<&P>,
    classic_unsupported: impl FnOnce(Option<&P>) -> Result<bool, E>,
    supports_uta_routing: impl FnOnce() -> Result<bool, E>,
    testnet: impl FnOnce() -> Result<bool, E>,
    is_uta: &Cell<bool>,
    account_type_detected: &Cell<bool>,
) -> Result<bool, E> {
    if !classic_unsupported(payload)? || !supports_uta_routing()? || testnet()? {
        return Ok(false);
    }
    is_uta.set(true);
    account_type_detected.set(true);
    Ok(true)
}
