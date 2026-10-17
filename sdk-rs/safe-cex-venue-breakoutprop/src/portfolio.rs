//! Portfolio read formatters extracted from BreakoutpropExchange's source methods.
use safe_cex_exchange::{
    js_runtime::{
        ops,
        value::{Js, Thrown, from_json, from_typed, to_typed},
    },
    runtime::ExchangeError,
};
use safe_cex_num::{
    js_date_parse_utf16, js_number_output, js_time_clip, math_max,
    numeric_ingress::{JsNumberInput, JsNumberOutput, js_deserialize_passthrough},
};
use safe_cex_store::Store;
use safe_cex_types::{
    Balance, MarginType, Order, OrderTimeInForce, Position, PositionMode, PositionSide,
    model_utils::create_position_id,
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

fn native_open_positions(
    input: &JsNumberInput,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Option<Vec<Js>>, ExchangeError> {
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
        let raw_portfolio = raw["portfolios"]
            .get(0)
            .or_else(|| raw["portfolios"].get("0"))
            .expect("native portfolio zero has a retained counterpart");
        let mut positions = Vec::with_capacity(rows.len());
        for (index, position) in rows.iter().enumerate() {
            let side_key = to_typed::<String>(&Js::String(position.get("side")?.to_js_string()?))?;
            let side = match safe_cex_num::js_table_lookup(crate::types::POSITION_SIDE, &side_key) {
                safe_cex_num::TableHit::Mapped(value) => Js::str(value),
                safe_cex_num::TableHit::Missing => Js::Undefined,
                safe_cex_num::TableHit::Inherited(member) => {
                    Js::Builtin(safe_cex_num::PrototypeRealm::current().reference(member))
                }
            };
            let symbol = position.get("symbol")?;
            let symbol_key = to_typed::<String>(&Js::String(symbol.to_js_string()?))?;
            let open_price = position.get("openPrice")?;
            let ticker = tickers.get(&symbol_key);
            let mark_price = ticker
                .map(|t| t.mark)
                .filter(|n| truthy(*n))
                .or_else(|| ticker.map(|t| t.last).filter(|n| truthy(*n)))
                .map(Js::Number)
                .unwrap_or_else(|| open_price.clone());
            let direction = if side.strict_equals(&Js::str("long")) {
                1.0
            } else {
                -1.0
            };
            let notional = position.get("quantityNotional")?.to_number()?.abs();
            let leverage = 1.0 / position.get("marginRate")?.to_number()?;
            let unrealized_pnl = direction
                * ops::subtract(&mark_price, &open_price)?.to_number()?
                * position.get("quantity")?.to_number()?;
            let contracts = position.get("quantity")?;
            let margin_size = notional * position.get("marginRate")?.to_number()?;
            let id = position.get("symbol")?;
            let date = ops::to_primitive(&position.get("lastUpdateTime")?)?;
            let last_update = if let Js::String(text) = date {
                js_date_parse_utf16(&text, local_to_utc)
            } else {
                js_time_clip(date.to_number()?)
            };
            positions.push(Js::object(vec![
                ("symbol", symbol),
                ("side", side),
                ("entryPrice", open_price),
                ("notional", Js::Number(notional)),
                ("leverage", Js::Number(leverage)),
                ("unrealizedPnl", Js::Number(unrealized_pnl)),
                ("contracts", contracts),
                ("liquidationPrice", Js::Number(0.0)),
                ("marginSize", Js::Number(margin_size)),
                ("id", id),
                ("positionMode", from_typed(&PositionMode::OneWay)?),
                ("marginType", from_typed(&MarginType::Cross)?),
                ("lastUpdate", Js::Number(last_update)),
                ("info", from_json(&raw_portfolio["positions"][index])),
            ]));
        }
        Ok(Some(positions))
    })();
    match result {
        Err(error) => {
            report(&error.message)?;
            Ok(Some(
                store
                    .positions()
                    .iter()
                    .map(from_typed)
                    .collect::<Result<_, _>>()?,
            ))
        }
        value => value,
    }
}

fn project_position(position: Js) -> Result<Position, ExchangeError> {
    if matches!(position.get("side")?, Js::Builtin(_)) {
        let key = position.get("info")?.get("side")?.to_display();
        return Err(ExchangeError::new(format!(
            "Unrepresentable position side: {key}"
        )));
    }
    Ok(to_typed(&position)?)
}

