use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use safe_cex_algos::{AlgoStore, Scope};
use safe_cex_engine::model::{Order, OrderSide, PositionSide};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::time::Instant;

/// `chase ... to <length>`: stop once the live price passes the threshold
/// in the unfavorable direction (up for buys, down for sells).
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Terminate {
    #[serde(rename = "type")]
    pub kind: TerminateKind,
    pub value: f64,
    #[serde(default)]
    pub anchor_price: f64,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TerminateKind {
    Price,
    Percent,
    Usd,
}

/// The `chaser:start` payload, as the TS `ChaserOrderOptions`.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChaserOptions {
    pub symbol: String,
    pub size: f64,
    pub side: OrderSide,
    pub max_attempts: f64,
    #[serde(default)]
    pub reduce_only: Option<bool>,
    #[serde(default)]
    pub origin: Option<String>,
    #[serde(default)]
    pub account_name: Option<String>,
    #[serde(default)]
    pub terminate: Option<Terminate>,
    #[serde(default)]
    pub client_id: Option<String>,
}

impl ChaserOptions {
    pub fn reduce_only(&self) -> bool {
        self.reduce_only == Some(true)
    }

    /// The position side an entry chase grows.
    pub fn entry_side(&self) -> PositionSide {
        match self.side {
            OrderSide::Buy => PositionSide::Long,
            OrderSide::Sell => PositionSide::Short,
        }
    }

    /// The position side a reduce-only chase closes.
    pub fn reduced_side(&self) -> PositionSide {
        match self.side {
            OrderSide::Buy => PositionSide::Short,
            OrderSide::Sell => PositionSide::Long,
        }
    }
}

/// One `store.activeChasers` row, as the TS `ActiveChaser`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveChaser {
    pub id: String,
    pub symbol: String,
    pub size: f64,
    pub side: OrderSide,
    pub max_attempts: f64,
    pub reduce_only: bool,
    pub attempts: u32,
    pub is_stopped: bool,
    pub stop_reason: String,
    pub current_order_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// The `chaser:complete` event. `filled` is in placement units, clamped to
/// `[0, size]`, and may undercount a fill that lands during the final cancel.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChaserComplete {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    pub symbol: String,
    pub side: OrderSide,
    pub reduce_only: bool,
    pub size: f64,
    pub filled: f64,
    pub stop_reason: String,
}

/// The fill in the unit of `PlaceOrderOpts.amount`.
pub fn placement_filled(order: &Order) -> f64 {
    match order
        .extra
        .as_ref()
        .and_then(|extra| extra.filled_in_placement_units)
    {
        Some(fill) if fill.is_finite() && fill >= 0.0 => fill,
        _ => order.filled,
    }
}

#[derive(Default)]
struct State {
    attempts: u32,
    stopped: bool,
    stop_reason: String,
    current: Vec<String>,
    ever: Vec<String>,
    filled_by: HashMap<String, f64>,
    remaining: f64,
    min_tick_distance: u32,
    pending_update: bool,
    fresh_order: bool,
    complete_emitted: bool,
    last_partial_fill_at: Option<Instant>,
}

/// One chase: the TS `ChaserOrderInstance` state.
pub struct Chase {
    pub id: String,
    pub options: ChaserOptions,
    pub target_position_contracts: Option<f64>,
    /// The supervisor's tasks. Cleanup never aborts it mid-placement; it exits
    /// cooperatively once stopped, so a placement in flight is still cancelled.
    pub scope: Scope,
    /// The change watcher and timers, disposed first by cleanup.
    pub watch_scope: Scope,
    /// Wakes the supervisor: a change arrived or the chase stopped.
    pub wake: Notify,
    pub dirty: Cell<Dirty>,
    /// A feed rejoin lost fill events; reconcile before the next replacement.
    pub needs_reconcile: Cell<bool>,
    state: RefCell<State>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Dirty {
    pub orders: bool,
    pub positions: bool,
    pub tickers: bool,
}

impl Chase {
    pub fn new(id: String, options: ChaserOptions, store: &AlgoStore, scope: Scope) -> Self {
        let target_position_contracts = (!options.reduce_only()).then(|| {
            let side = options.entry_side();
            let existing = store
                .positions
                .iter()
                .find(|position| position.symbol == options.symbol && position.side == side)
                .map_or(0.0, |position| position.contracts);
            existing.abs() + options.size
        });
        let watch_scope = scope.child();
        Self {
            id,
            target_position_contracts,
            scope,
            watch_scope,
            wake: Notify::new(),
            dirty: Cell::new(Dirty::default()),
            needs_reconcile: Cell::new(false),
            state: RefCell::new(State {
                remaining: options.size,
                ..State::default()
            }),
            options,
        }
    }

