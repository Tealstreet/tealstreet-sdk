//! Public MEXC WebSocket protocol over the exchange-core lifecycle.
use crate::{
    types::{BASE_WS_URL, is_mexc_control_channel},
    utils::{from_mexc_symbol, normalize_order_book, normalize_ticker},
    ws::subscription_key,
};
use indexmap::IndexMap;
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeFuture, LogSeverity},
    websocket::{
        BaseWebSocket, MessageHandler, WsDiagnosticReceiveOptions, WsDiagnosticSendOptions,
        WsProtocol,
    },
};
use safe_cex_store::{Store, model_patch::ModelPatch};
use safe_cex_transport::relay::WebSocketConnectionOptions;
use safe_cex_types::OrderBook;
use serde_json::{Map, Value, json};
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Debug, Clone, PartialEq)]
struct Subscription {
    method: String,
    param: Option<Value>,
}
impl Subscription {
    fn value(&self) -> Value {
        let mut value = Map::new();
        value.insert("method".into(), Value::String(self.method.clone()));
        if let Some(param) = &self.param {
            value.insert("param".into(), param.clone());
        }
        Value::Object(value)
    }
    fn key(&self) -> Result<String, ExchangeError> {
        subscription_key(&self.method, self.param.as_ref()).map_err(ExchangeError::new)
    }
}

/// MEXC public protocol state. BaseWebSocket owns dial, timers, and disposal.
pub struct MexcPublicProtocol {
    subscriptions: RefCell<IndexMap<String, Subscription>>,
    ticker_subscriptions: RefCell<IndexMap<String, Subscription>>,
    orderbooks: RefCell<HashMap<String, OrderBook>>,
    local_to_utc: Rc<dyn Fn(f64) -> f64>,
}

impl MexcPublicProtocol {
    pub fn new(local_to_utc: Rc<dyn Fn(f64) -> f64>) -> Self {
        Self {
            subscriptions: RefCell::new(IndexMap::new()),
            ticker_subscriptions: RefCell::new(IndexMap::new()),
            orderbooks: RefCell::new(HashMap::new()),
            local_to_utc,
        }
    }

    pub fn subscribe(
        &self,
        socket: &Rc<BaseWebSocket>,
        method: impl Into<String>,
        param: Option<Value>,
        track: bool,
    ) -> Result<(), ExchangeError> {
        let subscription = Subscription {
            method: method.into(),
            param,
        };
        let key = subscription.key()?;
        if track {
            self.subscriptions
                .borrow_mut()
                .insert(key, subscription.clone());
        }
        if socket.is_connected() {
            self.send(socket, &subscription.value())?;
        }
        Ok(())
    }

    pub fn unsubscribe(
        &self,
        socket: &Rc<BaseWebSocket>,
        method: &str,
        param: Option<Value>,
        track: bool,
    ) -> Result<(), ExchangeError> {
        let subscription = Subscription {
            method: method.into(),
            param,
        };
        let key = subscription.key()?;
        if track {
            self.subscriptions.borrow_mut().shift_remove(&key);
        }
        if socket.is_connected() {
            let unsubscribe_method = method
                .strip_prefix("sub.")
                .map_or_else(|| method.to_owned(), |rest| format!("unsub.{rest}"));
            self.send(
                socket,
                &Subscription {
                    method: unsubscribe_method,
                    param: subscription.param,
                }
                .value(),
            )?;
        }
        Ok(())
    }

    fn resubscribe(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        for subscription in self.subscriptions.borrow().values() {
            self.send(socket, &subscription.value())?;
        }
        for subscription in self.ticker_subscriptions.borrow().values() {
            self.send(socket, &subscription.value())?;
        }
        Ok(())
    }

    pub fn add_handler(
        &self,
        socket: &Rc<BaseWebSocket>,
        key: impl Into<String>,
        handler: MessageHandler,
    ) {
        let key = key.into();
        // The source spread creates a new array. An in-flight route retains
        // the previous array, including its existing callbacks.
        let mut next = socket
            .message_handlers
            .borrow()
            .get(&key)
            .map(|list| list.borrow().clone())
            .unwrap_or_default();
        next.push(handler);
        socket
            .message_handlers
            .borrow_mut()
            .insert(key, Rc::new(RefCell::new(next)));
    }

