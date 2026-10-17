//! Pure predicates used by the Bybit API client. Transport lifecycle remains separate.
use serde_json::Value;
pub const BYBIT_ENV_PROBE_CONFIG_KEY: &str = "safeCexBybitEnvProbe";
pub const BYBIT_ENV_MISMATCH_MESSAGE_PREFIX: &str = "Bybit API key environment mismatch";
const CLOCK_SYNC_RETRY_CONFIG_KEY: &str = "safeCexBybitClockSyncRetry";

pub(crate) fn truthy(value: &Value) -> bool {
    safe_cex_num::js_truthy(Some(value))
}

pub fn is_signed_request(config: &Value) -> bool {
    truthy(&config["headers"]["X-BAPI-SIGN"])
}
pub fn is_public_endpoint(url: Option<&str>) -> bool {
    url.is_some_and(|url| url.contains("/v5/market/"))
}
pub fn is_instruments_info_request(error: &Value) -> bool {
    error["config"]["url"].as_str() == Some("/v5/market/instruments-info")
}
pub fn is_bybit_clock_sync_ret_msg(message: &str) -> bool {
    message.to_lowercase().contains("recv")
}
pub fn parse_bybit_server_timestamp(message: &str) -> Option<f64> {
    // JS regex is unanchored, ASCII-only, and returns the first complete match.
    for (offset, _) in message.match_indices("server_timestamp[") {
        let rest = &message[offset + "server_timestamp[".len()..];
        let count = rest.bytes().take_while(u8::is_ascii_digit).count();
        if count >= 12 && rest.as_bytes().get(count) == Some(&b']') {
            let number = rest[..count].parse::<f64>().ok()?;
            return number.is_finite().then_some(number);
        }
    }
    None
}
pub fn has_retried_clock_sync(config: &Value) -> bool {
    truthy(&config[CLOCK_SYNC_RETRY_CONFIG_KEY])
}
pub fn is_bybit_env_probe_request(config: &Value) -> bool {
    truthy(&config[BYBIT_ENV_PROBE_CONFIG_KEY])
}

// ts: isBybitSignRejection — Bybit has said both "error sign!" and "Error sign, please
// check your signature generation algorithm"; 10004 is the code behind either.
pub fn is_bybit_sign_rejection(data: &Value) -> bool {
    let meta = crate::response::get_bybit_response_meta(data);
    meta.ret_code == Some(10004.)
        || meta
            .ret_msg
            .as_deref()
            .unwrap_or("")
            .to_ascii_lowercase()
            .contains("error sign")
}
