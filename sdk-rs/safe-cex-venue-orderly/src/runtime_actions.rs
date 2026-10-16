//! Startup, polling and order execution over source runtime bindings.
use crate::{
    data_mappers::{DataHost, Op},
    ws_controls::{Result, Value},
};
use std::future::Future;
pub trait RuntimeHost: DataHost + Clone + 'static {
    fn strict_eq(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn same(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn is_number(&self, v: &Self::Value) -> bool;
    fn is_object(&self, v: &Self::Value) -> bool;
    fn own(&self, v: Value<Self>, key: &str) -> Result<Self, bool>;
    fn keys(&self, v: Value<Self>) -> Result<Self, Vec<Vec<u16>>>;
    fn find(
        &self,
        v: Value<Self>,
        f: impl FnMut(Value<Self>) -> Result<Self, bool>,
    ) -> Result<Self, Value<Self>>;
    fn adjust(&self, value: Value<Self>, step: Value<Self>) -> Result<Self, Value<Self>>;
    fn inverse_type(&self, v: Value<Self>) -> Result<Self, Value<Self>>;
    fn inverse_side(&self, v: Value<Self>) -> Result<Self, Value<Self>>;
    fn omit_undefined(&self, v: Value<Self>) -> Result<Self, Value<Self>>;
    fn ceil(&self, v: Value<Self>) -> Result<Self, Value<Self>>;
    fn remainder(&self, a: Value<Self>, b: Value<Self>) -> Result<Self, Value<Self>>;
    fn add(&self, a: Value<Self>, b: Value<Self>) -> Result<Self, Value<Self>>;
    fn times(
        &self,
        n: Value<Self>,
        f: impl FnMut() -> Result<Self, Value<Self>>,
    ) -> Result<Self, Value<Self>>;
    fn map_series<F: Future<Output = Result<Self, Value<Self>>>>(
        &self,
        rows: Value<Self>,
        f: impl FnMut(Value<Self>) -> F,
    ) -> impl Future<Output = Result<Self, Value<Self>>>;
    fn sum_by(&self, rows: Value<Self>, key: &str) -> Result<Self, Value<Self>>;
    fn new_error(&self, message: Value<Self>) -> Self::Error;
    fn error_log_options(&self, message: Value<Self>) -> Result<Self, Value<Self>>;
    fn attach_catch(&self, promise: Value<Self>, callback: LeverageCatch<Self>)
    -> Result<Self, ()>;
    // The imported async loop is detached; its failures are observed out of band.
    fn schedule(&self, callback: TickCallback<Self>, interval: Value<Self>);
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text<H: RuntimeHost>(h: &H, s: &str) -> Value<H> {
    h.text(&u(s))
}
fn get<H: RuntimeHost>(h: &H, v: &Value<H>, k: &str) -> Result<H, Value<H>> {
    h.get(v, &u(k))
}
fn optional<H: RuntimeHost>(h: &H, v: &Value<H>, k: &str) -> Result<H, Value<H>> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, k)
    }
}
fn call<H: RuntimeHost>(
    h: &H,
    r: Value<H>,
    name: &str,
    args: Vec<Value<H>>,
) -> Result<H, Value<H>> {
    let f = get(h, &r, name)?;
    h.call(f, Some(r), args)
}
async fn awaited<H: RuntimeHost>(
    h: &H,
    r: Value<H>,
    name: &str,
    args: Vec<Value<H>>,
) -> Result<H, Value<H>> {
    let p = call(h, r, name, args)?;
    h.await_value(p).await
}
fn endpoint<H: RuntimeHost>(h: &H, name: &str) -> Value<H> {
    text(
        h,
        crate::types::ENDPOINTS
            .iter()
            .find(|(k, _)| *k == name)
            .expect("endpoint")
            .1,
    )
}
fn choose<H: RuntimeHost>(
    h: &H,
    v: Value<H>,
    f: impl FnOnce() -> Result<H, Value<H>>,
) -> Result<H, Value<H>> {
    if h.truthy(&v) { Ok(v) } else { f() }
}
fn eq<H: RuntimeHost>(h: &H, a: Value<H>, s: &str) -> bool {
    h.strict_eq(&a, &text(h, s))
}
fn stringify<H: RuntimeHost>(h: &H, v: Value<H>) -> Result<H, Value<H>> {
    Ok(h.text(&h.string(&v)?))
}
fn message<H: RuntimeHost>(h: &H, e: &H::Error, key: &str) -> Result<H, Value<H>> {
    let v = h.error_value(e);
    let response = optional(h, &v, "response")?;
    let data = optional(h, &response, "data")?;
    choose(h, optional(h, &data, key)?, || optional(h, &v, "message"))
}
fn log<H: RuntimeHost>(h: &H, r: &Value<H>, e: &H::Error, fallback: Option<&str>) -> Result<H, ()> {
    let f = get(h, r, "emitAndLogError")?;
    let msg = message(h, e, "message")?;
    let msg = if let Some(s) = fallback {
        choose(h, msg, || Ok(text(h, s)))?
    } else {
        msg
    };
    h.call(f, Some(r.clone()), vec![msg])?;
    Ok(())
}
fn finite<H: RuntimeHost>(h: &H, v: &Value<H>) -> Result<H, bool> {
    Ok(h.is_number(v) && h.numeric(v.clone())?.is_finite())
}
fn length<H: RuntimeHost>(h: &H, v: Value<H>) -> Result<H, Value<H>> {
    get(h, &v, "length")
}
fn loaded<H: RuntimeHost>(
    h: &H,
    r: &Value<H>,
    fields: Vec<(&'static str, Value<H>)>,
) -> Result<H, Value<H>> {
    let current = get(h, &get(h, r, "store")?, "loaded")?;
    h.merge(current, fields)
}
fn update<H: RuntimeHost>(h: &H, store: Value<H>, f: Value<H>, patch: Value<H>) -> Result<H, ()> {
    h.call(f, Some(store), vec![patch])?;
    Ok(())
}
pub fn transient<H: RuntimeHost>(h: &H, error: Value<H>) -> Result<H, bool> {
    let status = optional(h, &optional(h, &error, "response")?, "status")?;
    if h.nullish(&status) {
        return Ok(true);
    }
    if h.strict_eq(&status, &h.number(408.0)) || h.strict_eq(&status, &h.number(429.0)) {
        return Ok(true);
    }
    Ok(h.numeric(status)? >= 500.0)
}
pub fn dispose<H: RuntimeHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    h.set(&r, &u("isDisposed"), h.boolean(true))?;
    call(h, get(h, &r, "publicWebsocket")?, "dispose", vec![])?;
    call(h, get(h, &r, "privateWebsocket")?, "dispose", vec![])?;
    Ok(h.undefined())
}
pub struct LeverageCatch<H: RuntimeHost> {
    host: H,
    receiver: Value<H>,
}
impl<H: RuntimeHost> LeverageCatch<H> {
    pub fn invoke(&self, error: H::Error) -> Result<H, ()> {
        log(
            &self.host,
            &self.receiver,
            &error,
            Some("Orderly account leverage fetch failed"),
        )
    }
}
pub struct TickCallback<H: RuntimeHost> {
    host: H,
    receiver: Value<H>,
    count: Value<H>,
}
impl<H: RuntimeHost> TickCallback<H> {
    pub fn invoke(&self) -> Result<H, Value<H>> {
        let h = &self.host;
        let f = get(h, &self.receiver, "tick")?;
        let count = h.add(self.count.clone(), h.number(1.0))?;
        h.call(f, Some(self.receiver.clone()), vec![count])
    }
}
pub async fn start<H: RuntimeHost>(h: &H, r: Value<H>) -> Result<H, Value<H>> {
    let public = get(h, &r, "isPublicDataOnly")?;
    if !h.truthy(&public) {
        let result = async {
            let xhr = get(h, &r, "xhr")?;
            let f = get(h, &xhr, "get")?;
            let p = h.call(f, Some(xhr), vec![endpoint(h, "ACCOUNT")])?;
            h.await_value(p).await
        }
        .await;
        if let Err(e) = result {
            log(h, &r, &e, None)?;
            if transient(h, h.error_value(&e))? {
                let msg = choose(h, message(h, &e, "msg")?, || Ok(text(h, "unknown error")))?;
                return Err(h.new_error(h.text(&[u("Orderly account validation failed transiently, will retry on next reconnect: "),h.string(&msg)?].concat())));
            }
            call(
                h,
                r.clone(),
                "emitAndLogError",
                vec![text(h, "API key expired")],
            )?;
            call(
                h,
                get(h, &r, "emitter")?,
                "emit",
                vec![text(h, "deactivate")],
            )?;
            return Ok(h.undefined());
        }
    }
    let markets = awaited(h, r.clone(), "fetchMarkets", vec![]).await?;
    if h.truthy(&*get(h, &r, "isDisposed")?) {
        return Ok(h.undefined());
    }
    let store = get(h, &r, "store")?;
    let f = get(h, &store, "update")?;
    let patch = h.object(vec![
        ("markets", markets.clone()),
        ("loaded", loaded(h, &r, vec![("markets", h.boolean(true))])?),
    ])?;
    update(h, store, f, patch)?;
    let tickers = awaited(h, r.clone(), "fetchTickers", vec![]).await?;
    if h.truthy(&*get(h, &r, "isDisposed")?) {
        return Ok(h.undefined());
    }
    if !h.truthy(&public) {
        if let Err(e) = awaited(h, r.clone(), "fetchAndUpdateMarginTypes", vec![]).await {
            log(h, &r, &e, Some("Orderly margin-mode fetch failed"))?
        }
        let p = call(h, r.clone(), "fetchAndUpdateAccountLeverage", vec![])?;
        h.attach_catch(
            p,
            LeverageCatch {
                host: h.clone(),
                receiver: r.clone(),
            },
        )?;
    }
    let f = get(h, &r, "log")?;
    let n = h.arithmetic(Op::Min, length(h, tickers.clone())?, length(h, markets)?)?;
    let s = h.text(
        &[
            u("Loaded "),
            h.string(&n)?,
            u(" "),
            h.string(&*get(h, &r, "orderlyExchangeName")?)?,
            u(" markets"),
        ]
        .concat(),
    );
    h.call(f, Some(r.clone()), vec![s])?;
    let store = get(h, &r, "store")?;
    let f = get(h, &store, "update")?;
    let patch = h.object(vec![
        ("tickers", tickers),
        ("loaded", loaded(h, &r, vec![("tickers", h.boolean(true))])?),
    ])?;
    update(h, store, f, patch)?;
    call(
        h,
        get(h, &r, "publicWebsocket")?,
        "connectAndSubscribe",
        vec![],
    )?;
    if h.truthy(&public) {
        let store = get(h, &r, "store")?;
        let f = get(h, &store, "update")?;
        let patch = h.object(vec![(
            "loaded",
            loaded(
                h,
                &r,
                vec![
                    ("balance", h.boolean(true)),
                    ("positions", h.boolean(true)),
                    ("orders", h.boolean(true)),
                ],
            )?,
        )])?;
        update(h, store, f, patch)?;
        return Ok(h.undefined());
    }
    call(
        h,
        get(h, &r, "privateWebsocket")?,
        "connectAndSubscribe",
        vec![],
    )?;
    awaited(h, r.clone(), "tick", vec![]).await?;
    if h.truthy(&*get(h, &r, "isDisposed")?) {
        return Ok(h.undefined());
    }
    let f = get(h, &r, "log")?;
    let msg = h.text(
        &[
            u("Ready to trade on "),
            h.string(&*get(h, &r, "orderlyExchangeName")?)?,
        ]
        .concat(),
    );
    h.call(f, Some(r.clone()), vec![msg])?;
    let orders = awaited(h, r.clone(), "fetchOrders", vec![]).await?;
    if h.truthy(&*get(h, &r, "isDisposed")?) {
        return Ok(h.undefined());
    }
    let f = get(h, &r, "log")?;
    let msg = h.text(
        &[
            u("Loaded "),
            h.string(&*get(h, &r, "orderlyExchangeName")?)?,
            u(" orders"),
        ]
        .concat(),
    );
    h.call(f, Some(r.clone()), vec![msg])?;
    let store = get(h, &r, "store")?;
    let f = get(h, &store, "update")?;
    let patch = h.object(vec![
        ("orders", orders),
        ("loaded", loaded(h, &r, vec![("orders", h.boolean(true))])?),
    ])?;
    update(h, store, f, patch)?;
    Ok(h.undefined())
}
pub async fn tick<H: RuntimeHost>(h: &H, r: Value<H>, count: Value<H>) -> Result<H, Value<H>> {
    let count = if h.is_undefined(&count) {
        h.number(0.0)
    } else {
        count
    };
    if !h.truthy(&*get(h, &r, "isDisposed")?) {
        if h.truthy(&*get(h, &r, "isPublicDataOnly")?) {
            return Ok(h.undefined());
        }
        let work = async {
            let positions = awaited(h, r.clone(), "fetchPositions", vec![]).await?;
            if h.truthy(&*get(h, &r, "isDisposed")?) {
                return Ok(true);
            }
            let store = get(h, &r, "store")?;
            let f = get(h, &store, "update")?;
            let patch = h.object(vec![
                ("positions", positions.clone()),
                (
                    "loaded",
                    loaded(h, &r, vec![("positions", h.boolean(true))])?,
                ),
            ])?;
            update(h, store, f, patch)?;
            let upnl = if h.compare(length(h, positions.clone())?, h.number(0.0), false)? {
                let sum = h.sum_by(positions, "unrealizedPnl")?;
                h.arithmetic(
                    Op::Div,
                    h.round(h.arithmetic(Op::Mul, sum, h.number(100.0))?)?,
                    h.number(100.0),
                )?
            } else {
                h.number(0.0)
            };
            let store = get(h, &r, "store")?;
            let f = get(h, &store, "update")?;
            let balance = h.object(vec![
                (
                    "total",
                    h.arithmetic(
                        Op::Sub,
                        get(h, &get(h, &get(h, &r, "store")?, "balance")?, "total")?,
                        upnl.clone(),
                    )?,
                ),
                (
                    "used",
                    h.arithmetic(
                        Op::Sub,
                        get(h, &get(h, &get(h, &r, "store")?, "balance")?, "used")?,
                        upnl.clone(),
                    )?,
                ),
                (
                    "free",
                    get(h, &get(h, &get(h, &r, "store")?, "balance")?, "free")?,
                ),
                ("upnl", upnl),
            ])?;
            let patch = h.object(vec![
                ("balance", balance),
                ("loaded", loaded(h, &r, vec![("balance", h.boolean(true))])?),
            ])?;
            update(h, store, f, patch)?;
            if h.strict_eq(
                &*h.remainder(count.clone(), h.number(4.0))?,
                &*h.number(0.0),
            ) {
                let orders = awaited(h, r.clone(), "fetchOrders", vec![]).await?;
                if h.truthy(&*get(h, &r, "isDisposed")?) {
                    return Ok(true);
                }
                let store = get(h, &r, "store")?;
                let f = get(h, &store, "update")?;
                let patch = h.object(vec![
                    ("orders", orders),
                    ("loaded", loaded(h, &r, vec![("orders", h.boolean(true))])?),
                ])?;
                update(h, store, f, patch)?;
            }
            Ok(false)
        }
        .await;
        match work {
            Ok(true) => return Ok(h.undefined()),
            Ok(false) => {}
            Err(e) => {
                let f = get(h, &r, "emitAndLogError")?;
                let msg = optional(h, &h.error_value(&e), "message")?;
                h.call(f, Some(r.clone()), vec![msg])?;
            }
        }
        if !h.truthy(&*get(h, &r, "isDisposed")?) {
            let cb = TickCallback {
                host: h.clone(),
                receiver: r.clone(),
                count,
            };
            let interval = call(h, r, "getTickInterval", vec![])?;
            h.schedule(cb, interval);
        }
    }
    Ok(h.undefined())
}
fn fail<H: RuntimeHost>(h: &H, r: &Value<H>, reason: &str) -> Result<H, H::Error> {
    let err = h.new_error(text(h, &format!("Orderly native amend {reason}")));
    call(h, r.clone(), "emitAndLogError", vec![h.error_value(&err)])?;
    Ok(err)
}
pub async fn update_order<H: RuntimeHost>(
    h: &H,
    r: Value<H>,
    options: Value<H>,
) -> Result<H, Value<H>> {
    let order = get(h, &options, "order")?;
    let change = get(h, &options, "update")?;
    let f = get(h, &r, "isAlgoOrder")?;
    let kind = get(h, &order, "type")?;
    if h.truthy(&*h.call(f, Some(r.clone()), vec![kind])?) {
        let arg = h.object(vec![("order", order), ("update", change)])?;
        return awaited(h, r, "updateAlgoOrder", vec![arg]).await;
    }
    let keys = h.keys(change.clone())?;
    if keys.len() != 1 || (keys[0] != u("price") && keys[0] != u("amount")) {
        return Err(fail(
            h,
            &r,
            "requires exactly one price or quantity change",
        )?);
    }
    let rows = get(h, &get(h, &r, "store")?, "orders")?;
    let current = h.find(rows, |c| {
        Ok(h.strict_eq(&*get(h, &c, "id")?, &*get(h, &order, "id")?)
            && h.strict_eq(&*get(h, &c, "symbol")?, &*get(h, &order, "symbol")?))
    })?;
    if h.nullish(&current) {
        return Err(fail(h, &r, "requires an exactly reconciled store order")?);
    }
    for key in [
        "status",
        "symbol",
        "type",
        "side",
        "price",
        "amount",
        "filled",
        "remaining",
        "reduceOnly",
    ] {
        if !h.same(&*get(h, &current, key)?, &*get(h, &order, key)?) {
            return Err(fail(
                h,
                &r,
                "requires the submitted order to match the reconciled store row",
            )?);
        }
    }
    if !eq(h, get(h, &current, "status")?, "open")
        || !eq(h, get(h, &current, "type")?, "limit")
        || !h.nullish(&*get(h, &current, "parentId")?)
        || h.strict_eq(&*get(h, &current, "close")?, &*h.boolean(true))
        || !h.nullish(&*get(h, &current, "takeProfit")?)
        || !h.nullish(&*get(h, &current, "stopLoss")?)
        || {
            let tif = get(h, &current, "timeInForce")?;
            !h.nullish(&tif) && !eq(h, get(h, &current, "timeInForce")?, "GoodTillCancel")
        }
    {
        return Err(fail(h, &r, "supports only ordinary open GTC LIMIT orders")?);
    }
    let symbol = get(h, &current, "symbol")?;
    let market = h.get(
        &get(h, &get(h, &r, "store")?, "marketsBySymbol")?,
        &h.string(&symbol)?,
    )?;
    if !h.truthy(&market) {
        return Err(fail(h, &r, "requires reconciled market precision")?);
    }
    if !finite(h, &get(h, &get(h, &market, "precision")?, "price")?)?
        || h.numeric(get(h, &get(h, &market, "precision")?, "price")?)? <= 0.0
        || !finite(h, &get(h, &get(h, &market, "precision")?, "amount")?)?
        || h.numeric(get(h, &get(h, &market, "precision")?, "amount")?)? <= 0.0
    {
        return Err(fail(h, &r, "requires valid market precision")?);
    }
    let raw = get(h, &current, "info")?;
    if h.nullish(&raw) || !h.is_object(&raw) || h.is_array(&raw) {
        return Err(fail(h, &r, "requires a complete REST order row")?);
    }
    for key in [
        "order_id",
        "symbol",
        "type",
        "side",
        "status",
        "price",
        "quantity",
        "total_executed_quantity",
        "visible_quantity",
        "reduce_only",
        "margin_mode",
    ] {
        if !h.own(raw.clone(), key)? {
            return Err(fail(
                h,
                &r,
                "requires explicit REST identity, fill, visibility, reduce-only, and margin-mode provenance",
            )?);
        }
    }
    let raw_id = get(h, &raw, "order_id")?;
    let canonical = if h.is_number(&raw_id) {
        let n = h.numeric(raw_id.clone())?;
        if n.is_finite() && n.fract() == 0.0 && n > 0.0 && n <= 9007199254740991.0 {
            stringify(h, raw_id)?
        } else {
            h.null()
        }
    } else if h.is_string(&raw_id) {
        let s = h.string(&raw_id)?;
        if !s.is_empty()
            && (b'1' as u16..=b'9' as u16).contains(&s[0])
            && s.iter().all(|c| (b'0' as u16..=b'9' as u16).contains(c))
        {
            raw_id
        } else {
            h.null()
        }
    } else {
        h.null()
    };
    let side = get(h, &current, "side")?;
    let expected = if eq(h, side.clone(), "buy") {
        text(h, "BUY")
    } else if eq(h, side, "sell") {
        text(h, "SELL")
    } else {
        h.null()
    };
    if h.nullish(&canonical) {
        return Err(fail(
            h,
            &r,
            "requires exact immutable order identity parity",
        )?);
    }
    if !h.strict_eq(&canonical, &*get(h, &current, "id")?)
        || !h.strict_eq(&*get(h, &current, "id")?, &*get(h, &order, "id")?)
        || !h.strict_eq(
            &*get(h, &raw, "symbol")?,
            &*h.reverse_symbol(get(h, &current, "symbol")?)?,
        )
        || !eq(h, get(h, &raw, "type")?, "LIMIT")
        || !h.strict_eq(&*get(h, &raw, "side")?, &expected)
    {
        return Err(fail(
            h,
            &r,
            "requires exact immutable order identity parity",
        )?);
    }
    if !eq(h, get(h, &raw, "status")?, "NEW") {
        return Err(fail(h, &r, "requires an unfilled pending REST order")?);
    }
    let reduce = get(h, &raw, "reduce_only")?;
    if !h.strict_eq(&reduce, &*get(h, &current, "reduceOnly")?)
        || (!h.strict_eq(&reduce, &*h.boolean(true)) && !h.strict_eq(&reduce, &*h.boolean(false)))
    {
        return Err(fail(h, &r, "requires explicit reduce-only parity")?);
    }
    let margin = get(h, &raw, "margin_mode")?;
    if !eq(h, margin.clone(), "CROSS") && !eq(h, margin, "ISOLATED") {
        return Err(fail(
            h,
            &r,
            "requires proven CROSS or ISOLATED margin provenance",
        )?);
    }
    let mut executions = vec![get(h, &raw, "total_executed_quantity")?];
    for key in ["executed_quantity", "executed"] {
        if h.own(raw.clone(), key)? {
            executions.push(get(h, &raw, key)?)
        }
    }
    if executions
        .iter()
        .any(|v| !h.is_number(v) || !h.same(v, &h.number(0.0)))
        || !h.same(&*get(h, &current, "filled")?, &*h.number(0.0))
        || !h.strict_eq(
            &*get(h, &current, "remaining")?,
            &*get(h, &current, "amount")?,
        )
    {
        return Err(fail(h, &r, "requires exact zero-fill state")?);
    }
    if !h.is_number(&*get(h, &raw, "price")?)
        || !finite(h, &get(h, &raw, "price")?)?
        || h.numeric(get(h, &raw, "price")?)? <= 0.0
        || !h.is_number(&*get(h, &raw, "quantity")?)
        || !finite(h, &get(h, &raw, "quantity")?)?
        || h.numeric(get(h, &raw, "quantity")?)? <= 0.0
        || !h.is_number(&*get(h, &raw, "visible_quantity")?)
        || !finite(h, &get(h, &raw, "visible_quantity")?)?
        || !h.strict_eq(
            &*get(h, &raw, "visible_quantity")?,
            &*get(h, &raw, "quantity")?,
        )
        || !h.strict_eq(&*get(h, &raw, "price")?, &*get(h, &current, "price")?)
        || !h.strict_eq(&*get(h, &raw, "quantity")?, &*get(h, &current, "amount")?)
        || !h.strict_eq(
            &*h.adjust(
                get(h, &raw, "price")?,
                get(h, &get(h, &market, "precision")?, "price")?,
            )?,
            &*get(h, &raw, "price")?,
        )
        || !h.strict_eq(
            &*h.adjust(
                get(h, &raw, "quantity")?,
                get(h, &get(h, &market, "precision")?, "amount")?,
            )?,
            &*get(h, &raw, "quantity")?,
        )
    {
        return Err(fail(
            h,
            &r,
            "requires exact ordinary LIMIT price, quantity, visibility, and precision parity",
        )?);
    }
    if h.own(raw.clone(), "time_in_force")? && !eq(h, get(h, &raw, "time_in_force")?, "GTC") {
        return Err(fail(
            h,
            &r,
            "cannot preserve an unsupported time-in-force or trigger shape",
        )?);
    }
    for key in [
        "algo_order_id",
        "root_algo_order_id",
        "trigger_price",
        "algo_type",
    ] {
        if h.own(raw.clone(), key)? && !h.nullish(&*get(h, &raw, key)?) {
            return Err(fail(
                h,
                &r,
                "cannot preserve an unsupported time-in-force or trigger shape",
            )?);
        }
    }
    let price_change = keys[0] == u("price");
    let name = if price_change { "price" } else { "amount" };
    let requested = get(h, &change, name)?;
    let step = get(
        h,
        &get(h, &market, "precision")?,
        if price_change { "price" } else { "amount" },
    )?;
    if !finite(h, &requested)? || h.numeric(requested.clone())? <= 0.0 {
        return Err(fail(h, &r, "requires a finite positive amended value")?);
    }
    let amended = h.adjust(requested.clone(), step)?;
    if h.numeric(amended.clone())? <= 0.0 || !h.strict_eq(&amended, &requested) {
        return Err(fail(
            h,
            &r,
            "requires an amended value aligned exactly to market precision",
        )?);
    }
    if !price_change {
        let limits = get(h, &get(h, &market, "limits")?, "amount")?;
        if h.compare(amended.clone(), get(h, &limits, "min")?, true)?
            || h.compare(amended.clone(), get(h, &limits, "max")?, false)?
        {
            return Err(fail(
                h,
                &r,
                "quantity is outside the reconciled market limits",
            )?);
        }
    }
    if h.strict_eq(&amended, &*get(h, &current, name)?) {
        return Err(fail(
            h,
            &r,
            "requires a real change after market-precision validation",
        )?);
    }
    let payload = h.object(vec![
        ("order_id", canonical),
        ("symbol", get(h, &raw, "symbol")?),
        ("order_type", get(h, &raw, "type")?),
        ("side", get(h, &raw, "side")?),
        (
            if price_change {
                "order_price"
            } else {
                "order_quantity"
            },
            amended,
        ),
    ])?;
    let work = async {
        let response = awaited(
            h,
            get(h, &r, "xhr")?,
            "put",
            vec![endpoint(h, "UPDATE_ORDER"), payload],
        )
        .await?;
        let data = get(h, &response, "data")?;
        if !h.strict_eq(&*optional(h, &data, "success")?, &*h.boolean(true))
            || !eq(
                h,
                optional(h, &get(h, &data, "data")?, "status")?,
                "EDIT_SENT",
            )
        {
            let msg = choose(h, optional(h, &data, "message")?, || {
                let status = choose(
                    h,
                    optional(h, &optional(h, &data, "data")?, "status")?,
                    || Ok(text(h, "no status")),
                )?;
                Ok(h.text(&[u("Orderly native amend returned "), h.string(&status)?].concat()))
            })?;
            return Err(h.new_error(msg));
        }
        Ok(h.array(vec![get(h, &current, "id")?]))
    }
    .await;
    match work {
        Ok(v) => Ok(v),
        Err(e) => {
            log(h, &r, &e, None)?;
            Err(e)
        }
    }
}
pub async fn place_order<H: RuntimeHost>(
    h: &H,
    r: Value<H>,
    opts: Value<H>,
) -> Result<H, Value<H>> {
    let client = call(
        h,
        r.clone(),
        "resolveCallerClientOrderId",
        vec![opts.clone()],
    )?;
    let f = get(h, &r, "isAlgoOrder")?;
    let kind = get(h, &opts, "type")?;
    if h.truthy(&*h.call(f, Some(r.clone()), vec![kind])?) {
        return awaited(h, r, "placeAlgoOrder", vec![opts]).await;
    }
    let symbol = get(h, &opts, "symbol")?;
    let market = h.get(
        &get(h, &get(h, &r, "store")?, "marketsBySymbol")?,
        &h.string(&symbol)?,
    )?;
    if !h.truthy(&market) {
        return Err(h.new_error(h.text(&[u("Market not found: "), h.string(&symbol)?].concat())));
    }
    let max = get(h, &get(h, &get(h, &market, "limits")?, "amount")?, "max")?;
    let precision = get(h, &market, "precision")?;
    let pp = get(h, &precision, "price")?;
    let pa = get(h, &precision, "amount")?;
    let amount = h.adjust(get(h, &opts, "amount")?, pa.clone())?;
    let raw_price = get(h, &opts, "price")?;
    let price = if h.truthy(&raw_price) {
        h.adjust(get(h, &opts, "price")?, pp)?
    } else {
        h.null()
    };
    let kind = h.inverse_type(get(h, &opts, "type")?)?;
    let margin = call(
        h,
        r.clone(),
        "resolveMarginTypeForOrder",
        vec![opts.clone()],
    )?;
    let mode = call(h, r.clone(), "toOrderlyMarginMode", vec![margin])?;
    let mut tag = "TEALSTREET";
    for (exchange, t) in [
        ("SALSADEX", "TEALSALSA"),
        ("KODIAKFI", "TEALKODIAK"),
        ("MODE", "TEALMODE"),
        ("WHAT", "TEALWHAT"),
    ] {
        if eq(h, get(h, &r, "orderlyExchangeName")?, exchange) {
            tag = t;
            break;
        }
    }
    let symbol = h.reverse_symbol(get(h, &opts, "symbol")?)?;
    let order_price = if eq(h, get(h, &opts, "type")?, "limit") {
        price
    } else {
        h.undefined()
    };
    let reduce = get(h, &opts, "reduceOnly")?;
    let side = h.inverse_side(get(h, &opts, "side")?)?;
    let position = awaited(h, r.clone(), "getPositionSideForOrder", vec![opts.clone()]).await?;
    let req = h.omit_undefined(h.object(vec![
        ("symbol", symbol),
        ("order_type", kind),
        ("order_price", order_price),
        ("reduce_only", reduce),
        ("side", side),
        ("position_side", position),
        ("margin_mode", mode.clone()),
        ("order_tag", text(h, tag)),
        ("client_order_id", client.clone()),
    ])?)?;
    let lots = if h.compare(amount.clone(), max.clone(), false)? {
        h.ceil(h.arithmetic(Op::Div, amount.clone(), max.clone())?)?
    } else {
        h.number(1.0)
    };
    let rest = if h.compare(amount.clone(), max.clone(), false)? {
        h.adjust(h.remainder(amount.clone(), max)?, pa.clone())?
    } else {
        h.number(0.0)
    };
    let lot_size = h.adjust(
        h.arithmetic(
            Op::Div,
            h.arithmetic(Op::Sub, amount, rest.clone())?,
            lots.clone(),
        )?,
        pa,
    )?;
    let payloads = h.times(lots, || {
        h.merge(req.clone(), vec![("order_quantity", lot_size.clone())])
    })?;
    if h.truthy(&rest) {
        let extra = h.merge(req, vec![("order_quantity", rest)])?;
        h.push(payloads.clone(), vec![extra])?
    }
    call(
        h,
        r.clone(),
        "assertSingleNativeClientOrderId",
        vec![client, length(h, payloads.clone())?],
    )?;
    let work = async {
        let ids = h
            .map_series(payloads, |payload| {
                let r = r.clone();
                async move {
                    let response = awaited(
                        h,
                        get(h, &r, "xhr")?,
                        "post",
                        vec![endpoint(h, "PLACE_ORDER"), payload],
                    )
                    .await?;
                    let data = get(h, &get(h, &response, "data")?, "data")?;
                    stringify(h, get(h, &data, "order_id")?)
                }
            })
            .await?;
        if h.truthy(&*get(h, &opts, "stopLoss")?) || h.truthy(&*get(h, &opts, "takeProfit")?) {
            let copied = h.merge(opts.clone(), vec![])?;
            let side = if eq(h, get(h, &opts, "side")?, "buy") {
                text(h, "sell")
            } else {
                text(h, "buy")
            };
            h.set(&copied, &u("side"), side)?;
            h.set(&copied, &u("marginMode"), mode)?;
            let algo = awaited(h, r.clone(), "placePositionalAlgoOrder", vec![copied]).await?;
            h.push(ids.clone(), h.spread(algo)?)?
        }
        Ok(ids)
    }
    .await;
    match work {
        Ok(v) => Ok(v),
        Err(e) => {
            let msg = message(h, &e, "message")?;
            let f = get(h, &r, "emitAndLogError")?;
            let options = h.error_log_options(msg.clone())?;
            h.call(f, Some(r), vec![h.error_value(&e), msg, options])?;
            Err(e)
        }
    }
}
pub async fn place_algo<H: RuntimeHost>(h: &H, r: Value<H>, opts: Value<H>) -> Result<H, Value<H>> {
    if eq(h, get(h, &opts, "type")?, "trailing_stop_market") {
        let e = h.new_error(text(h, "Orderly trailing-stop placement is not supported"));
        call(h, r, "emitAndLogError", vec![h.error_value(&e)])?;
        return Err(e);
    }
    if !h.truthy(&*get(h, &opts, "price")?) {
        call(
            h,
            r,
            "emitAndLogError",
            vec![text(h, "Price is required for algo orders")],
        )?;
        return Ok(h.array(vec![]));
    }
    let f = get(h, &r, "toOrderlyMarginMode")?;
    let resolve = get(h, &r, "resolveMarginTypeForOrder")?;
    let arg = h.merge(opts.clone(), vec![("reduceOnly", h.boolean(true))])?;
    let margin = h.call(resolve, Some(r.clone()), vec![arg])?;
    let mode = h.call(f, Some(r.clone()), vec![margin])?;
    let rows = get(h, &get(h, &r, "store")?, "orders")?;
    let sibling = h.find(rows, |o| {
        if !h.strict_eq(&*get(h, &o, "symbol")?, &*get(h, &opts, "symbol")?) {
            return Ok(false);
        }
        if !eq(h, get(h, &o, "type")?, "stop_market")
            && !eq(h, get(h, &o, "type")?, "take_profit_market")
        {
            return Ok(false);
        }
        if !eq(h, get(h, &o, "status")?, "open") {
            return Ok(false);
        }
        let f = get(h, &r, "toOrderlyMarginMode")?;
        let map = get(h, &r, "mapMarginType")?;
        let raw = h.get_key(get(h, &o, "info")?, "margin_mode")?;
        let margin = h.call(map, Some(r.clone()), vec![raw])?;
        Ok(h.strict_eq(&*h.call(f, Some(r.clone()), vec![margin])?, &mode))
    })?;
    let mut stop = if eq(h, get(h, &opts, "type")?, "stop_market") {
        get(h, &opts, "price")?
    } else {
        h.undefined()
    };
    let mut take = if eq(h, get(h, &opts, "type")?, "take_profit_market") {
        get(h, &opts, "price")?
    } else {
        h.undefined()
    };
    if h.truthy(&sibling) {
        if eq(h, get(h, &sibling, "type")?, "stop_market") {
            stop = get(h, &sibling, "price")?
        }
        if eq(h, get(h, &sibling, "type")?, "take_profit_market") {
            take = get(h, &sibling, "price")?
        }
        awaited(h, r.clone(), "cancelAlgoOrder", vec![sibling]).await?;
    }
    let params = h.object(vec![
        ("symbol", get(h, &opts, "symbol")?),
        ("side", get(h, &opts, "side")?),
    ])?;
    if h.is_number(&stop) {
        h.set(&params, &u("stopLoss"), stop)?
    }
    if h.is_number(&take) {
        h.set(&params, &u("takeProfit"), take)?
    }
    let f = get(h, &r, "placePositionalAlgoOrder")?;
    let payload = h.merge(params, vec![("marginMode", mode)])?;
    let p = h.call(f, Some(r), vec![payload])?;
    h.await_value(p).await
}
pub async fn place_positional<H: RuntimeHost>(
    h: &H,
    r: Value<H>,
    opts: Value<H>,
) -> Result<H, Value<H>> {
    let symbol = get(h, &opts, "symbol")?;
    let market = h.get(
        &get(h, &get(h, &r, "store")?, "marketsBySymbol")?,
        &h.string(&symbol)?,
    )?;
    if !h.truthy(&market) {
        return Err(
            h.new_error(h.text(&[u("Market "), h.string(&symbol)?, u(" not found")].concat()))
        );
    }
    let pp = get(h, &get(h, &market, "precision")?, "price")?;
    let other = h.omit_undefined(h.object(vec![("reduce_only", h.boolean(true))])?)?;
    let req = h.merge(other.clone(), vec![])?;
    h.set(
        &req,
        &u("symbol"),
        h.reverse_symbol(get(h, &opts, "symbol")?)?,
    )?;
    h.set(&req, &u("algo_type"), text(h, "POSITIONAL_TP_SL"))?;
    let mode = get(h, &opts, "marginMode")?;
    let mode = if h.nullish(&mode) {
        let f = get(h, &r, "toOrderlyMarginMode")?;
        let margin = call(
            h,
            r.clone(),
            "getCurrentMarginType",
            vec![get(h, &opts, "symbol")?],
        )?;
        h.call(f, Some(r.clone()), vec![margin])?
    } else {
        mode
    };
    h.set(&req, &u("margin_mode"), mode)?;
    h.set(&req, &u("child_orders"), h.array(vec![]))?;
    for (key, kind) in [("stopLoss", "STOP_LOSS"), ("takeProfit", "TAKE_PROFIT")] {
        if h.is_number(&*get(h, &opts, key)?) {
            let children = get(h, &req, "child_orders")?;
            let child = h.merge(other.clone(), vec![])?;
            h.set(&child, &u("algo_type"), text(h, kind))?;
            h.set(&child, &u("type"), text(h, "CLOSE_POSITION"))?;
            let side = if eq(h, get(h, &opts, "side")?, "sell") {
                text(h, "SELL")
            } else {
                text(h, "BUY")
            };
            h.set(&child, &u("side"), side)?;
            let trigger = h.adjust(get(h, &opts, key)?, pp.clone())?;
            h.set(&child, &u("trigger_price"), stringify(h, trigger)?)?;
            h.push(children, vec![child])?;
        }
    }
    let work = async {
        let response = awaited(
            h,
            get(h, &r, "xhr")?,
            "post",
            vec![endpoint(h, "ALGO_ORDER"), req],
        )
        .await?;
        let data = get(h, &get(h, &response, "data")?, "data")?;
        h.map(get(h, &data, "rows")?, |row| {
            stringify(h, get(h, &row, "orderId")?)
        })
    }
    .await;
    match work {
        Ok(v) => Ok(v),
        Err(e) => {
            let msg = message(h, &e, "message")?;
            let f = get(h, &r, "emitAndLogError")?;
            let options = h.error_log_options(msg.clone())?;
            h.call(f, Some(r), vec![msg, h.undefined(), options])?;
            Ok(h.array(vec![]))
        }
    }
}
