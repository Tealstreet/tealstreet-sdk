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
        Some(Self {
            journal: WritePlan::new(
                ids.iter()
                    .enumerate()
                    .filter(|(_, id)| native_targets.contains(&id.as_str()))
                    .map(|(index, _)| Element {
                        index,
                        client_order_id: None,
                    })
                    .collect(),
            ),
            targets: ids,
        })
    }

    fn record(&mut self, target: &str, result: ElementResult) {
        for (index, id) in self.targets.iter().enumerate() {
            if id == target {
                let _ = self.journal.record(index, result.clone());
            }
        }
    }

    pub(super) fn accepted(&mut self, target: &str) {
        self.record(
            target,
            ElementResult::Applied {
                effect: AppliedEffect::Cancel,
                target_order_id: Some(target.into()),
                position_key: None,
            },
        );
    }

    pub(super) fn rows(&mut self, ids: &[String], rows: &[Row], response: &Response<'_>) {
        for (id, row) in ids.iter().zip(rows) {
            match row {
                Row::Accepted(_) => self.accepted(id),
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
                    self.record(id, failed(error));
                }
            }
        }
    }

    pub(super) fn failure(
        &mut self,
        cx: &mut Cx<'_, Pending>,
        command: CommandId,
        call: &Call,
        error: VenueError,
    ) -> VenueError {
        let Call::Cancel { ids } = call else {
            return self.journal.attach(error);
        };
        let error = unknown_text(error);
        for row in self.journal.outcomes() {
            if ids.contains(&self.targets[row.element.index])
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
        self.journal.attach(error)
    }
}
