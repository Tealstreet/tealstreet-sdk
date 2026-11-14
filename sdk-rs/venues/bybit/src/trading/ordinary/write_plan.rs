//! Logical mutation rows and effect targets for passive outcome reporting.
use super::{Leg, fields, not_sent};
use crate::trading::Call;
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, WritePlan};
use safe_cex_engine::{Outcome, VenueError};
use std::collections::VecDeque;

#[derive(Clone)]
pub(super) struct EffectTarget {
    effect: AppliedEffect,
    target_order_id: Option<String>,
    position_key: Option<String>,
}

impl EffectTarget {
    pub fn applied(&self) -> ElementResult {
        ElementResult::Applied {
            effect: self.effect,
            target_order_id: self.target_order_id.clone(),
            position_key: self.position_key.clone(),
        }
    }
}

fn effects(leg: &Leg, wire: &MutationWire) -> Result<Vec<EffectTarget>, VenueError> {
    let target = |effect, id: String| EffectTarget {
        effect,
        target_order_id: Some(id),
        position_key: None,
    };
    let scoped = |effect| {
        let scope = serde_json::to_string(&wire.scope)
            .map_err(|_| not_sent("Malformed Bybit planned effect scope"))?;
        Ok::<_, VenueError>(EffectTarget {
            effect,
            target_order_id: None,
            position_key: Some(format!("{}:{scope}", leg.planned.path)),
        })
    };
    let amend = || {
        wire.order_id
            .as_deref()
            .filter(|id| !id.is_empty())
            .map(|id| vec![target(AppliedEffect::Amend, id.to_owned())])
            .ok_or_else(|| not_sent("Bybit planned amend is missing its native target"))
    };
    Ok(match &leg.planned.call {
        Call::PlaceOne | Call::PlaceLot { .. } | Call::PlaceBatch { .. } => Vec::new(),
        Call::Cancel { ids } => ids
            .iter()
            .cloned()
            .map(|id| target(AppliedEffect::Cancel, id))
            .collect(),
        Call::CancelPartialStop { id, .. } => vec![target(AppliedEffect::Cancel, id.clone())],
        Call::CancelAll { .. } => vec![scoped(AppliedEffect::Cancel)?],
        Call::Amend | Call::ClearAttachedLeg { .. } => return amend(),
        Call::TradingStop | Call::PartialStop { .. } | Call::RestorePartialStop { .. } => {
            vec![scoped(AppliedEffect::TradingStop)?]
        }
        Call::Leverage { .. } | Call::MarginMode { .. } | Call::PositionMode { .. } => {
            vec![scoped(AppliedEffect::Settings)?]
        }
        Call::PartialProof(_) => return Err(not_sent("Read-only proof is not a mutation")),
    })
}

#[derive(Default, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct NativeScope {
    #[serde(skip_serializing_if = "Option::is_none")]
    category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    symbol: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    settle_coin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    position_idx: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    order_filter: Option<String>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct MutationWire {
    #[serde(flatten)]
    scope: NativeScope,
    order_id: Option<String>,
}

pub(super) fn record_rows(
    journal: &mut WritePlan,
    indices: &[usize],
    results: Vec<ElementResult>,
) -> Result<(), String> {
    if indices.len() != results.len() {
        return Err("Bybit report row count differs from its native rows".into());
    }
    for (&index, result) in indices.iter().zip(results) {
        journal
            .record(index, result)
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

pub(super) fn record_failure(
    journal: &mut WritePlan,
    indices: &[usize],
    mut error: VenueError,
) -> Result<(), String> {
    // Row details must not recursively contain the complete reporting journal.
    error.elements = None;
    let result = if matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected)) {
        ElementResult::NotSent {
            error: Some(Box::new(error)),
        }
    } else {
        ElementResult::Unknown {
            error: Box::new(error.outcome(Outcome::Unknown)),
        }
    };
    record_rows(journal, indices, vec![result; indices.len()])
}

pub(super) fn build(legs: &mut VecDeque<Leg>) -> Result<Option<WritePlan>, VenueError> {
    let mut elements = Vec::new();
    for leg in legs {
        if matches!(leg.planned.call, Call::PartialProof(_)) {
            continue;
        }
        let wire: MutationWire = serde_json::from_str(&leg.planned.body)
            .map_err(|_| not_sent("Malformed Bybit reporting payload"))?;
        leg.effects = effects(leg, &wire)?;
        let own = fields(&leg.planned.body);
        let links = match &leg.planned.call {
            Call::PlaceBatch { .. } => own.items.into_iter().map(|row| row.order_link_id).collect(),
            Call::PlaceOne | Call::PlaceLot { .. } => vec![own.order_link_id],
            _ => vec![None; leg.effects.len()],
        };
        for client_order_id in links {
            let index = elements.len();
            leg.elements.push(index);
            elements.push(Element {
                index,
                client_order_id,
            });
        }
    }
    Ok((!elements.is_empty()).then(|| WritePlan::new(elements)))
}

pub(super) fn append_restore(journal: &mut WritePlan) -> Result<usize, String> {
    let previous = journal.outcomes();
    let index = previous.len();
    let mut elements: Vec<_> = previous.iter().map(|row| row.element.clone()).collect();
    elements.push(Element {
        index,
        client_order_id: None,
    });
    let mut extended = WritePlan::new(elements);
    for row in previous {
        extended
            .record(row.element.index, row.result)
            .map_err(|error| error.to_string())?;
    }
    *journal = extended;
    Ok(index)
}
