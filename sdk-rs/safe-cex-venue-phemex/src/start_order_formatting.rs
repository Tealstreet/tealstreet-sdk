//! Startup, construction and order formatting retain original host carriers.
use crate::websocket_data::WsHost;
use safe_cex_num::{math_max, math_min};
use std::{
    cell::RefCell,
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::{Poll, Waker},
};

pub trait OrderHost: WsHost {
    fn async_call(
        &mut self,
        member: &'static str,
        args: Vec<Self::V>,
    ) -> Result<Pin<Box<dyn Future<Output = Result<Self::V, Self::E>>>>, Self::E>;
    fn error(&mut self, message: Self::V) -> Result<Self::E, Self::E>;
    fn has(&mut self, value: &Self::V, key: &'static str) -> Result<bool, Self::E>;
    fn method_text(&mut self, value: Self::V) -> Result<Vec<u16>, Self::E>;
    fn table(
        &mut self,
        name: &'static str,
        key: Self::V,
        inverse: bool,
    ) -> Result<Self::V, Self::E>;
    fn times_count(&mut self, count: f64) -> Result<usize, Self::E>;
    fn base_construct(
        &mut self,
        name: Self::V,
        opts: Self::V,
        store: Self::V,
    ) -> Result<(), Self::E>;
    fn initialize(&mut self) -> Result<ExchangeState<Self::V>, Self::E>;
    fn assign(&mut self, field: &'static str, value: Self::V) -> Result<(), Self::E>;
    fn secondary_xhr(&mut self, opts: Self::V, max_rps: f64) -> Result<Self::V, Self::E>;
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn get<H: WsHost>(h: &mut H, v: &H::V, key: &str) -> Result<H::V, H::E> {
    h.get(v, &u(key))
}
fn opt<H: WsHost>(h: &mut H, v: &H::V, key: &str) -> Result<H::V, H::E> {
    h.optional(v, &u(key))
}
fn is<H: WsHost>(h: &mut H, v: &H::V, text: &str) -> bool {
    let text = h.string(u(text));
    h.same(v, &text)
}
fn field_is<H: WsHost>(h: &mut H, v: &H::V, key: &str, text: &str) -> Result<bool, H::E> {
    let v = get(h, v, key)?;
    Ok(is(h, &v, text))
}
fn template<H: WsHost>(h: &mut H, v: H::V) -> Result<H::V, H::E> {
    let text = h.text(v)?;
    Ok(h.string(text))
}
fn uuid<H: WsHost>(h: &mut H) -> Result<H::V, H::E> {
    let id = h.call("uuid", vec![])?;
    let text = u("Tealstreet_").into_iter().chain(h.text(id)?).collect();
    Ok(h.string(text))
}
fn fail<H: OrderHost>(h: &mut H, message: Vec<u16>) -> Result<H::V, H::E> {
    let message = h.string(message);
    h.call("emit", vec![message.clone()])?;
    Err(h.error(message)?)
}
fn round<H: WsHost>(h: &mut H, v: H::V, step: H::V) -> Result<H::V, H::E> {
    h.call("adjust", vec![v, step])
}

pub struct ExchangeState<V> {
    pub client_order_id_rule: V,
    pub xhr: Option<V>,
    pub unlimited_xhr: Option<V>,
    pub public_websocket: Option<V>,
    pub private_websocket: Option<V>,
    pub grafana_endpoints_periodic: V,
    pub grafana_endpoints_every: V,
    pub timeframes: V,
}
impl<V> ExchangeState<V> {
    pub fn initialize<H: WsHost<V = V>>(h: &mut H) -> Result<Self, H::E> {
        let rule = h.call("clientRule", vec![])?;
        let periodic = h.call("grafanaPeriodic", vec![])?;
        let every = h.call("grafanaEvery", vec![])?;
        let mut frames = vec![];
        for (key, value) in [
            ("1m", "60"),
            ("3m", "180"),
            ("5m", "300"),
            ("15m", "900"),
            ("30m", "1800"),
            ("1h", "3600"),
            ("2h", "7200"),
            ("3h", "10800"),
            ("4h", "14400"),
            ("6h", "21600"),
            ("12h", "43200"),
            ("1d", "86400"),
            ("1w", "604800"),
            ("1M", "2592000"),
            ("3M", "7776000"),
            ("1Y", "31104000"),
        ] {
            frames.push((key, h.string(u(value))));
        }
        Ok(Self {
            client_order_id_rule: rule,
            xhr: None,
            unlimited_xhr: None,
            public_websocket: None,
            private_websocket: None,
            grafana_endpoints_periodic: periodic,
            grafana_endpoints_every: every,
            timeframes: h.object(frames),
        })
    }
}
pub fn new_exchange<H: OrderHost>(
    h: &mut H,
    name: H::V,
    opts: H::V,
    store: H::V,
) -> Result<ExchangeState<H::V>, H::E> {
    h.base_construct(name, opts.clone(), store)?;
    let mut state = h.initialize()?;
    state.xhr = Some(h.secondary_xhr(opts.clone(), 3.0)?);
    h.assign("xhr", state.xhr.as_ref().unwrap().clone())?;
    state.unlimited_xhr = Some(h.call("createAPI", vec![opts])?);
    h.assign(
        "unlimitedXHR",
        state.unlimited_xhr.as_ref().unwrap().clone(),
    )?;
    state.public_websocket = Some(h.call("publicSocket", vec![])?);
    h.assign(
        "publicWebsocket",
        state.public_websocket.as_ref().unwrap().clone(),
    )?;
    state.private_websocket = Some(h.call("privateSocket", vec![])?);
    h.assign(
        "privateWebsocket",
        state.private_websocket.as_ref().unwrap().clone(),
    )?;
    Ok(state)
}
struct Join<V, E> {
    left: Option<V>,
    right: Option<V>,
    error: Option<E>,
    waker: Option<Waker>,
}
fn settle<V, E>(state: &Rc<RefCell<Join<V, E>>>, value: Result<V, E>, left: bool) {
    let mut state = state.borrow_mut();
    match value {
        Ok(v) => {
            if left {
                state.left = Some(v)
            } else {
                state.right = Some(v)
            }
        }
        Err(e) => {
            if state.error.is_none() {
                state.error = Some(e)
            }
        }
    }
    if let Some(waker) = state.waker.take() {
        waker.wake()
    }
}
pub async fn start<H: OrderHost>(h: &mut H) -> Result<(), H::E> {
    let external = h.async_call("initExternal", vec![])?.await?;
    let public = h.field("publicOnly")?;
    let public_only = h.truthy(&public);
    if !h.truthy(&external) {
        let markets = h.async_call("fetchMarkets", vec![])?;
        let raw = h.async_call("fetchTickersRaw", vec![]);
        let state = Rc::new(RefCell::new(Join {
            left: None,
            right: None,
            error: None,
            waker: None,
        }));
        let target = state.clone();
        let mut host = h.clone();
        h.spawn(Box::pin(async move {
            let result = async {
                let markets = markets.await?;
                let disposed = host.field("disposed")?;
                if host.truthy(&disposed) {
                    return Ok(host.array(vec![]));
                }
                let loaded = host.store("loaded")?;
                let loaded = host.spread(&loaded)?;
                let yes = host.boolean(true);
                host.set(&loaded, &u("markets"), yes)?;
                let update = host.object(vec![("markets", markets.clone()), ("loaded", loaded)]);
                host.call("update", vec![update])?;
                host.call("connectPublic", vec![])?;
                Ok(markets)
            }
            .await;
            settle(&target, result, true);
            Ok(())
        }))?;
        let raw = raw?;
        let target = state.clone();
        h.spawn(Box::pin(async move {
            settle(&target, raw.await, false);
            Ok(())
        }))?;
        let raw = poll_fn(|cx| {
            let mut state = state.borrow_mut();
            if let Some(error) = state.error.take() {
                return Poll::Ready(Err(error));
            }
            if state.left.is_some() && state.right.is_some() {
                return Poll::Ready(Ok(state.right.take().unwrap()));
            }
            state.waker = Some(cx.waker().clone());
            Poll::Pending
        })
        .await?;
        let disposed = h.field("disposed")?;
        if h.truthy(&disposed) {
            return Ok(());
        }
        let tickers = h.call("parseTickers", vec![raw])?;
        let loaded = h.store("loaded")?;
        let loaded = h.spread(&loaded)?;
        let yes = h.boolean(true);
        h.set(&loaded, &u("tickers"), yes)?;
        let update = h.object(vec![("tickers", tickers), ("loaded", loaded)]);
        h.call("update", vec![update])?;
    }
    h.async_call(
        if public_only {
            "startPublicOnly"
        } else {
            "startPrivate"
        },
        vec![],
    )?
    .await?;
    Ok(())
}
pub async fn update_order<H: OrderHost>(h: &mut H, input: H::V) -> Result<H::V, H::E> {
    let order = get(h, &input, "order")?;
    let update = get(h, &input, "update")?;
    let price = if h.has(&update, "price")? {
        get(h, &update, "price")?
    } else {
        h.undefined()
    };
    let amount = if h.has(&update, "amount")? {
        get(h, &update, "amount")?
    } else {
        h.undefined()
    };
    let new_order = h.spread(&order)?;
    for field in ["symbol", "type", "side"] {
        let v = get(h, &order, field)?;
        h.set(&new_order, &u(field), v)?;
    }
    for (field, value) in [("amount", amount), ("price", price.clone())] {
        let v = if h.nullish(&value) {
            get(h, &order, field)?
        } else {
            value
        };
        h.set(&new_order, &u(field), v)?;
    }
    let reduce = get(h, &order, "reduceOnly")?;
    let reduce = if h.truthy(&reduce) {
        reduce
    } else {
        h.boolean(false)
    };
    h.set(&new_order, &u("reduceOnly"), reduce)?;
    let id = get(h, &order, "id")?;
    let id = h.method_text(id)?;
    let end = id
        .windows(2)
        .position(|v| v == [95, 95])
        .unwrap_or(id.len());
    let real = &id[..end];
    let stored = h.store("orders")?;
    let mut relative = vec![];
    for o in h.rows(stored)? {
        let oid = get(h, &o, "id")?;
        let oid = h.method_text(oid)?;
        if oid.starts_with(real) {
            relative.push(o)
        }
    }
    if relative.len() > 1 {
        let mut parent = None;
        let mut sl = None;
        let mut tp = None;
        for o in &relative {
            let oid = get(h, o, "id")?;
            let oid = h.method_text(oid)?;
            if !oid.windows(2).any(|v| v == [95, 95]) {
                parent = Some(o.clone());
                break;
            }
        }
        for o in &relative {
            let oid = get(h, o, "id")?;
            let oid = h.method_text(oid)?;
            if oid.ends_with(&u("__stop_loss")) {
                sl = Some(o.clone());
                break;
            }
        }
        for o in &relative {
            let oid = get(h, o, "id")?;
            let oid = h.method_text(oid)?;
            if oid.ends_with(&u("__take_profit")) {
                tp = Some(o.clone());
                break;
            }
        }
        if let Some(parent) = parent {
            for field in ["type", "side", "amount", "price", "reduceOnly"] {
                let v = get(h, &parent, field)?;
                h.set(&new_order, &u(field), v)?;
            }
        }
        for (value, fields) in [
            (
                sl,
                [("price", "stopLoss"), ("slTriggerType", "slTriggerType")],
            ),
            (
                tp,
                [("price", "takeProfit"), ("tpTriggerType", "tpTriggerType")],
            ),
        ] {
            if let Some(value) = value {
                for (from, to) in fields {
                    let v = opt(h, &value, from)?;
                    if h.truthy(&v) {
                        let v = get(h, &value, from)?;
                        h.set(&new_order, &u(to), v)?;
                    }
                }
            }
        }
        let undefined = h.undefined();
        if !h.same(&price, &undefined) {
            let id = get(h, &order, "id")?;
            let id = h.method_text(id)?;
            let field = if id.ends_with(&u("__stop_loss")) {
                "stopLoss"
            } else if id.ends_with(&u("__take_profit")) {
                "takeProfit"
            } else {
                "price"
            };
            h.set(&new_order, &u(field), price)?;
        }
    }
    let relative = h.array(relative);
    h.async_call("cancelOrders", vec![relative])?.await?;
    h.async_call("placeOrder", vec![new_order])?.await
}
pub async fn place_order<H: OrderHost>(h: &mut H, opts: H::V) -> Result<H::V, H::E> {
    h.call("resolveCaller", vec![opts.clone()])?;
    let payloads = h.async_call("formatCreate", vec![opts])?.await?;
    let mut ids = vec![];
    let mut open = vec![];
    let mut message = h.string(vec![]);
    for payload in h.rows(payloads)? {
        let endpoint = h.string(u(crate::constants::ENDPOINTS[5].1));
        let response = h.async_call("put", vec![endpoint, payload])?.await?;
        let data = get(h, &response, "data")?;
        let code = get(h, &data, "code")?;
        let zero = h.num(0.0);
        if !h.same(&code, &zero) {
            let msg = get(h, &data, "msg")?;
            h.call("emit", vec![msg])?;
            message = get(h, &data, "msg")?;
        } else {
            let model = get(h, &data, "data")?;
            let status = opt(h, &model, "ordStatus")?;
            let suspended = if is(h, &status, "Suspended") {
                let model = get(h, &data, "data")?;
                let status = opt(h, &model, "execStatus")?;
                is(h, &status, "PendingAllocate")
            } else {
                false
            };
            if suspended {
                message = h.string(u("Order is suspended. Likely insufficient margin."));
            } else {
                let model = get(h, &data, "data")?;
                ids.push(get(h, &model, "orderID")?);
                let model = get(h, &data, "data")?;
                let mapped = h.call("mapOrder", vec![model])?;
                for order in h.rows(mapped)? {
                    if field_is(h, &order, "status", "open")? {
                        open.push(order)
                    }
                }
            }
        }
    }
    if !open.is_empty() {
        let open = h.array(open);
        h.call("upsertOrders", vec![open])?;
    }
    if h.truthy(&message) {
        h.call("emit", vec![message.clone()])?;
        return Err(h.error(message)?);
    }
    Ok(h.array(ids))
}
pub async fn format_create_order<H: OrderHost>(h: &mut H, opts: H::V) -> Result<H::V, H::E> {
    if field_is(h, &opts, "type", "trailing_stop_market")? {
        return h.async_call("formatTrailing", vec![opts])?.await;
    }
    if field_is(h, &opts, "type", "conditional")? {
        return fail(
            h,
            u(
                "Phemex opening trigger orders are disabled until stopPxRp and triggerType are implemented and verified.",
            ),
        );
    }
    let symbol = get(h, &opts, "symbol")?;
    let market = h.async_call("market", vec![symbol])?.await?;
    if !h.truthy(&market) {
        let symbol = get(h, &opts, "symbol")?;
        let mut message = u("Market ");
        message.extend(h.text(symbol)?);
        message.extend(u(" not found"));
        return fail(h, message);
    }
    let stop = field_is(h, &opts, "type", "stop_market")?
        || field_is(h, &opts, "type", "take_profit_market")?;
    let side = h.async_call("positionSide", vec![opts.clone()])?.await?;
    let limits = get(h, &market, "limits")?;
    let limits = get(h, &limits, "amount")?;
    let max = get(h, &limits, "max")?;
    let precision = get(h, &market, "precision")?;
    let pprice = get(h, &precision, "price")?;
    let precision = get(h, &market, "precision")?;
    let pamount = get(h, &precision, "amount")?;
    let amount = get(h, &opts, "amount")?;
    let amount = round(h, amount, pamount.clone())?;
    let raw = get(h, &opts, "price")?;
    let price = if h.truthy(&raw) && !field_is(h, &opts, "type", "market")? {
        let raw = get(h, &opts, "price")?;
        round(h, raw, pprice.clone())?
    } else {
        h.undefined()
    };
    let raw = get(h, &opts, "stopLoss")?;
    let sl = if h.truthy(&raw) {
        let raw = get(h, &opts, "stopLoss")?;
        round(h, raw, pprice.clone())?
    } else {
        h.call("null", vec![])?
    };
    let raw = get(h, &opts, "takeProfit")?;
    let tp = if h.truthy(&raw) {
        let raw = get(h, &opts, "takeProfit")?;
        round(h, raw, pprice.clone())?
    } else {
        h.call("null", vec![])?
    };
    let symbol = get(h, &opts, "symbol")?;
    let positions = h.async_call("positions", vec![symbol])?.await?;
    let position = h.at(&positions, 0)?;
    let mode = opt(h, &position, "positionMode")?;
    let reduce = if is(h, &mode, "oneway") {
        get(h, &opts, "reduceOnly")?
    } else {
        h.boolean(false)
    };
    let time = get(h, &opts, "timeInForce")?;
    let time = if h.truthy(&time) {
        get(h, &opts, "timeInForce")?
    } else {
        h.string(u("GoodTillCancel"))
    };
    let time = h.table("timeInForce", time, true)?;
    let id = uuid(h)?;
    let symbol = get(h, &opts, "symbol")?;
    let symbol = h.call("formatOut", vec![symbol])?;
    let raw = get(h, &opts, "side")?;
    let raw = h.table("side", raw, true)?;
    let raw = h.call("capitalize", vec![raw])?;
    let type_ = get(h, &opts, "type")?;
    let type_ = h.table("type", type_, false)?;
    let mut fields = vec![
        ("clOrdID", id),
        ("symbol", symbol),
        ("posSide", side),
        ("side", raw),
        ("ordType", type_),
    ];
    let qty = if h.truthy(&amount) {
        template(h, amount.clone())?
    } else {
        h.undefined()
    };
    fields.push(("orderQtyRq", qty));
    let value = if h.truthy(&price) {
        template(h, price.clone())?
    } else {
        h.undefined()
    };
    fields.push((if stop { "stopPxRp" } else { "priceRp" }, value));
    let value = if h.truthy(&price) {
        template(h, price)?
    } else {
        h.undefined()
    };
    fields.push(("priceRp", value));
    let limit = field_is(h, &opts, "type", "limit")?;
    let v = if limit { time } else { h.undefined() };
    fields.push(("timeInForce", v));
    let v = if stop {
        h.string(u("true"))
    } else {
        h.undefined()
    };
    fields.push(("closeOnTrigger", v));
    let v = if h.truthy(&reduce) && !stop {
        h.string(u("true"))
    } else {
        h.undefined()
    };
    fields.push(("reduceOnly", v));
    for (field, out, value) in [
        ("stopLoss", "stopLossRp", sl),
        ("takeProfit", "takeProfitRp", tp),
    ] {
        let raw = get(h, &opts, field)?;
        let v = if h.truthy(&raw) {
            template(h, value)?
        } else {
            h.undefined()
        };
        fields.push((out, v));
    }
    for (field, trigger, out, kind, default) in [
        (
            "takeProfit",
            "tpTriggerType",
            "tpTrigger",
            "mark_price",
            "ByLastPrice",
        ),
        (
            "stopLoss",
            "slTriggerType",
            "slTrigger",
            "last_price",
            "ByMarkPrice",
        ),
    ] {
        let raw = get(h, &opts, field)?;
        if h.truthy(&raw) {
            let v = get(h, &opts, trigger)?;
            let text = if is(h, &v, kind) {
                if kind == "mark_price" {
                    "ByMarkPrice"
                } else {
                    "ByLastPrice"
                }
            } else {
                default
            };
            fields.push((out, h.string(u(text))));
        }
    }
    for (type_, trigger, kind, yes, no) in [
        (
            "take_profit_market",
            "tpTriggerType",
            "mark_price",
            "ByMarkPrice",
            "ByLastPrice",
        ),
        (
            "stop_market",
            "slTriggerType",
            "last_price",
            "ByLastPrice",
            "ByMarkPrice",
        ),
    ] {
        if field_is(h, &opts, "type", type_)? {
            let v = get(h, &opts, trigger)?;
            let match_ = is(h, &v, kind);
            fields.push(("triggerType", h.string(u(if match_ { yes } else { no }))));
        }
    }
    let request = h.object(fields);
    let request = h.call("omitUndefined", vec![request])?;
    let an = h.number(amount)?;
    let max = h.number(max)?;
    let large = an > max;
    let lots = if large { (an / max).ceil() } else { 1.0 };
    let rest = if large {
        let raw = h.num(an % max);
        let rounded = round(h, raw, pamount.clone())?;
        h.number(rounded)?
    } else {
        0.0
    };
    let raw = h.num((an - rest) / lots);
    let lot_size = round(h, raw, pamount)?;
    let count = h.times_count(lots)?;
    let mut payloads = vec![];
    for _ in 0..count {
        let payload = h.spread(&request)?;
        let value = if h.truthy(&lot_size) {
            template(h, lot_size.clone())?
        } else {
            h.undefined()
        };
        h.set(&payload, &u("orderQtyRq"), value)?;
        payloads.push(payload);
    }
    if rest != 0.0 && !rest.is_nan() {
        let payload = h.spread(&request)?;
        let value = h.num(rest);
        let value = template(h, value)?;
        h.set(&payload, &u("orderQtyRq"), value)?;
        payloads.push(payload);
    }
    let client = h.call("resolveCaller", vec![opts])?;
    let count = h.num(payloads.len() as f64);
    h.call("assertSingle", vec![client.clone(), count])?;
    for payload in &payloads {
        let value = if h.nullish(&client) {
            uuid(h)?
        } else {
            client.clone()
        };
        h.set(payload, &u("clOrdID"), value)?;
    }
    Ok(h.array(payloads))
}
pub async fn format_create_trailing_stop_loss_order<H: OrderHost>(
    h: &mut H,
    opts: H::V,
) -> Result<H::V, H::E> {
    let markets = h.store("marketsBySymbol")?;
    let symbol = get(h, &opts, "symbol")?;
    let key = h.text(symbol)?;
    let market = h.get(&markets, &key)?;
    let tickers = h.store("tickersBySymbol")?;
    let symbol = get(h, &opts, "symbol")?;
    let key = h.text(symbol)?;
    let ticker = h.get(&tickers, &key)?;
    let side = get(h, &opts, "side")?;
    let buy = is(h, &side, "buy");
    let pside = h.string(u(if buy { "short" } else { "long" }));
    let positions = h.store("positions")?;
    let mut position = None;
    for p in h.rows(positions)? {
        let a = get(h, &p, "symbol")?;
        let b = get(h, &opts, "symbol")?;
        if h.same(&a, &b) {
            let side = get(h, &p, "side")?;
            if h.same(&side, &pside) {
                position = Some(p);
                break;
            }
        }
    }
    for (value, label) in [(&market, "Market "), (&ticker, "Ticker ")] {
        if !h.truthy(value) {
            let symbol = get(h, &opts, "symbol")?;
            let mut msg = u(label);
            msg.extend(h.text(symbol)?);
            msg.extend(u(" not found"));
            return fail(h, msg);
        }
    }
    let Some(position) = position else {
        let mut msg = u("No ");
        msg.extend(h.text(pside)?);
        msg.extend(u(" position open for "));
        let symbol = get(h, &opts, "symbol")?;
        msg.extend(h.text(symbol)?);
        msg.extend(u(" — nothing to attach a trailing stop to"));
        return fail(h, msg);
    };
    let last = get(h, &ticker, "last")?;
    let last = h.number(last)?;
    let price = get(h, &opts, "price")?;
    let price = h.number(price)?;
    let high = math_max(last, price);
    let last = get(h, &ticker, "last")?;
    let last = h.number(last)?;
    let price = get(h, &opts, "price")?;
    let price = h.number(price)?;
    let low = math_min(last, price);
    let precision = get(h, &market, "precision")?;
    let precision = get(h, &precision, "price")?;
    let diff = h.num(high - low);
    let distance = round(h, diff, precision)?;
    let symbol = get(h, &opts, "symbol")?;
    let symbol = h.call("formatOut", vec![symbol])?;
    let side = h.async_call("positionSide", vec![opts.clone()])?.await?;
    let raw = get(h, &opts, "side")?;
    let raw = h.table("side", raw, true)?;
    let raw = h.call("capitalize", vec![raw])?;
    let contracts = get(h, &position, "contracts")?;
    let qty = template(h, contracts)?;
    let type_ = h.string(u("Stop"));
    let side_ = get(h, &opts, "side")?;
    let distance = if is(h, &side_, "sell") {
        let n = h.number(distance)?;
        h.num(-n)
    } else {
        distance
    };
    let distance = template(h, distance)?;
    let peg = h.string(u("TrailingStopPeg"));
    let id = uuid(h)?;
    let payload = h.object(vec![
        ("symbol", symbol),
        ("posSide", side),
        ("side", raw),
        ("orderQtyRq", qty),
        ("ordType", type_),
        ("pegOffsetValueRp", distance),
        ("pegPriceType", peg),
        ("clOrdID", id),
    ]);
    Ok(h.array(vec![payload]))
}

#[cfg(test)]
mod accepted_native_contract {
    use super::*;

    #[test]
    fn utf16_encoder_preserves_native_units() {
        assert_eq!(u(""), Vec::<u16>::new());
        assert_eq!(u("A🚀\0"), vec![65, 55357, 56960, 0]);
        assert_eq!(u("é雪"), vec![233, 38634]);
    }
}
