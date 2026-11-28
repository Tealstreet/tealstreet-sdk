//! Closed NX #9262 telemetry schema; never copy request wires or free venue text.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{EngineUnknownOutcome, Level, LogEvent, UNKNOWN_OUTCOME_TEXT};

const MAX_ENTRIES: usize = 16;

pub(crate) fn event(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let mut placed = String::new();
    if let Some(partial) = &facts.error.partial {
        for id in partial
            .order_ids
            .iter()
            .filter(|id| (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()))
            .take(MAX_ENTRIES)
        {
            let separator = usize::from(!placed.is_empty());
            if placed.len() + separator + id.len() > safe_cex_engine::log::MAX_LOG_BYTES {
                break;
            }
            if separator != 0 {
                placed.push(',');
            }
            placed.push_str(id);
        }
    }
    let indices = match &facts.error.elements {
        Some(rows) => rows
            .iter()
            .filter(|row| {
                matches!(row.result, ElementResult::Unknown { .. })
                    && row.element.index <= u16::MAX as usize
            })
            .take(MAX_ENTRIES)
            .map(|row| row.element.index.to_string())
            .collect::<Vec<_>>()
            .join(","),
        None => facts
            .error
            .partial
            .as_ref()
            .and_then(|partial| partial.failed_index)
            .filter(|index: &usize| *index <= u16::MAX as usize)
            .map(|index| index.to_string())
            .unwrap_or_default(),
    };
    let mut fields = vec![
        ("command", format!("woo.{}", facts.command).into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed.into()),
        ("unknownIndices", indices.into()),
        ("request", facts.request.0.to_string().into()),
    ];
    if let Some(status) = facts
        .status
        .filter(|status: &u16| (100..=599).contains(status))
    {
        fields.push(("status", u32::from(status).into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}
