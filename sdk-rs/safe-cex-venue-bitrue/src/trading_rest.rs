//! Trading REST and market reads over original runtime/import bindings.
use crate::collection_reads::{Op, ReadHost};
use std::{future::Future, pin::Pin, rc::Rc};
pub type Body<H> =
    Pin<Box<dyn Future<Output = Result<<H as ReadHost>::Value, <H as ReadHost>::Error>>>>;
pub type AsyncCallback<H> = Rc<dyn Fn(Vec<<H as ReadHost>::Value>) -> Body<H>>;
pub trait TradingHost: ReadHost {
    fn key(&self, v: &Self::Value, key: Self::Value) -> Result<Self::Value, Self::Error>;
    fn set_dynamic(
        &self,
        v: &Self::Value,
        key: Self::Value,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn null_is(&self, v: &Self::Value) -> bool;
    fn has(&self, v: Self::Value, key: &str) -> Result<bool, Self::Error>;
    fn flat_map(
        &self,
        v: Self::Value,
        f: impl FnMut(Self::Value) -> Result<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn find(
        &self,
        v: Self::Value,
        f: impl FnMut(Self::Value) -> Result<bool, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn copy(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn remove_key(&self, v: Self::Value, key: &str) -> Result<(), Self::Error>;
    fn chunk(&self, v: Self::Value, n: usize) -> Result<Self::Value, Self::Error>;
    fn times(
        &self,
        n: Self::Value,
        f: impl FnMut() -> Result<Self::Value, Self::Error>,
    ) -> Result<Self::Value, Self::Error>;
    fn push(&self, v: Self::Value, item: Self::Value) -> Result<(), Self::Error>;
    fn ceil(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn remainder(&self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn power(&self, a: Self::Value, b: Self::Value) -> Result<Self::Value, Self::Error>;
    fn adjust(&self, v: Self::Value, step: Self::Value) -> Result<Self::Value, Self::Error>;
    fn parse_float(&self, v: Self::Value) -> Result<Self::Value, Self::Error>;
    fn uuid(&self) -> Result<Self::Value, Self::Error>;
    fn inverse_order_type(&self, key: Self::Value) -> Result<Self::Value, Self::Error>;
    fn now(&self) -> Result<Self::Value, Self::Error>;
    // Calling the returned function polls its async prefix eagerly, as JavaScript.
    fn async_function(&self, f: AsyncCallback<Self>) -> Self::Value;
    // Attach every child; fail-fast rejection must not cancel outstanding siblings.
    fn promise_all(&self, promises: Self::Value) -> Result<Self::Value, Self::Error>;
}
fn get<H: TradingHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    h.get(v, k)
}
fn opt<H: TradingHost>(h: &H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if h.nullish(v) {
        Ok(h.undefined())
    } else {
        get(h, v, k)
    }
}
fn method<H: TradingHost>(h: &H, r: H::Value, k: &str) -> Result<(H::Value, H::Value), H::Error> {
    Ok((get(h, &r, k)?, r))
}
fn invoke<H: TradingHost>(
    h: &H,
    p: (H::Value, H::Value),
    a: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.call(p.0, p.1, a)
}
fn call<H: TradingHost>(
    h: &H,
    r: H::Value,
    k: &str,
    a: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    invoke(h, method(h, r, k)?, a)
}
fn fallback<H: TradingHost>(h: &H, v: H::Value, to: H::Value) -> H::Value {
    if h.truthy(&v) { v } else { to }
}
fn empty<H: TradingHost>(h: &H) -> H::Value {
    h.array(vec![])
}
fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(k, _)| *k == name)
        .expect("source endpoint")
        .1
}
fn stop<H: TradingHost>(h: &H, o: &H::Value) -> Result<bool, H::Error> {
    Ok(h.equal(&get(h, o, "type")?, &h.text("stop_market"))
        || h.equal(&get(h, o, "type")?, &h.text("take_profit_market")))
}
fn refresh<H: TradingHost>(h: &H, o: &H::Value) -> Result<bool, H::Error> {
    Ok(stop(h, o)? || h.truthy(&get(h, o, "stopLoss")?) || h.truthy(&get(h, o, "takeProfit")?))
}
fn reason<H: TradingHost>(h: &H, e: &H::Error) -> Result<H::Value, H::Error> {
    let e = h.error_value(e);
    let msg = opt(h, &opt(h, &opt(h, &e, "response")?, "data")?, "msg")?;
    if h.truthy(&msg) {
        Ok(msg)
    } else {
        opt(h, &e, "message")
    }
}
fn success<H: TradingHost>(
    h: &H,
    r: H::Value,
    data: &H::Value,
    otherwise: &str,
) -> Result<(), H::Error> {
    let p = method(h, r, "isSuccessCode")?;
    if !h.truthy(&invoke(h, p, vec![opt(h, data, "code")?])?) {
        let msg = fallback(h, opt(h, data, "msg")?, h.text(otherwise));
        return Err(h.error(h.string(msg)?)?);
    }
    Ok(())
}
pub async fn markets<H: TradingHost>(h: &H, r: H::Value) -> Result<H::Value, H::Error> {
    let p = call(
        h,
        get(h, &r, "unlimitedXHR")?,
        "get",
        vec![h.text(endpoint("MARKETS"))],
    )?;
    let response = h.await_value(p).await?;
    let data = get(h, &response, "data")?;
    h.map(data, |m| {
        let multiplier = get(h, &m, "multiplier")?;
        let id = get(h, &m, "symbol")?;
        let symbol = get(h, &m, "symbol")?;
        let base = h.split_at(get(h, &m, "symbol")?, "-", 1)?;
        let quote = h.split_at(get(h, &m, "symbol")?, "-", 2)?;
        let active = h.boolean(h.equal(&get(h, &m, "status")?, &h.number(1.0)));
        let precision = h.object(vec![
            ("amount", multiplier.clone()),
            (
                "price",
                h.arithmetic(
                    Op::Div,
                    h.number(1.0),
                    h.power(h.number(10.0), get(h, &m, "pricePrecision")?)?,
                )?,
            ),
        ]);
        let min = h.arithmetic(Op::Mul, get(h, &m, "minOrderVolume")?, multiplier.clone())?;
        let max = h.arithmetic(Op::Mul, get(h, &m, "maxMarketVolume")?, multiplier)?;
        Ok(h.object(vec![
            ("id", id),
            ("symbol", symbol),
            ("base", base),
            ("quote", quote),
            ("active", active),
            ("precision", precision),
            (
                "limits",
                h.object(vec![
                    ("amount", h.object(vec![("min", min), ("max", max)])),
                    (
                        "leverage",
                        h.object(vec![("min", h.number(1.0)), ("max", h.number(100.0))]),
                    ),
                ]),
            ),
            ("takerFee", h.number(0.001)),
            ("makerFee", h.number(0.001)),
            ("info", m),
        ]))
    })
}
pub async fn candles<H: TradingHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    if h.truthy(&get(h, &get(h, &r, "publicWebsocket")?, "ws")?) {
        let ws = get(h, &r, "publicWebsocket")?;
        let p = method(h, ws, "formatKlineTopic")?;
        let topic = invoke(
            h,
            p,
            vec![get(h, &opts, "symbol")?, get(h, &opts, "interval")?],
        )?;
        let recent = get(h, &r, "recentWsRequests")?;
        if h.truthy(&h.key(&recent, topic.clone())?) {
            let now = h.now()?;
            let cached = h.key(&get(h, &r, "recentWsRequests")?, topic.clone())?;
            let age = h.arithmetic(Op::Sub, now, get(h, &cached, "time")?)?;
            if h.less(age, h.number(10_000.0))? {
                return get(
                    h,
                    &h.key(&get(h, &r, "recentWsRequests")?, topic.clone())?,
                    "data",
                );
            }
        }
        let mut i = 0;
        while !h.equal(
            &get(
                h,
                &get(h, &get(h, &r, "publicWebsocket")?, "ws")?,
                "readyState",
            )?,
            &h.number(1.0),
        ) {
            i += 1;
            if i > 100 {
                return Err(h.error(h.text("fetchOHLCV: websocket not connected"))?);
            }
            let p = h.sleep(100.0)?;
            h.await_value(p).await?;
        }
        let p = call(
            h,
            get(h, &r, "publicWebsocket")?,
            "makeRequest",
            vec![topic.clone()],
        )?;
        let raw = h.await_value(p).await?;
        let candles = call(h, get(h, &r, "publicWebsocket")?, "mapKline", vec![raw])?;
        let recent = get(h, &r, "recentWsRequests")?;
        let time = h.now()?;
        let entry = h.object(vec![("time", time), ("data", candles.clone())]);
        h.set_dynamic(&recent, topic, entry)?;
        Ok(candles)
    } else {
        let p = call(h, r, "callMethodInWorker", vec![h.text("fetchOHLCV"), opts])?;
        h.await_value(p).await
    }
}
pub fn format_order<H: TradingHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let store = get(h, &r, "store")?;
    let markets = get(h, &store, "marketsBySymbol")?;
    let market = h.key(&markets, get(h, &opts, "symbol")?)?;
    if !h.truthy(&market) {
        return Err(h.error(h.concat(vec![
            h.text("Market "),
            get(h, &opts, "symbol")?,
            h.text(" not found"),
        ])?)?);
    }
    let conditional = stop(h, &opts)?;
    let positions = get(h, &get(h, &r, "store")?, "positions")?;
    let position = h.find(positions, |p| {
        Ok(h.equal(&get(h, &p, "symbol")?, &get(h, &opts, "symbol")?))
    })?;
    let max = get(h, &get(h, &get(h, &market, "limits")?, "amount")?, "max")?;
    let p_price = get(h, &get(h, &market, "precision")?, "price")?;
    let p_amount = get(h, &get(h, &market, "precision")?, "amount")?;
    let amount = h.adjust(get(h, &opts, "amount")?, p_amount.clone())?;
    let side = if h.equal(&get(h, &opts, "side")?, &h.text("buy")) {
        h.text("BUY")
    } else {
        h.text("SELL")
    };
    let direction = if h.truthy(&get(h, &opts, "reduceOnly")?) {
        h.text("CLOSE")
    } else {
        h.text("OPEN")
    };
    let kind = h.inverse_order_type(get(h, &opts, "type")?)?;
    let price = if h.truthy(&get(h, &opts, "price")?)
        && !h.equal(&get(h, &opts, "type")?, &h.text("market"))
    {
        h.adjust(get(h, &opts, "price")?, p_price)?
    } else {
        h.undefined()
    };
    let mut leverage = get(h, &get(h, &get(h, &market, "limits")?, "leverage")?, "max")?;
    let mut margin = h.text("cross");
    if !h.truthy(&position) {
        h.console("error","Bitrue position not found when placing order. Expected to determine leverage and margin type...")?;
    } else {
        leverage = get(h, &position, "leverage")?;
        margin = fallback(h, get(h, &position, "marginType")?, h.text("cross"));
    }
    if conditional && !h.truthy(&position) {
        h.console("error","Bitrue position not found when placing order. Needed for binding stop loss or take profit...")?;
        return Err(h.error(h.text("Position not found"))?);
    }
    if conditional
        && (!h.equal(&get(h, &opts, "stopLoss")?, &h.undefined())
            || !h.equal(&get(h, &opts, "takeProfit")?, &h.undefined()))
    {
        return Err(h.error(
            h.text("Bitrue only supports attached TP/SL on limit or market parent orders"),
        )?);
    }
    let req = h.object(vec![
        ("contractName", get(h, &opts, "symbol")?),
        ("leverage", leverage),
        (
            "price",
            if conditional {
                h.number(0.0)
            } else {
                price.clone()
            },
        ),
        (
            "triggerPrice",
            if conditional { price } else { h.undefined() },
        ),
        ("volume", amount.clone()),
        ("type", if conditional { h.text("MARKET") } else { kind }),
        ("side", side),
        ("open", direction),
        (
            "positionType",
            h.number(if h.equal(&margin, &h.text("cross")) {
                1.0
            } else {
                2.0
            }),
        ),
        ("futureModel", h.text("normal")),
    ]);
    if conditional {
        for (k, v) in [
            ("conditionOrder", h.boolean(true)),
            ("clientOrderId", h.uuid()?),
            (
                "triggerType",
                h.number(
                    if h.equal(&get(h, &opts, "type")?, &h.text("stop_market")) {
                        1.0
                    } else {
                        2.0
                    },
                ),
            ),
            (
                "triggerPriceType",
                h.number(
                    if h.equal(&get(h, &opts, "slTriggerType")?, &h.text("last_price")) {
                        1.0
                    } else {
                        2.0
                    },
                ),
            ),
        ] {
            h.set(&req, k, v)?;
        }
    }
    let req = h.omit_undefined(req)?;
    let lots = if h.less(max.clone(), amount.clone())? {
        h.ceil(h.arithmetic(Op::Div, amount.clone(), max.clone())?)?
    } else {
        h.number(1.0)
    };
    let rest = if h.less(max.clone(), amount.clone())? {
        h.adjust(h.remainder(amount.clone(), max)?, p_amount.clone())?
    } else {
        h.number(0.0)
    };
    let size = h.adjust(
        h.arithmetic(
            Op::Div,
            h.arithmetic(Op::Sub, amount, rest.clone())?,
            lots.clone(),
        )?,
        p_amount,
    )?;
    let mut payloads = h.times(lots, || {
        let p = h.copy(req.clone())?;
        h.set(&p, "volume", h.string(size.clone())?)?;
        Ok(p)
    })?;
    if h.truthy(&rest) {
        let p = h.copy(req)?;
        h.set(&p, "volume", h.string(rest)?)?;
        h.push(payloads.clone(), p)?;
    }
    let client = call(
        h,
        r.clone(),
        "resolveCallerClientOrderId",
        vec![opts.clone()],
    )?;
    let p = method(h, r.clone(), "assertSingleNativeClientOrderId")?;
    invoke(h, p, vec![client.clone(), get(h, &payloads, "length")?])?;
    if !h.equal(&client, &h.undefined()) {
        let first = h.key(&payloads, h.text("0"))?;
        h.set(&first, "clientOrderId", client)?;
    }
    if h.equal(&get(h, &opts, "type")?, &h.text("market"))
        && !h.truthy(&get(h, &opts, "reduceOnly")?)
    {
        let tickers = get(h, &get(h, &r, "store")?, "tickersBySymbol")?;
        let ticker = h.key(&tickers, get(h, &opts, "symbol")?)?;
        let mut reference = h.undefined();
        for k in ["mark", "last", "bid", "ask"] {
            if !h.truthy(&reference) {
                reference = opt(h, &ticker, k)?;
            }
        }
        let reference = fallback(h, reference, h.number(0.0));
        if !h.truthy(&reference) {
            return Err(h.error(h.text("Market order without reference price"))?);
        }
        payloads = h.map(payloads, |payload| {
            let volume = get(h, &payload, "volume")?;
            let p = h.copy(payload)?;
            h.remove_key(p.clone(), "volume")?;
            let parsed = h.parse_float(volume)?;
            let precision = get(h, &get(h, &market, "precision")?, "amount")?;
            let adjusted = h.arithmetic(
                Op::Add,
                parsed,
                h.arithmetic(Op::Div, precision, h.number(3.0))?,
            )?;
            let amount = h.adjust(
                h.arithmetic(Op::Mul, adjusted, reference.clone())?,
                h.number(0.0001),
            )?;
            let out = h.copy(p)?;
            let minimum = get(h, &get(h, &market, "info")?, "minOrderMoney")?;
            h.set(&out, "amount", h.arithmetic(Op::Max, amount, minimum)?)?;
            Ok(out)
        })?;
    }
    if !h.equal(&get(h, &opts, "stopLoss")?, &h.undefined())
        || !h.equal(&get(h, &opts, "takeProfit")?, &h.undefined())
    {
        payloads = h.map(payloads, |payload| {
            let mut triggers = vec![];
            for (name, kind, price_type) in [
                ("stopLoss", 1.0, "slTriggerType"),
                ("takeProfit", 2.0, "tpTriggerType"),
            ] {
                if !h.equal(&get(h, &opts, name)?, &h.undefined()) {
                    triggers.push(h.object(vec![
                        ("clientOrderId", h.uuid()?),
                        ("triggerType", h.number(kind)),
                        (
                            "triggerPriceType",
                            h.number(
                                if h.equal(&get(h, &opts, price_type)?, &h.text("last_price")) {
                                    1.0
                                } else {
                                    2.0
                                },
                            ),
                        ),
                        ("triggerPrice", get(h, &opts, name)?),
                        ("type", h.text("MARKET")),
                        ("price", h.number(0.0)),
                    ]));
                }
            }
            let p = h.copy(payload)?;
            h.set(&p, "triggerOrderCreateParams", h.array(triggers))?;
            Ok(p)
        })?;
    }
    Ok(payloads)
}
pub async fn place<H: TradingHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    call(
        h,
        r.clone(),
        "resolveCallerClientOrderId",
        vec![opts.clone()],
    )?;
    let payloads = call(h, r.clone(), "formatCreateOrder", vec![opts.clone()])?;
    let p = call(h, r.clone(), "placeOrderBatch", vec![payloads])?;
    let res = h.await_value(p).await?;
    if refresh(h, &opts)? {
        let p = method(h, r, "callMethodInWorker")?;
        let p = invoke(
            h,
            p,
            vec![
                h.text("fetchAndSetOrders"),
                h.array(vec![get(h, &opts, "symbol")?]),
            ],
        )?;
        h.await_value(p).await?;
        return Ok(empty(h));
    }
    Ok(res)
}
pub async fn places<H: TradingHost>(
    h: &H,
    r: H::Value,
    orders: H::Value,
) -> Result<H::Value, H::Error> {
    call(
        h,
        r.clone(),
        "assertCallerClientOrderIds",
        vec![orders.clone()],
    )?;
    let requests = h.flat_map(orders.clone(), |o| {
        call(h, r.clone(), "formatCreateOrder", vec![o])
    })?;
    let p = call(h, r.clone(), "placeOrderBatch", vec![requests])?;
    let res = h.await_value(p).await?;
    let conditional = h.filter(orders, |o| refresh(h, &o))?;
    let symbols = h.map(conditional, |o| get(h, &o, "symbol"))?;
    let mut unique = vec![];
    for s in h.items(symbols)? {
        if !unique.iter().any(|v| h.set_equal(v, &s)) {
            unique.push(s)
        }
    }
    if !unique.is_empty() {
        let p = call(
            h,
            r,
            "callMethodInWorker",
            vec![h.text("fetchAndSetOrders"), h.array(unique)],
        )?;
        h.await_value(p).await?;
        return Ok(empty(h));
    }
    Ok(res)
}
pub async fn batch<H: TradingHost>(
    h: &H,
    r: H::Value,
    payloads: H::Value,
) -> Result<H::Value, H::Error> {
    let lots = h.chunk(payloads, 5)?;
    let ids = empty(h);
    for lot in h.items(lots)? {
        let host = h.clone();
        let receiver = r.clone();
        let captured = ids.clone();
        let f = h.async_function(Rc::new(move |args| {
            let h = host.clone();
            let r = receiver.clone();
            let ids = captured.clone();
            Box::pin(async move {
                let payload = args.first().cloned().unwrap_or_else(|| h.undefined());
                let attempt = async {
                    let p = call(
                        &h,
                        get(&h, &r, "unlimitedXHR")?,
                        "post",
                        vec![h.text(endpoint("PLACE_ORDER")), payload],
                    )?;
                    let response = h.await_value(p).await?;
                    let data = get(&h, &response, "data")?;
                    success(&h, r.clone(), &data, "Failed to place order")?;
                    h.push(ids, opt(&h, &opt(&h, &data, "data")?, "orderId")?)?;
                    Ok(h.undefined())
                }
                .await;
                match attempt {
                    Ok(v) => Ok(v),
                    Err(e) => {
                        let p = method(&h, r, "emitAndLogError")?;
                        invoke(&h, p, vec![reason(&h, &e)?])?;
                        Err(h.error(reason(&h, &e)?)?)
                    }
                }
            })
        }));
        let promises = h.map(lot, |payload| {
            h.call(f.clone(), h.undefined(), vec![payload])
        })?;
        let all = h.promise_all(promises)?;
        h.await_value(all).await?;
    }
    Ok(ids)
}
pub async fn update<H: TradingHost>(
    h: &H,
    r: H::Value,
    opts: H::Value,
) -> Result<H::Value, H::Error> {
    let order = get(h, &opts, "order")?;
    let update = get(h, &opts, "update")?;
    let next = h.copy(order.clone())?;
    for k in ["symbol", "type", "side", "price", "amount"] {
        h.set(&next, k, get(h, &order, k)?)?;
    }
    h.set(
        &next,
        "reduceOnly",
        fallback(h, get(h, &order, "reduceOnly")?, h.boolean(false)),
    )?;
    for k in ["price", "amount"] {
        if h.has(update.clone(), k)? {
            h.set(&next, k, get(h, &update, k)?)?;
        }
    }
    let p = call(h, r.clone(), "cancelOrders", vec![h.array(vec![order])])?;
    h.await_value(p).await?;
    let p = call(h, r, "placeOrder", vec![next])?;
    h.await_value(p).await
}
pub async fn leverage<H: TradingHost>(
    h: &H,
    r: H::Value,
    symbol: H::Value,
    level: H::Value,
    silent: H::Value,
) -> Result<H::Value, H::Error> {
    let attempt = async {
        let xhr = get(h, &r, "xhr")?;
        let p = call(
            h,
            xhr,
            "post",
            vec![
                h.text(endpoint("SET_LEVERAGE")),
                h.object(vec![("contractName", symbol.clone()), ("leverage", level)]),
            ],
        )?;
        let response = h.await_value(p).await?;
        let data = get(h, &response, "data")?;
        success(h, r.clone(), &data, "Failed to set leverage")?;
        call(
            h,
            get(h, &r, "positionMetadataBySymbol")?,
            "delete",
            vec![symbol.clone()],
        )?;
        Ok(h.undefined())
    }
    .await;
    match attempt {
        Ok(v) => Ok(v),
        Err(e) => {
            let message = fallback(h, reason(h, &e)?, h.text("Failed to set leverage"));
            let max = call(
                h,
                r.clone(),
                "parseMaxLeverageFromErrorMessage",
                vec![message.clone()],
            )?;
            if !h.null_is(&max) {
                call(h, r.clone(), "setMarketMaxLeverage", vec![symbol, max])?;
            }
            if !h.truthy(&silent) {
                call(h, r, "emitAndLogError", vec![message.clone()])?;
            }
            Err(h.error(message)?)
        }
    }
}
pub async fn cancel<H: TradingHost>(
    h: &H,
    r: H::Value,
    orders: H::Value,
) -> Result<H::Value, H::Error> {
    let attempt = async {
        let payloads = h.map(orders.clone(), |o| {
            let condition = h.boolean(!h.equal(&get(h, &o, "type")?, &h.text("limit")));
            if h.truthy(&opt(h, &get(h, &o, "info")?, "clientOrderId")?) {
                Ok(h.object(vec![
                    (
                        "clientOrderId",
                        get(h, &get(h, &o, "info")?, "clientOrderId")?,
                    ),
                    ("contractName", get(h, &o, "symbol")?),
                    ("conditionOrder", condition),
                ]))
            } else {
                Ok(h.object(vec![
                    ("orderId", get(h, &o, "id")?),
                    ("contractName", get(h, &o, "symbol")?),
                    ("conditionOrder", condition),
                ]))
            }
        })?;
        let lots = h.chunk(payloads, 50)?;
        let host = h.clone();
        let receiver = r.clone();
        let lot_fn = h.async_function(Rc::new(move |args| {
            let h = host.clone();
            let r = receiver.clone();
            Box::pin(async move {
                let lot = args.first().cloned().unwrap_or_else(|| h.undefined());
                let host = h.clone();
                let receiver = r.clone();
                let child = h.async_function(Rc::new(move |args| {
                    let h = host.clone();
                    let r = receiver.clone();
                    Box::pin(async move {
                        let payload = args.first().cloned().unwrap_or_else(|| h.undefined());
                        let xhr = get(&h, &r, "xhr")?;
                        let p = method(&h, xhr, "post")?;
                        let p = invoke(
                            &h,
                            p,
                            vec![h.text(endpoint("CANCEL_ORDER")), h.copy(payload)?],
                        )?;
                        let response = h.await_value(p).await?;
                        let data = get(&h, &response, "data")?;
                        success(&h, r, &data, "Failed to cancel order")?;
                        opt(&h, &get(&h, &data, "data")?, "orderId")
                    })
                }));
                let promises = h.map(lot, |payload| {
                    h.call(child.clone(), h.undefined(), vec![payload])
                })?;
                let p = h.promise_all(promises)?;
                let responses = h.await_value(p).await?;
                h.flatten(responses)
            })
        }));
        let p = h.map_series(lots, lot_fn)?;
        h.await_value(p).await?;
        let ids = h.map(orders, |o| Ok(h.object(vec![("id", get(h, &o, "id")?)])))?;
        let p = call(
            h,
            r.clone(),
            "callMethodInWorker",
            vec![h.text("removeOrders"), ids],
        )?;
        h.await_value(p).await?;
        Ok(h.undefined())
    }
    .await;
    match attempt {
        Ok(v) => Ok(v),
        Err(e) => {
            let p = method(h, r, "emitAndLogError")?;
            invoke(h, p, vec![reason(h, &e)?])?;
            Err(h.error(reason(h, &e)?)?)
        }
    }
}