    pub fn remove_handler(
        &self,
        socket: &Rc<BaseWebSocket>,
        key: &str,
        handler: &MessageHandler,
    ) -> Result<(), ExchangeError> {
        // filter creates a replacement array even when no callbacks match,
        // and removing from a missing key creates an own empty array.
        let next: Vec<_> = socket
            .message_handlers
            .borrow()
            .get(key)
            .map(|list| {
                list.borrow()
                    .iter()
                    .filter(|candidate| !Rc::ptr_eq(candidate, handler))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        let empty = next.is_empty();
        socket
            .message_handlers
            .borrow_mut()
            .insert(key.into(), Rc::new(RefCell::new(next)));
        if empty {
            socket.cleanup_message_handler_stats(key)?;
        }
        Ok(())
    }

    fn send(&self, socket: &Rc<BaseWebSocket>, data: &Value) -> Result<(), ExchangeError> {
        let text =
            serde_json::to_string(data).map_err(|error| ExchangeError::new(error.to_string()))?;
        socket.send(&text)?;
        socket.emit_ws_diagnostic_send(
            &text,
            &WsDiagnosticSendOptions {
                format: Some(Rc::new(|message| Some(format!("[mexc public] {message}")))),
                redact: None,
            },
        )?;
        Ok(())
    }

    fn parse_frame(frame: &safe_cex_transport::socket::SocketFrame) -> Option<Value> {
        let text = match frame {
            safe_cex_transport::socket::SocketFrame::Text(text) => text.as_str(),
            safe_cex_transport::socket::SocketFrame::Binary(bytes) => {
                // MEXC's JS handler calls String(data); Node Buffer conversion
                // replaces invalid UTF-8, matching from_utf8_lossy here.
                return Some(safe_cex_transport::how::big_int_as_string_json_parse(
                    &String::from_utf8_lossy(bytes),
                ));
            }
            safe_cex_transport::socket::SocketFrame::Close { .. } => return None,
        };
        Some(safe_cex_transport::how::big_int_as_string_json_parse(text))
    }

    fn emit_receive_diagnostics(
        &self,
        socket: &Rc<BaseWebSocket>,
        raw: &str,
        parsed: &Value,
    ) -> Result<(), ExchangeError> {
        let allow = parsed
            .get("channel")
            .and_then(Value::as_str)
            .is_none_or(|channel| {
                !matches!(channel, "push.ticker" | "push.depth" | "push.depth.full")
            });
        if !allow {
            return Ok(());
        }
        let data = Value::String(raw.into());
        socket.emit_ws_diagnostic_receive(
            &data,
            &WsDiagnosticReceiveOptions {
                allow_channel: None,
                format: Some(Rc::new(|message| {
                    Some(format!(
                        "[mexc public] {}",
                        message.as_str().unwrap_or_default()
                    ))
                })),
                redact: None,
            },
        )?;
        Ok(())
    }

    fn route_handlers(
        &self,
        socket: &Rc<BaseWebSocket>,
        key: &str,
        message: &Value,
    ) -> Result<(), ExchangeError> {
        let Some(handlers) = socket.message_handlers.borrow().get(key).cloned() else {
            return Ok(());
        };
        let mut index = 0;
        loop {
            // JS for..of observes an array's live length and current index.
            let handler = handlers.borrow().get(index).cloned();
            let Some(handler) = handler else {
                return Ok(());
            };
            handler(message)?;
            index += 1;
        }
    }

    fn handle_depth(&self, socket: &Rc<BaseWebSocket>, frame: &Value) -> Result<(), ExchangeError> {
        let Some((symbol, handler_key, book)) = handle_depth_frame(socket.store().as_ref(), frame)?
        else {
            return Ok(());
        };
        self.orderbooks.borrow_mut().insert(symbol.clone(), book);
        self.route_handlers(socket, &handler_key, frame)
    }

    fn handle_kline(&self, socket: &Rc<BaseWebSocket>, frame: &Value) -> Result<(), ExchangeError> {
        let key = kline_handler_key(frame)?;
        self.route_handlers(socket, &key, frame)
    }
}

/// Normalize one MEXC depth frame and return the symbol used for its handler key.
pub fn handle_depth_frame(
    store: &dyn Store,
    frame: &Value,
) -> Result<Option<(String, String, OrderBook)>, ExchangeError> {
    let data = frame
        .get("data")
        .filter(|value| safe_cex_num::js_truthy(Some(value)));
    let symbol_value = frame
        .get("symbol")
        .filter(|value| !value.is_null())
        .or_else(|| data.and_then(|row| row.get("symbol")));
    let symbol =
        from_mexc_symbol(symbol_value).map_err(|error| ExchangeError::new(error.message))?;
    let market = store.markets_by_symbol().get(&symbol).cloned();
    let Some(market) = market else {
        return Ok(None);
    };
    let raw = data.unwrap_or(frame);
    let raw = raw.as_object().cloned().unwrap_or_default();
    let book =
        normalize_order_book(&raw, &market).map_err(|error| ExchangeError::new(error.message))?;
    let handler_key = format!("depth.{symbol}");
    Ok(Some((symbol, handler_key, book)))
}

/// Return the exact MEXC callback key selected by a kline frame.
pub fn kline_handler_key(frame: &Value) -> Result<String, ExchangeError> {
    let data = frame.get("data");
    let symbol_value = frame
        .get("symbol")
        .filter(|value| !value.is_null())
        .or_else(|| data.and_then(|row| row.get("symbol")));
    let symbol =
        from_mexc_symbol(symbol_value).map_err(|error| ExchangeError::new(error.message))?;
    let interval = data
        .and_then(|row| {
            row.get("interval")
                .filter(|v| safe_cex_num::js_truthy(Some(v)))
                .or_else(|| row.get("i").filter(|v| safe_cex_num::js_truthy(Some(v))))
        })
        .map(|value| safe_cex_num::js_string(Some(value)))
        .transpose()
        .map_err(|error| ExchangeError::new(error.message))?
        .unwrap_or_default();
    Ok(format!("kline.{symbol}.{interval}"))
}

impl WsProtocol for MexcPublicProtocol {
    fn ping(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        if !socket.is_disposed() {
            socket.mark_ping_sent();
            self.send(socket, &json!({"method":"ping"}))?;
        }
        Ok(())
    }

    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _is_reconnect: bool,
    ) -> Result<(), ExchangeError> {
        if socket.is_disposed() {
            return Ok(());
        }
        let options = socket.emitter().options();
        let network = if options.testnet.unwrap_or(false) {
            "testnet"
        } else {
            "livenet"
        };
        let url = BASE_WS_URL
            .get("public")
            .and_then(|value| value.get(network))
            .and_then(Value::as_str)
            .ok_or_else(|| ExchangeError::new("Missing MEXC public WebSocket URL"))?
            .to_owned();
        let headers = indexmap::IndexMap::from([("User-Agent".into(), "Tealstreet".into())]);
        let options = WebSocketConnectionOptions {
            headers: Some(safe_cex_exchange::websocket::UpgradeHeaders::Static(
                headers,
            )),
            ..Default::default()
        };
        socket.start_websocket(url, vec![], Some(options));
        Ok(())
    }

    fn on_open(&self, socket: Rc<BaseWebSocket>) -> ExchangeFuture<'static, ()> {
        let result = if socket.is_disposed() {
            Ok(())
        } else {
            self.ping(&socket).and_then(|()| self.resubscribe(&socket))
        };
        if let Err(error) = result {
            return Box::pin(async move { Err(error) });
        }
        if !socket.is_disposed() {
            if let Err(error) = socket.emit_ws_reconnected() {
                return Box::pin(async move { Err(error) });
            }
        }
        Box::pin(async { Ok(()) })
    }

