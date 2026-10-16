//! Payload diagnostics; UTF-8/JSON host boundary is declared in the parity manifest.
use regex::{Captures, Regex};
use serde_json::{Map, Value};
use std::cell::Cell;

const REDACTED: &str = "[redacted]";
const SECRET_KEYS: &[&str] = &[
    "apikey",
    "api_key",
    "key",
    "secret",
    "passphrase",
    "sign",
    "signature",
    "wsauthsignature",
    "jwt",
    "token",
    "sessiontoken",
    "session",
    "authkey",
    "listenkey",
    "password",
];

pub fn set_ws_diagnostics_enabled(enabled: bool) {
    with_enabled(|value| value.set(enabled));
}
pub fn is_ws_diagnostics_enabled() -> bool {
    with_enabled(Cell::get)
}
fn with_enabled<T>(operation: impl FnOnce(&Cell<bool>) -> T) -> T {
    thread_local! { static ENABLED: Cell<bool> = const { Cell::new(false) }; }
    ENABLED.with(operation)
}
pub fn stringify_ws_diagnostic_payload(data: &Value) -> String {
    data.as_str().map_or_else(
        || serde_json::to_string(data).expect("JSON value"),
        str::to_owned,
    )
}
pub fn normalize_secret_key(key: &str) -> String {
    key.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .map(|c| c.to_ascii_lowercase())
        .collect()
}
pub fn is_auth_operation_object(value: &Map<String, Value>) -> bool {
    let re = Regex::new("(?i-u:auth|login|signature|jwt)").expect("literal regex");
    ["op", "method", "type", "event"].iter().any(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .is_some_and(|text| re.is_match(text))
    })
}
pub fn redact_json_value(value: &Value, depth: usize, redact_strings: bool) -> Value {
    if depth > 12 {
        return value.clone();
    }
    match value {
        Value::String(_) if redact_strings => Value::String(REDACTED.into()),
        Value::Array(values) => Value::Array(
            values
                .iter()
                .map(|child| redact_json_value(child, depth + 1, redact_strings))
                .collect(),
        ),
        Value::Object(values) => {
            let auth_scope = redact_strings || is_auth_operation_object(values);
            Value::Object(
                values
                    .iter()
                    // Source assigns into {}, invoking its local __proto__ setter.
                    .filter(|(key, _)| key.as_str() != "__proto__")
                    .map(|(key, child)| {
                        let normalized = normalize_secret_key(key);
                        let result = if SECRET_KEYS.contains(&normalized.as_str()) {
                            Value::String(REDACTED.into())
                        } else {
                            redact_json_value(
                                child,
                                depth + 1,
                                auth_scope && (normalized == "args" || normalized == "params"),
                            )
                        };
                        (key.clone(), result)
                    })
                    .collect(),
            )
        }
        _ => value.clone(),
    }
}
pub fn redact_structured_json(message: &str) -> Option<String> {
    let trimmed = safe_cex_num::js_trim(message);
    if !trimmed.starts_with(['{', '[']) {
        return None;
    }
    let parsed: Value = serde_json::from_str(trimmed).ok()?;
    Some(stringify_ws_diagnostic_payload(&redact_json_value(
        &parsed, 0, false,
    )))
}
pub fn shorten_hex_secret(value: &str) -> String {
    // Only called on ASCII hex runs by the payload redactor.
    format!(
        "{}...{}",
        value.chars().take(4).collect::<String>(),
        value
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>()
    )
}
pub fn redact_ws_diagnostic_payload(message: &str) -> String {
    let structured = redact_structured_json(message);
    let source = structured.as_deref().unwrap_or(message);
    let names = "apiKey|api_key|key|secret|passphrase|sign|signature|wsAuthSignature|jwt|token|sessionToken|session|authKey|listenKey|password";
    // JS whitespace includes BOM and excludes U+0085. Keep case-folding ASCII like /i.
    let space = r"[\t\n\v\f\r \u{00a0}\u{1680}\u{2000}-\u{200a}\u{2028}\u{2029}\u{202f}\u{205f}\u{3000}\u{feff}]";
    let json = Regex::new(&format!(r#"("(?i-u:{names})"{space}*:{space}*)("(?:\\.|[^"])*"|[^,}}\]\t\n\v\f\r \u{{00a0}}\u{{1680}}\u{{2000}}-\u{{200a}}\u{{2028}}\u{{2029}}\u{{202f}}\u{{205f}}\u{{3000}}\u{{feff}}]+)"#)).expect("literal JSON regex");
    let source = json.replace_all(source, |caps: &Captures<'_>| {
        format!("{}\"{REDACTED}\"", &caps[1])
    });
    let query = Regex::new(&format!(r"((?:^|[?&]|{space})(?i-u:{names})=)([^&\t\n\v\f\r \u{{00a0}}\u{{1680}}\u{{2000}}-\u{{200a}}\u{{2028}}\u{{2029}}\u{{202f}}\u{{205f}}\u{{3000}}\u{{feff}}]+)")).expect("literal query regex");
    let source = query.replace_all(&source, |caps: &Captures<'_>| {
        format!("{}{REDACTED}", &caps[1])
    });
    Regex::new(r"(?i-u:\b[0-9a-f]{40,}\b)")
        .expect("literal hex regex")
        .replace_all(&source, |caps: &Captures<'_>| shorten_hex_secret(&caps[0]))
        .into_owned()
}
