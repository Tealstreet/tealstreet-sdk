//! `TwapInstance.ts` as a clock-driven state machine: timers are deadlines the
//! host wakes on, and engine work leaves as [`TwapEffect`]s.

use crate::math::{add, lodash_random, multiply, subtract};
use crate::types::{
    InFlightSlice, OrderSide, SliceAction, TwapOptions, TwapOrigin, TwapState, TwapStatus,
};
use crate::{ChaserRequest, OrderKind, TwapEffect, TwapEnv, TwapMarketView, TwapOrderRequest};
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, PlannedWrite, WritePlan};
use serde_json::Map;

/// The chaser a slice spawns gives up after this many reprices, as in TS.
const SLICE_CHASER_MAX_ATTEMPTS: u32 = 1000;
/// How often a timed-out slice looks whether its chase has settled.
const SLICE_SETTLE_POLL_MS: f64 = 250.0;

/// A slice placement awaiting [`TwapInstance::placement_settled`].
#[derive(Clone, Debug)]
struct PendingSlice {
    token: u64,
    write_index: usize,
    priced_limit: bool,
    executed_before: u32,
    next_order_at: f64,
    wait_ms: f64,
}

#[derive(Clone, Debug)]
struct SliceTimeout {
    due_at: f64,
    chaser_id: String,
    /// The chase was told to stop: its remainder goes to market once it has settled.
    market_when_settled: bool,
}

#[derive(Debug)]
pub struct TwapInstance {
    pub id: String,
    pub options: TwapOptions,
    pub state: TwapState,
    tracked_chaser_ids: Vec<String>,
    tracked_limit_order_ids: Vec<String>,
    slice_due_at: Option<f64>,
    slice_timeouts: Vec<SliceTimeout>,
    pending: Vec<PendingSlice>,
    next_token: u64,
    state_changed: bool,
    prepared: bool,
    planned_writes: Vec<crate::plan::PlannedOrder>,
    write_plan: Option<WritePlan<crate::plan::PlannedOrder>>,
    terminal_error: Option<VenueError>,
    outcomes_reported: bool,
}

impl TwapInstance {
    pub fn new(
        id: String,
        options: TwapOptions,
        state: Option<TwapState>,
        env: &mut dyn TwapEnv,
    ) -> Self {
        let state = state.unwrap_or_else(|| new_state(&options, env));
        Self {
            id,
            options,
            state,
            tracked_chaser_ids: Vec::new(),
            tracked_limit_order_ids: Vec::new(),
            slice_due_at: None,
            slice_timeouts: Vec::new(),
            pending: Vec::new(),
            next_token: 0,
            state_changed: false,
            prepared: false,
            planned_writes: Vec::new(),
            write_plan: None,
            terminal_error: None,
            outcomes_reported: false,
        }
    }

    /// Direct limit orders this TWAP placed (`currentOrderIds`).
    pub fn current_order_ids(&self) -> &[String] {
        &self.tracked_limit_order_ids
    }

    pub fn tracked_chaser_ids(&self) -> &[String] {
        &self.tracked_chaser_ids
    }

    /// Every admitted child, including explicit unknown notional facts.
    pub fn planned_writes(&self) -> &[crate::plan::PlannedOrder] {
        &self.planned_writes
    }

    pub(crate) fn report_outcomes(&mut self, effects: &mut Vec<TwapEffect>) {
        if self.state.status != TwapStatus::Completed
            || self.has_outstanding_work()
            || self.outcomes_reported
        {
            return;
        }
        if let Some(plan) = &self.write_plan {
            effects.push(TwapEffect::Outcome {
                twap_id: self.id.clone(),
                elements: plan.outcomes(),
                error: self.terminal_error.clone().map(Box::new),
            });
            self.outcomes_reported = true;
        }
    }

    fn stop_plan(&mut self, error: VenueError) {
        self.terminal_error = Some(match &mut self.write_plan {
            Some(plan) => plan.failure(error),
            None => error,
        });
        self.terminate();
    }

    /// The earliest timer deadline, if any timer is armed.
    pub fn wake_at(&self) -> Option<f64> {
        self.slice_timeouts
            .iter()
            .map(|timeout| timeout.due_at)
            .chain(self.slice_due_at)
            .min_by(f64::total_cmp)
    }

    /// True while work started before completion can still produce effects.
    pub fn has_outstanding_work(&self) -> bool {
        !self.pending.is_empty() || self.wake_at().is_some()
    }

    pub(crate) fn mark_state_changed(&mut self) {
        self.state_changed = true;
    }

