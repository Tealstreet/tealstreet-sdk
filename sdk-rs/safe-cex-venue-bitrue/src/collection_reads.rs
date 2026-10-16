//! Account, order reconciliation and trade-history bodies over source bindings.
use std::{future::Future, rc::Rc};
pub type Callback<H> = Rc<
    dyn Fn(Vec<<H as ReadHost>::Value>) -> Result<<H as ReadHost>::Value, <H as ReadHost>::Error>,
>;
#[derive(Clone, Copy)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Min,
    Max,
}
pub trait ReadHost: Clone + 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    fn undefined(&self) -> Self::Value;
    fn number(&self, n: f64) -> Self::Value;
    fn text(&self, s: &str) -> Self::Value;
    fn boolean(&self, b: bool) -> Self::Value;
    fn get(&self, v: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn set(&self, v: &Self::Value, key: &str, next: Self::Value) -> Result<(), Self::Error>;
    fn truthy(&self, v: &Self::Value) -> bool;
    fn nullish(&self, v: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn set_equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn is_array(&self, v: &Self::Value) -> bool;
    fn is_boolean(&self, v: &Self::Value) -> bool;
    fn is_finite(&self, v: &Self::Value) -> bool;
    fn numeric(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn string(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn uppercase(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn concat(&self, v: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    fn arithmetic(
        &self,
        op: Op,
        a: Self::Value,
        b: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn less(&self, a: Self::Value, b: Self::Value) -> Result<bool, Self::Error>;
    fn abs(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn floor(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn date_parse(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn split_at(
        &self,
        v: Self::Value,
        separator: &str,
        index: usize,
    ) -> Result<Self::Value, Self::Error>;
    fn array(&self, v: Vec<Self::Value>) -> Self::Value;
    fn object(&self, v: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn assign_spread(&self, target: Self::Value, value: Self::Value) -> Result<(), Self::Error>;
    fn omit_undefined(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn items(&self, v: Self::Value) -> Result<Vec<Self::Value>, Self::Error>;
    fn map(
        &self,
        v: Self::Value,
        f: impl FnMut(Self::Value) -> Result<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn filter(
        &self,
        v: Self::Value,
        f: impl FnMut(Self::Value) -> Result<bool, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn some(
        &self,
        v: Self::Value,
        f: impl FnMut(Self::Value) -> Result<bool, Self::Error>,
    ) -> Result<bool, Self::Error>;
    fn includes(&self, v: Self::Value, needle: Self::Value) -> Result<bool, Self::Error>;
    fn flatten(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn sort_time_desc(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn slice_limit(&self, v: Self::Value, limit: Self::Value) -> Result<Self::Value, Self::Error>;
    fn table(&self, name: &str, key: Self::Value) -> Result<Self::Value, Self::Error>;
    fn call(
        &self,
        f: Self::Value,
        receiver: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn function(&self, f: Callback<Self>) -> Self::Value;
    fn map_series(
        &self,
        values: Self::Value,
        callback: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn await_value(&self, v: Self::Value)
    -> impl Future<Output = Result<Self::Value, Self::Error>>;
    fn observe_detached(&self, v: Self::Value);
    fn sleep(&self, ms: f64) -> Result<Self::Value, Self::Error>;
    fn console(&self, method: &str, message: &str) -> Result<(), Self::Error>;
    fn error(&self, message: Self::Value) -> Result<Self::Error, Self::Error>;
    fn error_value(&self, error: &Self::Error) -> Self::Value;
    fn position_id(
        &self,
        symbol: Self::Value,
        side: Self::Value,
        mode: Self::Value,
    ) -> Result<Self::Value, Self::Error>;
}
fn get<H: ReadHost>(h: &H, v: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    h.get(v, key)
}
fn opt<H: ReadHost>(h: &H, v: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, key)
    }
}
fn fallback<H: ReadHost>(h: &H, v: H::Value, to: H::Value) -> H::Value {
    if h.truthy(&v) { v } else { to }
}
fn nullish<H: ReadHost>(h: &H, v: H::Value, to: H::Value) -> H::Value {
    if h.nullish(&v) { to } else { v }
}
fn method<H: ReadHost>(h: &H, r: H::Value, name: &str) -> Result<(H::Value, H::Value), H::Error> {
    Ok((get(h, &r, name)?, r))
}
fn invoke<H: ReadHost>(
    h: &H,
    p: (H::Value, H::Value),
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.call(p.0, p.1, args)
}
fn call<H: ReadHost>(
    h: &H,
    r: H::Value,
    name: &str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    invoke(h, method(h, r, name)?, args)
}
fn empty<H: ReadHost>(h: &H) -> H::Value {
    h.array(vec![])
}
fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == name)
        .expect("source endpoint")
        .1
}
fn limit<H: ReadHost>(h: &H, opts: &H::Value) -> Result<H::Value, H::Error> {
    let v = fallback(h, get(h, opts, "limit")?, h.number(200.0));
    let v = h.arithmetic(Op::Max, v, h.number(1.0))?;
    h.arithmetic(Op::Min, v, h.number(1000.0))
}
fn guard<H: ReadHost>(h: &H, r: &H::Value, opts: &H::Value) -> Result<bool, H::Error> {
    Ok(h.truthy(&get(h, r, "isPublicDataOnly")?) || !h.truthy(&get(h, opts, "symbol")?))
}
fn success<H: ReadHost>(h: &H, r: &H::Value, data: &H::Value) -> Result<bool, H::Error> {
    let p = method(h, r.clone(), "isSuccessCode")?;
    Ok(h.truthy(&invoke(h, p, vec![opt(h, data, "code")?])?))
}
fn log_error<H: ReadHost>(h: &H, r: H::Value, e: H::Error) -> Result<(), H::Error> {
    let p = method(h, r, "emitAndLogError")?;
    let e = h.error_value(&e);
    let msg = opt(h, &opt(h, &opt(h, &e, "response")?, "data")?, "msg")?;
    let msg = if h.truthy(&msg) {
        msg
    } else {
        opt(h, &e, "message")?
    };
    invoke(h, p, vec![msg])?;
    Ok(())
}
pub async fn open_positions<H: ReadHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    let p = call(h, r.clone(), "fetchBalanceAndPositionsRaw", vec![])?;
    let data = h.await_value(p).await?;
    let p = call(h, r, "parsePositions", vec![data])?;
    h.await_value(p).await
}
pub async fn positions<H: ReadHost>(
    h: &H,
    r: H::Value,
    symbols: H::Value,
) -> Result<H::Value, H::Error> {
    let p = call(h, r.clone(), "fetchOpenPositions", vec![])?;
    let open = h.await_value(p).await?;
    if !h.truthy(&symbols) {
        return Ok(open);
    }
    let selected = h.filter(open, |p| h.includes(symbols.clone(), get(h, &p, "symbol")?))?;
    let missing = h.filter(symbols, |s| {
        let found = h.some(
            selected.clone(),
            |p| Ok(h.equal(&get(h, &p, "symbol")?, &s)),
        )?;
        Ok(!found)
    })?;
    let p = call(h, r, "fetchPositionMetadata", vec![missing])?;
    let metadata = h.await_value(p).await?;
    let fake = h.map(metadata, |m| {
        let id = h.position_id(
            get(h, &m, "symbol")?,
            get(h, &m, "side")?,
            get(h, &m, "positionMode")?,
        )?;
        let obj = h.object(vec![("id", id)]);
        h.assign_spread(obj.clone(), m)?;
        for k in [
            "entryPrice",
            "notional",
            "unrealizedPnl",
            "contracts",
            "liquidationPrice",
        ] {
            h.set(&obj, k, h.number(0.0))?;
        }
        Ok(obj)
    })?;
    let mut out = h.items(selected)?;
    out.extend(h.items(fake)?);
    Ok(h.array(out))
}
pub async fn balance_positions<H: ReadHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    let p = call(h, r.clone(), "fetchBalanceAndPositionsRaw", vec![])?;
    let data = h.await_value(p).await?;
    let p = call(h, r.clone(), "parsePositions", vec![data.clone()])?;
    let positions = h.await_value(p).await?;
    let balance = call(h, r, "parseBalance", vec![data])?;
    Ok(h.object(vec![("balance", balance), ("positions", positions)]))
}
pub async fn orders<H: ReadHost>(
    h: &H,
    r: H::Value,
    symbols: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&opt(h, &symbols, "length")?) {
        let host = h.clone();
        let callback = h.function(Rc::new(move |args| {
            let symbol = args.first().cloned().unwrap_or_else(|| host.undefined());
            call(&host, r.clone(), "fetchOrdersForSymbol", vec![symbol])
        }));
        let p = h.map_series(symbols, callback)?;
        let orders = h.await_value(p).await?;
        h.flatten(orders)
    } else {
        h.console("warn", "No symbols provided... fetching async")?;
        let p = call(h, r, "fetchAndEmitOrders", vec![])?;
        h.observe_detached(p);
        Ok(empty(h))
    }
}
pub async fn emit_orders<H: ReadHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    let id = h.arithmetic(
        Op::Add,
        get(h, &r, "inProgressOrderFetchId")?,
        h.number(1.0),
    )?;
    h.set(&r, "inProgressOrderFetchId", id.clone())?;
    let markets = get(h, &get(h, &r, "store")?, "markets")?;
    let symbols = h.map(markets, |m| get(h, &m, "symbol"))?;
    let mut to_fetch = vec![];
    for s in h.items(symbols)? {
        if !to_fetch.iter().any(|v| h.set_equal(v, &s)) {
            to_fetch.push(s)
        }
    }
    while !to_fetch.is_empty() {
        if !h.equal(&get(h, &r, "inProgressOrderFetchId")?, &id) {
            h.console("log", "fetchAndEmitOrders: fetch cancelled")?;
            break;
        }
        let priorities = h.items(get(h, &r, "prioritySymbols")?)?;
        let priority = priorities
            .into_iter()
            .find(|p| to_fetch.iter().any(|s| h.set_equal(s, p)));
        let next = fallback(
            h,
            priority.unwrap_or_else(|| h.undefined()),
            to_fetch[0].clone(),
        );
        if !h.truthy(&next) {
            break;
        }
        to_fetch.retain(|v| !h.set_equal(v, &next));
        let p = call(h, r.clone(), "fetchOrdersForSymbol", vec![next])?;
        let orders = h.await_value(p).await?;
        if h.truthy(&get(h, &r, "isDisposed")?) {
            return Ok(h.undefined());
        }
        call(h, get(h, &r, "store")?, "addOrUpdateOrders", vec![orders])?;
        let p = h.sleep(100.0)?;
        h.await_value(p).await?;
        if h.truthy(&get(h, &r, "isDisposed")?) {
            return Ok(h.undefined());
        }
    }
    Ok(h.undefined())
}
pub async fn symbol_orders<H: ReadHost>(
    h: &H,
    r: H::Value,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    let attempt = async {
        let xhr = get(h, &r, "xhr")?;
        let p = method(h, xhr, "get")?;
        let config = h.object(vec![(
            "params",
            h.object(vec![("contractName", symbol.clone())]),
        )]);
        let p = invoke(h, p, vec![h.text(endpoint("ORDERS")), config])?;
        let response = h.await_value(p).await?;
        let data = get(h, &response, "data")?;
        if !success(h, &r, &data)? {
            let msg = opt(h, &data, "msg")?;
            let msg = if h.truthy(&msg) {
                msg
            } else {
                h.concat(vec![h.text("Failed to fetch orders for "), symbol.clone()])?
            };
            return Err(h.error(h.string(msg)?)?);
        }
        let rows = opt(h, &data, "data")?;
        let rows = if h.is_array(&rows) {
            get(h, &data, "data")?
        } else {
            empty(h)
        };
        h.map(rows, |order| {
            let side = h.table("ORDER_SIDE", get(h, &order, "side")?)?;
            let kind = if h.truthy(&get(h, &order, "type")?) {
                h.table("ORDER_TYPE", get(h, &order, "type")?)?
            } else if h.equal(&get(h, &order, "triggerType")?, &h.number(1.0)) {
                h.text("stop_market")
            } else if h.equal(&get(h, &order, "triggerType")?, &h.number(2.0)) {
                h.text("take_profit_market")
            } else {
                h.text("market")
            };
            let key =
                h.uppercase(h.string(fallback(h, get(h, &order, "status")?, h.text("")))?)?;
            let mapped = h.table("ORDER_STATUS", key)?;
            let status = if h.truthy(&mapped) {
                mapped
            } else if h.equal(&get(h, &order, "action")?, &h.text("OPEN")) {
                h.text("open")
            } else {
                h.text("closed")
            };
            let stop = h.equal(&kind, &h.text("stop_market"))
                || h.equal(&kind, &h.text("take_profit_market"));
            let amount = get(h, &order, if stop { "executedQty" } else { "origQty" })?;
            let filled = if stop {
                h.number(0.0)
            } else {
                get(h, &order, "executedQty")?
            };
            Ok(h.object(vec![
                ("id", h.string(get(h, &order, "orderId")?)?),
                ("status", status),
                ("symbol", symbol.clone()),
                ("type", kind),
                ("side", side),
                (
                    "price",
                    get(h, &order, if stop { "triggerPrice" } else { "price" })?,
                ),
                ("amount", amount.clone()),
                ("filled", filled.clone()),
                ("remaining", h.arithmetic(Op::Sub, amount, filled)?),
                (
                    "reduceOnly",
                    h.boolean(h.equal(&get(h, &order, "action")?, &h.text("CLOSE"))),
                ),
                ("info", order.clone()),
                ("timeInForce", h.text("GoodTillCancel")),
                ("lastUpdate", get(h, &order, "transactTime")?),
            ]))
        })
    }
    .await;
    match attempt {
        Ok(v) => Ok(v),
        Err(e) => {
            log_error(h, r.clone(), e)?;
            let rows = get(h, &get(h, &r, "store")?, "orders")?;
            h.filter(rows, |o| Ok(h.equal(&get(h, &o, "symbol")?, &symbol)))
        }
    }
}
pub async fn trade_rows<H: ReadHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if guard(h, &r, &opts)? {
        return Ok(empty(h));
    }
    let symbol = get(h, &opts, "symbol")?;
    let from_id = get(h, &opts, "fromId")?;
    let max = limit(h, &opts)?;
    let p = method(h, r.clone(), "normalizeTimestampMs")?;
    let from = invoke(h, p, vec![get(h, &opts, "from")?])?;
    let p = method(h, r.clone(), "normalizeTimestampMs")?;
    let to = invoke(h, p, vec![get(h, &opts, "to")?])?;
    let params = h.omit_undefined(h.object(vec![
        ("contractName", symbol),
        ("fromId", from_id),
        ("limit", max),
        ("startTime", from),
        ("endTime", to),
    ]))?;
    let xhr = get(h, &r, "xhr")?;
    let p = method(h, xhr, "get")?;
    let p = invoke(
        h,
        p,
        vec![
            h.text(endpoint("MY_TRADES")),
            h.object(vec![("params", params)]),
        ],
    )?;
    let response = h.await_value(p).await?;
    let data = get(h, &response, "data")?;
    if !success(h, &r, &data)? {
        let msg = fallback(
            h,
            opt(h, &data, "msg")?,
            h.text("Failed to fetch my trades"),
        );
        return Err(h.error(h.string(msg)?)?);
    }
    let rows = opt(h, &data, "data")?;
    Ok(if h.is_array(&rows) {
        get(h, &data, "data")?
    } else {
        empty(h)
    })
}
pub async fn history<H: ReadHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if guard(h, &r, &opts)? {
        return Ok(empty(h));
    }
    let attempt = async {
        let p = call(h, r.clone(), "fetchBitrueTradeRows", vec![opts.clone()])?;
        let rows = h.await_value(p).await?;
        let requested_id = if h.truthy(&get(h, &opts, "orderId")?) {
            h.string(get(h, &opts, "orderId")?)?
        } else {
            h.undefined()
        };
        let p = method(h, r.clone(), "normalizeTimestampMs")?;
        let from = invoke(h, p, vec![get(h, &opts, "from")?])?;
        let p = method(h, r.clone(), "normalizeTimestampMs")?;
        let to = invoke(h, p, vec![get(h, &opts, "to")?])?;
        let p = method(h, r.clone(), "normalizeTradeSymbol")?;
        let requested_symbol = invoke(h, p, vec![get(h, &opts, "symbol")?])?;
        let max = limit(h, &opts)?;
        let mapped = h.map(rows, |entry| {
            let p = method(h, r.clone(), "normalizeTradeSymbol")?;
            let symbol = invoke(
                h,
                p,
                vec![fallback(
                    h,
                    get(h, &entry, "contractName")?,
                    requested_symbol.clone(),
                )],
            )?;
            if !h.truthy(&symbol) {
                return Ok(h.undefined());
            }
            let key = h.uppercase(h.string(fallback(h, get(h, &entry, "side")?, h.text("")))?)?;
            let side = fallback(h, h.table("ORDER_SIDE", key)?, h.text("buy"));
            let price = h.numeric(fallback(h, get(h, &entry, "price")?, h.number(0.0)))?;
            let qty = call(
                h,
                r.clone(),
                "resolveTradeQty",
                vec![entry.clone(), symbol.clone(), price.clone()],
            )?;
            let quote = h.numeric(nullish(h, get(h, &entry, "amount")?, h.number(0.0)))?;
            let fee = h.numeric(nullish(h, get(h, &entry, "fee")?, h.number(0.0)))?;
            let mut raw = h.undefined();
            for key in ["ctime", "cTime", "time"] {
                if h.nullish(&raw) {
                    raw = get(h, &entry, key)?;
                }
            }
            let raw = nullish(h, raw, h.number(0.0));
            let numeric = h.numeric(raw.clone())?;
            let time = if h.is_finite(&numeric) {
                if h.less(h.number(1_000_000_000_000.0), numeric.clone())? {
                    numeric
                } else {
                    h.arithmetic(Op::Mul, numeric, h.number(1000.0))?
                }
            } else {
                h.date_parse(h.string(fallback(h, raw, h.text("")))?)?
            };
            let chosen = call(h, r.clone(), "pickTradeOrderId", vec![entry.clone()])?;
            let id = if h.truthy(&chosen) {
                chosen
            } else {
                h.concat(vec![symbol.clone(), h.text("-"), time.clone()])?
            };
            if !h.is_finite(&price)
                || !h.less(h.number(0.0), price.clone())?
                || !h.is_finite(&qty)
                || !h.less(h.number(0.0), qty.clone())?
                || !h.is_finite(&time)
            {
                return Ok(h.undefined());
            }
            if !h.equal(&symbol, &requested_symbol) {
                return Ok(h.undefined());
            }
            if h.truthy(&requested_id) && !h.equal(&id, &requested_id) {
                return Ok(h.undefined());
            }
            if !h.equal(&from, &h.undefined()) && h.less(time.clone(), from.clone())? {
                return Ok(h.undefined());
            }
            if !h.equal(&to, &h.undefined()) && h.less(to.clone(), time.clone())? {
                return Ok(h.undefined());
            }
            let trade_id = get(h, &entry, "tradeId")?;
            let trade_id =
                if !h.nullish(&trade_id) && !h.equal(&get(h, &entry, "tradeId")?, &h.text("")) {
                    h.string(get(h, &entry, "tradeId")?)?
                } else {
                    h.undefined()
                };
            let buyer = get(h, &entry, "isBuyer")?;
            let buyer = if h.is_boolean(&buyer) {
                get(h, &entry, "isBuyer")?
            } else {
                h.undefined()
            };
            let maker = get(h, &entry, "isMaker")?;
            let maker = if h.is_boolean(&maker) {
                get(h, &entry, "isMaker")?
            } else {
                h.undefined()
            };
            h.omit_undefined(h.object(vec![
                ("id", trade_id),
                ("symbol", symbol.clone()),
                ("orderId", id),
                ("price", price),
                ("qty", qty),
                (
                    "quoteQty",
                    if h.is_finite(&quote) {
                        quote
                    } else {
                        h.undefined()
                    },
                ),
                ("positionSide", h.text("both")),
                ("side", side),
                ("buyer", buyer),
                ("maker", maker),
                (
                    "commission",
                    if h.is_finite(&fee) {
                        h.abs(fee)?
                    } else {
                        h.undefined()
                    },
                ),
                ("commissionAsset", h.split_at(symbol, "-", 2)?),
                ("time", time),
            ]))
        })?;
        let filtered = h.filter(mapped, |v| Ok(h.truthy(&v)))?;
        let sorted = h.sort_time_desc(filtered)?;
        h.slice_limit(sorted, max)
    }
    .await;
    match attempt {
        Ok(v) => Ok(v),
        Err(e) => {
            log_error(h, r, e)?;
            Ok(empty(h))
        }
    }
}
pub async fn fills<H: ReadHost>(h: &H, r: H::Value, opts: H::Value) -> Result<H::Value, H::Error> {
    if guard(h, &r, &opts)? {
        return Ok(empty(h));
    }
    let attempt = async {
        let p = method(h, r.clone(), "fetchHistoricalTradesInternal")?;
        let mut fields = vec![];
        for k in ["symbol", "orderId", "from", "to", "limit"] {
            fields.push((k, get(h, &opts, k)?));
        }
        let p = invoke(h, p, vec![h.object(fields)])?;
        let trades = h.await_value(p).await?;
        h.map(trades, |t| {
            let id = get(h, &t, "id")?;
            let timestamp =
                h.floor(h.arithmetic(Op::Div, get(h, &t, "time")?, h.number(1000.0))?)?;
            let mut fields = vec![("id", id), ("timestamp", timestamp)];
            for k in [
                "symbol",
                "orderId",
                "price",
                "qty",
                "side",
                "realizedPnl",
                "positionSide",
                "maker",
                "commission",
                "commissionAsset",
            ] {
                fields.push((k, get(h, &t, k)?));
            }
            h.omit_undefined(h.object(fields))
        })
    }
    .await;
    match attempt {
        Ok(v) => Ok(v),
        Err(e) => {
            log_error(h, r, e)?;
            Ok(empty(h))
        }
    }
}
