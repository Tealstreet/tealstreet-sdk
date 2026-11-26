//! Closed telemetry fields for engine-sealed and native Kraken Futures Unknowns.
use safe_cex_engine::write_plan::{ElementOutcome, ElementResult};
use safe_cex_engine::{
    CommandId, EngineUnknownOutcome, Level, LogEvent, RequestId, UNKNOWN_OUTCOME_TEXT,
};

const MAX_LIST_ENTRIES: usize = 16;

pub(super) fn telemetry(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let elements = match facts.error.elements.as_deref() {
        Some(rows) => &rows[..],
        None => &[],
    };
    projected(
        facts.command,
        facts.request,
        facts.status,
        engine_placed_ids(elements),
        unknown_indices(elements),
    )
}

pub(super) fn native(
    command: CommandId,
    request: RequestId,
    status: Option<u16>,
    placed_ids: &[String],
) -> LogEvent {
    projected(
        command,
        request,
        status,
        order_ids(placed_ids.iter()),
        String::new(),
    )
}

fn projected(
    command: CommandId,
    request: RequestId,
    status: Option<u16>,
    placed_ids: String,
    unknown_indices: String,
) -> LogEvent {
    let mut fields = vec![
        ("command", format!("kraken-{command}").into()),
        ("request", request.0.to_string().into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed_ids.into()),
        ("unknownIndices", unknown_indices.into()),
    ];
    if let Some(status) = status
        && (100..=599).contains(&status)
    {
        fields.push(("status", status.into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}

fn engine_placed_ids(elements: &[ElementOutcome]) -> String {
    // Unknown partial IDs are reconciliation evidence. Only placed elements
    // prove accepted orders; client IDs and native diagnostics stay private.
    order_ids(
        elements
            .iter()
            .filter_map(|row| match &row.result {
                ElementResult::Placed { order_ids } => Some(order_ids),
                _ => None,
            })
            .flatten(),
    )
}

fn order_ids<'a>(ids: impl Iterator<Item = &'a String>) -> String {
    let mut placed = String::new();
    for id in ids
        .filter(|id| (1..=32).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit()))
        .take(MAX_LIST_ENTRIES)
    {
        let separator = usize::from(!placed.is_empty());
        // Preserve whole IDs when the engine caps each log field's bytes.
        if placed.len() + separator + id.len() > safe_cex_engine::log::MAX_LOG_BYTES {
            break;
        }
        if separator != 0 {
            placed.push(',');
        }
        placed.push_str(id);
    }
    placed
}

fn unknown_indices(elements: &[ElementOutcome]) -> String {
    elements
        .iter()
        .filter(|row| {
            matches!(row.result, ElementResult::Unknown { .. })
                && row.element.index <= u16::MAX as usize
        })
        .take(MAX_LIST_ENTRIES)
        .map(|row| row.element.index.to_string())
        .collect::<Vec<_>>()
        .join(",")
}
