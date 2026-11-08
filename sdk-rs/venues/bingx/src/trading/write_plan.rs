//! Shared journals retain facts; native cancellation and held restore policy stay in Trading.
use super::*;
use safe_cex_engine::write_plan::{
    AppliedEffect, Element, ElementOutcomes, ElementResult, PlannedWrite, WritePlan,
};

pub(super) struct Journals {
    plans: Vec<WritePlan<Step>>,
    active: usize,
    independent: bool,
    remaining: usize,
}

fn validate_prepared(step: &Step) -> Result<(), VenueError> {
    // Legacy input/quantity/price rules already ran while formatting the entire Job.
    // Check only its closed native route here; do not add new economic refusals.
    let valid = match &step.call {
        Call::Place => step.method == Method::Post && step.path == ORDER,
        Call::Cancel { .. } => {
            step.method == Method::Delete && matches!(step.path, ORDER | CANCEL_ALL)
        }
        Call::Amend { .. } => step.method == Method::Post && step.path == AMEND,
        Call::Replace { .. } => step.method == Method::Post && step.path == REPLACE,
        Call::Leverage { .. } => step.method == Method::Post && step.path == crate::paths::LEVERAGE,
        Call::Margin { .. } => {
            step.method == Method::Post && step.path == "/openApi/swap/v2/trade/marginType"
        }
        Call::Mode { .. } => {
            step.method == Method::Post && step.path == "/openApi/swap/v1/positionSide/dual"
        }
    };
    if valid {
        Ok(())
    } else {
        Err(not_sent("Invalid prepared BingX write route").kind(ErrorKind::Internal))
    }
}

fn planned(steps: impl IntoIterator<Item = Step>) -> Result<WritePlan<Step>, VenueError> {
    let mut index = 0;
    let writes = steps
        .into_iter()
        .map(|step| {
            let count = match &step.call {
                Call::Replace { .. } => 2,
                Call::Cancel { ids, .. } => ids.len().max(1),
                _ => 1,
            };
            let client = step
                .receipts
                .iter()
                .find(|receipt| receipt.kind == OrderIdentityKind::ClientOrderId)
                .map(|receipt| receipt.identity.clone());
            let elements = (0..count)
                .map(|row| {
                    let element = Element {
                        index,
                        client_order_id: if matches!(step.call, Call::Replace { .. }) && row == 0 {
                            None
                        } else {
                            client.clone()
                        },
                    };
                    index += 1;
                    element
                })
                .collect();
            PlannedWrite {
                payload: step,
                elements,
            }
        })
        .collect();
    WritePlan::validate(writes, validate_prepared)
}

fn applied(
    effect: AppliedEffect,
    target: Option<String>,
    position: Option<String>,
) -> ElementResult {
    ElementResult::Applied {
        effect,
        target_order_id: target,
        position_key: position,
    }
}

fn failed(error: &VenueError) -> ElementResult {
    let mut error = error.clone();
    error.elements = None;
    match error.outcome {
        Some(Outcome::NotSent | Outcome::Rejected) => ElementResult::NotSent {
            error: Some(Box::new(error)),
        },
        _ => ElementResult::Unknown {
            error: Box::new(error.outcome(Outcome::Unknown)),
        },
    }
}

