use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
};

pub type StreamCallback<V, E> = Rc<dyn Fn(V) -> Result<(), E>>;
pub type StreamFuture<E> = Pin<Box<dyn Future<Output = Result<(), E>>>>;
pub struct StreamOps<V, E> {
    pub get: Rc<dyn Fn(&V, &str) -> Result<V, E>>,
    pub set: Rc<dyn Fn(&V, &str, V) -> Result<(), E>>,
    pub capture: Rc<dyn Fn(&str) -> Result<V, E>>,
    pub invoke: Rc<dyn Fn(V, Vec<V>) -> Result<V, E>>,
    pub intrinsic: Rc<dyn Fn(&str, Vec<V>) -> Result<V, E>>,
    pub truthy: Rc<dyn Fn(&V) -> bool>,
    pub nullish: Rc<dyn Fn(&V) -> bool>,
    pub equal: Rc<dyn Fn(&V, &V) -> bool>,
    pub literal: Rc<dyn Fn(&str) -> V>,
    pub number: Rc<dyn Fn(f64) -> V>,
    pub undefined: Rc<dyn Fn() -> V>,
    pub array: Rc<dyn Fn(Vec<V>) -> V>,
    pub object: Rc<dyn Fn(Vec<(&str, V)>) -> V>,
    pub callback: Rc<dyn Fn(&str, usize, StreamCallback<V, E>) -> V>,
    pub map: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<V, E>>) -> Result<V, E>>,
    pub filter: Rc<dyn Fn(V, Rc<dyn Fn(V) -> Result<bool, E>>) -> Result<V, E>>,
    pub for_of: Rc<dyn Fn(V, StreamCallback<V, E>) -> Result<(), E>>,
    pub await_value: Rc<dyn Fn(V) -> Pin<Box<dyn Future<Output = Result<V, E>>>>>,
    pub spawn: Rc<dyn Fn(StreamFuture<E>)>,
    pub error_value: Rc<dyn Fn(&E) -> V>,
}
fn optional<V: Clone, E>(h: &StreamOps<V, E>, v: &V, key: &str) -> Result<V, E> {
    if (h.nullish)(v) {
        Ok((h.undefined)())
    } else {
        (h.get)(v, key)
    }
}
fn parse<V, E>(h: &StreamOps<V, E>, evaluate: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    let callee = (h.capture)("parseFloat")?;
    let value = evaluate()?;
    (h.invoke)(callee, vec![value])
}
fn integer<V, E>(h: &StreamOps<V, E>, evaluate: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    let callee = (h.capture)("parseInt")?;
    let value = evaluate()?;
    (h.invoke)(callee, vec![value, (h.number)(10.0)])
}
fn fallback<V, E>(h: &StreamOps<V, E>, v: V, next: impl FnOnce() -> Result<V, E>) -> Result<V, E> {
    if (h.truthy)(&v) { Ok(v) } else { next() }
}
fn null_fallback<V, E>(
    h: &StreamOps<V, E>,
    v: V,
    next: impl FnOnce() -> Result<V, E>,
) -> Result<V, E> {
    if !(h.nullish)(&v) { Ok(v) } else { next() }
}
pub fn handle_ticker_v2<V: Clone, E>(h: &StreamOps<V, E>, json: V) -> Result<(), E> {
    let data = (h.get)(&json, "data")?;
    let callee = (h.capture)("this.tickerForApiSymbol")?;
    let ticker = (h.invoke)(callee, vec![optional(h, &data, "symbol")?])?;
    if !(h.truthy)(&ticker) {
        return Ok(());
    }
    let update = (h.capture)("this.parent.store.updateTicker")?;
    let bid_parser = (h.capture)("parseFloat")?;
    let bid = fallback(h, (h.get)(&data, "bestBidPrice")?, || {
        fallback(h, (h.get)(&ticker, "bid")?, || Ok((h.literal)("0")))
    })?;
    let bid = (h.invoke)(bid_parser, vec![bid])?;
    let ask_parser = (h.capture)("parseFloat")?;
    let ask = fallback(h, (h.get)(&data, "bestAskPrice")?, || {
        fallback(h, (h.get)(&ticker, "ask")?, || Ok((h.literal)("0")))
    })?;
    let ask = (h.invoke)(ask_parser, vec![ask])?;
    (h.invoke)(
        update,
        vec![ticker, (h.object)(vec![("bid", bid), ("ask", ask)])],
    )?;
    Ok(())
}
pub fn handle_snapshot<V: Clone, E>(h: &StreamOps<V, E>, json: V) -> Result<(), E> {
    let data = (h.get)(&json, "data")?;
    let callee = (h.capture)("this.tickerForApiSymbol")?;
    let ticker = (h.invoke)(callee, vec![optional(h, &data, "symbol")?])?;
    if !(h.truthy)(&ticker) {
        return Ok(());
    }
    let update = (h.object)(vec![]);
    for (source, target) in [
        ("lastPrice", "last"),
        ("priceChgPct", "percentage"),
        ("fundingRate", "fundingRate"),
        ("volume", "volume"),
        ("turnover", "quoteVolume"),
    ] {
        if !(h.nullish)(&(h.get)(&data, source)?) {
            let value = parse(h, || (h.get)(&data, source))?;
            let value = if source == "priceChgPct" {
                (h.intrinsic)("multiply-100", vec![value])?
            } else {
                value
            };
            (h.set)(&update, target, value)?;
        }
    }
    if (h.truthy)(&(h.intrinsic)("Object.keys-length", vec![update.clone()])?) {
        let callee = (h.capture)("this.parent.store.updateTicker")?;
        (h.invoke)(callee, vec![ticker, update])?;
    }
    Ok(())
}
pub fn handle_instrument<V: Clone, E>(h: &StreamOps<V, E>, json: V) -> Result<(), E> {
    let data = (h.get)(&json, "data")?;
    if !(h.truthy)(&data) {
        return Ok(());
    }
    let callee = (h.capture)("this.tickerForApiSymbol")?;
    let topic = fallback(h, (h.get)(&json, "topic")?, || Ok((h.literal)("")))?;
    let symbol = (h.intrinsic)("split-colon-index-one", vec![topic])?;
    let ticker = (h.invoke)(callee, vec![symbol])?;
    if !(h.truthy)(&ticker) {
        return Ok(());
    }
    let update = (h.object)(vec![]);
    for (source, target) in [("markPrice", "mark"), ("indexPrice", "index")] {
        if !(h.nullish)(&(h.get)(&data, source)?) {
            let value = parse(h, || (h.get)(&data, source))?;
            (h.set)(&update, target, value)?;
        }
    }
    if (h.truthy)(&(h.intrinsic)("Object.keys-length", vec![update.clone()])?) {
        let callee = (h.capture)("this.parent.store.updateTicker")?;
        (h.invoke)(callee, vec![ticker, update])?;
    }
    Ok(())
}
pub fn handle_spot_ticker_snapshot<V: Clone, E>(h: &StreamOps<V, E>, json: V) -> Result<(), E> {
    let data = (h.get)(&json, "data")?;
    let symbol = fallback(h, optional(h, &data, "symbol")?, || {
        fallback(h, (h.get)(&json, "subject")?, || {
            let topic = fallback(h, (h.get)(&json, "topic")?, || Ok((h.literal)("")))?;
            (h.intrinsic)("template-split-colon-index-one", vec![topic])
        })
    })?;
    if !(h.truthy)(&data) || !(h.truthy)(&symbol) || (h.equal)(&symbol, &(h.literal)("all")) {
        return Ok(());
    }
    let tickers = (h.capture)("this.parent.store.tickersBySymbol")?;
    let ticker = (h.intrinsic)("get-index", vec![tickers, symbol])?;
    if !(h.truthy)(&ticker) {
        return Ok(());
    }
    let mut values = Vec::new();
    for keys in [
        vec!["bestBid", "bestBidPrice", "buy"],
        vec!["bestAsk", "bestAskPrice", "sell"],
        vec!["price", "last", "lastPrice"],
        vec!["changeRate"],
        vec!["volume", "vol", "volume24h"],
        vec!["turnover", "volValue", "turnover24h"],
    ] {
        let callee = (h.capture)("parseNumber")?;
        let mut value = (h.get)(&data, keys[0])?;
        for key in keys.iter().skip(1) {
            value = null_fallback(h, value, || (h.get)(&data, key))?;
        }
        values.push((h.invoke)(callee, vec![value])?);
    }
    let update = (h.object)(vec![]);
    for (i, key) in [
        (0, "bid"),
        (1, "ask"),
        (2, "last"),
        (3, "percentage"),
        (4, "volume"),
        (5, "quoteVolume"),
    ] {
        if !(h.equal)(&values[i], &(h.undefined)()) {
            let value = if i == 3 {
                (h.intrinsic)("multiply-100", vec![values[i].clone()])?
            } else {
                values[i].clone()
            };
            (h.set)(&update, key, value)?;
            if i == 2 {
                (h.set)(&update, "mark", values[i].clone())?;
                (h.set)(&update, "index", values[i].clone())?;
            }
        }
    }
    if (h.truthy)(&(h.intrinsic)("Object.keys-length", vec![update.clone()])?) {
        let callee = (h.capture)("this.parent.store.updateTicker")?;
        (h.invoke)(callee, vec![ticker, update])?;
    }
    Ok(())
}
fn append_handler<V: Clone, E>(h: &StreamOps<V, E>, topic: V, parser: V) -> Result<(), E> {
    let lhs = (h.capture)("this.messageHandlers")?;
    let rhs = (h.capture)("this.messageHandlers")?;
    let old = (h.intrinsic)("get-index", vec![rhs, topic.clone()])?;
    let old = fallback(h, old, || Ok((h.array)(vec![])))?;
    let next = (h.intrinsic)("spread-append", vec![old, parser])?;
    (h.intrinsic)("set-index", vec![lhs, topic, next])?;
    Ok(())
}
fn remove_handler<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    topic: V,
    parser: V,
    spread: bool,
) -> Result<(), E> {
    let lhs = (h.capture)("this.messageHandlers")?;
    let rhs = (h.capture)("this.messageHandlers")?;
    let old = (h.intrinsic)("get-index", vec![rhs, topic.clone()])?;
    let old = fallback(&h, old, || Ok((h.array)(vec![])))?;
    let ops = h.clone();
    let next = (h.filter)(old, Rc::new(move |value| Ok(!(ops.equal)(&value, &parser))))?;
    let next = if spread {
        (h.intrinsic)("array-spread", vec![next])?
    } else {
        next
    };
    (h.intrinsic)("set-index", vec![lhs, topic, next])?;
    Ok(())
}
fn send_frame<V: Clone, E>(
    h: &StreamOps<V, E>,
    kind: &str,
    topic: impl FnOnce() -> Result<V, E>,
    inline: bool,
) -> Result<(), E> {
    let send = if inline {
        Some((h.capture)("this.send")?)
    } else {
        None
    };
    let stringify = if inline {
        Some((h.capture)("JSON.stringify")?)
    } else {
        None
    };
    let id = (h.capture)("Date.now")?;
    let id = (h.invoke)(id, vec![])?;
    let payload = (h.object)(vec![
        ("id", id),
        ("type", (h.literal)(kind)),
        ("topic", topic()?),
        ("privateChannel", (h.intrinsic)("false", vec![])?),
        ("response", (h.intrinsic)("true", vec![])?),
    ]);
    let send = match send {
        Some(c) => c,
        None => (h.capture)("this.send")?,
    };
    let stringify = match stringify {
        Some(c) => c,
        None => (h.capture)("JSON.stringify")?,
    };
    let text = (h.invoke)(stringify, vec![payload])?;
    (h.invoke)(send, vec![text])?;
    Ok(())
}
pub fn candle_parser<V: Clone, E>(
    h: &StreamOps<V, E>,
    message: V,
    callback: V,
    spot: bool,
) -> Result<(), E> {
    let data = (h.get)(&message, "data")?;
    if !spot && !(h.truthy)(&data) {
        return Ok(());
    }
    let raw = if spot {
        optional(h, &data, "candles")?
    } else {
        (h.get)(&(h.get)(&message, "data")?, "candles")?
    };
    let mut fields = Vec::new();
    if (h.truthy)(&(h.intrinsic)("Array.isArray", vec![raw.clone()])?) {
        let timestamp = if spot {
            let callee = (h.capture)("normalizeTimestamp")?;
            (h.invoke)(callee, vec![(h.get)(&raw, "0")?])?
        } else {
            integer(h, || (h.get)(&raw, "0"))?
        };
        fields.push(("timestamp", timestamp));
        for (key, index) in [
            ("open", "1"),
            ("close", "2"),
            ("high", "3"),
            ("low", "4"),
            ("volume", "5"),
        ] {
            fields.push((key, parse(h, || (h.get)(&raw, index))?));
        }
    } else if spot {
        return Ok(());
    } else {
        fields.push((
            "timestamp",
            (h.intrinsic)(
                "divide-1000",
                vec![(h.get)(&(h.get)(&message, "data")?, "time")?],
            )?,
        ));
        for key in ["open", "high", "low", "close", "volume"] {
            fields.push((key, parse(h, || (h.get)(&(h.get)(&message, "data")?, key))?));
        }
    }
    (h.invoke)(callback, vec![(h.object)(fields)])?;
    Ok(())
}
pub fn wait_candle<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    topic: V,
    api: V,
    interval: V,
    parser: V,
) -> Result<(), E> {
    if (h.truthy)(&(h.capture)("this.isConnected")?) {
        if !(h.truthy)(&(h.capture)("this.isDisposed")?) {
            append_handler(&h, topic, parser)?;
            send_frame(
                &h,
                "subscribe",
                || (h.intrinsic)("futures-candle-topic", vec![api, interval]),
                false,
            )?;
        }
    } else {
        let timer = (h.capture)("setTimeout")?;
        let ops = h.clone();
        let callback = (h.callback)(
            "",
            0,
            Rc::new(move |_| {
                wait_candle(
                    ops.clone(),
                    topic.clone(),
                    api.clone(),
                    interval.clone(),
                    parser.clone(),
                )
            }),
        );
        (h.invoke)(timer, vec![callback, (h.number)(100.0)])?;
    }
    Ok(())
}
pub fn listen_ohlcv<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    opts: V,
    callback: V,
) -> Result<V, E> {
    let table = (h.capture)("WS_INTERVAL")?;
    let interval = (h.get)(&opts, "interval")?;
    let interval = (h.intrinsic)("get-index", vec![table, interval])?;
    let symbol = (h.get)(&opts, "symbol")?;
    let callee = (h.capture)("this.parent.toApiSymbol")?;
    let api = (h.invoke)(callee, vec![symbol.clone()])?;
    let topic = (h.intrinsic)("candle-topic", vec![symbol, interval.clone()])?;
    let ops = h.clone();
    let parser = (h.callback)(
        "parser",
        1,
        Rc::new(move |message| candle_parser(&ops, message, callback.clone(), false)),
    );
    wait_candle(
        h.clone(),
        topic.clone(),
        api.clone(),
        interval.clone(),
        parser.clone(),
    )?;
    let ops = h.clone();
    Ok((h.callback)(
        "",
        0,
        Rc::new(move |_| {
            remove_handler(ops.clone(), topic.clone(), parser.clone(), true)?;
            if (ops.truthy)(&(ops.capture)("this.isConnected")?) {
                send_frame(
                    &ops,
                    "unsubscribe",
                    || (ops.intrinsic)("futures-candle-topic", vec![api.clone(), interval.clone()]),
                    true,
                )?;
                let cleanup = (ops.capture)("this.cleanupMessageHandlerStats")?;
                (ops.invoke)(cleanup, vec![topic.clone()])?;
            }
            Ok(())
        }),
    ))
}
fn spot_cleanup<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    topic: V,
    ws_topic: V,
    parser: V,
) -> Result<(), E> {
    remove_handler(h.clone(), topic.clone(), parser, false)?;
    let handlers = (h.capture)("this.messageHandlers")?;
    let current = (h.intrinsic)("get-index", vec![handlers, topic.clone()])?;
    if (h.equal)(&(h.get)(&current, "length")?, &(h.number)(0.0)) {
        let handlers = (h.capture)("this.messageHandlers")?;
        (h.intrinsic)("delete-index", vec![handlers, topic.clone()])?;
        let cleanup = (h.capture)("this.cleanupMessageHandlerStats")?;
        (h.invoke)(cleanup, vec![topic])?;
        let unsub = (h.capture)("this.unsubscribeTopic")?;
        (h.invoke)(unsub, vec![ws_topic])?;
    }
    Ok(())
}
pub fn listen_spot_ohlcv<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    opts: V,
    callback: V,
) -> Result<V, E> {
    let table = (h.capture)("KUCOIN_SPOT_INTERVAL")?;
    let interval = (h.get)(&opts, "interval")?;
    let interval = (h.intrinsic)("get-index", vec![table, interval])?;
    let symbol = (h.get)(&opts, "symbol")?;
    let topic = (h.intrinsic)("candle-topic", vec![symbol, interval.clone()])?;
    let symbol = (h.get)(&opts, "symbol")?;
    let ws_topic = (h.intrinsic)("spot-candle-topic", vec![symbol, interval])?;
    let ops = h.clone();
    let parser = (h.callback)(
        "parser",
        1,
        Rc::new(move |message| candle_parser(&ops, message, callback.clone(), true)),
    );
    append_handler(&h, topic.clone(), parser.clone())?;
    let sub = (h.capture)("this.subscribeTopic")?;
    (h.invoke)(sub, vec![ws_topic.clone()])?;
    let ops = h.clone();
    Ok((h.callback)(
        "",
        0,
        Rc::new(move |_| {
            spot_cleanup(ops.clone(), topic.clone(), ws_topic.clone(), parser.clone())
        }),
    ))
}
fn map_levels<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    rows: V,
    template: bool,
) -> Result<V, E> {
    let ops = h.clone();
    (h.map)(
        rows,
        Rc::new(move |row| {
            let pair = (ops.intrinsic)("destructure-two", vec![row])?;
            let price = (ops.get)(&pair, "0")?;
            let amount = (ops.get)(&pair, "1")?;
            let price = parse(&ops, || {
                if template {
                    (ops.intrinsic)("template-string", vec![price])
                } else {
                    Ok(price)
                }
            })?;
            let amount = parse(&ops, || {
                if template {
                    (ops.intrinsic)("template-string", vec![amount])
                } else {
                    Ok(amount)
                }
            })?;
            Ok((ops.object)(vec![
                ("price", price),
                ("amount", amount),
                ("total", (ops.number)(0.0)),
            ]))
        }),
    )
}
pub fn spot_book_parser<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    message: V,
    callback: V,
) -> Result<(), E> {
    let data = (h.get)(&message, "data")?;
    if !(h.truthy)(&data) {
        return Ok(());
    }
    let mut sides = Vec::new();
    for side in ["bids", "asks"] {
        let rows = fallback(&h, (h.get)(&data, side)?, || Ok((h.array)(vec![])))?;
        let rows = map_levels(h.clone(), rows, false)?;
        let ops = h.clone();
        let rows = (h.filter)(
            rows,
            Rc::new(move |row| {
                let price = (ops.get)(&row, "price")?;
                let amount = (ops.get)(&row, "amount")?;
                if !(ops.truthy)(&(ops.intrinsic)("Number.isFinite", vec![price])?) {
                    return Ok(false);
                }
                Ok((ops.truthy)(&(ops.intrinsic)(
                    "Number.isFinite",
                    vec![amount],
                )?))
            }),
        )?;
        sides.push((side, rows));
    }
    let book = (h.object)(sides);
    let sort = (h.capture)("sortOrderBook")?;
    (h.invoke)(sort, vec![book.clone()])?;
    let total = (h.capture)("calcOrderBookTotal")?;
    (h.invoke)(total, vec![book.clone()])?;
    (h.invoke)(callback, vec![book])?;
    Ok(())
}
pub fn listen_spot_order_book<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    symbol: V,
    callback: V,
) -> Result<V, E> {
    let topic = (h.intrinsic)("book-topic", vec![symbol.clone()])?;
    let ws_topic = (h.intrinsic)("spot-book-topic", vec![symbol])?;
    let ops = h.clone();
    let parser = (h.callback)(
        "parser",
        1,
        Rc::new(move |message| spot_book_parser(ops.clone(), message, callback.clone())),
    );
    append_handler(&h, topic.clone(), parser.clone())?;
    let sub = (h.capture)("this.subscribeTopic")?;
    (h.invoke)(sub, vec![ws_topic.clone()])?;
    let ops = h.clone();
    Ok((h.callback)(
        "",
        0,
        Rc::new(move |_| {
            spot_cleanup(ops.clone(), topic.clone(), ws_topic.clone(), parser.clone())
        }),
    ))
}

