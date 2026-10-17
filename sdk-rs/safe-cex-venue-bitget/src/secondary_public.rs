//! Concrete secondary ticker protocol attached to the shared socket lifecycle.
use safe_cex_exchange::{
    exchange::Exchange,
    runtime::{ExchangeError, ExchangeFuture},
    websocket::{BaseWebSocket, WsProtocol, WsRuntime},
    ws_parent::WsExchangeParent,
};
use safe_cex_transport::{RoutingOptions, resolve_exchange_routing, socket::SocketFrame};
use safe_cex_types::{ExchangeName, object_map::JsObjectMap};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

type Topics = Rc<RefCell<JsObjectMap<JsObjectMap<f64>>>>;

// ts: BitgetSecondaryPublicWebsocket
pub struct BitgetSecondaryPublicWebsocket {
    base: Rc<BaseWebSocket>,
    protocol: Rc<SecondaryProtocol>,
}
struct SecondaryProtocol {
    parent: Weak<dyn Exchange>,
    runtime: Rc<dyn WsRuntime>,
    core: RefCell<Weak<BaseWebSocket>>,
    index: Rc<Cell<f64>>,
    topics: Topics,
    message_count: Rc<Cell<f64>>,
    last_message_count: Rc<Cell<f64>>,
    connection_attempts: Rc<Cell<f64>>,
    use_fallback_url: Rc<Cell<bool>>,
    markets_listener: RefCell<Option<String>>,
    self_ref: RefCell<Weak<SecondaryProtocol>>,
}
impl BitgetSecondaryPublicWebsocket {
    // ts: BitgetSecondaryPublicWebsocket.constructor
    pub fn new(
        id: u64,
        parent: &Rc<dyn Exchange>,
        runtime: Rc<dyn WsRuntime>,
        index: f64,
    ) -> Rc<Self> {
        let protocol = Rc::new(SecondaryProtocol {
            parent: Rc::downgrade(parent),
            runtime: runtime.clone(),
            core: RefCell::default(),
            index: Rc::new(Cell::new(index)),
            topics: Rc::default(),
            message_count: Rc::new(Cell::new(0.0)),
            last_message_count: Rc::new(Cell::new(0.0)),
            connection_attempts: Rc::new(Cell::new(0.0)),
            use_fallback_url: Rc::new(Cell::new(false)),
            markets_listener: RefCell::default(),
            self_ref: RefCell::default(),
        });
        *protocol.self_ref.borrow_mut() = Rc::downgrade(&protocol);
        let base = BaseWebSocket::new(id, WsExchangeParent::new(parent), runtime, protocol.clone());
        *protocol.core.borrow_mut() = Rc::downgrade(&base);
        Rc::new(Self { base, protocol })
    }
    /// Stable shared core view; callers cannot replace the facade's binding.
    pub fn base(&self) -> &Rc<BaseWebSocket> {
        &self.base
    }
    pub fn index(&self) -> &Cell<f64> {
        &self.protocol.index
    }
    pub fn topics(&self) -> &RefCell<JsObjectMap<JsObjectMap<f64>>> {
        &self.protocol.topics
    }
    pub fn message_count(&self) -> f64 {
        self.protocol.message_count.get()
    }
    pub fn last_message_count(&self) -> f64 {
        self.protocol.last_message_count.get()
    }
    pub fn connection_attempts(&self) -> f64 {
        self.protocol.connection_attempts.get()
    }
    pub fn use_fallback_url(&self) -> bool {
        self.protocol.use_fallback_url.get()
    }
    // ts: BitgetSecondaryPublicWebsocket.connectAndSubscribe
    pub fn connect_and_subscribe(&self) -> Result<(), ExchangeError> {
        self.base.connect_and_subscribe(false)
    }
    // ts: BitgetSecondaryPublicWebsocket.onOpen
    pub fn on_open(&self) -> Result<(), ExchangeError> {
        self.protocol.open(&self.base)
    }
    // ts: BitgetSecondaryPublicWebsocket.onMarketsChanged
    pub fn on_markets_changed(&self, added: &[String]) -> Result<(), ExchangeError> {
        self.protocol.markets_changed(&self.base, added)
    }
    // ts: BitgetSecondaryPublicWebsocket.subscribe
    pub fn subscribe(&self) -> Result<(), ExchangeError> {
        self.protocol.subscribe(&self.base)
    }
    // ts: BitgetSecondaryPublicWebsocket.onMessage
    pub fn on_message(&self, frame: &SocketFrame) -> Result<(), ExchangeError> {
        self.base.on_message(frame)
    }
}
impl SecondaryProtocol {
    fn parent(&self) -> Result<Rc<dyn Exchange>, ExchangeError> {
        self.parent
            .upgrade()
            .ok_or_else(|| ExchangeError::new("Bitget secondary socket parent was dropped"))
    }
    fn product_type(&self) -> Result<String, ExchangeError> {
        let parent = self.parent()?;
        let testnet = parent.base().options().testnet.unwrap_or(false);
        let value = match parent.name() {
            ExchangeName::Bitget => crate::profile::api_product_type(testnet),
            ExchangeName::Bitgetusdc => crate::profile::usdc_api_product_type(testnet),
            ExchangeName::Bitgetinverse => crate::profile::inverse_api_product_type(testnet),
            _ => {
                return Err(ExchangeError::new(
                    "Bitget secondary socket requires a Bitget-family parent",
                ));
            }
        };
        Ok(value.to_uppercase())
    }
    fn argument(&self, channel: &str, symbol: &str) -> Result<Value, ExchangeError> {
        Ok(json!({"instType": self.product_type()?, "channel":channel,"instId":symbol}))
    }
    fn send(&self, socket: &Rc<BaseWebSocket>, args: Vec<Value>) -> Result<(), ExchangeError> {
        socket.send(
            &serde_json::to_string(&json!({"op":"subscribe","args":args}))
                .map_err(|e| ExchangeError::new(e.to_string()))?,
        )
    }
    fn subscribe(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        // Capture own keys before effects; release the map borrow before send can
        // synchronously re-enter the protocol or the parent event emitter.
        let keys: Vec<(String, String)> = self
            .topics
            .borrow()
            .iter()
            .flat_map(|(channel, symbols)| {
                symbols
                    .keys()
                    .map(|symbol| (channel.clone(), symbol.clone()))
            })
            .collect();
        let args = keys
            .iter()
            .map(|(channel, symbol)| self.argument(channel, symbol))
            .collect::<Result<Vec<_>, _>>()?;
        self.send(socket, args)
    }
    fn open(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        if socket.is_disposed() {
            return Ok(());
        }
        self.subscribe(socket)?;
        self.ping(socket)?;
        let parent = self.parent()?;
        if let Some(id) = self.markets_listener.borrow_mut().take() {
            parent.base().off("markets:changed", Some(&id));
        }
        let weak = self.core.borrow().clone();
        let protocol = self.self_ref.borrow().clone();
        let id = parent.base().on(
            "markets:changed",
            Rc::new(move |_, value| {
                let Some(socket) = weak.upgrade() else {
                    return Ok(());
                };
                let rows = value
                    .get("added")
                    .and_then(Value::as_array)
                    .ok_or_else(|| {
                        type_error("Cannot read properties of undefined (reading 'length')")
                    })?;
                let added = rows
                    .iter()
                    .map(|row| {
                        row.get("symbol")
                            .and_then(Value::as_str)
                            .map(str::to_owned)
                            .ok_or_else(|| {
                                type_error("Bitget added market requires a string symbol")
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let Some(protocol) = protocol.upgrade() else {
                    return Ok(());
                };
                protocol.markets_changed(&socket, &added)
            }),
        );
        *self.markets_listener.borrow_mut() = Some(id);
        if self.runtime.enable_reconnecting() {
            socket.emit_ws_reconnected()?;
        }
        Ok(())
    }
    fn markets_changed(
        &self,
        socket: &Rc<BaseWebSocket>,
        added: &[String],
    ) -> Result<(), ExchangeError> {
        if added.is_empty() || !socket.has_websocket() || socket.is_disposed() {
            return Ok(());
        }
        let existing = self
            .topics
            .borrow()
            .get("ticker")
            .map_or(0, |rows| rows.len());
        let symbols: Vec<&String> = added
            .iter()
            .enumerate()
            .filter(|(i, _)| ((existing + i) % 3) as f64 == self.index.get())
            .map(|(_, symbol)| symbol)
            .collect();
        if symbols.is_empty() {
            return Ok(());
        }
        let args = symbols
            .iter()
            .map(|symbol| self.argument("ticker", symbol))
            .collect::<Result<Vec<_>, _>>()?;
        let mut topics = self.topics.borrow_mut();
        if !topics.contains_key("ticker") {
            topics.insert("ticker".into(), JsObjectMap::new());
        }
        let ticker = topics.get_mut("ticker").unwrap();
        for symbol in symbols {
            // Plain-object assignment to the inherited __proto__ setter ignores
            // a numeric value; a computed own property from connect is updated.
            if symbol.as_str() != "__proto__" || ticker.contains_key(symbol) {
                ticker.insert(symbol.clone(), 1.0);
            }
        }
        drop(topics);
        self.send(socket, args)
    }
}
fn type_error(message: &str) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}
impl WsProtocol for SecondaryProtocol {
    fn ping(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        if !socket.is_disposed() {
            socket.mark_ping_sent();
            socket.send("ping")?;
        }
        Ok(())
    }
    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _: bool,
    ) -> Result<(), ExchangeError> {
        if socket.is_disposed() {
            return Ok(());
        }
        if self.last_message_count.get() == 0.0 && self.message_count.get() == 0.0 {
            self.connection_attempts
                .set(self.connection_attempts.get() + 1.0);
            if self.connection_attempts.get() >= 2.0 {
                self.use_fallback_url.set(true);
            }
        }
        self.last_message_count.set(self.message_count.get());
        self.message_count.set(0.0);
        let parent = self.parent()?;
        let ticker = parent
            .base()
            .store
            .markets()
            .iter()
            .enumerate()
            .filter(|(i, _)| (*i % 3) as f64 == self.index.get())
            .map(|(_, market)| (market.symbol.clone(), 1.0))
            .collect();
        self.topics.borrow_mut().insert("ticker".into(), ticker);
        let options = parent.base().options();
        let routing = resolve_exchange_routing(parent.name(), &RoutingOptions::from(&options));
        let fallback = routing
            .ws_public
            .fallback
            .filter(|url| !url.is_empty())
            .filter(|_| self.use_fallback_url.get());
        let url = if let Some(url) = fallback {
            // Source diagnostics are best effort; listener errors here do not
            // prevent the dial. Other listener failures still propagate.
            let _ = parent.base().emit(
                "grafana-info",
                json!("Using tealstreet bitget proxy (secondary public)"),
            );
            url
        } else {
            crate::protocol::BASE_WS_URL.to_owned()
        };
        socket.start_websocket(url, vec![], None);
        Ok(())
    }
    fn on_open(&self, socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = self.open(&socket);
        Box::pin(async move { result })
    }
    fn on_message(
        &self,
        socket: &Rc<BaseWebSocket>,
        frame: &SocketFrame,
    ) -> Result<(), ExchangeError> {
        if socket.is_disposed() {
            return Ok(());
        }
        socket.track_last_received()?;
        self.message_count.set(self.message_count.get() + 1.0);
        let text = match frame {
            SocketFrame::Text(text) => {
                if text == "pong" {
                    return socket.handle_pong_event(5_000);
                }
                std::borrow::Cow::Borrowed(text.as_str())
            }
            SocketFrame::Binary(bytes) => {
                std::borrow::Cow::Owned(String::from_utf8_lossy(bytes).into_owned())
            }
            SocketFrame::Close { .. } => return Err(type_error("data.includes is not a function")),
        };
        if text.contains("\"channel\":\"ticker\"") {
            let value = safe_cex_transport::how::json_parse(&text)
                .into_value()
                .map_err(|error| {
                    safe_cex_exchange::runtime::ExchangeError::new(error.to_string())
                })?;
            if value
                .get("arg")
                .and_then(|arg| arg.get("channel"))
                .and_then(Value::as_str)
                == Some("ticker")
                && value.get("event").and_then(Value::as_str) != Some("subscribe")
            {
                socket.record_message_handler_stats("ticker")?;
                crate::ticker::handle_ticker_topic(socket.store().as_ref(), Some(&value)).map_err(
                    |error| match error {
                        crate::ticker::TickerHandlerError::Coercion(error) => {
                            let mut converted = ExchangeError::new(error.message);
                            converted.name = error.name.into();
                            converted
                        }
                        crate::ticker::TickerHandlerError::Store(error) => {
                            ExchangeError::from(error)
                        }
                    },
                )?;
            }
        }
        Ok(())
    }
}