    fn on_message(
        &self,
        socket: &Rc<BaseWebSocket>,
        frame: &safe_cex_transport::socket::SocketFrame,
    ) -> Result<(), ExchangeError> {
        on_message(self, socket, frame)
    }
}

/// Route one MEXC public socket frame through the venue handler dispatch.
pub fn on_message(
    protocol: &MexcPublicProtocol,
    socket: &Rc<BaseWebSocket>,
    frame: &safe_cex_transport::socket::SocketFrame,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received()?;
    let json = MexcPublicProtocol::parse_frame(frame).unwrap_or(Value::Null);
    if !safe_cex_num::js_truthy(Some(&json)) {
        return Ok(());
    }
    let raw = match frame {
        safe_cex_transport::socket::SocketFrame::Text(text) => text.clone(),
        safe_cex_transport::socket::SocketFrame::Binary(bytes) => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        safe_cex_transport::socket::SocketFrame::Close { .. } => return Ok(()),
    };
    protocol.emit_receive_diagnostics(socket, &raw, &json)?;
    if ["channel", "msg", "method"]
        .into_iter()
        .any(|key| json.get(key).and_then(Value::as_str) == Some("pong"))
    {
        socket.handle_pong_event(15_000)?;
        return Ok(());
    }
    if json.get("success") == Some(&Value::Bool(false)) {
        let message = [json.get("message"), json.get("msg")]
            .into_iter()
            .find(|value| value.is_some_and(|value| safe_cex_num::js_truthy(Some(value))))
            .map(|value| safe_cex_num::js_string(value))
            .transpose()
            .map_err(|error| ExchangeError::new(error.message))?
            .unwrap_or_else(|| "unknown error".into());
        socket.emitter().log(
            &format!("MEXC public WS subscription failed: {message}"),
            LogSeverity::Warning,
        )?;
        return Ok(());
    }
    let channel = json
        .get("channel")
        .filter(|value| safe_cex_num::js_truthy(Some(value)))
        .map(|value| safe_cex_num::js_string(Some(value)))
        .transpose()
        .map_err(|error| ExchangeError::new(error.message))?
        .unwrap_or_default();
    if channel.is_empty() {
        return Ok(());
    }
    if !is_mexc_control_channel(&channel) {
        socket.record_message_handler_stats(&channel)?;
    }
    match channel.as_str() {
        "push.ticker" => {
            handle_ticker_frame(
                socket.store().as_ref(),
                &json,
                protocol.local_to_utc.as_ref(),
            )?;
            Ok(())
        }
        "push.depth" | "push.depth.full" => protocol.handle_depth(socket, &json),
        "push.kline" => protocol.handle_kline(socket, &json),
        "push.deal" => {
            let symbol = from_mexc_symbol(json.get("symbol"))
                .map_err(|error| ExchangeError::new(error.message))?;
            protocol.route_handlers(socket, &format!("trade.{symbol}"), &json)
        }
        _ => Ok(()),
    }
}

