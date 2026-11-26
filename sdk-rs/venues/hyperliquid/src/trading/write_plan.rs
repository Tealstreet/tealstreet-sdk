//! Passive outcomes: native planning and dispatch remain entirely in the driver.
use super::*;
use safe_cex_engine::write_plan::{Element, ElementOutcomes, ElementResult, LegRole, WritePlan};

#[derive(Default)]
pub(super) struct Journal {
    writes: Vec<WritePlan>,
    next: usize,
    pending: Option<usize>,
}

type Row = (Option<String>, Option<LegRole>);

fn clients(step: &Step) -> Vec<Row> {
    let plain = |id: &Option<String>| (id.clone(), None);
    match step {
        Step::Place(opts) => {
            let mut clients = vec![(opts.client_order_id.clone(), None)];
            // Mirrors `wires`: limit and market entries carry their legs (AGT-117).
            if matches!(opts.kind, OrderType::Limit | OrderType::Market) {
                for (price, role) in [
                    (opts.stop_loss, LegRole::StopLoss),
                    (opts.take_profit, LegRole::TakeProfit),
                ] {
                    if price.is_some_and(|price| price > 0.) {
                        clients.push((None, Some(role)));
                    }
                }
            }
            // AGT-214: one grouped action, but each leg is its own reported row.
            if clients.len() > 1 {
                clients[0].1 = Some(LegRole::Entry);
            }
            clients
        }
        Step::Places(orders) => orders
            .iter()
            .map(|order| plain(&order.client_order_id))
            .collect(),
        Step::Cancel(orders) => orders
            .iter()
            .map(|order| plain(&order.client_order_id))
            .collect(),
        Step::ScheduleCancel { .. } | Step::Leverage { .. } | Step::Restore => vec![(None, None)],
    }
}

fn write(clients: Vec<Row>) -> WritePlan {
    WritePlan::new(
        clients
            .into_iter()
            .enumerate()
            .map(|(index, (client_order_id, role))| Element {
                index,
                client_order_id,
                role,
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
        let clients: Vec<_> = match effect {
            Effect::Place(records) => records
                .iter()
                .map(|r| r.order.client_order_id.clone())
                .collect(),
            Effect::Cancel(orders) => orders.iter().map(|o| o.client_order_id.clone()).collect(),
            Effect::ScheduleCancel | Effect::Leverage { .. } => vec![None],
        };
        // The prepared rows keep the roles planned from the step, row for row.
        let roles: Vec<_> = self.writes[index]
            .outcomes()
            .iter()
            .map(|row| row.element.role)
            .collect();
        self.writes[index] = write(
            clients
                .into_iter()
                .enumerate()
                .map(|(row, client)| (client, roles.get(row).copied().flatten()))
                .collect(),
        );
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

    pub(super) fn report_unknown(&self, cx: &mut Cx<'_, Pending>, command: CommandId) {
        let outcomes = self.outcomes();
        let unknown = outcomes
            .iter()
            .filter(|row| matches!(row.result, ElementResult::Unknown { .. }))
            .map(|row| row.element.index);
        if unknown.clone().next().is_none() {
            return;
        }
        let placed = outcomes
            .iter()
            .flat_map(|row| match &row.result {
                ElementResult::Placed { order_ids } => order_ids.as_slice(),
                _ => &[],
            })
            .map(String::as_str);
        let event = super::unknown_telemetry::event(command, placed, unknown);
        cx.event(event.level, event.code, event.message, event.fields);
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