fn results(
    call: &Call,
    result: &Result<Vec<String>, VenueError>,
    canceled: bool,
) -> Vec<ElementResult> {
    match (call, result) {
        (Call::Replace { old_id }, result) => {
            let cancel = if canceled {
                applied(AppliedEffect::Cancel, Some(old_id.clone()), None)
            } else {
                ElementResult::Unknown {
                    error: Box::new(unknown("Replacement cancellation is unproven")),
                }
            };
            let replacement = match result {
                Ok(ids) => ElementResult::Placed {
                    order_ids: ids.clone(),
                },
                Err(error) => failed(error),
            };
            vec![cancel, replacement]
        }
        (Call::Cancel { ids, symbol, .. }, Ok(_)) => {
            if ids.is_empty() {
                vec![applied(AppliedEffect::Cancel, None, Some(symbol.clone()))]
            } else {
                ids.iter()
                    .map(|id| applied(AppliedEffect::Cancel, Some(id.clone()), None))
                    .collect()
            }
        }
        (Call::Amend { id, .. }, Ok(_)) => {
            vec![applied(AppliedEffect::Amend, Some(id.clone()), None)]
        }
        (Call::Leverage { symbol, side, .. }, Ok(_)) => vec![applied(
            AppliedEffect::Settings,
            None,
            Some(format!(
                "{symbol}:{}",
                match side {
                    Some(PositionSide::Long) => "LONG",
                    Some(PositionSide::Short) => "SHORT",
                    None => "BOTH",
                }
            )),
        )],
        (Call::Margin { symbol, .. }, Ok(_)) => {
            vec![applied(AppliedEffect::Settings, None, Some(symbol.clone()))]
        }
        (Call::Mode { .. }, Ok(_)) => vec![applied(AppliedEffect::Settings, None, None)],
        (Call::Place, Ok(ids)) => vec![ElementResult::Placed {
            order_ids: ids.clone(),
        }],
        (Call::Cancel { ids, .. }, Err(error)) => {
            (0..ids.len().max(1)).map(|_| failed(error)).collect()
        }
        (_, Err(error)) => vec![failed(error)],
    }
}

impl Journals {
    pub(super) fn new(steps: VecDeque<Step>, independent: bool) -> Result<Self, VenueError> {
        let remaining = steps.len();
        let plans = if independent {
            steps
                .into_iter()
                .map(|step| planned([step]))
                .collect::<Result<_, _>>()?
        } else {
            vec![planned(steps)?]
        };
        Ok(Self {
            plans,
            active: 0,
            independent,
            remaining,
        })
    }

    pub(super) fn next(&mut self) -> Option<(usize, usize, Step)> {
        loop {
            let plan = self.plans.get_mut(self.active)?;
            if let Some((write, planned)) = plan.next_write() {
                self.remaining = self.remaining.saturating_sub(1);
                return Some((self.active, write, planned.payload.clone()));
            }
            if !self.independent {
                return None;
            }
            self.active += 1;
        }
    }

    pub(super) fn has_next(&self) -> bool {
        self.remaining > 0
    }

    pub(super) fn discard_suffix(&mut self) {
        self.remaining = 0;
    }

    pub(super) fn restore(&mut self, step: Step) -> Result<(), VenueError> {
        self.plans.push(planned([step])?);
        self.active = self.plans.len() - 1;
        self.remaining = 1;
        Ok(())
    }

    pub(super) fn settle(
        &mut self,
        request: &Request,
        result: &Result<Vec<String>, VenueError>,
        canceled: bool,
    ) -> Result<(), VenueError> {
        self.plans
            .get_mut(request.journal)
            .ok_or_else(|| unknown("BingX write journal is unavailable"))?
            .settle(request.write, results(&request.call, result, canceled))
            .map_err(|error| unknown(error.to_string()).kind(ErrorKind::Internal))
    }

    pub(super) fn fail(&mut self, journal: usize, write: usize, mut error: VenueError) {
        error.elements = None;
        if let Some(plan) = self.plans.get_mut(journal) {
            let _ = plan.fail(write, error);
        }
    }

    pub(super) fn failure(&mut self, mut error: VenueError) -> VenueError {
        let mut outcomes = Vec::new();
        for plan in &mut self.plans {
            error = plan.failure(error);
            if let Some(elements) = error.elements.take() {
                for mut row in elements.0 {
                    row.element.index = outcomes.len();
                    outcomes.push(row);
                }
            }
        }
        error.elements = Some(Box::new(ElementOutcomes(outcomes)));
        error
    }

    pub(super) fn evidence(
        &self,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        evidence.write_plans(&self.plans);
    }
}
