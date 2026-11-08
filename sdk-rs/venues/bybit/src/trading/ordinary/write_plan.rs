//! Exact mutation payloads and effect targets for reporting legacy fail-stop paths.
use super::{Leg, fields, not_sent};
use crate::trading::Call;
use safe_cex_engine::VenueError;
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, PlannedWrite, WritePlan};
use std::collections::VecDeque;

pub(super) struct NativeWrite {
    pub body: String,
    pub effects: Vec<EffectTarget>,
}

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
        Call::PlaceOne | Call::PlaceBatch { .. } => Vec::new(),
        Call::Cancel { ids } => ids
            .iter()
            .cloned()
            .map(|id| target(AppliedEffect::Cancel, id))
            .collect(),
        Call::CancelPartialStop { id, .. } => vec![target(AppliedEffect::Cancel, id.clone())],
        Call::CancelAll { .. } => vec![scoped(AppliedEffect::Cancel)?],
        Call::Amend | Call::ClearAttachedLeg { .. } => return amend(),
        Call::TradingStop | Call::PartialStop { .. } => vec![scoped(AppliedEffect::TradingStop)?],
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

pub(super) fn settle_failure(
    writes: &mut WritePlan<NativeWrite>,
    index: usize,
    error: VenueError,
) -> Result<(), safe_cex_engine::write_plan::PlanError> {
    writes.fail(index, error)
}

pub(super) fn build(legs: &VecDeque<Leg>) -> Result<Option<WritePlan<NativeWrite>>, VenueError> {
    let mut writes = Vec::new();
    let mut index = 0;
    for leg in legs {
        if matches!(leg.planned.call, Call::PartialProof(_)) {
            continue;
        }
        let wire: MutationWire = serde_json::from_str(&leg.planned.body)
            .map_err(|_| not_sent("Malformed Bybit planned mutation payload"))?;
        let effects = effects(leg, &wire)?;
        let own = fields(&leg.planned.body);
        let links = match &leg.planned.call {
            Call::PlaceBatch { .. } => own.items.into_iter().map(|row| row.order_link_id).collect(),
            Call::PlaceOne => vec![own.order_link_id],
            _ => vec![None; effects.len()],
        };
        let elements = links
            .into_iter()
            .map(|client_order_id| {
                let element = Element {
                    index,
                    client_order_id,
                };
                index += 1;
                element
            })
            .collect();
        writes.push(PlannedWrite {
            payload: NativeWrite {
                body: leg.planned.body.clone(),
                effects,
            },
            elements,
        });
    }
    if writes.is_empty() {
        return Ok(None);
    }
    // Legacy adapter preflight owns admission; this journal only records outcomes.
    WritePlan::validate(writes, |_| Ok(())).map(Some)
}
