//! Eight extracted private WebSocket event bodies. Calls remain at their source
//! sites, with bound current methods captured before argument/clock evaluation.
//! Concrete class/Store composition and executable property hooks remain pending.
use crate::{
    private_event_value::*,
    request_weight::{Failure, type_error},
};
use safe_cex_types::{MarginType, OrderStatus, OrderType, PositionMode, PositionSide};
use std::{cell::Cell, rc::Rc};
/// The production private socket rejection classifier over native values.
pub fn get_order_reject_reason(data: &EventValue) -> Result<EventValue, Failure> {
    let code = fallback(data.read("r")?, string("")).text()?.to_uppercase();
    Ok(
        if ["POST_ONLY", "GTX", "MAKER"]
            .iter()
            .any(|s| code.contains(s))
        {
            EventValue::enum_value(PrivateEnum::Reject(
                safe_cex_types::OrderRejectReason::PostOnly,
            ))
        } else {
            EventValue::Undefined
        },
    )
}
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
        data.read("i")?.optional_id("data.i")
    } else if venue == OrderVenue::Algo {
        data.read("aid")?.optional_id("data.aid")
    } else {
        data.read("c")
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
        fallback(
            value,
            EventValue::enum_value(PrivateEnum::Type(OrderType::Limit)),
        )
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
        data.read("S")?,
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
        data.read("wt")?,
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
        EventValue::enum_value(PrivateEnum::Status(status)),
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
        ("symbol", data.read("s")?),
        (
            "price",
            number(
                data.get(if venue == OrderVenue::Spot { "p" } else { "ap" })?
                    .float()?,
            ),
        ),
        ("amount", number(data.read("l")?.float()?)),
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
    let filled = data.read("z")?.float()?;
    let amount = data.read("q")?.float()?;
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
            EventValue::enum_value(PrivateEnum::Status(OrderStatus::Open)),
        ),
        ("symbol", data.read("s")?),
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
        fallback(number(data.read("tp")?.float()?), number(0.0))
    } else {
        let price = data.read("p")?.float()?;
        if price != 0.0 && !price.is_nan() {
            number(price)
        } else {
            number(data.read("sp")?.float()?)
        }
    };
    fields.push(("price", price));
    let amount = number(data.read("q")?.float()?);
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
        number(data.read("z")?.float()?)
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
        fallback(number(data.read("q")?.float()?), number(0.0))
    } else {
        let r = number(data.read("q")?.float()? - data.read("z")?.float()?);
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
            fallback(data.read("R")?, EventValue::Bool(false))
        },
    ));
    if venue == OrderVenue::Algo {
        fields.push(("close", fallback(data.read("cp")?, EventValue::Bool(false))))
    }
    let info = if venue == OrderVenue::Futures {
        let info = EventValue::record_object(data.spread());
        info.write("orderId", id(data, venue)?);
        info
    } else if venue == OrderVenue::Algo {
        let info = record([("algoId", data.read("aid")?)]);
        for (k, v) in data.spread() {
            info.write(&k, v)
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
        if current_type.same(&EventValue::enum_value(PrivateEnum::Type(
            OrderType::TakeProfit,
        ))) {
            fields.push(("tpTriggerType", trigger(data, venue)?))
        }
        let current_type = if venue == OrderVenue::Algo {
            current_type
        } else {
            order_type(data, venue)?
        };
        if current_type.same(&EventValue::enum_value(PrivateEnum::Type(
            OrderType::StopLoss,
        ))) {
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
            data.read("x")?.same(&string("TRADE"))
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
                    EventValue::enum_value(PrivateEnum::Status(OrderStatus::Canceled)),
                ])?;
                order_id
            };
            if venue == OrderVenue::Futures {
                let reject = b.reject_reason(&data)?;
                if reject.truthy() {
                    let existing = find(&b.orders()?, |o| Ok(o.read("id")?.same(&order_id)))?;
                    let call = b.capture(PrivateOperation::UpdateOrder)?;
                    let extra = if existing.nullish() {
                        EventValue::Undefined
                    } else {
                        existing.read("extra")?
                    };
                    let extra = EventValue::record_object(extra.spread());
                    extra.write("rejectReason", reject);
                    call(vec![record([("id", order_id)]), record([("extra", extra)])])?;
                }
            }
        }
        if venue != OrderVenue::Spot && status(&data, venue, &["FILLED"])? {
            let call = b.capture(PrivateOperation::OrderStatus)?;
            call(vec![
                record([("id", id(&data, venue)?)]),
                EventValue::enum_value(PrivateEnum::Status(OrderStatus::Closed)),
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
        let data = event.read("o")?;
        if !data.truthy() {
            return Ok(());
        }
        let order_id = id(&data, OrderVenue::Algo)?;
        let algo_status = data.read("X")?;
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
    if matches!(symbol, EventValue::String(_))
        && let Ok(side) = crate::class_decode::decode::<PositionSide>(side.clone())
    {
        return Ok(string(safe_cex_types::model_utils::create_position_id(
            &symbol.text()?,
            Some(side),
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
        let a = event.read("a")?;
        let p = if a.nullish() {
            EventValue::Undefined
        } else {
            a.read("P")?
        };
        let length = if p.nullish() {
            EventValue::Undefined
        } else {
            p.read("length")?
        };
        if length.truthy() {
            b.capture(PrivateOperation::MarkPosition)?(vec![])?;
            state.set(true);
            b.capture(PrivateOperation::ThrottlePositions)?(vec![])?;
        }
        let p = event.read("a")?.read("P")?;
        array_for_each(p, "event.a.P?.forEach", |p| {
            let symbol = p.read("s")?;
            let signed = p.read("pa")?.float()?;
            let mode = if p.read("ps")?.same(&string("BOTH")) {
                PositionMode::OneWay
            } else {
                PositionMode::Hedged
            };
            let side = fallback(
                table(
                    crate::binance_types::POSITION_SIDE,
                    p.read("ps")?,
                    PrivateEnum::PositionSide,
                )?,
                EventValue::enum_value(PrivateEnum::PositionSide(if signed >= 0.0 {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                })),
            );
            let id = position_id(symbol.clone(), &side, mode)?;
            let position = find(&b.positions()?, |c| Ok(c.read("id")?.same(&id)))?;
            let side = if signed == 0.0 && position.truthy() {
                position.read("side")?
            } else {
                side
            };
            if !position.truthy() && signed == 0.0 {
                return Ok(());
            }
            let entry = p.read("ep")?.float()?;
            let contracts = signed.abs();
            let upnl = p.read("up")?.float()?;
            let call = b.capture(PrivateOperation::AddPosition)?;
            let out = EventValue::record_object(position.spread());
            out.write("id", id);
            out.write("symbol", symbol);
            out.write("side", side);
            out.write("entryPrice", number(entry));
            out.write("contracts", number(contracts));
            out.write("notional", number((signed * entry + upnl).abs()));
            out.write("unrealizedPnl", number(upnl));
            for (key, default) in [
                ("leverage", number(0.0)),
                ("liquidationPrice", number(0.0)),
                (
                    "marginType",
                    EventValue::enum_value(PrivateEnum::Margin(MarginType::Cross)),
                ),
            ] {
                out.write(
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
            out.write(
                "positionMode",
                EventValue::enum_value(PrivateEnum::PositionMode(mode)),
            );
            let old_info = if position.nullish() {
                EventValue::Undefined
            } else {
                position.read("info")?
            };
            let info = EventValue::record_object(old_info.spread());
            for (k, v) in p.spread() {
                info.write(&k, v)
            }
            out.write("info", info);
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
        let a = event.read("a")?;
        let p = if a.nullish() {
            EventValue::Undefined
        } else {
            a.read("P")?
        };
        if p.truthy() {
            state.set(true);
            b.capture(PrivateOperation::ThrottlePositions)?(vec![])?;
        }
        let a = event.read("a")?;
        let p = if a.nullish() {
            EventValue::Undefined
        } else {
            a.read("P")?
        };
        array_for_each(p, "event.a?.P?.forEach", |p| {
            let symbol = p.read("s")?;
            let side = EventValue::enum_value(PrivateEnum::PositionSide(
                if p.read("pa")?.number()? > 0.0 {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                },
            ));
            let one_way = p.read("ps")?.same(&string("BOTH"));
            let position = find(&b.positions()?, |c| {
                Ok(c.read("symbol")?.same(&symbol) && (one_way || c.read("side")?.same(&side)))
            })?;
            if position.truthy() {
                let entry = p.read("ep")?.float()?;
                let contracts = p.read("pa")?.float()?.abs();
                let upnl = p.read("up")?.float()?;
                let call = b.capture(PrivateOperation::UpdatePosition)?;
                let patch = record([
                    ("entryPrice", number(entry)),
                    ("contracts", number(contracts)),
                    ("notional", number(contracts * entry)),
                    ("unrealizedPnl", number(upnl)),
                    ("info", position.clone()),
                ]);
                if one_way {
                    patch.write("side", side)
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
        let currencies = EventValue::record_object(
            fallback(b.balance()?.read("currencies")?, record([])).spread(),
        );
        let coins =
            EventValue::record_object(fallback(b.balance()?.read("coins")?, record([])).spread());
        let balances = fallback(
            event.read("B")?,
            EventValue::Array(Rc::new(std::cell::RefCell::new(vec![]))),
        );
        let balances = match balances {
            EventValue::Array(a) => a,
            EventValue::String(s) => Rc::new(std::cell::RefCell::new(
                EventValue::String(s.clone())
                    .text()?
                    .chars()
                    .map(|c| Some(string(c.to_string())))
                    .collect(),
            )),
            _ => {
                return Err(type_error(
                    "object is not iterable (cannot read property Symbol(Symbol.iterator))",
                ));
            }
        };
        let mut i = 0;
        while i < balances.borrow().len() {
            let asset_balance = balances.borrow()[i]
                .clone()
                .unwrap_or(EventValue::Undefined);
            let asset = asset_balance.read("a")?;
            let free = fallback(asset_balance.read("f")?, string("0")).float()?;
            let used = fallback(asset_balance.read("l")?, string("0")).float()?;
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
            free += balance.read("free")?.number()? * price;
            used += balance.read("used")?.number()? * price;
            total += balance.read("total")?.number()? * price;
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
