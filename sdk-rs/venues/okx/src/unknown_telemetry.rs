//! Closed NX #9262 projection for the engine-sealed Unknown hook.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{EngineUnknownOutcome, Level, LogEvent, UNKNOWN_OUTCOME_TEXT};

const MAX_ENTRIES: usize = 16;

pub(super) fn project(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let partial = facts
        .error
        .partial
        .iter()
        .flat_map(|partial| &partial.order_ids);
    let journal = facts
        .error
        .elements
        .iter()
        .flat_map(|elements| elements.iter());
    let placed = journal.clone().flat_map(|element| match &element.result {
        ElementResult::Placed { order_ids } => order_ids.as_slice(),
        _ => &[],
    });
    let mut ids = Vec::new();
    let mut bytes = 0;
    for id in partial.chain(placed) {
        if !(1..=32).contains(&id.len())
            || !id.bytes().all(|b| b.is_ascii_digit())
            || ids.contains(&id.as_str())
        {
            continue;
        }
        let length = bytes + usize::from(!ids.is_empty()) + id.len();
        // Stay within the engine's string cap so it cannot truncate an ID
        // or append an ellipsis that NX would reject as a malformed list.
        if ids.len() == MAX_ENTRIES || length > safe_cex_engine::log::MAX_LOG_BYTES {
            break;
        }
        ids.push(id.as_str());
        bytes = length;
    }
    let indices = journal
        .filter(|element| matches!(element.result, ElementResult::Unknown { .. }))
        .filter(|element| u16::try_from(element.element.index).is_ok())
        .take(MAX_ENTRIES)
        .map(|element| element.element.index.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut fields = vec![
        ("request", facts.request.0.to_string().into()),
        ("command", format!("okx-{}", facts.command).into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", ids.join(",").into()),
        ("unknownIndices", indices.into()),
    ];
    if let Some(status) = facts.status.filter(|s: &u16| (100..=599).contains(s)) {
        fields.push(("status", status.into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}
