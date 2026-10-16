//! Portfolio read formatters extracted from BreakoutpropExchange's source methods.
use crate::types::PortfolioResponse;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    js_date_parse, js_number_output, math_max,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough},
};
use safe_cex_store::Store;
use safe_cex_types::{
    Balance, MarginType, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType, Position,
    PositionMode, PositionSide, model_utils::create_position_id,
};
use serde::Serialize;
use serde_json::Value;

#[derive(Serialize)]
#[serde(transparent)]
struct RawPortfolio {
    raw: Value,
}
impl JsNumberOutput for RawPortfolio {
    fn check_number_output(&self) {
        js_number_output!(self, struct {} passthrough { raw });
    }
}
fn decode(input: &JsNumberInput) -> Result<(PortfolioResponse, Value), ExchangeError> {
    let typed = input
        .clone()
        .decode::<PortfolioResponse>()
        .map_err(|e| ExchangeError::new(e.to_string()))?;
    let raw = input
        .clone()
        .decode_with(|d| js_deserialize_passthrough(d).map(|raw| RawPortfolio { raw }))
        .map_err(|e| ExchangeError::new(e.to_string()))?;
    Ok((typed, raw.raw))
}
fn truthy(number: f64) -> bool {
    number != 0.0 && !number.is_nan()
}

