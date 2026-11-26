//! NX #9262's closed diagnostic projection; never accepts transport material.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{EngineUnknownOutcome, Level, LogEvent, VenueError};

const MAX_LIST_ENTRIES: usize = 16;

pub(crate) fn engine(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let mut event = event(u64::from(facts.command), facts.error);
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

pub(crate) fn event(command: u64, error: &VenueError) -> LogEvent {
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
        )
        .take(MAX_LIST_ENTRIES)
        .map(|id| {
            (!id.is_empty() && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_digit()))
                .then_some(id.as_str())
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
    let unknown = journal
        .filter(|row| matches!(row.result, ElementResult::Unknown { .. }))
        .take(MAX_LIST_ENTRIES)
        .map(|row| {
            u16::try_from(row.element.index)
                .ok()
                .map(|index| index.to_string())
        })
        .collect::<Option<Vec<_>>>()
        .map(|indices| indices.join(","));
    let mut fields = vec![
        ("command", format!("command.{command}").into()),
        ("outcome", "unknown".into()),
    ];
    // The cap affects only telemetry. Invalid projected lists are omitted;
    // the caller's complete journal and native identities remain untouched.
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
