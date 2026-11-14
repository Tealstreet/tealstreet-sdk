//! Passive outcomes for explicit cancellation targets, including earlier batches.
//! Recording these facts grants no dispatch, retry or reconciliation authority.
//! Unknown terminals retain diagnostics and emit one bounded outcome event.
use super::{BYBIT, Call, Row};
use crate::Pending;
use safe_cex_engine::classify::{self, Intent, Native, NativeCode, Released};
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, WritePlan};
use safe_cex_engine::{Command, CommandId, Cx, Level, Outcome, Response, VenueError};

const UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";
const CANCEL_MESSAGE: &str = "Bybit cancel was not confirmed";

pub(super) struct Cancellations {
    targets: Vec<String>,
    journal: WritePlan,
    remaining: Vec<usize>,
    active: Vec<usize>,
}

fn unknown_text(mut error: VenueError) -> VenueError {
    if error.outcome == Some(Outcome::Unknown) {
        error.message = if error.message == CANCEL_MESSAGE {
            UNKNOWN_TEXT.into()
        } else if error.message.starts_with(UNKNOWN_TEXT) {
            error.message
        } else {
            format!("{UNKNOWN_TEXT}. {}", error.message)
        };
        error.presents = true;
    }
    error
}

fn failed(error: VenueError) -> ElementResult {
    if error.outcome == Some(Outcome::Unknown) {
        ElementResult::Unknown {
            error: Box::new(unknown_text(error)),
        }
    } else {
        ElementResult::NotSent {
            error: Some(Box::new(error)),
        }
    }
}

impl Cancellations {
    pub(super) fn retain(
        &self,
        cx: &mut Cx<'_, Pending>,
        plan: &safe_cex_engine::request_context::Plan,
    ) {
        let _ = cx.retain_idle_plan_elements(plan, &self.journal);
    }

    pub(super) fn new(command: &Command, native_targets: &[&str]) -> Option<Self> {
        let ids = match command {
            Command::CancelOrders { ids, .. } => ids.clone(),
            Command::CancelOrdersWithSymbols { orders, .. } => {
                orders.iter().map(|order| order.id.clone()).collect()
            }
            _ => return None,
        };
        if native_targets.is_empty() {
            return None;
        }
        let remaining: Vec<usize> = ids
            .iter()
            .enumerate()
            .filter(|(_, id)| native_targets.contains(&id.as_str()))
            .map(|(index, _)| index)
            .collect();
        Some(Self {
            journal: WritePlan::new(
                remaining
                    .iter()
                    .map(|index| Element {
                        index: *index,
                        client_order_id: None,
                    })
                    .collect(),
            ),
            targets: ids,
            remaining,
            active: Vec::new(),
        })
    }

    pub(super) fn begin(&mut self, ids: &[String]) {
        self.active.clear();
        for id in ids {
            let position = self
                .remaining
                .iter()
                .position(|index| self.targets[*index] == *id)
                .expect("planned cancellation occurrence");
            self.active.push(self.remaining.remove(position));
        }
    }

    /// Absence satisfies cancellation intent but does not prove this cancel applied.
    pub(super) fn already_gone(&mut self, mut error: VenueError) {
        error.outcome = Some(Outcome::Unknown);
        for index in &self.active {
            let _ = self.journal.record(*index, failed(error.clone()));
        }
    }

    pub(super) fn accepted(&mut self, target: &str) {
        let _ = self.journal.record(
            self.active[0],
            ElementResult::Applied {
                effect: AppliedEffect::Cancel,
                target_order_id: Some(target.into()),
                position_key: None,
            },
        );
    }

    pub(super) fn rows(&mut self, ids: &[String], rows: &[Row], response: &Response<'_>) {
        for (position, (id, row)) in ids.iter().zip(rows).enumerate() {
            let result = match row {
                Row::Accepted(_) => ElementResult::Applied {
                    effect: AppliedEffect::Cancel,
                    target_order_id: Some(id.clone()),
                    position_key: None,
                },
                Row::Refused { code, message } => {
                    let native = Native {
                        code: NativeCode::Int(*code),
                        message,
                    };
                    let verdict = classify::verdict(
                        &BYBIT,
                        Intent::Cancel,
                        Released::Response(response),
                        Some(&native),
                    );
                    let mut error = classify::error(
                        verdict,
                        Intent::Cancel,
                        Released::Response(response),
                        Some(&native),
                        CANCEL_MESSAGE,
                    );
                    // Already-gone satisfies the intent, but does not prove this cancel.
                    if error.outcome.is_none() {
                        error.outcome = Some(Outcome::Unknown);
                    }
                    failed(error)
                }
            };
            let _ = self.journal.record(self.active[position], result);
        }
    }

    pub(super) fn unproven(
        &self,
        evidence: &mut safe_cex_engine::request_context::UnprovenEvidence,
    ) {
        let rows = self.journal.outcomes();
        let mut snapshot = WritePlan::new(rows.iter().map(|row| row.element.clone()).collect());
        for row in rows {
            let result = if self.active.contains(&row.element.index)
                && matches!(row.result, ElementResult::NotSent { error: None })
            {
                failed(VenueError::new(UNKNOWN_TEXT).outcome(Outcome::Unknown))
            } else {
                row.result
            };
            let _ = snapshot.record(row.element.index, result);
        }
        evidence.write_plan(&snapshot);
    }

    pub(super) fn failure(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        call: &Call,
        error: VenueError,
    ) -> VenueError {
        let Call::Cancel { .. } = call else {
            return self.journal.attach(error);
        };
        let mut error = unknown_text(error);
        for row in self.journal.outcomes() {
            if self.active.contains(&row.element.index)
                && matches!(row.result, ElementResult::NotSent { error: None })
            {
                let _ = self
                    .journal
                    .record(row.element.index, failed(error.clone()));
            }
        }
        if error.outcome == Some(Outcome::Unknown) {
            let outcomes = self.journal.outcomes();
            let applied = outcomes
                .iter()
                .filter(|row| matches!(row.result, ElementResult::Applied { .. }))
                .count();
            let unknown = outcomes
                .iter()
                .filter(|row| matches!(row.result, ElementResult::Unknown { .. }))
                .count();
            let attempted = outcomes
                .iter()
                .filter(|row| !matches!(row.result, ElementResult::NotSent { error: None }))
                .count();
            cx.event(
                Level::Warn,
                "order.outcome_unknown",
                UNKNOWN_TEXT,
                vec![
                    ("command", command.into()),
                    ("outcome", "unknown".into()),
                    ("attemptedCount", (attempted as f64).into()),
                    ("appliedCount", (applied as f64).into()),
                    ("unknownCount", (unknown as f64).into()),
                ],
            );
        }
        let outcomes = self.journal.outcomes();
        let failed_index = outcomes
            .iter()
            .find(|row| matches!(row.result, ElementResult::Unknown { .. }))
            .or_else(|| {
                outcomes
                    .iter()
                    .find(|row| matches!(row.result, ElementResult::NotSent { error: Some(_) }))
            })
            .map(|row| row.element.index);
        let partial = error.partial.get_or_insert_with(|| {
            Box::new(safe_cex_engine::Partial {
                order_ids: Vec::new(),
                total: None,
                failed_index: None,
            })
        });
        partial.total = Some(self.targets.len());
        partial.failed_index = failed_index;
        self.journal.attach(error)
    }
}
