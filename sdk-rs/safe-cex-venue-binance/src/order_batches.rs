//! Multi-order/HTTP batch bodies and Futures close-position conflict cancellation.
//! Current bound client/Store/helper/notice operations are explicit boundaries.
//! Dense native graphs retain f64, undefined, enums and shallow/reference identity.
//! POST yields decoded response.data; response/data getters, concrete class/client/
//! Store/helper composition, event-loop scheduling and started-request drop/unwind
//! ownership remain caller-owned. Executable properties/iterators, sparse arrays,
//! mutable or non-Error exceptions, nested Error response/message identity and
//! cyclic/toJSON graphs and non-BMP/lone UTF16 coercion are unclaimed.
use crate::{
    order_operations::{Method, Operation, OperationFuture, Sync, Venue, array},
    private_event_value::{
        EventOps, EventValue, PrivateEnum, array_values, fallback, number, record, string,
    },
    request_weight::{Failure, type_error},
};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_types::{OrderType, PositionSide};
use std::rc::Rc;
#[derive(Clone, Debug)]
pub enum Rejection {
    Error(Failure),
    Row(EventValue),
}
pub type Post = Rc<dyn Fn(&'static str, EventValue) -> Result<OperationFuture, Failure>>;
pub type Classifier = Rc<dyn Fn(Rejection) -> Result<bool, Failure>>;
pub type Log = Rc<dyn Fn(String, LogSeverity) -> Result<(), Failure>>;
pub trait BatchBindings {
    fn capture_assert_ids(&mut self) -> Result<Sync, Failure>;
    fn capture_format(&mut self) -> Result<Sync, Failure>;
    fn capture_method(&mut self, method: Method) -> Result<Operation, Failure>;
    fn capture_post(&mut self, venue: Venue) -> Result<Post, Failure>;
    fn capture_emit(&mut self) -> Result<Sync, Failure>;
    fn capture_post_only(&mut self) -> Result<Classifier, Failure>;
    fn stringify(&mut self, values: EventValue) -> Result<EventValue, Failure>;
    fn positions(&mut self) -> Result<EventValue, Failure>;
    fn market(&mut self, opts: &EventValue) -> Result<EventValue, Failure>;
    fn adjust(&mut self, value: EventValue, precision: EventValue) -> Result<EventValue, Failure>;
    fn orders(&mut self) -> Result<EventValue, Failure>;
    fn capture_log(&mut self) -> Result<Log, Failure>;
}
fn dense(v: &EventValue, expr: &str, method: &str) -> Result<Vec<EventValue>, Failure> {
    if let EventValue::Array(a) = v {
        return Ok(array_values(&a)?);
    }
    if v.nullish() {
        return Err(type_error(format!(
            "Cannot read properties of {} (reading '{method}')",
            v.text()?
        )));
    }
    Err(type_error(format!("{expr}.{method} is not a function")))
}
fn iterable(v: &EventValue, expression: &str) -> Result<(), Failure> {
    if matches!(v, EventValue::Array(_)) {
        Ok(())
    } else {
        Err(type_error(format!("{expression} is not iterable")))
    }
}
fn chunks(v: &EventValue) -> Result<Vec<EventValue>, Failure> {
    if v.nullish() {
        Ok(vec![])
    } else {
        dense(v, "payloads", "map")
    }
}
fn spread(out: &mut Vec<EventValue>, v: EventValue) -> Result<(), Failure> {
    match v {
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
fn message(e: &Failure) -> EventValue {
    let msg = e
        .response
        .as_ref()
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("msg"))
        .cloned()
        .map(EventValue::from_json)
        .unwrap_or(EventValue::Undefined);
    fallback(msg, string(&e.message))
}
fn emit(b: &mut impl BatchBindings, e: &Failure) -> Result<(), Failure> {
    let f = b.capture_emit()?;
    f(message(e))?;
    Ok(())
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
// ts: BinanceExchange.placeOrders
pub async fn many_futures(
    orders: EventValue,
    b: &mut impl BatchBindings,
) -> Result<EventValue, Failure> {
    let assert = b.capture_assert_ids()?;
    assert(orders.clone())?;
    iterable(&orders, "orders")?;
    let (mut regular, mut algo) = (vec![], vec![]);
    let mut i = 0;
    while i < orders.read("length")?.number()? as usize {
        let o = orders.get(&i.to_string())?;
        i += 1;
        let f = b.capture_format()?;
        let formatted = f(o)?;
        if formatted.nullish() {
            return Err(type_error(format!(
                "Cannot destructure property 'regular' of 'this.formatCreateOrder(...)' as it is {}.",
                formatted.text()?
            )));
        }
        let r = formatted.read("regular")?;
        let a = formatted.read("algo")?;
        spread(&mut regular, r)?;
        spread(&mut algo, a)?;
    }
    let regular = array(regular);
    let algo = array(algo);
    let mut ids = vec![];
    if regular.read("length")?.number()? > 0.0 {
        let f = b.capture_method(Method::RegularBatch)?;
        spread(&mut ids, f(regular)?.await?)?;
    }
    if algo.read("length")?.number()? > 0.0 {
        let f = b.capture_method(Method::AlgoBatch)?;
        spread(&mut ids, f(algo)?.await?)?;
    }
    Ok(array(ids))
}
async fn many(orders: EventValue, b: &mut impl BatchBindings) -> Result<EventValue, Failure> {
    let assert = b.capture_assert_ids()?;
    assert(orders.clone())?;
    let len = dense(&orders, "orders", "flatMap")?.len();
    let mut payloads = vec![];
    for i in 0..len {
        let o = {
            if let EventValue::Array(a) = &orders {
                crate::private_event_value::array_element(&a,i)?
            } else {
                None
            }
        };
        if let Some(o) = o {
            let f = b.capture_format()?;
            let formatted = f(o)?;
            if let EventValue::Array(a) = formatted {
                payloads.extend(array_values(&a)?)
            } else {
                payloads.push(formatted)
            }
        }
    }
    let f = b.capture_method(Method::Batch)?;
    f(array(payloads))?.await
}
// ts: BinancePMExchange.placeOrders
pub async fn many_pm(
    orders: EventValue,
    b: &mut impl BatchBindings,
) -> Result<EventValue, Failure> {
    many(orders, b).await
}
// ts: BinanceSpotExchange.placeOrders
pub async fn many_spot(
    orders: EventValue,
    b: &mut impl BatchBindings,
) -> Result<EventValue, Failure> {
    many(orders, b).await
}
async fn post(
    b: &mut impl BatchBindings,
    v: Venue,
    key: &str,
    payload: EventValue,
) -> Result<EventValue, Failure> {
    let f = b.capture_post(v)?;
    f(endpoint(v, key), payload)?.await
}
fn id(v: &EventValue, key: &str, expression: &str) -> Result<EventValue, Failure> {
    v.get(key)?.optional_id(expression)
}
fn post_only(b: &mut impl BatchBindings, value: Rejection) -> Result<bool, Failure> {
    let f = b.capture_post_only()?;
    f(value)
}
// ts: BinanceExchange#placeRegularOrderBatch
pub async fn regular_futures(
    payloads: EventValue,
    b: &mut impl BatchBindings,
) -> Result<EventValue, Failure> {
    let lots = chunks(&payloads)?;
    let mut ids = vec![];
    for lot in lots.chunks(5) {
        let result = async {
            if lot.len() == 1 {
                let data = post(b, Venue::Futures, "ORDER", lot[0].clone()).await?;
                ids.push(id(&data, "orderId", "data.orderId")?);
            }
            if lot.len() > 1 {
                let f = b.capture_post(Venue::Futures)?;
                let encoded = b.stringify(array(lot.to_vec()))?;
                let data = f(
                    endpoint(Venue::Futures, "BATCH_ORDERS"),
                    record([("batchOrders", encoded)]),
                )?
                .await?;
                if let EventValue::Array(rows) = data {
                    let len = rows.len();
                    for i in 0..len {
                        let o = { crate::private_event_value::array_element(&rows,i)? };
                        if let Some(o) = o {
                            if o.read("code")?.truthy() {
                                if !post_only(b, Rejection::Row(o.clone()))? {
                                    let emit = b.capture_emit()?;
                                    emit(o.read("msg")?)?;
                                }
                            } else {
                                ids.push(id(&o, "orderId", "o.orderId")?)
                            }
                        }
                    }
                } else if !data.nullish() {
                    let each = data.read("forEach")?;
                    if !each.nullish() {
                        return Err(type_error("data?.forEach is not a function"));
                    }
                }
            }
            Ok::<(), Failure>(())
        }
        .await;
        if let Err(e) = result {
            if !post_only(b, Rejection::Error(e.clone()))? {
                emit(b, &e)?;
            }
            return Err(e);
        }
    }
    Ok(array(ids))
}
// ts: BinanceExchange#placeAlgoOrders
pub async fn algo_futures(
    payloads: EventValue,
    b: &mut impl BatchBindings,
) -> Result<EventValue, Failure> {
    iterable(&payloads, "payloads")?;
    let mut ids = vec![];
    let mut i = 0;
    while i < payloads.read("length")?.number()? as usize {
        let payload = payloads.get(&i.to_string())?;
        i += 1;
        let result = async {
            let data = post(b, Venue::Futures, "ALGO_ORDER", payload).await?;
            ids.push(id(&data, "algoId", "data.algoId")?);
            Ok::<(), Failure>(())
        }
        .await;
        if let Err(e) = result {
            emit(b, &e)?;
            return Err(e);
        }
    }
    Ok(array(ids))
}
// ts: BinancePMExchange#placeOrderBatch
pub async fn batch_pm(
    payloads: EventValue,
    b: &mut impl BatchBindings,
) -> Result<EventValue, Failure> {
    iterable(&payloads, "payloads")?;
    let (mut regular, mut conditional) = (vec![], vec![]);
    let mut i = 0;
    while i < payloads.read("length")?.number()? as usize {
        let payload = payloads.get(&i.to_string())?;
        i += 1;
        let payload_type = payload.read("type")?;
        if ["STOP_MARKET", "TAKE_PROFIT_MARKET", "TRAILING_STOP_MARKET"]
            .iter()
            .any(|v| payload_type.same(&string(*v)))
        {
            let ty = payload.read("type")?;
            let quantity = payload.read("quantity")?;
            let mut rest = payload.spread();
            rest.remove("type");
            rest.remove("quantity");
            let numeric_quantity = quantity.number()?;
            let q = if numeric_quantity != 0.0 && !numeric_quantity.is_nan() {
                quantity
            } else {
                let positions = b.positions()?;
                let positions = dense(&positions, "this.store.positions", "find")?;
                let mut found = EventValue::Undefined;
                for p in positions {
                    if p.read("symbol")?.same(&payload.read("symbol")?)
                        && ((payload.read("side")?.same(&string("SELL"))
                            && p.read("side")?.same(&EventValue::enum_value(
                                PrivateEnum::PositionSide(PositionSide::Long),
                            )))
                            || (payload.read("side")?.same(&string("BUY"))
                                && p.read("side")?.same(&EventValue::enum_value(
                                    PrivateEnum::PositionSide(PositionSide::Short),
                                ))))
                    {
                        found = p.read("contracts")?;
                        break;
                    }
                }
                fallback(found, number(0.0))
            };
            let result = record([("strategyType", ty), ("quantity", q)]);
            for (k, v) in rest {
                result.write(&k, v)
            }
            conditional.push(result);
        } else {
            regular.push(payload)
        }
    }
    let mut ids = vec![];
    for (key, rows) in [("ORDER", regular), ("CONDITIONAL_ORDER", conditional)] {
        for payload in rows {
            let result = async {
                post(b, Venue::PM, key, payload.clone()).await?;
                ids.push(payload.read("newClientOrderId")?);
                Ok::<(), Failure>(())
            }
            .await;
            if let Err(e) = result {
                emit(b, &e)?;
                return Err(e);
            }
        }
    }
    Ok(array(ids))
}
// ts: BinanceSpotExchange#placeOrderBatch
pub async fn batch_spot(
    payloads: EventValue,
    b: &mut impl BatchBindings,
) -> Result<EventValue, Failure> {
    let rows = chunks(&payloads)?;
    let mut ids = vec![];
    for lot in rows.chunks(5) {
        for payload in lot {
            let result = async {
                post(b, Venue::Spot, "ORDER", payload.clone()).await?;
                ids.push(payload.read("newClientOrderId")?);
                Ok::<(), Failure>(())
            }
            .await;
            if let Err(e) = result {
                emit(b, &e)?;
                return Err(e);
            }
        }
    }
    Ok(array(ids))
}
// ts: BinanceExchange#cancelExistingClosePositionOrders
pub async fn cancel_close(
    opts: EventValue,
    b: &mut impl BatchBindings,
) -> Result<EventValue, Failure> {
    let ty = opts.read("type")?;
    if !ty.same(&EventValue::enum_value(PrivateEnum::Type(
        OrderType::StopLoss,
    ))) && !opts
        .read("type")?
        .same(&EventValue::enum_value(PrivateEnum::Type(
            OrderType::TakeProfit,
        )))
    {
        return Ok(EventValue::Undefined);
    }
    let market = b.market(&opts)?;
    if !market.truthy() {
        return Ok(EventValue::Undefined);
    }
    let precision = market.read("precision")?.read("amount")?;
    let amount = b.adjust(opts.read("amount")?, precision)?;
    if !opts.read("close")?.truthy() && amount.truthy() {
        return Ok(EventValue::Undefined);
    }
    let orders = b.orders()?;
    let len = dense(&orders, "this.store.orders", "filter")?.len();
    let mut existing = vec![];
    for i in 0..len {
        let o = orders.get(&i.to_string())?;
        if o.read("symbol")?.same(&opts.read("symbol")?)
            && o.read("close")?.same(&EventValue::Bool(true))
            && !o.read("amount")?.truthy()
            && o.read("type")?.same(&opts.read("type")?)
            && o.read("side")?.same(&opts.read("side")?)
        {
            existing.push(o)
        }
    }
    if !existing.is_empty() {
        let f = b.capture_log()?;
        f(
            format!(
                "Canceling {} existing closePosition order(s) before placing new one",
                existing.len()
            ),
            LogSeverity::Info,
        )?;
        let cancel = b.capture_method(Method::Cancel)?;
        cancel(array(existing))?.await?;
    }
    Ok(EventValue::Undefined)
}
