//! Closed telemetry projection shared by engine-sealed and native Unknowns.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{
    CommandId, Cx, EngineUnknownOutcome, Level, LogEvent, RequestId, VenueError,
};

const LIST_LIMIT: usize = 16;

fn decimal_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_digit())
}

pub(crate) fn telemetry(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    project(
        facts.command,
        facts.request,
        facts.status,
        facts.error,
        std::iter::empty(),
    )
}

fn project<'a>(
    command: CommandId,
    request_id: RequestId,
    status: Option<u16>,
    error: &'a VenueError,
    placed: impl Iterator<Item = &'a str>,
) -> LogEvent {
    let ids = error
        .partial
        .iter()
        .flat_map(|partial| partial.order_ids.iter().map(String::as_str));
    let rows = error.elements.iter().flat_map(|rows| rows.iter());
    let placed_rows = rows.clone().flat_map(|row| match &row.result {
        ElementResult::Placed { order_ids } => order_ids.iter().map(String::as_str),
        _ => [].iter().map(String::as_str),
    });
    let mut placed_ids = String::new();
    // Leave native IDs whole under the engine's per-field log cap.
    for id in placed
        .chain(ids)
        .chain(placed_rows)
        .filter(|id| decimal_id(id))
        .take(LIST_LIMIT)
    {
        let separator = usize::from(!placed_ids.is_empty());
        if placed_ids.len() + separator + id.len() > safe_cex_engine::log::MAX_LOG_BYTES {
            break;
        }
        if separator != 0 {
            placed_ids.push(',');
        }
        placed_ids.push_str(id);
    }
    let indices = rows
        .filter(|row| matches!(row.result, ElementResult::Unknown { .. }))
        .filter_map(|row| u16::try_from(row.element.index).ok())
        .take(LIST_LIMIT)
        .map(|index| index.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let mut fields = vec![
        ("command", format!("kucoinWrite.{command}").into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed_ids.into()),
        ("unknownIndices", indices.into()),
        ("request", request_id.0.to_string().into()),
    ];
    if let Some(status) = status.filter(|status| (100..=599).contains(status)) {
        fields.push(("status", u32::from(status).into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: safe_cex_engine::UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}

#[derive(Clone, Copy)]
pub(crate) struct NativeSource {
    pub request_id: RequestId,
    pub status: Option<u16>,
}

impl NativeSource {
    pub fn emit<'a, P>(
        self,
        cx: &mut Cx<'_, P>,
        command: CommandId,
        error: &'a VenueError,
        placed: impl Iterator<Item = &'a str>,
    ) {
        let event = project(command, self.request_id, self.status, error, placed);
        cx.event(event.level, event.code, event.message, event.fields);
    }
}