pub struct BookState<V> {
    pub timeout: RefCell<V>,
    pub loaded: Cell<bool>,
    pub loading: Cell<bool>,
    pub refetch: Cell<bool>,
    pub last: RefCell<V>,
    pub buffered: RefCell<V>,
    pub book: V,
}
pub fn apply_snapshot<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    s: &BookState<V>,
    snapshot: V,
) -> Result<(), E> {
    for side in ["bids", "asks"] {
        let rows = fallback(&h, (h.get)(&snapshot, side)?, || Ok((h.array)(vec![])))?;
        let mapped = map_levels(h.clone(), rows, true)?;
        (h.set)(&s.book, side, mapped)?;
    }
    let sequence = integer(&h, || {
        fallback(&h, (h.get)(&snapshot, "sequence")?, || {
            fallback(&h, (h.get)(&snapshot, "sn")?, || Ok((h.literal)("0")))
        })
    })?;
    if (h.truthy)(&sequence) {
        *s.last.borrow_mut() = sequence;
    }
    Ok(())
}
pub fn emit_book<V: Clone, E>(h: &StreamOps<V, E>, s: &BookState<V>, callback: V) -> Result<(), E> {
    let total = (h.capture)("calcOrderBookTotal")?;
    (h.invoke)(total, vec![s.book.clone()])?;
    (h.invoke)(callback, vec![s.book.clone()])?;
    Ok(())
}
pub fn recover_gap<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    s: Rc<BookState<V>>,
    message: V,
    api: V,
    symbol: V,
    callback: V,
) -> Result<(), E> {
    s.loaded.set(false);
    *s.buffered.borrow_mut() = (h.array)(vec![message]);
    if s.loading.get() {
        s.refetch.set(true);
        return Ok(());
    }
    let ops = h.clone();
    (h.spawn)(Box::pin(async move {
        fetch_snapshot(ops, s, api, symbol, callback).await
    }));
    Ok(())
}
pub fn apply_increment<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    s: Rc<BookState<V>>,
    message: V,
    api: V,
    symbol: V,
    callback: V,
) -> Result<bool, E> {
    let payload = fallback(&h, (h.get)(&message, "data")?, || Ok((h.object)(vec![])))?;
    let sequence = integer(&h, || {
        fallback(&h, (h.get)(&payload, "sequence")?, || {
            fallback(&h, (h.get)(&message, "sn")?, || Ok((h.literal)("0")))
        })
    })?;
    let last = s.last.borrow().clone();
    if (h.truthy)(&sequence)
        && (h.truthy)(&last)
        && (h.truthy)(&(h.intrinsic)(
            "less-equal",
            vec![sequence.clone(), last.clone()],
        )?)
    {
        return Ok(false);
    }
    let last = s.last.borrow().clone();
    if (h.truthy)(&sequence)
        && (h.truthy)(&last)
        && (h.truthy)(&(h.intrinsic)(
            "greater-last-plus-one",
            vec![sequence.clone(), last],
        )?)
    {
        recover_gap(h, s, message, api, symbol, callback)?;
        return Ok(false);
    }
    if (h.truthy)(&(h.intrinsic)(
        "typeof-string",
        vec![(h.get)(&payload, "change")?],
    )?) {
        let change = (h.get)(&payload, "change")?;
        let parts = (h.intrinsic)("split-comma", vec![change])?;
        let side = if (h.equal)(&(h.get)(&parts, "1")?, &(h.literal)("buy")) {
            "bids"
        } else {
            "asks"
        };
        let update = (h.capture)("updateOrderBookByPrice")?;
        let price = parse(&h, || (h.get)(&parts, "0"))?;
        let amount = parse(&h, || (h.get)(&parts, "2"))?;
        (h.invoke)(
            update,
            vec![s.book.clone(), (h.literal)(side), price, amount],
        )?;
        if (h.truthy)(&sequence) {
            *s.last.borrow_mut() = sequence;
        }
        return Ok(true);
    }
    if (h.equal)(&(h.get)(&message, "type")?, &(h.literal)("snapshot")) {
        apply_snapshot(h, &s, payload)?;
        return Ok(true);
    }
    if (h.equal)(&(h.get)(&message, "type")?, &(h.literal)("update")) {
        for (field, side) in [("bids", "bids"), ("asks", "asks")] {
            let rows = fallback(&h, (h.get)(&payload, field)?, || Ok((h.array)(vec![])))?;
            let ops = h.clone();
            let state = s.clone();
            (h.for_of)(
                rows,
                Rc::new(move |row| {
                    let update = (ops.capture)("updateOrderBookByPrice")?;
                    let price = parse(&ops, || (ops.get)(&row, "0"))?;
                    let amount = parse(&ops, || (ops.get)(&row, "1"))?;
                    (ops.invoke)(
                        update,
                        vec![state.book.clone(), (ops.literal)(side), price, amount],
                    )?;
                    Ok(())
                }),
            )?;
        }
        if (h.truthy)(&sequence) {
            *s.last.borrow_mut() = sequence;
        }
        return Ok(true);
    }
    Ok(false)
}
pub async fn fetch_snapshot<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    s: Rc<BookState<V>>,
    api: V,
    symbol: V,
    callback: V,
) -> Result<(), E> {
    if s.loading.get() || (h.truthy)(&(h.capture)("this.isDisposed")?) {
        return Ok(());
    }
    s.loading.set(true);
    let loaded = async {
        let get = (h.capture)("this.parent.xhr.get")?;
        let endpoints = (h.capture)("ENDPOINTS")?;
        let route = (h.get)(&endpoints, "ORDERBOOK")?;
        let params = (h.object)(vec![("symbol", api.clone())]);
        let response = (h.invoke)(get, vec![route, (h.object)(vec![("params", params)])])?;
        let response = (h.await_value)(response).await?;
        let data = (h.get)(&response, "data")?;
        if !(h.equal)(&(h.get)(&data, "code")?, &(h.literal)("200000"))
            || !(h.truthy)(&(h.get)(&data, "data")?)
        {
            let emitter = (h.capture)("this.parent.emitAndLogError")?;
            let msg = fallback(&h, (h.get)(&data, "msg")?, || {
                (h.intrinsic)("snapshot-error", vec![symbol.clone()])
            })?;
            (h.invoke)(emitter, vec![msg])?;
            return Ok(());
        }
        apply_snapshot(h.clone(), &s, (h.get)(&data, "data")?)?;
        s.loaded.set(true);
        let buffered = s.buffered.borrow().clone();
        let ops = h.clone();
        let state = s.clone();
        let replay = (h.filter)(
            buffered,
            Rc::new(move |update| {
                let seq = integer(&ops, || {
                    let data = (ops.get)(&update, "data")?;
                    fallback(&ops, optional(&ops, &data, "sequence")?, || {
                        fallback(&ops, (ops.get)(&update, "sn")?, || Ok((ops.literal)("0")))
                    })
                })?;
                let last = state.last.borrow().clone();
                Ok(!(ops.truthy)(&seq)
                    || !(ops.truthy)(&last)
                    || (ops.truthy)(&(ops.intrinsic)("greater-than", vec![seq, last])?))
            }),
        )?;
        *s.buffered.borrow_mut() = (h.array)(vec![]);
        let ops = h.clone();
        let state = s.clone();
        let api = api.clone();
        let symbol = symbol.clone();
        let cb = callback.clone();
        (h.for_of)(
            replay,
            Rc::new(move |update| {
                apply_increment(
                    ops.clone(),
                    state.clone(),
                    update,
                    api.clone(),
                    symbol.clone(),
                    cb.clone(),
                )?;
                Ok(())
            }),
        )?;
        if s.loaded.get() {
            emit_book(&h, &s, callback.clone())?;
        }
        Ok(())
    }
    .await;
    let caught = match loaded {
        Ok(()) => Ok(()),
        Err(error) => (|| {
            let emitter = (h.capture)("this.parent.emitAndLogError")?;
            let err = (h.error_value)(&error);
            let response = optional(&h, &err, "response")?;
            let data = optional(&h, &response, "data")?;
            let msg = fallback(&h, optional(&h, &data, "msg")?, || {
                fallback(&h, optional(&h, &err, "message")?, || {
                    (h.intrinsic)("book-error", vec![symbol.clone()])
                })
            })?;
            (h.invoke)(emitter, vec![msg])?;
            Ok(())
        })(),
    };
    s.loading.set(false);
    if s.refetch.get() && !(h.truthy)(&(h.capture)("this.isDisposed")?) {
        s.refetch.set(false);
        let ops = h.clone();
        (h.spawn)(Box::pin(async move {
            fetch_snapshot(ops, s, api, symbol, callback).await
        }));
    }
    caught
}
pub fn book_parser<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    s: Rc<BookState<V>>,
    message: V,
    api: V,
    symbol: V,
    callback: V,
) -> Result<(), E> {
    if !(h.truthy)(&(h.get)(&message, "data")?) {
        return Ok(());
    }
    if !s.loaded.get() {
        let buffered = s.buffered.borrow().clone();
        (h.intrinsic)("array-push", vec![buffered, message])?;
        let ops = h.clone();
        (h.spawn)(Box::pin(async move {
            fetch_snapshot(ops, s, api, symbol, callback).await
        }));
        return Ok(());
    }
    if apply_increment(h.clone(), s.clone(), message, api, symbol, callback.clone())? {
        emit_book(&h, &s, callback)?;
    }
    Ok(())
}
pub fn wait_book<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    s: Rc<BookState<V>>,
    topic: V,
    parser: V,
    api: V,
    symbol: V,
    callback: V,
) -> Result<(), E> {
    if (h.truthy)(&(h.capture)("this.isConnected")?) {
        if !(h.truthy)(&(h.capture)("this.isDisposed")?) {
            append_handler(&h, topic, parser)?;
            send_frame(
                &h,
                "subscribe",
                || (h.intrinsic)("futures-book-topic", vec![api.clone()]),
                false,
            )?;
            let active = (h.capture)("this.subscribeActiveTicker")?;
            (h.invoke)(active, vec![api.clone()])?;
            let ops = h.clone();
            (h.spawn)(Box::pin(async move {
                fetch_snapshot(ops, s, api, symbol, callback).await
            }));
        }
    } else {
        let timer = (h.capture)("setTimeout")?;
        let ops = h.clone();
        let state = s.clone();
        let cb = (h.callback)(
            "",
            0,
            Rc::new(move |_| {
                wait_book(
                    ops.clone(),
                    state.clone(),
                    topic.clone(),
                    parser.clone(),
                    api.clone(),
                    symbol.clone(),
                    callback.clone(),
                )
            }),
        );
        let id = (h.invoke)(timer, vec![cb, (h.number)(100.0)])?;
        *s.timeout.borrow_mut() = id;
    }
    Ok(())
}
pub fn listen_order_book<V: Clone + 'static, E: 'static>(
    h: Rc<StreamOps<V, E>>,
    symbol: V,
    callback: V,
) -> Result<V, E> {
    let book = (h.object)(vec![
        ("bids", (h.array)(vec![])),
        ("asks", (h.array)(vec![])),
    ]);
    let s = Rc::new(BookState {
        timeout: RefCell::new((h.intrinsic)("null", vec![])?),
        loaded: Cell::new(false),
        loading: Cell::new(false),
        refetch: Cell::new(false),
        last: RefCell::new((h.number)(0.0)),
        buffered: RefCell::new((h.array)(vec![])),
        book,
    });
    let api = (h.capture)("this.parent.toApiSymbol")?;
    let api = (h.invoke)(api, vec![symbol.clone()])?;
    let topic = (h.intrinsic)("book-topic", vec![symbol.clone()])?;
    let ops = h.clone();
    let state = s.clone();
    let ap = api.clone();
    let sy = symbol.clone();
    let cb = callback.clone();
    let parser = (h.callback)(
        "parser",
        1,
        Rc::new(move |message| {
            book_parser(
                ops.clone(),
                state.clone(),
                message,
                ap.clone(),
                sy.clone(),
                cb.clone(),
            )
        }),
    );
    wait_book(
        h.clone(),
        s.clone(),
        topic.clone(),
        parser.clone(),
        api.clone(),
        symbol,
        callback,
    )?;
    let ops = h.clone();
    Ok((h.callback)(
        "",
        0,
        Rc::new(move |_| {
            remove_handler(ops.clone(), topic.clone(), parser.clone(), true)?;
            (ops.set)(&s.book, "asks", (ops.array)(vec![]))?;
            (ops.set)(&s.book, "bids", (ops.array)(vec![]))?;
            let cleanup = (ops.capture)("this.cleanupMessageHandlerStats")?;
            (ops.invoke)(cleanup, vec![topic.clone()])?;
            let timeout = s.timeout.borrow().clone();
            if (ops.truthy)(&timeout) {
                let clear = (ops.capture)("clearTimeout")?;
                let current = s.timeout.borrow().clone();
                (ops.invoke)(clear, vec![current])?;
                *s.timeout.borrow_mut() = (ops.intrinsic)("null", vec![])?;
            }
            if (ops.truthy)(&(ops.capture)("this.isConnected")?) {
                send_frame(
                    &ops,
                    "unsubscribe",
                    || (ops.intrinsic)("futures-book-topic", vec![api.clone()]),
                    false,
                )?;
                let unsub = (ops.capture)("this.unsubscribeActiveTicker")?;
                (ops.invoke)(unsub, vec![api.clone()])?;
            }
            Ok(())
        }),
    ))
}
