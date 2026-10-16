//! Eight websocket bodies over bound source receiver/runtime intrinsics.
//! Concrete sockets, JSON parser, registry and complete classes stay separate.
use std::rc::Rc;

/// Values/errors retain native identity. Bindings implement source property
/// reads, strict string equality, truthiness and String coercion in call order.
pub trait Runtime: 'static {
    type Value: Clone + 'static;
    type Error: 'static;
    fn undefined(&self) -> Self::Value;
    fn nullish(&self, value: &Self::Value) -> bool;
    fn truthy(&self, value: &Self::Value) -> bool;
    fn is_text(&self, value: &Self::Value, text: &str) -> bool;
    fn field(&self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    /// ECMAScript abstract ToString, as used by templates/parseFloat;
    /// Symbol throws here, unlike the global String(Symbol) constructor.
    fn string(&self, value: &Self::Value) -> Result<Vec<u16>, Self::Error>;
    fn log(&self, message: &[u16], severity: &'static str) -> Result<(), Self::Error>;
}
/// Current receiver fields and ordinary websocket addEventListener bindings.
pub trait ConnectHost: Runtime {
    type Socket: Clone;
    type Listener: Clone;
    fn credentials(&self) -> Result<Self::Value, Self::Error>;
    fn disposed(&self) -> bool;
    fn socket(&self) -> Result<Option<Self::Socket>, Self::Error>;
    fn url(&self) -> Result<Vec<u16>, Self::Error>;
    fn create_socket(&self, url: &[u16]) -> Result<Self::Socket, Self::Error>;
    fn assign_socket(&self, socket: Self::Socket) -> Result<(), Self::Error>;
    fn listener(&self, event: &'static str) -> Result<Self::Listener, Self::Error>;
    fn add_listener(
        &self,
        socket: Self::Socket,
        event: &'static str,
        listener: Self::Listener,
    ) -> Result<(), Self::Error>;
    fn add_error_listener(
        &self,
        socket: Self::Socket,
        callback: Rc<dyn Fn(Self::Value) -> Result<(), Self::Error>>,
    ) -> Result<(), Self::Error>;
    fn missing_socket_error(&self) -> Self::Error;
}
/// Parsed values and live Array.forEach bindings. Length is captured after
/// stats; each callback is looked up at invocation, skipping deleted holes.
/// handlers returns None only for falsy entries. Truthy invalid/inherited entries
/// must retain a handle and fail at handler_length after stats, as source does.
pub trait MessageHost: Runtime {
    type Event;
    type Handlers: Clone;
    type Handler;
    fn track_received(&self) -> Result<(), Self::Error>;
    fn event_data(&self, event: &Self::Event) -> Result<Self::Value, Self::Error>;
    fn json_parse(&self, raw: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn pong(&self) -> Result<(), Self::Error>;
    fn handlers(&self, topic: &Self::Value) -> Result<Option<Self::Handlers>, Self::Error>;
    fn stats(&self, topic: &Self::Value) -> Result<(), Self::Error>;
    fn handler_length(&self, handlers: &Self::Handlers) -> Result<usize, Self::Error>;
    fn handler_at(
        &self,
        handlers: &Self::Handlers,
        index: usize,
    ) -> Result<Option<Self::Handler>, Self::Error>;
    fn invoke(&self, handler: Self::Handler, data: &Self::Value) -> Result<(), Self::Error>;
    fn refresh_positions(&self, trigger: &'static str) -> Result<(), Self::Error>;
}
/// Anonymous subscribe/unsubscribe object before JSON.stringify. The send
/// binding applies the source stringify/send intrinsic, preserving these fields.
#[derive(Debug, Clone)]
pub struct AssetOperation {
    pub assets_ids: Vec<Vec<u16>>,
    pub operation: &'static str,
}
/// Ordinary Set/Map intrinsics and current subscription receiver child calls.
pub trait SubscriptionHost: Runtime {
    type Callback: Clone + 'static;
    type Callbacks: Clone;
    fn has_asset(&self, token: &[u16]) -> Result<bool, Self::Error>;
    fn insert_asset(&self, token: &[u16]) -> Result<(), Self::Error>;
    fn delete_asset(&self, token: &[u16]) -> Result<(), Self::Error>;
    fn connected(&self) -> bool;
    fn send_operation(&self, operation: AssetOperation) -> Result<(), Self::Error>;
    fn has_callbacks(&self, token: &[u16]) -> Result<bool, Self::Error>;
    fn insert_callback_set(&self, token: &[u16]) -> Result<(), Self::Error>;
    fn callbacks(&self, token: &[u16]) -> Result<Option<Self::Callbacks>, Self::Error>;
    fn add_callback(
        &self,
        set: &Self::Callbacks,
        callback: Self::Callback,
    ) -> Result<(), Self::Error>;
    fn delete_callback(
        &self,
        set: &Self::Callbacks,
        callback: &Self::Callback,
    ) -> Result<(), Self::Error>;
    fn callback_size(&self, set: &Self::Callbacks) -> usize;
    fn delete_callback_set(&self, token: &[u16]) -> Result<(), Self::Error>;
    fn subscribe_asset(&self, token: &[u16]) -> Result<(), Self::Error>;
    fn unsubscribe_asset(&self, token: &[u16]) -> Result<(), Self::Error>;
    fn missing_callbacks_error(&self) -> Self::Error;
}
/// Native partial Ticker object and id selector, before any store encoding.
#[derive(Debug, Clone)]
pub struct LastTradePatch<V> {
    pub id: V,
    pub symbol: V,
    pub last: f64,
}
pub trait TradeHost: Runtime {
    fn token_symbol(&self, token: &Self::Value) -> Result<Self::Value, Self::Error>;
    fn update_ticker(
        &self,
        id: &Self::Value,
        update: LastTradePatch<Self::Value>,
    ) -> Result<(), Self::Error>;
}

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}
fn error_callback<H: ConnectHost>(
    host: Rc<H>,
    prefix: &'static str,
) -> Rc<dyn Fn(H::Value) -> Result<(), H::Error>> {
    Rc::new(move |event| {
        let message = if host.nullish(&event) {
            host.undefined()
        } else {
            host.field(&event, "message")?
        };
        let suffix = if host.truthy(&message) {
            host.string(&message)?
        } else {
            units("Unknown error")
        };
        let mut text = units(prefix);
        text.extend(suffix);
        host.log(&text, "error")
    })
}
fn install<H: ConnectHost>(host: Rc<H>, prefix: &'static str) -> Result<(), H::Error> {
    for event in ["open", "message", "close"] {
        let socket = host.socket()?.ok_or_else(|| host.missing_socket_error())?;
        let listener = host.listener(event)?;
        host.add_listener(socket, event, listener)?;
    }
    let socket = host.socket()?.ok_or_else(|| host.missing_socket_error())?;
    let callback = error_callback(host.clone(), prefix);
    host.add_error_listener(socket, callback)
}
fn route<H: MessageHost>(host: &H, topic: &H::Value, data: &H::Value) -> Result<(), H::Error> {
    if let Some(handlers) = host.handlers(topic)? {
        host.stats(topic)?;
        let length = host.handler_length(&handlers)?;
        for index in 0..length {
            if let Some(handler) = host.handler_at(&handlers, index)? {
                host.invoke(handler, data)?;
            }
        }
    }
    Ok(())
}
fn parsed<H: MessageHost>(
    host: &H,
    event: &H::Event,
) -> Result<Option<(H::Value, H::Value)>, H::Error> {
    host.track_received()?;
    if host.is_text(&host.event_data(event)?, "PONG") {
        host.pong()?;
        return Ok(None);
    }
    let data = host.json_parse(&host.event_data(event)?)?;
    if !host.truthy(&data) {
        return Ok(None);
    }
    let primary = host.field(&data, "event_type")?;
    let topic = if host.truthy(&primary) {
        primary
    } else {
        host.field(&data, "type")?
    };
    if host.is_text(&topic, "pong") {
        host.pong()?;
        return Ok(None);
    }
    Ok(Some((data, topic)))
}

/// Source PolymarketPublicWebSocket.connectAndSubscribe.
pub fn connect_public<H: ConnectHost>(host: Rc<H>) -> Result<(), H::Error> {
    if !host.disposed() && host.socket()?.is_none() {
        host.log(
            &units("Connecting to Polymarket public WebSocket..."),
            "info",
        )?;
        let socket = host.create_socket(&host.url()?)?;
        host.assign_socket(socket)?;
        install(host, "Public WebSocket error: ")?;
    }
    Ok(())
}
/// Source PolymarketPrivateWebSocket.connectAndSubscribe; credentials first.
pub fn connect_private<H: ConnectHost>(host: Rc<H>) -> Result<(), H::Error> {
    if !host.truthy(&host.credentials()?) {
        host.log(
            &units("Cannot connect private WebSocket: no credentials"),
            "warning",
        )?;
        return Ok(());
    }
    if !host.disposed() && host.socket()?.is_none() {
        host.log(
            &units("Connecting to Polymarket private WebSocket..."),
            "info",
        )?;
        let socket = host.create_socket(&host.url()?)?;
        host.assign_socket(socket)?;
        install(host, "Private WebSocket error: ")?;
    }
    Ok(())
}
/// Source PolymarketPublicWebSocket.onMessage.
pub fn message_public<H: MessageHost>(host: &H, event: &H::Event) -> Result<(), H::Error> {
    if let Some((data, topic)) = parsed(host, event)? {
        route(host, &topic, &data)?;
    }
    Ok(())
}
/// Source PolymarketPrivateWebSocket.onMessage; auth and error precede routing.
pub fn message_private<H: MessageHost>(host: &H, event: &H::Event) -> Result<(), H::Error> {
    let Some((data, topic)) = parsed(host, event)? else {
        return Ok(());
    };
    if host.is_text(&topic, "authenticated") || host.truthy(&host.field(&data, "authenticated")?) {
        host.log(&units("Private WebSocket authenticated"), "info")?;
        host.refresh_positions("authentication")?;
        return Ok(());
    }
    if host.is_text(&topic, "error") || host.truthy(&host.field(&data, "error")?) {
        let error = host.field(&data, "error")?;
        let value = if host.truthy(&error) {
            error
        } else {
            host.field(&data, "message")?
        };
        let mut text = units("Private WebSocket error: ");
        text.extend(host.string(&value)?);
        host.log(&text, "error")?;
        return Ok(());
    }
    route(host, &topic, &data)
}
/// Source PolymarketPublicWebSocket.subscribeToAsset.
pub fn subscribe_asset<H: SubscriptionHost>(host: &H, token: &[u16]) -> Result<(), H::Error> {
    if host.has_asset(token)? {
        return Ok(());
    }
    host.insert_asset(token)?;
    if host.connected() {
        host.send_operation(AssetOperation {
            assets_ids: vec![token.to_vec()],
            operation: crate::constants::WS_MESSAGE_TYPES
                .iter()
                .find(|(key, _)| *key == "SUBSCRIBE")
                .expect("source constant")
                .1,
        })?;
    }
    Ok(())
}
/// Source PolymarketPublicWebSocket.unsubscribeFromAsset.
pub fn unsubscribe_asset<H: SubscriptionHost>(host: &H, token: &[u16]) -> Result<(), H::Error> {
    if !host.has_asset(token)? {
        return Ok(());
    }
    host.delete_asset(token)?;
    if host.connected() {
        host.send_operation(AssetOperation {
            assets_ids: vec![token.to_vec()],
            operation: crate::constants::WS_MESSAGE_TYPES
                .iter()
                .find(|(key, _)| *key == "UNSUBSCRIBE")
                .expect("source constant")
                .1,
        })?;
    }
    Ok(())
}
/// Source subscribeToOrderBook and its repeatable returned unsubscribe closure.
pub fn subscribe_book<H: SubscriptionHost>(
    host: Rc<H>,
    token: &[u16],
    callback: H::Callback,
) -> Result<Rc<dyn Fn() -> Result<(), H::Error>>, H::Error> {
    if !host.has_callbacks(token)? {
        host.insert_callback_set(token)?;
    }
    let set = host
        .callbacks(token)?
        .ok_or_else(|| host.missing_callbacks_error())?;
    host.add_callback(&set, callback.clone())?;
    host.subscribe_asset(token)?;
    let token = token.to_vec();
    Ok(Rc::new(move || {
        if let Some(set) = host.callbacks(&token)? {
            host.delete_callback(&set, &callback)?;
            if host.callback_size(&set) == 0 {
                host.delete_callback_set(&token)?;
                host.unsubscribe_asset(&token)?;
            }
        }
        Ok(())
    }))
}
/// Source PolymarketPublicWebSocket#handleLastTradePrice. Symbol lookup precedes
/// price coercion/parsing; parsing still runs when the symbol is falsy.
pub fn last_trade<H: TradeHost>(host: &H, data: &H::Value) -> Result<(), H::Error> {
    let token = host.field(data, "asset_id")?;
    let symbol = host.token_symbol(&token)?;
    let price = host.string(&host.field(data, "price")?)?;
    let last = safe_cex_num::parse_float(&String::from_utf16_lossy(&price));
    if !host.truthy(&symbol) || !last.is_finite() {
        return Ok(());
    }
    host.update_ticker(
        &token,
        LastTradePatch {
            id: token.clone(),
            symbol,
            last,
        },
    )
}
