use safe_cex_types::Order;

// ts: placementFilled. A valid placement-unit fill overrides the display-unit fill.
pub fn placement_filled(order: &Order) -> f64 {
    order
        .extra
        .as_ref()
        .and_then(|extra| extra.filled_in_placement_units)
        .filter(|fill| fill.is_finite() && *fill >= 0.0)
        .unwrap_or(order.filled)
}

use crate::task_types::{ChaserOrderOptions, ChaserOrderState, ChaserTerminateKind};
use indexmap::{IndexMap, IndexSet};
use safe_cex_exchange::{exchange::Exchange, runtime::ExchangeError};
use safe_cex_types::{OrderSide, PositionSide};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub const DEFAULT_CHASER_STATE: ChaserOrderState = ChaserOrderState {
    attempts: 0.0,
    is_stopped: false,
    stop_reason: String::new(),
    current_orders_ids: Vec::new(),
};

type Disposer = Box<dyn FnOnce() -> Result<(), ExchangeError>>;
#[derive(Clone, Copy)]
enum Fill {
    Number(f64),
    // ts-bug: ChaserOrderInstance's plain object can add a native function to a number.
    NativeFunctionString,
}

pub struct ChaserOrderInstance {
    pub exchange: Rc<dyn Exchange>,
    options: Rc<RefCell<ChaserOrderOptions>>,
    id: String,
    state: Rc<RefCell<ChaserOrderState>>,
    remaining_size: Cell<f64>,
    filled_by_order_id: RefCell<IndexMap<String, Fill>>,
    min_tick_distance: Cell<f64>,
    pending_orders_update: Cell<bool>,
    fresh_order: Cell<bool>,
    disposers: RefCell<Vec<Disposer>>,
    target_position_contracts: Option<f64>,
    complete_emitted: Cell<bool>,
    ever_tracked_order_ids: Rc<RefCell<IndexSet<String>>>,
    last_partial_fill_at: Cell<f64>,
}
impl ChaserOrderInstance {
    pub fn new(
        exchange: Rc<dyn Exchange>,
        options: Rc<RefCell<ChaserOrderOptions>>,
    ) -> Result<Self, ExchangeError> {
        let input = options.borrow();
        let target_position_contracts = if input.reduce_only == Some(true) {
            None
        } else {
            let side = if input.side == OrderSide::Buy {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            let positions = exchange.base().store.positions()?;
            let existing = positions
                .iter()
                .find(|p| p.symbol == input.symbol && p.side == side)
                .map_or(0.0, |p| p.contracts);
            Some(
                if existing.is_nan() {
                    0.0
                } else {
                    existing.abs()
                } + input.size,
            )
        };
        let size = input.size;
        drop(input);
        Ok(Self {
            id: exchange.base().runtime.unique_id(),
            exchange,
            options,
            state: Rc::new(RefCell::new(DEFAULT_CHASER_STATE)),
            remaining_size: Cell::new(size),
            filled_by_order_id: RefCell::new(IndexMap::new()),
            min_tick_distance: Cell::new(0.0),
            pending_orders_update: Cell::new(false),
            fresh_order: Cell::new(false),
            disposers: RefCell::new(Vec::new()),
            target_position_contracts,
            complete_emitted: Cell::new(false),
            ever_tracked_order_ids: Rc::new(RefCell::new(IndexSet::new())),
            last_partial_fill_at: Cell::new(0.0),
        })
    }
    pub fn register_disposer(&self, disposer: Disposer) {
        self.disposers.borrow_mut().push(disposer);
    }
    pub fn dispose(&self) {
        let disposers = std::mem::take(&mut *self.disposers.borrow_mut());
        // Release the collection before callbacks: disposers may re-enter or register another disposer.
        for disposer in disposers {
            let _ = disposer();
        }
    }
    pub fn increment_attempts(&self) {
        let max = self.options.borrow().max_attempts;
        let mut state = self.state.borrow_mut();
        // ts-bug: incrementAttempts permits maxAttempts + 1 before stopping.
        if state.attempts > max {
            state.is_stopped = true;
            state.stop_reason = "Reached maximum number of attempts".into();
        } else {
            state.attempts += 1.0;
        }
    }
    pub fn mark_complete(&self, reason: Option<&str>) {
        let mut state = self.state.borrow_mut();
        state.is_stopped = true;
        state.stop_reason = reason.unwrap_or("Completed").into();
    }
    fn previous_fill(&self, id: &str) -> Fill {
        match self.filled_by_order_id.borrow().get(id).copied() {
            Some(Fill::Number(n)) if n.is_nan() || n == 0.0 => Fill::Number(0.0),
            Some(value) => value,
            None if safe_cex_num::js_table_lookup::<&str, ()>(&[], id).is_present() => {
                Fill::NativeFunctionString
            }
            None => Fill::Number(0.0),
        }
    }
    fn set_fill(&self, id: &str, value: Fill) {
        // ts-bug: the inherited __proto__ setter ignores numeric/string assignments.
        if id != "__proto__" {
            self.filled_by_order_id
                .borrow_mut()
                .insert(id.to_owned(), value);
        }
    }
    pub fn recalculate_remaining_size(&self) {
        let fills = self.filled_by_order_id.borrow();
        let mut total = 0.0;
        for key in safe_cex_num::js_own_keys(fills.keys().map(String::as_str)) {
            let fill = &fills[key];
            match fill {
                Fill::Number(n) => total += n,
                Fill::NativeFunctionString => {
                    total = f64::NAN;
                    break;
                }
            }
        }
        let size = self.options.borrow().size;
        let remaining = size - total;
        // Summed fills land a float hair off the size (0.1 + 0.058 vs 0.158);
        // that is filled, not a remainder to chase.
        self.remaining_size
            .set(if remaining.abs() <= size.abs() * 1e-9 {
                0.0
            } else {
                remaining
            });
    }
    pub fn subtract_filled_size(&self, order: &Order) {
        let filled = placement_filled(order);
        if let Fill::Number(previous) = self.previous_fill(&order.id)
            && filled > previous
        {
            self.last_partial_fill_at
                .set(self.exchange.base().runtime.now_ms());
        }
        self.set_fill(&order.id, Fill::Number(filled));
        self.recalculate_remaining_size();
        if self.remaining_size.get() <= 0.0 {
            self.mark_complete(Some("Fully filled already"));
        }
    }
    pub fn add_fill_amount(&self, id: &str, amount: f64) {
        if !amount.is_finite() || amount <= 0.0 {
            return;
        }
        self.last_partial_fill_at
            .set(self.exchange.base().runtime.now_ms());
        let value = match self.previous_fill(id) {
            Fill::Number(n) => Fill::Number(n + amount),
            Fill::NativeFunctionString => Fill::NativeFunctionString,
        };
        self.set_fill(id, value);
        self.recalculate_remaining_size();
        if self.remaining_size.get() <= 0.0 {
            self.mark_complete(Some("Fully filled via fill events"));
        }
    }
    pub fn set_filled_amount(&self, id: &str, amount: f64) {
        if let Fill::Number(previous) = self.previous_fill(id)
            && amount > previous
        {
            self.last_partial_fill_at
                .set(self.exchange.base().runtime.now_ms());
            self.set_fill(id, Fill::Number(amount));
            self.recalculate_remaining_size();
            if self.remaining_size.get() <= 0.0 {
                self.mark_complete(Some("Fully filled via REST check"));
            }
        }
    }
    pub fn claim_complete_emission(&self) -> bool {
        !self.complete_emitted.replace(true)
    }
    pub fn filled_size(&self) -> f64 {
        let size = self.options.borrow().size;
        safe_cex_num::math_min(
            size,
            safe_cex_num::math_max(0.0, size - self.remaining_size.get()),
        )
    }

    pub fn last_partial_fill_at(&self) -> f64 {
        self.last_partial_fill_at.get()
    }
    pub fn set_current_order_ids(&self, ids: Vec<String>) {
        self.ever_tracked_order_ids
            .borrow_mut()
            .extend(ids.iter().cloned());
        self.state.borrow_mut().current_orders_ids = ids;
    }
    pub fn ever_tracked_order_ids(&self) -> Rc<RefCell<IndexSet<String>>> {
        self.ever_tracked_order_ids.clone()
    }
    pub fn is_stopped(&self) -> bool {
        self.state.borrow().is_stopped
    }
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn options(&self) -> Rc<RefCell<ChaserOrderOptions>> {
        self.options.clone()
    }
    pub fn state(&self) -> Rc<RefCell<ChaserOrderState>> {
        self.state.clone()
    }
    pub fn current_order_ids(&self) -> Vec<String> {
        self.state.borrow().current_orders_ids.clone()
    }
    pub fn remaining_size(&self) -> f64 {
        self.remaining_size.get()
    }
    pub fn target_position_contracts(&self) -> Option<f64> {
        self.target_position_contracts
    }
    pub fn min_tick_distance(&self) -> f64 {
        self.min_tick_distance.get()
    }
    pub fn set_min_tick_distance(&self, value: f64) {
        self.min_tick_distance.set(value);
    }
    pub fn pending_orders_update(&self) -> bool {
        self.pending_orders_update.get()
    }
    pub fn set_pending_orders_update(&self, value: bool) {
        self.pending_orders_update.set(value);
    }
    pub fn fresh_order(&self) -> bool {
        self.fresh_order.get()
    }
    pub fn set_fresh_order(&self, value: bool) {
        self.fresh_order.set(value);
    }
    pub fn should_terminate(&self, price: f64) -> bool {
        let options = self.options.borrow();
        let Some(t) = &options.terminate else {
            return false;
        };
        if !price.is_finite() || price <= 0.0 {
            return false;
        }
        let buy = options.side == OrderSide::Buy;
        let threshold = match t.kind {
            ChaserTerminateKind::Price => t.value,
            ChaserTerminateKind::Percent => {
                t.anchor_price
                    * if buy {
                        1.0 + t.value / 100.0
                    } else {
                        1.0 - t.value / 100.0
                    }
            }
            ChaserTerminateKind::Usd => {
                if buy {
                    t.anchor_price + t.value
                } else {
                    t.anchor_price - t.value
                }
            }
        };
        if buy {
            price >= threshold
        } else {
            price <= threshold
        }
    }
}
