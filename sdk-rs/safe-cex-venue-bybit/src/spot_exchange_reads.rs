//! Spot account/market reads and order translation bodies over native capabilities.
use crate::spot_order_writes::SpotOrderHost;
pub trait SpotExchangeHost: SpotOrderHost {
    /// Iterator destructuring with elisions that do not read value, then IteratorClose.
    fn destructure(
        &self,
        value: Self::Value,
        skip: usize,
        count: usize,
    ) -> Result<Vec<Self::Value>, Self::Error>;
    fn for_each<F>(&self, method: Self::Value, body: F) -> Result<(), Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<(), Self::Error>;
    fn times<F>(
        &self,
        callee: Self::Value,
        count: Self::Value,
        body: F,
    ) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value) -> Result<Self::Value, Self::Error>;
    fn new_error(
        &self,
        constructor: Self::Value,
        message: Self::Value,
    ) -> Result<Self::Error, Self::Error>;
    fn key_set(
        &self,
        object: &Self::Value,
        key: Self::Value,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
}
macro_rules! call {
    ($h:expr,$owner:expr,$key:expr,$args:expr) => {{
        let selected = $h.select($owner, $key)?;
        $h.apply(selected, $args)?
    }};
}
fn p<H: SpotExchangeHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.get(v, k)
}
fn constant<H: SpotExchangeHost>(h: &H, k: &str, n: &str) -> Result<H::Value, H::Error> {
    h.get(&h.get(&h.global(), k)?, n)
}
fn eqs<H: SpotExchangeHost>(h: &H, v: &H::Value, s: &str) -> bool {
    h.equal(v, &h.literal(s))
}
fn is<H: SpotExchangeHost>(h: &H, v: &H::Value, k: &str, n: &str) -> Result<bool, H::Error> {
    Ok(h.equal(v, &constant(h, k, n)?))
}
fn key<H: SpotExchangeHost>(h: &H, v: H::Value, k: H::Value) -> Result<H::Value, H::Error> {
    h.intrinsic("keyGet", vec![v, k])
}
fn or<H: SpotExchangeHost>(
    h: &H,
    v: H::Value,
    f: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { f() }
}
fn nullish<H: SpotExchangeHost>(h: &H, v: &H::Value) -> Result<bool, H::Error> {
    Ok(h.equal(v, &h.undefined()) || h.equal(v, &h.intrinsic("null", vec![])?))
}
fn coalesce<H: SpotExchangeHost>(
    h: &H,
    v: H::Value,
    f: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    if nullish(h, &v)? { f() } else { Ok(v) }
}
fn parse<H: SpotExchangeHost>(
    h: &H,
    f: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    let parse = h.select(h.global(), "parseFloat")?;
    h.apply(parse, vec![f()?])
}
fn template<H: SpotExchangeHost>(h: &H, v: H::Value) -> Result<H::Value, H::Error> {
    h.intrinsic("template", vec![v])
}
fn numeric<H: SpotExchangeHost>(
    h: &H,
    name: &str,
    a: H::Value,
    b: H::Value,
) -> Result<H::Value, H::Error> {
    h.intrinsic(name, vec![a, b])
}
fn emit<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    msg: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<(), H::Error> {
    let f = h.select(r.clone(), "emitAndLogError")?;
    h.apply(f, vec![msg()?])?;
    Ok(())
}
fn error<H: SpotExchangeHost>(
    h: &H,
    msg: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Error, H::Error> {
    let f = h.select(h.global(), "Error")?;
    h.new_error(f, msg()?)
}
fn v<H: SpotExchangeHost>(h: &H, value: H::Value, k: &str) -> Result<H::Value, H::Error> {
    Ok(call!(h, h.global(), "v", vec![value, h.literal(k)]))
}
async fn awaited<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    k: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    let value = call!(h, r.clone(), k, args);
    h.wait(value).await
}
fn mix<H: SpotExchangeHost>(h: &H, base: H::Value, extra: H::Value) -> Result<H::Value, H::Error> {
    let out = h.spread(base)?;
    h.intrinsic("copyOwnDataProperties", vec![out.clone(), h.spread(extra)?])?;
    Ok(out)
}
// ts: BybitSpotExchange.fetchRawBalance
pub async fn fetch_raw_balance<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
) -> Result<H::Value, H::Error> {
    let get = h.select(p(h, r, "xhr")?, "get")?;
    let path = constant(h, "ENDPOINTS", "BALANCE")?;
    let account = if h.truthy(&p(h, r, "forceUnifiedBalance")?) {
        h.literal("UNIFIED")
    } else {
        p(h, r, "accountType")?
    };
    let response = h
        .wait(h.apply(
            get,
            vec![
                path,
                h.object(vec![("params", h.object(vec![("accountType", account)]))]),
            ],
        )?)
        .await?;
    let data = p(h, &response, "data")?;
    if !eqs(h, &v(h, data.clone(), "retMsg")?, "OK") {
        if eqs(
            h,
            &v(h, data.clone(), "retMsg")?,
            "accountType only support UNIFIED.",
        ) && !h.truthy(&p(h, r, "forceUnifiedBalance")?)
        {
            h.set(r, "forceUnifiedBalance", h.boolean(true))?;
            h.set(r, "unifiedMarginStatus", h.number(6.))?;
            return awaited(h, r, "fetchRawBalance", vec![]).await;
        }
        emit(h, r, || v(h, data, "retMsg"))?;
        return or(
            h,
            h.optional(&p(h, &p(h, r, "store")?, "balance")?, "info")?,
            || Ok(h.array(vec![])),
        );
    }
    p(h, &p(h, &data, "result")?, "list")
}
// ts: BybitSpotExchange.formatBalance
pub fn format_balance<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    raw: H::Value,
) -> Result<H::Value, H::Error> {
    let is_array = call!(h, p(h, &h.global(), "Array")?, "isArray", vec![raw.clone()]);
    if !h.truthy(&is_array) || h.equal(&p(h, &raw, "length")?, &h.number(0.)) {
        return p(h, &p(h, r, "store")?, "balance");
    }
    let first = h.destructure(raw, 0, 1)?.remove(0);
    let total = parse(h, || p(h, &first, "totalEquity"))?;
    let maintenance = parse(h, || p(h, &first, "totalMaintenanceMargin"))?;
    let initial = parse(h, || p(h, &first, "totalInitialMargin"))?;
    let used = numeric(h, "add", maintenance, initial)?;
    let free = parse(h, || {
        coalesce(h, p(h, &first, "totalAvailableBalance")?, || {
            p(h, &first, "totalMarginBalance")
        })
    })?;
    let reduce = h.select(p(h, &first, "coin")?, "reduce")?;
    let coins = h.reduce(reduce, h.object(vec![]), |acc, coin| {
        // Assignment evaluates its reference/key before evaluating the object value.
        let name = p(h, &coin, "coin")?;
        let base = p(h, &coin, "coin")?;
        let amount = parse(h, || p(h, &coin, "equity"))?;
        let notional = parse(h, || p(h, &coin, "usdValue"))?;
        h.key_set(
            &acc,
            name,
            h.object(vec![
                ("base", base),
                ("amount", amount),
                ("notional", notional),
            ]),
        )?;
        Ok(acc)
    })?;
    Ok(h.object(vec![
        ("total", total),
        ("upnl", h.number(0.)),
        ("used", used),
        ("free", free),
        ("coins", coins),
        ("info", first),
    ]))
}
// ts: BybitSpotExchange.fetchOrders
pub async fn fetch_orders<H: SpotExchangeHost>(h: &H, r: &H::Value) -> Result<H::Value, H::Error> {
    let mut cursor = h.literal("");
    let mut orders = h.array(vec![]);
    loop {
        if h.equal(&cursor, &h.undefined()) {
            cursor = h.literal("");
        }
        let reinit = h.select(r.clone(), "reinitOnThrow")?;
        let get = h.select(p(h, r, "xhr")?, "get")?;
        let path = constant(h, "ENDPOINTS", "UNFILLED_ORDERS")?;
        let promise = h.apply(
            get,
            vec![
                path,
                h.object(vec![(
                    "params",
                    h.object(vec![
                        ("category", h.literal("spot")),
                        ("limit", h.number(50.)),
                        ("cursor", cursor),
                    ]),
                )]),
            ],
        )?;
        let response = h.wait(h.apply(reinit, vec![promise])?).await?;
        let data = p(h, &response, "data")?;
        if !eqs(h, &v(h, data.clone(), "retMsg")?, "OK") {
            emit(h, r, || v(h, data.clone(), "retMsg"))?;
            h.set(r, "needsReinit", h.boolean(true))?;
            return Err(error(h, || v(h, data, "retMsg"))?);
        }
        let array = h.select(p(h, &h.global(), "Array")?, "isArray")?;
        let list = h.optional(&h.optional(&data, "result")?, "list")?;
        let is_array = h.apply(array, vec![list])?;
        let list = if h.truthy(&is_array) {
            p(h, &p(h, &data, "result")?, "list")?
        } else {
            h.array(vec![])
        };
        if h.equal(&p(h, &list, "length")?, &h.number(0.)) {
            break;
        }
        if h.truthy(&p(h, &p(h, &data, "result")?, "nextPageCursor")?) {
            // Recursive-call arguments read the next cursor before spreading both lists.
            cursor = p(h, &p(h, &data, "result")?, "nextPageCursor")?;
            orders = h.intrinsic("arraySpread", vec![orders, list])?;
        } else {
            orders = h.intrinsic("arraySpread", vec![orders, list])?;
            break;
        }
    }
    let flatmap = h.select(orders, "flatMap")?;
    let map = p(h, r, "mapOrder")?;
    let bound = call!(h, map, "bind", vec![r.clone()]);
    let orders = h.apply(flatmap, vec![bound])?;
    Ok(call!(
        h,
        r.clone(),
        "preserveRecentlyPlacedOrders",
        vec![orders]
    ))
}
// ts: BybitSpotExchange.fetchMarkets
pub async fn fetch_markets<H: SpotExchangeHost>(h: &H, r: &H::Value) -> Result<H::Value, H::Error> {
    let markets = awaited(h, r, "fetchMarketsHelper", vec![h.literal("spot")]).await?;
    let markets = call!(h, markets, "flat", vec![]);
    let some = h.select(markets.clone(), "some")?;
    if h.some(some, |m, _| Ok(h.equal(&m, &h.intrinsic("null", vec![])?)))? {
        emit(h, r, || Ok(h.literal("Failed to fetch markets")))?;
        return p(h, &p(h, r, "store")?, "markets");
    }
    Ok(markets)
}
// ts: BybitSpotExchange.fetchOHLCV
pub async fn fetch_ohlcv<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    fetch_ohlcv_for_category(h, r, opts, |_| Ok(h.literal("spot")), true, |_, _| Ok(())).await
}

