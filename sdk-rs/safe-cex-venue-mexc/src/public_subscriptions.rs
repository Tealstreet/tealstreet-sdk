//! Eight public websocket bodies over source receiver/runtime bindings.
//! Production class construction/composition remains a separate FINDING.
use std::rc::Rc;

/// Native values retain identity and UTF-16 strings. Coercion/property/equality
/// bindings implement the source intrinsics, including Symbol's distinct
/// abstract ToString (throws) and global String (returns a description).
pub trait Runtime: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    fn undefined(&self) -> Self::Value;
    fn truthy(&self, value: &Self::Value) -> bool;
    /// SameValueZero for Set membership: NaN matches NaN and +/-0 match.
    fn same_value(&self, left: &Self::Value, right: &Self::Value) -> bool;
    fn is_text(&self, value: &Self::Value, text: &str) -> bool;
    fn field(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    fn abstract_string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn global_string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn log(&self, text: &[u16], severity: &'static str) -> Result<(), Self::Error>;
    fn new_error(&self, text: Vec<u16>) -> Self::Error;
}
/// Prepared source createWebsocket arguments, before any transport encoding.
pub struct DialRequest<V> {
    pub url: Vec<u16>,
    pub protocols: V,
    pub headers: Vec<(&'static str, Vec<u16>)>,
}
pub trait ConnectHost: Runtime {
    type Socket: Clone;
    type Listener;
    fn disposed(&self) -> bool;
    fn testnet(&self) -> Result<Self::Value, Self::Error>;
    fn url(&self, network: &'static str) -> Result<Vec<u16>, Self::Error>;
    fn create(&self, request: DialRequest<Self::Value>) -> Result<Self::Socket, Self::Error>;
    fn assign_socket(&self, socket: Self::Socket) -> Result<(), Self::Error>;
    /// Resolve current socket/addEventListener before reading its argument.
    fn socket(&self) -> Result<Self::Socket, Self::Error>;
    /// close resolves handleClose, not the parent onClose callback.
    fn listener(&self, event: &'static str) -> Result<Self::Listener, Self::Error>;
    fn add_listener(
        &self,
        socket: Self::Socket,
        event: &'static str,
        listener: Self::Listener,
    ) -> Result<(), Self::Error>;
}
pub trait CandleHost: Runtime {
    type Callback: Clone + 'static;
    fn interval(&self, key: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn to_symbol(&self, symbol: &Self::Value) -> Result<Self::Value, Self::Error>;
    /// Construct a fresh ordinary object with method and nested param in order.
    fn subscription(
        &self,
        method: &'static str,
        param: Vec<(&'static str, Self::Value)>,
    ) -> Self::Value;
    fn normalize(&self, data: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn callback(&self, callback: &Self::Callback, candle: Self::Value) -> Result<(), Self::Error>;
    fn add_handler(
        &self,
        key: &[u16],
        handler: Rc<dyn Fn(Self::Value) -> Result<(), Self::Error>>,
    ) -> Result<(), Self::Error>;
    fn remove_handler(
        &self,
        key: &[u16],
        handler: &Rc<dyn Fn(Self::Value) -> Result<(), Self::Error>>,
    ) -> Result<(), Self::Error>;
    fn subscribe(&self, subscription: Self::Value) -> Result<(), Self::Error>;
    fn unsubscribe(&self, subscription: Self::Value) -> Result<(), Self::Error>;
}
/// Ordered Map snapshots/cursors preserve source identity. entries/values
/// capture the receiver once; next reads its live map including append/delete.
/// Getter/child calls below reread the current receiver field at each call site.
pub trait TickerHost: Runtime {
    type Map;
    type Cursor;
    fn disposed(&self) -> bool;
    fn reconcile(&self) -> Result<(), Self::Error>;
    /// Ordinary dense typed Array.map and active-symbol Array projections.
    /// Values are detached snapshots, captured in source read order.
    fn available_symbols(&self) -> Result<Vec<Self::Value>, Self::Error>;
    fn active_symbols(&self) -> Result<Vec<Self::Value>, Self::Error>;
    fn market(&self, symbol: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn subscription(
        &self,
        method: &'static str,
        param: Vec<(&'static str, Self::Value)>,
    ) -> Self::Value;
    fn stringify(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn map(&self, name: &'static str) -> Result<Self::Map, Self::Error>;
    fn cursor(&self, map: Self::Map, values_only: bool) -> Result<Self::Cursor, Self::Error>;
    fn next(
        &self,
        cursor: &mut Self::Cursor,
    ) -> Result<Option<(Vec<u16>, Self::Value)>, Self::Error>;
    fn has(&self, map: &Self::Map, key: &[u16]) -> Result<bool, Self::Error>;
    fn set(&self, map: &Self::Map, key: Vec<u16>, value: Self::Value) -> Result<(), Self::Error>;
    fn delete(&self, map: &Self::Map, key: &[u16]) -> Result<(), Self::Error>;
    fn subscribe(&self, subscription: Self::Value, track: bool) -> Result<(), Self::Error>;
    fn unsubscribe(&self, subscription: Self::Value, track: bool) -> Result<(), Self::Error>;
    fn send(&self, data: Vec<u16>) -> Result<(), Self::Error>;
}
/// Source diagnostics options keep their callbacks live beyond the root call.
pub struct SendOptions<V, E> {
    pub format: Rc<dyn Fn(V) -> Result<Vec<u16>, E>>,
}
pub struct ReceiveOptions<V, E> {
    pub allow_channel: Rc<dyn Fn() -> Result<bool, E>>,
    pub format: Rc<dyn Fn(V) -> Result<Vec<u16>, E>>,
}
pub trait DiagnosticHost: Runtime {
    fn send(&self, data: &[u16]) -> Result<(), Self::Error>;
    fn emit_send(
        &self,
        data: &[u16],
        options: SendOptions<Self::Value, Self::Error>,
    ) -> Result<(), Self::Error>;
    fn emit_receive(
        &self,
        data: Self::Value,
        options: ReceiveOptions<Self::Value, Self::Error>,
    ) -> Result<(), Self::Error>;
}
/// Optional length access precedes obtaining the iterator. Array iteration
/// rereads live length/slots and yields undefined for holes; truthy inherited
/// non-iterable entries fail at iterator creation rather than becoming absent.
pub trait RouteHost: Runtime {
    type Handlers;
    type Iterator;
    type Handler;
    /// None means nullish only; non-null falsy values still get length read.
    fn handlers(&self, key: &[u16]) -> Result<Option<Self::Handlers>, Self::Error>;
    fn length(&self, handlers: &Self::Handlers) -> Result<Self::Value, Self::Error>;
    fn iterator(&self, handlers: Self::Handlers) -> Result<Self::Iterator, Self::Error>;
    fn next_handler(
        &self,
        iterator: &mut Self::Iterator,
    ) -> Result<Option<Self::Handler>, Self::Error>;
    fn invoke(&self, handler: Self::Handler, json: &Self::Value) -> Result<(), Self::Error>;
}
fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
/// Source MexcPublicWebsocket.connectAndSubscribe; only the disposal guard.
pub fn connect<H: ConnectHost>(host: &H) -> Result<(), H::Error> {
    if host.disposed() {
        return Ok(());
    }
    let network = if host.truthy(&host.testnet()?) {
        "testnet"
    } else {
        "livenet"
    };
    let request = DialRequest {
        url: host.url(network)?,
        protocols: host.undefined(),
        headers: vec![("User-Agent", units("Tealstreet"))],
    };
    let socket = host.create(request)?;
    host.assign_socket(socket)?;
    for event in ["open", "message", "close"] {
        let socket = host.socket()?;
        let listener = host.listener(event)?;
        host.add_listener(socket, event, listener)?;
    }
    Ok(())
}
/// Source listenOHLCV plus retained handler and repeatable cleanup.
pub fn listen_ohlcv<H: CandleHost>(
    host: Rc<H>,
    opts: &H::Value,
    callback: H::Callback,
) -> Result<Rc<dyn Fn() -> Result<(), H::Error>>, H::Error> {
    let interval = host.interval(&host.field(opts, "interval")?)?;
    if !host.truthy(&interval) {
        let mut text = units("MEXC public WebSocket does not support ");
        text.extend(host.abstract_string(&host.field(opts, "interval")?)?);
        text.extend(units(" candles"));
        host.log(&text, "warning")?;
        return Err(host.new_error(text));
    }
    let mut key = units("kline.");
    key.extend(host.abstract_string(&host.field(opts, "symbol")?)?);
    key.push('.' as u16);
    key.extend(host.abstract_string(&interval)?);
    let symbol = host.to_symbol(&host.field(opts, "symbol")?)?;
    let subscription = host.subscription(
        "sub.kline",
        vec![("symbol", symbol), ("interval", interval)],
    );
    let bound = host.clone();
    let handler: Rc<dyn Fn(H::Value) -> Result<(), H::Error>> = Rc::new(move |json| {
        let data = bound.field(&json, "data")?;
        let data = if bound.truthy(&data) { data } else { json };
        let candle = bound.normalize(&data)?;
        if bound.truthy(&candle) {
            bound.callback(&callback, candle)?;
        }
        Ok(())
    });
    host.add_handler(&key, handler.clone())?;
    host.subscribe(subscription.clone())?;
    Ok(Rc::new(move || {
        host.remove_handler(&key, &handler)?;
        host.unsubscribe(subscription.clone())
    }))
}
/// Source disposal guard then current reconcile child call.
pub fn active_tickers_changed<H: TickerHost>(host: &H) -> Result<(), H::Error> {
    if host.disposed() {
        return Ok(());
    }
    host.reconcile()
}
/// Source desired Map difference, 100-symbol cap, and ordered live map traversal.
pub fn reconcile_tickers<H: TickerHost>(host: &H) -> Result<(), H::Error> {
    let available = host.available_symbols()?;
    let active = host.active_symbols()?;
    let symbols: Vec<_> = available
        .into_iter()
        .filter(|symbol| active.is_empty() || active.iter().any(|v| host.same_value(v, symbol)))
        .collect();
    let mut desired: Vec<(Vec<u16>, H::Value)> = vec![];
    for symbol in symbols.into_iter().take(100) {
        let market = host.market(&symbol)?;
        if !host.truthy(&market) {
            continue;
        }
        let subscription =
            host.subscription("sub.ticker", vec![("symbol", host.field(&market, "id")?)]);
        let key = host.stringify(&subscription)?;
        if let Some((_, value)) = desired.iter_mut().find(|(k, _)| *k == key) {
            *value = subscription;
        } else {
            desired.push((key, subscription));
        }
    }
    let mut cursor = host.cursor(host.map("tickerSubscriptions")?, false)?;
    while let Some((key, subscription)) = host.next(&mut cursor)? {
        if !desired.iter().any(|(k, _)| *k == key) {
            host.unsubscribe(subscription, false)?;
            let map = host.map("tickerSubscriptions")?;
            host.delete(&map, &key)?;
        }
    }
    for (key, subscription) in desired {
        let map = host.map("tickerSubscriptions")?;
        if !host.has(&map, &key)? {
            let map = host.map("tickerSubscriptions")?;
            host.set(&map, key, subscription.clone())?;
            host.subscribe(subscription, false)?;
        }
    }
    Ok(())
}
/// Source regular subscriptions precede ticker subscriptions; both iterators are live.
pub fn resubscribe<H: TickerHost>(host: &H) -> Result<(), H::Error> {
    for name in ["subscriptions", "tickerSubscriptions"] {
        let mut cursor = host.cursor(host.map(name)?, true)?;
        while let Some((_, subscription)) = host.next(&mut cursor)? {
            host.send(host.stringify(&subscription)?)?;
        }
    }
    Ok(())
}
/// Source send before diagnostic emit; template interpolation uses abstract ToString.
pub fn send_diagnostics<H: DiagnosticHost>(host: Rc<H>, data: &[u16]) -> Result<(), H::Error> {
    DiagnosticHost::send(&*host, data)?;
    let bound = host.clone();
    host.emit_send(
        data,
        SendOptions {
            format: Rc::new(move |message| {
                let mut text = units("[mexc public] ");
                text.extend(bound.abstract_string(&message)?);
                Ok(text)
            }),
        },
    )
}
/// Source deferred short-circuit channel predicate and global String formatter.
pub fn receive_diagnostics<H: DiagnosticHost>(
    host: Rc<H>,
    data: H::Value,
    json: H::Value,
) -> Result<(), H::Error> {
    let bound = host.clone();
    let channel = json.clone();
    let format = host.clone();
    host.emit_receive(
        data,
        ReceiveOptions {
            allow_channel: Rc::new(move || {
                Ok(
                    !bound.is_text(&bound.field(&channel, "channel")?, "push.ticker")
                        && !bound.is_text(&bound.field(&channel, "channel")?, "push.depth")
                        && !bound.is_text(&bound.field(&channel, "channel")?, "push.depth.full"),
                )
            }),
            format: Rc::new(move |message| {
                let mut text = units("[mexc public] ");
                text.extend(format.global_string(&message)?);
                Ok(text)
            }),
        },
    )
}
/// Source optional-length guard then live for-of iteration over captured handlers.
pub fn route_message<H: RouteHost>(host: &H, key: &[u16], json: &H::Value) -> Result<(), H::Error> {
    let Some(handlers) = host.handlers(key)? else {
        return Ok(());
    };
    if !host.truthy(&host.length(&handlers)?) {
        return Ok(());
    }
    let mut iterator = host.iterator(handlers)?;
    while let Some(handler) = host.next_handler(&mut iterator)? {
        host.invoke(handler, json)?;
    }
    Ok(())
}
