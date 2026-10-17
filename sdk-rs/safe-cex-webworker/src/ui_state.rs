use indexmap::IndexSet;
use safe_cex_exchange::exchange::Exchange;
use serde_json::Value;

pub fn handle_active_symbols_change(
    exchange: &dyn Exchange,
    data: Option<&Value>,
) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
    let Some(symbols) = data
        .and_then(|data| data.get("activeSymbols"))
        .and_then(Value::as_array)
    else {
        return Ok(());
    };
    let mut merged: IndexSet<String> = exchange
        .base()
        .get_active_ticker_symbols()
        .into_iter()
        .collect();
    for symbol in symbols.iter().filter_map(Value::as_str) {
        if !safe_cex_num::js_trim(symbol).is_empty() {
            merged.insert(symbol.to_owned());
        }
    }
    exchange
        .base()
        .set_active_ticker_symbols(&merged.into_iter().collect::<Vec<_>>())
}

pub fn update_positions_with_leverage(
    exchange: &dyn Exchange,
    symbol: &str,
    leverage: &safe_cex_types::FetchedLeverage,
) -> Result<(), safe_cex_store::StoreMutationError> {
    use safe_cex_store::model_patch::ModelPatch;
    use safe_cex_types::{PositionMode, PositionSide};

    let updates: Vec<_> = exchange
        .base()
        .store
        .positions()
        .iter()
        .filter(|position| position.symbol == symbol)
        .filter_map(|position| {
            let next = if position.position_mode == Some(PositionMode::OneWay)
                || position.side == PositionSide::Long
            {
                leverage.buy_leverage
            } else {
                leverage.sell_leverage
            };
            if position.leverage == next {
                return None;
            }
            let mut patch = ModelPatch::default();
            patch.set_number("leverage", next);
            Some((Some(position.id.clone()), patch))
        })
        .collect();
    if !updates.is_empty() {
        exchange.base().store.update_positions(&updates)?;
    }
    Ok(())
}

fn ordered_fetched_leverage(
    fetched: &indexmap::IndexMap<String, safe_cex_types::FetchedLeverage>,
) -> indexmap::IndexMap<String, safe_cex_types::FetchedLeverage> {
    safe_cex_num::js_own_keys(fetched.keys().map(String::as_str))
        .into_iter()
        .map(|key| (key.to_owned(), fetched[key].clone()))
        .collect()
}

fn clear_fetched_leverage(
    exchange: &dyn Exchange,
    symbol: &str,
) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
    let mut fetched = exchange.base().store.fetched_leverage().clone();
    fetched.shift_remove(symbol);
    exchange.base().store.update(
        safe_cex_store::StoreUpdate {
            fetched_leverage: Some(ordered_fetched_leverage(&fetched)),
            ..Default::default()
        },
        true,
    )?;
    Ok(())
}

struct MarginFetchGuard<'a> {
    exchange: &'a dyn Exchange,
    symbol: &'a str,
    settled: bool,
}
impl Drop for MarginFetchGuard<'_> {
    fn drop(&mut self) {
        if !self.settled {
            // Store markers have no request identity; an older drop must not clear a newer fetch.
            let error = safe_cex_exchange::runtime::ExchangeError::new(
                "Worker leverage fetch cancelled; current Store marker retained",
            );
            self.exchange.base().runtime.report_exception(
                &error,
                &serde_json::json!({"method":"handleActiveMarginSymbolChange","symbol":self.symbol}),
            );
        }
    }
}

pub async fn handle_active_margin_symbol_change(
    exchange: &dyn Exchange,
    enabled: bool,
    symbol: Option<&str>,
) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
    let Some(symbol) = symbol.filter(|symbol| enabled && !symbol.is_empty()) else {
        return Ok(());
    };
    let mut fetched = exchange.base().store.fetched_leverage().clone();
    let entries: Vec<_> = fetched.iter().collect();
    // ts-bug: inherited Object.prototype entries count as existing leverage and suppress fetching.
    if safe_cex_num::js_table_lookup(&entries, symbol).is_present() {
        return Ok(());
    }
    fetched.insert(
        symbol.into(),
        safe_cex_types::FetchedLeverage {
            buy_leverage: 0.0,
            sell_leverage: 0.0,
            margin_type: None,
            position_mode: None,
            is_fetching: Some(true),
        },
    );
    exchange.base().store.update(
        safe_cex_store::StoreUpdate {
            fetched_leverage: Some(ordered_fetched_leverage(&fetched)),
            ..Default::default()
        },
        true,
    )?;
    let mut guard = MarginFetchGuard {
        exchange,
        symbol,
        settled: false,
    };
    let completion = exchange
        .fetch_leverage_for_symbol(symbol.into())
        .await
        .and_then(|leverage| match leverage {
            Some(leverage) => update_positions_with_leverage(exchange, symbol, &leverage)
                .map_err(safe_cex_exchange::runtime::ExchangeError::from),
            None => clear_fetched_leverage(exchange, symbol),
        });
    let result = match completion {
        Ok(()) => Ok(()),
        // The source catch consumes the first error only after its cleanup succeeds.
        Err(_) => clear_fetched_leverage(exchange, symbol),
    };
    guard.settled = true;
    result
}
