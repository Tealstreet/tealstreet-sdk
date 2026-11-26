//! Closed NX #9262 projection shared by native completion and the static hook.
const UNKNOWN_LIST_CAP: usize = 16;

fn bounded_unknown_list<T: AsRef<str>>(entries: impl Iterator<Item = T>) -> String {
    let mut list = String::new();
    for entry in entries.take(UNKNOWN_LIST_CAP) {
        let entry = entry.as_ref();
        let separator = usize::from(!list.is_empty());
        if list.len() + separator + entry.len() > safe_cex_engine::log::MAX_LOG_BYTES {
            break;
        }
        if separator != 0 {
            list.push(',');
        }
        list.push_str(entry);
    }
    list
}

fn unknown_indices(error: &safe_cex_engine::VenueError) -> impl Iterator<Item = usize> + '_ {
    error
        .elements
        .iter()
        .flat_map(|elements| elements.iter())
        .filter_map(|element| {
            matches!(
                element.result,
                safe_cex_engine::write_plan::ElementResult::Unknown { .. }
            )
            .then_some(element.element.index)
        })
}

pub(crate) fn unknown_event<'a>(
    command: safe_cex_engine::CommandId,
    request: Option<safe_cex_engine::RequestId>,
    status: Option<u16>,
    order_ids: impl Iterator<Item = &'a str>,
    indices: impl Iterator<Item = usize>,
) -> safe_cex_engine::LogEvent {
    let order_ids =
        bounded_unknown_list(order_ids.filter(|id| {
            (1..=32).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit())
        }));
    let indices = bounded_unknown_list(
        indices
            .filter(|index| u16::try_from(*index).is_ok())
            .map(|index| index.to_string()),
    );
    let mut fields = vec![
        ("command", format!("txflow-{command}").into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", order_ids.into()),
        ("unknownIndices", indices.into()),
    ];
    if let Some(request) = request {
        fields.push(("request", request.0.to_string().into()));
    }
    if let Some(status) = status.filter(|status| (100..=599).contains(status)) {
        fields.push(("status", u32::from(status).into()));
    }
    safe_cex_engine::LogEvent {
        level: safe_cex_engine::Level::Warn,
        code: "order.outcome_unknown",
        message: safe_cex_engine::UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}

pub(crate) fn engine_unknown_telemetry(
    facts: &safe_cex_engine::EngineUnknownOutcome<'_>,
) -> safe_cex_engine::LogEvent {
    unknown_event(
        facts.command,
        Some(facts.request),
        facts.status,
        facts
            .error
            .partial
            .iter()
            .flat_map(|partial| partial.order_ids.iter())
            .map(String::as_str),
        unknown_indices(facts.error),
    )
}
