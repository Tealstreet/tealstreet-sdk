//! Closed NX #9262 fields; native payloads and free-form errors never enter telemetry.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{
    CommandId, EngineUnknownOutcome, Level, LogEvent, RequestId, UNKNOWN_OUTCOME_TEXT,
};

pub(crate) const MAX_ENTRIES: usize = 16;

pub(crate) fn project(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let indices = facts.error.elements.iter().flat_map(|elements| {
        elements.iter().filter_map(|element| {
            matches!(element.result, ElementResult::Unknown { .. }).then_some(element.element.index)
        })
    });
    let fallback = facts
        .error
        .partial
        .as_ref()
        .and_then(|partial| partial.failed_index)
        .filter(|_| facts.error.elements.is_none());
    event(
        facts.command,
        facts.request,
        facts.status,
        facts
            .error
            .partial
            .as_ref()
            .map_or(&[], |partial| &partial.order_ids),
        indices.chain(fallback),
    )
}

pub(crate) fn event(
    command: CommandId,
    attempt_id: RequestId,
    status: Option<u16>,
    placed: &[String],
    indices: impl Iterator<Item = usize>,
) -> LogEvent {
    let placed = bounded_list(
        placed
            .iter()
            .filter(|id| (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()))
            .cloned(),
    );
    let indices = bounded_list(
        indices
            .filter(|index| *index <= 65_535)
            .map(|i| i.to_string()),
    );
    let mut fields = vec![
        ("command", format!("coinbase.{command}").into()),
        ("request", attempt_id.0.to_string().into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed.into()),
        ("unknownIndices", indices.into()),
    ];
    if let Some(status) = status.filter(|status| (100..=599).contains(status)) {
        fields.push(("status", u32::from(status).into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}

fn bounded_list(entries: impl Iterator<Item = String>) -> String {
    let mut output = String::new();
    for entry in entries.take(MAX_ENTRIES) {
        let separator = usize::from(!output.is_empty());
        // Preserve whole entries below the engine's string cap, avoiding an invalid NX list.
        if output.len() + separator + entry.len() > safe_cex_engine::log::MAX_LOG_BYTES {
            break;
        }
        if separator != 0 {
            output.push(',');
        }
        output.push_str(&entry);
    }
    output
}
