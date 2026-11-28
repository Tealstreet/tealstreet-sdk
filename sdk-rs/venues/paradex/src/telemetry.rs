//! Closed NX #9262 projection for engine-sealed and native-decoded uncertainty.
use safe_cex_engine::log::MAX_LOG_BYTES;
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{
    CommandId, EngineUnknownOutcome, Level, LogEvent, RequestId, UNKNOWN_OUTCOME_TEXT,
};

const MAX_LIST_ENTRIES: usize = 16;

pub(crate) fn engine_unknown(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    unknown_outcome_event(
        facts.command,
        facts.request,
        facts.status,
        facts
            .error
            .partial
            .iter()
            .flat_map(|partial| partial.order_ids.iter().map(String::as_str)),
        facts
            .error
            .elements
            .iter()
            .flat_map(|rows| rows.iter())
            .filter_map(|row| {
                matches!(row.result, ElementResult::Unknown { .. }).then_some(row.element.index)
            }),
    )
}

pub(crate) fn unknown_outcome_event<'a>(
    command: CommandId,
    request: RequestId,
    status: Option<u16>,
    placed: impl Iterator<Item = &'a str>,
    unknown: impl Iterator<Item = usize>,
) -> LogEvent {
    let placed = bounded_list(
        placed
            .filter(|id| {
                (1..=32).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit())
            })
            .map(str::to_owned),
    );
    let unknown = bounded_list(
        unknown
            .filter(|index: &usize| *index <= usize::from(u16::MAX))
            .map(|index| index.to_string()),
    );
    let mut fields = vec![
        ("command", format!("paradex.{command}").into()),
        ("outcome", "unknown".into()),
        ("request", request.0.to_string().into()),
        ("placedOrderIds", placed.into()),
        ("unknownIndices", unknown.into()),
    ];
    if let Some(status) = status.filter(|status: &u16| (100..=599).contains(status)) {
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
    for (index, item) in items.take(MAX_LIST_ENTRIES).enumerate() {
        let separator = usize::from(index != 0);
        // Keep complete entries below the engine cap so it cannot cut an ID.
        if output.len() + separator + item.len() > MAX_LOG_BYTES {
            break;
        }
        if index != 0 {
            output.push(',');
        }
        output.push_str(&item);
    }
    output
}
