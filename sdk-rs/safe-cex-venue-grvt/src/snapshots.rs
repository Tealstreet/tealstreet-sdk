//! GRVT snapshot and lifecycle methods against shared Store and configured host hooks.
use indexmap::IndexMap;
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_store::{Store, StoreUpdate, model_patch::ModelPatch};
use safe_cex_types::{Order, Position, PositionMode, PositionSide, Ticker};
use serde_json::{Value, json};
use std::{cell::Cell, future::Future};

/// ts: GrvtExchange.recordNonce. Updates preserve insertion order and earliest time.
pub fn record_nonce(
    creation_times: &mut IndexMap<String, Option<f64>>,
    real_ids: &mut IndexMap<String, String>,
    nonce: &str,
    real_order_id: Option<&str>,
    creation_time_ms: Option<f64>,
) {
    let valid = creation_time_ms.filter(|time| time.is_finite() && *time > 0.0);
    match creation_times.get_mut(nonce) {
        None => {
            creation_times.insert(nonce.to_owned(), valid);
        }
        Some(existing) => {
            if let Some(time) = valid
                && existing.is_none_or(|prior| time < prior)
            {
                *existing = Some(time);
            }
        }
    }
    if let Some(id) = real_order_id.filter(|id| !id.is_empty() && !matches!(*id, "0x00" | "0x0")) {
        real_ids.insert(nonce.to_owned(), id.to_owned());
    }
    while creation_times.len() > 200 {
        let (oldest, _) = creation_times
            .shift_remove_index(0)
            .expect("nonempty nonce map");
        real_ids.shift_remove(&oldest);
    }
}

/// ts: GrvtExchange.getPositionMode.
pub async fn get_position_mode() -> PositionMode {
    PositionMode::OneWay
}

/// ts: GrvtExchange#calculatePositionUpnlFromLastPrice.
pub fn calculate_position_upnl_from_last_price(
    position: &Position,
    store: &dyn Store,
) -> Result<Option<f64>, ExchangeError> {
    let Some(last) = store
        .tickers_by_symbol()?
        .get(&position.symbol)
        .map(|ticker| ticker.last)
    else {
        return Ok(None);
    };
    if !truthy_number(last)
        || last <= 0.0
        || !truthy_number(position.entry_price)
        || position.entry_price <= 0.0
        || !truthy_number(position.contracts)
    {
        return Ok(None);
    }
    let direction = if position.side == PositionSide::Long {
        1.0
    } else {
        -1.0
    };
    let derived = direction * (last - position.entry_price) * position.contracts.abs();
    Ok(derived.is_finite().then_some(derived))
}

fn truthy_number(value: f64) -> bool {
    value != 0.0 && !value.is_nan()
}

/// ts: GrvtExchange#calculateUpnlFromPositions. Addition order is source array order.
pub fn calculate_upnl_from_positions(
    positions: &[Position],
    store: &dyn Store,
) -> Result<f64, ExchangeError> {
    let mut total = 0.0;
    for position in positions {
        if !truthy_number(position.contracts) {
            continue;
        }
        if let Some(derived) = calculate_position_upnl_from_last_price(position, store)? {
            total += derived;
        } else if position.unrealized_pnl.is_finite() {
            total += position.unrealized_pnl;
        }
    }
    Ok(total)
}

/// ts: GrvtExchange.syncBalanceUpnlFromPositions. Position notification precedes balance read.
pub fn sync_balance_upnl_from_positions(
    store: &dyn Store,
    positions: Option<&[Position]>,
) -> Result<(), ExchangeError> {
    let defaults;
    let positions = match positions {
        Some(positions) => positions,
        None => {
            defaults = store.positions()?.to_vec();
            &defaults
        }
    };
    let mut updates = Vec::new();
    let mut total = 0.0;
    for position in positions {
        if !truthy_number(position.contracts) {
            continue;
        }
        if let Some(derived) = calculate_position_upnl_from_last_price(position, store)? {
            total += derived;
            let old = if truthy_number(position.unrealized_pnl) {
                position.unrealized_pnl
            } else {
                0.0
            };
            if (old - derived).abs() > 1e-9 {
                let mut patch = ModelPatch::default();
                patch.set_number("unrealizedPnl", derived);
                updates.push((Some(position.id.clone()), patch));
            }
        } else if position.unrealized_pnl.is_finite() {
            total += position.unrealized_pnl;
        }
    }
    if !total.is_finite() {
        return Ok(());
    }
    if !updates.is_empty() {
        store.update_positions(&updates)?;
    }
    // Release the Ref before notifying. Subscribers may synchronously mutate balance.
    let mut balance = store.balance()?.clone();
    let old = if truthy_number(balance.upnl) {
        balance.upnl
    } else {
        0.0
    };
    if (old - total).abs() > 1e-9 {
        balance.upnl = total;
        store.update(
            StoreUpdate {
                balance: Some(balance),
                ..Default::default()
            },
            false,
        )?;
    }
    Ok(())
}

