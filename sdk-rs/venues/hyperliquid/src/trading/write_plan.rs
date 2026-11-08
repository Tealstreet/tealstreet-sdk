//! Passive per-write journals: native planning, dispatch and recovery stay in the driver.
use super::*;
use safe_cex_engine::write_plan::{
    Element, ElementOutcomes, ElementResult, PlannedWrite, WritePlan,
};

#[derive(Default)]
pub(super) struct Journal {
    writes: Vec<WritePlan<()>>,
    next: usize,
    pending: Option<usize>,
}

fn clients(step: &Step) -> Vec<Option<String>> {
    match step {
        Step::Place(opts) => {
            let mut clients = vec![opts.client_order_id.clone()];
            if opts.kind == OrderType::Limit {
                for price in [opts.stop_loss, opts.take_profit].into_iter().flatten() {
                    if price > 0. {
                        clients.push(None);
                    }
                }
            }
            clients
        }
        Step::Cancel(orders) => orders
            .iter()
            .map(|order| order.client_order_id.clone())
            .collect(),
        Step::Restore | Step::ScheduleCancel { .. } | Step::Leverage { .. } => vec![None],
    }
}

fn write(clients: Vec<Option<String>>) -> Result<WritePlan<()>, VenueError> {
    let elements = clients
        .into_iter()
        .enumerate()
        .map(|(index, client_order_id)| Element {
            index,
            client_order_id,
        })
        .collect();
    // TS formats each write as it becomes due; this journal adds no native gate.
    WritePlan::validate(
        vec![PlannedWrite {
            payload: (),
            elements,
        }],
        |_| Ok(()),
    )
}

impl Journal {
    pub(super) fn new(steps: &VecDeque<Step>) -> Result<Self, VenueError> {
        let writes = steps
            .iter()
            .map(clients)
            .filter(|clients| !clients.is_empty())
            .map(write)
            .collect::<Result<_, _>>()?;
        Ok(Self {
            writes,
            next: 0,
            pending: None,
        })
    }

    pub(super) fn begin(&mut self, step: &Step) -> Result<(), VenueError> {
        let clients = clients(step);
        if clients.is_empty() {
            return Ok(());
        }
        // A legacy compensating write gets its own journal after the failed write.
        if self.next == self.writes.len() {
            self.writes.push(write(clients)?);
        }
        let plan = self
            .writes
            .get_mut(self.next)
            .ok_or_else(|| unproven("Missing write journal"))?;
        plan.next_write()
            .ok_or_else(|| unproven("Write journal already dispatched"))?;
        self.pending = Some(self.next);
        self.next += 1;
        Ok(())
    }

    pub(super) fn prepared(&mut self, effect: &Effect) -> Result<(), VenueError> {
        let Some(index) = self.pending else {
            return Ok(());
        };
        let clients = match effect {
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
        let mut plan = write(clients)?;
        plan.next_write()
            .ok_or_else(|| unproven("Empty write journal"))?;
        self.writes[index] = plan;
        Ok(())
    }

    pub(super) fn fail(&mut self, error: VenueError) -> Result<(), VenueError> {
        if let Some(index) = self.pending {
            self.writes[index]
                .fail(0, error)
                .map_err(|e| unproven(e.to_string()))?;
            self.pending = None;
        }
        Ok(())
    }

    pub(super) fn settle(&mut self, results: Vec<ElementResult>) -> Result<(), VenueError> {
        if let Some(index) = self.pending {
            self.writes[index]
                .settle(0, results)
                .map_err(|e| unproven(e.to_string()))?;
            self.pending = None;
        }
        Ok(())
    }

    pub(super) fn pending(&self) -> bool {
        self.pending.is_some()
    }

    fn outcomes(&self) -> Vec<safe_cex_engine::write_plan::ElementOutcome> {
        self.writes
            .iter()
            .flat_map(WritePlan::outcomes)
            .enumerate()
            .map(|(index, mut outcome)| {
                outcome.element.index = index;
                outcome
            })
            .collect()
    }

    pub(super) fn failure(&self, mut error: VenueError) -> VenueError {
        if !self.writes.is_empty() {
            error.elements = Some(Box::new(ElementOutcomes(self.outcomes())));
        }
        error
    }

    pub(super) fn evidence(
        &self,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        let outcomes = self.outcomes();
        if outcomes.is_empty() {
            return;
        }
        let elements = outcomes
            .iter()
            .map(|outcome| outcome.element.clone())
            .collect();
        let projection = WritePlan::validate(
            vec![PlannedWrite {
                payload: (),
                elements,
            }],
            |_| Ok(()),
        );
        let projected = projection.and_then(|mut plan| {
            plan.next_write()
                .ok_or_else(|| unproven("Empty evidence journal"))?;
            plan.settle(
                0,
                outcomes.into_iter().map(|outcome| outcome.result).collect(),
            )
            .map_err(|e| unproven(e.to_string()))?;
            Ok(plan)
        });
        match projected {
            Ok(plan) => evidence.write_plan(&plan),
            Err(error) => evidence.venue_message(&error.message),
        }
    }
}
