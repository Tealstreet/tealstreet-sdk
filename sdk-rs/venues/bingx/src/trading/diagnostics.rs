//! Passive projection from prepared native intent; no authorization or raw values.
use super::*;
use safe_cex_engine::error::bingx_write_shape::{
    BingxWriteShape, Endpoint, IdClass, PARAMETERS, Parts, Replacement, SymbolFormat, TargetKind,
};

pub(super) fn capture(
    cx: &Cx<'_, Pending>,
    step: &Step,
    guard: Option<&protection::Guard>,
    observed: &HashMap<String, Observation>,
    replacements: &VecDeque<String>,
) -> Option<BingxWriteShape> {
    let (id, multiple) = match &step.call {
        Call::Cancel { ids, .. } => (
            ids.first().filter(|_| ids.len() == 1).map(String::as_str),
            ids.len() > 1,
        ),
        Call::Amend { id, .. } => (Some(id.as_str()), false),
        Call::Replace { old_id } => (Some(old_id.as_str()), false),
        Call::Place if guard.is_some() => (guard.map(|g| g.order.id.as_str()), false),
        _ => return None,
    };
    let endpoint = match step.path {
        ORDER => Endpoint::Order,
        AMEND => Endpoint::Amend,
        REPLACE => Endpoint::CancelReplace,
        CANCEL_ALL => Endpoint::CancelAll,
        _ => return None,
    };
    let target = id
        .and_then(|id| cx.store.order(id))
        .or_else(|| guard.map(|g| &g.order));
    let meta = id.and_then(|id| observed.get(id));
    let kind = if multiple {
        TargetKind::Multiple
    } else if meta.is_some_and(|m| m.attached) {
        TargetKind::Attached
    } else if meta
        .is_some_and(|m| matches!(m.native_kind.as_str(), "TRIGGER_MARKET" | "TRIGGER_LIMIT"))
    {
        TargetKind::Trigger
    } else {
        match target.map(|o| o.kind) {
            Some(OrderType::Limit) => TargetKind::Limit,
            Some(OrderType::TakeProfit) => TargetKind::TakeProfit,
            Some(OrderType::StopLoss) => TargetKind::StopLoss,
            Some(OrderType::TrailingStopLoss) => TargetKind::Trailing,
            Some(_) => TargetKind::Other,
            None => TargetKind::Unavailable,
        }
    };
    let (id_class, id_digits) = match id {
        Some(id) if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) => {
            (IdClass::Numeric, id.len())
        }
        Some(_) => (IdClass::Other, 0),
        None => (IdClass::Missing, 0),
    };
    let symbol = step.params.text_value("symbol");
    let token = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    };
    let symbol_format = match symbol {
        None if step.params.names().any(|name| name == "symbol") => SymbolFormat::Other,
        None | Some("") => SymbolFormat::Missing,
        Some(s) if s.split_once('-').is_some_and(|(a, b)| token(a) && token(b)) => {
            SymbolFormat::Hyphenated
        }
        Some(s) if token(s) => SymbolFormat::Compact,
        Some(_) => SymbolFormat::Other,
    };
    let mut parameters = 0;
    let mut other_parameters = false;
    for name in step.params.names().chain(["timestamp", "signature"]) {
        if let Some(index) = PARAMETERS.iter().position(|p| *p == name) {
            parameters |= 1 << index;
        } else {
            other_parameters = true;
        }
    }
    Some(BingxWriteShape::new(Parts {
        endpoint,
        post: step.method == Method::Post,
        parameters,
        other_parameters,
        id_class,
        id_digits,
        target_kind: kind,
        symbol_format,
        replacement: match id {
            Some(id) if replacements.iter().any(|r| r == id) => Replacement::Observed,
            Some(_) => Replacement::Unobserved,
            None => Replacement::NotApplicable,
        },
    }))
}

pub(super) fn remember(replacements: &mut VecDeque<String>, ids: &[String]) {
    for id in ids {
        if id.len() > 64 {
            continue;
        }
        if let Some(index) = replacements.iter().position(|old| old == id) {
            replacements.remove(index);
        }
        if replacements.len() == 64 {
            replacements.pop_front();
        }
        replacements.push_back(id.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_evidence_is_bounded_without_claiming_unobserved_history() {
        let mut ids = VecDeque::new();
        remember(
            &mut ids,
            &(0..70).map(|i| i.to_string()).collect::<Vec<_>>(),
        );
        assert_eq!(ids.len(), 64);
        assert_eq!(ids.front().unwrap(), "6");
        remember(&mut ids, &["6".into(), "private-canary".repeat(20)]);
        assert_eq!(ids.len(), 64);
        assert_eq!(ids.back().unwrap(), "6");
        assert!(ids.iter().all(|id| id.len() <= 64));
    }
}
