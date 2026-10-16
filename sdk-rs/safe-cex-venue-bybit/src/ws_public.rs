//! Public protocol operations; dialing remains owned by BaseWebSocket.
use safe_cex_exchange::{
    runtime::{ExchangeError, ExchangeRuntime},
    websocket::{BaseWebSocket, HandlerList, WsDiagnosticReceiveOptions, WsDiagnosticSendOptions},
};
use serde_json::{Map, Value, json};
use std::rc::Rc;

pub fn connect_and_subscribe(
    socket: &Rc<BaseWebSocket>,
    runtime: &dyn safe_cex_exchange::websocket::WsRuntime,
    category: &str,
    is_demo: bool,
    direct_override: Option<&str>,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    let parent = socket.emitter();
    let exchange = serde_json::from_value(Value::String(parent.name().into()))
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    let route = runtime
        .transport()
        .ws_routing(&exchange, &parent.options().into())
        .ws_public;
    let network = if parent.options().testnet == Some(true) {
        "testnet"
    } else {
        "livenet"
    };
    let direct = direct_override
        .or_else(|| crate::types::BASE_WS_URL["public"][category][network].as_str())
        .ok_or_else(|| ExchangeError::new("Unknown Bybit public websocket category"))?;
    let primary = route
        .host
        .as_deref()
        .filter(|host| route.tunnelled && !host.is_empty())
        .unwrap_or(direct);
    let base = socket
        .direct_fallback
        .borrow_mut()
        .resolve_url(primary, &[route.fallback.as_deref()])
        .to_owned();
    if Some(base.as_str()) == route.fallback.as_deref() {
        parent.emit("grafana-info", "Using tealstreet bybit public linear proxy")?;
    }
    let options = connection_options(&parent.options(), is_demo);
    // Set every input before starting: an inline host can open during this call.
    socket.start_websocket(
        format!("{base}?requestId={}", runtime.unique_id()),
        vec![],
        Some(options),
    );
    Ok(())
}

pub(crate) fn connection_options(
    options: &safe_cex_types::ExchangeOptions,
    is_demo: bool,
) -> safe_cex_exchange::websocket::WebSocketConnectionOptions {
    use safe_cex_exchange::websocket::{UpgradeHeaders, WebSocketConnectionOptions};
    let headers = (!is_demo && options.bybit_site_id.is_some()).then(|| {
        UpgradeHeaders::Static(
            [(
                String::from("x-site-id"),
                crate::site::BYBIT_BRAZIL_SITE_ID.into(),
            )]
            .into_iter()
            .collect(),
        )
    });
    WebSocketConnectionOptions {
        headers,
        ..Default::default()
    }
}

pub async fn subscribe(
    socket: &Rc<BaseWebSocket>,
    topics: &Map<String, Value>,
    runtime: &dyn ExchangeRuntime,
) -> Result<(), ExchangeError> {
    let topics: Vec<Value> = crate::api_request::ordered_entries(topics)
        .into_iter()
        .flat_map(|(_, value)| match value {
            Value::Array(values) => values.clone(),
            value => vec![value.clone()],
        })
        .collect();
    for topics in topics.chunks(10) {
        socket.send(&json!({"op":"subscribe","args":topics}).to_string())?;
        // Source awaits even after the final chunk, including a one-chunk send.
        runtime.sleep(10).await?;
    }
    Ok(())
}