pub fn format_open_positions(
    input: &JsNumberInput,
    store: &dyn Store,
    local_to_utc: &dyn Fn(f64) -> f64,
    report: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<Option<Vec<Position>>, ExchangeError> {
    native_open_positions(input, store, local_to_utc, report)?
        .map(|positions| positions.into_iter().map(project_position).collect())
        .transpose()
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
        let Some(positions) = native_open_positions(input, store, local_to_utc, report)? else {
            return Err(Thrown::type_error(
                "Cannot read properties of undefined (reading 'reduce')",
            )
            .into());
        };
        let mut used = 0.0;
        let mut upnl = 0.0;
        for position in &positions {
            let margin_size = position.get("marginSize")?;
            used += if margin_size.is_nullish() {
                position.get("notional")?.to_number()?.abs()
                    / math_max(position.get("leverage")?.to_number()?, 1.0)
            } else {
                margin_size.to_number()?
            };
        }
        for position in &positions {
            upnl += position.get("unrealizedPnl")?.to_number()?;
        }
        let total: f64 = to_typed(&ops::add(&cash_balance, &Js::Number(upnl))?)
            .inspect_err(|_| projection_refusal = true)?;
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
        Err(error) if !projection_refusal => {
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
    let open = native_open_positions(input, store, local_to_utc, report)?.ok_or_else(|| {
        ExchangeError::from(Thrown::type_error(
            "Cannot read properties of undefined (reading 'filter')",
        ))
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
    let mut positions = Vec::new();
    for position in open {
        let symbol = position.get("symbol")?;
        if symbols
            .iter()
            .any(|value| symbol.strict_equals(&Js::str(value)))
        {
            positions.push(project_position(position)?);
        }
    }
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
        let raw_portfolio = raw["portfolios"]
            .get(0)
            .or_else(|| raw["portfolios"].get("0"))
            .expect("native portfolio zero has a retained counterpart");
        let mut orders = Vec::with_capacity(rows.len());
        for (index, order) in rows.iter().enumerate() {
            let mut mapped =
                |table: &[(&str, &str)], key: &str, field| match safe_cex_num::js_table_lookup(
                    table, key,
                ) {
                    safe_cex_num::TableHit::Mapped(value) => Js::str(value),
                    safe_cex_num::TableHit::Missing => Js::Undefined,
                    safe_cex_num::TableHit::Inherited(_) => {
                        refusal
                            .get_or_insert_with(|| format!("Unrepresentable order {field}: {key}"));
                        Js::Undefined
                    }
                };
            let id = order.get("orderCode")?;
            let status = mapped(
                crate::types::ORDER_STATUS,
                &to_typed::<String>(&Js::String(order.get("status")?.to_js_string()?))?,
                "status",
            );
            let symbol = order.get("instrument")?;
            let kind = mapped(
                crate::types::ORDER_TYPE,
                &to_typed::<String>(&Js::String(order.get("type")?.to_js_string()?))?,
                "type",
            );
            let leg = order.get("legs")?.get("0")?;
            let side = mapped(
                crate::types::ORDER_SIDE,
                &to_typed::<String>(&Js::String(leg.get("side")?.to_js_string()?))?,
                "side",
            );
            let price = leg.get("price")?;
            let amount = leg.get("quantity")?;
            let filled = leg.get("filledQuantity")?;
            let remaining = leg.get("remainingQuantity")?;
            let reduce_only = leg.get("positionEffect")?.strict_equals(&Js::str("CLOSE"));
            let date = ops::to_primitive(&order.get("transactionTime")?)?;
            let last_update = if let Js::String(text) = date {
                js_date_parse_utf16(&text, local_to_utc)
            } else {
                js_time_clip(date.to_number()?)
            };
            orders.push(Js::object(vec![
                ("id", id),
                ("status", status),
                ("symbol", symbol),
                ("type", kind),
                ("side", side),
                ("price", price),
                ("amount", amount),
                ("filled", filled),
                ("remaining", remaining),
                ("reduceOnly", Js::Bool(reduce_only)),
                ("info", from_json(&raw_portfolio["orders"][index])),
                (
                    "timeInForce",
                    from_typed(&OrderTimeInForce::GoodTillCancel)?,
                ),
                ("lastUpdate", Js::Number(last_update)),
            ]));
        }
        Ok(Some(orders))
    })();
    let rows = match result {
        Ok(rows) => rows,
        Err(error) => {
            report(&error.message)?;
            return Ok(Some(store.orders().to_vec()));
        }
    };
    if let Some(error) = refusal {
        return Err(ExchangeError::new(error));
    }
    rows.map(|orders| {
        orders
            .iter()
            .map(to_typed)
            .collect::<Result<Vec<Order>, _>>()
            .map_err(Into::into)
    })
    .transpose()
}
