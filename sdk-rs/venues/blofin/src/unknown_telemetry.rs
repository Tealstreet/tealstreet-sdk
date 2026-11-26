//! Closed telemetry projection shared by engine and native Unknown outcomes.
use safe_cex_engine::write_plan::ElementResult;
use safe_cex_engine::{EngineUnknownOutcome, Level, LogEvent, UNKNOWN_OUTCOME_TEXT};

const MAX_ENTRIES: usize = 16;

fn capped_list<S: AsRef<str>>(items: impl Iterator<Item = S>) -> String {
    let mut result = String::new();
    for item in items.take(MAX_ENTRIES) {
        let item = item.as_ref();
        let extra = usize::from(!result.is_empty());
        if result.len() + extra + item.len() > safe_cex_engine::log::MAX_LOG_BYTES {
            break;
        }
        if extra != 0 {
            result.push(',');
        }
        result.push_str(item);
    }
    result
}

pub(crate) fn project(facts: &EngineUnknownOutcome<'_>) -> LogEvent {
    let ids = facts
        .error
        .partial
        .iter()
        .flat_map(|partial| &partial.order_ids);
    let placed = capped_list(ids.filter(|id| {
        !id.is_empty() && id.len() <= 32 && id.bytes().all(|byte| byte.is_ascii_digit())
    }));
    let indices = facts
        .error
        .elements
        .iter()
        .flat_map(|elements| elements.iter());
    let unknown = capped_list(
        indices
            .filter(|element| {
                matches!(element.result, ElementResult::Unknown { .. })
                    && element.element.index <= 65_535
            })
            .map(|element| element.element.index.to_string()),
    );
    let mut fields = vec![
        ("command", format!("blofin-{}", facts.command).into()),
        ("outcome", "unknown".into()),
        ("placedOrderIds", placed.into()),
        ("unknownIndices", unknown.into()),
        ("request", facts.request.0.to_string().into()),
    ];
    if let Some(status) = facts
        .status
        .filter(|status: &u16| (100..=599).contains(status))
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
