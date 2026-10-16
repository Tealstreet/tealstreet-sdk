//! Public REST and stream bootstrap bodies over supplied native operations.
//! Opaque values retain references. Array callbacks execute in the original
//! native operation; receiver paths and captured methods retain their JS owner.
use std::{future::Future, pin::Pin};
pub type PublicRead<'a, V, E> = Pin<Box<dyn Future<Output = Result<V, E>> + 'a>>;
pub trait PublicDataHost {
    type Value: Clone;
    type Error;
    type Iterator;
    fn receiver(&self, path: &str) -> Result<Self::Value, Self::Error>;
    fn set_receiver(&self, path: &str, value: Self::Value) -> Result<(), Self::Error>;
    /// Capture the callable and owner along the native member chain, before args.
    fn capture(&self, path: &str) -> Result<Self::Value, Self::Error>;
    fn invoke(
        &self,
        method: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn capture_member(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn flat_map<F>(&self, value: Self::Value, callback: F) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<Self::Value, Self::Error>;
    fn property(&self, value: &Self::Value, name: &str) -> Result<Self::Value, Self::Error>;
    fn key_get(&self, value: &Self::Value, key: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn assign(
        &self,
        value: &Self::Value,
        key: Self::Value,
        field: Self::Value,
    ) -> Result<(), Self::Error>;
    fn define(
        &self,
        value: &Self::Value,
        key: Self::Value,
        field: Self::Value,
    ) -> Result<(), Self::Error>;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn equal(&self, a: &Self::Value, b: &Self::Value) -> bool;
    fn number(&self, value: f64) -> Self::Value;
    fn numeric(&self, value: &Self::Value) -> Result<f64, Self::Error>;
    fn boolean(&self, value: bool) -> Self::Value;
    fn string(&self, value: &[u16]) -> Self::Value;
    fn array(&self, value: Vec<Self::Value>) -> Self::Value;
    fn object(&self, fields: Vec<(&str, Self::Value)>) -> Self::Value;
    fn spread(&self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn index(&self, value: &Self::Value, index: usize) -> Result<Self::Value, Self::Error>;
    fn iterator(&self, value: &Self::Value) -> Result<Self::Iterator, Self::Error>;
    fn next(&self, iterator: &mut Self::Iterator) -> Result<Option<Self::Value>, Self::Error>;
    /// Native method lookup/call, captured length, holes, live reads and callbacks.
    fn filter<F>(&self, value: Self::Value, callback: F) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<bool, Self::Error>;
    fn map<F>(&self, value: Self::Value, callback: F) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, usize) -> Result<Self::Value, Self::Error>;
    fn reduce<F>(
        &self,
        value: Self::Value,
        initial: Self::Value,
        callback: F,
    ) -> Result<Self::Value, Self::Error>
    where
        F: FnMut(Self::Value, Self::Value, usize) -> Result<Self::Value, Self::Error>;
    /// Original JS global/imported operation, including coercion and exceptions.
    fn operation(&self, name: &str, args: Vec<Self::Value>) -> Result<Self::Value, Self::Error>;
    /// Capture xhr/get before constructing params; fulfilled outer Axios data.
    fn read(
        &self,
        client: Self::Value,
        endpoint: &'static str,
        params: Self::Value,
    ) -> Result<PublicRead<'_, Self::Value, Self::Error>, Self::Error>;
}
fn s<H: PublicDataHost>(h: &H, v: &str) -> H::Value {
    h.string(&v.encode_utf16().collect::<Vec<_>>())
}
fn p<H: PublicDataHost>(h: &H, v: &H::Value, n: &str) -> Result<H::Value, H::Error> {
    h.property(v, n)
}
fn parsed<H: PublicDataHost>(h: &H, v: &H::Value, n: &str) -> Result<f64, H::Error> {
    let v = h.operation("parseFloat", vec![p(h, v, n)?])?;
    h.numeric(&v)
}
fn fallback(n: f64, other: f64) -> f64 {
    if n == 0. || n.is_nan() { other } else { n }
}
fn call<H: PublicDataHost>(h: &H, n: &str, args: Vec<H::Value>) -> Result<H::Value, H::Error> {
    let f = h.capture(n)?;
    h.invoke(f, args)
}
fn append<H: PublicDataHost>(h: &H, out: &mut Vec<H::Value>, v: H::Value) -> Result<(), H::Error> {
    let mut i = h.iterator(&v)?;
    while let Some(v) = h.next(&mut i)? {
        out.push(v)
    }
    Ok(())
}
fn market<H: PublicDataHost>(h: &H, m: H::Value, inverse: bool) -> Result<H::Value, H::Error> {
    let price_place = parsed(h, &m, "pricePlace")?;
    let power = h.operation("power", vec![h.number(10.), h.number(price_place + 1.)])?;
    let decimals = 10. / h.numeric(&power)?;
    let price = decimals * parsed(h, &m, "priceEndStep")?;
    let max = if inverse {
        None
    } else {
        let a = parsed(h, &m, "maxOrderQty")?;
        Some(if a != 0. && !a.is_nan() {
            a
        } else {
            let b = parsed(h, &m, "maxMarketOrderQty")?;
            if b != 0. && !b.is_nan() {
                b
            } else {
                fallback(parsed(h, &m, "maxTradeNum")?, f64::INFINITY)
            }
        })
    };
    let id = p(h, &m, "symbol")?;
    let symbol = p(h, &m, "symbol")?;
    let base = p(h, &m, "baseCoin")?;
    let quote = p(h, &m, "quoteCoin")?;
    let active = h.equal(&p(h, &m, "symbolStatus")?, &s(h, "normal"))
        || h.equal(&p(h, &m, "symbolStatus")?, &s(h, "listed"))
        || h.equal(&p(h, &m, "symbolStatus")?, &s(h, "limit_open"));
    let amount = parsed(h, &m, "sizeMultiplier")?;
    let precision = h.object(vec![
        ("amount", h.number(amount)),
        ("price", h.number(price)),
    ]);
    let min = fallback(parsed(h, &m, "minTradeNum")?, 0.);
    let max = match max {
        Some(v) => v,
        None => fallback(parsed(h, &m, "maxTradeNum")?, f64::INFINITY),
    };
    let amount = h.object(vec![("min", h.number(min)), ("max", h.number(max))]);
    let min = fallback(parsed(h, &m, "minLever")?, 1.);
    let max = fallback(parsed(h, &m, "maxLever")?, 20.);
    let leverage = h.object(vec![("min", h.number(min)), ("max", h.number(max))]);
    let mut fields = vec![
        ("id", id),
        ("symbol", symbol),
        ("base", base),
        ("quote", quote),
        ("active", h.boolean(active)),
        ("precision", precision),
        (
            "limits",
            h.object(vec![("amount", amount), ("leverage", leverage)]),
        ),
    ];
    if inverse {
        fields.push(("isInverse", h.boolean(true)))
    }
    fields.push(("info", m));
    Ok(h.object(fields))
}
async fn markets<H: PublicDataHost>(h: &H, inverse: bool) -> Result<H::Value, H::Error> {
    let client = h.capture("xhr.get")?;
    let params = h.object(vec![("productType", h.receiver("apiProductType")?)]);
    let response = h.read(client, "MARKETS", params)?.await?;
    let rows = p(h, &response, "data")?;
    let selected = h.filter(rows, |m, _| {
        let quote = p(h, &m, "quoteCoin")?;
        let expected = if inverse {
            s(h, "USD")
        } else {
            h.receiver("apiMarginCoin")?
        };
        Ok(h.equal(&quote, &expected))
    })?;
    h.map(selected, |m, _| market(h, m, inverse))
}
// ts: BitgetExchange.fetchMarkets
pub async fn fetch_markets<H: PublicDataHost>(h: &H) -> Result<H::Value, H::Error> {
    markets(h, false).await
}
// ts: BitgetInverseExchange.fetchMarkets
pub async fn fetch_inverse_markets<H: PublicDataHost>(h: &H) -> Result<H::Value, H::Error> {
    markets(h, true).await
}
// ts: BitgetExchange.fetchTickers
pub async fn fetch_tickers<H: PublicDataHost>(h: &H) -> Result<H::Value, H::Error> {
    let client = h.capture("xhr.get")?;
    let params = h.object(vec![("productType", h.receiver("apiProductType")?)]);
    let response = h.read(client, "TICKERS", params)?.await?;
    h.reduce(p(h, &response, "data")?, h.array(vec![]), |acc, t, _| {
        let by_id = h.receiver("store.marketsById")?;
        let symbol = p(h, &t, "symbol")?;
        let market = h.key_get(&by_id, &symbol)?;
        if !h.truthy(&market) {
            return Ok(acc);
        }
        let last = parsed(h, &t, "lastPr")?;
        let id = p(h, &market, "id")?;
        let symbol = p(h, &market, "symbol")?;
        let bid = parsed(h, &t, "bidPr")?;
        let ask = parsed(h, &t, "askPr")?;
        let index = parsed(h, &t, "indexPrice")?;
        let percentage = parsed(h, &t, "changeUtc24h")? * 100.;
        let funding = parsed(h, &t, "fundingRate")?;
        let volume = parsed(h, &t, "baseVolume")?;
        let quote = parsed(h, &t, "quoteVolume")?;
        let ticker = h.object(vec![
            ("id", id),
            ("symbol", symbol),
            ("bid", h.number(bid)),
            ("ask", h.number(ask)),
            ("last", h.number(last)),
            ("mark", h.number(last)),
            ("index", h.number(index)),
            ("percentage", h.number(percentage)),
            ("fundingRate", h.number(funding)),
            ("fundingTime", h.number(0.)),
            ("openInterest", h.number(0.)),
            ("volume", h.number(volume)),
            ("quoteVolume", h.number(quote)),
        ]);
        let mut out = vec![];
        append(h, &mut out, acc)?;
        out.push(ticker);
        Ok(h.array(out))
    })
}
// ts: BitgetExchange.fetchOHLCV
pub async fn fetch_ohlcv<H: PublicDataHost>(h: &H, opts: H::Value) -> Result<H::Value, H::Error> {
    let raw = p(h, &opts, "limit")?;
    let limit = h.operation(
        "Math.min",
        vec![
            if h.truthy(&raw) { raw } else { h.number(500.) },
            h.number(1000.),
        ],
    )?;
    let interval = h.operation("INTERVAL", vec![p(h, &opts, "interval")?])?;
    let parts = h.operation("splitDigits", vec![p(h, &opts, "interval")?])?;
    let amount = h.index(&parts, 1)?;
    let unit = h.index(&parts, 2)?;
    let timeframe = h.operation("timeframeToSeconds", vec![p(h, &opts, "interval")?])?;
    let interval_ms = if h.truthy(&timeframe) {
        h.numeric(&timeframe)?
    } else {
        0.
    } * 1000.;
    let end = if h.truthy(&p(h, &opts, "to")?) {
        h.operation("dayjs", vec![p(h, &opts, "to")?])?
    } else {
        h.operation("dayjs", vec![])?
    };
    let start = if h.truthy(&p(h, &opts, "from")?) {
        h.operation("dayjs", vec![p(h, &opts, "from")?])?
    } else {
        let subtract = h.capture_member(&end, "subtract")?;
        let amount = h.operation("parseFloat", vec![amount])?;
        h.invoke(
            subtract,
            vec![h.number(h.numeric(&amount)? * h.numeric(&limit)?), unit],
        )?
    };
    let start_time = h.operation("dayjs.valueOf", vec![start])?;
    let end_time = if h.truthy(&p(h, &opts, "from")?) {
        let end_value = h.operation("dayjs.valueOf", vec![end])?;
        let span = h.operation(
            "Math.min",
            vec![
                h.number(89. * 24. * 60. * 60. * 1000.),
                h.number(interval_ms * h.numeric(&limit)?),
            ],
        )?;
        h.operation(
            "Math.min",
            vec![
                end_value,
                h.number(h.numeric(&start_time)? + h.numeric(&span)?),
            ],
        )?
    } else {
        h.operation("dayjs.valueOf", vec![end])?
    };
    let send_start =
        h.truthy(&p(h, &opts, "from")?) && h.numeric(&start_time)? < h.numeric(&end_time)?;
    let client = h.capture("xhr.get")?;
    let mut fields = vec![
        ("symbol", p(h, &opts, "symbol")?),
        ("granularity", interval),
        ("endTime", end_time),
    ];
    if send_start {
        fields.push(("startTime", start_time))
    }
    fields.push(("productType", h.receiver("apiProductType")?));
    fields.push(("limit", limit));
    let response = h.read(client, "KLINE", h.object(fields))?.await?;
    let rows = p(h, &response, "data")?;
    if !h.truthy(&rows) {
        return Ok(h.array(vec![]));
    }
    h.map(rows, |c, _| {
        let timestamp = h.operation("parseInt", vec![h.index(&c, 0)?, h.number(10.)])?;
        let mut fields = vec![("timestamp", h.number(h.numeric(&timestamp)? / 1000.))];
        for (name, i) in [
            ("open", 1),
            ("high", 2),
            ("low", 3),
            ("close", 4),
            ("volume", 6),
        ] {
            fields.push((name, h.operation("parseFloat", vec![h.index(&c, i)?])?))
        }
        Ok(h.object(fields))
    })
}
// ts: BitgetPublicWebsocket.connectAndSubscribe
pub fn connect_and_subscribe<H: PublicDataHost>(h: &H) -> Result<(), H::Error> {
    if h.truthy(&h.receiver("isDisposed")?) {
        return Ok(());
    }
    if h.equal(&h.receiver("lastMessageCount")?, &h.number(0.))
        && h.equal(&h.receiver("messageCount")?, &h.number(0.))
    {
        let old = h.receiver("connectionAttempts")?;
        let next = h.operation("increment", vec![old])?;
        h.set_receiver("connectionAttempts", next)?;
        if h.truthy(&h.operation(
            "greaterEqual",
            vec![
                h.receiver("connectionAttempts")?,
                h.receiver("MAX_ATTEMPTS")?,
            ],
        )?) {
            h.set_receiver("useFallbackUrl", h.boolean(true))?
        }
    }
    let count = h.receiver("messageCount")?;
    h.set_receiver("lastMessageCount", count)?;
    h.set_receiver("messageCount", h.number(0.))?;
    let topics = h.receiver("topics")?;
    let selected = h.filter(h.receiver("parent.store.markets")?, |_, i| Ok(i % 3 == 0))?;
    let ticker = h.reduce(selected, h.object(vec![]), |acc, m, _| {
        let out = h.spread(&acc)?;
        h.define(&out, p(h, &m, "symbol")?, h.number(1.))?;
        Ok(out)
    })?;
    h.assign(&topics, s(h, "ticker"), ticker)?;
    let routing = h.operation(
        "resolveTransportRouting",
        vec![h.receiver("parent.name")?, h.receiver("parent.options")?],
    )?;
    let fallback = p(h, &p(h, &routing, "wsPublic")?, "fallback")?;
    let use_fallback = h.truthy(&h.receiver("useFallbackUrl")?) && h.truthy(&fallback);
    let url = if use_fallback {
        fallback
    } else {
        h.operation("BASE_WS_URL", vec![])?
    };
    if use_fallback {
        let _ = call(
            h,
            "parent.emitter.emit",
            vec![
                s(h, "grafana-info"),
                s(h, "Using tealstreet bitget proxy (public)"),
            ],
        );
    }
    let creator = h.capture("createWebsocket")?;
    let ws = h.invoke(creator, vec![url])?;
    h.set_receiver("ws", ws)?;
    for (event, handler) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let add = h.capture("ws.addEventListener")?;
        let cb = h.receiver(handler)?;
        h.invoke(add, vec![s(h, event), cb])?;
    }
    Ok(())
}
// ts: BitgetPublicWebsocket.onOpen
pub fn on_open<H: PublicDataHost>(h: &H) -> Result<(), H::Error> {
    if h.truthy(&h.receiver("isDisposed")?) {
        return Ok(());
    }
    call(h, "subscribe", vec![])?;
    call(h, "ping", vec![])?;
    for method in ["parent.emitter.off", "parent.emitter.on"] {
        let f = h.capture(method)?;
        let cb = h.receiver("onMarketsChanged")?;
        h.invoke(f, vec![s(h, "markets:changed"), cb])?;
    }
    if h.truthy(&h.operation("ENABLE_RECONNECTING_WEBSOCKET", vec![])?) {
        call(h, "emitWsReconnected", vec![])?;
    }
    Ok(())
}
// ts: BitgetPublicWebsocket#onMarketsChanged; argument prologue supplies `added`.
pub fn on_markets_changed<H: PublicDataHost>(h: &H, added: H::Value) -> Result<(), H::Error> {
    if h.equal(&p(h, &added, "length")?, &h.number(0.))
        || !h.truthy(&h.receiver("ws")?)
        || h.truthy(&h.receiver("isDisposed")?)
    {
        return Ok(());
    }
    let ticker = p(h, &h.receiver("topics")?, "ticker")?;
    let keys = h.operation(
        "Object.keys",
        vec![if h.truthy(&ticker) {
            ticker
        } else {
            h.object(vec![])
        }],
    )?;
    let count = h.numeric(&p(h, &keys, "length")?)?;
    let selected = h.filter(added, |_, i| Ok((count + i as f64) % 3. == 0.))?;
    let args = h.map(selected, |m, _| {
        let product = h.receiver("parent.apiProductType")?;
        let upper = h.operation("toUpperCase", vec![product])?;
        Ok(h.object(vec![
            ("instType", upper),
            ("channel", s(h, "ticker")),
            ("instId", p(h, &m, "symbol")?),
        ]))
    })?;
    if h.equal(&p(h, &args, "length")?, &h.number(0.)) {
        return Ok(());
    }
    let mut i = h.iterator(&args)?;
    while let Some(arg) = h.next(&mut i)? {
        if !h.truthy(&p(h, &h.receiver("topics")?, "ticker")?) {
            let topics = h.receiver("topics")?;
            h.assign(&topics, s(h, "ticker"), h.object(vec![]))?;
        }
        let ticker = p(h, &h.receiver("topics")?, "ticker")?;
        let key = p(h, &arg, "instId")?;
        h.assign(&ticker, key, h.number(1.))?;
    }
    let send = h.capture("send")?;
    let payload = h.object(vec![("op", s(h, "subscribe")), ("args", args)]);
    let text = h.operation("JSON.stringify", vec![payload])?;
    h.invoke(send, vec![text])?;
    Ok(())
}
// ts: BitgetPublicWebsocket.subscribe
pub fn subscribe<H: PublicDataHost>(h: &H) -> Result<(), H::Error> {
    let entries = h.operation("Object.entries", vec![h.receiver("topics")?])?;
    // Native flatMap's callback returns dense arrays produced by native map.
    let args = h.flat_map(entries, |entry, _| {
        let channel = h.index(&entry, 0)?;
        let symbols = h.index(&entry, 1)?;
        let keys = h.operation("Object.keys", vec![symbols])?;
        h.map(keys, |inst_id, _| {
            let product = h.receiver("parent.apiProductType")?;
            let upper = h.operation("toUpperCase", vec![product])?;
            Ok(h.object(vec![
                ("instType", upper),
                ("channel", channel.clone()),
                ("instId", inst_id),
            ]))
        })
    })?;
    let send = h.capture("send")?;
    let payload = h.object(vec![("op", s(h, "subscribe")), ("args", args)]);
    let text = h.operation("JSON.stringify", vec![payload])?;
    h.invoke(send, vec![text])?;
    Ok(())
}
