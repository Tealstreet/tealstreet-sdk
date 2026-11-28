//! NX #9262 projection: identifiers only, never signed URLs or venue text.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{EngineUnknownOutcome, Level, LogEvent, RequestId};

pub(crate) fn unknown(source: &EngineUnknownOutcome<'_>) -> LogEvent {
    let EngineUnknownOutcome {
        request: RequestId(request_id),
        ..
    } = source;
    let mut placed = String::new();
    // Native decoded failures precede engine sealing, which adds aggregate IDs.
    let ids = source
        .error
        .partial
        .iter()
        .flat_map(|partial| &partial.order_ids)
        .chain(
            source
                .error
                .elements
                .iter()
                .filter(|_| source.error.partial.is_none())
                .flat_map(|elements| elements.iter())
                .flat_map(|row| match &row.result {
                    ElementResult::Placed { order_ids } => order_ids.as_slice(),
                    ElementResult::Applied {
                        target_order_id: Some(id),
                        ..
                    } => std::slice::from_ref(id),
                    _ => &[],
                }),
        );
    for id in ids
        .filter(|id| (1..=32).contains(&id.len()) && id.bytes().all(|byte| byte.is_ascii_digit()))
        .take(16)
    {
        // Keep the engine's field cap from replacing an ID with an ellipsis.
        if placed.len() + usize::from(!placed.is_empty()) + id.len()
            > safe_cex_engine::log::MAX_LOG_BYTES
        {
            break;
        }
        if !placed.is_empty() {
            placed.push(',');
        }
        placed.push_str(id);
    }
    let indices = source
        .error
        .elements
        .as_ref()
        .map(|elements| {
            elements
                .iter()
                .filter(|row| {
                    row.element.index <= u16::MAX as usize
                        && matches!(row.result, ElementResult::Unknown { .. })
                })
                .take(16)
                .map(|row| row.element.index.to_string())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default();
    let mut fields = vec![
        ("command", format!("asterdex.{}", source.command).into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed.into()),
        ("unknownIndices", indices.into()),
        ("request", request_id.to_string().into()),
    ];
    if let Some(status) = source.status.filter(|status| (100..=599).contains(status)) {
        fields.push(("status", status.into()));
    }
    LogEvent {
        level: Level::Warn,
        code: "order.outcome_unknown",
        message: safe_cex_engine::UNKNOWN_OUTCOME_TEXT.into(),
        fields,
    }
}
