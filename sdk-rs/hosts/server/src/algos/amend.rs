//! Typed DELEGATED amendments reuse the resident factories. An amendment is an
//! explicit stop-and-replace with a new id and a caller-supplied remaining size.
//! Never infer remaining quantity from a stale frame or restart an uncertain job.
use super::*;
use crate::order_actions::NukeReceipt;
use safe_cex_algo_twap::{TwapOptions, factory::TwapFactory};
use safe_cex_chaser::{ChaserBook, ChaserOptions};

struct Pending {
    id: String,
    options: ChaserOptions,
    receipt: Box<NukeReceipt>,
}

#[derive(Default)]
pub(super) struct Amendments {
    chases: Vec<Pending>,
    pub pending: Rc<Cell<usize>>,
}

fn finish(receipt: NukeReceipt, message: &str, outcome: &str) {
    receipt.finish(Err(
        serde_json::json!({"message":message,"outcome":outcome}),
    ));
}
fn completed(cx: &AlgoCx, op: &str, old: &str, new: Result<String, String>, receipt: NukeReceipt) {
    match new {
        Ok(id) => {
            cx.port.emit(
                "algo:amended",
                serde_json::json!({"op":op,"previousId":old,"id":id}).to_string(),
            );
            finish(
                receipt,
                "Replacement admitted; reconcile its account events",
                "unknown",
            );
        }
        Err(message) => finish(
            receipt,
            &format!("Old job stopped; replacement refused: {message}"),
            "unknown",
        ),
    }
}

impl Amendments {
    pub fn is_empty(&self) -> bool {
        self.chases.is_empty()
    }