    pub(crate) fn take_state_changed(&mut self) -> bool {
        std::mem::take(&mut self.state_changed)
    }

    /// Materialize every wire quantity, carrying dust before checking any slice.
    pub(crate) fn prepare(&mut self, view: &dyn TwapMarketView) -> Result<(), VenueError> {
        if self.prepared {
            let released = self
                .planned_writes
                .len()
                .saturating_sub(self.state.orders_sizes.len());
            for write in &self.planned_writes[released..] {
                let current = view.prepare_order(write.request.clone())?;
                crate::plan::validate_write(&current)?;
                if current.request != write.request {
                    return Err(crate::plan::invalid(
                        "TWAP prepared wire values no longer match native rules",
                    ));
                }
            }
            return Ok(());
        }
        let mut sizes = self.state.orders_sizes.clone();
        let step = view
            .order_rules(&self.request(sizes.first().copied().unwrap_or(0.0), view))
            .and_then(|rules| rules.amount_step)
            .filter(|step| step.is_finite() && *step > 0.0);
        if !self.prepared
            && let Some(step) = step
        {
            let original = sizes.iter().sum::<f64>();
            let mut carry = 0.0;
            for size in &mut sizes {
                let raw = add(*size, carry);
                let whole = multiply((raw / step + 1e-9).floor(), step);
                carry = subtract(raw, whole);
                *size = whole;
            }
            if (sizes.iter().sum::<f64>() - original).abs() > step * 1e-8 {
                return Err(crate::plan::invalid(
                    "TWAP size cannot be conserved on the native step",
                ));
            }
        }
        let mut writes = Vec::with_capacity(sizes.len());
        for size in &sizes {
            let mut write = view.prepare_order(self.request(*size, view))?;
            if !self.options.use_chaser_orders {
                write.request.client_order_id = view.client_order_id(&write.request);
            }
            writes.push(write);
        }
        self.write_plan = Some(WritePlan::validate(
            writes
                .iter()
                .enumerate()
                .map(|(index, write)| PlannedWrite {
                    payload: write.clone(),
                    elements: vec![Element {
                        index,
                        client_order_id: write.request.client_order_id.clone(),
                    }],
                })
                .collect(),
            crate::plan::validate_write,
        )?);
        self.planned_writes = writes;
        self.state.orders_sizes = sizes;
        self.prepared = true;
        Ok(())
    }

    fn request(&self, amount: f64, view: &dyn TwapMarketView) -> TwapOrderRequest {
        let symbol = self.options.symbol().to_owned();
        let price = if self.options.use_limit_orders || self.options.use_chaser_orders {
            view.ticker(&symbol)
                .and_then(|ticker| match self.options.side {
                    OrderSide::Buy => truthy(ticker.bid).or(truthy(ticker.last)),
                    OrderSide::Sell => truthy(ticker.ask).or(truthy(ticker.last)),
                })
        } else {
            None
        };
        TwapOrderRequest {
            symbol,
            side: self.options.side,
            kind: if price.is_some() {
                OrderKind::Limit
            } else {
                OrderKind::Market
            },
            price,
            amount,
            reduce_only: self.options.reduce_only,
            client_order_id: None,
        }
    }

    fn set_status(&mut self, status: TwapStatus) {
        self.state.status = status;
        self.state_changed = true;
    }

    fn slice_interval_ms(&self) -> f64 {
        self.options.duration / f64::from(self.options.orders_count) * 60.0 * 1000.0
    }

