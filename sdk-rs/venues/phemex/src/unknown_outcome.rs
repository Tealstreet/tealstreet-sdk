//! Bounded telemetry only; no request data or mutation authority enters this projection.
use safe_cex_engine::{
    CommandId, EngineUnknownOutcome, Level, LogEvent, RequestId, UNKNOWN_OUTCOME_TEXT, VenueError,
    log::MAX_LOG_BYTES, write_plan::ElementResult,
};

const MAX_ENTRIES: usize = 16;

fn bounded_list(values: impl Iterator<Item = String>) -> String {
    let mut result = String::new();
    for value in values.take(MAX_ENTRIES) {
        if result.len() + usize::from(!result.is_empty()) + value.len() > MAX_LOG_BYTES {
            break;
        }
        if !result.is_empty() {
            result.push(',');
        }
        result.push_str(&value);
    }
    result
}

pub(crate) fn engine_event(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let EngineUnknownOutcome {
        command,
        request: request_id,
        status,
        error,
        ..
    } = facts;
    project(*command, *request_id, *status, error)
}

pub(crate) fn project(
    command: CommandId,
    request_id: RequestId,
    status: Option<u16>,
    error: &VenueError,
) -> LogEvent {
    let placed = bounded_list(
        error
            .partial
            .iter()
            .flat_map(|partial| &partial.order_ids)
            .filter(|id| !id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_digit()))
            .cloned(),
    );
    let unknown = if let Some(elements) = &error.elements {
        bounded_list(
            elements
                .iter()
                .filter(|row| matches!(row.result, ElementResult::Unknown { .. }))
                .map(|row| row.element.index)
                .filter(|index| *index <= 65535)
                .map(|index| index.to_string()),
        )
    } else {
        error
            .partial
            .as_ref()
            .and_then(|partial| partial.failed_index)
            .filter(|index| *index <= 65535)
            .map(|index| index.to_string())
            .unwrap_or_default()
    };
    let mut fields = vec![
        ("command", format!("phemex.{command}").into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed.into()),
        ("unknownIndices", unknown.into()),
        ("request", request_id.0.to_string().into()),
    ];
    if let Some(status) = status.filter(|status| (100..=599).contains(status)) {
        fields.push(("status", status.into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}