pub fn ping(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if !socket.is_disposed() {
        socket.mark_ping_sent();
        socket.send(r#"{"op":"ping"}"#)?;
    }
    Ok(())
}

pub fn send_ohlcv_diagnostic_payload(
    socket: &Rc<BaseWebSocket>,
    op: &str,
    topics: &[String],
) -> Result<(), ExchangeError> {
    let message = json!({"op":op,"args":topics}).to_string();
    socket.send(&message)?;
    socket.emit_ws_diagnostic_send(&message, &WsDiagnosticSendOptions::default())?;
    Ok(())
}

pub fn emit_ohlcv_diagnostic_receive(
    socket: &BaseWebSocket,
    data: &str,
) -> Result<(), ExchangeError> {
    socket.emit_ws_diagnostic_receive(
        &Value::String(data.into()),
        &WsDiagnosticReceiveOptions {
            allow_channel: Some(Rc::new(|frame| {
                Ok(frame.is_object()
                    && (frame
                        .get("topic")
                        .and_then(Value::as_str)
                        .is_some_and(|topic| {
                            topic.starts_with("kline.") || topic.starts_with("publicTrade.")
                        })
                        || matches!(
                            frame.get("op").and_then(Value::as_str),
                            Some("subscribe" | "unsubscribe")
                        )))
            })),
            ..Default::default()
        },
    )
}

fn run_handlers(handlers: &HandlerList, value: &Value) -> Result<(), ExchangeError> {
    let mut index = 0;
    loop {
        // Source captures the array but observes changes to its length/elements.
        // Release the borrow before callbacks: they may mutate either handler list.
        let callback = handlers.borrow().get(index).cloned();
        let Some(callback) = callback else { break };
        callback(value)?;
        index += 1;
    }
    Ok(())
}

fn parse_frame(data: &str) -> Option<Value> {
    fn round_numbers(value: &mut Value) -> Option<()> {
        match value {
            Value::Number(number) => {
                let n = number.as_f64().filter(|n| n.is_finite())?;
                let text = if n == 0.0 && n.is_sign_negative() {
                    "-0".into()
                } else {
                    safe_cex_num::js_number_string(n)
                };
                *number = text.parse().ok()?;
            }
            Value::Array(values) => {
                for value in values {
                    round_numbers(value)?;
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    round_numbers(value)?;
                }
            }
            _ => {}
        }
        Some(())
    }
    let mut value: Value = serde_json::from_str(data).ok()?;
    round_numbers(&mut value)?;
    Some(value)
}

pub fn on_message(socket: &Rc<BaseWebSocket>, data: &str) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received()?;
    socket.direct_fallback.borrow_mut().on_message();
    emit_ohlcv_diagnostic_receive(socket, data)?;
    if let Some(index) = data.find("\"topic\":\"") {
        let start = index + 9;
        let Some(end) = data[start..].find('"').filter(|end| *end > 0) else {
            return Ok(());
        };
        let full_topic = &data[start..start + end];
        let prefix_key = full_topic
            .find('.')
            .filter(|index| *index > 0)
            .map(|index| &full_topic[..index]);
        let (exact, prefix) = {
            let handlers = socket.message_handlers.borrow();
            (
                handlers.get(full_topic).cloned(),
                prefix_key.and_then(|key| handlers.get(key).cloned()),
            )
        };
        let has_exact = exact.as_ref().is_some_and(|h| !h.borrow().is_empty());
        let has_prefix = prefix.as_ref().is_some_and(|h| !h.borrow().is_empty());
        if !has_exact && !has_prefix {
            return Ok(());
        }
        socket.record_message_handler_stats(if has_exact {
            full_topic
        } else {
            prefix_key.expect("prefix matched")
        })?;
        let Some(value) = parse_frame(data) else {
            return Ok(());
        };
        // A literal topic marker cannot occur in a falsy valid JSON primitive.
        if has_exact {
            run_handlers(exact.as_ref().unwrap(), &value)?;
        }
        if has_prefix {
            run_handlers(prefix.as_ref().unwrap(), &value)?;
        }
        return Ok(());
    }
    if data.contains("pong") {
        let handlers = socket.message_handlers.borrow().get("pong").cloned();
        if let Some(handlers) = handlers.filter(|handlers| !handlers.borrow().is_empty()) {
            socket.record_message_handler_stats("pong")?;
            run_handlers(&handlers, &json!({}))?;
        }
    }
    Ok(())
}

#[derive(Debug)]
pub enum TickerEventError {
    Source(String),
    Store(safe_cex_store::StoreMutationError),
}
impl std::fmt::Display for TickerEventError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Source(message) => f.write_str(message),
            Self::Store(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for TickerEventError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(_) => None,
            Self::Store(error) => Some(error),
        }
    }
}

// ts: BybitPublicWebsocket.handleTickersEvent
// Shared by linear/inverse; spot sets a present funding rate to zero and omits
// fundingTime. Keep the source truthiness checks before fallible parseFloat.
pub fn handle_tickers_event(
    store: &dyn safe_cex_store::Store,
    frame: &Value,
    is_spot: bool,
) -> Result<(), TickerEventError> {
    if frame.is_null() {
        return Err(TickerEventError::Source(
            "Cannot read properties of null (reading 'data')".into(),
        ));
    }
    let data = &frame["data"];
    let symbol = &data["symbol"];
    if !crate::api::truthy(symbol) {
        return Ok(());
    }
    let symbol = safe_cex_num::js_string(Some(symbol))
        .map_err(|error| TickerEventError::Source(error.message.to_owned()))?;
    let Some(id) = store
        .tickers_by_symbol()
        .get(&symbol)
        .map(|ticker| ticker.id.clone())
    else {
        return Ok(());
    };
    let mut update = safe_cex_store::model_patch::ModelPatch::default();
    for (source, target, multiplier) in [
        ("bid1Price", "bid", 1.),
        ("ask1Price", "ask", 1.),
        ("lastPrice", "last", 1.),
        ("markPrice", "mark", 1.),
        ("indexPrice", "index", 1.),
        ("price24hPcnt", "percentage", 100.),
        ("openInterest", "openInterest", 1.),
        ("fundingRate", "fundingRate", 1.),
        ("nextFundingTime", "fundingTime", 1.),
        ("volume24h", "volume", 1.),
        ("turnover24h", "quoteVolume", 1.),
    ] {
        let value = &data[source];
        if !crate::api::truthy(value) || (is_spot && source == "nextFundingTime") {
            continue;
        }
        let number = if is_spot && source == "fundingRate" {
            0.
        } else {
            crate::exchange_state::parse_float(Some(value))
                .map_err(|message| TickerEventError::Source(message.into()))?
                * multiplier
        };
        // Native patches preserve both presence and the binary64 value, including
        // NaN/Infinity; JSON would collapse these to null before the Store saw them.
        update.set_number(target, number);
    }
    let updated: Result<(), safe_cex_store::StoreMutationError> = store.update_ticker(&id, &update);
    updated.map_err(TickerEventError::Store)?;
    Ok(())
}