    pub fn start(
        &mut self,
        env: &mut dyn TwapEnv,
        view: &dyn TwapMarketView,
        effects: &mut Vec<TwapEffect>,
    ) {
        if matches!(
            self.state.status,
            TwapStatus::Completed | TwapStatus::Paused
        ) {
            return;
        }
        if let Err(error) = self.prepare(view) {
            effects.push(TwapEffect::Log {
                message: error.message.clone(),
            });
            self.stop_plan(error);
            return;
        }
        let order_size =
            (!self.state.orders_sizes.is_empty()).then(|| self.state.orders_sizes.remove(0));
        let randomness = self.options.randomness_percentage;
        let rand = multiply(
            lodash_random(-randomness, randomness, env.random_unit()),
            0.01,
        );
        let wait_ms = self.slice_interval_ms();
        let wait_ms = wait_ms + wait_ms * rand;
        let now = env.now_ms();
        let next_order_at = now + wait_ms;

        if self.state.status == TwapStatus::Idle {
            self.state.status = TwapStatus::Running;
            self.state.next_order_at = now;
            self.state.started_at = Some(now);
            self.state_changed = true;
        }

        let executed_before = self.state.orders_executed;
        let Some(order_size) = order_size else {
            self.finish_slice(executed_before, next_order_at, wait_ms, now);
            return;
        };

        let Some((write_index, write)) = self.write_plan.as_mut().and_then(WritePlan::next_write)
        else {
            self.stop_plan(crate::plan::invalid(
                "TWAP has no available validated write",
            ));
            return;
        };
        let request = write.payload.request.clone();

        if self.options.use_chaser_orders {
            let started = self.start_slice_chaser(order_size, env, view, effects);
            let settled = match &mut self.write_plan {
                Some(plan) if started => plan.settle(
                    write_index,
                    vec![ElementResult::Applied {
                        effect: AppliedEffect::DeferredChild,
                        target_order_id: None,
                        position_key: None,
                    }],
                ),
                Some(plan) => plan.fail(write_index, crate::plan::invalid("chaser refused")),
                None => return,
            };
            if let Err(error) = settled {
                self.stop_plan(VenueError::new(error.to_string()).outcome(Outcome::Unknown));
                return;
            }
            if !started {
                self.stop_plan(crate::plan::invalid("chaser refused"));
                self.report_shortfall(effects);
                return;
            }
            self.finish_slice(
                executed_before + u32::from(started),
                next_order_at,
                wait_ms,
                now,
            );
            self.report_shortfall(effects);
            return;
        }

        let priced = request.kind == OrderKind::Limit;
        let token = self.next_token;
        self.next_token += 1;
        self.pending.push(PendingSlice {
            token,
            write_index,
            priced_limit: priced,
            executed_before,
            next_order_at,
            wait_ms,
        });
        // Persisted before the order goes out: a host that dies before the slice
        // is counted leaves a restore something to look up instead of re-placing.
        self.state.in_flight = Some(InFlightSlice {
            size: order_size,
            client_order_id: request.client_order_id.clone(),
        });
        self.state_changed = true;
        effects.push(TwapEffect::PlaceOrder {
            twap_id: self.id.clone(),
            settle_token: Some(token),
            request,
        });
    }

    /// A restored slice a previous host was placing when it died that the
    /// restoring host did not reconcile: counted, never placed again (TS
    /// `reconcileInFlight` when the order cannot be confirmed).
    pub(crate) fn count_unconfirmed_in_flight(&mut self, effects: &mut Vec<TwapEffect>) {
        let Some(in_flight) = self.state.in_flight.take() else {
            return;
        };
        self.state.orders_executed += 1;
        self.state_changed = true;
        effects.push(TwapEffect::Log {
            message: format!(
                "TWAP {}: a {} slice was being placed when the page closed and could not be confirmed; it is not placed again. Check your orders.",
                self.options.symbol(),
                in_flight.size
            ),
        });
    }

    /// The engine's answer to a slice [`TwapEffect::PlaceOrder`]: the order ids,
    /// or the classified native failure. Any failure seals the remaining plan.
    pub fn placement_settled(
        &mut self,
        token: u64,
        outcome: Result<Vec<String>, VenueError>,
        env: &mut dyn TwapEnv,
        effects: &mut Vec<TwapEffect>,
    ) {
        let Some(index) = self
            .pending
            .iter()
            .position(|pending| pending.token == token)
        else {
            return;
        };
        let pending = self.pending.remove(index);
        // `finish_slice` reports the cleared marker; a stopped TWAP is no longer persisted.
        if self.pending.is_empty() {
            self.state.in_flight = None;
        }
        let mut executed = pending.executed_before + 1;
        let outcome = match outcome {
            Ok(ids) if ids.is_empty() || ids.iter().any(String::is_empty) => {
                Err(VenueError::new("order ids unknown, the order may be live")
                    .outcome(Outcome::Unknown))
            }
            other => other,
        };
        let settlement = match &mut self.write_plan {
            Some(plan) => match &outcome {
                Ok(ids) => plan.settle(
                    pending.write_index,
                    vec![ElementResult::Placed {
                        order_ids: ids.clone(),
                    }],
                ),
                Err(error) => plan.fail(pending.write_index, error.clone()),
            },
            None => return,
        };
        if let Err(error) = settlement {
            self.stop_plan(VenueError::new(error.to_string()).outcome(Outcome::Unknown));
            return;
        }
        match outcome {
            Ok(order_ids) => {
                if pending.priced_limit && !order_ids.is_empty() {
                    self.tracked_limit_order_ids.extend(order_ids);
                }
            }
            Err(failure) => {
                if matches!(failure.outcome, Some(Outcome::NotSent | Outcome::Rejected))
                    && failure.partial.is_none()
                {
                    executed = pending.executed_before;
                }
                effects.push(TwapEffect::Log {
                    message: format!(
                        "TWAP {} slice failed: {}",
                        self.options.symbol(),
                        failure.message
                    ),
                });
                self.state.orders_executed = executed;
                self.stop_plan(failure);
                self.report_shortfall(effects);
                return;
            }
        }
        // terminate() may have run while the placement was in flight.
        if self.state.status == TwapStatus::Completed {
            return;
        }
        let now = env.now_ms();
        self.finish_slice(executed, pending.next_order_at, pending.wait_ms, now);
        self.report_shortfall(effects);
    }

