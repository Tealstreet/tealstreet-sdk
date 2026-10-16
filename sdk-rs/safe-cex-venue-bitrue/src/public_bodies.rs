//! Complete public websocket bodies at explicit JavaScript operation boundaries.
//! The class, builtin implementations, Promise scheduler, JSON codec and transport
//! bindings remain separate. Values, errors, receivers and callbacks stay native.
use std::rc::Rc;

pub type NativeCallback<V, E> = Rc<dyn Fn(Vec<V>) -> Result<V, E>>;
/// A runtime boundary for ordinary property/callee operations. `call` must use
/// the captured callee and receiver; `iterate` owns IteratorClose on body error.
/// Array methods, setters and Promise construction may synchronously reenter.
pub trait PublicBodyHost: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    fn undefined(&self) -> Self::Value;
    fn text(&self, value: &str) -> Self::Value;
    fn number(&self, value: f64) -> Self::Value;
    fn boolean(&self, value: bool) -> Self::Value;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn equal_zero(&self, value: &Self::Value) -> bool;
    fn global(&self, name: &'static str) -> Result<Self::Value, Self::Error>;
    fn get(&self, receiver: &Self::Value, key: Self::Value) -> Result<Self::Value, Self::Error>;
    fn set(
        &self,
        receiver: &Self::Value,
        key: Self::Value,
        value: Self::Value,
    ) -> Result<(), Self::Error>;
    fn call(
        &self,
        callee: Self::Value,
        receiver: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn construct(
        &self,
        callee: Self::Value,
        args: Vec<Self::Value>,
    ) -> Result<Self::Value, Self::Error>;
    fn function(&self, callback: NativeCallback<Self::Value, Self::Error>) -> Self::Value;
    fn object(&self, fields: Vec<(&'static str, Self::Value)>) -> Self::Value;
    fn array(&self, values: Vec<Self::Value>) -> Self::Value;
    fn template(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn add(&self, left: Self::Value, right: Self::Value) -> Result<Self::Value, Self::Error>;
    fn divide(&self, left: Self::Value, right: Self::Value) -> Result<Self::Value, Self::Error>;
    fn multiply(&self, left: Self::Value, right: Self::Value) -> Result<Self::Value, Self::Error>;
    fn unary_plus(&self, value: Self::Value) -> Result<Self::Value, Self::Error>;
    fn iterate(
        &self,
        value: Self::Value,
        body: &mut dyn FnMut(Self::Value) -> Result<(), Self::Error>,
    ) -> Result<(), Self::Error>;
}
fn get<H: PublicBodyHost>(h: &H, receiver: &H::Value, key: &str) -> Result<H::Value, H::Error> {
    h.get(receiver, h.text(key))
}
fn method<H: PublicBodyHost>(
    h: &H,
    receiver: H::Value,
    key: &str,
) -> Result<(H::Value, H::Value), H::Error> {
    Ok((get(h, &receiver, key)?, receiver))
}
fn invoke<H: PublicBodyHost>(
    h: &H,
    prepared: (H::Value, H::Value),
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.call(prepared.0, prepared.1, args)
}
fn global_call<H: PublicBodyHost>(
    h: &H,
    name: &'static str,
    args: Vec<H::Value>,
) -> Result<H::Value, H::Error> {
    h.call(h.global(name)?, h.undefined(), args)
}
fn arg<H: PublicBodyHost>(h: &H, args: &[H::Value], index: usize) -> H::Value {
    args.get(index).cloned().unwrap_or_else(|| h.undefined())
}
fn ticker_topic<H: PublicBodyHost>(
    h: &H,
    this: &H::Value,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    let prepared = method(h, this.clone(), "convertToWsSymbol")?;
    let converted = invoke(h, prepared, vec![symbol])?;
    let converted = h.template(converted)?;
    let topic = h.add(h.text("market_"), converted)?;
    h.add(topic, h.text("_ticker"))
}
fn payload<H: PublicBodyHost>(h: &H, event: &str, topic: H::Value) -> Result<H::Value, H::Error> {
    let id = global_call(h, "nanoid", vec![])?;
    let params = h.object(vec![("channel", topic), ("cb_id", id)]);
    Ok(h.object(vec![("event", h.text(event)), ("params", params)]))
}
fn send_payload<H: PublicBodyHost>(
    h: &H,
    this: &H::Value,
    event: &str,
    topic: H::Value,
) -> Result<(), H::Error> {
    let send = method(h, this.clone(), "send")?;
    let json = h.global("JSON")?;
    let stringify = method(h, json, "stringify")?;
    let frame = payload(h, event, topic)?;
    let encoded = invoke(h, stringify, vec![frame])?;
    invoke(h, send, vec![encoded])?;
    Ok(())
}

pub fn ensure_ticker_subscription<H: PublicBodyHost>(
    h: Rc<H>,
    this: H::Value,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    let set = get(h.as_ref(), &this, "tickerSymbols")?;
    let has = method(h.as_ref(), set, "has")?;
    if h.truthy(&invoke(h.as_ref(), has, vec![symbol.clone()])?) {
        return Ok(h.undefined());
    }
    let topic = ticker_topic(h.as_ref(), &this, symbol.clone())?;
    let set = get(h.as_ref(), &this, "tickerSymbols")?;
    let add = method(h.as_ref(), set, "add")?;
    invoke(h.as_ref(), add, vec![symbol.clone()])?;
    let topics = get(h.as_ref(), &this, "topics")?;
    h.set(&topics, topic.clone(), h.number(1.0))?;
    let handlers = get(h.as_ref(), &this, "messageHandlers")?;
    let host = h.clone();
    let receiver = this.clone();
    let callback = h.function(Rc::new(move |args| {
        let prepared = method(host.as_ref(), receiver.clone(), "handleTickersEvent")?;
        invoke(
            host.as_ref(),
            prepared,
            vec![arg(host.as_ref(), &args, 0), symbol.clone()],
        )
    }));
    h.set(&handlers, topic, h.array(vec![callback]))?;
    Ok(h.undefined())
}

pub fn connect_and_subscribe<H: PublicBodyHost>(
    h: Rc<H>,
    this: H::Value,
) -> Result<H::Value, H::Error> {
    if !h.truthy(&get(h.as_ref(), &this, "isDisposed")?) {
        let parent = get(h.as_ref(), &this, "parent")?;
        let store = get(h.as_ref(), &parent, "store")?;
        let markets = get(h.as_ref(), &store, "markets")?;
        let each = method(h.as_ref(), markets, "forEach")?;
        let host = h.clone();
        let receiver = this.clone();
        let callback = h.function(Rc::new(move |args| {
            let prepared = method(host.as_ref(), receiver.clone(), "ensureTickerSubscription")?;
            let symbol = get(host.as_ref(), &arg(host.as_ref(), &args, 0), "symbol")?;
            invoke(host.as_ref(), prepared, vec![symbol])?;
            Ok(host.undefined())
        }));
        invoke(h.as_ref(), each, vec![callback])?;
        let create = method(h.as_ref(), this.clone(), "createWebsocket")?;
        let urls = h.global("BASE_WS_URL")?;
        let url = get(h.as_ref(), &urls, "public")?;
        let url = h.add(url, h.text("?requestId="))?;
        let id = global_call(h.as_ref(), "nanoid", vec![])?;
        let socket = invoke(h.as_ref(), create, vec![h.add(url, id)?])?;
        h.set(&this, h.text("ws"), socket)?;
        for (event, handler) in [
            ("open", "onOpen"),
            ("message", "onMessage"),
            ("close", "onClose"),
        ] {
            let socket = get(h.as_ref(), &this, "ws")?;
            let listener = method(h.as_ref(), socket, "addEventListener")?;
            let handler = get(h.as_ref(), &this, handler)?;
            invoke(h.as_ref(), listener, vec![h.text(event), handler])?;
        }
    }
    Ok(h.undefined())
}

pub fn on_open<H: PublicBodyHost>(h: Rc<H>, this: H::Value) -> Result<H::Value, H::Error> {
    if !h.truthy(&get(h.as_ref(), &this, "isDisposed")?) {
        invoke(
            h.as_ref(),
            method(h.as_ref(), this.clone(), "subscribe")?,
            vec![],
        )?;
        invoke(
            h.as_ref(),
            method(h.as_ref(), this.clone(), "ping")?,
            vec![],
        )?;
        for name in ["off", "on"] {
            let parent = get(h.as_ref(), &this, "parent")?;
            let emitter = get(h.as_ref(), &parent, "emitter")?;
            let prepared = method(h.as_ref(), emitter, name)?;
            let handler = get(h.as_ref(), &this, "onMarketsChanged")?;
            invoke(
                h.as_ref(),
                prepared,
                vec![h.text("markets:changed"), handler],
            )?;
        }
    }
    Ok(h.undefined())
}

pub fn on_markets_changed<H: PublicBodyHost>(
    h: Rc<H>,
    this: H::Value,
    event: H::Value,
) -> Result<H::Value, H::Error> {
    let added = get(h.as_ref(), &event, "added")?;
    if h.equal_zero(&get(h.as_ref(), &added, "length")?)
        || !h.truthy(&get(h.as_ref(), &this, "ws")?)
        || h.truthy(&get(h.as_ref(), &this, "isDisposed")?)
    {
        return Ok(h.undefined());
    }
    h.iterate(added, &mut |market| {
        let prepared = method(h.as_ref(), this.clone(), "ensureTickerSubscription")?;
        let symbol = get(h.as_ref(), &market, "symbol")?;
        invoke(h.as_ref(), prepared, vec![symbol])?;
        let convert = method(h.as_ref(), this.clone(), "convertToWsSymbol")?;
        let symbol = get(h.as_ref(), &market, "symbol")?;
        let converted = invoke(h.as_ref(), convert, vec![symbol])?;
        let converted = h.template(converted)?;
        let topic = h.add(h.text("market_"), converted)?;
        let topic = h.add(topic, h.text("_ticker"))?;
        if h.truthy(&get(h.as_ref(), &this, "isConnected")?) {
            // The source builds this frame BEFORE resolving send/stringify.
            let frame = payload(h.as_ref(), "sub", topic)?;
            let send = method(h.as_ref(), this.clone(), "send")?;
            let json = h.global("JSON")?;
            let stringify = method(h.as_ref(), json, "stringify")?;
            let encoded = invoke(h.as_ref(), stringify, vec![frame])?;
            invoke(h.as_ref(), send, vec![encoded])?;
        }
        Ok(())
    })?;
    Ok(h.undefined())
}

pub fn subscribe<H: PublicBodyHost>(h: Rc<H>, this: H::Value) -> Result<H::Value, H::Error> {
    let object = h.global("Object")?;
    let keys = method(h.as_ref(), object, "keys")?;
    let topics = get(h.as_ref(), &this, "topics")?;
    let keys = invoke(h.as_ref(), keys, vec![topics])?;
    let each = method(h.as_ref(), keys, "forEach")?;
    let host = h.clone();
    let receiver = this.clone();
    let callback = h.function(Rc::new(move |args| {
        let frame = payload(host.as_ref(), "sub", arg(host.as_ref(), &args, 0))?;
        let send = method(host.as_ref(), receiver.clone(), "send")?;
        let json = host.global("JSON")?;
        let stringify = method(host.as_ref(), json, "stringify")?;
        let encoded = invoke(host.as_ref(), stringify, vec![frame])?;
        invoke(host.as_ref(), send, vec![encoded])?;
        Ok(host.undefined())
    }));
    invoke(h.as_ref(), each, vec![callback])?;
    Ok(h.undefined())
}

pub fn map_kline<H: PublicBodyHost>(
    h: Rc<H>,
    _this: H::Value,
    json: H::Value,
) -> Result<H::Value, H::Error> {
    let map = method(h.as_ref(), json, "map")?;
    let host = h.clone();
    let callback = h.function(Rc::new(move |args| {
        let candle = arg(host.as_ref(), &args, 0);
        let dayjs = host.global("dayjs")?;
        let ds = get(host.as_ref(), &candle, "ds")?;
        let day = host.call(dayjs, host.undefined(), vec![ds])?;
        let offset = method(host.as_ref(), day, "utcOffset")?;
        let adjusted = invoke(
            host.as_ref(),
            offset,
            vec![host.number(480.0), host.boolean(true)],
        )?;
        let value = invoke(
            host.as_ref(),
            method(host.as_ref(), adjusted, "valueOf")?,
            vec![],
        )?;
        let timestamp = host.divide(value, host.number(1000.0))?;
        let open = get(host.as_ref(), &candle, "open")?;
        let high = get(host.as_ref(), &candle, "high")?;
        let low = get(host.as_ref(), &candle, "low")?;
        let close = get(host.as_ref(), &candle, "close")?;
        let volume = get(host.as_ref(), &candle, "vol")?;
        Ok(host.object(vec![
            ("timestamp", timestamp),
            ("open", open),
            ("high", high),
            ("low", low),
            ("close", close),
            ("volume", volume),
            ("info", candle),
        ]))
    }));
    invoke(h.as_ref(), map, vec![callback])
}

pub fn handle_tickers_event<H: PublicBodyHost>(
    h: Rc<H>,
    this: H::Value,
    json: H::Value,
    symbol: H::Value,
) -> Result<H::Value, H::Error> {
    invoke(
        h.as_ref(),
        method(h.as_ref(), this.clone(), "recordMessageHandlerStats")?,
        vec![h.text("ticker")],
    )?;
    let resolved = invoke(
        h.as_ref(),
        method(h.as_ref(), this.clone(), "resolveMarketSymbol")?,
        vec![symbol],
    )?;
    let close = get(h.as_ref(), &json, "close")?;
    let last = h.unary_plus(close)?;
    let rose = get(h.as_ref(), &json, "rose")?;
    let percentage = h.multiply(h.unary_plus(rose)?, h.number(100.0))?;
    let vol = get(h.as_ref(), &json, "vol")?;
    let volume = h.unary_plus(vol)?;
    let amount = get(h.as_ref(), &json, "amount")?;
    let quote_volume = h.unary_plus(amount)?;
    let ticker = h.object(vec![
        ("id", resolved.clone()),
        ("symbol", resolved.clone()),
        ("last", last.clone()),
        ("mark", last),
        ("percentage", percentage),
        ("volume", volume),
        ("quoteVolume", quote_volume),
        ("info", json),
    ]);
    let parent = get(h.as_ref(), &this, "parent")?;
    let store = get(h.as_ref(), &parent, "store")?;
    let update = method(h.as_ref(), store, "updateTicker")?;
    let selector = h.object(vec![("id", resolved)]);
    invoke(h.as_ref(), update, vec![selector, ticker])?;
    Ok(h.undefined())
}

pub fn make_request<H: PublicBodyHost>(
    h: Rc<H>,
    this: H::Value,
    topic: H::Value,
) -> Result<H::Value, H::Error> {
    let promise = h.global("Promise")?;
    let host = h.clone();
    let executor = h.function(Rc::new(move |args| {
        let resolve = arg(host.as_ref(), &args, 0);
        let reject = arg(host.as_ref(), &args, 1);
        let timer_fn = host.global("setTimeout")?;
        let timeout_host = host.clone();
        let timeout_this = this.clone();
        let timeout_topic = topic.clone();
        let timeout_callback = host.function(Rc::new(move |_| {
            let requests = get(timeout_host.as_ref(), &timeout_this, "resolveRequests")?;
            timeout_host.set(&requests, timeout_topic.clone(), timeout_host.array(vec![]))?;
            let constructor = timeout_host.global("Error")?;
            let error =
                timeout_host.construct(constructor, vec![timeout_host.text("Request timeout")])?;
            timeout_host.call(reject.clone(), timeout_host.undefined(), vec![error])?;
            Ok(timeout_host.undefined())
        }));
        let timeout = host.call(
            timer_fn,
            host.undefined(),
            vec![timeout_callback, host.number(5000.0)],
        )?;
        let resolve_host = host.clone();
        let resolve_handler = host.function(Rc::new(move |args| {
            global_call(resolve_host.as_ref(), "clearTimeout", vec![timeout.clone()])?;
            let data = get(
                resolve_host.as_ref(),
                &arg(resolve_host.as_ref(), &args, 0),
                "data",
            )?;
            resolve_host.call(resolve.clone(), resolve_host.undefined(), vec![data])?;
            Ok(resolve_host.undefined())
        }));
        let requests = get(host.as_ref(), &this, "resolveRequests")?;
        if !host.truthy(&host.get(&requests, topic.clone())?) {
            let requests = get(host.as_ref(), &this, "resolveRequests")?;
            host.set(&requests, topic.clone(), host.array(vec![]))?;
        }
        let requests = get(host.as_ref(), &this, "resolveRequests")?;
        let handlers = host.get(&requests, topic.clone())?;
        invoke(
            host.as_ref(),
            method(host.as_ref(), handlers, "push")?,
            vec![resolve_handler],
        )?;
        send_payload(host.as_ref(), &this, "req", topic.clone())?;
        Ok(host.undefined())
    }));
    h.construct(promise, vec![executor])
}
