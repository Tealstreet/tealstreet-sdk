//! `TwapInstance.ts` as a clock-driven state machine: timers are deadlines the
//! host wakes on, and engine work leaves as [`TwapEffect`]s.

use crate::math::{add, lodash_random, multiply, subtract};
use crate::types::{
    InFlightSlice, OrderSide, SliceAction, TwapOptions, TwapOrigin, TwapState, TwapStatus,
};
use crate::{
    ChaserRequest, OrderKind, SliceFailure, TwapEffect, TwapEnv, TwapMarketView, TwapOrderRequest,
};
use safe_cex_engine::error::{Outcome, VenueError};
use safe_cex_engine::write_plan::{AppliedEffect, Element, ElementResult, WritePlan};
use serde_json::Map;

/// The chaser a slice spawns gives up after this many reprices, as in TS.
const SLICE_CHASER_MAX_ATTEMPTS: u32 = 1000;
/// How often a timed-out slice looks whether its chase has settled.
const SLICE_SETTLE_POLL_MS: f64 = 250.0;

/// A slice placement awaiting [`TwapInstance::placement_settled`].
#[derive(Clone, Debug)]
struct PendingSlice {
    token: u64,
    element_index: usize,
    priced_limit: bool,
    executed_before: u32,
    next_order_at: f64,
    wait_ms: f64,
}

#[derive(Clone, Debug)]
struct SliceTimeout {
    element_index: usize,
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
    chaser_watch_at: Option<f64>,
    slice_timeouts: Vec<SliceTimeout>,
    pending: Vec<PendingSlice>,
    pending_remainders: Vec<usize>,
    chaser_elements: Vec<usize>,
    next_token: u64,
    state_changed: bool,
    journal: WritePlan,
    terminal_unknown: Option<VenueError>,
    next_element_index: usize,
    elements_per_slice: usize,
}

impl TwapInstance {
    pub fn new(
        id: String,
        options: TwapOptions,
        state: Option<TwapState>,
        env: &mut dyn TwapEnv,
    ) -> Self {
        let state = state.unwrap_or_else(|| new_state(&options, env));
        let elements_per_slice = if options.use_chaser_orders
            && options.timeout_pct.is_some()
            && (options.timeout_action.unwrap_or(SliceAction::Market) == SliceAction::Market
                || options.fail_action.unwrap_or(SliceAction::Market) == SliceAction::Market)
        {
            2
        } else {
            1
        };
        let restored_in_flight = usize::from(state.in_flight.is_some());
        let elements = (0..state.orders_sizes.len() * elements_per_slice + restored_in_flight)
            .map(|index| Element {
                index,
                client_order_id: if index == 0 {
                    state
                        .in_flight
                        .as_ref()
                        .and_then(|flight| flight.client_order_id.clone())
                } else {
                    None
                },
            })
            .collect();
        Self {
            id,
            options,
            state,
            tracked_chaser_ids: Vec::new(),
            tracked_limit_order_ids: Vec::new(),
            slice_due_at: None,
            chaser_watch_at: None,
            slice_timeouts: Vec::new(),
            pending: Vec::new(),
            pending_remainders: Vec::new(),
            chaser_elements: Vec::new(),
            next_token: 0,
            state_changed: false,
            journal: WritePlan::new(elements),
            terminal_unknown: None,
            next_element_index: restored_in_flight,
            elements_per_slice,
        }
    }

    fn record(&mut self, index: usize, result: ElementResult, effects: &mut Vec<TwapEffect>) {
        if let Err(error) = self.journal.record(index, result) {
            effects.push(TwapEffect::Diagnostic {
                message: format!("TWAP outcome journal: {error}"),
            });
        }
    }

    fn record_success(&mut self, index: usize, ids: &[String], effects: &mut Vec<TwapEffect>) {
        let result = if ids.is_empty() {
            ElementResult::Applied {
                effect: AppliedEffect::DeferredChild,
                target_order_id: None,
                position_key: None,
            }
        } else {
            ElementResult::Placed {
                order_ids: ids.to_vec(),
            }
        };
        self.record(index, result, effects);
    }

    fn halt_unknown(&mut self, index: usize, error: VenueError, effects: &mut Vec<TwapEffect>) {
        let error = crate::unknown_outcome(error);
        self.record(
            index,
            ElementResult::Unknown {
                error: Box::new(error.clone()),
            },
            effects,
        );
        self.terminate();
        for chaser_id in &self.tracked_chaser_ids {
            effects.push(TwapEffect::HaltChaser {
                chaser_id: chaser_id.clone(),
            });
        }
        effects.push(TwapEffect::Log {
            message: error.message.clone(),
        });
        self.terminal_unknown = Some(error);
        self.report_unknown(effects);
    }

