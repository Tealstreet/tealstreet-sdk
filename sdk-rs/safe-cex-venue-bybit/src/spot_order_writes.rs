//! Bybit Spot order-write/local-update bodies. Dependencies remain native,
//! separately mapped collaborators; values/callables retain their owners/aliases.
use crate::hyro_private_session::{SessionFuture, SessionHost};
pub trait SpotOrderHost: SessionHost {
    fn spread(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn map<F>(&self, method: Self::Value, body: F) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<Self::Value, Self::Error>;
    fn filter<F>(&self, method: Self::Value, body: F) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<bool, Self::Error>;
    fn some<F>(&self, method: Self::Value, body: F) -> Result<bool, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<bool, Self::Error>;
    fn find<F>(&self, method: Self::Value, body: F) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<bool, Self::Error>;
    fn reduce<F>(
        &self,
        method: Self::Value,
        initial: Self::Value,
        body: F,
    ) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, Self::Value) -> Result<Self::Value, Self::Error>;
    /// Native for-of iteration, including IteratorClose on callback failure.
    fn for_of<F>(&self, values: Self::Value, body: F) -> Result<(), Self::Error>
    where
        F: FnMut(Self::Value) -> Result<(), Self::Error>;
    /// Imported p-iteration helper: preserves live length, holes, input awaits
    /// and ordered async callbacks. The original runtime owns started promises.
    fn series<'a, F>(
        &'a self,
        callee: Self::Value,
        values: Self::Value,
        body: F,
    ) -> SessionFuture<'a, Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> SessionFuture<'a, Self::Value, Self::Error> + 'a;
}
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
fn p<H: SpotOrderHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.get(v, k)
}
fn op<H: SpotOrderHost>(h: &H, k: &str, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    h.intrinsic(k, args)
}
fn constant<H: SpotOrderHost>(h: &H, kind: &str, name: &str) -> Result<H::Value, H::Error> {
    h.get(&h.get(&h.global(), kind)?, name)
}
fn is<H: SpotOrderHost>(h: &H, v: &H::Value, kind: &str, name: &str) -> Result<bool, H::Error> {
    Ok(h.equal(v, &constant(h, kind, name)?))
}
fn parse<H: SpotOrderHost>(
    h: &H,
    v: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(&h.global(), "parseFloat")?;
    h.apply(f, vec![v()?])
}
fn or<H: SpotOrderHost>(
    h: &H,
    a: H::Value,
    b: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&a) { Ok(a) } else { b() }
}
fn template<H: SpotOrderHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    op(h, "template", vec![v])
}
async fn awaited<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    k: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let promise = call!(h, r.clone(), k, args);
    h.wait(promise).await
}
fn emit<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    msg: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<(), H::Error> {
    let selected = h.select(r.clone(), "emitAndLogError")?;
    h.apply(selected, vec![msg()?])?;
    Ok(())
}
fn endpoint<H: SpotOrderHost>(h: &H, name: &str) -> Result<H::Value, H::Error> {
    constant(h, "ENDPOINTS", name)
}
fn virtual_id<H: SpotOrderHost>(h: &H, r: &H::Value) -> Result<bool, H::Error> {
    if h.truthy(&call!(
        h,
        p(h, r, "id")?,
        "endsWith",
        vec![h.literal("__stop_loss")]
    )) {
        Ok(true)
    } else {
        Ok(h.truthy(&call!(
            h,
            p(h, r, "id")?,
            "endsWith",
            vec![h.literal("__take_profit")]
        )))
    }
}
// ts: BybitSpotExchange#recordRecentlyPlacedOrderFromOpts
pub fn record_recently_placed_order_from_opts<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    id: H::Value,
    opts: H::Value,
    payload: H::Value,
) -> Result<(), H::Error> {
    let qty = or(h, parse(h, || p(h, &payload, "qty"))?, || {
        or(h, p(h, &opts, "amount")?, || Ok(h.number(0.)))
    })?;
    let price = parse(h, || {
        let a = p(h, &payload, "price")?;
        if h.equal(&a, &h.undefined()) || h.equal(&a, &op(h, "null", vec![])?) {
            p(h, &payload, "triggerPrice")
        } else {
            Ok(a)
        }
    })?;
    let price = or(h, price, || {
        or(h, p(h, &opts, "price")?, || {
            or(h, p(h, &opts, "triggerPrice")?, || Ok(h.number(0.)))
        })
    })?;
    let f = h.select(r.clone(), "recordRecentlyPlacedOrder")?;
    let id = id;
    let status = constant(h, "OrderStatus", "Open")?;
    let symbol = p(h, &opts, "symbol")?;
    let kind = p(h, &opts, "type")?;
    let side = p(h, &opts, "side")?;
    let reduce = if is(h, &p(h, &opts, "type")?, "OrderType", "Conditional")? {
        h.boolean(false)
    } else {
        p(h, &opts, "reduceOnly")?
    };
    let reduce = or(h, reduce, || Ok(h.boolean(false)))?;
    let tif = or(h, p(h, &opts, "timeInForce")?, || {
        constant(h, "OrderTimeInForce", "GoodTillCancel")
    })?;
    let now = call!(
        h,
        p(h, &h.global(), "virtualClock")?,
        "getCurrentTimeMs",
        vec![]
    );
    h.apply(
        f,
        vec![h.object(vec![
            ("id", id),
            ("status", status),
            ("symbol", symbol),
            ("type", kind),
            ("side", side),
            ("price", price),
            ("amount", qty.clone()),
            ("filled", h.number(0.)),
            ("remaining", qty),
            ("reduceOnly", reduce),
            ("timeInForce", tif),
            ("lastUpdate", now),
        ])],
    )?;
    Ok(())
}
// ts: BybitSpotExchange#applyLocalOrderUpdateAfterRestWrite
pub fn apply_local_order_update_after_rest_write<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    order: H::Value,
    update: H::Value,
) -> Result<(), H::Error> {
    if !h.truthy(&call!(
        h,
        r.clone(),
        "shouldUseLocalRestOrderLifecycleFallback",
        vec![]
    )) {
        return Ok(());
    }
    let next = h.spread(order.clone())?;
    let updated = h.spread(update.clone())?;
    op(h, "copyOwnDataProperties", vec![next.clone(), updated])?;
    if h.equal(
        &op(h, "typeof", vec![p(h, &update, "amount")?])?,
        &h.literal("number"),
    ) {
        let max = h.select(p(h, &h.global(), "Math")?, "max")?;
        let amount = p(h, &update, "amount")?;
        let filled = or(h, p(h, &order, "filled")?, || Ok(h.number(0.)))?;
        let remaining = op(h, "subtract", vec![amount, filled])?;
        let remaining = h.apply(max, vec![h.number(0.), remaining])?;
        h.define(&next, "remaining", remaining)?;
    }
    let now = call!(
        h,
        p(h, &h.global(), "virtualClock")?,
        "getCurrentTimeMs",
        vec![]
    );
    h.define(&next, "lastUpdate", now)?;
    call!(
        h,
        p(h, r, "store")?,
        "addOrUpdateOrders",
        vec![h.array(vec![next])]
    );
    Ok(())
}
// ts: BybitSpotExchange._hasFullSizedPositionOppositeSide
pub fn has_full_sized_position_opposite_side<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    symbol: H::Value,
    side: H::Value,
    amount: H::Value,
) -> Result<bool, H::Error> {
    let buy = is(h, &side, "OrderSide", "Buy")?;
    let some = h.select(p(h, &p(h, r, "store")?, "positions")?, "some")?;
    h.some(some, |position, _| {
        if !h.equal(&p(h, &position, "symbol")?, &symbol)
            || !is(
                h,
                &p(h, &position, "side")?,
                "PositionSide",
                if buy { "Short" } else { "Long" },
            )?
            || h.equal(&p(h, &position, "contracts")?, &h.number(0.))
        {
            return Ok(false);
        }
        Ok(h.truthy(&op(
            h,
            "greaterEqual",
            vec![p(h, &position, "contracts")?, amount.clone()],
        )?))
    })
}
// ts: BybitSpotExchange.placeOrders
pub async fn place_orders<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    orders: H::Value,
) -> Result<H::Value, H::Error> {
    call!(
        h,
        r.clone(),
        "assertCallerClientOrderIds",
        vec![orders.clone()]
    );
    h.for_of(orders.clone(), |order| {
        call!(h, r.clone(), "getOrderExecutionParams", vec![order]);
        Ok(())
    })?;
    let map = h.select(h.global(), "mapSeries")?;
    let ids = h
        .series(map, orders, |order, _| {
            Box::pin(async move { awaited(h, r, "placeOrder", vec![order]).await })
        })
        .await?;
    Ok(call!(h, ids, "flat", vec![]))
}
// ts: BybitSpotExchange.placeStopLossOrTakeProfit
pub async fn place_stop_loss_or_take_profit<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let market = op(
        h,
        "keyGet",
        vec![
            p(h, &p(h, r, "store")?, "marketsBySymbol")?,
            p(h, &opts, "symbol")?,
        ],
    )?;
    if !h.truthy(&market) {
        let message = op(h, "missingMarket", vec![p(h, &opts, "symbol")?])?;
        return Err(h.error(message)?);
    }
    let precision = p(h, &p(h, &market, "precision")?, "amount")?;
    let adjust = h.select(h.global(), "adjust")?;
    let amount = h.apply(adjust, vec![p(h, &opts, "amount")?, precision])?;
    let amount = if h.truthy(&amount) && !h.equal(&amount, &h.number(0.)) {
        amount
    } else {
        h.undefined()
    };
    let category = h.literal("spot");
    let symbol = p(h, &opts, "symbol")?;
    let index = awaited(h, r, "getStopOrderPositionIdx", vec![opts.clone()]).await?;
    let payload = h.object(vec![
        ("category", category),
        ("symbol", symbol),
        ("positionIdx", index),
        (
            "tpslMode",
            h.literal(if h.truthy(&amount) { "Partial" } else { "Full" }),
        ),
    ]);
    if is(h, &p(h, &opts, "type")?, "OrderType", "StopLoss")? {
        h.set(&payload, "stopLoss", template(h, p(h, &opts, "price")?)?)?;
        h.set(
            &payload,
            "slTriggerBy",
            h.literal(
                if is(
                    h,
                    &p(h, &opts, "slTriggerType")?,
                    "OrderStopTriggerType",
                    "LastPrice",
                )? {
                    "LastPrice"
                } else {
                    "MarkPrice"
                },
            ),
        )?;
        if h.truthy(&amount) {
            h.set(&payload, "slSize", template(h, amount.clone())?)?;
        }
    }
    if is(h, &p(h, &opts, "type")?, "OrderType", "TakeProfit")? {
        h.set(&payload, "takeProfit", template(h, p(h, &opts, "price")?)?)?;
        h.set(
            &payload,
            "tpTriggerBy",
            h.literal(
                if is(
                    h,
                    &p(h, &opts, "tpTriggerType")?,
                    "OrderStopTriggerType",
                    "MarkPrice",
                )? {
                    "MarkPrice"
                } else {
                    "LastPrice"
                },
            ),
        )?;
        if h.truthy(&amount) {
            h.set(&payload, "tpSize", template(h, amount)?)?;
        }
    }
    let post = h.select(p(h, r, "xhr")?, "post")?;
    let e = endpoint(h, "SET_TRADING_STOP")?;
    let response = h.wait(h.apply(post, vec![e, payload])?).await?;
    let data = p(h, &response, "data")?;
    if !h.equal(&p(h, &data, "retMsg")?, &h.literal("OK")) {
        emit(h, r, || p(h, &data, "retMsg"))?;
    }
    Ok(h.array(vec![p(h, &p(h, &data, "result")?, "orderId")?]))
}
// ts: BybitSpotExchange.placeTrailingStopLoss
pub async fn place_trailing_stop_loss<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let ticker = op(
        h,
        "keyGet",
        vec![
            p(h, &p(h, r, "store")?, "tickersBySymbol")?,
            p(h, &opts, "symbol")?,
        ],
    )?;
    let market = op(
        h,
        "keyGet",
        vec![
            p(h, &p(h, r, "store")?, "marketsBySymbol")?,
            p(h, &opts, "symbol")?,
        ],
    )?;
    if !h.truthy(&ticker) || !h.truthy(&market) {
        return Err(h.error(op(h, "missingTicker", vec![p(h, &opts, "symbol")?])?)?);
    }
    let adjust = h.select(h.global(), "adjust")?;
    let max = h.select(p(h, &h.global(), "Math")?, "max")?;
    let max = h.apply(max, vec![p(h, &ticker, "last")?, p(h, &opts, "price")?])?;
    let min = h.select(p(h, &h.global(), "Math")?, "min")?;
    let min = h.apply(min, vec![p(h, &ticker, "last")?, p(h, &opts, "price")?])?;
    let distance = op(h, "subtract", vec![max, min])?;
    let precision = p(h, &p(h, &market, "precision")?, "price")?;
    let distance = h.apply(adjust, vec![distance, precision])?;
    let category = h.literal("spot");
    let symbol = p(h, &opts, "symbol")?;
    let index = awaited(h, r, "getStopOrderPositionIdx", vec![opts]).await?;
    let payload = h.object(vec![
        ("category", category),
        ("symbol", symbol),
        ("positionIdx", index),
        ("trailingStop", template(h, distance)?),
    ]);
    let post = h.select(p(h, r, "xhr")?, "post")?;
    let e = endpoint(h, "SET_TRADING_STOP")?;
    let response = h.wait(h.apply(post, vec![e, payload])?).await?;
    let data = p(h, &response, "data")?;
    if !h.equal(&p(h, &data, "retMsg")?, &h.literal("OK")) {
        emit(h, r, || p(h, &data, "retMsg"))?;
    }
    Ok(h.array(vec![p(h, &p(h, &data, "result")?, "orderId")?]))
}
// ts: BybitSpotExchange.cancelOrders
pub async fn cancel_orders<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    orders: H::Value,
) -> Result<H::Value, H::Error> {
    h.for_of(orders.clone(), |order| {
        call!(h, r.clone(), "markOrderSeen", vec![p(h, &order, "id")?]);
        Ok(())
    })?;
    let method = h.select(orders, "filter")?;
    let filtered = h.filter(method, |o, _| Ok(!virtual_id(h, &o)?))?;
    if h.equal(&p(h, &filtered, "length")?, &h.number(0.)) {
        return Ok(h.undefined());
    }
    let reduce = h.select(filtered, "reduce")?;
    let grouped = h.reduce(reduce, h.object(vec![]), |acc, order| {
        if !h.truthy(&p(h, &acc, "spot")?) {
            h.set(&acc, "spot", h.array(vec![]))?;
        }
        call!(h, p(h, &acc, "spot")?, "push", vec![order]);
        Ok(acc)
    })?;
    let series = h.select(h.global(), "forEachSeries")?;
    let entries = call!(h, p(h, &h.global(), "Object")?, "entries", vec![grouped]);
    h.series(series, entries, |entry, _| {
        Box::pin(async move {
            let pair = op(h, "destructure2", vec![entry])?;
            let category = p(h, &pair, "0")?;
            let orders = p(h, &pair, "1")?;
            awaited(h, r, "cancelOrdersHelper", vec![orders, category]).await?;
            Ok(h.undefined())
        })
    })
    .await?;
    Ok(h.undefined())
}
// ts: BybitSpotExchange.cancelOrdersHelper
pub async fn cancel_orders_helper<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    orders: H::Value,
    category: H::Value,
) -> Result<H::Value, H::Error> {
    let lots = call!(h, h.global(), "chunk", vec![orders, h.number(10.)]);
    let each = h.select(h.global(), "forEachSeries")?;
    h.series(each, lots, |orders, i| {
        let category = category.clone();
        Box::pin(async move {
            if i > 0 {
                let sleep = call!(h, h.global(), "sleep", vec![h.number(1000.)]);
                h.wait(sleep).await?;
            }
            let post = h.select(p(h, r, "unlimitedXHR")?, "post")?;
            let e = endpoint(h, "CANCEL_ORDERS")?;
            let map = h.select(orders.clone(), "map")?;
            let request = h.map(map, |o, _| {
                Ok(h.object(vec![
                    ("symbol", p(h, &o, "symbol")?),
                    ("orderId", p(h, &o, "id")?),
                ]))
            })?;
            let response = h
                .wait(h.apply(
                    post,
                    vec![
                        e,
                        h.object(vec![("category", category), ("request", request)]),
                    ],
                )?)
                .await?;
            let data = p(h, &response, "data")?;
            if !h.equal(&p(h, &data, "retMsg")?, &h.literal("OK")) {
                if h.truthy(&call!(
                    h,
                    h.global(),
                    "isBybitOrderAlreadyGoneCancelMessage",
                    vec![p(h, &data, "retMsg")?]
                )) {
                    call!(h, r.clone(), "retireCancelledOrders", vec![orders]);
                    return Ok(h.undefined());
                }
                emit(h, r, || p(h, &data, "retMsg"))?;
                return Err(h.error(p(h, &data, "retMsg")?)?);
            }
            let info = p(h, &data, "retExtInfo")?;
            let found = if h.equal(&info, &h.undefined()) || h.equal(&info, &op(h, "null", vec![])?)
            {
                h.undefined()
            } else {
                let find = h.select(p(h, &info, "list")?, "find")?;
                h.find(find, |l, _| {
                    if h.equal(&p(h, &l, "code")?, &h.number(0.)) {
                        return Ok(false);
                    }
                    Ok(!h.truthy(&call!(
                        h,
                        h.global(),
                        "isBybitOrderAlreadyGoneCancelMessage",
                        vec![p(h, &l, "msg")?]
                    )))
                })?
            };
            if h.truthy(&found) {
                emit(h, r, || p(h, &found, "msg"))?;
                return Err(h.error(p(h, &found, "msg")?)?);
            }
            call!(h, r.clone(), "retireCancelledOrders", vec![orders]);
            Ok(h.undefined())
        })
    })
    .await?;
    Ok(h.undefined())
}
// ts: BybitSpotExchange.cancelSymbolOrders
pub async fn cancel_symbol_orders<H: SpotOrderHost>(
    h: &H,
    r: &H::Value,
    symbol: H::Value,
    cancel_tp_sl: H::Value,
) -> Result<H::Value, H::Error> {
    let cancel = if h.equal(&cancel_tp_sl, &h.undefined()) {
        h.boolean(true)
    } else {
        cancel_tp_sl
    };
    let tps = if h.truthy(&cancel) {
        h.array(vec![])
    } else {
        let filter = h.select(p(h, &p(h, r, "store")?, "orders")?, "filter")?;
        h.filter(filter, |o, _| {
            Ok(h.equal(&p(h, &o, "symbol")?, &symbol)
                && !is(h, &p(h, &o, "type")?, "OrderType", "Limit")?)
        })?
    };
    let filter = h.select(p(h, &p(h, r, "store")?, "orders")?, "filter")?;
    let rows = h.filter(filter, |o, _| Ok(h.equal(&p(h, &o, "symbol")?, &symbol)))?;
    let map = h.select(rows, "map")?;
    let rows = h.map(map, |o, _| {
        Ok(h.object(vec![
            ("id", p(h, &o, "id")?),
            ("symbol", p(h, &o, "symbol")?),
        ]))
    })?;
    let post = h.select(p(h, r, "unlimitedXHR")?, "post")?;
    let e = endpoint(h, "CANCEL_SYMBOL_ORDERS")?;
    let response = h
        .wait(h.apply(
            post,
            vec![
                e,
                h.object(vec![("category", h.literal("spot")), ("symbol", symbol)]),
            ],
        )?)
        .await?;
    let data = p(h, &response, "data")?;
    if h.truthy(&op(
        h,
        "greater",
        vec![p(h, &tps, "length")?, h.number(0.)],
    )?) {
        let place = h.select(r.clone(), "placeOrders")?;
        let map = h.select(tps, "map")?;
        let payloads = h.map(map, |o, _| {
            let copy = h.spread(o.clone())?;
            h.define(
                &copy,
                "side",
                constant(
                    h,
                    "OrderSide",
                    if is(h, &p(h, &o, "side")?, "OrderSide", "Buy")? {
                        "Sell"
                    } else {
                        "Buy"
                    },
                )?,
            )?;
            Ok(copy)
        })?;
        h.wait(h.apply(place, vec![payloads])?).await?;
    }
    let msg = call!(h, h.global(), "v", vec![data.clone(), h.literal("retMsg")]);
    if !h.equal(&msg, &h.literal("OK")) {
        emit(h, r, || {
            Ok(call!(h, h.global(), "v", vec![data, h.literal("retMsg")]))
        })?;
        return Ok(h.undefined());
    }
    call!(h, r.clone(), "retireCancelledOrders", vec![rows]);
    Ok(h.undefined())
}
