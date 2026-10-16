//! Private feed routing, parsing and order reconciliation against bound JS and store operations.
use crate::{
    builder_workflow::{JsPromise, JsResult},
    private_commands::PrivateCommands,
};
use safe_cex_exchange::runtime::LogSeverity;
use std::{hash::Hash, rc::Rc};

pub type TimerCallback = Rc<dyn Fn()>;
pub type Predicate<'a, V> = dyn FnMut(V) -> JsResult<V, bool> + 'a;
pub type Visitor<'a, V> = dyn FnMut(V) -> JsResult<V, ()> + 'a;

/// Values preserve identity; FillKey binds SameValueZero without string conversion.
/// Array hooks bind find/filter/forEach/spread, including live reads and sparse slots.
/// start owns eager async continuations; timers retain callbacks until cancellation/firing.
pub trait PrivateFeedHost: 'static {
    type Value: Clone + 'static;
    type FillKey: Clone + Eq + Hash + 'static;
    fn state(&self) -> &PrivateCommands<Self::Value, Self::FillKey>;
    fn start(&self, task: JsPromise<Self::Value>) -> JsPromise<Self::Value>;
    fn property(&self, value: &Self::Value, key: &str) -> JsResult<Self::Value>;
    fn indexed(&self, value: &Self::Value, key: &Self::Value) -> JsResult<Self::Value>;
    fn string(&self, value: &str) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn undefined(&self) -> Self::Value;
    fn null(&self) -> Self::Value;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn is_undefined(&self, value: &Self::Value) -> bool;
    fn is_number(&self, value: &Self::Value) -> bool;
    fn is_string(&self, value: &Self::Value) -> bool;
    fn is_array(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn loose_number_equal(&self, value: &Self::Value, number: f64) -> JsResult<Self::Value, bool>;
    fn number_value(&self, value: &Self::Value) -> JsResult<Self::Value, f64>;
    fn parse_float(&self, value: &Self::Value) -> JsResult<Self::Value, f64>;
    fn parse_int(&self, value: &Self::Value) -> JsResult<Self::Value, f64>;
    fn decimal_pattern(&self, value: &Self::Value) -> JsResult<Self::Value, bool>;
    fn to_string(&self, value: &Self::Value) -> JsResult<Self::Value>;
    /// Interpolation ToString returns a primitive string and preserves abrupt completion.
    fn template(&self, parts: Vec<Self::Value>) -> JsResult<Self::Value>;
    fn uppercase(&self, value: &Self::Value, expression: &'static str) -> JsResult<Self::Value>;
    fn keys_join(&self, value: &Self::Value) -> JsResult<Self::Value>;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    /// CreateDataProperty bypasses inherited setters.
    fn define(
        &self,
        value: &Self::Value,
        key: &'static str,
        field: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    /// CopyDataProperties defines own keys without invoking inherited setters.
    fn spread(&self, target: &Self::Value, source: &Self::Value) -> JsResult<Self::Value, ()>;
    fn find(
        &self,
        array: &Self::Value,
        predicate: &mut Predicate<'_, Self::Value>,
    ) -> JsResult<Self::Value>;
    fn filter(
        &self,
        array: &Self::Value,
        predicate: &mut Predicate<'_, Self::Value>,
    ) -> JsResult<Self::Value>;
    fn for_each(
        &self,
        array: &Self::Value,
        visitor: &mut Visitor<'_, Self::Value>,
    ) -> JsResult<Self::Value, ()>;
    fn append_spread(&self, array: &Self::Value, value: Self::Value) -> JsResult<Self::Value>;
    fn store(&self) -> JsResult<Self::Value>;
    fn json_parse(&self, data: &Self::Value) -> JsResult<Self::Value>;
    fn now(&self) -> JsResult<Self::Value, f64>;
    fn virtual_now(&self) -> JsResult<Self::Value, f64>;
    fn position_id(&self, symbol: &Self::Value) -> JsResult<Self::Value>;
    fn trigger_metadata(&self, feed: &Self::Value) -> JsResult<Self::Value>;
    fn time_in_force(&self, feed: &Self::Value) -> JsResult<Self::Value>;
    fn order_creation_time(&self, metadata: &Self::Value) -> JsResult<Self::Value>;
    fn record_nonce(
        &self,
        id: Self::Value,
        real_id: Self::Value,
        time: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    fn fill_key(&self, value: &Self::Value) -> Self::FillKey;
    fn log(&self, message: Self::Value, severity: Option<LogSeverity>)
    -> JsResult<Self::Value, ()>;
    fn emit_error(&self, message: Self::Value) -> JsResult<Self::Value, ()>;
    fn emit_fill(&self, fill: Self::Value) -> JsResult<Self::Value, ()>;
    fn record_stats(&self, topic: &'static str) -> JsResult<Self::Value, ()>;
    fn track_received(&self) -> JsResult<Self::Value, ()>;
    fn pong(&self) -> JsResult<Self::Value, ()>;
    fn subscribe(&self) -> JsResult<Self::Value, ()>;
    fn refresh_session(&self, reason: &'static str) -> JsResult<Self::Value, ()>;
    fn update_order_status(
        &self,
        selector: Self::Value,
        status: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    fn remove_order(&self, selector: Self::Value) -> JsResult<Self::Value, ()>;
    fn add_or_update_order(&self, order: Self::Value) -> JsResult<Self::Value, ()>;
    fn update_position(
        &self,
        existing: Self::Value,
        changes: Self::Value,
    ) -> JsResult<Self::Value, ()>;
    fn update_store(&self, changes: Self::Value) -> JsResult<Self::Value, ()>;
    fn sync_upnl(&self) -> JsResult<Self::Value, ()>;
    fn throttle_positions(&self) -> JsResult<Self::Value, ()>;
    fn schedule(&self, callback: TimerCallback, delay: f64) -> JsResult<Self::Value>;
    fn clear_timeout(&self, timer: &Self::Value) -> JsResult<Self::Value, ()>;
    fn fetch_orders(&self) -> JsResult<Self::Value, JsPromise<Self::Value>>;
}
macro_rules! js_template {
    ($host:expr; $($part:expr),* $(,)?) => {{
        (|| -> JsResult<_> {
            let mut parts = Vec::new();
            $(let value = $part; parts.push($host.template(vec![value])?);)*
            $host.template(parts)
        })()
    }};
}
fn or<H: PrivateFeedHost>(h: &H, value: &H::Value, a: &str, b: &str) -> JsResult<H::Value> {
    let a = h.property(value, a)?;
    if h.truthy(&a) {
        Ok(a)
    } else {
        h.property(value, b)
    }
}
fn optional<H: PrivateFeedHost>(h: &H, value: &H::Value, key: &str) -> JsResult<H::Value> {
    if h.nullish(value) {
        Ok(h.undefined())
    } else {
        h.property(value, key)
    }
}
fn default<H: PrivateFeedHost>(h: &H, value: H::Value, fallback: &str) -> H::Value {
    if h.truthy(&value) {
        value
    } else {
        h.string(fallback)
    }
}
fn store_field<H: PrivateFeedHost>(h: &H, key: &str) -> JsResult<H::Value> {
    h.property(&h.store()?, key)
}
fn first_leg<H: PrivateFeedHost>(h: &H, feed: &H::Value) -> JsResult<H::Value> {
    let first = optional(h, &h.property(feed, "legs")?, "0")?;
    if h.truthy(&first) {
        Ok(first)
    } else {
        optional(h, &h.property(feed, "l")?, "0")
    }
}
fn market<H: PrivateFeedHost>(
    h: &H,
    instrument: &H::Value,
    optional_upper: bool,
    expression: &'static str,
) -> JsResult<H::Value> {
    let first = h.indexed(&store_field(h, "marketsById")?, instrument)?;
    if h.truthy(&first) {
        return Ok(first);
    }
    let map = store_field(h, "marketsById")?;
    let upper = if optional_upper && h.nullish(instrument) {
        h.undefined()
    } else {
        h.uppercase(instrument, expression)?
    };
    let second = h.indexed(&map, &upper)?;
    if h.truthy(&second) {
        return Ok(second);
    }
    let markets = store_field(h, "markets")?;
    let instrument = instrument.clone();
    h.find(&markets, &mut |m| {
        let left = h.uppercase(&h.property(&m, "id")?, "m.id")?;
        let right = if optional_upper && h.nullish(&instrument) {
            h.undefined()
        } else {
            h.uppercase(&instrument, expression)?
        };
        Ok(h.equal(&left, &right))
    })
}
fn report_cause<H: PrivateFeedHost>(
    h: &H,
    prefix: &str,
    error: H::Value,
) -> JsResult<H::Value, ()> {
    h.emit_error(js_template!(h; h.string(prefix), error)?)
}

/// ts: parsePositiveFiniteDecimal.
pub fn parse_positive_finite_decimal<H: PrivateFeedHost>(
    h: &H,
    value: &H::Value,
) -> JsResult<H::Value, Option<f64>> {
    let amount = if h.is_number(value) || h.is_string(value) && h.decimal_pattern(value)? {
        h.number_value(value)?
    } else {
        f64::NAN
    };
    Ok((amount.is_finite() && amount > 0.0).then_some(amount))
}

/// ts: GrvtPrivateWebSocket#parseOrder.
pub fn parse_order<H: PrivateFeedHost>(h: &H, feed: H::Value) -> JsResult<H::Value> {
    let result: JsResult<H::Value> = (|| {
        let first = first_leg(h, &feed)?;
        let instrument = optional(h, &first, "instrument")?;
        let instrument = if h.truthy(&instrument) {
            instrument
        } else {
            optional(h, &first, "i")?
        };
        if !h.truthy(&instrument) {
            let legs = or(h, &feed, "legs", "l")?;
            let count = if h.is_array(&legs) {
                h.property(&or(h, &feed, "legs", "l")?, "length")?
            } else {
                h.number(0.0)
            };
            h.log(
                js_template!(h;
                    h.string("parseOrder: missing instrument ID (event=order, legCount="),
                    count,
                    h.string(")"),
                )?,
                Some(LogSeverity::Warning),
            )?;
            return Ok(h.null());
        }
        let market = market(h, &instrument, false, "instrumentId")?;
        if !h.truthy(&market) {
            h.log(
                js_template!(h;
                    h.string("parseOrder: market not found (event=order, marketCount="),
                    h.property(&store_field(h, "markets")?, "length")?,
                    h.string(")"),
                )?,
                Some(LogSeverity::Warning),
            )?;
            return Ok(h.null());
        }
        let raw_id = or(h, &feed, "order_id", "oi")?;
        let leg = first_leg(h, &feed)?;
        let state = or(h, &feed, "state", "s1")?;
        let metadata = or(h, &feed, "metadata", "m")?;
        let client = optional(h, &metadata, "client_order_id")?;
        let client = if h.truthy(&client) {
            client
        } else {
            optional(h, &metadata, "co")?
        };
        let id = if h.truthy(&client) {
            h.to_string(&client)?
        } else {
            raw_id.clone()
        };
        let real = h.truthy(&raw_id)
            && !h.equal(&raw_id, &h.string("0x00"))
            && !h.equal(&raw_id, &h.string("0x0"));
        if h.truthy(&client) {
            let client = h.to_string(&client)?;
            let real = if real { raw_id.clone() } else { h.undefined() };
            let time = h.order_creation_time(&metadata)?;
            h.record_nonce(client, real, time)?;
        }
        if !h.truthy(&id) || h.equal(&id, &h.string("0x00")) || h.equal(&id, &h.string("0x0")) {
            h.log(
                js_template!(h;
                    h.string("parseOrder: no usable order ID (event=order, hasRawOrderId="),
                    h.boolean(h.truthy(&raw_id)),
                    h.string(", hasClientOrderId="),
                    h.boolean(h.truthy(&client)),
                    h.string(")"),
                )?,
                Some(LogSeverity::Warning),
            )?;
        }
        if !h.truthy(&state) {
            h.log(
                js_template!(h;
                    h.string("parseOrder: missing state (event=order, hasFullState="),
                    h.boolean(h.truthy(&h.property(&feed, "state")?)),
                    h.string(", hasLiteState="),
                    h.boolean(h.truthy(&h.property(&feed, "s1")?)),
                    h.string(")"),
                )?,
                Some(LogSeverity::Warning),
            )?;
        }
        if !h.truthy(&id) || !h.truthy(&leg) || !h.truthy(&state) {
            return Ok(h.null());
        }
        let upstream = or(h, &state, "status", "s")?;
        let status = if h.equal(&upstream, &h.string("FILLED")) {
            "closed"
        } else if h.equal(&upstream, &h.string("REJECTED"))
            || h.equal(&upstream, &h.string("CANCELLED"))
        {
            "canceled"
        } else {
            "open"
        };
        let buying = h.property(&leg, "is_buying_asset")?;
        let buying = if !h.is_undefined(&buying) {
            h.property(&leg, "is_buying_asset")?
        } else {
            h.property(&leg, "ib")?
        };
        let side = if h.truthy(&buying) { "buy" } else { "sell" };
        let trigger = h.trigger_metadata(&feed)?;
        let is_market = h.property(&feed, "is_market")?;
        let is_market = if !h.is_undefined(&is_market) {
            h.property(&feed, "is_market")?
        } else {
            h.property(&feed, "im")?
        };
        let kind = if h.equal(&optional(h, &trigger, "type")?, &h.string("STOP_LOSS")) {
            "stop_market"
        } else if h.equal(&optional(h, &trigger, "type")?, &h.string("TAKE_PROFIT")) {
            "take_profit_market"
        } else if h.truthy(&is_market) {
            "market"
        } else {
            "limit"
        };
        let size = h.parse_float(&default(h, or(h, &leg, "size", "s")?, "0"))?;
        let traded = optional(h, &h.property(&state, "traded_size")?, "0")?;
        let traded = if h.truthy(&traded) {
            traded
        } else {
            optional(h, &h.property(&state, "ts")?, "0")?
        };
        let filled = h.parse_float(&default(h, traded, "0"))?;
        let remaining = safe_cex_num::math_max(0.0, size - filled);
        let limit = h.parse_float(&default(h, or(h, &leg, "limit_price", "lp")?, "0"))?;
        let price = optional(h, &trigger, "triggerPrice")?;
        let price = if h.nullish(&price) {
            h.number(limit)
        } else {
            price
        };
        let time = or(h, &state, "update_time", "ut")?;
        let time = if h.truthy(&time) {
            time
        } else {
            optional(h, &metadata, "create_time")?
        };
        let time = if h.truthy(&time) {
            time
        } else {
            optional(h, &metadata, "ct")?
        };
        let last = if h.truthy(&time) {
            (h.parse_int(&time)? / 1000000.0).floor()
        } else {
            h.virtual_now()?
        };
        let order = h.object(vec![
            ("id", id),
            ("status", h.string(status)),
            ("symbol", h.property(&market, "symbol")?),
            ("type", h.string(kind)),
            ("side", h.string(side)),
            ("price", price),
            ("amount", h.number(size)),
            ("filled", h.number(filled)),
            ("remaining", h.number(remaining)),
            ("lastUpdate", h.number(last)),
            ("reduceOnly", {
                let v = or(h, &feed, "reduce_only", "ro")?;
                if h.truthy(&v) { v } else { h.boolean(false) }
            }),
            ("timeInForce", h.time_in_force(&feed)?),
        ]);
        if !h.is_undefined(&optional(h, &trigger, "close")?) {
            h.define(&order, "close", h.property(&trigger, "close")?)?;
        }
        if kind == "stop_market" && h.truthy(&optional(h, &trigger, "triggerType")?) {
            h.define(
                &order,
                "slTriggerType",
                h.property(&trigger, "triggerType")?,
            )?;
        }
        if kind == "take_profit_market" && h.truthy(&optional(h, &trigger, "triggerType")?) {
            h.define(
                &order,
                "tpTriggerType",
                h.property(&trigger, "triggerType")?,
            )?;
        }
        h.define(&order, "info", feed)?;
        Ok(order)
    })();
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            report_cause(h, "Error parsing order: ", e)?;
            Ok(h.null())
        }
    }
}

/// ts: GrvtPrivateWebSocket#parsePosition.
pub fn parse_position<H: PrivateFeedHost>(h: &H, feed: H::Value) -> JsResult<H::Value> {
    let result: JsResult<H::Value> = (|| {
        let instrument = or(h, &feed, "instrument", "i")?;
        if !h.truthy(&instrument) {
            h.log(
                h.string("Position update: missing instrument in feed"),
                Some(LogSeverity::Debug),
            )?;
            return Ok(h.null());
        }
        let size = h.property(&feed, "size")?;
        let size = if h.nullish(&size) {
            h.property(&feed, "s")?
        } else {
            size
        };
        if h.nullish(&size) {
            h.log(
                js_template!(h;
                    h.string("Position update: missing size for instrument "),
                    instrument,
                )?,
                Some(LogSeverity::Debug),
            )?;
            return Ok(h.null());
        }
        let size = h.parse_float(&size)?;
        if !size.is_finite() {
            h.log(
                js_template!(h;
                    h.string("Position update: invalid size for instrument "),
                    instrument,
                )?,
                Some(LogSeverity::Debug),
            )?;
            return Ok(h.null());
        }
        let market = market(h, &instrument, false, "instrument")?;
        if !h.truthy(&market) {
            h.log(
                js_template!(h;
                    h.string("Position update: market not found for instrument "),
                    instrument,
                )?,
                Some(LogSeverity::Warning),
            )?;
            return Ok(h.null());
        }
        let side = if size > 0.0 {
            "long"
        } else if size < 0.0 {
            "short"
        } else {
            return Ok(h.null());
        };
        let contracts = size.abs();
        let notional = h
            .parse_float(&default(h, or(h, &feed, "notional", "n")?, "0"))?
            .abs();
        let entry = h.parse_float(&default(h, or(h, &feed, "entry_price", "ep")?, "0"))?;
        let liquidation =
            h.parse_float(&default(h, or(h, &feed, "liquidation_price", "lp")?, "0"))?;
        let upnl = h.parse_float(&default(h, or(h, &feed, "unrealized_pnl", "up")?, "0"))?;
        let leverage = h.parse_float(&default(h, or(h, &feed, "leverage", "l")?, "1"))?;
        Ok(h.object(vec![
            ("id", h.position_id(&h.property(&market, "symbol")?)?),
            ("symbol", h.property(&market, "symbol")?),
            ("side", h.string(side)),
            ("notional", h.number(notional)),
            ("leverage", h.number(leverage)),
            ("entryPrice", h.number(entry)),
            ("unrealizedPnl", h.number(upnl)),
            ("contracts", h.number(contracts)),
            ("liquidationPrice", h.number(liquidation)),
        ]))
    })();
    match result {
        Ok(v) => Ok(v),
        Err(e) => {
            report_cause(h, "Error parsing position: ", e)?;
            Ok(h.null())
        }
    }
}

/// ts: GrvtPrivateWebSocket#throttledFetchOrders.
pub fn throttled_fetch_orders<H: PrivateFeedHost>(host: Rc<H>) -> JsResult<H::Value, ()> {
    let timer = host.state().orders_throttle_timeout.borrow().clone();
    if let Some(timer) = timer.filter(|v| host.truthy(v)) {
        host.clear_timeout(&timer)?;
    }
    let h = host.clone();
    let timer = host.schedule(
        Rc::new(move || {
            drop(fetch_throttled_orders(h.clone()));
        }),
        1000.0,
    )?;
    *host.state().orders_throttle_timeout.borrow_mut() = Some(timer);
    Ok(())
}
/// ts: GrvtPrivateWebSocket#fetchThrottledOrders.
pub fn fetch_throttled_orders<H: PrivateFeedHost>(host: Rc<H>) -> JsPromise<H::Value> {
    let h = host.clone();
    host.start(Box::pin(async move {
        *h.state().orders_throttle_timeout.borrow_mut() = None;
        if !h.state().is_disposed.get() {
            let result: JsResult<H::Value> = async {
                let before = h.property(&store_field(h.as_ref(),"orders")?,"length")?;
                let orders = h.fetch_orders()?.await?;
                let count = h.property(&orders,"length")?;
                h.log(js_template!(h; h.string("Throttled fetch: received "),count.clone(),h.string(" open orders from API (was "),before.clone(),h.string(" in store)"))?,Some(LogSeverity::Info))?;
                if h.number_value(&before)? > 0.0 && h.equal(&h.property(&orders,"length")?,&h.number(0.0)) {
                    h.log(js_template!(h; h.string("WARNING: Skipping store update - API returned 0 orders but store has "),before,h.string(". Keeping existing orders."))?,Some(LogSeverity::Warning))?;
                    return Ok(h.undefined());
                }
                let loaded = h.object(vec![]);
                h.spread(&loaded,&store_field(h.as_ref(),"loaded")?)?;
                h.define(&loaded,"orders",h.boolean(true))?;
                h.update_store(h.object(vec![("orders",orders),("loaded",loaded)]))?;
                Ok(h.undefined())
            }.await;
            if let Err(e)=result { h.log(js_template!(h; h.string("Throttled fetch error: "),e)?,Some(LogSeverity::Warning))?; }
        }
        Ok(h.undefined())
    }))
}

/// ts: GrvtPrivateWebSocket#handleOrderUpdate.
pub fn handle_order_update<H: PrivateFeedHost>(
    host: Rc<H>,
    json: H::Value,
) -> JsResult<H::Value, ()> {
    let h = host.as_ref();
    let result: JsResult<H::Value, ()> = (|| {
        let feed = or(h, &json, "feed", "f")?;
        if !h.truthy(&feed) {
            h.log(
                h.string("Order update: no feed data in message"),
                Some(LogSeverity::Debug),
            )?;
            return Ok(());
        }
        let order = parse_order(h, feed)?;
        if !h.truthy(&order) {
            h.log(
                h.string("Order update: parseOrder returned null"),
                Some(LogSeverity::Debug),
            )?;
            return Ok(());
        }
        h.log(
            js_template!(h;
                h.string("Order update: "),
                h.property(&order, "id")?,
                h.string(" status="),
                h.property(&order, "status")?,
                h.string(" symbol="),
                h.property(&order, "symbol")?,
                h.string(" side="),
                h.property(&order, "side")?,
                h.string(" amount="),
                h.property(&order, "amount")?,
            )?,
            Some(LogSeverity::Info),
        )?;
        let status = h.property(&order, "status")?;
        if h.equal(&status, &h.string("closed"))
            || h.equal(&h.property(&order, "status")?, &h.string("canceled"))
        {
            let status = if h.equal(&status, &h.string("closed")) {
                "closed"
            } else {
                "canceled"
            };
            h.update_order_status(
                h.object(vec![("id", h.property(&order, "id")?)]),
                h.string(status),
            )?;
            h.remove_order(h.object(vec![("id", h.property(&order, "id")?)]))?;
            throttled_fetch_orders(host.clone())?;
        } else {
            h.add_or_update_order(order)?;
            throttled_fetch_orders(host.clone())?;
        }
        h.record_stats("orders")?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(e) => report_cause(h, "Error handling order update: ", e),
    }
}

/// ts: GrvtPrivateWebSocket#handlePositionUpdate.
pub fn handle_position_update<H: PrivateFeedHost>(h: &H, json: H::Value) -> JsResult<H::Value, ()> {
    let result: JsResult<H::Value, ()> = (|| {
        let feed = or(h, &json, "feed", "f")?;
        if !h.truthy(&feed) {
            return Ok(());
        }
        let instrument = or(h, &feed, "instrument", "i")?;
        let size = h.property(&feed, "size")?;
        let size = if h.nullish(&size) {
            h.property(&feed, "s")?
        } else {
            size
        };
        let size = if h.is_number(&size) {
            h.number_value(&size)?
        } else {
            h.parse_float(&default(h, size, "NaN"))?
        };
        if h.truthy(&instrument) && size.is_finite() && size == 0.0 {
            let market = market(h, &instrument, false, "instrument")?;
            if h.truthy(&market) {
                let open = h.filter(&store_field(h, "positions")?, &mut |p| {
                    Ok(
                        h.equal(&h.property(&p, "symbol")?, &h.property(&market, "symbol")?)
                            && h.number_value(&h.property(&p, "contracts")?)?.abs() > 0.0,
                    )
                })?;
                h.for_each(&open, &mut |p| {
                    let now = h.virtual_now()?;
                    h.update_position(
                        p,
                        h.object(vec![
                            ("contracts", h.number(0.0)),
                            ("notional", h.number(0.0)),
                            ("unrealizedPnl", h.number(0.0)),
                            ("liquidationPrice", h.number(0.0)),
                            ("lastUpdate", h.number(now)),
                        ]),
                    )
                })?;
            }
            h.sync_upnl()?;
            h.state().has_pending_position_events.set(true);
            h.throttle_positions()?;
            h.record_stats("positions")?;
            return Ok(());
        }
        let position = parse_position(h, feed)?;
        if !h.truthy(&position) {
            h.log(
                h.string("Position update: parsePosition returned null"),
                Some(LogSeverity::Debug),
            )?;
            h.state().has_pending_position_events.set(true);
            h.throttle_positions()?;
            return Ok(());
        }
        h.log(
            js_template!(h;
                h.string("Position update: "),
                h.property(&position, "symbol")?,
                h.string(" "),
                h.property(&position, "side")?,
                h.string(" "),
                h.property(&position, "contracts")?,
                h.string(" contracts"),
            )?,
            Some(LogSeverity::Info),
        )?;
        let existing = h.find(&store_field(h, "positions")?, &mut |p| {
            Ok(h.equal(
                &h.property(&p, "symbol")?,
                &h.property(&position, "symbol")?,
            ))
        })?;
        if h.truthy(&existing) {
            h.log(
                js_template!(h;
                    h.string("Updating existing position for "),
                    h.property(&position, "symbol")?,
                )?,
                Some(LogSeverity::Debug),
            )?;
            h.update_position(existing, position)?;
        } else {
            h.log(
                js_template!(h;
                    h.string("Adding new position for "),
                    h.property(&position, "symbol")?,
                )?,
                Some(LogSeverity::Debug),
            )?;
            h.update_store(h.object(vec![(
                "positions",
                h.append_spread(&store_field(h, "positions")?, position)?,
            )]))?;
        }
        h.sync_upnl()?;
        h.state().has_pending_position_events.set(true);
        h.throttle_positions()?;
        h.record_stats("positions")?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(e) => report_cause(h, "Error handling position update: ", e),
    }
}

/// ts: GrvtPrivateWebSocket#handleFillUpdate.
pub fn handle_fill_update<H: PrivateFeedHost>(h: &H, json: H::Value) -> JsResult<H::Value, ()> {
    let result: JsResult<H::Value, ()> = (|| {
        let feed = or(h, &json, "feed", "f")?;
        if !h.truthy(&feed) {
            return Ok(());
        }
        let instrument = or(h, &feed, "instrument", "i")?;
        let buyer = h.property(&feed, "is_buyer")?;
        let buyer = if !h.is_undefined(&buyer) {
            h.property(&feed, "is_buyer")?
        } else {
            h.property(&feed, "ib")?
        };
        let size = or(h, &feed, "size", "s")?;
        let price = or(h, &feed, "price", "p")?;
        let trade = or(h, &feed, "trade_id", "ti")?;
        let Some(amount) = parse_positive_finite_decimal(h, &size)? else {
            h.log(
                h.string("Ignoring GRVT fill with invalid size"),
                Some(LogSeverity::Warning),
            )?;
            return Ok(());
        };
        let fill_key = if h.truthy(&trade) {
            trade.clone()
        } else {
            js_template!(h;
                instrument.clone(),
                h.string("-"),
                price.clone(),
                h.string("-"),
                size.clone(),
                h.string("-"),
                h.number(h.now()?),
            )?
        };
        let key = h.fill_key(&fill_key);
        if h.state().processed_fill_ids.borrow().contains(&key) {
            return Ok(());
        }
        h.state().processed_fill_ids.borrow_mut().insert(key);
        if h.state().processed_fill_ids.borrow().len() > 500 {
            h.state()
                .processed_fill_ids
                .borrow_mut()
                .shift_remove_index(0);
        }
        let market = market(h, &instrument, true, "instrument")?;
        if !h.truthy(&market) {
            return Ok(());
        }
        let client = or(h, &feed, "client_order_id", "co")?;
        let id = if h.truthy(&client) {
            h.to_string(&client)?
        } else {
            h.undefined()
        };
        h.log(
            js_template!(h;
                h.string("Fill event: tradeId="),
                trade.clone(),
                h.string(" orderId="),
                id.clone(),
                h.string(" symbol="),
                h.property(&market, "symbol")?,
                h.string(" size="),
                size,
                h.string(" price="),
                price.clone(),
                h.string(" feedKeys="),
                h.keys_join(&feed)?,
            )?,
            Some(LogSeverity::Info),
        )?;
        let fill = h.object(vec![("id", trade)]);
        if h.truthy(&id) {
            h.define(&fill, "orderId", id)?;
        }
        h.define(
            &fill,
            "side",
            h.string(if h.truthy(&buyer) { "buy" } else { "sell" }),
        )?;
        h.define(&fill, "symbol", h.property(&market, "symbol")?)?;
        h.define(&fill, "price", h.number(h.parse_float(&price)?))?;
        h.define(&fill, "amount", h.number(amount))?;
        h.emit_fill(fill)?;
        h.record_stats("fills")?;
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(_) => h.emit_error(h.string("Error handling fill update")),
    }
}

/// ts: GrvtPrivateWebSocket.onMessage.
pub fn on_message<H: PrivateFeedHost>(host: Rc<H>, event: H::Value) -> JsResult<H::Value, ()> {
    let h = host.as_ref();
    let data = h.property(&event, "data")?;
    if h.state().is_disposed.get() || h.state().authentication_failed.get() {
        return Ok(());
    }
    h.track_received()?;
    if h.equal(&data, &h.string("pong")) {
        return h.pong();
    }
    if h.equal(&data, &h.string("ping")) {
        return Ok(());
    }
    let result: JsResult<H::Value, ()> = (|| {
        let json = h.json_parse(&data)?;
        if !h.truthy(&json) {
            return Ok(());
        }
        if h.equal(&h.property(&json, "type")?, &h.string("auth_success")) {
            h.state().authenticated.set(true);
            h.log(h.string("GRVT proxy WebSocket authenticated"), None)?;
            h.subscribe()?;
            return Ok(());
        }
        if h.equal(&h.property(&json, "type")?, &h.string("error")) {
            return h.refresh_session("proxy authentication rejected");
        }
        if h.truthy(&h.property(&json, "result")?) && h.truthy(&h.property(&json, "id")?) {
            h.log(
                js_template!(h;
                    h.string("Private WS subscription confirmed (requestId="),
                    h.property(&json, "id")?,
                    h.string(")"),
                )?,
                None,
            )?;
            return Ok(());
        }
        if h.truthy(&h.property(&json, "error")?) {
            let error = h.property(&json, "error")?;
            let code = h.property(&error, "code")?;
            let code = if h.nullish(&code) {
                h.property(&h.property(&json, "error")?, "c")?
            } else {
                code
            };
            if h.loose_number_equal(&code, 1000.0)? {
                return h.refresh_session("GRVT error 1000");
            }
            let code = if h.is_number(&code) {
                code
            } else {
                h.string("unknown")
            };
            h.emit_error(js_template!(h;
                h.string("Private WS error (code="),
                code,
                h.string(")"),
            )?)?;
            return Ok(());
        }
        if h.truthy(&h.property(&json, "code")?) && h.truthy(&h.property(&json, "status")?) {
            if h.loose_number_equal(&h.property(&json, "code")?, 1000.0)? {
                return h.refresh_session("GRVT error 1000");
            }
            if h.equal(&h.property(&json, "code")?, &h.number(1003.0)) {
                return Ok(());
            }
            if h.equal(&h.property(&json, "code")?, &h.number(1008.0)) {
                h.emit_error(h.string("GRVT IP is not whitelisted. Please re-add your API key with correct IP whitelisting (18.176.98.246)."))?;
            } else {
                h.emit_error(js_template!(h;
                    h.string("GRVT error (code="),
                    h.property(&json, "code")?,
                    h.string(")"),
                )?)?;
            }
            return Ok(());
        }
        let stream = or(h, &json, "stream", "s")?;
        if h.truthy(&stream) {
            if h.state().authenticated.get() && h.state().private_stream_healthy_at.get().is_none()
            {
                h.state().private_stream_healthy_at.set(Some(h.now()?));
            }
            if ["v1.order", "v1.state", "v1.cancel"]
                .iter()
                .any(|s| h.equal(&stream, &h.string(s)))
            {
                handle_order_update(host.clone(), json)?;
            } else if h.equal(&stream, &h.string("v1.position")) {
                handle_position_update(h, json)?;
            } else if h.equal(&stream, &h.string("v1.fill")) {
                handle_fill_update(h, json)?;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(_) => h.emit_error(h.string("Error parsing private WS message")),
    }
}
