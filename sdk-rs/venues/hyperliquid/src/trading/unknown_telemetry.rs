use safe_cex_engine::{CommandId, LogEvent};

const MAX_LIST_ENTRIES: usize = 16;

// NX #9262 admits only bounded decimal lists. Invalid lists are omitted;
// truncation affects diagnostics only, never the complete outcome journal.
pub(super) fn event<'a>(
    command: CommandId,
    placed: impl Iterator<Item = &'a str>,
    unknown: impl Iterator<Item = usize>,
) -> LogEvent {
    let placed = placed
        .take(MAX_LIST_ENTRIES)
        .map(|id| {
            ((!id.is_empty()) && id.len() <= 32 && id.bytes().all(|b| b.is_ascii_digit()))
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
        level: safe_cex_engine::Level::Warn,
        code: "order.outcome_unknown",
        message: safe_cex_engine::UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}
