//! Journal-only projection of the native Bybit acknowledgement shape grammar.
use serde_json::{Value, json};

const PREFIX: &str = "Bybit create-batch acknowledgement does not prove its orders (";
const MAX_CHILDREN: usize = 8;

fn count(text: &str) -> Option<usize> {
    let value = text.parse::<usize>().ok()?;
    (value <= 500 && value.to_string() == text).then_some(value)
}

fn list(text: &str) -> Option<(Value, usize)> {
    let (shape, length) = text.split_once('/')?;
    let length = count(length)?;
    if !matches!(shape, "missing" | "null" | "array" | "invalid")
        || (shape != "array" && length != 0)
    {
        return None;
    }
    Some((json!({"shape":shape,"count":length}), length))
}

fn identity_shape(text: &str) -> bool {
    matches!(
        text,
        "missing-row"
            | "invalid-row"
            | "duplicate"
            | "missing"
            | "null"
            | "empty"
            | "nonempty"
            | "invalid"
    )
}

fn child(text: &str, expected_index: usize) -> Option<Value> {
    let (index, text) = text.split_once(":orderId=")?;
    let index = count(index)?;
    if index != expected_index {
        return None;
    }
    let (order_id, text) = text.split_once(",orderLinkId=")?;
    let (order_link_id, text) = text.split_once(",status=")?;
    let (status, text) = text.split_once(",code=")?;
    let (code, echo) = text.split_once(",echo=")?;
    if !identity_shape(order_id)
        || !identity_shape(order_link_id)
        || !matches!(
            echo,
            "unmatched"
                | "invalid"
                | "absent"
                | "requested-elsewhere"
                | "duplicated"
                | "unrequested"
                | "matched"
                | "mismatched"
        )
    {
        return None;
    }
    let code = match (status, code) {
        ("missing", "missing") | ("invalid", "invalid") => Value::String(code.into()),
        ("valid", code) => {
            let value = code.parse::<i64>().ok()?;
            if value.to_string() != code {
                return None;
            }
            value.into()
        }
        _ => return None,
    };
    Some(
        json!({"index":index,"orderId":order_id,"orderLinkId":order_link_id,
        "status":status,"code":code,"echo":echo}),
    )
}

pub(super) fn project(message: &str, legacy: Option<&Value>) -> Option<Value> {
    if message.len() > 4096 {
        return None;
    }
    let (_, text) = message.split_once(PREFIX)?;
    let text = text.strip_suffix(')')?.strip_prefix("targets=")?;
    let (targets, text) = text.split_once("; result.list=")?;
    let targets = count(targets)?;
    let (result, text) = text.split_once("; retExtInfo.list=")?;
    let (result, result_count) = list(result)?;
    let (statuses, text) = text.split_once("; children=[")?;
    let (statuses, status_count) = list(statuses)?;
    let (children, omitted) = text.split_once("]; omitted=")?;
    let omitted = count(omitted)?;
    let mut rows = Vec::new();
    if !children.is_empty() {
        for text in children.split(';') {
            if rows.len() == MAX_CHILDREN {
                return None;
            }
            rows.push(child(text, rows.len())?);
        }
    }
    let total = targets.max(result_count).max(status_count);
    if rows.len() != total.min(MAX_CHILDREN) || omitted != total.saturating_sub(MAX_CHILDREN) {
        return None;
    }
    // Native IDs are reconciliation evidence, never caller results or resend
    // authority. No client echoes, raw responses or arbitrary legacy keys survive.
    let evidence: Vec<_> = legacy
        .and_then(|value| value["partialOrderIds"].as_array())
        .into_iter()
        .flatten()
        .take(MAX_CHILDREN)
        .filter_map(Value::as_str)
        .filter(|id| {
            !id.is_empty()
                && id.len() <= 128
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
        .collect();
    Some(
        json!({"targets":targets,"result":result,"statuses":statuses,
        "children":rows,"omitted":omitted,"evidenceOrderIds":evidence}),
    )
}
