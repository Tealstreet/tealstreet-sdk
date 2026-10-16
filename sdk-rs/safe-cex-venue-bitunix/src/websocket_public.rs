//! Public heartbeat and subscription lifecycle operations.
use safe_cex_num::JsJsonValue;
use std::rc::Rc;

pub const BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION: f64 = 300.0;
pub const INTERVAL: &[(&str, &str)] = &[
    ("1m", "1min"),
    ("3m", "3min"),
    ("5m", "5min"),
    ("15m", "15min"),
    ("30m", "30min"),
    ("1h", "60min"),
    ("2h", "2h"),
    ("4h", "4h"),
    ("6h", "6h"),
    ("12h", "12h"),
    ("1d", "1day"),
    ("1w", "1week"),
];

pub fn ping<E>(
    disposed: bool,
    performance_now: impl FnOnce() -> Result<f64, E>,
    set_ping_at: impl FnOnce(f64) -> Result<(), E>,
    clock_now: impl FnOnce() -> Result<f64, E>,
    send_with_log: impl FnOnce(JsJsonValue) -> Result<(), E>,
) -> Result<(), E> {
    if !disposed {
        set_ping_at(performance_now()?)?;
        send_with_log(JsJsonValue::Object(vec![(
            "ping".into(),
            JsJsonValue::Number(clock_now()?),
        )]))?;
    }
    Ok(())
}

pub fn clear_subscription_queue<T, E>(
    read_timer: impl FnOnce() -> Result<Option<T>, E>,
    clear_timeout: impl FnOnce(Option<T>) -> Result<(), E>,
    unset_timer: impl FnOnce() -> Result<(), E>,
    replace_queue: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    clear_timeout(read_timer()?)?;
    unset_timer()?;
    replace_queue()
}

