use indexmap::IndexSet;
use serde_json::{Value, json};
use url::Url;

const TRUNCATED: &str = "[TRUNCATED]";

fn verbose_enabled(debug: Option<&str>, public_log: Option<&str>, process_available: bool) -> bool {
    process_available && (matches!(debug, Some("1" | "true")) || public_log == Some("1"))
}

pub fn is_trade_tunnel_verbose_logging_enabled() -> bool {
    verbose_enabled(
        std::env::var("DEBUG").ok().as_deref(),
        std::env::var("NEXT_PUBLIC_LOG_TRADETUNNEL_TO_CONSOLE_LOG")
            .ok()
            .as_deref(),
        !cfg!(target_arch = "wasm32"),
    )
}

pub fn log_trade_tunnel_verbose(message: &str, data: &Value) {
    println!("{message} {data}");
}

fn bounded_string(value: &str) -> String {
    let utf16: Vec<_> = value.encode_utf16().collect();
    if utf16.len() <= 500 {
        return value.into();
    }
    format!("{}...{TRUNCATED}", String::from_utf16_lossy(&utf16[..500]))
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "authorization",
        "cookie",
        "password",
        "passphrase",
        "secret",
        "token",
        "signature",
        "apikey",
        "api-key",
        "api_key",
        "accesskey",
        "access-key",
        "access_key",
    ]
    .iter()
    .any(|pattern| key.contains(pattern))
}

pub fn sanitize_for_trade_tunnel_log(value: &Value) -> Value {
    sanitize(value, 0)
}
fn sanitize(value: &Value, depth: usize) -> Value {
    match value {
        // ts-bug: sanitizeForTradeTunnelLog redacts keys, not secrets embedded in strings.
        Value::String(value) => Value::String(bounded_string(value)),
        Value::Array(values) if depth < 4 => {
            let mut output: Vec<_> = values
                .iter()
                .take(20)
                .map(|value| sanitize(value, depth + 1))
                .collect();
            if values.len() > 20 {
                output.push(json!(format!("{TRUNCATED} {} items", values.len() - 20)));
            }
            Value::Array(output)
        }
        Value::Object(values) if depth < 4 => {
            let mut output = serde_json::Map::new();
            for (key, value) in values.iter().take(50) {
                output.insert(
                    key.clone(),
                    if sensitive_key(key) {
                        json!("[REDACTED]")
                    } else {
                        sanitize(value, depth + 1)
                    },
                );
            }
            if values.len() > 50 {
                output.insert("__truncatedKeys".into(), json!(values.len() - 50));
            }
            Value::Object(output)
        }
        Value::Array(_) | Value::Object(_) => json!(TRUNCATED),
        _ => value.clone(),
    }
}

pub fn summarize_trade_tunnel_url(value: &str) -> Value {
    match Url::parse(value) {
        Ok(url) => {
            let keys: IndexSet<_> = url.query_pairs().map(|(key, _)| key.into_owned()).collect();
            json!({"origin":url.origin().ascii_serialization(), "pathname":url.path(), "queryKeys":keys.into_iter().collect::<Vec<_>>()})
        }
        Err(_) => {
            json!({"value":bounded_string(value.split('?').next().unwrap_or("")), "hasQuery":value.contains('?')})
        }
    }
}

pub fn has_loggable_content(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        _ => true,
    }
}

fn stringify_js(value: &Value) -> String {
    let mut value = value.clone();
    crate::how::normalize_json_numbers(&mut value, false, false);
    value.to_string()
}

pub fn format_trade_tunnel_error_message(error: &Value) -> String {
    match error {
        Value::String(value) => value.clone(),
        Value::Object(value) => {
            for key in ["message", "error", "msg"] {
                if let Some(value) = value
                    .get(key)
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty())
                {
                    return value.into();
                }
            }
            stringify_js(error)
        }
        Value::Array(_) => stringify_js(error),
        Value::Number(number) => {
            let number = number
                .to_string()
                .parse::<f64>()
                .expect("JSON numeric token");
            if number == f64::INFINITY {
                "Infinity".into()
            } else if number == f64::NEG_INFINITY {
                "-Infinity".into()
            } else {
                safe_cex_num::js_number_string(number)
            }
        }
        _ => error.to_string(),
    }
}

pub fn log_trade_tunnel_request(request: &Value, route: Option<&Value>) {
    if !is_trade_tunnel_verbose_logging_enabled() {
        return;
    }
    let mut data = json!({"url":summarize_trade_tunnel_url(request["url"].as_str().unwrap_or("")), "method":request["method"]});
    if let Some(route) = route {
        data["route"] = route.clone();
    }
    for key in ["params", "data"] {
        if let Some(value) = request.get(key).filter(|value| has_loggable_content(value)) {
            data[key] = sanitize_for_trade_tunnel_log(value);
        }
    }
    log_trade_tunnel_verbose("[TradeTunnel] REQUEST:", &data);
}

