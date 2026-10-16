//! Portable order arithmetic from script-api-core/src/order.
use crate::{add, handle_rounding_error, math_round, sizing::base_to_native_contracts, subtract};
use safe_cex_types::{Market, PositionSide};

pub fn round_to_step(value: f64, step: f64) -> f64 {
    if step > 0.0 {
        handle_rounding_error(math_round(value / step) * step)
    } else {
        value
    }
}

pub fn round_market_order_size(value: f64, step: f64, reduce_only: bool) -> f64 {
    let rounded = round_to_step(value, step);
    if rounded > 0.0 || !reduce_only {
        rounded
    } else {
        value
    }
}

pub fn round_close_size(value: f64, step: f64) -> f64 {
    round_market_order_size(value, step, true)
}

#[derive(Debug, Clone, Copy)]
pub struct BreakevenInputs {
    pub entry_price: f64,
    pub size: f64,
    pub side: PositionSide,
    pub banked_pnl: Option<f64>,
    pub fees: Option<f64>,
}

fn truthy_or_zero(value: Option<f64>) -> f64 {
    value.filter(|n| *n != 0.0 && !n.is_nan()).unwrap_or(0.0)
}

pub fn breakeven_price(inputs: &BreakevenInputs) -> f64 {
    let BreakevenInputs {
        entry_price,
        size,
        side,
        banked_pnl,
        fees,
    } = *inputs;
    if !entry_price.is_finite() {
        return entry_price;
    }
    let magnitude = truthy_or_zero(Some(size)).abs();
    if magnitude == 0.0 {
        return entry_price;
    }
    let banked_pnl = truthy_or_zero(banked_pnl);
    let fees = truthy_or_zero(fees);
    if banked_pnl == 0.0 && fees == 0.0 {
        return entry_price;
    }
    let per_contract = subtract(banked_pnl, fees) / magnitude;
    let result = if side == PositionSide::Long {
        subtract(entry_price, per_contract)
    } else {
        add(entry_price, per_contract)
    };
    if result.is_finite() {
        result
    } else {
        entry_price
    }
}

pub fn notional_to_native_size(notional: f64, market: &Market, price: f64) -> f64 {
    let contract_size = market.contract_size.filter(|n| *n > 0.0).unwrap_or(1.0);
    if market.is_inverse == Some(true) {
        notional / contract_size
    } else {
        base_to_native_contracts(market, notional / price)
    }
}

pub fn should_round_size_down(market: &Market, exchange_name: Option<&str>) -> bool {
    market.is_spot == Some(true) && matches!(exchange_name, Some("hyperliquid" | "hyperliquidspot"))
}