pub fn drain_subscription_queue<A, T, W, E>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    queue_is_empty: impl FnOnce() -> Result<bool, E>,
    shift: impl FnOnce() -> Result<Option<A>, E>,
    has_args: impl FnOnce(&A) -> Result<bool, E>,
    (serialize, send): (
        impl FnOnce(&'static str, A) -> Result<W, E>,
        impl FnOnce(W) -> Result<(), E>,
    ),
    (schedule, set_timer): (
        impl FnOnce(&'static str, f64) -> Result<T, E>,
        impl FnOnce(Option<T>) -> Result<(), E>,
    ),
) -> Result<(), E> {
    if is_disposed()? || queue_is_empty()? {
        return set_timer(None);
    }
    if let Some(args) = shift()? {
        if has_args(&args)? {
            send(serialize("subscribe", args)?)?;
        }
    }
    let timer = schedule("drainSubscriptionQueue", 250.0)?;
    set_timer(Some(timer))
}

pub fn dispose<E>(
    off: impl FnOnce(&'static str, &'static str) -> Result<(), E>,
    clear_ping_timeout: impl FnOnce() -> Result<(), E>,
    clear_queue: impl FnOnce() -> Result<(), E>,
    base_dispose: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    off("markets:changed", "onMarketsChanged")?;
    clear_ping_timeout()?;
    clear_queue()?;
    base_dispose()
}

pub fn listen_ohlcv<T>() -> Result<T, safe_cex_num::JsCoercionError> {
    Err(safe_cex_num::JsCoercionError {
        name: "Error",
        message: "Not implemented",
    })
}

pub fn listen_order_book<T>() -> Result<T, safe_cex_num::JsCoercionError> {
    Err(safe_cex_num::JsCoercionError {
        name: "Error",
        message: "Not implemented",
    })
}

pub fn set_market_symbols<S, I, E>(
    input: Option<I>,
    assign: impl FnOnce(Option<Vec<S>>) -> Result<(), E>,
) -> Result<(), E>
where
    S: PartialEq,
    I: IntoIterator<Item = Result<S, E>>,
{
    let symbols = match input {
        Some(input) => {
            let mut symbols = Vec::new();
            for symbol in input {
                let symbol = symbol?;
                if !symbols.contains(&symbol) {
                    symbols.push(symbol);
                }
            }
            Some(symbols)
        }
        None => None,
    };
    assign(symbols)
}

pub fn enqueue_subscriptions<A, E>(
    batch_size: Option<f64>,
    mut length: impl FnMut() -> Result<f64, E>,
    mut slice: impl FnMut(f64, f64) -> Result<A, E>,
    mut push: impl FnMut(A) -> Result<(), E>,
    timer_is_active: impl FnOnce() -> Result<bool, E>,
    drain: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    let batch_size = batch_size.unwrap_or(20.0);
    let mut index = 0.0;
    while index < length()? {
        push(slice(index, index + batch_size)?)?;
        index += batch_size;
    }
    if !timer_is_active()? {
        drain()?;
    }
    Ok(())
}

pub fn subscribe<K, A, E>(
    topics: impl FnOnce() -> Result<Vec<K>, E>,
    mut is_depth: impl FnMut(&K) -> Result<bool, E>,
    mut parse: impl FnMut(&K) -> Result<A, E>,
    mut enqueue: impl FnMut(Vec<A>, Option<f64>) -> Result<(), E>,
) -> Result<(), E> {
    let topics = topics()?;
    let mut depth = Vec::new();
    for topic in &topics {
        if is_depth(topic)? {
            depth.push(topic);
        }
    }
    let mut others = Vec::new();
    for topic in &topics {
        if !is_depth(topic)? {
            others.push(topic);
        }
    }
    for topic in depth {
        enqueue(vec![parse(topic)?], Some(1.0))?;
    }
    let args = others
        .into_iter()
        .map(&mut parse)
        .collect::<Result<_, E>>()?;
    enqueue(args, None)
}

pub fn subscribe_markets_to_channel<M, S, C, A, E>(
    channel: C,
    markets: impl FnOnce() -> Result<Vec<M>, E>,
    mut allowed: impl FnMut(&M) -> Result<bool, E>,
    mut symbol: impl FnMut(M) -> Result<S, E>,
    mut make_arg: impl FnMut(S, &C) -> Result<A, E>,
    mut track: impl FnMut(A, f64) -> Result<(), E>,
) -> Result<(), E> {
    let mut filtered = Vec::new();
    for market in markets()? {
        if allowed(&market)? {
            filtered.push(market);
        }
    }
    let args = filtered
        .into_iter()
        .take(BITUNIX_MAX_SUBSCRIPTIONS_PER_CONNECTION as usize)
        .map(|m| make_arg(symbol(m)?, &channel))
        .collect::<Result<Vec<_>, E>>()?;
    for arg in args {
        track(arg, 1.0)?;
    }
    Ok(())
}

pub fn subscribe_to_book<S, C, A, E>(
    symbol: S,
    channel: C,
    mut make_arg: impl FnMut(&S, &C) -> Result<A, E>,
    track: impl FnOnce(A, f64) -> Result<(), E>,
    enqueue: impl FnOnce(Vec<A>, f64) -> Result<(), E>,
) -> Result<(), E> {
    track(make_arg(&symbol, &channel)?, 1.0)?;
    enqueue(vec![make_arg(&symbol, &channel)?], 1.0)
}

pub fn on_markets_changed<M, A, E>(
    has_added: impl FnOnce() -> Result<bool, E>,
    has_socket: impl FnOnce() -> Result<bool, E>,
    is_disposed: impl FnOnce() -> Result<bool, E>,
    secondary: impl FnOnce() -> Result<bool, E>,
    added: impl FnOnce() -> Result<Vec<M>, E>,
    mut allowed: impl FnMut(&M) -> Result<bool, E>,
    mut make_arg: impl FnMut(M, &'static str) -> Result<A, E>,
    mut track: impl FnMut(&A, f64) -> Result<(), E>,
    enqueue: impl FnOnce(Vec<A>) -> Result<(), E>,
) -> Result<(), E> {
    if !has_added()? || !has_socket()? || is_disposed()? {
        return Ok(());
    }
    let channel = if secondary()? { "price" } else { "ticker" };
    let mut filtered = Vec::new();
    for market in added()? {
        if allowed(&market)? {
            filtered.push(market);
        }
    }
    let args = filtered
        .into_iter()
        .map(|m| make_arg(m, channel))
        .collect::<Result<Vec<_>, E>>()?;
    for arg in &args {
        track(arg, 1.0)?;
    }
    enqueue(args)
}

pub fn on_open<T, E>(
    mut is_disposed: impl FnMut() -> Result<bool, E>,
    mut is_secondary: impl FnMut() -> Result<bool, E>,
    mut subscribe_markets: impl FnMut(&'static str) -> Result<(), E>,
    reset_book: impl FnOnce() -> Result<(), E>,
    subscribe: impl FnOnce() -> Result<(), E>,
    (off, on): (
        impl FnOnce(&'static str, &'static str) -> Result<(), E>,
        impl FnOnce(&'static str, &'static str) -> Result<(), E>,
    ),
    (mut timer, active, clear): (
        impl FnMut() -> Result<Option<T>, E>,
        impl FnOnce(&Option<T>) -> bool,
        impl FnOnce(Option<T>) -> Result<(), E>,
    ),
    (ping, schedule, set): (
        impl FnOnce() -> Result<(), E>,
        impl FnOnce(&'static str, f64) -> Result<T, E>,
        impl FnOnce(T) -> Result<(), E>,
    ),
) -> Result<(), E> {
    if !is_disposed()? {
        if !is_secondary()? {
            subscribe_markets("ticker")?;
        }
        if is_secondary()? {
            subscribe_markets("price")?;
        }
        reset_book()?;
        subscribe()?;
        off("markets:changed", "onMarketsChanged")?;
        on("markets:changed", "onMarketsChanged")?;
        if active(&timer()?) {
            clear(timer()?)?;
        }
        ping_loop(is_disposed, ping, schedule, set)?;
    }
    Ok(())
}

pub fn handle_ticker_stream_event<T, D, E>(
    ticker: impl FnOnce() -> Result<Option<T>, E>,
    data: impl FnOnce() -> Result<D, E>,
    mut coerce_field: impl FnMut(&D, &'static str) -> Result<Vec<u16>, E>,
    update: impl FnOnce(T, [(&'static str, f64); 4]) -> Result<(), E>,
) -> Result<(), E> {
    if let Some(ticker) = ticker()? {
        let data = data()?;
        let last = parse_source_float(&coerce_field(&data, "la")?);
        let open = parse_source_float(&coerce_field(&data, "o")?);
        update(
            ticker,
            [
                ("last", last),
                ("percentage", ((last - open) / open) * 100.0),
                ("volume", parse_source_float(&coerce_field(&data, "b")?)),
                (
                    "quoteVolume",
                    parse_source_float(&coerce_field(&data, "q")?),
                ),
            ],
        )?;
    }
    Ok(())
}

pub fn handle_price_stream_event<T, D, E>(
    ticker: impl FnOnce() -> Result<Option<T>, E>,
    data: impl FnOnce() -> Result<D, E>,
    mut coerce_field: impl FnMut(&D, &'static str) -> Result<Vec<u16>, E>,
    update: impl FnOnce(T, [(&'static str, f64); 4]) -> Result<(), E>,
) -> Result<(), E> {
    if let Some(ticker) = ticker()? {
        let data = data()?;
        update(
            ticker,
            [
                ("mark", parse_source_float(&coerce_field(&data, "mp")?)),
                ("index", parse_source_float(&coerce_field(&data, "ip")?)),
                (
                    "fundingRate",
                    parse_source_float(&coerce_field(&data, "fr")?),
                ),
                (
                    "fundingTime",
                    parse_source_float(&coerce_field(&data, "ft")?),
                ),
            ],
        )?;
    }
    Ok(())
}

pub fn handle_depth_book_stream_event<T, D, E>(
    ticker: impl FnOnce() -> Result<Option<T>, E>,
    data: impl FnOnce() -> Result<D, E>,
    mut coerce_first_price: impl FnMut(&D, &'static str) -> Result<Vec<u16>, E>,
    update: impl FnOnce(T, [(&'static str, f64); 2]) -> Result<(), E>,
) -> Result<(), E> {
    if let Some(ticker) = ticker()? {
        let data = data()?;
        let bid = parse_source_float(&coerce_first_price(&data, "b")?);
        let bid = if bid == 0.0 || bid.is_nan() { 0.0 } else { bid };
        let ask = parse_source_float(&coerce_first_price(&data, "a")?);
        let ask = if ask == 0.0 || ask.is_nan() { 0.0 } else { ask };
        update(ticker, [("bid", bid), ("ask", ask)])?;
    }
    Ok(())
}

pub fn on_message<D, P, T, K, H, L, E>(
    data: D,
    is_disposed: impl FnOnce() -> Result<bool, E>,
    track: impl FnOnce() -> Result<(), E>,
    (count, set_count): (
        impl FnOnce() -> Result<f64, E>,
        impl FnOnce(f64) -> Result<(), E>,
    ),
    (parse, topic): (
        impl FnOnce(D) -> Result<P, E>,
        impl FnOnce(&P) -> Result<T, E>,
    ),
    (truthy, equal, starts_with): (
        impl Fn(&T) -> bool,
        impl Fn(&T, &'static str) -> bool,
        impl FnOnce(&T, &'static str) -> Result<bool, E>,
    ),
    stats: impl FnOnce(&T) -> Result<(), E>,
    stream: impl FnOnce(&'static str, &P) -> Result<(), E>,
    (key, handlers, mut next, mut call): (
        impl FnOnce(&P, &T, [&'static str; 2]) -> Result<K, E>,
        impl FnOnce(K) -> Result<L, E>,
        impl FnMut(&L, usize) -> Result<Option<H>, E>,
        impl FnMut(H, Rc<Vec<Rc<P>>>) -> Result<(), E>,
    ),
    report: impl FnOnce(E) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed()? {
        track()?;
        set_count(count()? + 1.0)?;
        let result = (|| {
            let parsed = Rc::new(parse(data)?);
            let topic = topic(&parsed)?;
            if truthy(&topic) {
                stats(&topic)?;
            }
            if !truthy(&topic) {
                return Ok(());
            }
            if equal(&topic, "ticker") {
                stream("ticker", &parsed)?;
            } else if equal(&topic, "price") {
                stream("price", &parsed)?;
            } else if equal(&topic, "depth_book1") {
                stream("depth_book1", &parsed)?;
            } else if starts_with(&topic, "market_kline_")? {
                let key = key(&parsed, &topic, ["symbol", "ch"])?;
                let list = handlers(key)?;
                let mut index = 0;
                while let Some(handler) = next(&list, index)? {
                    call(handler, Rc::new(vec![parsed.clone()]))?;
                    index += 1;
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            report(error)?;
        }
    }
    Ok(())
}

pub fn ping_loop<T, E>(
    is_disposed: impl FnOnce() -> Result<bool, E>,
    ping: impl FnOnce() -> Result<(), E>,
    schedule: impl FnOnce(&'static str, f64) -> Result<T, E>,
    set: impl FnOnce(T) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed()? {
        ping()?;
        set(schedule("doPingLoop", 15_000.0)?)?;
    }
    Ok(())
}

// Surrogates cannot extend ASCII numeric tokens or JavaScript whitespace.
fn parse_source_float(units: &[u16]) -> f64 {
    safe_cex_num::parse_float(&String::from_utf16_lossy(units))
}
