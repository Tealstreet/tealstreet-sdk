//! Cancel/update/single-order bodies for Futures, PM and Spot.
//! Current bound client/Store/format/batch/notice methods are configured operations.
//! Dense native graphs retain f64, own undefined, categorical enums and Rc aliases.
//! Concrete class/client/Store/helper wiring and started-request drop/unwind policy
//! remain caller-owned. Executable properties/custom iterators, sparse arrays,
//! mutable/non-Error exceptions and nested Error response/message identity are unclaimed.
//! Batch helpers return dense arrays (or ASCII strings for Futures spread); arbitrary
//! malformed/non-iterable helper outputs are outside this configured domain.
use crate::{
    private_event_value::{
        EventOps, EventValue, PrivateEnum, array_values, fallback, record, string,
    },
    request_weight::{Failure, type_error},
};
use safe_cex_types::{OrderStatus, OrderType, object_map::JsObjectMap};
use std::{future::Future, pin::Pin, rc::Rc};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Venue {
    Futures,
    PM,
    Spot,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Cancel,
    Place,
    ConflictCancel,
    RegularBatch,
    AlgoBatch,
    Batch,
}
pub type OperationFuture = Pin<Box<dyn Future<Output = Result<EventValue, Failure>>>>;
pub type Operation = Rc<dyn Fn(EventValue) -> Result<OperationFuture, Failure>>;
pub type Delete = Rc<dyn Fn(&'static str, EventValue) -> Result<OperationFuture, Failure>>;
pub type Sync = Rc<dyn Fn(EventValue) -> Result<EventValue, Failure>>;
pub type Status = Rc<dyn Fn(EventValue, OrderStatus) -> Result<(), Failure>>;
pub trait OrderBindings {
    fn capture_delete(&mut self, venue: Venue) -> Result<Delete, Failure>;
    fn capture_status(&mut self, single: bool) -> Result<Status, Failure>;
    fn capture_emit(&mut self) -> Result<Sync, Failure>;
    fn capture_is_algo(&mut self) -> Result<Sync, Failure>;
    fn capture_resolve_id(&mut self) -> Result<Sync, Failure>;
    fn capture_format(&mut self) -> Result<Sync, Failure>;
    fn capture_method(&mut self, method: Method) -> Result<Operation, Failure>;
}
pub fn array(values: Vec<EventValue>) -> EventValue {
    EventValue::array(values)
}
fn snapshot(v: &EventValue, expression: &str, method: &str) -> Result<Vec<EventValue>, Failure> {
    if let EventValue::Array(a) = v {
        return Ok(array_values(&a)?);
    }
    if v.nullish() {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{method}')",
            v.text()?
        )));
    }
    Err(type_error(format!(
        "{expression}.{method} is not a function"
    )))
}
fn each_map(
    v: &EventValue,
    expression: &str,
    mut f: impl FnMut(EventValue) -> Result<EventValue, Failure>,
) -> Result<EventValue, Failure> {
    let len = snapshot(v, expression, "map")?.len();
    let mut out = vec![];
    for i in 0..len {
        let row = {
            if let EventValue::Array(a) = v {
                crate::private_event_value::array_element(&a, i)?
            } else {
                None
            }
        };
        if let Some(row) = row {
            out.push(f(row)?);
        }
    }
    Ok(array(out))
}
fn ids(v: &EventValue) -> Result<EventValue, Failure> {
    each_map(v, "orders", |o| Ok(record([("id", o.read("id")?)])))
}
fn groups(v: &EventValue) -> Result<Vec<(String, EventValue)>, Failure> {
    // The configured lodash groupBy domain is dense arrays and nullish collections.
    if v.nullish() {
        return Ok(vec![]);
    }
    let mut grouped = JsObjectMap::<Vec<EventValue>>::new();
    for order in snapshot(v, "orders", "map")? {
        let symbol = if order.nullish() {
            EventValue::Undefined
        } else {
            order.read("symbol")?
        }
        .text()?;
        if let Some(g) = grouped.get_mut(&symbol) {
            g.push(order)
        } else {
            grouped.insert(symbol, vec![order]);
        }
    }
    Ok(grouped.into_iter().map(|(s, v)| (s, array(v))).collect())
}
fn endpoint(venue: Venue, key: &str) -> &'static str {
    let all = match venue {
        Venue::Futures => crate::binance_types::ENDPOINTS,
        Venue::PM => crate::binancepm_types::ENDPOINTS,
        Venue::Spot => crate::binancespot_types::ENDPOINTS,
    };
    all.iter()
        .find(|(k, _)| *k == key)
        .expect("generated endpoint")
        .1
}
fn selected_message(e: &Failure) -> EventValue {
    let msg = e
        .retained_response_input()
        .as_ref()
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("msg"))
        .cloned()
        .map(safe_cex_exchange::js_runtime::value::from_retained_input)
        .unwrap_or(EventValue::Undefined);
    fallback(msg, string(&e.message))
}
fn unknown_order(e: &Failure) -> bool {
    e.code.as_ref().and_then(|v| v.as_f64()) == Some(-2011.0)
}
fn emit(b: &mut impl OrderBindings, e: &Failure) -> Result<(), Failure> {
    let f = b.capture_emit()?;
    f(selected_message(e))?;
    Ok(())
}
async fn delete(
    b: &mut impl OrderBindings,
    venue: Venue,
    key: &str,
    params: impl FnOnce() -> Result<EventValue, Failure>,
) -> Result<(), Failure> {
    let f = b.capture_delete(venue)?;
    f(endpoint(venue, key), params()?)?.await?;
    Ok(())
}
fn status(
    b: &mut impl OrderBindings,
    single: bool,
    values: impl FnOnce() -> Result<EventValue, Failure>,
) -> Result<(), Failure> {
    let f = b.capture_status(single)?;
    f(values()?, OrderStatus::Canceled)
}
fn conditional(v: &EventValue) -> bool {
    [
        OrderType::StopLoss,
        OrderType::TakeProfit,
        OrderType::TrailingStopLoss,
    ]
    .iter()
    .any(|t| v.same(&EventValue::enum_value(PrivateEnum::Type(*t))))
}
fn classify(
    b: &mut impl OrderBindings,
    orders: &EventValue,
    algo: bool,
) -> Result<EventValue, Failure> {
    let len = snapshot(orders, "orders", "filter")?.len();
    let mut out = vec![];
    for i in 0..len {
        let row = {
            if let EventValue::Array(a) = orders {
                crate::private_event_value::array_element(&a, i)?
            } else {
                None
            }
        };
        if let Some(row) = row {
            let f = b.capture_is_algo()?;
            if f(row.read("type")?)?.truthy() == algo {
                out.push(row)
            }
        }
    }
    Ok(array(out))
}
// ts: BinanceExchange.cancelOrders
pub async fn cancel_futures(
    orders: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    let result = async {
        let algo = classify(b, &orders, true)?;
        let regular = classify(b, &orders, false)?;
        let requests = groups(&regular)?
            .into_iter()
            .map(|(symbol, orders)| {
                let captured = each_map(&orders, "symbolOrders", |o| o.read("id"))?;
                Ok((symbol, orders, captured))
            })
            .collect::<Result<Vec<_>, Failure>>()?;
        for (symbol, rows, captured) in requests {
            let result = async {
                let values = snapshot(&captured, "request.orderIdList", "map")?;
                if values.len() == 1 {
                    delete(b, Venue::Futures, "ORDER", || {
                        Ok(record([
                            ("symbol", string(&symbol)),
                            ("orderId", values[0].clone()),
                        ]))
                    })
                    .await?;
                } else {
                    for lot in values.chunks(10) {
                        let list = lot
                            .iter()
                            .map(|v| {
                                if v.nullish() {
                                    Ok(String::new())
                                } else {
                                    v.text()
                                }
                            })
                            .collect::<Result<Vec<_>, Failure>>()?
                            .join(",");
                        let order_id_list = string(format!("[{list}]"));
                        delete(b, Venue::Futures, "BATCH_ORDERS", || {
                            Ok(record([
                                ("symbol", string(&symbol)),
                                ("orderIdList", order_id_list),
                            ]))
                        })
                        .await?;
                    }
                }
                status(b, false, || ids(&rows))?;
                Ok::<(), Failure>(())
            }
            .await;
            if let Err(e) = result {
                if unknown_order(&e) {
                    status(b, false, || ids(&rows))?;
                } else {
                    return Err(e);
                }
            }
        }
        // Array iterator reads current length and values on each iteration.
        let mut i = 0;
        while i < algo.read("length")?.number()? as usize {
            let order = algo.get(&i.to_string())?;
            i += 1;
            let result = async {
                let algo_id = order.read("id")?;
                delete(b, Venue::Futures, "ALGO_ORDER", || {
                    Ok(record([("algoId", algo_id)]))
                })
                .await?;
                status(b, true, || Ok(record([("id", order.read("id")?)])))?;
                Ok::<(), Failure>(())
            }
            .await;
            if let Err(e) = result {
                if unknown_order(&e) {
                    status(b, true, || Ok(record([("id", order.read("id")?)])))?;
                    continue;
                }
                emit(b, &e)?;
                return Err(e);
            }
        }
        Ok(EventValue::Undefined)
    }
    .await;
    if let Err(e) = &result {
        emit(b, e)?;
    }
    result
}
// ts: BinancePMExchange.cancelOrders
pub async fn cancel_pm(
    orders: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    let result = async {
        for (symbol, rows) in groups(&orders)? {
            for order in snapshot(&rows, "request.orders", "map")? {
                let result = async {
                    let is_conditional = conditional(&order.read("type")?);
                    delete(
                        b,
                        Venue::PM,
                        if is_conditional {
                            "CONDITIONAL_ORDER"
                        } else {
                            "ORDER"
                        },
                        || {
                            Ok(EventValue::record_object(
                                [
                                    ("symbol".into(), string(&symbol)),
                                    (
                                        if is_conditional {
                                            "newClientStrategyId"
                                        } else {
                                            "origClientOrderId"
                                        }
                                        .into(),
                                        order.read("id")?,
                                    ),
                                ]
                                .into_iter()
                                .collect(),
                            ))
                        },
                    )
                    .await
                }
                .await;
                if let Err(e) = result {
                    let f = b.capture_emit()?;
                    f(string(format!(
                        "Failed to cancel order {}: {}",
                        order.read("id")?.text()?,
                        selected_message(&e).text()?
                    )))?;
                    return Err(e);
                }
            }
        }
        status(b, false, || ids(&orders))?;
        Ok(EventValue::Undefined)
    }
    .await;
    if let Err(e) = &result {
        emit(b, e)?;
    }
    result
}
// ts: BinanceSpotExchange.cancelOrders
pub async fn cancel_spot(
    orders: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    let result = async {
        let requests = groups(&orders)?
            .into_iter()
            .map(|(s, o)| Ok((s, each_map(&o, "symbolOrders", |o| o.read("id"))?)))
            .collect::<Result<Vec<_>, Failure>>()?;
        for (symbol, captured) in requests {
            for id in snapshot(&captured, "request.origClientOrderIdList", "map")? {
                delete(b, Venue::Spot, "ORDER", || {
                    Ok(record([
                        ("symbol", string(&symbol)),
                        ("origClientOrderId", id),
                    ]))
                })
                .await?;
            }
            status(b, false, || {
                each_map(&captured, "request.origClientOrderIdList", |id| {
                    Ok(record([("id", id)]))
                })
            })?;
        }
        Ok(EventValue::Undefined)
    }
    .await;
    if let Err(e) = &result {
        emit(b, e)?;
    }
    result
}
fn has(v: &EventValue, key: &str) -> Result<bool, Failure> {
    if let EventValue::Builtin(reference) = v {
        return Ok(reference.own_fields().contains_key(key)
            || !matches!(reference.get(key), safe_cex_num::PrototypeValue::Undefined));
    }
    if let EventValue::Object(o) = v {
        if v.has_own(key) {
            return Ok(true);
        }
        if let Some(p) = &*o.proto.borrow() {
            if !p.nullish() {
                return has(p, key);
            }
        }
        return Ok(false);
    }
    if let EventValue::Array(_) = v {
        return Ok(v.has_property(&key.encode_utf16().collect::<Vec<_>>()));
    }
    Err(type_error(format!(
        "Cannot use 'in' operator to search for '{key}' in {}",
        v.text()?
    )))
}
async fn update(
    venue: Venue,
    args: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    if args.nullish() {
        return Err(type_error(format!(
            "Cannot destructure property 'order' of '{}' as it is {}.",
            if matches!(args, EventValue::Null) {
                "object null"
            } else {
                "undefined"
            },
            args.text()?
        )));
    }
    let order = args.read("order")?;
    let changes = args.read("update")?;
    let new = record([
        ("symbol", order.read("symbol")?),
        ("type", order.read("type")?),
        ("side", order.read("side")?),
        ("price", order.read("price")?),
        ("amount", order.read("amount")?),
        (
            "reduceOnly",
            fallback(order.read("reduceOnly")?, EventValue::Bool(false)),
        ),
    ]);
    if venue == Venue::Futures {
        new.write(
            "close",
            fallback(order.read("close")?, EventValue::Bool(false)),
        )
    }
    if venue != Venue::Spot {
        for key in ["tpTriggerType", "slTriggerType"] {
            if has(&order, key)? {
                new.write(key, order.get(key)?);
            }
        }
    }
    for key in ["price", "amount"] {
        if has(&changes, key)? {
            new.write(key, changes.get(key)?);
        }
    }
    let cancel = b.capture_method(Method::Cancel)?;
    cancel(array(vec![order]))?.await?;
    let place = b.capture_method(Method::Place)?;
    place(new)?.await
}
// ts: BinanceExchange.updateOrder
pub async fn update_futures(
    args: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    update(Venue::Futures, args, b).await
}
// ts: BinancePMExchange.updateOrder
pub async fn update_pm(
    args: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    update(Venue::PM, args, b).await
}
// ts: BinanceSpotExchange.updateOrder
pub async fn update_spot(
    args: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    update(Venue::Spot, args, b).await
}
fn spread_ids(out: &mut Vec<EventValue>, ids: EventValue) -> Result<(), Failure> {
    match ids {
        EventValue::Array(a) => {
            let mut i = 0;
            while i < a.len() {
                out.push(EventValue::Array(a.clone()).get(&i.to_string())?);
                i += 1
            }
            Ok(())
        }
        EventValue::String(s) => {
            out.extend(
                EventValue::String(s.clone())
                    .text()?
                    .chars()
                    .map(|c| string(c.to_string())),
            );
            Ok(())
        }
        _ => Err(type_error(
            "Spread syntax requires ...iterable[Symbol.iterator] to be a function",
        )),
    }
}
// ts: BinanceExchange.placeOrder
pub async fn place_futures(
    opts: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    let resolve = b.capture_resolve_id()?;
    resolve(opts.clone())?;
    let cancel = b.capture_method(Method::ConflictCancel)?;
    cancel(opts.clone())?.await?;
    let format = b.capture_format()?;
    let formatted = format(opts)?;
    if formatted.nullish() {
        return Err(type_error(format!(
            "Cannot destructure property 'regular' of 'this.formatCreateOrder(...)' as it is {}.",
            formatted.text()?
        )));
    }
    let regular = formatted.read("regular")?;
    let algo = formatted.read("algo")?;
    let mut out = vec![];
    if regular.read("length")?.number()? > 0.0 {
        let batch = b.capture_method(Method::RegularBatch)?;
        spread_ids(&mut out, batch(regular)?.await?)?;
    }
    if algo.read("length")?.number()? > 0.0 {
        let batch = b.capture_method(Method::AlgoBatch)?;
        spread_ids(&mut out, batch(algo)?.await?)?;
    }
    Ok(array(out))
}
async fn place(opts: EventValue, b: &mut impl OrderBindings) -> Result<EventValue, Failure> {
    let format = b.capture_format()?;
    let payloads = format(opts)?;
    let batch = b.capture_method(Method::Batch)?;
    batch(payloads)?.await
}
// ts: BinancePMExchange.placeOrder
pub async fn place_pm(opts: EventValue, b: &mut impl OrderBindings) -> Result<EventValue, Failure> {
    place(opts, b).await
}
// ts: BinanceSpotExchange.placeOrder
pub async fn place_spot(
    opts: EventValue,
    b: &mut impl OrderBindings,
) -> Result<EventValue, Failure> {
    place(opts, b).await
}
