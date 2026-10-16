use crate::signing::{number, truthy};
use serde_json::Value;
pub const EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE: &str =
    "Hyperliquid API returned an empty order response body";
pub(crate) fn js_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => safe_cex_num::js_number_string(number(&Value::Number(n.clone()))),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".into(),
        Value::Object(_) => "[object Object]".into(),
        Value::Array(a) => a
            .iter()
            .map(|v| {
                if v.is_null() {
                    String::new()
                } else {
                    js_string(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}
pub(crate) fn describe_response(value: &Value) -> String {
    match value {
        Value::Number(_) => {
            let n = number(value);
            if n.is_finite() {
                safe_cex_num::js_number_string(n)
            } else {
                "null".into()
            }
        }
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(describe_response)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Object(fields) => {
            let mut entries = fields.iter().collect::<Vec<_>>();
            let index = |key: &str| {
                key.parse::<u32>()
                    .ok()
                    .filter(|n| *n < u32::MAX && n.to_string() == key)
            };
            entries.sort_by(|(a, _), (b, _)| match (index(a), index(b)) {
                (Some(a), Some(b)) => a.cmp(&b),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                _ => std::cmp::Ordering::Equal,
            });
            format!(
                "{{{}}}",
                entries
                    .into_iter()
                    .map(|(key, value)| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        describe_response(value)
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        _ => value.to_string(),
    }
}
fn nonempty(v: Option<&Value>) -> Option<String> {
    let v = v.filter(|v| !v.is_null())?;
    let text = if let Value::String(s) = v {
        s.clone()
    } else {
        describe_response(v)
    };
    (!text.is_empty()).then_some(text)
}
fn response_data(data: &Value) -> Option<&Value> {
    data.get("response")
        .filter(|v| v.is_object())?
        .get("data")
        .filter(|v| v.is_object())
}
fn statuses(data: &Value) -> Option<&Vec<Value>> {
    response_data(data)
        .and_then(|d| d.get("statuses"))
        .and_then(Value::as_array)
        .or_else(|| data.get("statuses").and_then(Value::as_array))
}
pub fn extract_hyperliquid_response_error(data: &Value) -> Option<String> {
    if !data.is_object() {
        return None;
    }
    if let Some(e) = nonempty(data.get("error")) {
        return Some(e);
    }
    if data["status"] == "err" {
        return Some(
            nonempty(data.get("response"))
                .unwrap_or_else(|| "Hyperliquid API rejected the request".into()),
        );
    }
    if let Some(d) = response_data(data) {
        if d["status"] == "err" {
            return Some(
                nonempty(d.get("response"))
                    .unwrap_or_else(|| "Hyperliquid API rejected the request".into()),
            );
        }
        if let Some(e) = nonempty(
            d.get("status")
                .filter(|v| v.is_object())
                .and_then(|v| v.get("error")),
        ) {
            return Some(e);
        }
    }
    let mut errors = Vec::new();
    for s in statuses(data).into_iter().flatten() {
        if let Some(e) = nonempty(s.get("error"))
            && !errors.contains(&e)
        {
            errors.push(e)
        }
    }
    (!errors.is_empty()).then(|| errors.join(", "))
}
pub fn is_only_hyperliquid_terminal_cancel_error(message: &str) -> bool {
    let marker = "Order was never placed, already canceled, or filled";
    let mut remainder = String::new();
    let mut input = message;
    let mut found = false;
    while let Some(pos) = input
        .to_ascii_lowercase()
        .find(&marker.to_ascii_lowercase())
    {
        found = true;
        remainder.push_str(&input[..pos]);
        input = &input[pos + marker.len()..];
        if let Some(rest) = input.strip_prefix('.') {
            input = rest;
        }
        let whitespace_len = input
            .chars()
            .take_while(|c| safe_cex_num::js_trim(&c.to_string()).is_empty())
            .map(char::len_utf8)
            .sum::<usize>();
        let candidate = &input[whitespace_len..];
        if candidate
            .get(..6)
            .is_some_and(|s| s.eq_ignore_ascii_case("asset="))
        {
            let rest = &candidate[6..];
            let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
            if digits > 0 {
                input = &rest[digits..];
            }
        }
    }
    remainder.push_str(input);
    found
        && remainder
            .chars()
            .all(|c| c == ',' || c == '.' || safe_cex_num::js_trim(&c.to_string()).is_empty())
}
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedHyperliquidOrderResponse {
    pub order_ids: Vec<String>,
    pub waiting_for_trigger: bool,
}
#[derive(Debug, Clone, PartialEq)]
pub struct HyperliquidUnreadableOrderResponseError {
    pub name: &'static str,
    pub message: String,
}
impl HyperliquidUnreadableOrderResponseError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            name: "HyperliquidUnreadableOrderResponseError",
            message: message.into(),
        }
    }
}
impl std::fmt::Display for HyperliquidUnreadableOrderResponseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.message.is_empty() {
            formatter.write_str(self.name)
        } else {
            write!(formatter, "{}: {}", self.name, self.message)
        }
    }
}
impl std::error::Error for HyperliquidUnreadableOrderResponseError {}
#[derive(Debug, Clone, PartialEq)]
pub enum HyperliquidOrderResponseError {
    Venue(String),
    Coercion(safe_cex_num::JsCoercionError),
    Unreadable(HyperliquidUnreadableOrderResponseError),
}
pub fn parse_hyperliquid_order_response(
    data: &Value,
) -> Result<ParsedHyperliquidOrderResponse, HyperliquidOrderResponseError> {
    let unreadable = |message| {
        HyperliquidOrderResponseError::Unreadable(HyperliquidUnreadableOrderResponseError::new(
            message,
        ))
    };
    if data.is_null() {
        return Err(unreadable(EMPTY_HYPERLIQUID_ORDER_RESPONSE_MESSAGE.into()));
    }
    if let Some(message) = extract_hyperliquid_response_error(data) {
        return Err(HyperliquidOrderResponseError::Venue(message));
    }
    let statuses = statuses(data).filter(|v| !v.is_empty()).ok_or_else(|| {
        unreadable(format!(
            "Hyperliquid order response missing statuses: {}",
            describe_response(data)
        ))
    })?;
    let mut result = ParsedHyperliquidOrderResponse {
        order_ids: Vec::new(),
        waiting_for_trigger: false,
    };
    for status in statuses {
        match status.as_str() {
            Some("waitingForTrigger") => {
                result.waiting_for_trigger = true;
                continue;
            }
            Some("waitingForFill") => continue,
            _ => {}
        }
        if !status.is_object() {
            return Err(unreadable(format!(
                "Hyperliquid order response contained unsupported status: {}",
                describe_response(status)
            )));
        }
        let oid = ["resting", "filled"]
            .into_iter()
            .find_map(|key| {
                status
                    .get(key)
                    .filter(|v| v.is_object())
                    .and_then(|v| v.get("oid"))
                    .filter(|v| !v.is_null())
                    .map(|value| safe_cex_num::js_string(Some(value)))
            })
            .transpose()
            .map_err(HyperliquidOrderResponseError::Coercion)?
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                unreadable(format!(
                    "Hyperliquid order response contained unrecognized status: {}",
                    describe_response(status)
                ))
            })?;
        result.order_ids.push(oid);
    }
    Ok(result)
}
pub fn hyperliquid_order_response_has_waiting_for_trigger(data: &Value) -> bool {
    statuses(data).is_some_and(|s| s.iter().any(|v| v == "waitingForTrigger"))
}
pub fn hyperliquid_order_response_has_deferred_order_status(data: &Value) -> bool {
    statuses(data).is_some_and(|s| {
        s.iter()
            .any(|v| v == "waitingForTrigger" || v == "waitingForFill")
    })
}
pub(crate) fn signed_body(body: &Value) -> bool {
    body.get("signature").is_some_and(truthy)
}
