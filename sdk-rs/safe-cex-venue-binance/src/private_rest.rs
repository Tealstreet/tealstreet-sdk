//! Private REST snapshot and lookup bodies. Bindings supply current HTTP, Store,
//! logging, clock, price and imported historical/client-ID helper operations.
//! Detached HTTP futures are handles to already-launched requests; their owner
//! retains background completion if the source Promise.all abandons its await.
//! HTTP response/data getter timing, concrete client/runtime/class/Store/helper
//! composition, non-Error rejections, mutable exception metadata and nested error-response/message
//! referent identity are outside this decoded immutable-Error binding boundary.
//! Source-array property replacement/appends are supported; deletion/truncation
//! creating map holes, custom callables/getters/iterators/prototypes and symbol
//! keys are not. Imported history/client-ID helpers are caller bindings; existing
//! typed history helper's inherited-output refusal is not changed or claimed here.
use crate::{
    private_event_value::{
        EventOps, EventValue, PrivateEnum, array_values, fallback, find, nullish, number, record,
        string, table,
    },
    request_weight::{Failure, type_error},
};
use safe_cex_types::{OrderStatus, OrderType};
use serde_json::Value;
use std::{
    cell::Cell,
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::Poll,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotClient {
    Private,
    Papi,
    Primary,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotMethod {
    ConsoleError,
    Log,
    Emit,
}
#[derive(Clone, Debug)]
pub enum SnapshotArgument {
    Value(EventValue),
    Error(Failure),
}
pub type SnapshotCall = Rc<dyn Fn(Vec<SnapshotArgument>) -> Result<(), Failure>>;
pub type SnapshotFuture = Pin<Box<dyn Future<Output = Result<EventValue, Failure>>>>;
/// Order of the owning runtime's Promise.all reactions, including already-settled
/// promises (their newly attached reactions run in input order). This is not the
/// wall-clock settlement time. Client/runtime composition supplies this order.
pub struct SnapshotCompletion {
    pub reaction_order: u64,
    pub result: Result<EventValue, Failure>,
}
/// All already-settled aggregate reactions must poll Ready; reaction orders are
/// unique/comparable for this aggregate attachment, not historical settlement.
/// Response.data is already decoded; response-object getter timing is caller-owned.
pub type SnapshotRequestFuture = Pin<Box<dyn Future<Output = SnapshotCompletion>>>;
pub trait SnapshotBindings {
    fn get(
        &mut self,
        client: SnapshotClient,
        endpoint: &'static str,
        params: Option<EventValue>,
    ) -> Result<SnapshotRequestFuture, Failure>;
    fn capture(&mut self, method: SnapshotMethod) -> Result<SnapshotCall, Failure>;
    fn now(&mut self) -> Result<f64, Failure>;
    fn orders(&mut self) -> Result<EventValue, Failure>;
    fn balance(&mut self) -> Result<EventValue, Failure>;
    fn positions(&mut self) -> Result<EventValue, Failure>;
    fn asset_price(&mut self, asset: EventValue) -> Result<f64, Failure>;
    fn market(&mut self, symbol: &str) -> Result<EventValue, Failure>;
    fn has_primary(&mut self) -> Result<bool, Failure>;
    fn carries_client_id(&mut self, order: &EventValue, id: &str) -> Result<bool, Failure>;
    fn map_history(&mut self, data: EventValue) -> Result<EventValue, Failure>;
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Venue {
    Futures,
    PM,
    Spot,
}
fn endpoint(v: Venue, key: &str) -> &'static str {
    let entries = match v {
        Venue::Futures => crate::binance_types::ENDPOINTS,
        Venue::PM => crate::binancepm_types::ENDPOINTS,
        Venue::Spot => crate::binancespot_types::ENDPOINTS,
    };
    entries
        .iter()
        .find(|(k, _)| *k == key)
        .expect("generated endpoint")
        .1
}
fn empty() -> EventValue {
    EventValue::array(vec![])
}
pub(crate) fn keyed(o: &EventValue, key: &str) -> Result<EventValue, Failure> {
    if o.nullish() {
        return Ok(EventValue::Undefined);
    }
    for name in [
        key.to_owned(),
        safe_cex_num::lodash_snake_case(key),
        safe_cex_num::lodash_camel_case(key),
    ] {
        if safe_cex_exchange::js_runtime::library::in_operator(o, &name).map_err(Rc::from)? {
            return o.read(&name);
        }
    }
    Ok(EventValue::Undefined)
}
pub(crate) fn pm_rows(v: EventValue, ep: &str) -> Result<EventValue, Failure> {
    crate::response_shape::native_array_guard("Binance PM", ep, &v).map_err(Rc::from)?;
    Ok(v)
}
pub(crate) fn map(
    v: EventValue,
    expression: &str,
    mut visit: impl FnMut(EventValue) -> Result<EventValue, Failure>,
) -> Result<EventValue, Failure> {
    let EventValue::Array(a) = v else {
        if v.nullish() {
            return Err(type_error(format!(
                "Cannot read properties of {} (reading 'map')",
                if matches!(v, EventValue::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )));
        }
        return Err(type_error(format!("{expression}.map is not a function")));
    };
    let n = a.len();
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let v = crate::private_event_value::array_element(&a, i)?;
        out.push(match v {
            Some(v) => visit(v)?,
            None => EventValue::Undefined,
        })
    }
    Ok(EventValue::array(out))
}
fn subtract(a: EventValue, b: EventValue) -> Result<f64, Failure> {
    fn digits(v: &EventValue) -> Result<f64, Failure> {
        if matches!(v, EventValue::Object(_)) && v.has_own("toString") {
            let method = v.read("toString")?;
            {
                if method.nullish() {
                    return Ok(2.0);
                }
                return Err(type_error("num?.toString is not a function"));
            }
        }
        Ok(match v {
            EventValue::Number(n) => safe_cex_num::after_decimal(*n),
            EventValue::Undefined | EventValue::Null => 2.0,
            _ => safe_cex_num::after_decimal(v.text()?),
        })
    }
    let p = digits(&a)?.max(digits(&b)?);
    let factor = 10_f64.powf(p);
    Ok(safe_cex_num::math_round((a.number()? - b.number()?) * factor) / factor)
}
fn order_type(o: &EventValue, v: Venue, key: &str) -> Result<EventValue, Failure> {
    let entries = match v {
        Venue::Futures => crate::binance_types::ORDER_TYPE,
        Venue::PM => crate::binancepm_types::ORDER_TYPE,
        Venue::Spot => crate::binancespot_types::ORDER_TYPE_SPOT,
    };
    table(entries, o.read(key)?, PrivateEnum::Type)
}
fn trigger(
    out: &EventValue,
    o: &EventValue,
    v: Venue,
    cached: Option<EventValue>,
) -> Result<(), Failure> {
    if v == Venue::Spot {
        return Ok(());
    }
    let entries = if v == Venue::PM {
        crate::binancepm_types::ORDER_TRIGGER_TYPE
    } else {
        crate::binance_types::ORDER_TRIGGER_TYPE
    };
    for (typ, key) in [
        (OrderType::TakeProfit, "tpTriggerType"),
        (OrderType::StopLoss, "slTriggerType"),
    ] {
        let value = match &cached {
            Some(v) => v.clone(),
            None => order_type(o, v, "type")?,
        };
        if value.same(&EventValue::enum_value(PrivateEnum::Type(typ))) {
            out.write(
                key,
                table(entries, keyed(o, "workingType")?, PrivateEnum::Trigger)?,
            )
        }
    }
    Ok(())
}
fn regular(
    o: EventValue,
    v: Venue,
    conditional: bool,
    b: &mut impl SnapshotBindings,
) -> Result<EventValue, Failure> {
    let id = keyed(
        &o,
        if v == Venue::Futures {
            "orderId"
        } else if conditional {
            "newClientStrategyId"
        } else {
            "clientOrderId"
        },
    )?;
    let id = if v == Venue::Futures {
        id.optional_id("v(...)")?
    } else {
        id
    };
    let status = EventValue::enum_value(PrivateEnum::Status(OrderStatus::Open));
    let symbol = o.read("symbol")?;
    let typ = order_type(&o, v, if conditional { "strategyType" } else { "type" })?;
    let sides = match v {
        Venue::Futures => crate::binance_types::ORDER_SIDE,
        Venue::PM => crate::binancepm_types::ORDER_SIDE,
        Venue::Spot => crate::binancespot_types::ORDER_SIDE,
    };
    let side = table(sides, o.read("side")?, PrivateEnum::Side)?;
    let first = o.read("price")?.float()?;
    let price = if first != 0.0 && !first.is_nan() {
        first
    } else {
        keyed(&o, "stopPrice")?.float()?
    };
    let amount = keyed(&o, "origQty")?.float()?;
    let reduce = fallback(keyed(&o, "reduceOnly")?, EventValue::Bool(false));
    let close = if v == Venue::Futures {
        Some(fallback(
            keyed(&o, "closePosition")?,
            EventValue::Bool(false),
        ))
    } else {
        None
    };
    let filled = keyed(&o, "executedQty")?;
    let filled = if conditional {
        fallback(filled, number(0.0))
    } else {
        filled
    };
    let filled = filled.float()?;
    let now = b.now()?;
    let amount_raw = keyed(&o, "origQty")?;
    let filled_raw = keyed(&o, "executedQty")?;
    let filled_raw = if conditional {
        fallback(filled_raw, number(0.0))
    } else {
        filled_raw
    };
    let remaining = subtract(amount_raw, filled_raw)?;
    let out = record([
        ("id", id),
        ("status", status),
        ("symbol", symbol),
        ("type", typ),
        ("side", side),
        ("price", number(price)),
        ("amount", number(amount)),
        ("reduceOnly", reduce),
    ]);
    if let Some(close) = close {
        out.write("close", close)
    }
    for (k, v) in [
        ("filled", number(filled)),
        ("lastUpdate", number(now)),
        ("remaining", number(remaining)),
        ("info", o.clone()),
    ] {
        out.write(k, v)
    }
    trigger(&out, &o, v, None)?;
    Ok(out)
}
fn algo(o: EventValue, b: &mut impl SnapshotBindings) -> Result<EventValue, Failure> {
    let typ = table(
        crate::binance_types::ORDER_TYPE,
        keyed(&o, "orderType")?,
        PrivateEnum::Type,
    )?;
    let aid = keyed(&o, "algoId")?;
    let id = aid.optional_id("algoId")?;
    let symbol = o.read("symbol")?;
    let side = table(
        crate::binance_types::ORDER_SIDE,
        o.read("side")?,
        PrivateEnum::Side,
    )?;
    let price = fallback(number(keyed(&o, "triggerPrice")?.float()?), number(0.0));
    let amount = fallback(number(keyed(&o, "quantity")?.float()?), number(0.0));
    let reduce = fallback(keyed(&o, "reduceOnly")?, EventValue::Bool(false));
    let close = fallback(keyed(&o, "closePosition")?, EventValue::Bool(false));
    let now = b.now()?;
    let remaining = fallback(number(keyed(&o, "quantity")?.float()?), number(0.0));
    let info = record([("algoId", aid)]);
    for (k, v) in o.spread() {
        info.write(&k, v)
    }
    let out = record([
        ("id", id),
        (
            "status",
            EventValue::enum_value(PrivateEnum::Status(OrderStatus::Open)),
        ),
        ("symbol", symbol),
        ("type", typ.clone()),
        ("side", side),
        ("price", price),
        ("amount", amount),
        ("reduceOnly", reduce),
        ("close", close),
        ("filled", number(0.0)),
        ("lastUpdate", number(now)),
        ("remaining", remaining),
        ("info", info),
    ]);
    trigger(&out, &o, Venue::Futures, Some(typ))?;
    Ok(out)
}
pub(crate) async fn pair(
    mut a: SnapshotRequestFuture,
    mut b: SnapshotRequestFuture,
) -> Result<(EventValue, EventValue), Failure> {
    let (mut av, mut bv) = (None, None);
    poll_fn(move |cx| {
        if av.is_none() {
            if let Poll::Ready(v) = a.as_mut().poll(cx) {
                av = Some(v)
            }
        }
        if bv.is_none() {
            if let Poll::Ready(v) = b.as_mut().poll(cx) {
                bv = Some(v)
            }
        }
        let ae = av.as_ref().filter(|c| c.result.is_err());
        let be = bv.as_ref().filter(|c| c.result.is_err());
        if ae.is_some() || be.is_some() {
            let first = match (ae, be) {
                (Some(a), Some(b)) => a.reaction_order <= b.reaction_order,
                (Some(_), None) => true,
                _ => false,
            };
            return Poll::Ready(if first {
                av.take().unwrap().result.map(|_| unreachable!())
            } else {
                bv.take().unwrap().result.map(|_| unreachable!())
            });
        }
        if av.is_some() && bv.is_some() {
            Poll::Ready(Ok((
                av.take().unwrap().result.unwrap(),
                bv.take().unwrap().result.unwrap(),
            )))
        } else {
            Poll::Pending
        }
    })
    .await
}
fn concat(a: EventValue, b: EventValue) -> Result<EventValue, Failure> {
    fn values(v: EventValue) -> Result<Vec<EventValue>, Failure> {
        match v {
            EventValue::Array(a) => Ok(array_values(&a)?),
            EventValue::String(s) => Ok(EventValue::String(s.clone())
                .text()?
                .chars()
                .map(|c| string(c.to_string()))
                .collect()),
            _ => Err(type_error("(intermediate value) is not iterable")),
        }
    }
    let mut out = values(a)?;
    out.extend(values(b)?);
    Ok(EventValue::array(out))
}
fn arg(v: EventValue) -> SnapshotArgument {
    SnapshotArgument::Value(v)
}
pub(crate) fn error_message(e: &Failure) -> EventValue {
    let venue = e
        .response
        .as_ref()
        .and_then(|v| v.get("data"))
        .and_then(|v| v.get("msg"))
        .cloned()
        .map(EventValue::from_json)
        .unwrap_or(EventValue::Undefined);
    let message = safe_cex_exchange::js_runtime::value::Thrown::from((**e).clone())
        .value
        .get("message")
        .expect("fresh canonical error message");
    fallback(venue, message)
}
fn console(b: &mut impl SnapshotBindings, args: Vec<SnapshotArgument>) -> Result<(), Failure> {
    b.capture(SnapshotMethod::ConsoleError)?(args)
}
fn emit(b: &mut impl SnapshotBindings, e: &Failure) -> Result<(), Failure> {
    let call = b.capture(SnapshotMethod::Emit)?;
    call(vec![arg(error_message(e))])
}
// ts: BinanceExchange.fetchOrders
pub async fn fetch_futures_orders(
    state: &Cell<bool>,
    b: &mut impl SnapshotBindings,
) -> Result<EventValue, Failure> {
    let result: Result<EventValue, Failure> = async {
        let a = b.get(
            SnapshotClient::Private,
            endpoint(Venue::Futures, "OPEN_ORDERS"),
            None,
        )?;
        let c = b.get(
            SnapshotClient::Private,
            endpoint(Venue::Futures, "OPEN_ALGO_ORDERS"),
            Some(record([("algoType", string("CONDITIONAL"))])),
        )?;
        let (a, c) = pair(a, c).await?;
        if !matches!(a, EventValue::Array(_)) {
            console(
                b,
                vec![
                    arg(string("[Binance] fetchOrders received non-array response:")),
                    arg(a.clone()),
                ],
            )?;
            let call = b.capture(SnapshotMethod::Log)?;
            let text = safe_cex_exchange::js_runtime::globals::stringify_json(&a)
                .map_err(Rc::from)?
                .map(|units| EventValue::utf16(&units))
                .unwrap_or_else(|| string("undefined"));
            call(vec![
                arg(safe_cex_exchange::js_runtime::ops::add(
                    &string("fetchOrders received non-array response: "),
                    &text,
                )
                .map_err(Rc::from)?),
                arg(string("error")),
            ])?;
        }
        let a = if matches!(a, EventValue::Array(_)) {
            a
        } else {
            empty()
        };
        let c = if matches!(c, EventValue::Array(_)) {
            c
        } else {
            empty()
        };
        let regular = map(a, "ordersData", |o| regular(o, Venue::Futures, false, b))?;
        let algo = map(c, "algoOrdersData", |o| algo(o, b))?;
        state.set(false);
        concat(regular, algo)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            console(
                b,
                vec![
                    arg(string("[Binance] fetchOrders error:")),
                    SnapshotArgument::Error(e.clone()),
                ],
            )?;
            let response = e
                .response
                .as_ref()
                .and_then(|v| v.get("data"))
                .cloned()
                .map(EventValue::from_json)
                .unwrap_or(EventValue::Undefined);
            console(
                b,
                vec![
                    arg(string("[Binance] fetchOrders error response:")),
                    arg(response),
                ],
            )?;
            let call = b.capture(SnapshotMethod::Log)?;
            call(vec![
                arg(string(format!(
                    "fetchOrders error: {}",
                    if e.message.is_empty() {
                        e.js_error_string()
                    } else {
                        e.message.clone()
                    }
                ))),
                arg(string("error")),
            ])?;
            state.set(true);
            b.orders()
        }
    }
}
// ts: BinancePMExchange.fetchRegularOrders
pub async fn fetch_pm_regular_orders(b: &mut impl SnapshotBindings) -> Result<EventValue, Failure> {
    let ep = endpoint(Venue::PM, "OPEN_ORDERS");
    let data = b.get(SnapshotClient::Papi, ep, None)?.await.result?;
    let data = pm_rows(data, ep)?;
    map(data, "rows", |o| regular(o, Venue::PM, false, b))
}
// ts: BinancePMExchange.fetchConditionalOrders
pub async fn fetch_pm_conditional_orders(
    b: &mut impl SnapshotBindings,
) -> Result<EventValue, Failure> {
    let ep = endpoint(Venue::PM, "OPEN_CONDITIONAL_ORDERS");
    let data = b.get(SnapshotClient::Papi, ep, None)?.await.result?;
    let data = pm_rows(data, ep)?;
    map(data, "rows", |o| regular(o, Venue::PM, true, b))
}
// ts: BinanceSpotExchange.fetchOrders
pub async fn fetch_spot_orders(b: &mut impl SnapshotBindings) -> Result<EventValue, Failure> {
    let data = b
        .get(
            SnapshotClient::Private,
            endpoint(Venue::Spot, "OPEN_ORDERS"),
            None,
        )?
        .await
        .result?;
    map(data, "data", |o| regular(o, Venue::Spot, false, b))
}
// ts: BinancePMExchange.fetchOrders
pub async fn fetch_pm_orders(
    state: &Cell<bool>,
    mut regular: impl FnMut() -> Result<SnapshotFuture, Failure>,
    mut conditional: impl FnMut() -> Result<SnapshotFuture, Failure>,
    b: &mut impl SnapshotBindings,
) -> Result<EventValue, Failure> {
    let result: Result<EventValue, Failure> = async {
        let first = regular()?.await?; // source spreads first result before calling second method
        let first = concat(first, empty())?;
        let second = conditional()?.await?;
        let out = concat(first, second)?;
        state.set(false);
        Ok(out)
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            if state.get() {
                let call = b.capture(SnapshotMethod::Log)?;
                let mut message = "fetchOrders error: ".encode_utf16().collect::<Vec<_>>();
                message
                    .extend_from_slice(&error_message(&e).to_js_string().map_err(Failure::from)?);
                call(vec![
                    arg(EventValue::utf16(&message)),
                    arg(string("warning")),
                ])?
            } else {
                emit(b, &e)?
            }
            state.set(true);
            b.orders()
        }
    }
}
// ts: BinancePMExchange.fetchBalance
pub async fn fetch_pm_balance(b: &mut impl SnapshotBindings) -> Result<EventValue, Failure> {
    let result: Result<EventValue, Failure> = async {
        let ep = endpoint(Venue::PM, "BALANCE");
        let data = b.get(SnapshotClient::Papi, ep, None)?.await.result?;
        let rows = pm_rows(data.clone(), ep)?;
        let EventValue::Array(rows) = rows else {
            unreachable!()
        };
        let (mut total, mut used, mut free, mut upnl) = (0.0, 0.0, 0.0, 0.0);
        let mut i = 0;
        while i < rows.len() {
            let item = EventValue::Array(rows.clone()).get(&i.to_string())?;
            let asset = item.read("asset")?;
            if ["USDT", "USDC", "FDUSD"]
                .iter()
                .any(|s| asset.same(&string(*s)))
            {
                let item = EventValue::Array(rows.clone()).get(&i.to_string())?;
                let item_total = item.read("totalWalletBalance")?.float()?;
                let item_free = item.read("crossMarginFree")?.float()?;
                free += item_free;
                total += item_total;
                used += item_total - item_free;
                upnl += item.read("umUnrealizedPNL")?.float()?
            }
            i += 1
        }
        Ok(record([(
            "balance",
            record([
                ("total", number(total)),
                ("free", number(free)),
                ("used", number(used)),
                ("upnl", number(upnl)),
                ("info", data),
            ]),
        )]))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(b, &e)?;
            let balance = b.balance()?;
            Ok(record([("balance", balance)]))
        }
    }
}
// ts: BinanceSpotExchange.fetchPositionsAndBalance
pub async fn fetch_spot_positions_balance(
    b: &mut impl SnapshotBindings,
) -> Result<EventValue, Failure> {
    let result: Result<EventValue, Failure> = async {
        let data = b
            .get(
                SnapshotClient::Private,
                endpoint(Venue::Spot, "ACCOUNT"),
                None,
            )?
            .await
            .result?;
        let currencies = record([]);
        let coins = record([]);
        let (mut free, mut used, mut total) = (0.0, 0.0, 0.0);
        let balances = fallback(data.read("balances")?, empty());
        let mut i = 0;
        loop {
            let entry = match &balances {
                EventValue::Array(a) => crate::private_event_value::array_element(&a, i)?,
                EventValue::String(s) => EventValue::String(s.clone())
                    .text()?
                    .chars()
                    .nth(i)
                    .map(|c| string(c.to_string())),
                _ => {
                    return Err(type_error(
                        "object is not iterable (cannot read property Symbol(Symbol.iterator))",
                    ));
                }
            };
            let Some(row) = entry else { break };
            i += 1;
            let asset = row.read("asset")?;
            let af = fallback(row.read("free")?, string("0")).float()?;
            let au = fallback(row.read("locked")?, string("0")).float()?;
            let at = af + au;
            let price = b.asset_price(asset.clone())?;
            currencies.assign(
                &asset.text()?,
                record([
                    ("free", number(af)),
                    ("used", number(au)),
                    ("total", number(at)),
                    ("upnl", number(0.0)),
                    ("info", row),
                ]),
            );
            coins.assign(
                &asset.text()?,
                record([
                    ("base", asset),
                    ("amount", number(at)),
                    ("notional", number(at * price)),
                ]),
            );
            free += af * price;
            used += au * price;
            total += at * price;
        }
        Ok(record([
            (
                "balance",
                record([
                    ("total", number(total)),
                    ("free", number(free)),
                    ("used", number(used)),
                    ("upnl", number(0.0)),
                    ("currencies", currencies),
                    ("coins", coins),
                    ("info", data),
                ]),
            ),
            ("positions", empty()),
        ]))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            emit(b, &e)?;
            let balance = b.balance()?;
            let positions = b.positions()?;
            Ok(record([("balance", balance), ("positions", positions)]))
        }
    }
}
// ts: BinanceExchange.findOrderByClientOrderId
pub async fn find_futures_order_by_client_id(
    symbol: &str,
    id: &str,
    b: &mut impl SnapshotBindings,
) -> Result<EventValue, Failure> {
    let orders = b.orders()?;
    let EventValue::Array(orders) = orders else {
        if orders.nullish() {
            orders.read("find")?;
        }
        return Err(type_error("this.store.orders.find is not a function"));
    };
    let found = find(&orders, |o| {
        Ok(o.read("symbol")?.same(&string(symbol)) && b.carries_client_id(o, id)?)
    })?;
    if found.truthy() {
        return Ok(record([
            ("found", EventValue::Bool(true)),
            ("order", found),
        ]));
    }
    if !b.market(symbol)?.truthy() {
        return Ok(record([
            ("found", EventValue::Bool(false)),
            ("definitive", EventValue::Bool(false)),
        ]));
    }
    let result: Result<EventValue, Failure> = async {
        let client = if b.has_primary()? {
            SnapshotClient::Primary
        } else {
            SnapshotClient::Private
        };
        let data = b
            .get(
                client,
                endpoint(Venue::Futures, "ORDER"),
                Some(record([
                    ("symbol", string(symbol)),
                    ("origClientOrderId", string(id)),
                ])),
            )?
            .await
            .result?;
        let order = b.map_history(nullish(data.clone(), record([])))?;
        if !order.truthy() {
            return Ok(record([
                ("found", EventValue::Bool(false)),
                ("definitive", EventValue::Bool(false)),
            ]));
        }
        let amount = order.read("amount")?;
        let filled = order.read("filled")?;
        let difference = amount.number()? - filled.number()?;
        let remaining = if difference.is_nan() {
            f64::NAN
        } else {
            difference.max(0.0)
        };
        let out = record([
            ("id", order.read("id")?),
            ("symbol", string(symbol)),
            ("side", order.read("side")?),
            ("type", order.read("type")?),
            ("status", order.read("status")?),
            ("price", nullish(order.read("price")?, number(0.0))),
            ("amount", amount),
            ("filled", filled),
            ("remaining", number(remaining)),
            (
                "reduceOnly",
                nullish(order.read("reduceOnly")?, EventValue::Bool(false)),
            ),
            (
                "lastUpdate",
                nullish(order.read("updatedTime")?, order.read("createdTime")?),
            ),
            ("info", data),
        ]);
        Ok(record([("found", EventValue::Bool(true)), ("order", out)]))
    }
    .await;
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            if e.response
                .as_ref()
                .and_then(|v| v.get("data"))
                .and_then(|v| v.get("code"))
                .and_then(Value::as_f64)
                == Some(-2013.0)
            {
                Ok(record([
                    ("found", EventValue::Bool(false)),
                    ("definitive", EventValue::Bool(true)),
                ]))
            } else {
                Err(e)
            }
        }
    }
}
