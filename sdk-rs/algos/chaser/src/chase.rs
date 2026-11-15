use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use safe_cex_algos::executor::Instant;
use safe_cex_algos::{AlgoStore, Scope};
use safe_cex_engine::error::VenueError;
use safe_cex_engine::model::{Order, OrderSide, OrderStatus, PositionSide};
use serde::{Deserialize, Serialize};
use tokio::sync::Notify;

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
    /// Set only by a TWAP starting its slice, never from a page's `chaser:start`.
    #[serde(skip_deserializing)]
    pub twap_id: Option<String>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub twap_id: Option<String>,
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
    /// The chaser already told how a stop it had to retry ended; the page adds no outcome of its own.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub outcome_told: bool,
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

/// Closed means fully filled, whatever the row says: BloFin and Bitget close a
/// row by status alone, leaving its fill at what it last showed.
fn closed_filled(order: &Order, placed: Option<f64>) -> f64 {
    let filled = placement_filled(order);
    match placed {
        Some(placed) if order.status == OrderStatus::Closed => filled.max(placed),
        _ => filled,
    }
}

#[derive(Default)]
struct State {
    attempts: u32,
    stopped: bool,
    stop_reason: String,
    current: Vec<String>,
    orders_released: bool,
    ever: Vec<String>,
    filled_by: HashMap<String, f64>,
    /// Fill events per order: the same fills a row's `filled` reports.
    event_fills: HashMap<String, f64>,
    /// What each order was placed for, in placement units.
    placed: HashMap<String, f64>,
    remaining: f64,
    min_tick_distance: u32,
    pending_update: bool,
    fresh_order: bool,
    complete_emitted: bool,
    cancels_claimed: Vec<String>,
    /// Orders whose stop cancel the venue refused while the store still had them open.
    cancel_refused: Vec<(String, String, VenueError)>,
    outcome_told: bool,
    /// A stop cancel ended without proof the order is gone.
    order_may_be_open: bool,
    placement_unknown: bool,
    last_partial_fill_at: Option<Instant>,
}

/// One chase: the TS `ChaserOrderInstance` state.
pub struct Chase {
    pub id: String,
    pub options: ChaserOptions,
    pub started_ms: f64,
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
    /// The position moved with no fill to explain it: a fill the stream missed
    /// may sit behind an order row that still reads open.
    pub verify_fills: Cell<bool>,
    /// The symbol's position size when the watcher last looked.
    pub position_seen: Cell<Option<f64>>,
    /// Ticker changes seen, to tell a chase that has stopped getting market data.
    pub tickers_seen: Cell<u64>,
    /// A placement is on the wire: its order may land, so nothing finishes the chase yet.
    placing: Cell<bool>,
    placing_ended: Notify,
    /// Counts `set_current`: a replacement's row has not been seen yet.
    placements: Cell<u32>,
    state: RefCell<State>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Dirty {
    pub orders: bool,
    pub positions: bool,
    pub tickers: bool,
}

impl Chase {
    pub fn new(
        id: String,
        options: ChaserOptions,
        store: &AlgoStore,
        scope: Scope,
        started_ms: f64,
    ) -> Self {
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
            started_ms,
            target_position_contracts,
            scope,
            watch_scope,
            wake: Notify::new(),
            dirty: Cell::new(Dirty::default()),
            needs_reconcile: Cell::new(false),
            verify_fills: Cell::new(false),
            position_seen: Cell::new(None),
            tickers_seen: Cell::new(0),
            placing: Cell::new(false),
            placing_ended: Notify::new(),
            placements: Cell::new(0),
            state: RefCell::new(State {
                remaining: options.size,
                ..State::default()
            }),
            options,
        }
    }

    pub fn set_placing(&self, placing: bool) {
        self.placing.set(placing);
        if !placing {
            self.placing_ended.notify_waiters();
        }
    }

    pub fn is_placing(&self) -> bool {
        self.placing.get()
    }

    /// Until no placement is on the wire and what landed is in `current`.
    pub async fn placement_settled(&self) {
        while self.placing.get() {
            self.placing_ended.notified().await;
        }
    }