    /// Called only inside the same admission lock as ordinary factory dispatch.
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        &mut self,
        op: &str,
        data: &str,
        receipt: Box<NukeReceipt>,
        cx: &AlgoCx,
        book: &Rc<ChaserBook>,
        twap: &TwapFactory,
        running: &Running,
    ) {
        let data: Value =
            serde_json::from_str(data).expect("typed amendment decoded before actor dispatch");
        let id = data["id"].as_str().expect("typed amendment id");
        if let Err(error) = receipt.guard().check() {
            finish(*receipt, error, "notSent");
            return;
        }
        if op == "chaser:amend" {
            let options: ChaserOptions =
                serde_json::from_value(data["options"].clone()).expect("typed chaser options");
            let old = book
                .active_chasers()
                .into_iter()
                .find(|row| row.id == id && !row.is_stopped);
            let Some(old) = old else {
                finish(*receipt, "Chaser is no longer active", "notSent");
                return;
            };
            // A slice's parent owns its progress and remainder policy.
            if old.twap_id.is_some()
                || old.symbol != options.symbol
                || old.side != options.side
                || old.reduce_only != options.reduce_only()
                || options.size <= 0.0
            {
                finish(
                    *receipt,
                    "Chaser identity cannot change; amend its parent TWAP for slices",
                    "notSent",
                );
                return;
            }
            if self.chases.len() >= safe_cex_chaser::MAX_CHASES_PER_ACCOUNT {
                finish(*receipt, "Too many pending chase replacements", "notSent");
                return;
            }
            // The book caps active chases. One pending replacement per active id
            // is possible: remove marks it stopped synchronously.
            if let Err(error) = receipt.guard().check().and_then(|()| {
                if running.io_allowed() && !cx.scope.is_disposed() {
                    Ok(())
                } else {
                    Err("Account authority retired")
                }
            }) {
                finish(*receipt, error, "notSent");
                return;
            }
            book.remove(id);
            self.chases.push(Pending {
                id: id.into(),
                options,
                receipt,
            });
            self.pending.set(self.chases.len());
        } else {
            let options: TwapOptions =
                serde_json::from_value(data["options"].clone()).expect("typed TWAP options");
            let old = twap
                .active_twaps(&options.account_name)
                .into_iter()
                .find(|row| row["id"] == id);
            let Some(old) = old else {
                finish(*receipt, "TWAP is no longer active", "notSent");
                return;
            };
            // TWAP stop leaves its already-created orders alone. Refuse before
            // touching the schedule unless there is no unsettled slice or child.
            let busy = twap.pending_placements() != 0
                || old.get("inFlight").is_some_and(|v| !v.is_null())
                || old["currentOrderIds"]
                    .as_array()
                    .is_none_or(|ids| !ids.is_empty())
                || book
                    .active_chasers()
                    .iter()
                    .any(|row| row.twap_id.as_deref() == Some(id));
            if busy {
                finish(
                    *receipt,
                    "TWAP slice or child order is unsettled; no amendment sent",
                    "notSent",
                );
                return;
            }
            if old["symbol"] != options.symbol()
                || old["side"] != serde_json::to_value(options.side).unwrap()
                || old["reduceOnly"] != options.reduce_only
            {
                finish(
                    *receipt,
                    "TWAP symbol, side and reduceOnly cannot change",
                    "notSent",
                );
                return;
            }
            // Validate business bounds before retiring the old schedule. The
            // factory still performs its own account/capacity validation.
            if options.size <= 0.0
                || options.duration <= 0.0
                || !(1..=safe_cex_algo_twap::MAX_ORDERS_COUNT).contains(&options.orders_count)
                || !(0.0..=100.0).contains(&options.randomness_percentage)
                || options
                    .timeout_pct
                    .is_some_and(|n| !(0.0..=100.0).contains(&n))
                || options.chaser_delay < 0.0
            {
                finish(*receipt, "Invalid replacement TWAP options", "notSent");
                return;
            }
            if let Err(error) = receipt.guard().check().and_then(|()| {
                if running.io_allowed() && !cx.scope.is_disposed() {
                    Ok(())
                } else {
                    Err("Account authority retired")
                }
            }) {
                finish(*receipt, error, "notSent");
                return;
            }
            if !twap.stop(id) {
                finish(*receipt, "TWAP is no longer active", "notSent");
                return;
            }
            let replacement = receipt
                .guard()
                .check()
                .map_err(str::to_owned)
                .and_then(|()| {
                    if running.io_allowed() && !cx.scope.is_disposed() {
                        twap.start(options)
                    } else {
                        Err("Account authority retired".into())
                    }
                });
            completed(cx, op, id, replacement, *receipt);
        }
    }

    /// The actor continues pumping command completions while chaser cleanup
    /// settles. No spawned continuation can reattribute this work to a new user.
    pub fn poll(&mut self, cx: &AlgoCx, book: &Rc<ChaserBook>, running: &Running) {
        let mut index = 0;
        while index < self.chases.len() {
            let pending = &self.chases[index];
            let guard = pending.receipt.guard();
            let stale = guard
                .check()
                .err()
                .or_else(|| (!running.io_allowed()).then_some("Router authority retired"));
            let progress = book.progress(&pending.id);
            if stale.is_none() && progress.is_some_and(|p| !p.ended) {
                index += 1;
                continue;
            }
            let pending = self.chases.remove(index);
            if let Some(message) = stale {
                finish(*pending.receipt, message, "unknown");
                continue;
            }
            if !progress.is_some_and(|p| p.ended && !p.order_may_be_open) {
                finish(
                    *pending.receipt,
                    "Old chaser cleanup is uncertain; no replacement started",
                    "unknown",
                );
                continue;
            }
            // Re-enter the release-proof/drain admission lock for the final
            // synchronous start, and recheck the captured registration/deadline.
            let admission = running.user.get().map(|user| user.admit());
            if matches!(admission, Some(Err(_))) {
                finish(
                    *pending.receipt,
                    "Server draining; old chaser stopped, no replacement started",
                    "unknown",
                );
                continue;
            }
            if let Err(message) = guard.check() {
                finish(*pending.receipt, message, "unknown");
                continue;
            }
            if !running.io_allowed() || cx.scope.is_disposed() {
                finish(
                    *pending.receipt,
                    "Account authority retired; no replacement started",
                    "unknown",
                );
                continue;
            }
            completed(
                cx,
                "chaser:amend",
                &pending.id,
                book.start(pending.options),
                *pending.receipt,
            );
        }
        self.pending.set(self.chases.len());
    }

    pub fn cancel(&mut self, id: Option<&str>) {
        let mut index = 0;
        while index < self.chases.len() {
            if id.is_some_and(|id| self.chases[index].id != id) {
                index += 1;
                continue;
            }
            let pending = self.chases.remove(index);
            finish(
                *pending.receipt,
                "Replacement canceled by a later stop; reconcile old job cleanup",
                "unknown",
            );
        }
        self.pending.set(self.chases.len());
    }
}
