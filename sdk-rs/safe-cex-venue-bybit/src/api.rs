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
fn includes_ascii_case_insensitive(message: &[u16], pattern: &[u8]) -> bool {
    message.windows(pattern.len()).any(|window| {
        window.iter().zip(pattern).all(|(unit, byte)| {
            u8::try_from(*unit).is_ok_and(|unit| unit.eq_ignore_ascii_case(byte))
        })
    })
}
pub fn is_bybit_clock_sync_ret_msg(message: &[u16]) -> bool {
    includes_ascii_case_insensitive(message, b"recv")
}
pub fn parse_bybit_server_timestamp(message: &[u16]) -> Option<f64> {
    // JS regex is unanchored, ASCII-only, and returns the first complete match.
    let prefix: Vec<_> = "server_timestamp[".encode_utf16().collect();
    for (offset, window) in message.windows(prefix.len()).enumerate() {
        if window != prefix {
            continue;
        }
        let rest = &message[offset + prefix.len()..];
        let count = rest
            .iter()
            .take_while(|unit| (48..=57).contains(*unit))
            .count();
        if count >= 12 && rest.get(count) == Some(&93) {
            let digits: String = rest[..count]
                .iter()
                .map(|unit| char::from(*unit as u8))
                .collect();
            let number = digits.parse::<f64>().ok()?;
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
pub fn is_bybit_sign_rejection(data: &safe_cex_types::retained_json::RetainedJsonInput) -> bool {
    let meta = crate::response::get_bybit_response_meta(data);
    meta.ret_code == Some(10004.)
        || meta
            .ret_msg
            .as_deref()
            .is_some_and(|message| includes_ascii_case_insensitive(message, b"error sign"))
}
