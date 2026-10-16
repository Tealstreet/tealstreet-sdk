//! Store-aware pure helpers; no transport or scheduling ownership.
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, Order, OrderSide, OrderType, PlaceOrderOpts, PositionSide};
use std::collections::HashMap;

// ts: KucoinExchange.toKucoinSymbol
pub fn to_kucoin_symbol(symbol: Option<&str>) -> String {
    symbol.unwrap_or_default().to_owned()
}
// ts: KucoinExchange.toInternalSymbol
pub fn to_internal_symbol(symbol: Option<&str>) -> String {
    let symbol = to_kucoin_symbol(symbol);
    if symbol.is_empty() {
        symbol
    } else {
        safe_cex_types::symbol_converter::to_binance(&symbol, &ExchangeName::Kucoin)
    }
}
// ts: KucoinExchange.toApiSymbol
pub fn to_api_symbol(symbol: Option<&str>, store: &dyn Store) -> String {
    let Some(symbol) = symbol.filter(|s| !s.is_empty()) else {
        return String::new();
    };
    store
        .markets_by_symbol()
        .get(symbol)
        .map(|m| m.id.as_str())
        .filter(|id| !id.is_empty())
        .unwrap_or(symbol)
        .to_owned()
}
// ts: KucoinExchange.contractSizeFor
pub fn contract_size_for(symbol: Option<&str>, store: &dyn Store) -> f64 {
    let markets = store.markets_by_symbol();
    let value = symbol
        .filter(|s| !s.is_empty())
        .and_then(|s| markets.get(s))
        .and_then(|m| m.contract_size);
    // Source numeric || keeps Infinity, but defaults zero and NaN.
    value.filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(1.0)
}
// ts: KucoinExchange.nativeAmountForCancelReplace
pub fn native_amount_for_cancel_replace(order: &Order, store: &dyn Store) -> f64 {
    order
        .extra
        .as_ref()
        .and_then(|e| e.remaining_native)
        .unwrap_or_else(|| {
            safe_cex_num::divide(
                order.remaining,
                contract_size_for(Some(&order.symbol), store),
                None,
            )
        })
}
// ts: KucoinExchange.reconcileFetchedOrderRemainders
pub fn reconcile_fetched_order_remainders(
    mut fetched: Vec<Order>,
    store: &dyn Store,
) -> Vec<Order> {
    let existing = store.orders();
    let by_id: HashMap<&str, &Order> = existing.iter().map(|o| (o.id.as_str(), o)).collect();
    for order in &mut fetched {
        let previous = by_id
            .get(order.id.as_str())
            .and_then(|o| o.extra.as_ref())
            .and_then(|e| e.remaining_native);
        let current = order.extra.as_ref().and_then(|e| e.remaining_native);
        if let (Some(previous), Some(current)) = (previous, current)
            && previous.is_finite()
            && previous >= 0.0
            && current.is_finite()
            && current >= 0.0
            && previous < current
        {
            order.remaining =
                safe_cex_num::multiply(previous, contract_size_for(Some(&order.symbol), store));
            if let Some(extra) = order.extra.as_mut() {
                extra.remaining_native = Some(previous);
            }
        }
    }
    fetched
}
// ts: KucoinExchange.getOrderPlanTypeAndAmount
pub fn get_order_plan_type_and_amount(
    opts: &PlaceOrderOpts,
    store: &dyn Store,
) -> Result<(&'static str, f64), String> {
    if opts.r#type == OrderType::TrailingStopLoss {
        return Ok(("track_plan", opts.amount));
    }
    let mut infinite_stop = true;
    if opts.amount != 0.0 {
        let base =
            safe_cex_num::multiply(opts.amount, contract_size_for(Some(&opts.symbol), store));
        let side = if opts.side == OrderSide::Buy {
            PositionSide::Short
        } else {
            PositionSide::Long
        };
        if store
            .positions()
            .iter()
            .any(|p| p.symbol == opts.symbol && p.side == side && p.contracts >= base)
        {
            infinite_stop = false;
        }
    }
    match (infinite_stop, opts.r#type) {
        (true, OrderType::StopLoss) => Ok(("pos_loss", 0.0)),
        (true, OrderType::TakeProfit) => Ok(("pos_profit", 0.0)),
        (false, OrderType::StopLoss) => Ok(("loss_plan", opts.amount)),
        (false, OrderType::TakeProfit) => Ok(("profit_plan", opts.amount)),
        _ => Err(format!(
            "Unknown order type: {}",
            crate::order_helpers::order_type_text(opts.r#type)
        )),
    }
}
// ts: KucoinSpotExchange.getUsdPrice
pub fn get_usd_price(currency: &str, store: &dyn Store) -> f64 {
    if matches!(currency, "USD" | "USDT" | "USDC") {
        return 1.0;
    }
    let tickers = store.tickers_by_symbol();
    let ticker = ["USDT", "USDC", "USD"]
        .iter()
        .find_map(|quote| tickers.get(&format!("{currency}-{quote}")));
    ticker
        .map(|t| t.last)
        .filter(|last| !last.is_nan() && *last != 0.0)
        .unwrap_or(0.0)
}

// ts: KucoinExchange.apiMarginCoin
// ts-bug: KucoinExchange.apiMarginCoin retains the legacy SUSDT testnet spelling.
pub fn api_margin_coin(testnet: bool) -> &'static str {
    if testnet { "SUSDT" } else { "USDT" }
}
