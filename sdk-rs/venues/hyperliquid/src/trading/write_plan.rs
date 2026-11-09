//! Passive outcomes: native planning and dispatch remain entirely in the driver.
use super::*;
use safe_cex_engine::write_plan::{Element, ElementOutcomes, ElementResult, WritePlan};

#[derive(Default)]
pub(super) struct Journal {
    writes: Vec<WritePlan>,
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
        Step::ScheduleCancel { .. } | Step::Leverage { .. } | Step::Restore => vec![None],
    }
}

fn write(clients: Vec<Option<String>>) -> WritePlan {
    WritePlan::new(
        clients
            .into_iter()
            .enumerate()
            .map(|(index, client_order_id)| Element {
                index,
                client_order_id,
            })
            .collect(),
    )
}

impl Journal {
    pub(super) fn new(steps: &VecDeque<Step>) -> Self {
        Self {
            writes: steps
                .iter()
                .map(clients)
                .filter(|c| !c.is_empty())
                .map(write)
                .collect(),
            next: 0,
            pending: None,
        }
    }

    pub(super) fn begin(&mut self, step: &Step) {
        if !clients(step).is_empty() {
            // The driver's one-shot protective restore is planned after a refusal.
            if self.next == self.writes.len() {
                self.writes.push(write(clients(step)));
            }
            self.pending = Some(self.next);
            self.next += 1;
        }
    }

    pub(super) fn prepared(&mut self, effect: &Effect) {
        let Some(index) = self.pending else {
            return;
        };
        let clients = match effect {
            Effect::Place(records) => records
                .iter()
                .map(|r| r.order.client_order_id.clone())
                .collect(),
            Effect::Cancel(orders) => orders.iter().map(|o| o.client_order_id.clone()).collect(),
            Effect::ScheduleCancel | Effect::Leverage { .. } => vec![None],
        };
        self.writes[index] = write(clients);
        let count = self.writes[index].outcomes().len();
        self.record(
            (0..count)
                .map(|_| ElementResult::Unknown {
                    error: Box::new(unproven("Hyperliquid write awaiting native outcome")),
                })
                .collect(),
        );
    }

    fn record(&mut self, results: Vec<ElementResult>) {
        if let Some(index) = self.pending {
            // Indices come from this journal itself; reporting cannot gate dispatch.
            for (row, result) in self.writes[index].outcomes().into_iter().zip(results) {
                let _ = self.writes[index].record(row.element.index, result);
            }
        }
    }

    pub(super) fn record_failure(&mut self, error: VenueError) {
        if let Some(index) = self.pending {
            let result = if matches!(error.outcome, Some(Outcome::NotSent | Outcome::Rejected)) {
                ElementResult::NotSent {
                    error: Some(Box::new(error)),
                }
            } else {
                ElementResult::Unknown {
                    error: Box::new(error),
                }
            };
            self.record(vec![result; self.writes[index].outcomes().len()]);
            self.pending = None;
        }
    }

    pub(super) fn settle(&mut self, results: Vec<ElementResult>) {
        self.record(results);
        self.pending = None;
    }

    pub(super) fn settle_native(&mut self, journal: WritePlan) {
        if let Some(index) = self.pending.take() {
            self.writes[index] = journal;
        }
    }

    pub(super) fn pending(&self) -> bool {
        self.pending.is_some()
    }

    fn outcomes(&self) -> Vec<safe_cex_engine::write_plan::ElementOutcome> {
        self.writes
            .iter()
            .flat_map(WritePlan::outcomes)
            .enumerate()
            .map(|(index, mut row)| {
                row.element.index = index;
                if let ElementResult::Unknown { error } = &mut row.result {
                    error.message = ENGINE_UNKNOWN_TEXT.into();
                }
                row
            })
            .collect()
    }

    pub(super) fn failure(&self, mut error: VenueError) -> VenueError {
        if !self.writes.is_empty() {
            error.elements = Some(Box::new(ElementOutcomes(self.outcomes())));
            error.message = terminal_failure_text(&error);
        }
        error
    }

    pub(super) fn report_unknown(
        &self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        message: &str,
    ) {
        let outcomes = self.outcomes();
        let unknown = outcomes
            .iter()
            .filter(|row| matches!(row.result, ElementResult::Unknown { .. }))
            .map(|row| row.element.index.to_string())
            .collect::<Vec<_>>();
        if unknown.is_empty() {
            return;
        }
        let placed = outcomes
            .iter()
            .filter_map(|row| match &row.result {
                ElementResult::Placed { order_ids } => Some(order_ids.iter()),
                _ => None,
            })
            .flatten()
            .map(String::as_str)
            .collect::<Vec<_>>();
        cx.event(
            safe_cex_engine::log::Level::Warn,
            "order.outcome_unknown",
            message,
            vec![
                ("command", command.to_string().into()),
                ("commandId", command.into()),
                ("outcome", "unknown".into()),
                ("placedOrderIds", placed.join(",").into()),
                ("unknownIndices", unknown.join(",").into()),
            ],
        );
    }

    pub(super) fn evidence(
        &self,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        let outcomes = self.outcomes();
        let mut plan = WritePlan::new(outcomes.iter().map(|row| row.element.clone()).collect());
        for row in outcomes {
            let _ = plan.record(row.element.index, row.result);
        }
        evidence.write_plan(&plan);
    }
}
