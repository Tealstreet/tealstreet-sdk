use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{EngineUnknownOutcome, Level, LogEvent, UNKNOWN_OUTCOME_TEXT};

const MAX_ENTRIES: usize = 16;

pub(crate) fn project(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let EngineUnknownOutcome {
        request: correlation,
        ..
    } = facts;
    let elements = facts
        .error
        .elements
        .as_deref()
        .map(|elements| &elements[..])
        .unwrap_or_default();
    let ids = elements
        .iter()
        .filter_map(|element| match &element.result {
            ElementResult::Placed { order_ids } => Some(order_ids.iter()),
            _ => None,
        })
        .flatten()
        .filter(|id| {
            !id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_digit())
        })
        .take(MAX_ENTRIES);
    let mut placed = String::new();
    for id in ids {
        let separator = usize::from(!placed.is_empty());
        if placed.len() + separator + id.len() > safe_cex_engine::log::MAX_LOG_BYTES {
            break;
        }
        if separator != 0 {
            placed.push(',');
        }
        placed.push_str(id);
    }
    let indices = elements
        .iter()
        .filter(|element| {
            element.element.index <= u16::MAX as usize
                && matches!(element.result, ElementResult::Unknown { .. })
        })
        .take(MAX_ENTRIES)
        .map(|element| element.element.index.to_string())
        .collect::<Vec<_>>()
        .join(",");
    // NX #9262 accepts only bounded identifiers; never project raw diagnostics.
    let request_id = correlation.0.to_string();
    let mut fields = vec![
        ("command", format!("weex.{}", facts.command).into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed.into()),
        ("unknownIndices", indices.into()),
        ("request", request_id.into()),
    ];
    if let Some(status) = facts.status.filter(|status| (100..=599).contains(status)) {
        fields.push(("status", (status as f64).into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}
