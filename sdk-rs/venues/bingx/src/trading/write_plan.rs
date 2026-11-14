//! Passive outcome reporting; native Job owns every dispatch decision.
use super::*;
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, WritePlan};

pub(super) struct Journals {
    plan: WritePlan,
    next: usize,
}

fn count(call: &Call) -> usize {
    match call {
        Call::Replace { .. } => 2,
        Call::Cancel { ids, .. } => ids.len().max(1),
        _ => 1,
    }
}

fn elements(step: &Step, start: usize) -> Vec<Element> {
    let client = step
        .receipts
        .iter()
        .find(|receipt| receipt.kind == OrderIdentityKind::ClientOrderId)
        .map(|receipt| receipt.identity.clone());
    (0..count(&step.call))
        .map(|row| Element {
            index: start + row,
            client_order_id: if matches!(step.call, Call::Replace { .. }) && row == 0 {
                None
            } else {
                client.clone()
            },
        })
        .collect()
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
    pub(super) fn new(steps: &VecDeque<Step>) -> Self {
        let mut rows = Vec::new();
        for step in steps {
            rows.extend(elements(step, rows.len()));
        }
        Self {
            plan: WritePlan::new(rows),
            next: 0,
        }
    }

    pub(super) fn begin(&mut self, step: &Step) -> usize {
        let start = self.next;
        self.next += count(&step.call);
        self.record_failure(start, &step.call, unknown(UNKNOWN_WORDS));
        start
    }

    pub(super) fn restore(&mut self, step: &Step) {
        let rows = self.plan.outcomes();
        let mut all = rows
            .iter()
            .map(|row| row.element.clone())
            .collect::<Vec<_>>();
        all.extend(elements(step, all.len()));
        self.plan = WritePlan::new(all);
        for row in rows {
            let _ = self.plan.record(row.element.index, row.result);
        }
    }

    pub(super) fn settle(
        &mut self,
        request: &Request,
        result: &Result<Vec<String>, VenueError>,
        canceled: bool,
    ) {
        for (offset, result) in results(&request.call, result, canceled)
            .into_iter()
            .enumerate()
        {
            let _ = self.plan.record(request.write + offset, result);
        }
    }

    pub(super) fn record_failure(&mut self, start: usize, call: &Call, mut error: VenueError) {
        error.elements = None;
        for index in start..start + count(call) {
            let _ = self.plan.record(index, failed(&error));
        }
    }

    pub(super) fn failure(&self, mut error: VenueError) -> VenueError {
        error.presents = true;
        self.plan.attach(error)
    }

    pub(super) fn placed_count(&self, end: usize) -> usize {
        self.plan.outcomes().into_iter().take(end).filter(|row| {
            matches!(&row.result, ElementResult::Placed { order_ids } if !order_ids.is_empty())
        }).count()
    }

    pub(super) fn evidence(
        &self,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        evidence.write_plan(&self.plan);
    }
}
