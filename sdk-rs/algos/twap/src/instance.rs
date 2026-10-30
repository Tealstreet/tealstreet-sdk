//! `TwapInstance.ts` as a clock-driven state machine: timers are deadlines the
//! host wakes on, and engine work leaves as [`TwapEffect`]s.

use crate::math::{add, lodash_random, multiply, subtract};
use crate::types::{
    InFlightSlice, OrderSide, SliceAction, TwapOptions, TwapOrigin, TwapState, TwapStatus,
};
use crate::{ChaserRequest, OrderKind, TwapEffect, TwapEnv, TwapMarketView, TwapOrderRequest};
use serde_json::Map;

/// The chaser a slice spawns gives up after this many reprices, as in TS.
const SLICE_CHASER_MAX_ATTEMPTS: u32 = 1000;

/// A slice placement awaiting [`TwapInstance::placement_settled`].
#[derive(Clone, Debug)]
struct PendingSlice {
    token: u64,
    priced_limit: bool,
    executed_before: u32,
    next_order_at: f64,
    wait_ms: f64,
}

#[derive(Clone, Debug)]
struct SliceTimeout {
    due_at: f64,
    chaser_id: String,
}

#[derive(Clone, Debug)]
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
        }
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
        // TS tests `if (orderSize)`: a zero or NaN slice places nothing.
        let Some(order_size) = order_size.filter(|size| *size != 0.0 && !size.is_nan()) else {
            self.finish_slice(executed_before, next_order_at, wait_ms, now);
            return;
        };

        if self.options.use_chaser_orders {
            self.start_slice_chaser(order_size, env, effects);
            self.finish_slice(executed_before + 1, next_order_at, wait_ms, now);
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
    /// or the error message. The slice advances either way, as in TS.
    pub fn placement_settled(
        &mut self,
        token: u64,
        outcome: Result<Vec<String>, String>,
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
        match outcome {
            Ok(order_ids) => {
                if pending.priced_limit && !order_ids.is_empty() {
                    self.tracked_limit_order_ids.extend(order_ids);
                }
            }
            Err(message) => effects.push(TwapEffect::Log {
                message: format!("TWAP {} slice failed: {message}", self.options.symbol()),
            }),
        }
        // terminate() may have run while the placement was in flight.
        if self.state.status == TwapStatus::Completed {
            return;
        }
        let now = env.now_ms();
        self.finish_slice(
            pending.executed_before + 1,
            pending.next_order_at,
            pending.wait_ms,
            now,
        );
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

    fn start_slice_chaser(
        &mut self,
        size: f64,
        env: &mut dyn TwapEnv,
        effects: &mut Vec<TwapEffect>,
    ) {
        let chaser_id = env.new_id();
        self.tracked_chaser_ids.push(chaser_id.clone());
        effects.push(TwapEffect::StartChaser {
            twap_id: self.id.clone(),
            request: ChaserRequest {
                id: chaser_id.clone(),
                symbol: self.options.symbol().to_owned(),
                size,
                side: self.options.side,
                max_attempts: SLICE_CHASER_MAX_ATTEMPTS,
                reduce_only: self.options.reduce_only,
                delay: self.options.chaser_delay,
            },
        });
        // `twap chase` policy: settle the chaser at `timeoutPct%` of the slice.
        if self.options.timeout_pct.is_some() {
            let pct = self.options.timeout_pct.unwrap_or(50.0);
            let timeout_ms = (self.slice_interval_ms() * (pct / 100.0)).max(0.0);
            self.slice_timeouts.push(SliceTimeout {
                due_at: env.now_ms() + timeout_ms,
                chaser_id,
            });
        }
    }

    fn fire_slice_timeout(
        &mut self,
        chaser_id: &str,
        view: &dyn TwapMarketView,
        effects: &mut Vec<TwapEffect>,
    ) {
        let Some(chaser) = view.chaser(chaser_id) else {
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
            chaser_id: chaser_id.to_owned(),
        });
        if action == SliceAction::Market {
            // Best effort: the host ignores the outcome and the TWAP keeps going.
            effects.push(TwapEffect::PlaceOrder {
                twap_id: self.id.clone(),
                settle_token: None,
                request: TwapOrderRequest {
                    symbol: self.options.symbol().to_owned(),
                    side: self.options.side,
                    kind: OrderKind::Market,
                    price: None,
                    amount: chaser.remaining_size,
                    reduce_only: self.options.reduce_only,
                    client_order_id: None,
                },
            });
        }
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
                    self.fire_slice_timeout(&timeout.chaser_id, view, effects);
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
