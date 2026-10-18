//! Submission diagnostics retain the payload separately from the fallback message.
use safe_cex_num::{js_number_string, js_trim};
use serde_json::Value;

pub fn string_from_unknown(value: &Value) -> Option<&str> {
    value.as_str().map(js_trim).filter(|s| !s.is_empty())
}

fn get_bybit_error_field<'a>(payload: &'a Value, keys: &[&str]) -> &'a Value {
    keys.iter()
        .find_map(|key| payload.get(*key).filter(|v| !v.is_null()))
        .unwrap_or(&Value::Null)
}

pub fn get_bybit_payload_message(payload: &Value) -> Option<String> {
    string_from_unknown(payload)
        .or_else(|| {
            string_from_unknown(get_bybit_error_field(
                payload,
                &["retMsg", "ret_msg", "message", "msg"],
            ))
        })
        .map(str::to_owned)
}

pub fn format_bybit_trading_agreement_error(
    payload: &Value,
    exchange_name: Option<&str>,
) -> Option<String> {
    let code = get_bybit_error_field(payload, &["retCode", "ret_code", "code"]);
    // Unlike response.normalizeCode, Number("") is zero here, not absent.
    let ret_code = if code.as_str().is_some_and(|s| js_trim(s).is_empty()) {
        Some(0.0)
    } else {
        crate::response::normalize_code(&code.clone().into())
    };
    let ret_msg = get_bybit_payload_message(payload);
    let normalized = ret_msg.as_deref().unwrap_or("").to_lowercase();
    let is_agreement_error = matches!(ret_code, Some(110123.0 | 110125.0))
        || (normalized.contains("trading terms")
            && (normalized.contains("must agree") || normalized.contains("must sign")))
        || (normalized.contains("agreement") && normalized.contains("trading this contract"));
    if !is_agreement_error {
        return None;
    }
    let action = if exchange_name == Some("hyrotrader") {
        "For HyroTrader, wait briefly and retry; if it persists, HyroTrader must sign the agreement on the master Bybit account."
    } else {
        "Sign the agreement from the Bybit master account before retrying."
    };
    let original = if let Some(message) = ret_msg {
        let message = message
            .trim_end_matches(|c: char| c == '.' || js_trim(c.encode_utf8(&mut [0; 4])).is_empty());
        Some(match ret_code {
            Some(code) => format!("{message} (retCode {})", js_number_string(code)),
            None => message.to_owned(),
        })
    } else {
        ret_code.map(|code| format!("retCode {}", js_number_string(code)))
    };
    Some(format!(
        "Bybit Trading Terms not accepted for this contract. {action}{}",
        original
            .filter(|s| !s.is_empty())
            .map_or_else(String::new, |s| format!(" Original Bybit error: {s}."))
    ))
}

/// Preserve the venue response and transport fallback as separate inputs.
pub fn format_bybit_venue_error(
    payload: &Value,
    fallback: &Value,
    exchange_name: Option<&str>,
) -> String {
    format_bybit_trading_agreement_error(payload, exchange_name)
        .or_else(|| get_bybit_payload_message(payload))
        .or_else(|| get_bybit_payload_message(fallback))
        .unwrap_or_else(|| "Unknown Bybit error".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires frozen workspace Node dependencies; mandatory venue-oracle:bybit CI step"]
    fn submission_diagnostics_match_live_source() {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/error-fixtures.mjs"
            ))
            .output()
            .expect("Node and TypeScript are required");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let cases: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            cases.as_array().unwrap().len(),
            13830,
            "exchange_errors.rs-CARDINALITY source total"
        );
        for (i, case) in cases.as_array().unwrap().iter().enumerate() {
            assert_eq!(
                format_bybit_venue_error(
                    &case["payload"],
                    &case["fallback"],
                    case["exchange"].as_str()
                ),
                case["formatted"].as_str().unwrap(),
                "complete venue diagnostic case {i}: {case}"
            );
            assert_eq!(
                get_bybit_payload_message(&case["payload"]),
                case["message"].as_str().map(str::to_owned),
                "message case {i}: {case}"
            );
            assert_eq!(
                format_bybit_trading_agreement_error(&case["payload"], case["exchange"].as_str()),
                case["agreement"].as_str().map(str::to_owned),
                "agreement case {i}: {case}"
            );
        }
    }
}
