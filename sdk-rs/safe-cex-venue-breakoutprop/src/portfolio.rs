//! Portfolio read formatters extracted from BreakoutpropExchange's source methods.
use safe_cex_exchange::{
    js_runtime::{
        ops,
        value::{Js, Thrown, from_json, to_typed},
    },
    runtime::ExchangeError,
};
use safe_cex_num::{
    js_date_parse_utf16, js_number_output, js_time_clip, math_max,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough},
};
use safe_cex_store::Store;
use safe_cex_types::{
    Balance, MarginType, ModelField, Order, OrderSide, OrderStatus, OrderTimeInForce, OrderType,
    Position, PositionMode, PositionSide, model_utils::create_position_id,
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
fn decode(input: &JsNumberInput) -> Result<(Js, Value), ExchangeError> {
    let raw = input
        .clone()
        .decode_with(|d| js_deserialize_passthrough(d).map(|raw| RawPortfolio { raw }))
        .map_err(|e| ExchangeError::new(e.to_string()))?;
    Ok((from_json(&raw.raw), raw.raw))
}
fn truthy(number: f64) -> bool {
    number != 0.0 && !number.is_nan()
}

pub fn format_open_positions(
    input: &JsNumberInput,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Option<Vec<Position>>, ExchangeError> {
    let mut refusal = false;
    let result: Result<_, ExchangeError> = (|| {
        let (data, raw) = decode(input)?;
        let portfolio = data.get("portfolios")?.get("0")?;
        if portfolio.is_nullish() {
            return Ok(None);
        }
        let tickers = store.tickers_by_symbol();
        let rows = portfolio.get("positions")?;
        if rows.is_nullish() {
            rows.get("map")?;
        }
        let rows = rows.array_values().ok_or_else(|| {
            Thrown::type_error("data.portfolios[0]?.positions.map is not a function")
        })?;
        let raw_portfolio = raw["portfolios"].get(0)
            .or_else(|| raw["portfolios"].get("0"))
            .expect("native portfolio zero has a retained counterpart");
        let mut positions = Vec::with_capacity(rows.len());
        for (index, position) in rows.iter().enumerate() {
            let side_key = to_typed::<String>(&Js::String(position.get("side")?.to_js_string()?))?;
            let symbol: String = to_typed(&position.get("symbol")?).map_err(|e| {
                refusal = true;
                e
            })?;
            let open_price = position.get("openPrice")?;
            let ticker = tickers.get(&symbol);
            let mark_price = ticker
                .map(|t| t.mark)
                .filter(|n| truthy(*n))
                .or_else(|| ticker.map(|t| t.last).filter(|n| truthy(*n)))
                .map(Js::Number)
                .unwrap_or_else(|| open_price.clone());
            let direction = if side_key == "BUY" { 1.0 } else { -1.0 };
            let notional = position.get("quantityNotional")?.to_number()?.abs();
            let leverage = 1.0 / position.get("marginRate")?.to_number()?;
            let unrealized_pnl = direction
                * ops::subtract(&mark_price, &open_price)?.to_number()?
                * position.get("quantity")?.to_number()?;
            let margin_size = notional * position.get("marginRate")?.to_number()?;
            let date = ops::to_primitive(&position.get("lastUpdateTime")?)?;
            let last_update = if let Js::String(text) = date {
                js_date_parse_utf16(&text, local_to_utc)
            } else {
                js_time_clip(date.to_number()?)
            };
            let side = match safe_cex_num::js_table_lookup(crate::types::POSITION_SIDE, &side_key) {
                safe_cex_num::TableHit::Mapped(&"long") => ModelField::Value(PositionSide::Long),
                safe_cex_num::TableHit::Mapped(_) => ModelField::Value(PositionSide::Short),
                safe_cex_num::TableHit::Missing => ModelField::Undefined,
                safe_cex_num::TableHit::Inherited(_) => {
                    refusal = true;
                    return Err(ExchangeError::new(format!(
                        "Unrepresentable position side: {}",
                        side_key
                    )));
                }
            };
            positions.push(Position {
                symbol: symbol.clone(),
                side,
                entry_price: to_typed(&position.get("openPrice")?).map_err(|e| {
                    refusal = true;
                    e
                })?,
                notional,
                leverage,
                unrealized_pnl,
                contracts: to_typed(&position.get("quantity")?).map_err(|e| {
                    refusal = true;
                    e
                })?,
                liquidation_price: 0.0,
                margin_size: (Some(margin_size)).into(),
                id: create_position_id(&symbol, None, None),
                position_mode: (Some(PositionMode::OneWay)).into(),
                margin_type: (Some(MarginType::Cross)).into(),
                last_update: (Some(last_update)).into(),
                info: (Some(raw_portfolio["positions"][index].clone())).into(),
                realized_pnl: (None).into(),
                extra: (None).into(),
                option: (None).into(),
            });
        }
        Ok(Some(positions))
    })();
    match result {
        Err(error) if !refusal => {
            report(&error.message)?;
            Ok(Some(store.positions().to_vec()))
        }
        value => value,
    }
}

pub fn format_balance(
    input: &JsNumberInput,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Balance, ExchangeError> {
    let mut projection_refusal = false;
    let result: Result<_, ExchangeError> = (|| {
        let (data, raw) = decode(input)?;
        let portfolio = data.get("portfolios")?.get("0")?;
        let balances = if portfolio.is_nullish() {
            Js::Undefined
        } else {
            portfolio.get("balances")?
        };
        let mut cash_balance = Js::Number(0.0);
        if !balances.is_nullish() {
            let rows = balances.array_values().ok_or_else(|| {
                Thrown::type_error("data.portfolios[0]?.balances?.find is not a function")
            })?;
            for balance in rows {
                if balance.get("currency")?.strict_equals(&Js::str("USD")) {
                    let value = balance.get("value")?;
                    if value.truthy() {
                        cash_balance = value;
                    }
                    break;
                }
            }
        }
        let Some(positions) = format_open_positions(input, store, local_to_utc, report)? else {
            return Err(Thrown::type_error(
                "Cannot read properties of undefined (reading 'reduce')",
            )
            .into());
        };
        let used = positions.iter().fold(0.0, |total, p| {
            total
                + p.margin_size
                    .unwrap_or(p.notional.abs() / math_max(p.leverage, 1.0))
        });
        let upnl = positions
            .iter()
            .fold(0.0, |total, p| total + p.unrealized_pnl);
        let total: f64 = to_typed(&ops::add(&cash_balance, &Js::Number(upnl))?).map_err(|e| {
            projection_refusal = true;
            e
        })?;
        Ok(Balance {
            total,
            free: total - used,
            upnl,
            used,
            info: Some(raw),
            currencies: None,
            coins: None,
        })
    })();
    match result {
        Err(error)
            if !projection_refusal
                && !error.message.starts_with("Unrepresentable position side: ") =>
        {
            report(&error.message)?;
            Ok(store.balance().clone())
        }
        value => value,
    }
}

pub fn format_positions(
    input: &JsNumberInput,
    store: &dyn Store,
    symbols: Option<&[String]>,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Vec<Position>, ExchangeError> {
    let open = format_open_positions(input, store, local_to_utc, report)?.ok_or_else(|| {
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
            side: (PositionSide::Long).into(),
            leverage: 0.0,
            margin_type: (Some(MarginType::Cross)).into(),
            position_mode: (Some(PositionMode::OneWay)).into(),
            entry_price: 0.0,
            notional: 0.0,
            unrealized_pnl: 0.0,
            contracts: 0.0,
            liquidation_price: 0.0,
            margin_size: (None).into(),
            info: (None).into(),
            extra: (None).into(),
            realized_pnl: (None).into(),
            last_update: (None).into(),
            option: (None).into(),
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
    let mut refusal = None;
    let mut projection_refusal = false;
    let result: Result<_, ExchangeError> = (|| {
        let (data, raw) = decode(input)?;
        let portfolio = data.get("portfolios")?.get("0")?;
        if portfolio.is_nullish() {
            return Ok(None);
        }
        let rows = portfolio.get("orders")?;
        if rows.is_nullish() {
            rows.get("map")?;
        }
        let rows = rows.array_values().ok_or_else(|| {
            Thrown::type_error("data.portfolios[0]?.orders.map is not a function")
        })?;
        let raw_portfolio = raw["portfolios"].get(0)
            .or_else(|| raw["portfolios"].get("0"))
            .expect("native portfolio zero has a retained counterpart");
        let mut orders = Vec::with_capacity(rows.len());
        // Stage closed-output refusals until later source errors have had their turn.
        for (index, order) in rows.iter().enumerate() {
            let order_code = order.get("orderCode")?;
            let mut mapped =
                |table, key: &str, field| match safe_cex_num::js_table_lookup(table, key) {
                    safe_cex_num::TableHit::Mapped(value) => Some(*value),
                    safe_cex_num::TableHit::Missing => None,
                    safe_cex_num::TableHit::Inherited(_) => {
                        refusal
                            .get_or_insert_with(|| format!("Unrepresentable order {field}: {key}"));
                        None
                    }
                };
            let status = mapped(
                crate::types::ORDER_STATUS,
                &to_typed::<String>(&Js::String(order.get("status")?.to_js_string()?))?,
                "status",
            )
            .map(|_| ModelField::Value(OrderStatus::Open))
            .unwrap_or(ModelField::Undefined);
            let order_type = mapped(
                crate::types::ORDER_TYPE,
                &to_typed::<String>(&Js::String(order.get("type")?.to_js_string()?))?,
                "type",
            )
            .map(|kind| {
                ModelField::Value(match kind {
                    "limit" => OrderType::Limit,
                    "market" => OrderType::Market,
                    _ => OrderType::StopLoss,
                })
            })
            .unwrap_or(ModelField::Undefined);
            let symbol = order.get("instrument")?;
            let leg = order.get("legs")?.get("0")?;
            if leg.is_nullish() {
                return Err(Thrown::type_error(&format!(
                    "Cannot read properties of {} (reading 'side')",
                    leg.to_display()
                ))
                .into());
            }
            let side = mapped(
                crate::types::ORDER_SIDE,
                &to_typed::<String>(&Js::String(leg.get("side")?.to_js_string()?))?,
                "side",
            )
            .map(|side| {
                ModelField::Value(if side == "buy" {
                    OrderSide::Buy
                } else {
                    OrderSide::Sell
                })
            })
            .unwrap_or(ModelField::Undefined);
            let last_update = {
                let date = ops::to_primitive(&order.get("transactionTime")?)?;
                if let Js::String(text) = date {
                    js_date_parse_utf16(&text, local_to_utc)
                } else {
                    js_time_clip(date.to_number()?)
                }
            };
            let projected: Result<Order, ExchangeError> = (|| {
                Ok(Order {
                    client_order_id: (None).into(),
                    id: to_typed(&order_code)?,
                    status,
                    symbol: to_typed(&symbol)?,
                    r#type: order_type,
                    side,
                    price: to_typed(&leg.get("price")?)?,
                    amount: to_typed(&leg.get("quantity")?)?,
                    filled: to_typed(&leg.get("filledQuantity")?)?,
                    remaining: to_typed(&leg.get("remainingQuantity")?)?,
                    reduce_only: leg.get("positionEffect")?.strict_equals(&Js::str("CLOSE")),
                    info: (Some(raw_portfolio["orders"][index].clone())).into(),
                    time_in_force: (Some(OrderTimeInForce::GoodTillCancel)).into(),
                    last_update,
                    parent_id: (None).into(),
                    close: (None).into(),
                    trigger_type: (None).into(),
                    tp_trigger_type: (None).into(),
                    sl_trigger_type: (None).into(),
                    take_profit: (None).into(),
                    stop_loss: (None).into(),
                    extra: (None).into(),
                })
            })();
            orders.push(projected.map_err(|error| {
                projection_refusal = true;
                error
            })?);
        }
        if let Some(error) = refusal.as_ref() {
            return Err(ExchangeError::new(error.clone()));
        }
        Ok(Some(orders))
    })();
    match result {
        Err(error) if !projection_refusal && refusal.as_deref() != Some(error.message.as_str()) => {
            report(&error.message)?;
            Ok(Some(store.orders().to_vec()))
        }
        value => value,
    }
}