/// ts: GrvtExchange.getFakePositions. Duplicate symbols use the last existing position.
pub fn get_fake_positions(
    store: &dyn Store,
    default_leverage: f64,
) -> Result<Vec<Position>, ExchangeError> {
    let positions = store.positions()?;
    let existing: IndexMap<_, _> = positions.iter().map(|p| (&p.symbol, p.side)).collect();
    let leverage = store.fetched_leverage()?;
    store
        .markets()?
        .iter()
        .map(|market| {
            if !leverage.contains_key(&market.symbol)
                && matches!(
                    safe_cex_num::js_table_lookup::<&str, ()>(&[], &market.symbol),
                    safe_cex_num::TableHit::Inherited(_)
                )
            {
                let mut error = ExchangeError::new(
                    "Inherited leverage has no numeric buyLeverage for a typed position",
                );
                error.name = "RepresentationError".into();
                return Err(error);
            }
            let fetched = leverage
                .get(&market.symbol)
                .filter(|value| value.is_fetching != Some(true));
            Ok(Position {
                id: safe_cex_types::model_utils::create_position_id(&market.symbol, None, None),
                symbol: market.symbol.clone(),
                side: existing
                    .get(&market.symbol)
                    .copied()
                    .unwrap_or(safe_cex_types::ModelField::Value(PositionSide::Long)),
                entry_price: 0.0,
                notional: 0.0,
                leverage: fetched.map_or(default_leverage, |value| value.buy_leverage),
                unrealized_pnl: 0.0,
                contracts: 0.0,
                liquidation_price: 0.0,
                margin_type: (Some(safe_cex_types::MarginType::Isolated)).into(),
                position_mode: (Some(PositionMode::OneWay)).into(),
                realized_pnl: (None).into(),
                margin_size: (None).into(),
                info: (None).into(),
                extra: (None).into(),
                last_update: (None).into(),
                option: (None).into(),
            })
        })
        .collect()
}

/// ts: GrvtExchange.fetchTickers. Logging errors pass through the source catch.
pub fn fetch_tickers(
    store: &dyn Store,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
) -> std::future::Ready<Result<Vec<Ticker>, ExchangeError>> {
    let markets = match store.markets() {
        Ok(markets) => markets,
        Err(error) => {
            let error = ExchangeError::from(error);
            let result = log(
                &format!("Error fetching tickers: {}", error.js_error_string()),
                LogSeverity::Error,
            )
            .and(Err(error));
            return std::future::ready(result);
        }
    };
    let tickers: Vec<_> = markets
        .iter()
        .map(|market| Ticker {
            id: market.id.clone(),
            symbol: market.symbol.clone(),
            bid: 0.0,
            ask: 0.0,
            last: 0.0,
            mark: 0.0,
            index: 0.0,
            mid_price: None,
            timestamp: None,
            percentage: 0.0,
            open_interest: 0.0,
            funding_rate: 0.0,
            funding_time: 0.0,
            volume: 0.0,
            quote_volume: 0.0,
            info: Some(json!({}).into()),
        })
        .collect();
    if let Err(error) = log(
        &format!("Returning {} simulated GRVT tickers", tickers.len()),
        LogSeverity::Debug,
    ) {
        if let Err(replacement) = log(
            &format!("Error fetching tickers: {}", error.js_error_string()),
            LogSeverity::Error,
        ) {
            return std::future::ready(Err(replacement));
        }
        return std::future::ready(Err(error));
    }
    std::future::ready(Ok(tickers))
}

/// ts: GrvtExchange.dispose. Mark disposed even if the first socket rejects cleanup.
pub fn dispose(
    disposed: &Cell<bool>,
    public: impl FnOnce() -> Result<(), ExchangeError>,
    private: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    disposed.set(true);
    public()?;
    private()
}

/// ts: GrvtExchange.fetchOrders. Only a singleton filter is forwarded.
pub fn fetch_orders<'a, F: Future<Output = Result<Vec<Order>, ExchangeError>>>(
    symbols: Option<&'a [String]>,
    fetch_open_orders: impl FnOnce(Option<&'a str>) -> F,
) -> F {
    fetch_open_orders(
        symbols
            .filter(|values| values.len() == 1)
            .map(|values| values[0].as_str()),
    )
}

/// ts: GrvtExchange.getAccount. `post` is the configured trading-client request hook.
pub fn get_account<F: Future<Output = Result<Value, ExchangeError>>>(
    sub_account_id: Option<&str>,
    account_id: impl FnOnce() -> Option<String>,
    post: impl FnOnce(&str, Value) -> F,
    mut log: impl FnMut(&str, LogSeverity) -> Result<(), ExchangeError>,
) -> impl Future<Output = Result<(String, Option<String>), ExchangeError>> {
    let request = json!({"sub_account_id": sub_account_id.unwrap_or("")});
    let response = post("/lite/v1/account_summary", request);
    async move {
        match response.await {
            Ok(_) => Ok((account_id().unwrap_or_default(), None)),
            Err(error) => {
                log(
                    &format!("Error fetching account: {}", error.js_error_string()),
                    LogSeverity::Error,
                )?;
                Err(error)
            }
        }
    }
}
