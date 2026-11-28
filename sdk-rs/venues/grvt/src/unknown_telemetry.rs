//! Bounded NX #9262 projection; request contents never enter telemetry.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{CommandId, EngineUnknownOutcome, Level, LogEvent};

const MAX_LIST_ENTRIES: usize = 16;

pub(crate) fn engine(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let error = facts.error;
    let journal = error.elements.iter().flat_map(|elements| elements.0.iter());
    let placed = error
        .partial
        .iter()
        .flat_map(|partial| partial.order_ids.iter())
        .chain(
            journal
                .clone()
                .filter(|_| error.partial.is_none())
                .flat_map(|row| match &row.result {
                    ElementResult::Placed { order_ids } => order_ids.as_slice(),
                    _ => &[],
                }),
        );
    let unknown = journal
        .filter(|row| matches!(row.result, ElementResult::Unknown { .. }))
        .map(|row| row.element.index);
    let mut event = project(facts.command, placed.map(String::as_str), unknown);
    event
        .fields
        .push(("request", facts.request.0.to_string().into()));
    if let Some(status) = facts
        .status
        .filter(|status: &u16| (100..=599).contains(status))
    {
        event.fields.push(("status", status.into()));
    }
    event
}

pub(crate) fn native(command: CommandId, placed: &[String]) -> LogEvent {
    project(
        command,
        placed.iter().map(String::as_str),
        std::iter::empty(),
    )
}

fn project<'a>(
    command: CommandId,
    placed: impl Iterator<Item = &'a str>,
    unknown: impl Iterator<Item = usize>,
) -> LogEvent {
    let placed = placed
        .take(MAX_LIST_ENTRIES)
        .map(|id| {
            (!id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_digit()))
                .then_some(id)
        })
        .collect::<Option<Vec<_>>>()
        .map(|ids| {
            let mut list = String::new();
            for id in ids {
                let separator = usize::from(!list.is_empty());
                if list.len() + separator + id.len() > safe_cex_engine::log::MAX_LOG_BYTES {
                    break;
                }
                if separator != 0 {
                    list.push(',');
                }
                list.push_str(id);
            }
            list
        });
    let unknown = unknown
        .take(MAX_LIST_ENTRIES)
        .map(|index| u16::try_from(index).ok().map(|index| index.to_string()))
        .collect::<Option<Vec<_>>>()
        .map(|indices| indices.join(","));
    let mut fields = vec![
        ("command", format!("command.{command}").into()),
        ("outcome", "unknown".into()),
    ];
    if let Some(placed) = placed {
        fields.push(("placedOrderIds", placed.into()));
    }
    if let Some(unknown) = unknown {
        fields.push(("unknownIndices", unknown.into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: safe_cex_engine::UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}