    /// `Completed` alone must not read as done: say how few slices were placed.
    fn report_shortfall(&self, effects: &mut Vec<TwapEffect>) {
        let state = &self.state;
        if state.status == TwapStatus::Completed && state.orders_executed < state.orders_count {
            effects.push(TwapEffect::Log {
                message: format!(
                    "TWAP {} ended with {} of {} slices placed",
                    self.options.symbol(),
                    state.orders_executed,
                    state.orders_count
                ),
            });
        }
    }

    fn finish_slice(&mut self, orders_executed: u32, next_order_at: f64, wait_ms: f64, now: f64) {
        self.state.orders_executed = orders_executed;
        self.state_changed = true;
        if self.state.orders_sizes.is_empty() {
            self.state.status = TwapStatus::Completed;
            return;
        }
        self.state.next_order_at = next_order_at;
        self.slice_due_at = Some(now + wait_ms);
    }

    /// False when the chaser runtime refused it: nothing runs for this slice.
    fn start_slice_chaser(
        &mut self,
        size: f64,
        env: &mut dyn TwapEnv,
        view: &dyn TwapMarketView,
        effects: &mut Vec<TwapEffect>,
    ) -> bool {
        let chaser_id = env.new_id();
        let request = ChaserRequest {
            id: chaser_id.clone(),
            symbol: self.options.symbol().to_owned(),
            size,
            side: self.options.side,
            max_attempts: SLICE_CHASER_MAX_ATTEMPTS,
            reduce_only: self.options.reduce_only,
            delay: self.options.chaser_delay,
            twap_id: self.id.clone(),
        };
        if let Err(message) = view.start_chaser(&request) {
            effects.push(TwapEffect::Log { message });
            return false;
        }
        self.tracked_chaser_ids.push(chaser_id.clone());
        // `twap chase` policy: settle the chaser at `timeoutPct%` of the slice.
        if self.options.timeout_pct.is_some() {
            let pct = self.options.timeout_pct.unwrap_or(50.0);
            let timeout_ms = (self.slice_interval_ms() * (pct / 100.0)).max(0.0);
            self.slice_timeouts.push(SliceTimeout {
                due_at: env.now_ms() + timeout_ms,
                chaser_id,
                market_when_settled: false,
            });
        }
        true
    }

    fn fire_slice_timeout(
        &mut self,
        timeout: SliceTimeout,
        now: f64,
        view: &dyn TwapMarketView,
        effects: &mut Vec<TwapEffect>,
    ) {
        let chaser = view.chaser(&timeout.chaser_id);
        if timeout.market_when_settled {
            match chaser {
                Some(chaser) if !chaser.ended => self.slice_timeouts.push(SliceTimeout {
                    due_at: now + SLICE_SETTLE_POLL_MS,
                    ..timeout
                }),
                Some(chaser) if chaser.order_may_be_open => effects.push(TwapEffect::Log {
                    message: format!(
                        "TWAP {} slice remainder was not sent: its order may still be open",
                        self.options.symbol()
                    ),
                }),
                Some(chaser) if chaser.remaining_size > 0.0 => {
                    self.place_slice_market(chaser.remaining_size, effects)
                }
                Some(_) => {}
                None => effects.push(TwapEffect::Log {
                    message: format!(
                        "TWAP {} slice remainder was not sent: its chase's end is unknown",
                        self.options.symbol()
                    ),
                }),
            }
            return;
        }
        let Some(chaser) = chaser.filter(|chaser| !chaser.ended) else {
            return;
        };
        if chaser.remaining_size <= 0.0 {
            return;
        }
        let action = if chaser.stopped {
            self.options.fail_action.unwrap_or(SliceAction::Market)
        } else {
            self.options.timeout_action.unwrap_or(SliceAction::Market)
        };
        effects.push(TwapEffect::RemoveChaser {
            twap_id: self.id.clone(),
            chaser_id: timeout.chaser_id.clone(),
        });
        if action == SliceAction::Market {
            // A market beside an order still being cancelled could overfill.
            self.slice_timeouts.push(SliceTimeout {
                due_at: now + SLICE_SETTLE_POLL_MS,
                market_when_settled: true,
                ..timeout
            });
        }
    }

