//! Closed projection for NX #9262: no native messages, URLs or signed payloads.
//! The static hook has only correlation IDs, so command labels are `command.<id>`.
//! Lists are diagnostic prefixes, bounded by NX and the engine's string cap.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{
    CommandId, EngineUnknownOutcome, Level, LogEvent, RequestId, UNKNOWN_OUTCOME_TEXT, VenueError,
};

const MAX_LIST_ENTRIES: usize = 16;

pub(crate) fn engine_unknown(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    unknown(
        facts.command,
        facts.request,
        facts.status,
        facts.error,
        facts
            .error
            .partial
            .as_ref()
            .map_or(&[], |p| p.order_ids.as_slice()),
    )
}

pub(crate) fn unknown(
    command: CommandId,
    request: RequestId,
    status: Option<u16>,
    error: &VenueError,
    placed_ids: &[String],
) -> LogEvent {
    let placed_ids = bounded_list(
        placed_ids
            .iter()
            .filter(|id| (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()))
            .cloned(),
    );
    let indices = bounded_list(
        error
            .elements
            .iter()
            .flat_map(|elements| elements.iter())
            .filter(|element| {
                matches!(element.result, ElementResult::Unknown { .. })
                    && element.element.index <= 65535
            })
            .map(|element| element.element.index.to_string()),
    );
    let request_id = request.0.to_string();
    let mut fields = vec![
        ("command", format!("command.{command}").into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed_ids.into()),
        ("unknownIndices", indices.into()),
        ("request", request_id.into()),
    ];
    if let Some(status) = status.filter(|s| (100..=599).contains(s)) {
        fields.push(("status", status.into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}

fn bounded_list(items: impl Iterator<Item = String>) -> String {
    let mut output = String::new();
    for item in items.take(MAX_LIST_ENTRIES) {
        let separator = usize::from(!output.is_empty());
        if output.len() + separator + item.len() > safe_cex_engine::log::MAX_LOG_BYTES {
            break;
        }
        if separator != 0 {
            output.push(',');
        }
        output.push_str(&item);
    }
    output
}