/// Apply the fields written by `MexcPublicWebsocket#handleTicker`.
pub fn handle_ticker_frame(
    store: &dyn Store,
    frame: &Value,
    local_to_utc: &dyn Fn(f64) -> f64,
) -> Result<bool, ExchangeError> {
    let data = frame.get("data").and_then(Value::as_object);
    let symbol_value = frame
        .get("symbol")
        .filter(|value| !value.is_null())
        .or_else(|| data.and_then(|row| row.get("symbol")));
    let symbol =
        from_mexc_symbol(symbol_value).map_err(|error| ExchangeError::new(error.message))?;
    let market = store.markets_by_symbol().get(&symbol).cloned();
    let current = store.tickers_by_symbol().get(&symbol).cloned();
    let (Some(market), Some(_current)) = (market, current) else {
        return Ok(false);
    };
    let mut raw = data.cloned().unwrap_or_default();
    raw.insert("symbol".into(), Value::String(market.id.clone()));
    let next = normalize_ticker(&raw, &market, local_to_utc)
        .map_err(|error| ExchangeError::new(error.message))?;
    let mut patch = ModelPatch::default();
    patch.set_number("bid", next.bid);
    patch.set_number("ask", next.ask);
    patch.set_number("last", next.last);
    patch.set_number("mark", next.mark);
    patch.set_number("index", next.index);
    patch.set_number("percentage", next.percentage);
    patch.set_number("openInterest", next.open_interest);
    patch.set_number("fundingRate", next.funding_rate);
    patch.set_number("fundingTime", next.funding_time);
    patch.set_number("volume", next.volume);
    patch.set_number("quoteVolume", next.quote_volume);
    // TypeScript and Rust both commit before propagating subscriber failures.
    store.update_ticker(&market.id, &patch)?;
    Ok(true)
}
