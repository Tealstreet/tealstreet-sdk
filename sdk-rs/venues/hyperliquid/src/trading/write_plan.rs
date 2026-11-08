//! Immutable native writes; custody and physical transport remain with the driver.
use super::*;
use safe_cex_engine::write_plan::{Element, ElementResult, PlannedWrite, WritePlan};

#[derive(Clone)]
pub(super) struct PreparedWrite {
    action: Action,
    effect: Effect,
    native: Option<NativePlacement>,
}

fn validate(write: &PreparedWrite) -> Result<(), VenueError> {
    if let Some(native) = &write.native {
        validate_placement(native)?;
    }
    Ok(())
}

pub(super) fn failure(plan: &WritePlan<PreparedWrite>) -> Option<VenueError> {
    let mut refusal = None;
    for element in plan.outcomes() {
        match element.result {
            ElementResult::Unknown { error } => return Some(*error),
            ElementResult::NotSent { error: Some(error) } if refusal.is_none() => {
                refusal = Some(*error)
            }
            _ => {}
        }
    }
    refusal
}

impl Hyperliquid {
    pub(super) fn finalize_writes(&mut self, cx: &mut Cx<'_, Pending>) -> Result<(), VenueError> {
        let Some(job) = self
            .trading
            .active
            .as_ref()
            .filter(|job| job.writes.is_none())
        else {
            return Ok(());
        };
        let steps = job.steps.clone();
        let mut writes = Vec::new();
        let mut index = 0;
        for step in steps {
            let (action, effect) = self.prepare_write(cx, step).map_err(not_sent)?;
            let native = match (&action, &effect) {
                (
                    Action::Order {
                        orders, grouping, ..
                    },
                    Effect::Place(records),
                ) => {
                    let record = records.first().ok_or_else(|| not_sent("Empty placement"))?;
                    let market = cx
                        .store
                        .market(&record.order.symbol)
                        .ok_or_else(|| not_sent("Market not found"))?;
                    Some(NativePlacement::new(market, orders.clone(), grouping))
                }
                _ => None,
            };
            let prepared = PreparedWrite {
                action,
                effect,
                native,
            };
            let clients = match &prepared.effect {
                Effect::Place(records) => records
                    .iter()
                    .map(|record| record.order.client_order_id.clone())
                    .collect(),
                Effect::Cancel(orders) => orders
                    .iter()
                    .map(|order| order.client_order_id.clone())
                    .collect(),
                Effect::ScheduleCancel | Effect::Leverage { .. } => vec![None],
            };
            let elements = clients
                .into_iter()
                .map(|client_order_id| {
                    let element = Element {
                        index,
                        client_order_id,
                    };
                    index += 1;
                    element
                })
                .collect::<Vec<_>>();
            if elements.is_empty() {
                continue;
            }
            writes.push(PlannedWrite {
                payload: prepared,
                elements,
            });
        }
        let plan = Some(WritePlan::validate(writes, validate)?);
        if let Some(job) = self.trading.active.as_mut() {
            job.writes = plan;
        }
        Ok(())
    }

    pub(super) fn next_planned_write(
        &mut self,
        cx: &mut Cx<'_, Pending>,
    ) -> Result<Option<(Action, Effect)>, VenueError> {
        let job = self
            .trading
            .active
            .as_mut()
            .ok_or_else(|| not_sent("No active write plan"))?;
        let step = job
            .steps
            .pop_front()
            .ok_or_else(|| not_sent("No planned step"))?;
        if matches!(&step, Step::Cancel(orders) if orders.is_empty()) {
            return Ok(None);
        }
        let (index, write) = job
            .writes
            .as_mut()
            .and_then(WritePlan::next_write)
            .ok_or_else(|| not_sent("Write plan stopped"))?;
        job.pending_write = Some(index);
        let prepared = write.payload.clone();
        // Recheck live inputs without replacing the validated wire bytes.
        let (current, _) = self.prepare_write(cx, step).map_err(not_sent)?;
        let frozen_bytes =
            serde_json::to_vec(&prepared.action).map_err(|e| not_sent(e.to_string()))?;
        let current_bytes = serde_json::to_vec(&current).map_err(|e| not_sent(e.to_string()))?;
        if current_bytes != frozen_bytes {
            return Err(not_sent(
                "Hyperliquid live inputs changed; frozen write not sent",
            ));
        }
        if matches!(&prepared.effect, Effect::Cancel(orders) if orders.is_empty()) {
            return Ok(None);
        }
        Ok(Some((prepared.action, prepared.effect)))
    }

    pub(super) fn fail_pending_write(&mut self, error: VenueError) -> VenueError {
        if let Some(job) = self.trading.active.as_mut()
            && let Some(index) = job.pending_write.take()
            && let Some(plan) = job.writes.as_mut()
            && let Err(invalid) = plan.fail(index, error.clone())
        {
            return unproven(format!(
                "Invalid write-plan failure: {invalid}; {}",
                error.message
            ));
        }
        error
    }
}