pub fn log_trade_tunnel_response(request: &Value, response: &Value, route: Option<&Value>) {
    if !is_trade_tunnel_verbose_logging_enabled() {
        return;
    }
    let mut data = json!({"url":summarize_trade_tunnel_url(request["url"].as_str().unwrap_or("")), "method":request["method"]});
    if let Some(route) = route {
        data["route"] = route.clone();
    }
    if let Some(value) = response.get("data") {
        data["data"] = sanitize_for_trade_tunnel_log(value);
    } else {
        data["response"] = sanitize_for_trade_tunnel_log(response);
    }
    log_trade_tunnel_verbose("[TradeTunnel] RESPONSE:", &data);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn log_limits_redaction_utf16_and_primitive_content_match() {
        for key in [
            "Authorization",
            "Cookie",
            "password",
            "passphrase",
            "clientSecret",
            "token",
            "signature",
            "api-key",
            "API_KEY",
            "apiKey",
            "access-key",
            "access_key",
            "accessKey",
            "x-mbx-apikey",
        ] {
            assert_eq!(
                sanitize_for_trade_tunnel_log(&json!({key:"hidden"}))[key],
                "[REDACTED]"
            );
        }
        assert_eq!(
            sanitize_for_trade_tunnel_log(&json!("token=visible")),
            "token=visible"
        );
        assert_eq!(
            sanitize_for_trade_tunnel_log(&json!("a".repeat(500))),
            "a".repeat(500)
        );
        assert_eq!(
            sanitize_for_trade_tunnel_log(&json!("a".repeat(501))),
            format!("{}...[TRUNCATED]", "a".repeat(500))
        );
        assert_eq!(
            sanitize_for_trade_tunnel_log(&json!("😀".repeat(251))),
            format!("{}...[TRUNCATED]", "😀".repeat(250))
        );
        let array = json!((0..21).collect::<Vec<_>>());
        assert_eq!(
            sanitize_for_trade_tunnel_log(&array)
                .as_array()
                .unwrap()
                .len(),
            21
        );
        assert_eq!(
            sanitize_for_trade_tunnel_log(&array)[20],
            "[TRUNCATED] 1 items"
        );
        let object: serde_json::Map<String, Value> = (0..51)
            .map(|index| (format!("key{index}"), json!(index)))
            .collect();
        let sanitized = sanitize_for_trade_tunnel_log(&Value::Object(object));
        assert_eq!(sanitized["__truncatedKeys"], 1);
        assert!(sanitized.get("key50").is_none());
        assert_eq!(
            sanitize_for_trade_tunnel_log(&json!([[[[{}]]]])),
            json!([[[["[TRUNCATED]"]]]])
        );
        assert_eq!(
            sanitize_for_trade_tunnel_log(&json!([[[[[1]]]]])),
            json!([[[["[TRUNCATED]"]]]])
        );
        for value in [json!(null), json!(""), json!([]), json!({})] {
            assert!(!has_loggable_content(&value));
        }
        for value in [
            json!(false),
            json!(0),
            json!("x"),
            json!([null]),
            json!({"x":null}),
        ] {
            assert!(has_loggable_content(&value));
        }
    }
    #[test]
    fn url_summary_discloses_split_utf16_surrogate_replacement() {
        let input = format!("a{}?secret=hidden", "😀".repeat(250));
        let summary = summarize_trade_tunnel_url(&input);
        assert_eq!(
            summary,
            json!({"value": format!("a{}�...[TRUNCATED]", "😀".repeat(249)), "hasQuery": true})
        );
    }

    #[test]
    fn log_enablement_url_summaries_and_error_priority_are_pinned() {
        for debug in [None, Some("0"), Some("TRUE")] {
            assert!(!verbose_enabled(debug, None, true));
        }
        assert!(verbose_enabled(Some("true"), None, true));
        assert!(verbose_enabled(Some("1"), None, true));
        assert!(verbose_enabled(None, Some("1"), true));
        assert!(!verbose_enabled(Some("1"), Some("1"), false));
        assert_eq!(
            summarize_trade_tunnel_url("https://EXAMPLE.com:443/a?secret=hidden&x=1&secret=other"),
            json!({"origin":"https://example.com","pathname":"/a","queryKeys":["secret","x"]})
        );
        assert_eq!(
            summarize_trade_tunnel_url("relative?token=hidden"),
            json!({"value":"relative","hasQuery":true})
        );
        assert_eq!(
            format_trade_tunnel_error_message(
                &json!({"message":"first","error":"second","msg":"third"})
            ),
            "first"
        );
        assert_eq!(
            format_trade_tunnel_error_message(&json!({"message":"","error":"second"})),
            "second"
        );
        assert_eq!(
            format_trade_tunnel_error_message(&json!({"msg":"third"})),
            "third"
        );
        assert_eq!(
            format_trade_tunnel_error_message(&json!({"x":1})),
            "{\"x\":1}"
        );
        assert_eq!(format_trade_tunnel_error_message(&json!("exact")), "exact");
        assert_eq!(format_trade_tunnel_error_message(&json!(null)), "null");
        assert_eq!(format_trade_tunnel_error_message(&json!([1, 2])), "[1,2]");
        assert_eq!(format_trade_tunnel_error_message(&json!(1.0)), "1");
    }
}
