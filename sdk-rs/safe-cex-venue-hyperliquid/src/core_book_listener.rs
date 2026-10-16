//! Core listenOrderBook and its retained depth/BBO/poll/cleanup closures.
use crate::{
    candle_listeners::CandleListenerHost, spot_json::JsString, ws_ping_lifecycle::WsPingHost,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Debug)]
pub struct CoreBookState<V> {
    socket: V,
    symbol: V,
    inner_id: V,
    topic: V,
    handler: V,
    bbo_handler: V,
    callback: V,
    parsers: RefCell<Option<(V, V)>>,
    depth_book: RefCell<V>,
    depth_time: RefCell<V>,
    latest_bbo: RefCell<V>,
    waiting_timeout: RefCell<V>,
}
#[derive(Clone, Debug)]
pub enum CoreBookCallback<V> {
    Find { symbol: V },
    CopyLevel,
    Retain { parser: V },
    Parser { state: Rc<CoreBookState<V>> },
    BboParser { state: Rc<CoreBookState<V>> },
    Poll { state: Rc<CoreBookState<V>> },
    Unsubscribe { state: Rc<CoreBookState<V>> },
}
pub trait CoreBookHost: CandleListenerHost {
    fn imported(&mut self, name: &str) -> Result<Self::Value, Self::Error>;
    /// Native object spread (own enumerable string AND symbol properties).
    fn spread_object(&mut self, source: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Strict native delete of a computed reference.
    fn delete_index(&mut self, object: &Self::Value, key: &Self::Value) -> Result<(), Self::Error>;
    fn construct(
        &mut self,
        constructor: &Self::Value,
        args: &[Self::Value],
    ) -> Result<Self::Value, Self::Error>;
    fn throw_value(&self, value: Self::Value) -> Self::Error;
    /// Native arrows dispatch run_core_book_callback, retaining these handles.
    /// Parser and BboParser infer names parser/bboParser, arity 1; Find,
    /// CopyLevel and Retain are anonymous, arity 1; Poll/Unsubscribe are
    /// anonymous, arity 0. Lexical this and nonconstructibility are bindings.
    /// Captured handles can form cycles; cyclic reclamation belongs to embedding.
    fn book_callback(
        &mut self,
        capture: CoreBookCallback<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
}
fn lit<H: WsPingHost>(h: &H, s: &str) -> H::Value {
    h.literal(&JsString(s.encode_utf16().collect()))
}
fn optional<H: CoreBookHost>(
    h: &mut H,
    value: &H::Value,
    name: &str,
) -> Result<H::Value, H::Error> {
    if h.nullish(value) {
        Ok(h.undefined())
    } else {
        h.get(value, name)
    }
}
fn topic<H: CoreBookHost>(
    h: &mut H,
    prefix: &str,
    symbol: &H::Value,
) -> Result<H::Value, H::Error> {
    let mut units = prefix.encode_utf16().collect::<Vec<_>>();
    units.extend(h.template_string(symbol)?.0);
    Ok(h.literal(&JsString(units)))
}
fn message<H: CoreBookHost>(
    h: &mut H,
    symbol: &H::Value,
    suffix: &str,
) -> Result<H::Value, H::Error> {
    let mut units = "Market with symbol ".encode_utf16().collect::<Vec<_>>();
    units.extend(h.template_string(symbol)?.0);
    units.extend(suffix.encode_utf16());
    Ok(h.literal(&JsString(units)))
}
fn table_get<H: CoreBookHost>(
    h: &mut H,
    s: &H::Value,
    table: &str,
    key: &H::Value,
) -> Result<H::Value, H::Error> {
    let table = h.get(s, table)?;
    h.index(&table, key)
}
fn or_array<H: CoreBookHost>(h: &mut H, v: H::Value) -> Result<H::Value, H::Error> {
    if h.truthy(&v) { Ok(v) } else { h.array(&[]) }
}
fn make_book<H: CoreBookHost>(h: &mut H) -> Result<H::Value, H::Error> {
    let book = h.object()?;
    let bids = h.array(&[])?;
    h.define_data(&book, "bids", &bids)?;
    let asks = h.array(&[])?;
    h.define_data(&book, "asks", &asks)?;
    Ok(book)
}
fn copy_level<H: CoreBookHost>(h: &mut H, level: &H::Value) -> Result<H::Value, H::Error> {
    let v = h.spread_object(level)?;
    h.define_data(&v, "total", &h.number(0.0))?;
    Ok(v)
}
fn number_field<H: CoreBookHost>(
    h: &mut H,
    object: &H::Value,
    name: &str,
) -> Result<H::Value, H::Error> {
    let f = h.global("Number")?;
    let value = h.get(object, name)?;
    h.call(&f, &h.undefined(), &[value])
}
fn finite<H: CoreBookHost>(h: &mut H, value: &H::Value) -> Result<bool, H::Error> {
    let number = h.global("Number")?;
    let f = h.get(&number, "isFinite")?;
    let result = h.call(&f, &number, std::slice::from_ref(value))?;
    Ok(h.truthy(&result))
}
fn overlay<H: CoreBookHost>(
    h: &mut H,
    levels: H::Value,
    best: H::Value,
    bid: bool,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&best) {
        return h.array(&[]);
    }
    let price = number_field(h, &best, "px")?;
    let amount = number_field(h, &best, "sz")?;
    if !finite(h, &price)? || !finite(h, &amount)? {
        return Ok(levels);
    }
    let top = h.object()?;
    h.define_data(&top, "price", &price)?;
    h.define_data(&top, "amount", &amount)?;
    h.define_data(&top, "total", &h.number(0.0))?;
    let combined = h.array(&[top])?;
    let mut i = 0.0;
    loop {
        let len = h.get(&levels, "length")?;
        let inside = h.binary("<", &h.number(i), &len)?;
        if !h.truthy(&inside) {
            break;
        }
        let len = h.get(&combined, "length")?;
        let inside = h.binary("<", &len, &h.number(20.0))?;
        if !h.truthy(&inside) {
            break;
        }
        let level = h.index(&levels, &h.number(i))?;
        let p = h.get(&level, "price")?;
        let keep = h.binary(if bid { "<" } else { ">" }, &p, &price)?;
        if h.truthy(&keep) {
            let push = h.get(&combined, "push")?;
            let level = copy_level(h, &level)?;
            h.call(&push, &combined, &[level])?;
        }
        i += 1.0;
    }
    Ok(combined)
}
fn nonempty<H: CoreBookHost>(h: &mut H, book: &H::Value) -> Result<bool, H::Error> {
    let asks = h.get(book, "asks")?;
    let len = h.get(&asks, "length")?;
    if !h.strict_equal(&len, &h.number(0.0)) {
        return Ok(true);
    }
    let bids = h.get(book, "bids")?;
    let len = h.get(&bids, "length")?;
    Ok(!h.strict_equal(&len, &h.number(0.0)))
}
fn emit<H: CoreBookHost>(h: &mut H, state: &Rc<CoreBookState<H::Value>>) -> Result<(), H::Error> {
    let depth = state.depth_book.borrow().clone();
    if !h.truthy(&depth) {
        return Ok(());
    }
    let inner = table_get(h, &state.socket, "orderBookInnerState", &state.inner_id)?;
    let loaded = optional(h, &inner, "isSnapshotLoaded")?;
    if !h.truthy(&loaded) {
        return Ok(());
    }
    let book = h.object()?;
    for side in ["bids", "asks"] {
        let depth = state.depth_book.borrow().clone();
        let levels = h.get(&depth, side)?;
        let map = h.get(&levels, "map")?;
        let callback = h.book_callback(CoreBookCallback::CopyLevel)?;
        let copied = h.call(&map, &levels, &[callback])?;
        h.define_data(&book, side, &copied)?;
    }
    let latest = state.latest_bbo.borrow().clone();
    if h.truthy(&latest) {
        let latest = state.latest_bbo.borrow().clone();
        let time = h.get(&latest, "time")?;
        let depth_time = state.depth_time.borrow().clone();
        let fresh = h.binary(">=", &time, &depth_time)?;
        if h.truthy(&fresh) {
            for (side, index, bid) in [("bids", 0.0, true), ("asks", 1.0, false)] {
                let levels = h.get(&book, side)?;
                let latest = state.latest_bbo.borrow().clone();
                let bbo = h.get(&latest, "bbo")?;
                let best = h.index(&bbo, &h.number(index))?;
                let combined = overlay(h, levels, best, bid)?;
                h.set(&book, side, &combined)?;
            }
        }
    }
    if nonempty(h, &book)? {
        let total = h.imported("calcOrderBookTotal")?;
        h.call(&total, &h.undefined(), std::slice::from_ref(&book))?;
        h.call(&state.callback, &h.undefined(), &[book])?;
    }
    Ok(())
}
fn first_price<H: CoreBookHost>(
    h: &mut H,
    event: &H::Value,
    index: f64,
) -> Result<H::Value, H::Error> {
    let levels = h.get(event, "levels")?;
    let bids = h.index(&levels, &h.number(0.0))?;
    let level = h.index(&bids, &h.number(index))?;
    optional(h, &level, "px")
}
fn parse<H: CoreBookHost>(
    h: &mut H,
    state: &Rc<CoreBookState<H::Value>>,
    event: &H::Value,
) -> Result<(), H::Error> {
    let first = first_price(h, event, 0.0)?;
    let second = first_price(h, event, 1.0)?;
    if !h.truthy(&first) || !h.truthy(&second) {
        let parent = h.get(&state.socket, "parent")?;
        let log = h.get(&parent, "log")?;
        let mut text = "Order book for ".encode_utf16().collect::<Vec<_>>();
        text.extend(h.template_string(&state.symbol)?.0);
        text.extend(" is empty".encode_utf16());
        let msg = h.literal(&JsString(text));
        h.call(&log, &parent, &[msg])?;
        return Ok(());
    }
    let book = make_book(h)?;
    for (side, index) in [("bids", 0.0), ("asks", 1.0)] {
        let levels = h.get(event, "levels")?;
        let levels = h.index(&levels, &h.number(index))?;
        let mut i = 0.0;
        loop {
            let len = h.get(&levels, "length")?;
            let inside = h.binary("<", &h.number(i), &len)?;
            if !h.truthy(&inside) {
                break;
            }
            let level = h.index(&levels, &h.number(i))?;
            let target = h.get(&book, side)?;
            let push = h.get(&target, "push")?;
            let result = h.object()?;
            let f = h.global("parseFloat")?;
            let raw = h.get(&level, "px")?;
            let price = h.call(&f, &h.undefined(), &[raw])?;
            h.define_data(&result, "price", &price)?;
            let f = h.global("parseFloat")?;
            let raw = h.get(&level, "sz")?;
            let amount = h.call(&f, &h.undefined(), &[raw])?;
            h.define_data(&result, "amount", &amount)?;
            h.define_data(&result, "total", &h.number(0.0))?;
            h.call(&push, &target, &[result])?;
            i += 1.0;
        }
    }
    let inner = table_get(h, &state.socket, "orderBookInnerState", &state.inner_id)?;
    if h.truthy(&inner) {
        let inner = table_get(h, &state.socket, "orderBookInnerState", &state.inner_id)?;
        h.set(&inner, "isSnapshotLoaded", &h.boolean(true))?;
    }
    *state.depth_book.borrow_mut() = book;
    let time = number_field(h, event, "time")?;
    let time = if h.truthy(&time) { time } else { h.number(0.0) };
    *state.depth_time.borrow_mut() = time;
    emit(h, state)
}
fn bbo<H: CoreBookHost>(
    h: &mut H,
    state: &Rc<CoreBookState<H::Value>>,
    event: &H::Value,
) -> Result<(), H::Error> {
    let array = h.global("Array")?;
    let is_array = h.get(&array, "isArray")?;
    let raw = h.get(event, "bbo")?;
    let valid = h.call(&is_array, &array, &[raw])?;
    if !h.truthy(&valid) {
        return Ok(());
    }
    let number = h.global("Number")?;
    let is_finite = h.get(&number, "isFinite")?;
    let time = number_field(h, event, "time")?;
    let valid = h.call(&is_finite, &number, &[time])?;
    if !h.truthy(&valid) {
        return Ok(());
    }
    let latest = state.latest_bbo.borrow().clone();
    if h.truthy(&latest) {
        let time = h.get(event, "time")?;
        let latest = state.latest_bbo.borrow().clone();
        let old = h.get(&latest, "time")?;
        let stale = h.binary("<", &time, &old)?;
        if h.truthy(&stale) {
            return Ok(());
        }
    }
    *state.latest_bbo.borrow_mut() = event.clone();
    emit(h, state)
}
fn append<H: CoreBookHost>(
    h: &mut H,
    state: &CoreBookState<H::Value>,
    key: &H::Value,
    parser: &H::Value,
) -> Result<(), H::Error> {
    let target = h.get(&state.socket, "messageHandlers")?;
    let old = table_get(h, &state.socket, "messageHandlers", key)?;
    let old = or_array(h, old)?;
    let value = h.spread_append(&old, parser)?;
    h.assign_index(&target, key, &value)
}
fn send<H: CoreBookHost>(
    h: &mut H,
    state: &CoreBookState<H::Value>,
    method_name: &str,
    kind: &str,
) -> Result<(), H::Error> {
    let send = h.get(&state.socket, method_name)?;
    let payload = h.object()?;
    h.define_data(&payload, "type", &lit(h, kind))?;
    h.define_data(&payload, "coin", &state.symbol)?;
    h.call(&send, &state.socket, &[payload])?;
    Ok(())
}
fn wait<H: CoreBookHost>(h: &mut H, state: &Rc<CoreBookState<H::Value>>) -> Result<(), H::Error> {
    let connected = h.get(&state.socket, "isConnected")?;
    if h.truthy(&connected) {
        let old = table_get(h, &state.socket, "messageHandlers", &state.handler)?;
        let len = optional(h, &old, "length")?;
        let should_subscribe = !h.truthy(&len);
        let (parser, bbo) = state
            .parsers
            .borrow()
            .clone()
            .expect("parsers initialized before wait");
        append(h, state, &state.handler, &parser)?;
        append(h, state, &state.bbo_handler, &bbo)?;
        let target = h.get(&state.socket, "topics")?;
        let old = table_get(h, &state.socket, "topics", &state.topic)?;
        let old = if h.truthy(&old) { old } else { h.number(0.0) };
        let count = h.binary("+", &old, &h.number(1.0))?;
        h.assign_index(&target, &state.topic, &count)?;
        if should_subscribe {
            send(h, state, "sendSubscription", "l2Book")?;
            send(h, state, "sendSubscription", "bbo")?;
        }
    } else {
        let timer = h.global("setTimeout")?;
        let callback = h.book_callback(CoreBookCallback::Poll {
            state: state.clone(),
        })?;
        let id = h.call(&timer, &h.undefined(), &[callback, h.number(100.0)])?;
        *state.waiting_timeout.borrow_mut() = id;
    }
    Ok(())
}
fn remove<H: CoreBookHost>(
    h: &mut H,
    state: &CoreBookState<H::Value>,
    key: &H::Value,
    parser: &H::Value,
) -> Result<(), H::Error> {
    let target = h.get(&state.socket, "messageHandlers")?;
    let old = table_get(h, &state.socket, "messageHandlers", key)?;
    let old = or_array(h, old)?;
    let filter = h.get(&old, "filter")?;
    let predicate = h.book_callback(CoreBookCallback::Retain {
        parser: parser.clone(),
    })?;
    let kept = h.call(&filter, &old, &[predicate])?;
    let value = h.spread_array(&kept)?;
    h.assign_index(&target, key, &value)
}
fn unsubscribe<H: CoreBookHost>(
    h: &mut H,
    state: &Rc<CoreBookState<H::Value>>,
) -> Result<(), H::Error> {
    let timer = state.waiting_timeout.borrow().clone();
    if h.truthy(&timer) {
        let clear = h.global("clearTimeout")?;
        let timer = state.waiting_timeout.borrow().clone();
        h.call(&clear, &h.undefined(), &[timer])?;
        *state.waiting_timeout.borrow_mut() = h.undefined();
    }
    let inner = h.get(&state.socket, "orderBookInnerState")?;
    h.delete_index(&inner, &state.inner_id)?;
    let (parser, bbo) = state
        .parsers
        .borrow()
        .clone()
        .expect("parsers initialized before cleanup");
    remove(h, state, &state.handler, &parser)?;
    remove(h, state, &state.bbo_handler, &bbo)?;
    let target = h.get(&state.socket, "topics")?;
    let old = h.index(&target, &state.topic)?;
    let count = h.decrement(&old)?;
    h.assign_index(&target, &state.topic, &count)?;
    let connected = h.get(&state.socket, "isConnected")?;
    if h.truthy(&connected) {
        let handlers = table_get(h, &state.socket, "messageHandlers", &state.handler)?;
        let len = h.get(&handlers, "length")?;
        if !h.truthy(&len) {
            send(h, state, "sendUnsubscription", "l2Book")?;
            send(h, state, "sendUnsubscription", "bbo")?;
        }
    }
    Ok(())
}
pub fn core_listen_order_book<H: CoreBookHost>(
    h: &mut H,
    socket: H::Value,
    symbol: H::Value,
    callback: H::Value,
) -> Result<H::Value, H::Error> {
    let parent = h.get(&socket, "parent")?;
    let store = h.get(&parent, "store")?;
    let markets = h.get(&store, "markets")?;
    let find = h.get(&markets, "find")?;
    let predicate = h.book_callback(CoreBookCallback::Find {
        symbol: symbol.clone(),
    })?;
    let market = h.call(&find, &markets, &[predicate])?;
    if !h.truthy(&market) {
        let parent = h.get(&socket, "parent")?;
        let log = h.get(&parent, "emitAndLogError")?;
        let mut text = "Market with symbol ".encode_utf16().collect::<Vec<_>>();
        text.extend(h.template_string(&symbol)?.0);
        text.extend(" not found. Available markets: ".encode_utf16());
        let p = h.get(&socket, "parent")?;
        let store = h.get(&p, "store")?;
        let markets = h.get(&store, "markets")?;
        let len = h.get(&markets, "length")?;
        text.extend(h.template_string(&len)?.0);
        let msg = h.literal(&JsString(text));
        h.call(&log, &parent, &[msg, lit(h, "listenOrderBook")])?;
        let error = h.global("Error")?;
        let msg = message(h, &symbol, " not found")?;
        let value = h.construct(&error, &[msg])?;
        return Err(h.throw_value(value));
    }
    let sub_symbol = h.get(&market, "id")?;
    let precision = h.get(&market, "precision")?;
    let price = h.get(&precision, "price")?;
    if !h.truthy(&price) {
        let parent = h.get(&socket, "parent")?;
        let log = h.get(&parent, "emitAndLogError")?;
        let msg = message(h, &symbol, " has no price precision")?;
        h.call(&log, &parent, &[msg, lit(h, "listenOrderBook")])?;
        let error = h.global("Error")?;
        let msg = message(h, &symbol, " has no price precision")?;
        let value = h.construct(&error, &[msg])?;
        return Err(h.throw_value(value));
    }
    let topic = topic(h, "l2Book.", &sub_symbol)?;
    let handler = self::topic(h, "l2Book.", &sub_symbol)?;
    let bbo_handler = self::topic(h, "bbo.", &sub_symbol)?;
    let nanoid = h.imported("nanoid")?;
    let inner_id = h.call(&nanoid, &h.undefined(), &[])?;
    let inner = table_get(h, &socket, "orderBookInnerState", &inner_id)?;
    if !h.truthy(&inner) {
        let target = h.get(&socket, "orderBookInnerState")?;
        let inner = h.object()?;
        let updates = h.array(&[])?;
        h.define_data(&inner, "updates", &updates)?;
        h.define_data(&inner, "isSnapshotLoaded", &h.boolean(false))?;
        h.assign_index(&target, &inner_id, &inner)?;
    }
    let state = Rc::new(CoreBookState {
        socket,
        symbol: sub_symbol,
        inner_id,
        topic,
        handler,
        bbo_handler,
        callback,
        parsers: RefCell::new(None),
        depth_book: RefCell::new(h.undefined()),
        depth_time: RefCell::new(h.number(0.0)),
        latest_bbo: RefCell::new(h.undefined()),
        waiting_timeout: RefCell::new(h.undefined()),
    });
    let parser = h.book_callback(CoreBookCallback::Parser {
        state: state.clone(),
    })?;
    let bbo = h.book_callback(CoreBookCallback::BboParser {
        state: state.clone(),
    })?;
    *state.parsers.borrow_mut() = Some((parser, bbo));
    wait(h, &state)?;
    h.book_callback(CoreBookCallback::Unsubscribe { state })
}
pub fn run_core_book_callback<H: CoreBookHost>(
    h: &mut H,
    capture: &CoreBookCallback<H::Value>,
    args: &[H::Value],
) -> Result<H::Value, H::Error> {
    let value = args.first().cloned().unwrap_or_else(|| h.undefined());
    match capture {
        CoreBookCallback::Find { symbol } => {
            let actual = h.get(&value, "symbol")?;
            return Ok(h.boolean(h.strict_equal(&actual, symbol)));
        }
        CoreBookCallback::CopyLevel => return copy_level(h, &value),
        CoreBookCallback::Retain { parser } => {
            return Ok(h.boolean(!h.strict_equal(&value, parser)));
        }
        CoreBookCallback::Parser { state } => parse(h, state, &value)?,
        CoreBookCallback::BboParser { state } => bbo(h, state, &value)?,
        CoreBookCallback::Poll { state } => wait(h, state)?,
        CoreBookCallback::Unsubscribe { state } => unsubscribe(h, state)?,
    }
    Ok(h.undefined())
}