    fn report_unknown(&self, effects: &mut Vec<TwapEffect>) {
        let Some(error) = self.terminal_unknown.clone() else {
            return;
        };
        let mut ids = Vec::new();
        for row in self.journal.outcomes() {
            let row_ids = match row.result {
                ElementResult::Placed { order_ids } => order_ids,
                ElementResult::Unknown { error }
                | ElementResult::NotSent { error: Some(error) } => {
                    crate::retained_order_ids(&error)
                }
                _ => Vec::new(),
            };
            for id in row_ids {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        let error = self.journal.attach(error.partial_ids(&ids));
        effects.push(TwapEffect::Outcome {
            twap_id: self.id.clone(),
            error: Box::new(error),
        });
    }

    pub(crate) fn remainder_settled(
        &mut self,
        index: usize,
        result: Result<Vec<String>, SliceFailure>,
        effects: &mut Vec<TwapEffect>,
    ) {
        self.pending_remainders.retain(|pending| *pending != index);
        match result {
            Ok(ids) => self.record_success(index, &ids, effects),
            Err(error) if crate::known_refusal(&error) => {
                effects.push(TwapEffect::Log {
                    message: format!(
                        "TWAP {} slice remainder failed: {}",
                        self.options.symbol(),
                        error.message
                    ),
                });
                self.record(
                    index,
                    ElementResult::NotSent {
                        error: Some(Box::new(error)),
                    },
                    effects,
                );
            }
            Err(error) => {
                self.halt_unknown(index, error, effects);
                return;
            }
        }
        self.report_unknown(effects);
    }

    /// Direct limit orders this TWAP placed (`currentOrderIds`).
    pub fn current_order_ids(&self) -> &[String] {
        &self.tracked_limit_order_ids
    }

    pub fn tracked_chaser_ids(&self) -> &[String] {
        &self.tracked_chaser_ids
    }

    /// The earliest timer deadline, if any timer is armed.
    pub fn wake_at(&self) -> Option<f64> {
        self.slice_timeouts
            .iter()
            .map(|timeout| timeout.due_at)
            .chain(self.slice_due_at)
            .chain(self.chaser_watch_at)
            .min_by(f64::total_cmp)
    }

    /// True while work started before completion can still produce effects.
    pub fn has_outstanding_work(&self) -> bool {
        !self.pending.is_empty() || !self.pending_remainders.is_empty() || self.wake_at().is_some()
    }

    pub(crate) fn mark_state_changed(&mut self) {
        self.state_changed = true;
    }

    pub(crate) fn take_state_changed(&mut self) -> bool {
        std::mem::take(&mut self.state_changed)
    }

    /// `size` floored to the market's step; the rest rides on the next slice.
    fn whole_steps(&mut self, size: f64, view: &dyn TwapMarketView) -> f64 {
        let Some(step) = view
            .amount_step(self.options.symbol())
            .filter(|step| step.is_finite() && *step > 0.0)
        else {
            return size;
        };
        let steps = (size / step + 1e-9).floor();
        let whole = multiply(steps, step);
        if let Some(next) = self.state.orders_sizes.first_mut() {
            *next = add(*next, subtract(size, whole));
        }
        whole
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
        let order_size =
            (!self.state.orders_sizes.is_empty()).then(|| self.state.orders_sizes.remove(0));
        let order_size = order_size.map(|size| self.whole_steps(size, view));
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
        let element_index = self.next_element_index;
        self.next_element_index += self.elements_per_slice;
        // TS tests `if (orderSize)`: a zero or NaN slice places nothing.
        let Some(order_size) = order_size.filter(|size| *size != 0.0 && !size.is_nan()) else {
            self.finish_slice(executed_before, next_order_at, wait_ms, now);
            return;
        };

        if self.options.use_chaser_orders {
            let started = self.start_slice_chaser(order_size, element_index, env, view, effects);
            self.record(
                element_index,
                if started {
                    ElementResult::Applied {
                        effect: AppliedEffect::DeferredChild,
                        target_order_id: None,
                        position_key: None,
                    }
                } else {
                    ElementResult::NotSent {
                        error: Some(Box::new(
                            VenueError::new("chaser refused").outcome(Outcome::NotSent),
                        )),
                    }
                },
                effects,
            );
            self.finish_slice(
                executed_before + u32::from(started),
                next_order_at,
                wait_ms,
                now,
            );
            self.report_shortfall(effects);
            return;
        }

        let symbol = self.options.symbol().to_owned();
        let price = if self.options.use_limit_orders {
            view.ticker(&symbol)
                .map_or(0.0, |ticker| match self.options.side {
                    OrderSide::Buy => truthy(ticker.bid).or(truthy(ticker.last)).unwrap_or(0.0),
                    OrderSide::Sell => truthy(ticker.ask).or(truthy(ticker.last)).unwrap_or(0.0),
                })
        } else {
            0.0
        };
        let priced = truthy(Some(price)).is_some();
        let token = self.next_token;
        self.next_token += 1;
        self.pending.push(PendingSlice {
            token,
            element_index,
            priced_limit: priced,
            executed_before,
            next_order_at,
            wait_ms,
        });
        let mut request = TwapOrderRequest {
            symbol,
            side: self.options.side,
            kind: if priced {
                OrderKind::Limit
            } else {
                OrderKind::Market
            },
            price: priced.then_some(price),
            amount: order_size,
            reduce_only: self.options.reduce_only,
            client_order_id: None,
        };
        request.client_order_id = view.client_order_id(&request);
        // Persisted before the order goes out: a host that dies before the slice
        // is counted leaves a restore something to look up instead of re-placing.
        self.state.in_flight = Some(InFlightSlice {
            size: order_size,
            client_order_id: request.client_order_id.clone(),
        });
        self.state_changed = true;
        self.record(
            element_index,
            ElementResult::Unknown {
                error: Box::new(crate::unknown_outcome(VenueError::new("pending child"))),
            },
            effects,
        );
        effects.push(TwapEffect::PlaceOrder {
            twap_id: self.id.clone(),
            settle_token: Some(token),
            element_index,
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
        self.halt_unknown(
            0,
            VenueError::new(format!("unconfirmed {} slice", in_flight.size)),
            effects,
        );
    }

    /// The engine's answer to a slice [`TwapEffect::PlaceOrder`]: the order ids,
    /// or the failure. Definite refusals continue without counting the slice;
    /// genuine uncertainty halts further writes for this TWAP.
    pub fn placement_settled(
        &mut self,
        token: u64,
        outcome: Result<Vec<String>, SliceFailure>,
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
        match outcome {
            Ok(order_ids) => {
                self.record_success(pending.element_index, &order_ids, effects);
                if pending.priced_limit && !order_ids.is_empty() {
                    self.tracked_limit_order_ids.extend(order_ids);
                }
            }
            Err(failure) => {
                if !crate::known_refusal(&failure) {
                    self.state.orders_executed = executed;
                    self.halt_unknown(pending.element_index, failure, effects);
                    return;
                }
                let placed_ids = crate::retained_order_ids(&failure);
                if placed_ids.is_empty() {
                    executed = pending.executed_before;
                }
                if pending.priced_limit {
                    self.tracked_limit_order_ids.extend(placed_ids);
                }
                self.record(
                    pending.element_index,
                    ElementResult::NotSent {
                        error: Some(Box::new(failure.clone())),
                    },
                    effects,
                );
                effects.push(TwapEffect::Log {
                    message: format!(
                        "TWAP {} slice failed: {}",
                        self.options.symbol(),
                        failure.message
                    ),
                });
            }
        }
        // terminate() may have run while the placement was in flight.
        if self.state.status == TwapStatus::Completed {
            self.report_unknown(effects);
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
        element_index: usize,
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
        self.chaser_elements.push(element_index);
        self.chaser_watch_at = Some(env.now_ms() + SLICE_SETTLE_POLL_MS);
        // `twap chase` policy: settle the chaser at `timeoutPct%` of the slice.
        if self.options.timeout_pct.is_some() {
            let pct = self.options.timeout_pct.unwrap_or(50.0);
            let timeout_ms = (self.slice_interval_ms() * (pct / 100.0)).max(0.0);
            self.slice_timeouts.push(SliceTimeout {
                element_index: element_index + 1,
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
        if chaser.is_some_and(|chaser| chaser.order_may_be_open) {
            self.halt_unknown(
                timeout.element_index.saturating_sub(1),
                VenueError::new("chaser order may be open"),
                effects,
            );
            return;
        }
        if timeout.market_when_settled {
            match chaser {
                Some(chaser) if !chaser.ended => self.slice_timeouts.push(SliceTimeout {
                    due_at: now + SLICE_SETTLE_POLL_MS,
                    ..timeout
                }),
                Some(chaser) if chaser.remaining_size > 0.0 => {
                    self.place_slice_market(chaser.remaining_size, timeout.element_index, effects)
                }
                Some(_) => {}
                None => self.halt_unknown(
                    timeout.element_index.saturating_sub(1),
                    VenueError::new("chaser end is unknown"),
                    effects,
                ),
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

    /// Definite remainder refusal continues; uncertainty stops this action.
    fn place_slice_market(
        &mut self,
        amount: f64,
        element_index: usize,
        effects: &mut Vec<TwapEffect>,
    ) {
        self.pending_remainders.push(element_index);
        self.record(
            element_index,
            ElementResult::Unknown {
                error: Box::new(crate::unknown_outcome(VenueError::new("pending remainder"))),
            },
            effects,
        );
        effects.push(TwapEffect::PlaceOrder {
            twap_id: self.id.clone(),
            settle_token: None,
            element_index,
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
        if let Some(index) = self
            .tracked_chaser_ids
            .iter()
            .zip(&self.chaser_elements)
            .find_map(|(id, index)| {
                view.chaser(id)
                    .filter(|progress| progress.order_may_be_open)
                    .map(|_| *index)
            })
        {
            self.halt_unknown(index, VenueError::new("chaser order may be open"), effects);
            return;
        }
        let now = env.now_ms();
        if self.chaser_watch_at.is_some_and(|due| due <= now) {
            self.chaser_watch_at = self
                .tracked_chaser_ids
                .iter()
                .any(|id| view.chaser(id).is_some_and(|progress| !progress.ended))
                .then_some(now + SLICE_SETTLE_POLL_MS);
        }
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
        }
        self.slice_due_at = None;
        self.slice_timeouts.clear();
        self.chaser_watch_at = None;
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