    pub fn mark_dirty(&self, change: Dirty) {
        let mut dirty = self.dirty.get();
        dirty.orders |= change.orders;
        dirty.positions |= change.positions;
        dirty.tickers |= change.tickers;
        self.dirty.set(dirty);
        self.wake.notify_one();
    }

    pub fn take_dirty(&self) -> Dirty {
        self.dirty.replace(Dirty::default())
    }

    pub fn increment_attempts(&self) {
        let mut state = self.state.borrow_mut();
        if f64::from(state.attempts) > self.options.max_attempts {
            state.stopped = true;
            state.stop_reason = "Reached maximum number of attempts".into();
        } else {
            state.attempts += 1;
        }
    }

    pub fn mark_complete(&self, reason: &str) {
        {
            let mut state = self.state.borrow_mut();
            state.stopped = true;
            state.stop_reason = reason.into();
        }
        self.wake.notify_one();
    }

    fn recalculate(state: &mut State, size: f64) {
        let remaining = size - state.filled_by.values().sum::<f64>();
        // Summed fills land a float hair off the size; that is filled.
        state.remaining = if remaining.abs() <= size.abs() * 1e-9 {
            0.0
        } else {
            remaining
        };
    }

    fn finish_if_filled(&self, state: &mut State, reason: &str) {
        Self::recalculate(state, self.options.size);
        if state.remaining <= 0.0 {
            state.stopped = true;
            state.stop_reason = reason.into();
        }
    }

    /// The order-row fill (WS status path). Unlike the TS assignment it only
    /// raises: a row lagging behind a fill event must not restore the
    /// remainder, or the replacement overfills.
    pub fn subtract_filled_size(&self, order: &Order) {
        let filled = placement_filled(order);
        let mut state = self.state.borrow_mut();
        if filled > state.filled_by.get(&order.id).copied().unwrap_or(0.0) {
            state.last_partial_fill_at = Some(Instant::now());
            state.filled_by.insert(order.id.clone(), filled);
        }
        self.finish_if_filled(&mut state, "Fully filled already");
    }

    /// An incremental fill event. Malformed amounts never grow the remainder.
    pub fn add_fill_amount(&self, order_id: &str, amount: f64) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        let mut state = self.state.borrow_mut();
        state.last_partial_fill_at = Some(Instant::now());
        *state.filled_by.entry(order_id.into()).or_default() += amount;
        self.finish_if_filled(&mut state, "Fully filled via fill events");
    }

    /// An absolute fill from REST or the store; only ever raises.
    pub fn set_filled_amount(&self, order_id: &str, total: f64) {
        let mut state = self.state.borrow_mut();
        if total > state.filled_by.get(order_id).copied().unwrap_or(0.0) {
            state.last_partial_fill_at = Some(Instant::now());
            state.filled_by.insert(order_id.into(), total);
            self.finish_if_filled(&mut state, "Fully filled via REST check");
        }
    }

    pub fn claim_complete_emission(&self) -> bool {
        let mut state = self.state.borrow_mut();
        !std::mem::replace(&mut state.complete_emitted, true)
    }

