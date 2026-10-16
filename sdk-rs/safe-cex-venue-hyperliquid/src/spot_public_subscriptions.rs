//! Spot public subscriptions, captured timers/parsers and complete public socket assembly.
use crate::{
    spot_account_reads::SpotAccountHost, spot_client::SpotClientHost, spot_json::JsString,
    spot_ws_dispatch::SpotWsHost, ws_ping_lifecycle::WsPingHost,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Copy, Debug)]
pub enum PublicMethod {
    Connect,
    Open,
    SubscribePublic,
    SubscribeUser,
    Ping,
    Message,
    Mids,
    Candles,
    Book,
}
#[derive(Clone, Debug)]
pub struct Subscription<V> {
    pub socket: V,
    pub options: V,
    pub callback: V,
    pub symbol: V,
    pub topic: V,
    pub handler: V,
    pub parser: V,
    pub waiting: Rc<RefCell<V>>,
    pub book: Option<BookCapture<V>>,
}
#[derive(Clone, Debug)]
pub struct BookCapture<V> {
    pub socket: V,
    pub callback: V,
    pub sub_symbol: V,
    pub precision: V,
    pub inner_id: V,
    pub levels: V,
    pub books: V,
}
#[derive(Clone, Debug)]
pub enum PublicCallback<V> {
    CandleMarket {
        opts: V,
    },
    BookMarket {
        symbol: V,
    },
    CandleParser {
        socket: V,
        opts: V,
        callback: V,
        symbol: V,
        topic: V,
    },
    Wait(Rc<Subscription<V>>),
    Poll(Rc<Subscription<V>>),
    Cleanup(Rc<Subscription<V>>),
    Filter {
        parser: V,
    },
    BookReducer,
    BookParser(BookCapture<V>),
    BookLevel {
        book: V,
        side: &'static str,
    },
    BookPayload {
        operation: &'static str,
        symbol: V,
    },
    Send {
        socket: V,
    },
}
pub trait SpotPublicHost: SpotClientHost + SpotWsHost {
    fn append_literal(
        &mut self,
        array: &Self::Value,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn decrement(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn negate(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn public_callback(
        &mut self,
        capture: PublicCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn public_arrow(
        &mut self,
        owner: &Self::Value,
        name: &str,
        length: u8,
        body: PublicMethod,
    ) -> Result<Self::Value, Self::Error>;
    fn base_socket(
        &mut self,
        parent: &Self::Value,
        target: &Self::Value,
        prototype: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn assign_index(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn delete_index(&mut self, object: &Self::Value, key: &Self::Value) -> Result<(), Self::Error>;
    fn record_binding(
        &mut self,
        value: &Self::Value,
        keys: &[&str],
    ) -> Result<Vec<Self::Value>, Self::Error>;
}
fn n<H: SpotAccountHost>(h: &H, v: f64) -> H::Value {
    SpotAccountHost::number(h, v)
}
fn lit<H: WsPingHost>(h: &H, v: &str) -> H::Value {
    h.literal(&JsString(v.encode_utf16().collect()))
}
fn method<H: WsPingHost>(
    h: &mut H,
    o: &H::Value,
    k: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.get(o, k)?;
    let a = args(h)?;
    h.call(&f, o, &a)
}
fn index<H: SpotAccountHost>(h: &mut H, o: &H::Value, k: &H::Value) -> Result<H::Value, H::Error> {
    SpotAccountHost::index(h, o, k)
}
fn optional<H: SpotAccountHost>(h: &mut H, v: &H::Value, k: &str) -> Result<H::Value, H::Error> {
    if SpotAccountHost::is_nullish(h, v) {
        Ok(h.undefined())
    } else {
        h.get(v, k)
    }
}
fn field<H: WsPingHost>(h: &mut H, o: &H::Value, k: &str, v: &H::Value) -> Result<(), H::Error> {
    h.define_data(o, k, v)
}
fn free<H: SpotAccountHost>(
    h: &mut H,
    name: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let f = h.imported(name)?;
    let a = args(h)?;
    h.call(&f, &h.undefined(), &a)
}
fn parse<H: SpotAccountHost>(h: &mut H, v: &H::Value) -> Result<H::Value, H::Error> {
    let f = h.global("parseFloat")?;
    h.call(&f, &h.undefined(), &[v.clone()])
}
fn template<H: SpotAccountHost>(
    h: &mut H,
    prefix: &str,
    parts: &[&H::Value],
) -> Result<H::Value, H::Error> {
    let mut t: Vec<u16> = prefix.encode_utf16().collect();
    for p in parts {
        t.extend(SpotAccountHost::template_string(h, p)?.0)
    }
    Ok(h.literal(&JsString(t)))
}
fn packet<H: SpotPublicHost>(h: &mut H, socket: &H::Value, body: H::Value) -> Result<(), H::Error> {
    method(h, socket, "send", |h| {
        let json = h.global("JSON")?;
        Ok(vec![method(h, &json, "stringify", |_| Ok(vec![body]))?])
    })?;
    Ok(())
}
fn payload<H: SpotAccountHost>(
    h: &mut H,
    operation: &str,
    kind: &str,
    symbol: &H::Value,
    interval: Option<&H::Value>,
    group: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let body = h.object()?;
    field(h, &body, "method", &lit(h, operation))?;
    let sub = h.object()?;
    field(h, &sub, "type", &lit(h, kind))?;
    field(h, &sub, "coin", symbol)?;
    if let Some(interval) = interval {
        field(h, &sub, "interval", interval)?
    }
    if let Some(group) = group {
        let zero = n(h, 0.);
        let value = if SpotAccountHost::strict_equal(h, group, &zero) {
            SpotAccountHost::null(h)
        } else {
            group.clone()
        };
        field(h, &sub, "nSigFigs", &value)?
    }
    field(h, &body, "subscription", &sub)?;
    Ok(body)
}
pub struct HyperliquidSpotPublicWebSocket<V> {
    pub constructor: V,
    pub prototype: V,
}
pub fn public_socket_class<H: SpotPublicHost>(
    h: &mut H,
) -> Result<HyperliquidSpotPublicWebSocket<H::Value>, H::Error> {
    let (constructor, prototype) =
        h.class_pair("HyperliquidSpotPublicWebSocket", 1, Some("BaseWebSocket"))?;
    Ok(HyperliquidSpotPublicWebSocket {
        constructor,
        prototype,
    })
}
pub fn new_public_socket<H: SpotPublicHost>(
    h: &mut H,
    class: &HyperliquidSpotPublicWebSocket<H::Value>,
    parent: &H::Value,
    target: Option<&H::Value>,
) -> Result<H::Value, H::Error> {
    let socket = h.base_socket(
        parent,
        target.unwrap_or(&class.constructor),
        &class.prototype,
    )?;
    for key in ["topics", "messageHandlers"] {
        let v = h.object()?;
        h.define_data(&socket, key, &v)?
    }
    for (name, len, body) in [
        ("connectAndSubscribe", 0, PublicMethod::Connect),
        ("onOpen", 0, PublicMethod::Open),
        ("subscribePublicTopics", 0, PublicMethod::SubscribePublic),
        ("subscribeUserTopics", 0, PublicMethod::SubscribeUser),
        ("ping", 0, PublicMethod::Ping),
        ("onMessage", 1, PublicMethod::Message),
    ] {
        let f = h.public_arrow(&socket, name, len, body)?;
        h.define_data(&socket, name, &f)?
    }
    let buffer = h.object()?;
    field(h, &buffer, "last", &n(h, 0.))?;
    field(h, &buffer, "midPrice", &n(h, 0.))?;
    h.define_data(&socket, "_midsUpdateBuf", &buffer)?;
    for (name, len, body) in [
        ("handleAllMids", 1, PublicMethod::Mids),
        ("listenOHLCV", 2, PublicMethod::Candles),
        ("listenOrderBook", 2, PublicMethod::Book),
    ] {
        let f = h.public_arrow(&socket, name, len, body)?;
        h.define_data(&socket, name, &f)?
    }
    Ok(socket)
}
fn candle_topic<H: SpotPublicHost>(
    h: &mut H,
    symbol: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let mut units: Vec<u16> = "candle.".encode_utf16().collect();
    units.extend(SpotAccountHost::template_string(h, symbol)?.0);
    units.push(46);
    let interval = h.get(opts, "interval")?;
    units.extend(SpotAccountHost::template_string(h, &interval)?.0);
    Ok(h.literal(&JsString(units)))
}
pub fn listen_ohlcv<H: SpotPublicHost>(
    h: &mut H,
    socket: &H::Value,
    opts: &H::Value,
    callback: &H::Value,
) -> Result<H::Value, H::Error> {
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let markets = h.get(&store, "markets")?;
    let market = method(h, &markets, "find", |h| {
        Ok(vec![h.public_callback(PublicCallback::CandleMarket {
            opts: opts.clone(),
        })?])
    })?;
    let id = optional(h, &market, "id")?;
    let symbol = if h.truthy(&id) { id } else { lit(h, "") };
    let topic = candle_topic(h, &symbol, opts)?;
    let handler = candle_topic(h, &symbol, opts)?;
    let parser = h.public_callback(PublicCallback::CandleParser {
        socket: socket.clone(),
        opts: opts.clone(),
        callback: callback.clone(),
        symbol: symbol.clone(),
        topic: topic.clone(),
    })?;
    let state = Rc::new(Subscription {
        socket: socket.clone(),
        options: opts.clone(),
        callback: callback.clone(),
        symbol,
        topic,
        handler,
        parser,
        waiting: Rc::new(RefCell::new(h.undefined())),
        book: None,
    });
    run_wait(h, &state)?;
    h.public_callback(PublicCallback::Cleanup(state))
}
fn fail_book<H: SpotPublicHost>(
    h: &mut H,
    socket: &H::Value,
    symbol: &H::Value,
    suffix: &str,
) -> Result<H::Value, H::Error> {
    let parent = h.get(socket, "parent")?;
    method(h, &parent, "emitAndLogError", |h| {
        Ok(vec![
            template(h, "Market with symbol ", &[symbol, &lit(h, suffix)])?,
            lit(h, "listenOrderBook"),
        ])
    })?;
    let ctor = h.global("Error")?;
    let msg = template(h, "Market with symbol ", &[symbol, &lit(h, suffix)])?;
    let error = h.construct(&ctor, &[msg])?;
    Err(h.throw_value(error))
}
pub fn listen_order_book<H: SpotPublicHost>(
    h: &mut H,
    socket: &H::Value,
    symbol: &H::Value,
    callback: &H::Value,
) -> Result<H::Value, H::Error> {
    let parent = h.get(socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let markets = h.get(&store, "markets")?;
    let market = method(h, &markets, "find", |h| {
        Ok(vec![h.public_callback(PublicCallback::BookMarket {
            symbol: symbol.clone(),
        })?])
    })?;
    if !h.truthy(&market) {
        return fail_book(h, socket, symbol, " not found");
    }
    let sub = h.get(&market, "id")?;
    let precision = h.get(&market, "precision")?;
    let precision = h.get(&precision, "price")?;
    if !h.truthy(&precision) {
        return fail_book(h, socket, symbol, " has no price precision");
    }
    let topic = template(h, "l2Book.", &[&sub])?;
    let handler = template(h, "l2Book.", &[&sub])?;
    let inner_id = free(h, "nanoid", |_| Ok(vec![]))?;
    let grouping = SpotAccountHost::array(h, &(0..7).map(|i| n(h, i as f64)).collect::<Vec<_>>())?;
    let initial = h.object()?;
    let books = method(h, &grouping, "reduce", |h| {
        Ok(vec![
            h.public_callback(PublicCallback::BookReducer)?,
            initial,
        ])
    })?;
    let states = h.get(socket, "orderBookInnerState")?;
    let exists = index(h, &states, &inner_id)?;
    if !h.truthy(&exists) {
        let states = h.get(socket, "orderBookInnerState")?;
        let state = h.object()?;
        let updates = SpotAccountHost::array(h, &[])?;
        field(h, &state, "updates", &updates)?;
        field(
            h,
            &state,
            "isSnapshotLoaded",
            &SpotAccountHost::boolean(h, false),
        )?;
        h.assign_index(&states, &inner_id, &state)?;
    }
    let book = BookCapture {
        socket: socket.clone(),
        callback: callback.clone(),
        sub_symbol: sub.clone(),
        precision,
        inner_id,
        levels: grouping,
        books,
    };
    let parser = h.public_callback(PublicCallback::BookParser(book.clone()))?;
    let state = Rc::new(Subscription {
        socket: socket.clone(),
        options: h.undefined(),
        callback: callback.clone(),
        symbol: sub,
        topic,
        handler,
        parser,
        waiting: Rc::new(RefCell::new(h.undefined())),
        book: Some(book),
    });
    run_wait(h, &state)?;
    h.public_callback(PublicCallback::Cleanup(state))
}
fn run_wait<H: SpotPublicHost>(
    h: &mut H,
    state: &Rc<Subscription<H::Value>>,
) -> Result<(), H::Error> {
    let connected = h.get(&state.socket, "isConnected")?;
    if h.truthy(&connected) {
        let subscribe = if state.book.is_some() {
            let handlers = h.get(&state.socket, "messageHandlers")?;
            let current = index(h, &handlers, &state.handler)?;
            let length = optional(h, &current, "length")?;
            !h.truthy(&length)
        } else {
            true
        };
        let lhs = h.get(&state.socket, "messageHandlers")?;
        let rhs = h.get(&state.socket, "messageHandlers")?;
        let old = index(h, &rhs, &state.handler)?;
        let old = if h.truthy(&old) {
            old
        } else {
            SpotAccountHost::array(h, &[])?
        };
        let handlers = h.spread_array(&old)?;
        h.append_literal(&handlers, &state.parser)?;
        h.assign_index(&lhs, &state.handler, &handlers)?;
        let lhs = h.get(&state.socket, "topics")?;
        let rhs = h.get(&state.socket, "topics")?;
        let old = index(h, &rhs, &state.topic)?;
        let old = if h.truthy(&old) { old } else { n(h, 0.) };
        let count = h.binary("+", &old, &n(h, 1.))?;
        h.assign_index(&lhs, &state.topic, &count)?;
        if let Some(book) = &state.book {
            if subscribe {
                let packets = method(h, &book.levels, "map", |h| {
                    Ok(vec![h.public_callback(PublicCallback::BookPayload {
                        operation: "subscribe",
                        symbol: state.symbol.clone(),
                    })?])
                })?;
                method(h, &packets, "forEach", |h| {
                    Ok(vec![h.public_callback(PublicCallback::Send {
                        socket: state.socket.clone(),
                    })?])
                })?;
            }
        } else {
            let interval = h.get(&state.options, "interval")?;
            let body = payload(
                h,
                "subscribe",
                "candle",
                &state.symbol,
                Some(&interval),
                None,
            )?;
            packet(h, &state.socket, body)?;
        }
    } else {
        let timer = h.global("setTimeout")?;
        let callback = h.public_callback(PublicCallback::Poll(state.clone()))?;
        let id = h.call(&timer, &h.undefined(), &[callback, n(h, 100.)])?;
        *state.waiting.borrow_mut() = id;
    }
    Ok(())
}
fn cleanup<H: SpotPublicHost>(
    h: &mut H,
    state: &Rc<Subscription<H::Value>>,
) -> Result<(), H::Error> {
    let waiting = state.waiting.borrow().clone();
    if h.truthy(&waiting) {
        let f = h.global("clearTimeout")?;
        let waiting = state.waiting.borrow().clone();
        h.call(&f, &h.undefined(), &[waiting])?;
        *state.waiting.borrow_mut() = h.undefined()
    }
    if let Some(book) = &state.book {
        let inner = h.get(&state.socket, "orderBookInnerState")?;
        h.delete_index(&inner, &book.inner_id)?;
    }
    let lhs = h.get(&state.socket, "messageHandlers")?;
    let rhs = h.get(&state.socket, "messageHandlers")?;
    let old = index(h, &rhs, &state.handler)?;
    let old = if h.truthy(&old) {
        old
    } else {
        SpotAccountHost::array(h, &[])?
    };
    let kept = method(h, &old, "filter", |h| {
        Ok(vec![h.public_callback(PublicCallback::Filter {
            parser: state.parser.clone(),
        })?])
    })?;
    let kept = h.spread_array(&kept)?;
    h.assign_index(&lhs, &state.handler, &kept)?;
    let topics = h.get(&state.socket, "topics")?;
    let old = index(h, &topics, &state.topic)?;
    let count = h.decrement(&old)?;
    h.assign_index(&topics, &state.topic, &count)?;
    let connected = h.get(&state.socket, "isConnected")?;
    if h.truthy(&connected) {
        let handlers = h.get(&state.socket, "messageHandlers")?;
        let handlers = index(h, &handlers, &state.handler)?;
        let length = h.get(&handlers, "length")?;
        if !h.truthy(&length) {
            if let Some(book) = &state.book {
                let packets = method(h, &book.levels, "map", |h| {
                    Ok(vec![h.public_callback(PublicCallback::BookPayload {
                        operation: "unsubscribe",
                        symbol: state.symbol.clone(),
                    })?])
                })?;
                method(h, &packets, "forEach", |h| {
                    Ok(vec![h.public_callback(PublicCallback::Send {
                        socket: state.socket.clone(),
                    })?])
                })?;
            } else {
                method(h, &state.socket, "cleanupMessageHandlerStats", |_| {
                    Ok(vec![state.handler.clone()])
                })?;
            }
        }
    }
    Ok(())
}
fn candle_parser<H: SpotPublicHost>(
    h: &mut H,
    socket: &H::Value,
    opts: &H::Value,
    callback: &H::Value,
    symbol: &H::Value,
    topic: &H::Value,
    data: &H::Value,
) -> Result<(), H::Error> {
    let entries = if SpotAccountHost::is_array(h, data)? {
        data.clone()
    } else {
        SpotAccountHost::array(h, &[data.clone()])?
    };
    method(h, socket, "recordMessageHandlerStats", |_| {
        Ok(vec![topic.clone()])
    })?;
    let mut i = 0.;
    loop {
        let len = h.get(&entries, "length")?;
        let less = h.binary("<", &n(h, i), &len)?;
        if !h.truthy(&less) {
            break;
        }
        let entry = index(h, &entries, &n(h, i))?;
        let s = h.get(&entry, "s")?;
        if SpotAccountHost::strict_equal(h, symbol, &s) {
            let interval = h.get(&entry, "i")?;
            let expected = h.get(opts, "interval")?;
            if SpotAccountHost::strict_equal(h, &interval, &expected) {
                let candle = h.object()?;
                let t = h.get(&entry, "t")?;
                let time = h.binary("/", &t, &n(h, 1000.))?;
                field(h, &candle, "timestamp", &time)?;
                for (k, src) in [
                    ("open", "o"),
                    ("high", "h"),
                    ("low", "l"),
                    ("close", "c"),
                    ("volume", "v"),
                ] {
                    let f = h.global("parseFloat")?;
                    let v = h.get(&entry, src)?;
                    let v = h.call(&f, &h.undefined(), &[v])?;
                    field(h, &candle, k, &v)?
                }
                h.call(callback, &h.undefined(), &[candle])?;
            }
        }
        i += 1.
    }
    Ok(())
}
fn level_value<H: SpotAccountHost>(
    h: &mut H,
    event: &H::Value,
    side: f64,
    item: f64,
) -> Result<H::Value, H::Error> {
    let levels = h.get(event, "levels")?;
    let side = index(h, &levels, &n(h, side))?;
    index(h, &side, &n(h, item))
}
fn math<H: SpotAccountHost>(
    h: &mut H,
    key: &str,
    args: impl FnOnce(&mut H) -> Result<Vec<H::Value>, H::Error>,
) -> Result<H::Value, H::Error> {
    let m = h.global("Math")?;
    method(h, &m, key, args)
}
fn book_parser<H: SpotPublicHost>(
    h: &mut H,
    c: &BookCapture<H::Value>,
    event: &H::Value,
) -> Result<(), H::Error> {
    let first = level_value(h, event, 0., 0.)?;
    let px1 = optional(h, &first, "px")?;
    let second = level_value(h, event, 0., 1.)?;
    let px2 = optional(h, &second, "px")?;
    if !h.truthy(&px1) || !h.truthy(&px2) {
        let parent = h.get(&c.socket, "parent")?;
        method(h, &parent, "log", |h| {
            Ok(vec![template(
                h,
                "Order book for ",
                &[&c.sub_symbol, &lit(h, " is empty")],
            )?])
        })?;
        return Ok(());
    }
    let apparent = free(h, "adjust", |h| {
        let value = math(h, "abs", |h| {
            let left = parse(h, &px1)?;
            let right = parse(h, &px2)?;
            Ok(vec![h.binary("-", &left, &right)?])
        })?;
        Ok(vec![value, c.precision.clone()])
    })?;
    let multiplier = math(h, "floor", |h| {
        Ok(vec![h.binary("/", &apparent, &c.precision)?])
    })?;
    let group = math(h, "round", |h| {
        Ok(vec![math(h, "log10", |_| Ok(vec![multiplier]))?])
    })?;
    let first = index(h, &c.levels, &n(h, 0.))?;
    let low = h.binary("<", &group, &first)?;
    let invalid = if h.truthy(&low) {
        true
    } else {
        let len = h.get(&c.levels, "length")?;
        let last = h.binary("-", &len, &n(h, 1.))?;
        let last = index(h, &c.levels, &last)?;
        let high = h.binary(">", &group, &last)?;
        h.truthy(&high)
    };
    if invalid {
        method(h, &c.socket, "emitAndLogError", |h| {
            let msg = template(
                h,
                "Order book for ",
                &[
                    &c.sub_symbol,
                    &lit(h, " has invalid grouping level "),
                    &group,
                ],
            )?;
            let extra = h.object()?;
            let json = h.global("JSON")?;
            let wire = method(h, &json, "stringify", |h| Ok(vec![h.get(event, "levels")?]))?;
            field(h, &extra, "book", &wire)?;
            Ok(vec![msg, h.undefined(), extra])
        })?;
    }
    let book = index(h, &c.books, &group)?;
    if !h.truthy(&book) {
        let parent = h.get(&c.socket, "parent")?;
        method(h, &parent, "log", |h| {
            Ok(vec![template(
                h,
                "Order book for ",
                &[
                    &c.sub_symbol,
                    &lit(h, " with grouping "),
                    &group,
                    &lit(h, " not found"),
                ],
            )?])
        })?;
        return Ok(());
    }
    for side in ["bids", "asks"] {
        let empty = SpotAccountHost::array(h, &[])?;
        h.set(&book, side, &empty)?
    }
    for (side, key) in [(0., "bids"), (1., "asks")] {
        let levels = h.get(event, "levels")?;
        let rows = index(h, &levels, &n(h, side))?;
        method(h, &rows, "forEach", |h| {
            Ok(vec![h.public_callback(PublicCallback::BookLevel {
                book: book.clone(),
                side: key,
            })?])
        })?;
    }
    let states = h.get(&c.socket, "orderBookInnerState")?;
    let state = index(h, &states, &c.inner_id)?;
    if h.truthy(&state) {
        let states = h.get(&c.socket, "orderBookInnerState")?;
        let state = index(h, &states, &c.inner_id)?;
        h.set(
            &state,
            "isSnapshotLoaded",
            &SpotAccountHost::boolean(h, true),
        )?;
    }
    let asks = h.get(&book, "asks")?;
    let asks = h.get(&asks, "length")?;
    let nonempty = !SpotAccountHost::strict_equal(h, &asks, &n(h, 0.));
    let nonempty = if nonempty {
        true
    } else {
        let bids = h.get(&book, "bids")?;
        let len = h.get(&bids, "length")?;
        !SpotAccountHost::strict_equal(h, &len, &n(h, 0.))
    };
    if nonempty {
        let mut last_bid = h.global("Infinity")?;
        let inf = h.global("Infinity")?;
        let mut last_ask = h.negate(&inf)?;
        let meta = h.object()?;
        for k in ["bids", "asks"] {
            let v = SpotAccountHost::array(h, &[])?;
            field(h, &meta, k, &v)?
        }
        let mut g = 0.;
        loop {
            let len = h.get(&c.levels, "length")?;
            let less = h.binary("<", &n(h, g), &len)?;
            if !h.truthy(&less) {
                break;
            }
            let level = index(h, &c.levels, &n(h, g))?;
            let group_book = index(h, &c.books, &level)?;
            for (side, last) in [("bids", &mut last_bid), ("asks", &mut last_ask)] {
                let values = h.get(&group_book, side)?;
                let mut i = 0.;
                loop {
                    let len = h.get(&values, "length")?;
                    let less = h.binary("<", &n(h, i), &len)?;
                    if !h.truthy(&less) {
                        break;
                    }
                    let entry = index(h, &values, &n(h, i))?;
                    let price = h.get(&entry, "price")?;
                    let ok = h.binary(if side == "bids" { "<" } else { ">" }, &price, last)?;
                    if h.truthy(&ok) {
                        *last = h.get(&entry, "price")?;
                        let target = h.get(&meta, side)?;
                        method(h, &target, "push", |_| Ok(vec![entry]))?;
                    }
                    i += 1.
                }
            }
            g += 1.
        }
        free(h, "calcOrderBookTotal", |_| Ok(vec![meta.clone()]))?;
        h.call(&c.callback, &h.undefined(), &[meta])?;
    }
    Ok(())
}
pub fn run_public_callback<H: SpotPublicHost>(
    h: &mut H,
    capture: &PublicCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let first = args.first().cloned().unwrap_or_else(|| h.undefined());
    match capture {
        PublicCallback::CandleMarket { opts } => {
            let symbol = h.get(&first, "symbol")?;
            let expected = h.get(opts, "symbol")?;
            Ok(SpotAccountHost::boolean(
                h,
                SpotAccountHost::strict_equal(h, &symbol, &expected),
            ))
        }
        PublicCallback::BookMarket { symbol } => {
            let actual = h.get(&first, "symbol")?;
            Ok(SpotAccountHost::boolean(
                h,
                SpotAccountHost::strict_equal(h, &actual, symbol),
            ))
        }
        PublicCallback::CandleParser {
            socket,
            opts,
            callback,
            symbol,
            topic,
        } => {
            candle_parser(h, socket, opts, callback, symbol, topic, &first)?;
            Ok(h.undefined())
        }
        PublicCallback::Wait(state) | PublicCallback::Poll(state) => {
            run_wait(h, state)?;
            Ok(h.undefined())
        }
        PublicCallback::Cleanup(state) => {
            cleanup(h, state)?;
            Ok(h.undefined())
        }
        PublicCallback::Filter { parser } => Ok(SpotAccountHost::boolean(
            h,
            !SpotAccountHost::strict_equal(h, &first, parser),
        )),
        PublicCallback::BookReducer => {
            let grouping = args.get(1).cloned().unwrap_or_else(|| h.undefined());
            let book = h.object()?;
            for k in ["bids", "asks"] {
                let empty = SpotAccountHost::array(h, &[])?;
                field(h, &book, k, &empty)?
            }
            h.assign_index(&first, &grouping, &book)?;
            Ok(first)
        }
        PublicCallback::BookParser(book) => {
            book_parser(h, book, &first)?;
            Ok(h.undefined())
        }
        PublicCallback::BookLevel { book, side } => {
            let parts = h.record_binding(&first, &["px", "sz"])?;
            let target = h.get(book, side)?;
            method(h, &target, "push", |h| {
                let row = h.object()?;
                let price = parse(h, &parts[0])?;
                field(h, &row, "price", &price)?;
                let amount = parse(h, &parts[1])?;
                field(h, &row, "amount", &amount)?;
                field(h, &row, "total", &n(h, 0.))?;
                Ok(vec![row])
            })?;
            Ok(h.undefined())
        }
        PublicCallback::BookPayload { operation, symbol } => {
            payload(h, operation, "l2Book", symbol, None, Some(&first))
        }
        PublicCallback::Send { socket } => {
            packet(h, socket, first)?;
            Ok(h.undefined())
        }
    }
}
pub fn run_public_method<H>(
    h: &mut H,
    socket: &H::Value,
    body: PublicMethod,
    args: &[H::Value],
) -> Result<H::Value, H::Error>
where
    H: SpotPublicHost,
    H::Value: 'static,
    H::Error: Clone + 'static,
{
    let a = args.first().cloned().unwrap_or_else(|| h.undefined());
    let b = args.get(1).cloned().unwrap_or_else(|| h.undefined());
    match body {
        PublicMethod::Connect => {
            crate::ws_transport_leaves::spot_public_connect_and_subscribe(h, socket)?
        }
        PublicMethod::Open => crate::ws_transport_leaves::spot_public_on_open(h, socket)?,
        PublicMethod::SubscribePublic => {
            crate::ws_emission::spot_public_subscribe_public_topics(h, socket)?
        }
        PublicMethod::SubscribeUser => {
            crate::ws_emission::spot_public_subscribe_user_topics(h, socket)?
        }
        PublicMethod::Ping => crate::ws_transport_leaves::spot_public_ping(h, socket)?,
        PublicMethod::Message => crate::spot_ws_dispatch::public_on_message(h, socket, &a)?,
        PublicMethod::Mids => crate::spot_ws_dispatch::handle_all_mids(h, socket, &a)?,
        PublicMethod::Candles => return listen_ohlcv(h, socket, &a, &b),
        PublicMethod::Book => return listen_order_book(h, socket, &a, &b),
    }
    Ok(h.undefined())
}