pub fn format_open_positions(
    input: &JsNumberInput,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Option<Vec<Position>>, ExchangeError> {
    let (data, raw) = decode(input)?;
    let Some(portfolio) = data.portfolios.first() else {
        return Ok(None);
    };
    let tickers = store.tickers_by_symbol();
    let mut positions = Vec::with_capacity(portfolio.positions.len());
    for (index, position) in portfolio.positions.iter().enumerate() {
        let ticker = tickers.get(&position.symbol);
        let mark_price = ticker
            .map(|t| t.mark)
            .filter(|n| truthy(*n))
            .or_else(|| ticker.map(|t| t.last).filter(|n| truthy(*n)))
            .unwrap_or(position.open_price);
        let direction = if position.side == "BUY" { 1.0 } else { -1.0 };
        let notional = position.quantity_notional.abs();
        let unrealized_pnl = direction * (mark_price - position.open_price) * position.quantity;
        let last_update = js_date_parse(&position.last_update_time, local_to_utc);
        // The source calculations run before the closed shared-model refusal.
        let side = match position.side.as_str() {
            "BUY" => PositionSide::Long,
            "SELL" => PositionSide::Short,
            other => {
                return Err(ExchangeError::new(format!(
                    "Unrepresentable position side: {other}"
                )));
            }
        };
        positions.push(Position {
            symbol: position.symbol.clone(),
            side,
            entry_price: position.open_price,
            notional,
            leverage: 1.0 / position.margin_rate,
            unrealized_pnl,
            contracts: position.quantity,
            liquidation_price: 0.0,
            margin_size: Some(notional * position.margin_rate),
            id: create_position_id(&position.symbol, None, None),
            position_mode: Some(PositionMode::OneWay),
            margin_type: Some(MarginType::Cross),
            last_update: Some(last_update),
            info: Some(raw["portfolios"][0]["positions"][index].clone()),
            realized_pnl: None,
            extra: None,
            option: None,
        });
    }
    Ok(Some(positions))
}

pub fn format_balance(
    input: &JsNumberInput,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Balance, ExchangeError> {
    let (data, raw) = decode(input)?;
    let cash_balance = data
        .portfolios
        .first()
        .and_then(|p| p.balances.iter().find(|b| b.currency == "USD"))
        .map(|b| b.value)
        .filter(|n| truthy(*n))
        .unwrap_or(0.0);
    let Some(positions) = format_open_positions(input, store, local_to_utc)? else {
        report("Cannot read properties of undefined (reading 'reduce')")?;
        return Ok(store.balance().clone());
    };
    let used = positions.iter().fold(0.0, |total, p| {
        total
            + p.margin_size
                .unwrap_or(p.notional.abs() / math_max(p.leverage, 1.0))
    });
    let upnl = positions
        .iter()
        .fold(0.0, |total, p| total + p.unrealized_pnl);
    let total = cash_balance + upnl;
    Ok(Balance {
        total,
        free: total - used,
        upnl,
        used,
        info: Some(raw),
        currencies: None,
        coins: None,
    })
}

pub fn format_positions(
    input: &JsNumberInput,
    store: &dyn Store,
    symbols: Option<&[String]>,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<Vec<Position>, ExchangeError> {
    let open = format_open_positions(input, store, local_to_utc)?.ok_or_else(|| {
        ExchangeError::new("Cannot read properties of undefined (reading 'filter')")
    })?;
    let default_symbols;
    let symbols = if let Some(symbols) = symbols {
        symbols
    } else {
        default_symbols = store
            .markets()
            .iter()
            .map(|m| m.symbol.clone())
            .collect::<Vec<_>>();
        &default_symbols
    };
    let mut positions: Vec<_> = open
        .into_iter()
        .filter(|p| symbols.contains(&p.symbol))
        .collect();
    let missing: Vec<_> = symbols
        .iter()
        .filter(|symbol| !positions.iter().any(|p| &p.symbol == *symbol))
        .collect();
    for symbol in missing {
        positions.push(Position {
            id: create_position_id(symbol, None, None),
            symbol: symbol.clone(),
            side: PositionSide::Long,
            leverage: 0.0,
            margin_type: Some(MarginType::Cross),
            position_mode: Some(PositionMode::OneWay),
            entry_price: 0.0,
            notional: 0.0,
            unrealized_pnl: 0.0,
            contracts: 0.0,
            liquidation_price: 0.0,
            margin_size: None,
            info: None,
            extra: None,
            realized_pnl: None,
            last_update: None,
            option: None,
        });
    }
    Ok(positions)
}

pub fn format_orders(
    input: &JsNumberInput,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Option<Vec<Order>>, ExchangeError> {
    let (data, raw) = decode(input)?;
    let Some(portfolio) = data.portfolios.first() else {
        return Ok(None);
    };
    let mut orders = Vec::with_capacity(portfolio.orders.len());
    // Stage closed-output refusals until later source errors have had their turn.
    let mut refusal = None;
    for (index, order) in portfolio.orders.iter().enumerate() {
        let Some(leg) = order.legs.first() else {
            report("Cannot read properties of undefined (reading 'side')")?;
            return Ok(Some(store.orders().to_vec()));
        };
        let last_update = js_date_parse(&order.transaction_time, local_to_utc);
        let status = match order.status.as_str() {
            "WORKING" => OrderStatus::Open,
            other => {
                refusal.get_or_insert_with(|| format!("Unrepresentable order status: {other}"));
                OrderStatus::Open
            }
        };
        let order_type = match order.r#type.as_str() {
            "LIMIT" => OrderType::Limit,
            "MARKET" => OrderType::Market,
            "STOP" => OrderType::StopLoss,
            other => {
                refusal.get_or_insert_with(|| format!("Unrepresentable order type: {other}"));
                OrderType::Limit
            }
        };
        let side = match leg.side.as_str() {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            other => {
                refusal.get_or_insert_with(|| format!("Unrepresentable order side: {other}"));
                OrderSide::Buy
            }
        };
        orders.push(Order {
            id: order.order_code.clone(),
            status,
            symbol: order.instrument.clone(),
            r#type: order_type,
            side,
            price: leg.price,
            amount: leg.quantity,
            filled: leg.filled_quantity,
            remaining: leg.remaining_quantity,
            reduce_only: leg.position_effect == "CLOSE",
            info: Some(raw["portfolios"][0]["orders"][index].clone()),
            time_in_force: Some(OrderTimeInForce::GoodTillCancel),
            last_update,
            parent_id: None,
            close: None,
            trigger_type: None,
            tp_trigger_type: None,
            sl_trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        });
    }
    if let Some(error) = refusal {
        return Err(ExchangeError::new(error));
    }
    Ok(Some(orders))
}
