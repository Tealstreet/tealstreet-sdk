//! Closed telemetry projection shared by engine-sealed and venue-decoded Unknowns.
use safe_cex_engine::log::MAX_LOG_BYTES;
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{
    EngineUnknownOutcome, Level, LogEvent, RequestId, UNKNOWN_OUTCOME_TEXT, VenueError,
};

const MAX_LIST_ENTRIES: usize = 16;

pub(crate) fn engine(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    project(facts.command, facts.request, facts.status, facts.error)
}

pub(crate) fn project(
    command: u32,
    request: RequestId,
    status: Option<u16>,
    error: &VenueError,
) -> LogEvent {
    let placed = list(
        error
            .partial
            .as_ref()
            .into_iter()
            .flat_map(|partial| &partial.order_ids)
            .filter(|id| {
                (1..=32).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit())
            })
            .cloned(),
    );
    let indices = list(
        error
            .elements
            .as_ref()
            .into_iter()
            .flat_map(|rows| rows.iter())
            .filter(|row| {
                matches!(&row.result, ElementResult::Unknown { .. })
                    && row.element.index <= usize::from(u16::MAX)
            })
            .map(|row| row.element.index.to_string()),
    );
    let request_id = request.0.to_string();
    let mut fields = vec![
        ("command", format!("bitunix.{command}").into()),
        ("outcome", "unknown".into()),
        ("request", request_id.into()),
        ("placedOrderIds", placed.into()),
        ("unknownIndices", indices.into()),
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

fn list(items: impl Iterator<Item = String>) -> String {
    let mut list = String::new();
    for (index, item) in items.enumerate() {
        // Keep whole identities below the engine's field cap, without truncation.
        let separator = usize::from(index != 0);
        if index == MAX_LIST_ENTRIES || list.len() + separator + item.len() > MAX_LOG_BYTES {
            break;
        }
        if index != 0 {
            list.push(',');
        }
        list.push_str(&item);
    }
    list
}