    pub fn filled_size(&self) -> f64 {
        let size = self.options.size;
        (size - self.state.borrow().remaining).clamp(0.0, size)
    }

    pub fn set_current(&self, ids: Vec<String>) {
        let mut state = self.state.borrow_mut();
        for id in &ids {
            if !state.ever.contains(id) {
                state.ever.push(id.clone());
            }
        }
        state.current = ids;
    }

    pub fn take_current(&self) -> Vec<String> {
        std::mem::take(&mut self.state.borrow_mut().current)
    }

    pub fn current(&self) -> Vec<String> {
        self.state.borrow().current.clone()
    }

    pub fn ever(&self) -> Vec<String> {
        self.state.borrow().ever.clone()
    }

    pub fn is_stopped(&self) -> bool {
        self.state.borrow().stopped
    }

    pub fn stop_reason(&self) -> String {
        self.state.borrow().stop_reason.clone()
    }

    pub fn remaining(&self) -> f64 {
        self.state.borrow().remaining
    }

    pub fn min_tick_distance(&self) -> u32 {
        self.state.borrow().min_tick_distance
    }

    pub fn bump_tick_distance(&self) {
        self.state.borrow_mut().min_tick_distance += 1;
    }

    pub fn pending_update(&self) -> bool {
        self.state.borrow().pending_update
    }

    pub fn set_pending_update(&self, pending: bool) {
        self.state.borrow_mut().pending_update = pending;
    }

    pub fn take_fresh_order(&self) -> bool {
        std::mem::replace(&mut self.state.borrow_mut().fresh_order, false)
    }

    pub fn set_fresh_order(&self, fresh: bool) {
        self.state.borrow_mut().fresh_order = fresh;
    }

    pub fn since_last_partial_fill_ms(&self) -> Option<u128> {
        self.state
            .borrow()
            .last_partial_fill_at
            .map(|at| at.elapsed().as_millis())
    }

    pub fn should_terminate(&self, price: f64) -> bool {
        let Some(terminate) = &self.options.terminate else {
            return false;
        };
        if !price.is_finite() || price <= 0.0 {
            return false;
        }
        let buy = self.options.side == OrderSide::Buy;
        let threshold = match terminate.kind {
            TerminateKind::Price => terminate.value,
            TerminateKind::Percent if buy => {
                terminate.anchor_price * (1.0 + terminate.value / 100.0)
            }
            TerminateKind::Percent => terminate.anchor_price * (1.0 - terminate.value / 100.0),
            TerminateKind::Usd if buy => terminate.anchor_price + terminate.value,
            TerminateKind::Usd => terminate.anchor_price - terminate.value,
        };
        if buy {
            price >= threshold
        } else {
            price <= threshold
        }
    }

    pub fn active(&self) -> ActiveChaser {
        let state = self.state.borrow();
        ActiveChaser {
            id: self.id.clone(),
            symbol: self.options.symbol.clone(),
            size: self.options.size,
            side: self.options.side,
            max_attempts: self.options.max_attempts,
            reduce_only: self.options.reduce_only(),
            attempts: state.attempts,
            is_stopped: state.stopped,
            stop_reason: state.stop_reason.clone(),
            current_order_ids: state.current.clone(),
            origin: self.options.origin.clone(),
        }
    }
}

/// The TS `adjust`: round to a multiple of `step`, then to its decimals.
pub fn adjust(value: f64, step: f64) -> f64 {
    let multiplier = 1.0 / step;
    let adjusted = (value * multiplier).round() / multiplier;
    let factor = 10f64.powi(decimals(step));
    (adjusted * factor).round() / factor
}

fn decimals(step: f64) -> i32 {
    if step.fract() == 0.0 {
        return 0;
    }
    let text = format!("{step}");
    if let Some((_, exponent)) = text.split_once('e') {
        return exponent.trim_start_matches('-').parse().unwrap_or(0);
    }
    text.split_once('.')
        .map_or(0, |(_, fraction)| fraction.len() as i32)
}