    /// Best effort: the host ignores the outcome and the TWAP keeps going.
    fn place_slice_market(&self, amount: f64, effects: &mut Vec<TwapEffect>) {
        effects.push(TwapEffect::PlaceOrder {
            twap_id: self.id.clone(),
            settle_token: None,
            request: TwapOrderRequest {
                symbol: self.options.symbol().to_owned(),
                side: self.options.side,
                kind: OrderKind::Market,
                price: None,
                amount,
                reduce_only: self.options.reduce_only,
                client_order_id: None,
            },
        });
    }

    /// Fire every timer due at `env.now_ms()`, earliest first.
    pub fn wake(
        &mut self,
        env: &mut dyn TwapEnv,
        view: &dyn TwapMarketView,
        effects: &mut Vec<TwapEffect>,
    ) {
        let now = env.now_ms();
        loop {
            let timeout = self
                .slice_timeouts
                .iter()
                .enumerate()
                .filter(|(_, timeout)| timeout.due_at <= now)
                .min_by(|(_, a), (_, b)| a.due_at.total_cmp(&b.due_at))
                .map(|(index, timeout)| (index, timeout.due_at));
            let slice_due = self.slice_due_at.filter(|due_at| *due_at <= now);
            match (timeout, slice_due) {
                (Some((index, due_at)), slice) if slice.is_none_or(|slice| due_at <= slice) => {
                    let timeout = self.slice_timeouts.remove(index);
                    self.fire_slice_timeout(timeout, now, view, effects);
                }
                (_, Some(_)) => {
                    self.slice_due_at = None;
                    self.start(env, view, effects);
                }
                _ => return,
            }
        }
    }

    pub fn pause(&mut self) {
        if self.state.status == TwapStatus::Running {
            self.set_status(TwapStatus::Paused);
            self.slice_due_at = None;
        }
    }

    pub fn resume(&mut self, env: &mut dyn TwapEnv) {
        if self.state.status == TwapStatus::Paused {
            let now = env.now_ms();
            self.slice_due_at = Some(self.state.next_order_at.max(now));
            self.set_status(TwapStatus::Running);
        }
    }

    pub fn terminate(&mut self) {
        if self.state.status != TwapStatus::Completed {
            self.set_status(TwapStatus::Completed);
            self.slice_due_at = None;
            self.slice_timeouts.clear();
        }
    }

    pub fn cleanup(&mut self) {
        self.state.mark_for_cleanup = Some(true);
        self.state_changed = true;
    }
}

/// JS truthiness of an optional number.
fn truthy(value: Option<f64>) -> Option<f64> {
    value.filter(|value| *value != 0.0 && !value.is_nan())
}

/// `createNewState`: paired slices skew by a random percentage in opposite
/// directions so the total stays exact; the last slice absorbs any residue.
fn new_state(options: &TwapOptions, env: &mut dyn TwapEnv) -> TwapState {
    let orders_count = f64::from(options.orders_count);
    let base_size = options.size / orders_count;
    let mut orders_sizes = Vec::with_capacity(options.orders_count as usize);
    for _ in 0..options.orders_count / 2 {
        let size_mod = multiply(
            lodash_random(0.0, options.randomness_percentage, env.random_unit()),
            0.01,
        );
        let diff = multiply(base_size, size_mod).abs();
        orders_sizes.push(add(base_size, diff));
        orders_sizes.push(subtract(base_size, diff));
    }
    if !options.orders_count.is_multiple_of(2) {
        orders_sizes.push(base_size);
    }
    let remaining = options.size - orders_sizes.iter().fold(0.0, |sum, size| sum + size);
    if let Some(last) = orders_sizes.last_mut() {
        *last += remaining;
    }
    TwapState {
        account_name: options.account_name.clone(),
        active_market: options.active_market.clone(),
        size: options.size,
        orders_sizes,
        side: options.side,
        status: TwapStatus::Idle,
        orders_count: options.orders_count,
        orders_executed: 0,
        next_order_at: 0.0,
        mark_for_cleanup: None,
        origin: Some(options.origin.unwrap_or(TwapOrigin::Web)),
        created_at: Some(env.now_ms()),
        started_at: None,
        in_flight: None,
        extra: Map::new(),
    }
}
