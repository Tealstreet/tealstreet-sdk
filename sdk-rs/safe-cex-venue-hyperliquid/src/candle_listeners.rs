//! Core/spot listenOHLCV bodies and their live closure environments.
use crate::{spot_json::JsString, ws_ping_lifecycle::WsPingHost};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandleVenue {
    Core,
    Spot,
}
#[derive(Debug)]
pub struct CandleWait<V> {
    socket: V,
    opts: V,
    symbol: V,
    topic: V,
    handler: V,
    parser: V,
    waiting_timeout: RefCell<V>,
    venue: CandleVenue,
}
#[derive(Clone, Debug)]
pub enum CandleCallback<V> {
    Find {
        opts: V,
    },
    Parser {
        socket: V,
        opts: V,
        callback: V,
        symbol: V,
        topic: V,
    },
    Retain {
        parser: V,
    },
    Poll {
        state: Rc<CandleWait<V>>,
    },
    Unsubscribe {
        state: Rc<CandleWait<V>>,
    },
    Noop,
}
pub trait CandleListenerHost: WsPingHost {
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn strict_equal(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn binary(
        &mut self,
        operator: &str,
        left: &Self::Value,
        right: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    /// Native ToNumeric/decrement, including BigInt, with the old value discarded.
    fn decrement(&mut self, value: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn template_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    fn index(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn assign_index(
        &mut self,
        object: &Self::Value,
        key: &Self::Value,
        value: &Self::Value,
    ) -> Result<(), Self::Error>;
    fn array(&mut self, values: &[Self::Value]) -> Result<Self::Value, Self::Error>;
    /// Native array literal spread/IteratorClose, followed by one final element.
    fn spread_append(
        &mut self,
        iterable: &Self::Value,
        last: &Self::Value,
    ) -> Result<Self::Value, Self::Error>;
    fn spread_array(&mut self, iterable: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Source arrows (inferred parser name, otherwise anonymous; native arities);
    /// body execution dispatches run_candle_callback.
    fn candle_callback(
        &mut self,
        capture: CandleCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn method<H: WsPingHost>(
    h: &mut H,
    receiver: &H::Value,
    key: &str,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let f = h.get(receiver, key)?;
    h.call(&f, receiver, args)
}
fn make_topic<H: CandleListenerHost>(
    h: &mut H,
    symbol: &H::Value,
    opts: &H::Value,
) -> Result<H::Value, H::Error> {
    let mut units = "candle.".encode_utf16().collect::<Vec<_>>();
    units.extend(h.template_string(symbol)?.0);
    units.push(46);
    let interval = h.get(opts, "interval")?;
    units.extend(h.template_string(&interval)?.0);
    Ok(h.literal(&JsString(units)))
}
fn or_array<H: CandleListenerHost>(h: &mut H, value: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&value) {
        Ok(value)
    } else {
        h.array(&[])
    }
}
fn subscription<H: CandleListenerHost>(
    h: &mut H,
    state: &CandleWait<H::Value>,
) -> Result<H::Value, H::Error> {
    let sub = h.object()?;
    h.define_data(&sub, "type", &lit(h, "candle"))?;
    h.define_data(&sub, "coin", &state.symbol)?;
    let interval = h.get(&state.opts, "interval")?;
    h.define_data(&sub, "interval", &interval)?;
    Ok(sub)
}
fn wait<H: CandleListenerHost>(
    h: &mut H,
    state: &Rc<CandleWait<H::Value>>,
) -> Result<(), H::Error> {
    let connected = h.get(&state.socket, "isConnected")?;
    if h.truthy(&connected) {
        let target = h.get(&state.socket, "messageHandlers")?;
        let source = h.get(&state.socket, "messageHandlers")?;
        let old = h.index(&source, &state.handler)?;
        let old = or_array(h, old)?;
        let parsers = h.spread_append(&old, &state.parser)?;
        h.assign_index(&target, &state.handler, &parsers)?;
        let target = h.get(&state.socket, "topics")?;
        let source = h.get(&state.socket, "topics")?;
        let old = h.index(&source, &state.topic)?;
        let old = if h.truthy(&old) { old } else { h.number(0.0) };
        let count = h.binary("+", &old, &h.number(1.0))?;
        h.assign_index(&target, &state.topic, &count)?;
        match state.venue {
            CandleVenue::Core => {
                let send = h.get(&state.socket, "sendSubscription")?;
                let sub = subscription(h, state)?;
                h.call(&send, &state.socket, &[sub])?;
            }
            CandleVenue::Spot => {
                let payload = h.object()?;
                h.define_data(&payload, "method", &lit(h, "subscribe"))?;
                let sub = subscription(h, state)?;
                h.define_data(&payload, "subscription", &sub)?;
                let send = h.get(&state.socket, "send")?;
                let json = h.global("JSON")?;
                let stringify = h.get(&json, "stringify")?;
                let wire = h.call(&stringify, &json, &[payload])?;
                h.call(&send, &state.socket, &[wire])?;
            }
        }
    } else {
        let timer = h.global("setTimeout")?;
        let callback = h.candle_callback(CandleCallback::Poll {
            state: state.clone(),
        })?;
        let id = h.call(&timer, &h.undefined(), &[callback, h.number(100.0)])?;
        // Release every borrow before a native call; timer registration may reenter.
        *state.waiting_timeout.borrow_mut() = id;
    }
    Ok(())
}
fn listen<H: CandleListenerHost>(
    h: &mut H,
    socket: H::Value,
    opts: H::Value,
    callback: H::Value,
    venue: CandleVenue,
) -> Result<H::Value, H::Error> {
    let parent = h.get(&socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let markets = h.get(&store, "markets")?;
    let find = h.get(&markets, "find")?;
    let predicate = h.candle_callback(CandleCallback::Find { opts: opts.clone() })?;
    let market = h.call(&find, &markets, &[predicate])?;
    if venue == CandleVenue::Core && !h.truthy(&market) {
        let console = h.global("console")?;
        let error = h.get(&console, "error")?;
        let mut text = "[Hyperliquid] listenOHLCV: Market not found for symbol \""
            .encode_utf16()
            .collect::<Vec<_>>();
        let symbol = h.get(&opts, "symbol")?;
        text.extend(h.template_string(&symbol)?.0);
        text.extend("\", skipping subscription".encode_utf16());
        h.call(&error, &console, &[h.literal(&JsString(text))])?;
        return h.candle_callback(CandleCallback::Noop);
    }
    let symbol = if venue == CandleVenue::Spot && h.nullish(&market) {
        h.undefined()
    } else {
        h.get(&market, "id")?
    };
    let symbol = if venue == CandleVenue::Spot && !h.truthy(&symbol) {
        lit(h, "")
    } else {
        symbol
    };
    let topic = make_topic(h, &symbol, &opts)?;
    let handler = make_topic(h, &symbol, &opts)?;
    let parser = h.candle_callback(CandleCallback::Parser {
        socket: socket.clone(),
        opts: opts.clone(),
        callback,
        symbol: symbol.clone(),
        topic: topic.clone(),
    })?;
    let state = Rc::new(CandleWait {
        socket,
        opts,
        symbol,
        topic,
        handler,
        parser,
        waiting_timeout: RefCell::new(h.undefined()),
        venue,
    });
    wait(h, &state)?;
    h.candle_callback(CandleCallback::Unsubscribe { state })
}
pub fn core_listen_ohlcv<H: CandleListenerHost>(
    h: &mut H,
    socket: H::Value,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    listen(h, socket, opts, callback, CandleVenue::Core)
}
pub fn spot_listen_ohlcv<H: CandleListenerHost>(
    h: &mut H,
    socket: H::Value,
    opts: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    listen(h, socket, opts, callback, CandleVenue::Spot)
}

pub fn run_candle_callback<H: CandleListenerHost>(
    h: &mut H,
    capture: &CandleCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    match capture {
        CandleCallback::Noop => {}
        CandleCallback::Find { opts } => {
            let market = args.first().cloned().unwrap_or_else(|| h.undefined());
            let symbol = h.get(&market, "symbol")?;
            let desired = h.get(opts, "symbol")?;
            return Ok(h.boolean(h.strict_equal(&symbol, &desired)));
        }
        CandleCallback::Retain { parser } => {
            let value = args.first().cloned().unwrap_or_else(|| h.undefined());
            return Ok(h.boolean(!h.strict_equal(&value, parser)));
        }
        CandleCallback::Poll { state } => wait(h, state)?,
        CandleCallback::Unsubscribe { state } => {
            let timeout = state.waiting_timeout.borrow().clone();
            if h.truthy(&timeout) {
                let clear = h.global("clearTimeout")?;
                let timeout = state.waiting_timeout.borrow().clone();
                h.call(&clear, &h.undefined(), &[timeout])?;
                *state.waiting_timeout.borrow_mut() = h.undefined();
            }
            let target = h.get(&state.socket, "messageHandlers")?;
            let source = h.get(&state.socket, "messageHandlers")?;
            let old = h.index(&source, &state.handler)?;
            let old = or_array(h, old)?;
            let filter = h.get(&old, "filter")?;
            let predicate = h.candle_callback(CandleCallback::Retain {
                parser: state.parser.clone(),
            })?;
            let filtered = h.call(&filter, &old, &[predicate])?;
            let filtered = h.spread_array(&filtered)?;
            h.assign_index(&target, &state.handler, &filtered)?;
            let topics = h.get(&state.socket, "topics")?;
            let count = h.index(&topics, &state.topic)?;
            let count = h.decrement(&count)?;
            h.assign_index(&topics, &state.topic, &count)?;
            let connected = h.get(&state.socket, "isConnected")?;
            if h.truthy(&connected) {
                let handlers = h.get(&state.socket, "messageHandlers")?;
                let handlers = h.index(&handlers, &state.handler)?;
                let length = h.get(&handlers, "length")?;
                if !h.truthy(&length) {
                    method(
                        h,
                        &state.socket,
                        "cleanupMessageHandlerStats",
                        std::slice::from_ref(&state.handler),
                    )?;
                }
            }
        }
        CandleCallback::Parser {
            socket,
            opts,
            callback,
            symbol,
            topic,
        } => {
            let data = args.first().cloned().unwrap_or_else(|| h.undefined());
            let array = h.global("Array")?;
            let is_array = h.get(&array, "isArray")?;
            let is_array = h.call(&is_array, &array, std::slice::from_ref(&data))?;
            let entries = if h.truthy(&is_array) {
                data
            } else {
                h.array(&[data])?
            };
            method(
                h,
                socket,
                "recordMessageHandlerStats",
                std::slice::from_ref(topic),
            )?;
            let mut index = 0.0;
            loop {
                let length = h.get(&entries, "length")?;
                let more = h.binary("<", &h.number(index), &length)?;
                if !h.truthy(&more) {
                    break;
                }
                let entry = h.index(&entries, &h.number(index))?;
                let actual_symbol = h.get(&entry, "s")?;
                if h.strict_equal(symbol, &actual_symbol) {
                    let actual_interval = h.get(&entry, "i")?;
                    let interval = h.get(opts, "interval")?;
                    if h.strict_equal(&actual_interval, &interval) {
                        let candle = h.object()?;
                        let timestamp = h.get(&entry, "t")?;
                        let timestamp = h.binary("/", &timestamp, &h.number(1000.0))?;
                        h.define_data(&candle, "timestamp", &timestamp)?;
                        for (key, source) in [
                            ("open", "o"),
                            ("high", "h"),
                            ("low", "l"),
                            ("close", "c"),
                            ("volume", "v"),
                        ] {
                            let parse = h.global("parseFloat")?;
                            let raw = h.get(&entry, source)?;
                            let value = h.call(&parse, &h.undefined(), &[raw])?;
                            h.define_data(&candle, key, &value)?;
                        }
                        h.call(callback, &h.undefined(), &[candle])?;
                    }
                }
                index += 1.0;
            }
        }
    }
    Ok(h.undefined())
}