#[cfg(test)]
mod tests;

/// BybitPublicWebsocket for the `bybit` connection: the linear socket, and the
/// inverse socket when inverse markets are enabled.
pub(crate) struct PublicProtocol {
    pub parent: std::rc::Weak<crate::exchange::BybitExchange>,
    pub runtime: Rc<dyn safe_cex_exchange::websocket::WsRuntime>,
    pub category: &'static str,
}
impl PublicProtocol {
    // ts: BybitPublicWebsocket constructor `messageHandlers` — tickers and pong.
    pub(crate) fn register_handlers(socket: &Rc<BaseWebSocket>) {
        let store_socket = Rc::downgrade(socket);
        let tickers: safe_cex_exchange::websocket::MessageHandler = Rc::new(move |json| {
            let Some(socket) = store_socket.upgrade() else {
                return Ok(());
            };
            handle_tickers_event(socket.store().as_ref(), json, false)
                .map_err(|error| ExchangeError::new(error.to_string()))
        });
        let pong_socket = Rc::downgrade(socket);
        let pong: safe_cex_exchange::websocket::MessageHandler = Rc::new(move |_| {
            match pong_socket.upgrade() {
                Some(socket) => socket.handle_pong_event(5_000),
                None => Ok(()),
            }
        });
        let mut handlers = socket.message_handlers.borrow_mut();
        handlers.insert(
            "tickers".into(),
            Rc::new(std::cell::RefCell::new(vec![tickers])),
        );
        handlers.insert("pong".into(), Rc::new(std::cell::RefCell::new(vec![pong])));
    }
}
impl safe_cex_exchange::websocket::WsProtocol for PublicProtocol {
    fn ping(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        ping(socket)
    }
    fn connect_and_subscribe(
        &self,
        socket: Rc<BaseWebSocket>,
        _is_reconnect: bool,
    ) -> Result<(), ExchangeError> {
        let Some(parent) = self.parent.upgrade() else {
            return Ok(());
        };
        connect_and_subscribe(
            &socket,
            self.runtime.as_ref(),
            self.category,
            parent.base.is_demo,
            None,
        )
    }
    // ts: BybitPublicWebsocket.onOpen — one tickers topic per market of this category.
    fn on_open(&self, socket: Rc<BaseWebSocket>) -> safe_cex_exchange::runtime::ExchangeFuture<'static, ()> {
        let result = (|| -> Result<(), ExchangeError> {
            if socket.is_disposed() {
                return Ok(());
            }
            let linear = self.category == "linear";
            let topics: Vec<String> = socket
                .store()
                .markets()
                .iter()
                .filter(|m| {
                    m.option.is_none() && ((m.quote == "USDT" || m.quote == "USDC") == linear)
                })
                .map(|m| format!("tickers.{}", m.symbol))
                .collect();
            // subscribe(): the first chunk goes out now, each later one 10ms after the last.
            let mut frames = topics
                .chunks(10)
                .map(|chunk| json!({"op": "subscribe", "args": chunk}).to_string())
                .collect::<Vec<_>>()
                .into_iter();
            if let Some(first) = frames.next() {
                socket.send(&first)?;
                let rest: Vec<String> = frames.collect();
                let runtime = self.runtime.clone();
                let sender = Rc::downgrade(&socket);
                socket.start_message_operation(async move {
                    runtime.sleep(10).await?;
                    for frame in rest {
                        let Some(socket) = sender.upgrade() else {
                            return Ok(());
                        };
                        socket.send(&frame)?;
                        runtime.sleep(10).await?;
                    }
                    Ok(())
                });
            }
            ping(&socket)?;
            if self.runtime.enable_reconnecting() {
                socket.emit_ws_reconnected()?;
            }
            Ok(())
        })();
        Box::pin(async move { result })
    }
    fn on_message(
        &self,
        socket: &Rc<BaseWebSocket>,
        frame: &safe_cex_transport::socket::SocketFrame,
    ) -> Result<(), ExchangeError> {
        match frame {
            safe_cex_transport::socket::SocketFrame::Text(text) => on_message(socket, text),
            _ => Ok(()),
        }
    }
}
