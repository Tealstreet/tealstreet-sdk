use safe_cex_types::{OrderSide, OrderType, PlaceOrderOpts};
use std::cell::Cell;

pub(crate) struct NukeGuard<'a> {
    latch: &'a Cell<bool>,
    armed: Cell<bool>,
}
impl<'a> NukeGuard<'a> {
    pub(crate) fn new(latch: &'a Cell<bool>) -> Self {
        latch.set(true);
        Self {
            latch,
            armed: Cell::new(true),
        }
    }
    pub(crate) fn finish(&self) {
        if self.armed.replace(false) {
            self.latch.set(false);
        }
    }
}
impl Drop for NukeGuard<'_> {
    fn drop(&mut self) {
        self.finish();
    }
}

#[cfg(test)]
#[path = "order_operations_tests.rs"]
mod tests;

/// Shared native order constructor used by exchange operations and cross-trade.
pub fn order_options(
    symbol: String,
    side: OrderSide,
    kind: OrderType,
    amount: f64,
) -> PlaceOrderOpts {
    PlaceOrderOpts {
        symbol,
        side,
        r#type: kind,
        amount,
        price: None,
        trigger_price: None,
        stop_loss: None,
        take_profit: None,
        reduce_only: None,
        time_in_force: None,
        trigger_type: None,
        tp_trigger_type: None,
        sl_trigger_type: None,
        client_order_id: None,
        max_slippage_pct: None,
        close: None,
        implied_volatility: None,
    }
}
