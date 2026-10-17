//! Public websocket construction, dispatch, subscriptions and book recovery.
use crate::types::BASE_WS_URL;

#[derive(Clone)]
pub enum WeexPublicCallback<V> {
    Candle {
        callback: V,
    },
    Book {
        symbol: V,
        callback: V,
    },
    CandleUnsubscribe {
        key: V,
        topic: V,
        handler: V,
    },
    BookUnsubscribe {
        symbol: V,
        key: V,
        topic: V,
        handler: V,
    },
    BookSnapshot {
        symbol: V,
        topic: V,
        callback: V,
    },
    GapSnapshot {
        symbol: V,
        json: V,
    },
    GapSnapshotFailure,
    SnapshotFailure {
        symbol: V,
    },
}

/// Host-owned observers retain their instance and callback values until settlement.
pub struct WeexPublicFeedRuntime<H, V, E> {
    pub base_construct: fn(&H, V) -> Result<V, E>,
    pub define: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub bind_field: fn(&H, &V, &'static str) -> Result<V, E>,
    pub new_set: fn(&H) -> Result<V, E>,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub array: fn(Vec<V>) -> V,
    pub number: fn(f64) -> V,
    pub boolean: fn(bool) -> V,
    pub text: fn(&str) -> V,
    pub string_units: fn(Vec<u16>) -> V,
    pub undefined: fn() -> V,
    pub this_property: fn(&H, &'static str) -> Result<V, E>,
    pub property: fn(&H, &V, &str) -> Result<V, E>,
    pub optional_property: fn(&H, &V, &str) -> Result<V, E>,
    pub lookup: fn(&H, &V, &V) -> Result<V, E>,
    pub assign: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub remove: fn(&H, &V, &V) -> Result<(), E>,
    pub truthy: fn(&V) -> bool,
    pub is_undefined: fn(&V) -> bool,
    pub equals_text: fn(&V, &str) -> bool,
    pub is_false: fn(&V) -> bool,
    pub is_array: fn(&H, &V) -> Result<bool, E>,
    pub string_constructor: fn(&H, &V) -> Result<Vec<u16>, E>,
    pub template_string: fn(&H, &V) -> Result<Vec<u16>, E>,
    pub to_number: fn(&H, V, V) -> Result<V, E>,
    pub number_value: fn(&V) -> f64,
    pub abs: fn(&H, &V) -> Result<f64, E>,
    pub multiply: fn(&H, &V, f64) -> Result<V, E>,
    pub to_integer_string: fn(&H, V) -> Result<V, E>,
    pub method: fn(&H, &V, &'static str) -> Result<V, E>,
    pub call_event: fn(&H, V, V, &'static str, V) -> Result<(), E>,
    pub disposed: fn(&H) -> Result<bool, E>,
    pub create_socket: fn(&H, V, V, V) -> Result<V, E>,
    pub set_socket: fn(&H, V) -> Result<(), E>,
    pub add_listener: fn(&H, V, V, &'static str, V) -> Result<(), E>,
    pub track_received: fn(&H) -> Result<(), E>,
    pub parse: fn(&H, Vec<u16>) -> Result<V, E>,
    pub subscription_id: fn(&H) -> Result<f64, E>,
    pub set_subscription_id: fn(&H, f64) -> Result<(), E>,
    pub send_pong: fn(&H, f64) -> Result<(), E>,
    pub handle_pong: fn(&H, f64) -> Result<(), E>,
    pub warning_sink: fn(&H) -> Result<V, E>,
    pub warning: fn(&H, V, Vec<u16>) -> Result<(), E>,
    pub record_stats: fn(&H, V) -> Result<(), E>,
    pub route_method: fn(&H) -> Result<V, E>,
    pub route: fn(&H, V, V, &V) -> Result<(), E>,
    pub interval: fn(&H, V) -> Result<V, E>,
    pub supported_interval: fn(&V) -> bool,
    pub make_callback: fn(&H, WeexPublicCallback<V>) -> Result<V, E>,
    pub add_handler: fn(&H, &V, &V) -> Result<(), E>,
    pub remove_handler: fn(&H, &V, &V) -> Result<(), E>,
    pub subscribe: fn(&H, &V) -> Result<(), E>,
    pub unsubscribe: fn(&H, &V) -> Result<(), E>,
    pub snapshot: fn(&H, &V, f64) -> Result<V, E>,
    /// Observe rejection and success-callback throws, like `.then(success).catch(failure)`.
    pub observe_snapshot: fn(&H, V, WeexPublicCallback<V>, WeexPublicCallback<V>) -> Result<(), E>,
    pub books: fn(&H) -> Result<V, E>,
    pub put_book: fn(&H, &V, &V, V) -> Result<(), E>,
    pub invoke: fn(&H, &V, V) -> Result<(), E>,
    pub normalize_candle: fn(&H, V) -> Result<V, E>,
    pub iterator: fn(&H, &V) -> Result<V, E>,
    pub next: fn(&H, &V) -> Result<Option<V>, E>,
    pub close_on_error: fn(&H, &V, &E),
    pub pair: fn(&H, V) -> Result<(V, V), E>,
    pub tickers_by_symbol: fn(&H) -> Result<V, E>,
    pub ticker_updater: fn(&H) -> Result<V, E>,
    pub update_ticker: fn(&H, V, V, V) -> Result<(), E>,
    pub update_price: fn(&H, &V, &'static str, V, V) -> Result<(), E>,
    pub calc_totals: fn(&H, &V) -> Result<(), E>,
    pub base_dispose: fn(&H) -> Result<(), E>,
    pub new_error: fn(&H, Vec<u16>) -> E,
}
fn template<H, V, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    parts: &[&V],
    separators: &[&str],
) -> Result<V, E> {
    let mut result = Vec::new();
    for (i, v) in parts.iter().enumerate() {
        result.extend(separators[i].encode_utf16());
        result.extend((rt.template_string)(host, v)?);
    }
    result.extend(separators[parts.len()].encode_utf16());
    Ok((rt.string_units)(result))
}
fn numeric<H, V, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    object: &V,
    key: &str,
    default: V,
) -> Result<V, E> {
    (rt.to_number)(host, (rt.property)(host, object, key)?, default)
}
fn body_loop<H, V, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    values: &V,
    mut body: impl FnMut(V) -> Result<(), E>,
) -> Result<(), E> {
    let iterator = (rt.iterator)(host, values)?;
    while let Some(value) = (rt.next)(host, &iterator)? {
        if let Err(error) = body(value) {
            (rt.close_on_error)(host, &iterator, &error);
            return Err(error);
        }
    }
    Ok(())
}

pub struct WeexPublicConstructionRuntime<H, V, E> {
    pub base_construct: fn(&H, V) -> Result<V, E>,
    pub define: fn(&H, &V, &'static str, V) -> Result<(), E>,
    pub new_set: fn(&H) -> Result<V, E>,
    pub object: fn(Vec<(&'static str, V)>) -> V,
    pub array: fn(Vec<V>) -> V,
    pub number: fn(f64) -> V,
    pub boolean: fn(bool) -> V,
    pub property: fn(&H, &V, &str) -> Result<V, E>,
    pub method: fn(&H, &V, &'static str) -> Result<V, E>,
    pub call_event: fn(&H, V, V, &'static str, V) -> Result<(), E>,
}

pub fn construct_public_websocket<H, V: Clone, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    parent: V,
) -> Result<V, E> {
    let construction = WeexPublicConstructionRuntime {
        base_construct: rt.base_construct,
        define: rt.define,
        new_set: rt.new_set,
        object: rt.object,
        array: rt.array,
        number: rt.number,
        boolean: rt.boolean,
        property: rt.property,
        method: rt.method,
        call_event: rt.call_event,
    };
    construct_public_websocket_with_bind(host, &construction, parent, rt.bind_field)
}

pub fn construct_public_websocket_with_bind<H, V: Clone, E>(
    host: &H,
    rt: &WeexPublicConstructionRuntime<H, V, E>,
    parent: V,
    mut bind: impl FnMut(&H, &V, &'static str) -> Result<V, E>,
) -> Result<V, E> {
    let instance = (rt.base_construct)(host, parent)?;
    (rt.define)(host, &instance, "treatAnyMessageAsPong", (rt.boolean)(true))?;
    (rt.define)(host, &instance, "messageHandlers", (rt.object)(Vec::new()))?;
    (rt.define)(host, &instance, "topics", (rt.new_set)(host)?)?;
    (rt.define)(host, &instance, "tickerTopics", (rt.new_set)(host)?)?;
    (rt.define)(host, &instance, "subscriptionId", (rt.number)(1.0))?;
    (rt.define)(
        host,
        &instance,
        "subscriptionOperationTimestamps",
        (rt.array)(Vec::new()),
    )?;
    (rt.define)(
        host,
        &instance,
        "hasLoggedSubscriptionLimit",
        (rt.boolean)(false),
    )?;
    (rt.define)(host, &instance, "orderbooks", (rt.object)(Vec::new()))?;
    for name in [
        "connectAndSubscribe",
        "onOpen",
        "ping",
        "onMessage",
        "listenOHLCV",
        "listenOrderBook",
        "subscribeTickerTopics",
        "handleActiveTickerSymbolsChanged",
        "getDesiredTickerTopics",
        "reconcileTickerTopics",
        "resubscribeTopics",
        "subscribeTopic",
        "unsubscribeTopic",
        "subscribeTopics",
        "sendSubscriptionOperation",
        "addHandler",
        "removeHandler",
        "handleRoutedMessage",
        "handleTicker",
        "handleDepth",
    ] {
        let bound = bind(host, &instance, name)?;
        (rt.define)(host, &instance, name, bound)?;
    }
    let parent = (rt.property)(host, &instance, "parent")?;
    let on = (rt.method)(host, &parent, "on")?;
    let handler = (rt.property)(host, &instance, "handleActiveTickerSymbolsChanged")?;
    (rt.call_event)(host, on, parent, "activeTickerSymbols:changed", handler)?;
    Ok(instance)
}

pub fn connect_and_subscribe<H, V, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
) -> Result<(), E> {
    if (rt.disposed)(host)? {
        return Ok(());
    }
    let parent = (rt.this_property)(host, "parent")?;
    let options = (rt.property)(host, &parent, "options")?;
    let testnet = (rt.property)(host, &options, "testnet")?;
    let key = if (rt.truthy)(&testnet) {
        "testnet"
    } else {
        "livenet"
    };
    let public = BASE_WS_URL
        .iter()
        .find(|(key, _)| *key == "public")
        .unwrap()
        .1;
    let url = public.iter().find(|(name, _)| *name == key).unwrap().1;
    let headers = (rt.object)(vec![("User-Agent", (rt.text)("Tealstreet"))]);
    let options = (rt.object)(vec![("headers", headers)]);
    let socket = (rt.create_socket)(host, (rt.text)(url), (rt.undefined)(), options)?;
    (rt.set_socket)(host, socket)?;
    for (event, name) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let socket = (rt.this_property)(host, "ws")?;
        let add = (rt.method)(host, &socket, "addEventListener")?;
        let handler = (rt.this_property)(host, name)?;
        (rt.add_listener)(host, add, socket, event, handler)?;
    }
    Ok(())
}

pub fn on_message<H, V: Clone, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    event: V,
) -> Result<(), E> {
    let data = (rt.property)(host, &event, "data")?;
    if (rt.disposed)(host)? {
        return Ok(());
    }
    (rt.track_received)(host)?;
    let json = (rt.parse)(host, (rt.string_constructor)(host, &data)?)?;
    if !(rt.truthy)(&json) {
        return Ok(());
    }
    if (rt.equals_text)(&(rt.property)(host, &json, "event")?, "ping") {
        let id = (rt.subscription_id)(host)?;
        (rt.set_subscription_id)(host, id + 1.0)?;
        (rt.send_pong)(host, id)?;
        (rt.handle_pong)(host, 15_000.0)?;
        return Ok(());
    }
    if !(rt.is_undefined)(&(rt.property)(host, &json, "result")?) {
        if (rt.is_false)(&(rt.property)(host, &json, "result")?) {
            let sink = (rt.warning_sink)(host)?;
            let msg = (rt.property)(host, &json, "msg")?;
            let msg = if (rt.truthy)(&msg) {
                msg
            } else {
                (rt.text)("unknown error")
            };
            let mut warning: Vec<u16> = "WEEX public WS subscription failed: "
                .encode_utf16()
                .collect();
            warning.extend((rt.template_string)(host, &msg)?);
            (rt.warning)(host, sink, warning)?;
        }
        return Ok(());
    }
    let event = (rt.property)(host, &json, "e")?;
    let symbol = (rt.property)(host, &json, "s")?;
    if !(rt.truthy)(&event) {
        return Ok(());
    }
    let topic = if (rt.truthy)(&symbol) {
        template(host, rt, &[&symbol, &event], &["", "@", ""])?
    } else {
        event.clone()
    };
    (rt.record_stats)(host, topic)?;
    if (rt.equals_text)(&event, "ticker") {
        (rt.invoke)(host, &(rt.this_property)(host, "handleTicker")?, json)
    } else if (rt.equals_text)(&event, "depth") {
        (rt.invoke)(host, &(rt.this_property)(host, "handleDepth")?, json)
    } else if (rt.equals_text)(&event, "kline") {
        let route = (rt.route_method)(host)?;
        let mut units: Vec<u16> = "kline.".encode_utf16().collect();
        units.extend((rt.template_string)(host, &symbol)?);
        units.push(46);
        let rows = (rt.property)(host, &json, "d")?;
        let row = (rt.optional_property)(host, &rows, "0")?;
        let interval = (rt.optional_property)(host, &row, "i")?;
        let interval = if (rt.truthy)(&interval) {
            interval
        } else {
            (rt.text)("")
        };
        units.extend((rt.template_string)(host, &interval)?);
        (rt.route)(host, route, (rt.string_units)(units), &json)
    } else if (rt.equals_text)(&event, "trade") {
        let route = (rt.route_method)(host)?;
        let key = template(host, rt, &[&symbol], &["trade.", ""])?;
        (rt.route)(host, route, key, &json)
    } else {
        Ok(())
    }
}

pub fn listen_ohlcv<H, V: Clone, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    opts: V,
    callback: V,
) -> Result<V, E> {
    let interval = (rt.interval)(host, (rt.property)(host, &opts, "interval")?)?;
    if !(rt.supported_interval)(&interval) {
        let mut message: Vec<u16> = "WEEX public WebSocket does not support "
            .encode_utf16()
            .collect();
        message.extend((rt.template_string)(
            host,
            &(rt.property)(host, &opts, "interval")?,
        )?);
        message.extend(" candles".encode_utf16());
        let sink = (rt.warning_sink)(host)?;
        (rt.warning)(host, sink, message.clone())?;
        return Err((rt.new_error)(host, message));
    }
    let symbol = (rt.property)(host, &opts, "symbol")?;
    let topic = template(
        host,
        rt,
        &[&symbol, &interval],
        &["", "@kline_", "_LAST_PRICE"],
    )?;
    let symbol = (rt.property)(host, &opts, "symbol")?;
    let key = template(host, rt, &[&symbol, &interval], &["kline.", ".", ""])?;
    let handler = (rt.make_callback)(host, WeexPublicCallback::Candle { callback })?;
    (rt.add_handler)(host, &key, &handler)?;
    (rt.subscribe)(host, &topic)?;
    (rt.make_callback)(
        host,
        WeexPublicCallback::CandleUnsubscribe {
            key,
            topic,
            handler,
        },
    )
}

pub fn listen_order_book<H, V: Clone, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    symbol: V,
    callback: V,
) -> Result<V, E> {
    let key = template(host, rt, &[&symbol], &["depth.", ""])?;
    let topic = template(host, rt, &[&symbol], &["", "@depth200", ""])?;
    let handler = (rt.make_callback)(
        host,
        WeexPublicCallback::Book {
            symbol: symbol.clone(),
            callback: callback.clone(),
        },
    )?;
    (rt.add_handler)(host, &key, &handler)?;
    let request = (rt.snapshot)(host, &symbol, 200.0)?;
    (rt.observe_snapshot)(
        host,
        request,
        WeexPublicCallback::BookSnapshot {
            symbol: symbol.clone(),
            topic: topic.clone(),
            callback,
        },
        WeexPublicCallback::SnapshotFailure {
            symbol: symbol.clone(),
        },
    )?;
    (rt.make_callback)(
        host,
        WeexPublicCallback::BookUnsubscribe {
            symbol,
            key,
            topic,
            handler,
        },
    )
}

pub fn run_public_callback<H, V: Clone, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    callback: &WeexPublicCallback<V>,
    value: V,
) -> Result<(), E> {
    match callback {
        WeexPublicCallback::Candle { callback } => {
            let candidate = (rt.property)(host, &value, "d")?;
            let rows = if (rt.is_array)(host, &candidate)? {
                (rt.property)(host, &value, "d")?
            } else {
                (rt.array)(Vec::new())
            };
            body_loop(host, rt, &rows, |row| {
                let candle = (rt.normalize_candle)(host, row)?;
                if (rt.truthy)(&candle) {
                    (rt.invoke)(host, callback, candle)?;
                }
                Ok(())
            })
        }
        WeexPublicCallback::Book { symbol, callback } => {
            let books = (rt.books)(host)?;
            let book = (rt.lookup)(host, &books, symbol)?;
            if (rt.truthy)(&book) {
                (rt.invoke)(host, callback, book)?;
            }
            Ok(())
        }
        WeexPublicCallback::CandleUnsubscribe {
            key,
            topic,
            handler,
        } => {
            (rt.remove_handler)(host, key, handler)?;
            (rt.unsubscribe)(host, topic)
        }
        WeexPublicCallback::BookUnsubscribe {
            symbol,
            key,
            topic,
            handler,
        } => {
            (rt.remove_handler)(host, key, handler)?;
            (rt.unsubscribe)(host, topic)?;
            let books = (rt.books)(host)?;
            (rt.remove)(host, &books, symbol)
        }
        WeexPublicCallback::BookSnapshot {
            symbol,
            topic,
            callback,
        } => {
            if (rt.disposed)(host)? {
                return Ok(());
            }
            let books = (rt.books)(host)?;
            (rt.put_book)(host, &books, symbol, value.clone())?;
            (rt.invoke)(host, callback, value)?;
            (rt.subscribe)(host, topic)
        }
        WeexPublicCallback::GapSnapshot { symbol, json } => {
            if !(rt.disposed)(host)? {
                let books = (rt.books)(host)?;
                (rt.put_book)(host, &books, symbol, value)?;
                let route = (rt.route_method)(host)?;
                let key = template(host, rt, &[symbol], &["depth.", ""])?;
                (rt.route)(host, route, key, json)?;
            }
            Ok(())
        }
        WeexPublicCallback::GapSnapshotFailure => Ok(()),
        WeexPublicCallback::SnapshotFailure { symbol } => {
            let sink = (rt.warning_sink)(host)?;
            let mut warning: Vec<u16> = "WEEX orderbook snapshot failed for "
                .encode_utf16()
                .collect();
            warning.extend((rt.template_string)(host, symbol)?);
            warning.extend(": ".encode_utf16());
            let message = (rt.optional_property)(host, &value, "message")?;
            let message = if (rt.truthy)(&message) {
                message
            } else {
                value
            };
            warning.extend((rt.template_string)(host, &message)?);
            (rt.warning)(host, sink, warning)
        }
    }
}

pub fn handle_ticker<H, V: Clone, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    json: V,
) -> Result<(), E> {
    let tickers = (rt.tickers_by_symbol)(host)?;
    let symbol = (rt.property)(host, &json, "s")?;
    let ticker = (rt.lookup)(host, &tickers, &symbol)?;
    let candidate = (rt.property)(host, &json, "d")?;
    let update = if (rt.is_array)(host, &candidate)? {
        let rows = (rt.property)(host, &json, "d")?;
        (rt.property)(host, &rows, "0")?
    } else {
        (rt.property)(host, &json, "d")?
    };
    if !(rt.truthy)(&ticker) || !(rt.truthy)(&update) {
        return Ok(());
    }
    let get = |key: &str, fallback: &str| {
        let raw = (rt.property)(host, &update, key)?;
        let fallback = (rt.property)(host, &ticker, fallback)?;
        (rt.to_number)(host, raw, fallback)
    };
    let last = get("c", "last")?;
    let mark = get("m", "mark")?;
    let index = get("i", "index")?;
    let percentage = get("P", "percentage")?;
    let updater = (rt.ticker_updater)(host)?;
    let percentage = if (rt.abs)(host, &percentage)? <= 1.0 {
        (rt.multiply)(host, &percentage, 100.0)?
    } else {
        percentage
    };
    let volume = get("v", "volume")?;
    let quote_volume = get("q", "quoteVolume")?;
    let update = (rt.object)(vec![
        ("last", last),
        ("mark", mark),
        ("index", index),
        ("percentage", percentage),
        ("volume", volume),
        ("quoteVolume", quote_volume),
    ]);
    (rt.update_ticker)(host, updater, ticker, update)
}

pub fn handle_depth<H, V: Clone, E>(
    host: &H,
    rt: &WeexPublicFeedRuntime<H, V, E>,
    json: V,
) -> Result<(), E> {
    let symbol = (rt.property)(host, &json, "s")?;
    let books = (rt.books)(host)?;
    let book = (rt.lookup)(host, &books, &symbol)?;
    if !(rt.truthy)(&book) {
        return Ok(());
    }
    let zero = || (rt.number)(0.0);
    let previous = numeric(host, rt, &book, "lastUpdateId", zero())?;
    let first = numeric(host, rt, &json, "U", zero())?;
    let last = numeric(host, rt, &json, "u", zero())?;
    let previous = (rt.number_value)(&previous);
    let first = (rt.number_value)(&first);
    if previous != 0.0 && first > previous + 1.0 {
        let request = (rt.snapshot)(host, &symbol, 200.0)?;
        (rt.observe_snapshot)(
            host,
            request,
            WeexPublicCallback::GapSnapshot {
                symbol: symbol.clone(),
                json,
            },
            WeexPublicCallback::GapSnapshotFailure,
        )?;
        return Ok(());
    }
    for (key, side) in [("b", "bids"), ("a", "asks")] {
        let rows = (rt.property)(host, &json, key)?;
        let rows = if (rt.truthy)(&rows) {
            rows
        } else {
            (rt.array)(Vec::new())
        };
        body_loop(host, rt, &rows, |row| {
            let (price, amount) = (rt.pair)(host, row)?;
            let price = (rt.to_number)(host, price, zero())?;
            let amount = (rt.to_number)(host, amount, zero())?;
            (rt.update_price)(host, &book, side, price, amount)
        })?;
    }
    let last = if (rt.truthy)(&last) {
        last
    } else {
        (rt.property)(host, &json, "u")?
    };
    let id = (rt.to_integer_string)(host, last)?;
    (rt.assign)(host, &book, "lastUpdateId", id)?;
    (rt.calc_totals)(host, &book)?;
    let route = (rt.route_method)(host)?;
    let key = template(host, rt, &[&symbol], &["depth.", ""])?;
    (rt.route)(host, route, key, &json)
}

pub fn dispose<H, V, E>(host: &H, rt: &WeexPublicFeedRuntime<H, V, E>) -> Result<(), E> {
    let parent = (rt.this_property)(host, "parent")?;
    let off = (rt.method)(host, &parent, "off")?;
    let handler = (rt.this_property)(host, "handleActiveTickerSymbolsChanged")?;
    (rt.call_event)(host, off, parent, "activeTickerSymbols:changed", handler)?;
    (rt.base_dispose)(host)
}