pub(crate) async fn fetch_ohlcv_for_category<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    opts: H::Value,
    category: impl FnOnce(&H::Value) -> Result<H::Value, H::Error>,
    optional_mapping: bool,
    validate_response: impl FnOnce(&H::Value, &H::Value) -> Result<(), H::Error>,
) -> Result<H::Value, H::Error> {
    let min = h.select(p(h, &h.global(), "Math")?, "min")?;
    let limit = h.apply(
        min,
        vec![
            or(h, p(h, &opts, "limit")?, || Ok(h.number(500.)))?,
            h.number(1000.),
        ],
    )?;
    let interval = key(h, p(h, &h.global(), "INTERVAL")?, p(h, &opts, "interval")?)?;
    let split = h.select(p(h, &opts, "interval")?, "split")?;
    let split = h.apply(split, vec![h.intrinsic("intervalRegex", vec![])?])?;
    let parts = h.destructure(split, 1, 2)?;
    let amount = parts[0].clone();
    let unit = parts[1].clone();
    let end = if h.truthy(&p(h, &opts, "to")?) {
        call!(h, h.global(), "dayjs", vec![p(h, &opts, "to")?])
    } else {
        call!(h, h.global(), "dayjs", vec![])
    };
    let start = if !h.truthy(&p(h, &opts, "limit")?) && h.truthy(&p(h, &opts, "from")?) {
        call!(h, h.global(), "dayjs", vec![p(h, &opts, "from")?])
    } else {
        let sub = h.select(end.clone(), "subtract")?;
        let parsed = parse(h, || Ok(amount))?;
        h.apply(
            sub,
            vec![numeric(h, "multiply", parsed, limit.clone())?, unit],
        )?
    };
    let params = h.object(vec![
        ("category", category(&opts)?),
        ("symbol", p(h, &opts, "symbol")?),
        ("start", call!(h, start, "valueOf", vec![])),
        ("end", call!(h, end, "valueOf", vec![])),
        ("interval", interval),
        ("limit", limit),
    ]);
    let get = h.select(p(h, r, "xhr")?, "get")?;
    let e = constant(h, "ENDPOINTS", "KLINE")?;
    let response = h
        .wait(h.apply(get, vec![e, h.object(vec![("params", params)])])?)
        .await?;
    let data = p(h, &response, "data")?;
    validate_response(&response, &data)?;
    let order_by = h.select(h.global(), "orderBy")?;
    let list = h.optional(&h.optional(&data, "result")?, "list")?;
    let candles = if optional_mapping && nullish(h, &list)? {
        h.undefined()
    } else {
        let map = p(h, &list, "map")?;
        if optional_mapping && nullish(h, &map)? {
            h.undefined()
        } else {
            let selected = h.intrinsic("capturedMember", vec![list, map])?;
            h.map(selected, |row, _| {
                let row = h.destructure(row, 0, 6)?;
                let timestamp = numeric(
                    h,
                    "divide",
                    parse(h, || Ok(row[0].clone()))?,
                    h.number(1000.),
                )?;
                Ok(h.object(vec![
                    ("timestamp", timestamp),
                    ("open", parse(h, || Ok(row[1].clone()))?),
                    ("high", parse(h, || Ok(row[2].clone()))?),
                    ("low", parse(h, || Ok(row[3].clone()))?),
                    ("close", parse(h, || Ok(row[4].clone()))?),
                    ("volume", parse(h, || Ok(row[5].clone()))?),
                ]))
            })?
        }
    };
    h.apply(
        order_by,
        vec![
            candles,
            h.array(vec![h.literal("timestamp")]),
            h.array(vec![h.literal("asc")]),
        ],
    )
}
// ts: BybitSpotExchange.mapOrder
pub fn map_order<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    raw: H::Value,
) -> Result<H::Value, H::Error> {
    let stop = v(h, raw.clone(), "stopOrderType")?;
    let mode = v(h, raw.clone(), "tpslMode")?;
    let position = eqs(h, &mode, "Full") || eqs(h, &mode, "Partial");
    let created = if position {
        let table = p(h, &h.global(), "POSITION_TPSL_ORDER_TYPE_FROM_CREATE_TYPE")?;
        key(h, table, v(h, raw.clone(), "createType")?)?
    } else {
        h.undefined()
    };
    let stopped = (h.truthy(&stop) && !eqs(h, &stop, "UNKNOWN")) || h.truthy(&created);
    let trigger = v(h, raw.clone(), "triggerPrice")?;
    let price = if stopped {
        or(h, trigger, || p(h, &raw, "price"))?
    } else {
        p(h, &raw, "price")?
    };
    let raw_type = or(h, created, || {
        if stopped {
            Ok(stop)
        } else {
            v(h, raw.clone(), "orderType")
        }
    })?;
    let kind = key(h, p(h, &h.global(), "ORDER_TYPE")?, raw_type.clone())?;
    let tif = key(
        h,
        p(h, &h.global(), "ORDER_TIME_IN_FORCE")?,
        p(h, &raw, "timeInForce")?,
    )?;
    let mut remaining = if h.truthy(&p(h, &raw, "leavesQty")?) {
        parse(h, || p(h, &raw, "leavesQty"))?
    } else {
        let sub = h.select(h.global(), "subtract")?;
        let qty = coalesce(h, p(h, &raw, "qty")?, || Ok(h.number(0.)))?;
        let fill = coalesce(h, v(h, raw.clone(), "cumExecQty")?, || Ok(h.number(0.)))?;
        h.apply(sub, vec![qty, fill])?
    };
    let native = remaining.clone();
    let price = parse(h, || Ok(price))?;
    let market = key(
        h,
        p(h, &p(h, r, "store")?, "marketsBySymbol")?,
        p(h, &raw, "symbol")?,
    )?;
    if h.truthy(&h.optional(&market, "isInverse")?) {
        let adjust = h.select(h.global(), "adjust")?;
        let value = numeric(h, "divide", remaining, price.clone())?;
        remaining = h.apply(adjust, vec![value, h.number(0.0000001)])?;
    }
    let id = p(h, &raw, "orderId")?;
    let status = key(
        h,
        p(h, &h.global(), "ORDER_STATUS")?,
        v(h, raw.clone(), "orderStatus")?,
    )?;
    let symbol = p(h, &raw, "symbol")?;
    let side = key(h, p(h, &h.global(), "ORDER_SIDE")?, p(h, &raw, "side")?)?;
    let amount = if (eqs(h, &raw_type, "StopLoss") || eqs(h, &raw_type, "TakeProfit"))
        && eqs(h, &mode, "Full")
    {
        h.number(0.)
    } else {
        parse(h, || coalesce(h, p(h, &raw, "qty")?, || Ok(h.number(0.))))?
    };
    let filled = parse(h, || {
        coalesce(h, v(h, raw.clone(), "cumExecQty")?, || Ok(h.number(0.)))
    })?;
    let reduce = or(h, v(h, raw.clone(), "reduceOnly")?, || Ok(h.boolean(false)))?;
    let now = call!(
        h,
        p(h, &h.global(), "virtualClock")?,
        "getCurrentTimeMs",
        vec![]
    );
    let order = h.object(vec![
        ("id", id),
        ("status", status),
        ("symbol", symbol),
        ("type", kind.clone()),
        ("side", side.clone()),
        ("remaining", remaining),
        ("extra", h.object(vec![("remainingNative", native)])),
        ("price", price),
        ("amount", amount.clone()),
        ("filled", filled),
        ("reduceOnly", reduce),
        ("info", raw.clone()),
        ("timeInForce", tif),
        ("lastUpdate", now),
    ]);
    if is(h, &kind, "OrderType", "TakeProfit")? {
        let table = p(h, &h.global(), "ORDER_TRIGGER_TYPE")?;
        h.define(
            &order,
            "tpTriggerType",
            key(h, table, v(h, raw.clone(), "triggerBy")?)?,
        )?;
    }
    if is(h, &kind, "OrderType", "StopLoss")? {
        let table = p(h, &h.global(), "ORDER_TRIGGER_TYPE")?;
        h.define(
            &order,
            "slTriggerType",
            key(h, table, v(h, raw.clone(), "triggerBy")?)?,
        )?;
    }
    let mut orders = vec![order.clone()];
    let sl = parse(h, || v(h, raw.clone(), "stopLoss"))?;
    let tp = parse(h, || v(h, raw.clone(), "takeProfit"))?;
    let inverse = constant(
        h,
        "OrderSide",
        if is(h, &side, "OrderSide", "Buy")? {
            "Sell"
        } else {
            "Buy"
        },
    )?;
    for (which, value) in [("sl", sl), ("tp", tp)] {
        if h.truthy(&numeric(h, "greater", value.clone(), h.number(0.))?) {
            let copy = h.spread(order.clone())?;
            let id = h.intrinsic(
                "suffixTemplate",
                vec![
                    p(h, &raw, "orderId")?,
                    h.literal(if which == "sl" {
                        "__stop_loss"
                    } else {
                        "__take_profit"
                    }),
                ],
            )?;
            h.define(&copy, "id", id)?;
            h.define(
                &copy,
                "type",
                constant(
                    h,
                    "OrderType",
                    if which == "sl" {
                        "StopLoss"
                    } else {
                        "TakeProfit"
                    },
                )?,
            )?;
            h.define(&copy, "side", inverse.clone())?;
            h.define(&copy, "price", value)?;
            h.define(&copy, "filled", h.number(0.))?;
            h.define(&copy, "remaining", p(h, &order, "amount")?)?;
            let now = call!(
                h,
                p(h, &h.global(), "virtualClock")?,
                "getCurrentTimeMs",
                vec![]
            );
            h.define(&copy, "lastUpdate", now)?;
            let trigger = key(
                h,
                p(h, &h.global(), "ORDER_TRIGGER_TYPE")?,
                v(
                    h,
                    raw.clone(),
                    if which == "sl" {
                        "slTriggerBy"
                    } else {
                        "tpTriggerBy"
                    },
                )?,
            )?;
            h.define(
                &copy,
                if which == "sl" {
                    "slTriggerType"
                } else {
                    "tpTriggerType"
                },
                trigger,
            )?;
            orders.push(copy);
        }
    }
    Ok(h.array(orders))
}
fn adjust<H: SpotExchangeHost>(
    h: &H,
    value: impl FnOnce() -> Result<H::Value, H::Error>,
    precision: H::Value,
) -> Result<H::Value, H::Error> {
    let f = h.select(h.global(), "adjust")?;
    h.apply(f, vec![value()?, precision])
}
fn optional_adjust<H: SpotExchangeHost>(
    h: &H,
    opts: &H::Value,
    k: &str,
    precision: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&p(h, opts, k)?) {
        adjust(h, || p(h, opts, k), precision)
    } else {
        h.intrinsic("null", vec![])
    }
}
fn inverse<H: SpotExchangeHost>(
    h: &H,
    table: &str,
    k: impl FnOnce() -> Result<H::Value, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.select(h.global(), "inverseObj")?;
    let table = h.apply(f, vec![p(h, &h.global(), table)?])?;
    key(h, table, k()?)
}
fn when_template<H: SpotExchangeHost>(
    h: &H,
    condition: H::Value,
    value: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&condition) {
        template(h, value)
    } else {
        Ok(h.undefined())
    }
}
// ts: BybitSpotExchange.placeOrder
pub async fn place_order<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let client = call!(
        h,
        r.clone(),
        "resolveCallerClientOrderId",
        vec![opts.clone()]
    );
    let execution = call!(h, r.clone(), "getOrderExecutionParams", vec![opts.clone()]);
    if h.truthy(&call!(
        h,
        r.clone(),
        "_hasFullSizedPositionOppositeSide",
        vec![
            p(h, &opts, "symbol")?,
            p(h, &opts, "side")?,
            p(h, &opts, "amount")?
        ]
    )) && (is(h, &p(h, &opts, "type")?, "OrderType", "StopLoss")?
        || is(h, &p(h, &opts, "type")?, "OrderType", "TakeProfit")?)
    {
        return awaited(h, r, "placeStopLossOrTakeProfit", vec![opts]).await;
    }
    if is(h, &p(h, &opts, "type")?, "OrderType", "TrailingStopLoss")? {
        return awaited(h, r, "placeTrailingStopLoss", vec![opts]).await;
    }
    let market = key(
        h,
        p(h, &p(h, r, "store")?, "marketsBySymbol")?,
        p(h, &opts, "symbol")?,
    )?;
    if !h.truthy(&market) {
        return Err(error(h, || {
            h.intrinsic("missingMarket", vec![p(h, &opts, "symbol")?])
        })?);
    }
    let index = awaited(h, r, "getOrderPositionIdx", vec![opts.clone()]).await?;
    let max = p(h, &p(h, &p(h, &market, "limits")?, "amount")?, "max")?;
    let pp = p(h, &p(h, &market, "precision")?, "price")?;
    let pa = p(h, &p(h, &market, "precision")?, "amount")?;
    let amount = adjust(h, || p(h, &opts, "amount"), pa.clone())?;
    let price = optional_adjust(h, &opts, "price", pp.clone())?;
    let sl = optional_adjust(h, &opts, "stopLoss", pp.clone())?;
    let tp = optional_adjust(h, &opts, "takeProfit", pp.clone())?;
    let tif = key(
        h,
        p(h, &h.global(), "ORDER_TIME_IN_FORCE_OUTBOUND")?,
        or(h, p(h, &opts, "timeInForce")?, || {
            constant(h, "OrderTimeInForce", "GoodTillCancel")
        })?,
    )?;
    let trigger = optional_adjust(h, &opts, "triggerPrice", pp)?;
    let direction = if h.truthy(&trigger) {
        let ticker = key(
            h,
            p(h, &p(h, r, "store")?, "tickersBySymbol")?,
            p(h, &opts, "symbol")?,
        )?;
        h.number(
            if h.truthy(&numeric(
                h,
                "greater",
                trigger.clone(),
                p(h, &ticker, "mark")?,
            )?) {
                1.
            } else {
                2.
            },
        )
    } else {
        h.undefined()
    };
    let mut kind = p(h, &opts, "type")?;
    if h.truthy(&trigger)
        && (is(h, &p(h, &opts, "type")?, "OrderType", "StopLoss")?
            || is(h, &p(h, &opts, "type")?, "OrderType", "TakeProfit")?)
    {
        kind = constant(h, "OrderType", "Market")?;
    }
    let omit = h.select(h.global(), "omitUndefined")?;
    let symbol = p(h, &opts, "symbol")?;
    let side = inverse(h, "ORDER_SIDE", || p(h, &opts, "side"))?;
    let ty = inverse(h, "ORDER_TYPE", || Ok(kind))?;
    let qty = template(h, amount.clone())?;
    let request_price = if is(h, &p(h, &opts, "type")?, "OrderType", "Limit")? {
        template(h, price)?
    } else {
        h.undefined()
    };
    let stop_loss = when_template(h, p(h, &opts, "stopLoss")?, sl)?;
    let take_profit = when_template(h, p(h, &opts, "takeProfit")?, tp)?;
    let reduce = or(h, p(h, &opts, "reduceOnly")?, || Ok(h.boolean(false)))?;
    let trigger_price = when_template(h, p(h, &opts, "triggerPrice")?, trigger)?;
    let trigger_direction = when_template(h, direction.clone(), direction)?;
    let trigger_by = if is(h, &p(h, &opts, "type")?, "OrderType", "StopLoss")? {
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
        )
    } else if is(h, &p(h, &opts, "type")?, "OrderType", "TakeProfit")? {
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
        )
    } else {
        h.undefined()
    };
    let sl_by = if h.truthy(&p(h, &opts, "stopLoss")?) {
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
        )
    } else {
        h.undefined()
    };
    // ts-bug: BybitSpotExchange.placeOrder consults slTriggerType for attached TP.
    let tp_by = if h.truthy(&p(h, &opts, "takeProfit")?) {
        h.literal(
            if is(
                h,
                &p(h, &opts, "slTriggerType")?,
                "OrderStopTriggerType",
                "MarkPrice",
            )? {
                "MarkPrice"
            } else {
                "LastPrice"
            },
        )
    } else {
        h.undefined()
    };
    let time = if is(h, &p(h, &opts, "type")?, "OrderType", "Limit")? {
        tif
    } else {
        h.undefined()
    };
    let req = h.object(vec![
        ("category", h.literal("spot")),
        ("symbol", symbol),
        ("side", side),
        ("orderType", ty),
        ("qty", qty),
        ("price", request_price),
        ("stopLoss", stop_loss),
        ("takeProfit", take_profit),
        ("reduceOnly", reduce),
        ("triggerPrice", trigger_price),
        ("triggerDirection", trigger_direction),
        ("triggerBy", trigger_by),
        ("slTriggerBy", sl_by),
        ("tpTriggerBy", tp_by),
        ("timeInForce", time),
        ("closeOnTrigger", h.boolean(false)),
        ("positionIdx", index),
    ]);
    let req = mix(h, req, execution)?;
    let req = h.apply(omit, vec![req])?;
    let lots = if h.truthy(&numeric(h, "greater", amount.clone(), max.clone())?) {
        let ceil = h.select(p(h, &h.global(), "Math")?, "ceil")?;
        h.apply(
            ceil,
            vec![numeric(h, "divide", amount.clone(), max.clone())?],
        )?
    } else {
        h.number(1.)
    };
    let rest = if h.truthy(&numeric(h, "greater", amount.clone(), max.clone())?) {
        let f = h.select(h.global(), "adjust")?;
        h.apply(
            f,
            vec![numeric(h, "remainder", amount.clone(), max)?, pa.clone()],
        )?
    } else {
        h.number(0.)
    };
    let f = h.select(h.global(), "adjust")?;
    let size = numeric(h, "subtract", amount, rest.clone())?;
    let size = numeric(h, "divide", size, lots.clone())?;
    let size = h.apply(f, vec![size, pa])?;
    let times = h.select(h.global(), "times")?;
    let payloads = h.times(times, lots, |i| {
        let payload = if h.truthy(&numeric(h, "greater", i, h.number(0.))?) {
            call!(
                h,
                h.global(),
                "omit",
                vec![
                    req.clone(),
                    h.array(vec![
                        h.literal("stopLoss"),
                        h.literal("takeProfit"),
                        h.literal("slTriggerBy"),
                        h.literal("tpTriggerBy")
                    ])
                ]
            )
        } else {
            req.clone()
        };
        let payload = h.spread(payload)?;
        h.define(&payload, "qty", template(h, size.clone())?)?;
        if !h.equal(&client, &h.undefined()) {
            h.define(&payload, "orderLinkId", client.clone())?;
        }
        Ok(payload)
    })?;
    if h.truthy(&rest) {
        let push = h.select(payloads.clone(), "push")?;
        let payload = h.spread(req)?;
        h.define(&payload, "qty", template(h, rest)?)?;
        h.apply(push, vec![payload])?;
    }
    call!(
        h,
        r.clone(),
        "assertSingleNativeClientOrderId",
        vec![client.clone(), p(h, &payloads, "length")?]
    );
    let map = h.select(h.global(), "mapSeries")?;
    let responses = h
        .series(map, payloads.clone(), |payload, _| {
            Box::pin({
                let client = client.clone();
                let payloads = payloads.clone();
                async move {
                    let response = async {
                        let post = h.select(p(h, r, "xhr")?, "post")?;
                        let e = constant(h, "ENDPOINTS", "CREATE_ORDER")?;
                        let response = h.wait(h.apply(post, vec![e, payload])?).await?;
                        p(h, &response, "data")
                    }
                    .await;
                    match response {
                        Ok(v) => Ok(v),
                        Err(e) => {
                            emit(h, r, || {
                                let value = h.error_value(e.clone());
                                or(
                                    h,
                                    h.optional(
                                        &h.optional(&h.optional(&value, "response")?, "data")?,
                                        "retMsg",
                                    )?,
                                    || p(h, &value, "message"),
                                )
                            })?;
                            if h.equal(&p(h, &payloads, "length")?, &h.number(1.))
                                && !h.equal(&client, &h.undefined())
                                && h.truthy(&call!(
                                    h,
                                    h.global(),
                                    "isUnknownOutcome",
                                    vec![h.error_value(e.clone())]
                                ))
                            {
                                return Err(e);
                            }
                            Ok(h.undefined())
                        }
                    }
                }
            })
        })
        .await?;
    let filter = h.select(responses, "filter")?;
    let fulfilled = h.filter(filter, |v, _| Ok(!h.equal(&v, &h.undefined())))?;
    let mut err = h.literal("");
    let each = h.select(fulfilled.clone(), "forEach")?;
    h.for_each(each, |response, _| {
        if !eqs(h, &v(h, response.clone(), "retMsg")?, "OK") {
            emit(h, r, || v(h, response.clone(), "retMsg"))?;
            err = v(h, response, "retMsg")?;
        }
        Ok(())
    })?;
    if !eqs(h, &err, "") {
        return Err(error(h, || Ok(err))?);
    }
    let map = h.select(fulfilled, "map")?;
    h.map(map, |response, i| {
        let id = p(h, &p(h, &response, "result")?, "orderId")?;
        let payload = p(h, &payloads, &i.to_string())?;
        if h.truthy(&id) && h.truthy(&payload) {
            call!(
                h,
                r.clone(),
                "recordRecentlyPlacedOrderFromOpts",
                vec![id.clone(), opts.clone(), payload]
            );
        }
        Ok(id)
    })
}
fn ends<H: SpotExchangeHost>(h: &H, o: &H::Value, suffix: &str) -> Result<bool, H::Error> {
    Ok(h.truthy(&call!(
        h,
        p(h, o, "id")?,
        "endsWith",
        vec![h.literal(suffix)]
    )))
}
fn has<H: SpotExchangeHost>(h: &H, obj: H::Value, k: &str) -> Result<bool, H::Error> {
    Ok(h.truthy(&h.intrinsic("has", vec![obj, h.literal(k)])?))
}
async fn post<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    name: &str,
    payload: H::Value,
) -> Result<H::Value, H::Error> {
    let f = h.select(p(h, r, "xhr")?, "post")?;
    let e = constant(h, "ENDPOINTS", name)?;
    let response = h.wait(h.apply(f, vec![e, payload])?).await?;
    p(h, &response, "data")
}
fn local<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    order: H::Value,
    update: H::Value,
) -> Result<(), H::Error> {
    call!(
        h,
        r.clone(),
        "applyLocalOrderUpdateAfterRestWrite",
        vec![order, update]
    );
    Ok(())
}
// ts: BybitSpotExchange.updateOrder; input destructuring remains native property lookup.
pub async fn update_order<H: SpotExchangeHost>(
    h: &H,
    r: &H::Value,
    input: H::Value,
) -> Result<H::Value, H::Error> {
    let order = p(h, &input, "order")?;
    let update = p(h, &input, "update")?;
    let virtual_ = ends(h, &order, "__stop_loss")? || ends(h, &order, "__take_profit")?;
    if is(h, &p(h, &order, "type")?, "OrderType", "TrailingStopLoss")? {
        awaited(h, r, "cancelOrders", vec![h.array(vec![order.clone()])]).await?;
        let place = h.select(r.clone(), "placeTrailingStopLoss")?;
        let merged = mix(h, order.clone(), update.clone())?;
        let ids = h.wait(h.apply(place, vec![merged])?).await?;
        local(h, r, order, update)?;
        return Ok(ids);
    }
    if !virtual_
        && (is(h, &p(h, &order, "type")?, "OrderType", "TakeProfit")?
            || is(h, &p(h, &order, "type")?, "OrderType", "StopLoss")?)
        && has(h, update.clone(), "price")?
    {
        let ticker = key(
            h,
            p(h, &p(h, r, "store")?, "tickersBySymbol")?,
            p(h, &order, "symbol")?,
        )?;
        let mark = h.optional(&ticker, "mark")?;
        if h.truthy(&mark) {
            let tp = if is(h, &p(h, &order, "side")?, "OrderSide", "Sell")? {
                h.truthy(&numeric(h, "greater", p(h, &update, "price")?, mark)?)
            } else {
                h.truthy(&numeric(h, "lessEqual", p(h, &update, "price")?, mark)?)
            };
            let projected = constant(h, "OrderType", if tp { "TakeProfit" } else { "StopLoss" })?;
            awaited(h, r, "cancelOrders", vec![h.array(vec![order.clone()])]).await?;
            let f = h.select(r.clone(), "placeStopLossOrTakeProfit")?;
            let merged = mix(h, order.clone(), update.clone())?;
            h.define(&merged, "type", projected.clone())?;
            let ids = h.wait(h.apply(f, vec![merged])?).await?;
            let local = h.select(r.clone(), "applyLocalOrderUpdateAfterRestWrite")?;
            let updated = h.spread(update)?;
            h.define(&updated, "type", projected)?;
            h.apply(local, vec![order, updated])?;
            return Ok(ids);
        }
    }
    let mut ids = vec![];
    if has(h, update.clone(), "price")? && virtual_ {
        let find = h.select(p(h, &p(h, r, "store")?, "orders")?, "find")?;
        let original = h.find(find, |o, _| {
            let id = p(h, &o, "id")?;
            let stripped = call!(
                h,
                p(h, &order, "id")?,
                "replace",
                vec![h.literal("__stop_loss"), h.literal("")]
            );
            let stripped = call!(
                h,
                stripped,
                "replace",
                vec![h.literal("__take_profit"), h.literal("")]
            );
            Ok(h.equal(&id, &stripped))
        })?;
        if h.truthy(&original) {
            let name = if ends(h, &order, "__stop_loss")? {
                "stopLoss"
            } else {
                "takeProfit"
            };
            let payload = h.object(vec![
                ("category", h.literal("spot")),
                ("orderId", p(h, &original, "id")?),
                ("symbol", p(h, &order, "symbol")?),
                (name, template(h, p(h, &update, "price")?)?),
            ]);
            let result = async {
                let data = post(h, r, "REPLACE_ORDER", payload).await?;
                if eqs(h, &p(h, &data, "retMsg")?, "OK") {
                    ids.push(p(h, &p(h, &data, "result")?, "orderId")?);
                    local(h, r, order.clone(), update.clone())?;
                } else if !eqs(h, &p(h, &data, "retMsg")?, "order not modified") {
                    emit(h, r, || p(h, &data, "retMsg"))?;
                    return Err(error(h, || p(h, &data, "retMsg"))?);
                }
                Ok(())
            }
            .await;
            if let Err(e) = result {
                let emit = h.select(r.clone(), "emitAndLogError")?;
                let value = h.error_value(e.clone());
                let detail = or(
                    h,
                    h.optional(
                        &h.optional(&h.optional(&value, "response")?, "data")?,
                        "retMsg",
                    )?,
                    || p(h, &value, "message"),
                )?;
                h.apply(emit, vec![value, detail])?;
                return Err(e);
            }
        }
    }
    let conditional = !virtual_
        && (is(h, &p(h, &order, "type")?, "OrderType", "StopLoss")?
            || is(h, &p(h, &order, "type")?, "OrderType", "TakeProfit")?)
        && h.truthy(&h.optional(&p(h, &order, "info")?, "triggerPrice")?);
    if is(h, &p(h, &order, "type")?, "OrderType", "Limit")? || conditional {
        let payload = h.object(vec![
            ("category", h.literal("spot")),
            ("orderId", p(h, &order, "id")?),
            ("symbol", p(h, &order, "symbol")?),
        ]);
        if has(h, update.clone(), "amount")? {
            h.set(&payload, "qty", template(h, p(h, &update, "amount")?)?)?;
        }
        if has(h, update.clone(), "price")? {
            h.set(
                &payload,
                if conditional { "triggerPrice" } else { "price" },
                template(h, p(h, &update, "price")?)?,
            )?;
        }
        let data = post(h, r, "REPLACE_ORDER", payload).await?;
        if eqs(h, &p(h, &data, "retMsg")?, "OK") {
            ids.push(p(h, &p(h, &data, "result")?, "orderId")?);
            local(h, r, order.clone(), update.clone())?;
        } else if !eqs(h, &p(h, &data, "retMsg")?, "order not modified") {
            emit(h, r, || p(h, &data, "retMsg"))?;
        }
    }
    if !conditional
        && !virtual_
        && (is(h, &p(h, &order, "type")?, "OrderType", "StopLoss")?
            || is(h, &p(h, &order, "type")?, "OrderType", "TakeProfit")?)
    {
        let category = h.literal("spot");
        let symbol = p(h, &order, "symbol")?;
        let index = awaited(h, r, "getOrderPositionIdx", vec![order.clone()]).await?;
        let payload = h.object(vec![
            ("category", category),
            ("symbol", symbol),
            ("positionIdx", index),
        ]);
        if has(h, update.clone(), "price")? {
            if is(h, &p(h, &order, "type")?, "OrderType", "StopLoss")? {
                h.set(&payload, "stopLoss", template(h, p(h, &update, "price")?)?)?;
            } else if is(h, &p(h, &order, "type")?, "OrderType", "TakeProfit")? {
                h.set(
                    &payload,
                    "takeProfit",
                    template(h, p(h, &update, "price")?)?,
                )?;
            }
        }
        let data = post(h, r, "SET_TRADING_STOP", payload).await?;
        if eqs(h, &p(h, &data, "retMsg")?, "order not modified") {
        } else if !eqs(h, &p(h, &data, "retMsg")?, "OK") {
            emit(h, r, || p(h, &data, "retMsg"))?;
        } else {
            local(h, r, order, update)?;
        }
    }
    Ok(h.array(ids))
}
