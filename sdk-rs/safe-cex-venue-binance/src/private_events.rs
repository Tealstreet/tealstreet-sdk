//! Eight extracted private WebSocket event bodies. Calls remain at their source
//! sites, with bound current methods captured before argument/clock evaluation.
//! Concrete class/Store composition and executable property hooks remain pending.
use crate::{
    private_event_value::*,
    request_weight::{Failure, type_error},
};
use safe_cex_types::{MarginType, OrderStatus, OrderType, PositionMode, PositionSide};
use std::{cell::Cell, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrivateOperation {
    Fill,
    UpdateOrder,
    AddOrder,
    OrderStatus,
    ThrottleOrders,
    MarkPosition,
    ThrottlePositions,
    AddPosition,
    UpdatePosition,
    UpdateStore,
}
pub type PrivateHandler = Rc<dyn Fn(Vec<EventValue>) -> Result<(), Failure>>;
pub trait PrivateEventBindings {
    fn capture(&mut self, operation: PrivateOperation) -> Result<PrivateHandler, Failure>;
    fn now(&mut self) -> Result<f64, Failure>;
    fn orders(&mut self) -> Result<EventArray, Failure>;
    fn positions(&mut self) -> Result<EventArray, Failure>;
    fn balance(&mut self) -> Result<EventValue, Failure>;
    fn reject_reason(&mut self, data: &EventValue) -> Result<EventValue, Failure>;
    fn asset_price(&mut self, asset: &EventValue) -> Result<f64, Failure>;
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum OrderVenue {
    Futures,
    PM,
    Spot,
    Algo,
    Conditional,
}
fn id(data: &EventValue, venue: OrderVenue) -> Result<EventValue, Failure> {
    if venue == OrderVenue::Futures {
        data.get("i")?.optional_id("data.i")
    } else if venue == OrderVenue::Algo {
        data.get("aid")?.optional_id("data.aid")
    } else {
        data.get("c")
    }
}
fn order_type(data: &EventValue, venue: OrderVenue) -> Result<EventValue, Failure> {
    let (entries, key) = match venue {
        OrderVenue::Futures => (crate::binance_types::ORDER_TYPE, "ot"),
        OrderVenue::PM => (crate::binancepm_types::ORDER_TYPE, "o"),
        OrderVenue::Conditional => (crate::binancepm_types::ORDER_TYPE, "st"),
        OrderVenue::Algo => (crate::binance_types::ORDER_TYPE, "o"),
        OrderVenue::Spot => (crate::binancespot_types::ORDER_TYPE_SPOT, "o"),
    };
    let value = table(entries, data.get(key)?, PrivateEnum::Type)?;
    Ok(if venue == OrderVenue::Spot {
        fallback(value, EventValue::Enum(PrivateEnum::Type(OrderType::Limit)))
    } else {
        value
    })
}
fn side(data: &EventValue, venue: OrderVenue) -> Result<EventValue, Failure> {
    table(
        if matches!(venue, OrderVenue::PM | OrderVenue::Conditional) {
            crate::binancepm_types::ORDER_SIDE
        } else if venue == OrderVenue::Spot {
            crate::binancespot_types::ORDER_SIDE
        } else {
            crate::binance_types::ORDER_SIDE
        },
        data.get("S")?,
        PrivateEnum::Side,
    )
}
fn trigger(data: &EventValue, venue: OrderVenue) -> Result<EventValue, Failure> {
    table(
        if matches!(venue, OrderVenue::PM | OrderVenue::Conditional) {
            crate::binancepm_types::ORDER_TRIGGER_TYPE
        } else {
            crate::binance_types::ORDER_TRIGGER_TYPE
        },
        data.get("wt")?,
        PrivateEnum::Trigger,
    )
}
fn status_field(venue: OrderVenue) -> &'static str {
    if venue == OrderVenue::Conditional {
        "os"
    } else {
        "X"
    }
}
fn status(data: &EventValue, venue: OrderVenue, values: &[&str]) -> Result<bool, Failure> {
    let current = data.get(status_field(venue))?;
    Ok(values.iter().any(|v| current.same(&string(*v))))
}
fn order_status(
    b: &mut impl PrivateEventBindings,
    id: EventValue,
    status: OrderStatus,
) -> Result<(), Failure> {
    let call = b.capture(PrivateOperation::OrderStatus)?;
    call(vec![
        record([("id", id)]),
        EventValue::Enum(PrivateEnum::Status(status)),
    ])
}
fn fill(
    b: &mut impl PrivateEventBindings,
    data: &EventValue,
    venue: OrderVenue,
) -> Result<(), Failure> {
    let call = b.capture(PrivateOperation::Fill)?;
    let payload = record([
        ("id", id(data, venue)?),
        ("side", side(data, venue)?),
        ("symbol", data.get("s")?),
        (
            "price",
            number(
                data.get(if venue == OrderVenue::Spot { "p" } else { "ap" })?
                    .float()?,
            ),
        ),
        ("amount", number(data.get("l")?.float()?)),
    ]);
    call(vec![payload])
}
fn regular_update(
    b: &mut impl PrivateEventBindings,
    data: &EventValue,
    venue: OrderVenue,
) -> Result<(f64, f64, EventValue), Failure> {
    let early_id = if venue == OrderVenue::PM {
        None
    } else {
        Some(id(data, venue)?)
    };
    let filled = data.get("z")?.float()?;
    let amount = data.get("q")?.float()?;
    let call = b.capture(PrivateOperation::UpdateOrder)?;
    let order_id = if let Some(id) = early_id {
        id
    } else {
        id(data, venue)?
    };
    let selector = record([("id", order_id.clone())]);
    let patch = record([
        ("filled", number(filled)),
        ("remaining", number(amount - filled)),
        ("lastUpdate", number(b.now()?)),
    ]);
    call(vec![selector, patch])?;
    Ok((filled, amount, order_id))
}
fn add_order(
    b: &mut impl PrivateEventBindings,
    data: &EventValue,
    venue: OrderVenue,
    algo_id: Option<EventValue>,
    algo_type: Option<EventValue>,
) -> Result<(), Failure> {
    let call = b.capture(PrivateOperation::AddOrder)?;
    let mut fields = vec![
        (
            "id",
            if let Some(id) = algo_id {
                id
            } else {
                id(data, venue)?
            },
        ),
        (
            "status",
            EventValue::Enum(PrivateEnum::Status(OrderStatus::Open)),
        ),
        ("symbol", data.get("s")?),
        (
            "type",
            if let Some(t) = algo_type {
                t
            } else {
                order_type(data, venue)?
            },
        ),
        ("side", side(data, venue)?),
    ];
    let price = if venue == OrderVenue::Algo {
        fallback(number(data.get("tp")?.float()?), number(0.0))
    } else {
        let price = data.get("p")?.float()?;
        if price != 0.0 && !price.is_nan() {
            number(price)
        } else {
            number(data.get("sp")?.float()?)
        }
    };
    fields.push(("price", price));
    let amount = number(data.get("q")?.float()?);
    fields.push((
        "amount",
        if venue == OrderVenue::Algo {
            fallback(amount, number(0.0))
        } else {
            amount
        },
    ));
    let filled = if venue == OrderVenue::Algo {
        number(0.0)
    } else {
        number(data.get("z")?.float()?)
    };
    fields.push((
        "filled",
        if venue == OrderVenue::Conditional {
            fallback(filled, number(0.0))
        } else {
            filled
        },
    ));
    fields.push(("lastUpdate", number(b.now()?)));
    let remaining = if venue == OrderVenue::Algo {
        fallback(number(data.get("q")?.float()?), number(0.0))
    } else {
        let r = number(data.get("q")?.float()? - data.get("z")?.float()?);
        if venue == OrderVenue::Conditional {
            fallback(r, number(0.0))
        } else {
            r
        }
    };
    fields.push(("remaining", remaining));
    fields.push((
        "reduceOnly",
        if venue == OrderVenue::Spot {
            EventValue::Bool(false)
        } else {
            fallback(data.get("R")?, EventValue::Bool(false))
        },
    ));
    if venue == OrderVenue::Algo {
        fields.push(("close", fallback(data.get("cp")?, EventValue::Bool(false))))
    }
    let info = if venue == OrderVenue::Futures {
        let info = EventValue::object(data.spread());
        info.define("orderId", id(data, venue)?);
        info
    } else if venue == OrderVenue::Algo {
        let info = record([("algoId", data.get("aid")?)]);
        for (k, v) in data.spread() {
            info.define(&k, v)
        }
        info
    } else {
        data.clone()
    };
    fields.push(("info", info));
    if venue != OrderVenue::Spot {
        let current_type = if venue == OrderVenue::Algo {
            fields.iter().find(|(k, _)| *k == "type").unwrap().1.clone()
        } else {
            order_type(data, venue)?
        };
        if current_type.same(&EventValue::Enum(PrivateEnum::Type(OrderType::TakeProfit))) {
            fields.push(("tpTriggerType", trigger(data, venue)?))
        }
        let current_type = if venue == OrderVenue::Algo {
            current_type
        } else {
            order_type(data, venue)?
        };
        if current_type.same(&EventValue::Enum(PrivateEnum::Type(OrderType::StopLoss))) {
            fields.push(("slTriggerType", trigger(data, venue)?))
        }
    }
    call(vec![record(fields)])
}
fn regular_events(
    events: &EventArray,
    b: &mut impl PrivateEventBindings,
    venue: OrderVenue,
) -> Result<(), Failure> {
    for_each(events, |event| {
        let data = if venue == OrderVenue::Spot {
            event
        } else {
            let key = if venue == OrderVenue::Conditional {
                "so"
            } else {
                "o"
            };
            if event.nullish() {
                let kind = if matches!(event, EventValue::Null) {
                    "null"
                } else {
                    "undefined"
                };
                let description = if kind == "null" {
                    "object null"
                } else {
                    "undefined"
                };
                return Err(type_error(format!(
                    "Cannot destructure property '{key}' of '{description}' as it is {kind}."
                )));
            }
            event.get(key)?
        };
        let is_fill = if venue == OrderVenue::Spot {
            data.get("x")?.same(&string("TRADE"))
        } else {
            status(&data, venue, &["PARTIALLY_FILLED", "FILLED"])?
        };
        if is_fill {
            fill(b, &data, venue)?;
            if venue != OrderVenue::Conditional {
                let (filled, amount, order_id) = regular_update(b, &data, venue)?;
                if venue == OrderVenue::Spot
                    && (status(&data, venue, &["FILLED"])? || filled >= amount)
                {
                    order_status(b, order_id, OrderStatus::Closed)?;
                }
            }
        }
        if status(&data, venue, &["NEW"])? {
            add_order(b, &data, venue, None, None)?
        }
        let cancel = if venue == OrderVenue::Spot {
            &["CANCELED", "REJECTED", "EXPIRED"][..]
        } else {
            &["CANCELED", "EXPIRED", "CANCELLED"][..]
        };
        if status(&data, venue, cancel)? {
            let order_id = if venue == OrderVenue::Futures {
                let order_id = id(&data, venue)?;
                order_status(b, order_id.clone(), OrderStatus::Canceled)?;
                order_id
            } else {
                let call = b.capture(PrivateOperation::OrderStatus)?;
                let order_id = id(&data, venue)?;
                call(vec![
                    record([("id", order_id.clone())]),
                    EventValue::Enum(PrivateEnum::Status(OrderStatus::Canceled)),
                ])?;
                order_id
            };
            if venue == OrderVenue::Futures {
                let reject = b.reject_reason(&data)?;
                if reject.truthy() {
                    let existing = find(&b.orders()?, |o| Ok(o.get("id")?.same(&order_id)))?;
                    let call = b.capture(PrivateOperation::UpdateOrder)?;
                    let extra = if existing.nullish() {
                        EventValue::Undefined
                    } else {
                        existing.get("extra")?
                    };
                    let extra = EventValue::object(extra.spread());
                    extra.define("rejectReason", reject);
                    call(vec![record([("id", order_id)]), record([("extra", extra)])])?;
                }
            }
        }
        if venue != OrderVenue::Spot && status(&data, venue, &["FILLED"])? {
            let call = b.capture(PrivateOperation::OrderStatus)?;
            call(vec![
                record([("id", id(&data, venue)?)]),
                EventValue::Enum(PrivateEnum::Status(OrderStatus::Closed)),
            ])?
        }
        Ok(())
    })?;
    if venue != OrderVenue::Spot {
        b.capture(PrivateOperation::ThrottleOrders)?(vec![])?;
    }
    Ok(())
}
// ts: BinancePrivateWebsocket.handleOrderEvents (binance.ws-private.ts)
pub fn handle_futures_order_events(
    events: &EventArray,
    b: &mut impl PrivateEventBindings,
) -> Result<(), Failure> {
    regular_events(events, b, OrderVenue::Futures)
}
// ts: BinancePMPrivateWebsocket.handleOrderEvents
pub fn handle_pm_order_events(
    events: &EventArray,
    b: &mut impl PrivateEventBindings,
) -> Result<(), Failure> {
    regular_events(events, b, OrderVenue::PM)
}
// ts: BinancePrivateWebsocket.handleOrderEvents (binancespot.ws-private.ts)
pub fn handle_spot_order_events(
    events: &EventArray,
    b: &mut impl PrivateEventBindings,
) -> Result<(), Failure> {
    regular_events(events, b, OrderVenue::Spot)
}
// ts: BinancePMPrivateWebsocket.handleConditionalOrderEvents
pub fn handle_pm_conditional_order_events(
    events: &EventArray,
    b: &mut impl PrivateEventBindings,
) -> Result<(), Failure> {
    regular_events(events, b, OrderVenue::Conditional)
}
// ts: BinancePrivateWebsocket.handleAlgoOrderEvents
pub fn handle_futures_algo_order_events(
    events: &EventArray,
    b: &mut impl PrivateEventBindings,
) -> Result<(), Failure> {
    for_each(events, |event| {
        let data = event.get("o")?;
        if !data.truthy() {
            return Ok(());
        }
        let order_id = id(&data, OrderVenue::Algo)?;
        let algo_status = data.get("X")?;
        let order_type = order_type(&data, OrderVenue::Algo)?;
        let matches = |values: &[&str]| values.iter().any(|v| algo_status.same(&string(*v)));
        if matches(&["TRIGGERED", "TRIGGERING"]) {
            order_status(b, order_id.clone(), OrderStatus::Closed)?
        }
        if matches(&["NEW"]) {
            add_order(
                b,
                &data,
                OrderVenue::Algo,
                Some(order_id.clone()),
                Some(order_type),
            )?
        }
        if matches(&["CANCELED", "CANCELLED", "EXPIRED", "REJECTED"]) {
            order_status(b, order_id.clone(), OrderStatus::Canceled)?
        }
        if matches(&["FINISHED"]) {
            order_status(b, order_id, OrderStatus::Closed)?
        }
        Ok(())
    })?;
    b.capture(PrivateOperation::ThrottleOrders)?(vec![])
}
fn position_id(
    symbol: EventValue,
    side: &EventValue,
    mode: PositionMode,
) -> Result<EventValue, Failure> {
    if mode == PositionMode::OneWay || !side.truthy() {
        return Ok(symbol);
    }
    if let (EventValue::String(symbol), EventValue::Enum(PrivateEnum::PositionSide(side))) =
        (&symbol, side)
    {
        return Ok(string(safe_cex_types::model_utils::create_position_id(
            symbol,
            Some(*side),
            Some(mode),
        )));
    }
    Ok(string(format!("{}:{}", symbol.text()?, side.text()?)))
}
// ts: BinancePrivateWebsocket.handleAccountEvents
pub fn handle_futures_account_events(
    events: &EventArray,
    state: &Cell<bool>,
    b: &mut impl PrivateEventBindings,
) -> Result<(), Failure> {
    for_each(events, |event| {
        let a = event.get("a")?;
        let p = if a.nullish() {
            EventValue::Undefined
        } else {
            a.get("P")?
        };
        let length = if p.nullish() {
            EventValue::Undefined
        } else {
            p.get("length")?
        };
        if length.truthy() {
            b.capture(PrivateOperation::MarkPosition)?(vec![])?;
            state.set(true);
            b.capture(PrivateOperation::ThrottlePositions)?(vec![])?;
        }
        let p = event.get("a")?.get("P")?;
        array_for_each(p, "event.a.P?.forEach", |p| {
            let symbol = p.get("s")?;
            let signed = p.get("pa")?.float()?;
            let mode = if p.get("ps")?.same(&string("BOTH")) {
                PositionMode::OneWay
            } else {
                PositionMode::Hedged
            };
            let side = fallback(
                table(
                    crate::binance_types::POSITION_SIDE,
                    p.get("ps")?,
                    PrivateEnum::PositionSide,
                )?,
                EventValue::Enum(PrivateEnum::PositionSide(if signed >= 0.0 {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                })),
            );
            let id = position_id(symbol.clone(), &side, mode)?;
            let position = find(&b.positions()?, |c| Ok(c.get("id")?.same(&id)))?;
            let side = if signed == 0.0 && position.truthy() {
                position.get("side")?
            } else {
                side
            };
            if !position.truthy() && signed == 0.0 {
                return Ok(());
            }
            let entry = p.get("ep")?.float()?;
            let contracts = signed.abs();
            let upnl = p.get("up")?.float()?;
            let call = b.capture(PrivateOperation::AddPosition)?;
            let out = EventValue::object(position.spread());
            out.define("id", id);
            out.define("symbol", symbol);
            out.define("side", side);
            out.define("entryPrice", number(entry));
            out.define("contracts", number(contracts));
            out.define("notional", number((signed * entry + upnl).abs()));
            out.define("unrealizedPnl", number(upnl));
            for (key, default) in [
                ("leverage", number(0.0)),
                ("liquidationPrice", number(0.0)),
                (
                    "marginType",
                    EventValue::Enum(PrivateEnum::Margin(MarginType::Cross)),
                ),
            ] {
                out.define(
                    key,
                    nullish(
                        if position.nullish() {
                            EventValue::Undefined
                        } else {
                            position.get(key)?
                        },
                        default,
                    ),
                );
            }
            out.define(
                "positionMode",
                EventValue::Enum(PrivateEnum::PositionMode(mode)),
            );
            let old_info = if position.nullish() {
                EventValue::Undefined
            } else {
                position.get("info")?
            };
            let info = EventValue::object(old_info.spread());
            for (k, v) in p.spread() {
                info.define(&k, v)
            }
            out.define("info", info);
            call(vec![out])
        })
    })
}
// ts: BinancePMPrivateWebsocket.handleAccountEvents
pub fn handle_pm_account_events(
    events: &EventArray,
    state: &Cell<bool>,
    b: &mut impl PrivateEventBindings,
) -> Result<(), Failure> {
    for_each(events, |event| {
        let a = event.get("a")?;
        let p = if a.nullish() {
            EventValue::Undefined
        } else {
            a.get("P")?
        };
        if p.truthy() {
            state.set(true);
            b.capture(PrivateOperation::ThrottlePositions)?(vec![])?;
        }
        let a = event.get("a")?;
        let p = if a.nullish() {
            EventValue::Undefined
        } else {
            a.get("P")?
        };
        array_for_each(p, "event.a?.P?.forEach", |p| {
            let symbol = p.get("s")?;
            let side =
                EventValue::Enum(PrivateEnum::PositionSide(if p.get("pa")?.number()? > 0.0 {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                }));
            let one_way = p.get("ps")?.same(&string("BOTH"));
            let position = find(&b.positions()?, |c| {
                Ok(c.get("symbol")?.same(&symbol) && (one_way || c.get("side")?.same(&side)))
            })?;
            if position.truthy() {
                let entry = p.get("ep")?.float()?;
                let contracts = p.get("pa")?.float()?.abs();
                let upnl = p.get("up")?.float()?;
                let call = b.capture(PrivateOperation::UpdatePosition)?;
                let patch = record([
                    ("entryPrice", number(entry)),
                    ("contracts", number(contracts)),
                    ("notional", number(contracts * entry)),
                    ("unrealizedPnl", number(upnl)),
                    ("info", position.clone()),
                ]);
                if one_way {
                    patch.define("side", side)
                }
                call(vec![position, patch])?;
            }
            Ok(())
        })
    })
}
// ts: BinancePrivateWebsocket.handleAccountEvents (binancespot.ws-private.ts)
pub fn handle_spot_account_events(
    events: &EventArray,
    b: &mut impl PrivateEventBindings,
) -> Result<(), Failure> {
    for_each(events, |event| {
        let currencies =
            EventValue::object(fallback(b.balance()?.get("currencies")?, record([])).spread());
        let coins = EventValue::object(fallback(b.balance()?.get("coins")?, record([])).spread());
        let balances = fallback(
            event.get("B")?,
            EventValue::Array(Rc::new(std::cell::RefCell::new(vec![]))),
        );
        let balances = match balances {
            EventValue::Array(a) => a,
            EventValue::String(s) => Rc::new(std::cell::RefCell::new(
                s.chars().map(|c| string(c.to_string())).collect(),
            )),
            _ => {
                return Err(type_error(
                    "object is not iterable (cannot read property Symbol(Symbol.iterator))",
                ));
            }
        };
        let mut i = 0;
        while i < balances.borrow().len() {
            let asset_balance = balances.borrow()[i].clone();
            let asset = asset_balance.get("a")?;
            let free = fallback(asset_balance.get("f")?, string("0")).float()?;
            let used = fallback(asset_balance.get("l")?, string("0")).float()?;
            let total = free + used;
            let price = b.asset_price(&asset)?;
            let key = asset.text()?;
            currencies.assign(
                &key,
                record([
                    ("free", number(free)),
                    ("used", number(used)),
                    ("total", number(total)),
                    ("upnl", number(0.0)),
                    ("info", asset_balance),
                ]),
            );
            let key = asset.text()?;
            coins.assign(
                &key,
                record([
                    ("base", asset),
                    ("amount", number(total)),
                    ("notional", number(total * price)),
                ]),
            );
            i += 1;
        }
        let (mut free, mut used, mut total) = (0.0, 0.0, 0.0);
        let entries = currencies.spread();
        for (asset, balance) in entries {
            let price = b.asset_price(&string(asset))?;
            free += balance.get("free")?.number()? * price;
            used += balance.get("used")?.number()? * price;
            total += balance.get("total")?.number()? * price;
        }
        let call = b.capture(PrivateOperation::UpdateStore)?;
        let balance = record([
            ("free", number(free)),
            ("used", number(used)),
            ("total", number(total)),
            ("upnl", number(0.0)),
            ("currencies", currencies),
            ("coins", coins),
            ("info", event),
        ]);
        call(vec![record([("balance", balance)])])
    })
}