    pub fn placements(&self) -> u32 {
        self.placements.get()
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
        let mut state = self.state.borrow_mut();
        let filled = closed_filled(order, state.placed.get(&order.id).copied());
        if filled > state.filled_by.get(&order.id).copied().unwrap_or(0.0) {
            state.last_partial_fill_at = Some(Instant::now());
            state.filled_by.insert(order.id.clone(), filled);
        }
        self.finish_if_filled(&mut state, "Fully filled already");
    }

    pub fn note_placed(&self, order_id: &str, amount: f64) {
        self.state
            .borrow_mut()
            .placed
            .insert(order_id.to_owned(), amount);
    }

    /// The row's fill, the whole order once it is Closed (see [`closed_filled`]).
    pub fn row_filled(&self, order: &Order) -> f64 {
        closed_filled(order, self.state.borrow().placed.get(&order.id).copied())
    }

    /// An incremental fill event. Malformed amounts never grow the remainder.
    pub fn add_fill_amount(&self, order_id: &str, amount: f64) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        let mut state = self.state.borrow_mut();
        state.last_partial_fill_at = Some(Instant::now());
        let events = state.event_fills.entry(order_id.into()).or_default();
        *events += amount;
        let events = *events;
        let filled = state.filled_by.entry(order_id.into()).or_default();
        *filled = filled.max(events);
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

    pub fn filled_by(&self, order_id: &str) -> f64 {
        self.state
            .borrow()
            .filled_by
            .get(order_id)
            .copied()
            .unwrap_or(0.0)
    }

    /// A stop cancel the venue refused: (order, symbol, refusal), told at completion.
    pub fn note_cancel_refused(&self, order_id: &str, symbol: &str, error: VenueError) {
        let mut state = self.state.borrow_mut();
        if !state.cancel_refused.iter().any(|(id, ..)| id == order_id) {
            state
                .cancel_refused
                .push((order_id.to_owned(), symbol.to_owned(), error));
        }
    }

    pub fn note_outcome_told(&self) {
        self.state.borrow_mut().outcome_told = true;
    }

    pub fn note_order_may_be_open(&self) {
        self.state.borrow_mut().order_may_be_open = true;
    }

    pub fn order_may_be_open(&self) -> bool {
        let state = self.state.borrow();
        state.order_may_be_open || state.placement_unknown
    }

    pub fn set_placement_unknown(&self, unknown: bool) {
        self.state.borrow_mut().placement_unknown = unknown;
    }

    pub fn outcome_told(&self) -> bool {
        self.state.borrow().outcome_told
    }

    pub fn take_cancel_refused(&self) -> Vec<(String, String, VenueError)> {
        std::mem::take(&mut self.state.borrow_mut().cancel_refused)
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
        self.placements.set(self.placements.get() + 1);
        let mut state = self.state.borrow_mut();
        for id in &ids {
            if !state.ever.contains(id) {
                state.ever.push(id.clone());
            }
        }
        if !state.orders_released {
            state.current = ids;
        }
    }

    pub fn release_orders(&self) {
        let mut state = self.state.borrow_mut();
        state.orders_released = true;
        state.current.clear();
    }

    /// True once per order, for the task that will cancel it.
    pub fn claim_cancel(&self, id: &str) -> bool {
        let mut state = self.state.borrow_mut();
        let claimed = state.cancels_claimed.iter().any(|other| other == id);
        if !claimed {
            state.cancels_claimed.push(id.to_owned());
        }
        !claimed
    }

    pub fn drop_current(&self, id: &str) {
        self.state
            .borrow_mut()
            .current
            .retain(|current| current != id);
    }

    /// The venue now shows this order under its own id.
    pub fn adopt_venue_id(&self, placed: &str, venue_id: &str) {
        let mut state = self.state.borrow_mut();
        if let Some(id) = state.current.iter_mut().find(|id| *id == placed) {
            *id = venue_id.to_owned();
        }
        if !state.ever.iter().any(|id| id == venue_id) {
            state.ever.push(venue_id.to_owned());
        }
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
            twap_id: self.options.twap_id.clone(),
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
