//! Per-socket host: the four Kraken socket classes' bodies run here, with the
//! socket object as receiver and the venue-owned slots the traits borrow.
use super::{
    host::{lossy, units},
    promise::await_js,
    rt::Rt,
    value::{Js, Thrown, from_json, from_typed, to_json},
};
use crate::{
    book_listeners::{Feed, Frame, Key, Listener, ListenerHost},
    lifecycle::{ConnectionHost, ConnectionSocket, SocketCallback},
    message_dispatch::MessageHost,
    orderbooks::{Book, BookHost, Side},
    private_auth::{FuturesAuthState, SpotAuthState},
    private_control::{AuthHost, RefreshCallback, RefreshHost, SignRequest, Subscription},
    public_subscriptions::{PublicSubscription, PublicSubscriptionHost, Topic},
    spot_dispatch::SpotDispatchHost,
    stream_updates::{CandleUpdateHost, Lookup, NumericChanges, StreamValues, TickerUpdateHost},
};
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_num::{JsJsonValue, TableHit};
use safe_cex_types::{Candle, retained_json::RetainedJsonInput};
use std::{
    any::Any,
    cell::RefCell,
    future::Future,
    rc::{Rc, Weak},
};

fn ex(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}

/// The host-owned state of one socket object.
pub(crate) struct SocketHost {
    pub rt: Rt,
    pub conn: RefCell<Option<Rc<WsHandle>>>,
    pub futures_auth: RefCell<FuturesAuthState>,
    pub spot_auth: RefCell<SpotAuthState>,
    pub self_ref: RefCell<Weak<SocketHost>>,
}

impl SocketHost {
    pub(crate) fn new(rt: Rt) -> Rc<Self> {
        let host = Rc::new(Self {
            rt,
            conn: RefCell::new(None),
            futures_auth: RefCell::default(),
            spot_auth: RefCell::default(),
            self_ref: RefCell::new(Weak::new()),
        });
        *host.self_ref.borrow_mut() = Rc::downgrade(&host);
        host
    }
    fn this(&self) -> &Js {
        &self.rt.this
    }
    fn parent(&self) -> Result<Js, ExchangeError> {
        self.this().get("parent").map_err(ex)
    }
    fn store(&self) -> Result<Js, ExchangeError> {
        self.this().get("store").map_err(ex)
    }
    fn call(&self, member: &str, args: Vec<Js>) -> Result<Js, ExchangeError> {
        self.this().call_method(member, args).map_err(ex)
    }
    fn console(&self, severity: &str, message: String) {
        self.rt.shared.runtime.console(severity, &message);
    }
    fn handlers_map(&self, feed: Feed) -> Result<Js, ExchangeError> {
        self.this()
            .get(match feed {
                Feed::Book => "orderBookHandlers",
                Feed::Ohlc => "ohlcvHandlers",
            })
            .map_err(ex)
    }
}

/// The socket host registered for a socket receiver.
pub(crate) fn host_of(rt: &Rt) -> Result<Rc<SocketHost>, Thrown> {
    rt.shared
        .extra
        .sockets
        .borrow()
        .iter()
        .find(|s| s.rt.this.same_reference(&rt.this))
        .cloned()
        .ok_or_else(|| Thrown::error("Error", "Kraken socket host is not registered"))
}

fn length_of(value: &Js) -> Result<Option<usize>, ExchangeError> {
    if value.is_nullish() {
        return Ok(None);
    }
    let length = value.get("length").map_err(ex)?;
    Ok(length.as_f64().map(|n| n as usize))
}

fn json_text(value: &JsJsonValue) -> Result<String, ExchangeError> {
    safe_cex_num::js_json_stringify(value)
        .map_err(|e| ExchangeError::new(e.message))?
        .ok_or_else(|| ExchangeError::new("Undefined WebSocket frame"))
}

fn topic(t: &[u16]) -> RetainedJsonInput {
    RetainedJsonInput::Text(t.to_vec())
}

fn interval_json(interval: &TableHit<f64>) -> Option<RetainedJsonInput> {
    match interval {
        TableHit::Mapped(n) => Some(RetainedJsonInput::Float(*n)),
        // A stock-prototype member is a function: JSON.stringify omits the field.
        TableHit::Inherited(_) => None,
        TableHit::Missing => Some(RetainedJsonInput::Float(1.0)),
    }
}

/// The source `ws` handle from `createWebsocket`.
pub(crate) struct WsHandle(pub Js);

impl ConnectionSocket for WsHandle {
    fn ready_state(&self) -> Result<f64, ExchangeError> {
        self.0
            .get("readyState")
            .and_then(|v| v.to_number())
            .map_err(ex)
    }
    fn close(&self) -> Result<(), ExchangeError> {
        self.0.call_method("close", vec![]).map_err(ex)?;
        Ok(())
    }
    fn add_event_listener(
        &self,
        event: &str,
        callback: SocketCallback,
    ) -> Result<(), ExchangeError> {
        let listener = Js::function("listener", move |_, a| {
            let event = super::value::arg(&a, 0);
            callback(&event as &dyn Any).map_err(Thrown::from)?;
            Ok(Js::Undefined)
        });
        self.0
            .call_method("addEventListener", vec![Js::str(event), listener])
            .map_err(ex)?;
        Ok(())
    }
}

fn member_callback(host: &SocketHost, member: &'static str) -> SocketCallback {
    let this = host.this().clone();
    Rc::new(move |event: &dyn Any| {
        let event = event.downcast_ref::<Js>().cloned().unwrap_or(Js::Undefined);
        let result = this.call_method(member, vec![event]).map_err(ex)?;
        if let Js::Promise(p) = result
            && let Some(Err(e)) = p.result()
        {
            return Err(ex(e));
        }
        Ok(())
    })
}

impl ConnectionHost for SocketHost {
    type Socket = WsHandle;
    fn is_disposed(&self) -> bool {
        self.this().get("isDisposed").is_ok_and(|v| v.truthy())
    }
    fn testnet(&self) -> bool {
        self.parent()
            .and_then(|p| p.get("options").map_err(ex))
            .and_then(|o| o.get("testnet").map_err(ex))
            .is_ok_and(|v| v.truthy())
    }
    fn socket(&self) -> &RefCell<Option<Rc<WsHandle>>> {
        &self.conn
    }
    fn create_websocket(&self, url: &str) -> Result<Rc<WsHandle>, ExchangeError> {
        let ws = self.call("createWebsocket", vec![Js::str(url)])?;
        self.this().set("ws", ws.clone()).map_err(ex)?;
        Ok(Rc::new(WsHandle(ws)))
    }
    fn on_open(&self) -> SocketCallback {
        member_callback(self, "onOpen")
    }
    fn on_message(&self) -> SocketCallback {
        member_callback(self, "onMessage")
    }
    fn on_close(&self) -> Result<(), ExchangeError> {
        self.call("onClose", vec![])?;
        Ok(())
    }
}

fn keys_of(value: &Js) -> Vec<Topic> {
    value.own_keys().into_iter().map(|k| k.to_vec()).collect()
}

impl PublicSubscriptionHost for SocketHost {
    fn is_disposed(&self) -> bool {
        ConnectionHost::is_disposed(self)
    }
    fn ticker_product_ids(&self) -> Result<Vec<Topic>, ExchangeError> {
        let ids = self.call("getTickerProductIds", vec![])?;
        ids.array_values()?
            .unwrap_or_default()
            .iter()
            .map(|v| v.to_js_string().map(|s| s.to_vec()).map_err(ex))
            .collect()
    }
    fn market_symbols(&self) -> Result<Vec<Topic>, ExchangeError> {
        let markets = self
            .parent()?
            .get("store")
            .and_then(|s| s.get("markets"))
            .map_err(ex)?;
        markets
            .array_values()?
            .unwrap_or_default()
            .iter()
            .map(|m| {
                m.get("symbol")
                    .and_then(|s| s.to_js_string())
                    .map(|s| s.to_vec())
                    .map_err(ex)
            })
            .collect()
    }
    fn book_keys(&self) -> Result<Vec<Topic>, ExchangeError> {
        Ok(keys_of(&self.handlers_map(Feed::Book)?))
    }
    fn book_length(&self, key: &[u16]) -> Result<Option<usize>, ExchangeError> {
        length_of(&self.handlers_map(Feed::Book)?.get_key(key).map_err(ex)?)
    }
    fn ohlcv_keys(&self) -> Result<Vec<Topic>, ExchangeError> {
        Ok(keys_of(&self.handlers_map(Feed::Ohlc)?))
    }
    fn ohlcv_length(&self, key: &[u16]) -> Result<Option<usize>, ExchangeError> {
        length_of(&self.handlers_map(Feed::Ohlc)?.get_key(key).map_err(ex)?)
    }
    fn stringify_and_send(&self, frame: PublicSubscription) -> Result<(), ExchangeError> {
        let value = match frame {
            PublicSubscription::Futures {
                event,
                feed,
                product_ids,
            } => RetainedJsonInput::Object(vec![
                (
                    "event".encode_utf16().collect(),
                    RetainedJsonInput::Text(event.encode_utf16().collect()),
                ),
                (
                    "feed".encode_utf16().collect(),
                    RetainedJsonInput::Text(feed.encode_utf16().collect()),
                ),
                (
                    "product_ids".encode_utf16().collect(),
                    RetainedJsonInput::Array(product_ids.iter().map(|t| topic(t)).collect()),
                ),
            ]),
            PublicSubscription::SpotTicker {
                method,
                channel,
                symbols,
                event_trigger,
            } => spot_frame(
                method,
                vec![
                    (
                        "channel".encode_utf16().collect(),
                        RetainedJsonInput::Text(channel.encode_utf16().collect()),
                    ),
                    (
                        "symbol".encode_utf16().collect(),
                        RetainedJsonInput::Array(symbols.iter().map(|t| topic(t)).collect()),
                    ),
                    (
                        "event_trigger".encode_utf16().collect(),
                        RetainedJsonInput::Text(event_trigger.encode_utf16().collect()),
                    ),
                ],
            ),
            PublicSubscription::SpotBook {
                method,
                channel,
                symbols,
                depth,
            } => spot_frame(
                method,
                vec![
                    (
                        "channel".encode_utf16().collect(),
                        RetainedJsonInput::Text(channel.encode_utf16().collect()),
                    ),
                    (
                        "symbol".encode_utf16().collect(),
                        RetainedJsonInput::Array(symbols.iter().map(|t| topic(t)).collect()),
                    ),
                    (
                        "depth".encode_utf16().collect(),
                        RetainedJsonInput::Float(depth),
                    ),
                ],
            ),
            PublicSubscription::SpotOhlc {
                method,
                channel,
                symbols,
                interval,
            } => {
                let mut params = vec![
                    (
                        "channel".encode_utf16().collect(),
                        RetainedJsonInput::Text(channel.encode_utf16().collect()),
                    ),
                    (
                        "symbol".encode_utf16().collect(),
                        RetainedJsonInput::Array(symbols.iter().map(|t| topic(t)).collect()),
                    ),
                ];
                if let Some(interval) = interval_json(&interval) {
                    params.push(("interval".encode_utf16().collect(), interval));
                }
                spot_frame(method, params)
            }
        };
        self.call(
            "send",
            vec![Js::str(
                &safe_cex_num::js_retained_json_stringify(&value)
                    .map_err(|e| ExchangeError::new(e.message))?
                    .ok_or_else(|| ExchangeError::new("Undefined WebSocket frame"))?,
            )],
        )?;
        Ok(())
    }
    // ts: `console.error(message, symbol, e)`.
    fn book_resubscribe_error(
        &self,
        message: &'static str,
        symbol: &[u16],
        error: ExchangeError,
    ) -> Result<(), ExchangeError> {
        self.console(
            "error",
            format!("{message} {} {}", lossy(symbol), error.message),
        );
        Ok(())
    }
}

fn spot_frame(method: &str, params: Vec<(Vec<u16>, RetainedJsonInput)>) -> RetainedJsonInput {
    RetainedJsonInput::Object(vec![
        (
            "method".encode_utf16().collect(),
            RetainedJsonInput::Text(method.encode_utf16().collect()),
        ),
        (
            "params".encode_utf16().collect(),
            RetainedJsonInput::Object(params),
        ),
    ])
}

/// `JSON.parse(typeof data === 'string' ? data : data?.toString?.() || '{}')`;
/// the tolerant Futures private reader yields null for an unparsable frame.
fn parse_frame(data: &Js, tolerant: bool) -> Result<Js, ExchangeError> {
    let text = match data {
        Js::String(s) => s.to_vec(),
        Js::Undefined | Js::Null => Vec::new(),
        other => other.to_js_string().map_err(ex)?.to_vec(),
    };
    let text = if text.is_empty() && !tolerant {
        "{}".encode_utf16().collect()
    } else {
        text
    };
    match safe_cex_types::retained_json::RetainedJsonInput::parse_json(&text) {
        Ok(value) => Ok(safe_cex_exchange::js_runtime::value::from_retained_input(
            value,
        )),
        Err(_) if tolerant => Ok(Js::Null),
        Err(e) => {
            let mut error = ExchangeError::new(e.to_string());
            error.name = "SyntaxError".into();
            Err(error)
        }
    }
}

impl MessageHost for SocketHost {
    type Value = Js;
    fn disposed(&self) -> Result<bool, ExchangeError> {
        Ok(ConnectionHost::is_disposed(self))
    }
    fn track_received(&self) -> Result<(), ExchangeError> {
        self.call("trackLastReceived", vec![])?;
        Ok(())
    }
    fn parse_data(&self, data: &Js, tolerant: bool) -> Result<Rc<Js>, ExchangeError> {
        parse_frame(data, tolerant).map(Rc::new)
    }
    fn field(
        &self,
        value: &Js,
        key: &'static str,
        optional: bool,
    ) -> Result<Rc<Js>, ExchangeError> {
        if optional && value.is_nullish() {
            return Ok(Rc::new(Js::Undefined));
        }
        value.get(key).map(Rc::new).map_err(ex)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn equals(&self, value: &Js, text: &'static str) -> bool {
        value.strict_equals(&Js::str(text))
    }
    fn string(&self, value: &Js) -> Result<Vec<u16>, ExchangeError> {
        value.to_js_string().map(|s| s.to_vec()).map_err(ex)
    }
    fn stringify(&self, value: &Js) -> Result<Vec<u16>, ExchangeError> {
        self.rt
            .global("JSON.stringify")
            .and_then(|stringify| stringify.call(Js::Undefined, vec![value.clone()]))
            .and_then(|json| json.to_js_string())
            .map(|json| json.to_vec())
            .map_err(ex)
    }
    fn includes(&self, value: &Js, text: &'static str) -> Result<bool, ExchangeError> {
        if value.is_nullish() {
            return Ok(false);
        }
        value
            .call_method("includes", vec![Js::str(text)])
            .map(|v| v.truthy())
            .map_err(ex)
    }
    fn stats(&self, feed: Rc<Js>) -> Result<(), ExchangeError> {
        self.call("recordMessageHandlerStats", vec![(*feed).clone()])?;
        Ok(())
    }
    fn invoke(&self, method: &'static str, message: Rc<Js>) -> Result<(), ExchangeError> {
        self.call(method, vec![(*message).clone()])?;
        Ok(())
    }
    fn perform_auth(&self) -> Result<(), ExchangeError> {
        self.call("performAuth", vec![])?;
        Ok(())
    }
    fn pending_challenge(&self, value: Vec<u16>) -> Result<(), ExchangeError> {
        self.this()
            .set("pendingChallenge", Js::utf16(&value))
            .map_err(ex)
    }
    fn waiting_challenge(&self, value: bool) -> Result<(), ExchangeError> {
        self.futures_auth.borrow_mut().waiting_challenge = value;
        self.this()
            .set("waitingChallenge", Js::Bool(value))
            .map_err(ex)
    }
    fn authenticated(&self, value: bool) -> Result<(), ExchangeError> {
        self.spot_auth.borrow_mut().authenticated = value;
        self.this()
            .set("authenticated", Js::Bool(value))
            .map_err(ex)
    }
    fn clear_token(&self) -> Result<(), ExchangeError> {
        self.spot_auth.borrow_mut().token = JsJsonValue::Undefined;
        self.this().set("wsToken", Js::Undefined).map_err(ex)
    }
    fn log(&self, message: Vec<u16>, severity: LogSeverity) -> Result<(), ExchangeError> {
        let severity = serde_json::to_value(severity)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "info".into());
        self.parent()?
            .call_method("log", vec![Js::utf16(&message), Js::str(&severity)])
            .map_err(ex)?;
        Ok(())
    }
    fn console_subscription(
        &self,
        prefix: &'static str,
        value: Rc<Js>,
    ) -> Result<(), ExchangeError> {
        self.console("error", format!("{prefix} {}", value.to_display()));
        Ok(())
    }
    fn console_parse(
        &self,
        prefix: &'static str,
        error: ExchangeError,
        data: Rc<Js>,
    ) -> Result<(), ExchangeError> {
        self.console(
            "error",
            format!(
                "{prefix} {} Raw data: {}",
                error.js_error_string(),
                data.to_display()
            ),
        );
        Ok(())
    }
}

impl StreamValues for SocketHost {
    type Value = Js;
    type Items = std::vec::IntoIter<Result<Rc<Js>, ExchangeError>>;
    fn field(
        &self,
        value: &Js,
        key: &'static str,
        lookup: Lookup,
    ) -> Result<Rc<Js>, ExchangeError> {
        match lookup {
            Lookup::Direct => value.get(key).map(Rc::new).map_err(ex),
            Lookup::GetKey => {
                if value.is_object() {
                    for name in [
                        key.to_owned(),
                        safe_cex_num::lodash_snake_case(key),
                        safe_cex_num::lodash_camel_case(key),
                    ] {
                        if value.has_own(&name) {
                            return value.get(&name).map(Rc::new).map_err(ex);
                        }
                    }
                }
                Ok(Rc::new(if value.is_nullish() {
                    Js::Undefined
                } else {
                    value.get(key).map_err(ex)?
                }))
            }
        }
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn parse_float(&self, value: &Js) -> Result<f64, ExchangeError> {
        value.parse_float().map_err(ex)
    }
    // ts: `if (!Array.isArray(msg?.data)) return;` then `for (const item of msg.data)`.
    fn array_data(&self, message: &Js) -> Result<Option<Self::Items>, ExchangeError> {
        let data = if message.is_nullish() {
            Js::Undefined
        } else {
            message.get("data").map_err(ex)?
        };
        Ok(data.array_values()?.map(|items| {
            items
                .into_iter()
                .map(|v| Ok(Rc::new(v)))
                .collect::<Vec<_>>()
                .into_iter()
        }))
    }
}

impl TickerUpdateHost for SocketHost {
    type Ticker = Js;
    fn ticker_for(&self, symbol: &Js) -> Result<Option<Rc<Js>>, ExchangeError> {
        let key = symbol.to_js_string().map_err(ex)?;
        let ticker = self
            .parent()?
            .get("store")
            .and_then(|s| s.get("tickersBySymbol"))
            .and_then(|t| t.get_key(&key))
            .map_err(ex)?;
        Ok(ticker.truthy().then(|| Rc::new(ticker)))
    }
    fn update_ticker(&self, ticker: Rc<Js>, changes: NumericChanges) -> Result<(), ExchangeError> {
        let patch = Js::object(changes.iter().map(|(k, v)| (*k, Js::Number(*v))).collect());
        self.store()?
            .call_method("updateTicker", vec![(*ticker).clone(), patch])
            .map_err(ex)?;
        Ok(())
    }
}

fn handler_list(map: &Js, key: &[u16]) -> Result<Js, ExchangeError> {
    let list = map.get_key(key).map_err(ex)?;
    Ok(if list.is_nullish() {
        Js::array(vec![])
    } else {
        list
    })
}

fn call_each(
    handlers: &Js,
    mut build: impl FnMut() -> Result<Js, ExchangeError>,
) -> Result<(), ExchangeError> {
    for callback in handlers.array_values()?.unwrap_or_default() {
        callback.call(Js::Undefined, vec![build()?]).map_err(ex)?;
    }
    Ok(())
}

impl CandleUpdateHost for SocketHost {
    type Handlers = Js;
    fn strict_number_eq(&self, value: &Js, number: f64) -> bool {
        value.as_f64() == Some(number)
    }
    fn string_units(&self, value: &Js) -> Result<Vec<u16>, ExchangeError> {
        value.to_js_string().map(|s| s.to_vec()).map_err(ex)
    }
    fn handlers(&self, key: &[u16]) -> Result<Rc<Js>, ExchangeError> {
        handler_list(&self.handlers_map(Feed::Ohlc)?, key).map(Rc::new)
    }
    fn handler_length(&self, handlers: &Js) -> Result<usize, ExchangeError> {
        Ok(length_of(handlers)?.unwrap_or(0))
    }
    fn record_stats(&self, feed: &'static str) -> Result<(), ExchangeError> {
        self.call("recordMessageHandlerStats", vec![Js::str(feed)])?;
        Ok(())
    }
    fn date_ms(&self, value: &Js) -> Result<f64, ExchangeError> {
        let primitive = safe_cex_exchange::js_runtime::ops::to_primitive(value).map_err(ex)?;
        let n = match primitive {
            Js::String(s) => safe_cex_num::js_date_parse_utf16(&s, |n| n),
            other => other.to_number().map_err(ex)?,
        };
        Ok(safe_cex_num::js_time_clip(n))
    }
    fn candle_for_each(
        &self,
        handlers: Rc<Js>,
        candle: Rc<RefCell<Candle>>,
    ) -> Result<(), ExchangeError> {
        let value = from_typed(&*candle.borrow()).map_err(ex)?;
        call_each(&handlers, || Ok(value.clone()))
    }
}

fn levels_js(levels: &crate::orderbooks::Levels) -> Result<Js, ExchangeError> {
    let out = levels
        .borrow()
        .iter()
        .map(|l| from_typed(&*l.borrow()).map_err(ex))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Js::array(out))
}

impl BookHost for SocketHost {
    type Handlers = Js;
    type Cache = Js;
    type Map = Js;
    fn handlers(&self, symbol: &Js) -> Result<Rc<Js>, ExchangeError> {
        let key = symbol.to_js_string().map_err(ex)?;
        handler_list(&self.handlers_map(Feed::Book)?, &key).map(Rc::new)
    }
    fn handler_length(&self, handlers: &Js) -> Result<usize, ExchangeError> {
        Ok(length_of(handlers)?.unwrap_or(0))
    }
    fn stats(&self, feed: &'static str) -> Result<(), ExchangeError> {
        self.call("recordMessageHandlerStats", vec![Js::str(feed)])?;
        Ok(())
    }
    // ts: `v(msg, 'bids') || []` iterated with for-of.
    fn iterable_or_empty(&self, value: Rc<Js>) -> Result<Self::Items, ExchangeError> {
        let items = if value.truthy() {
            value.iterable_values().map_err(ex)?
        } else {
            vec![]
        };
        Ok(items
            .into_iter()
            .map(|v| Ok(Rc::new(v)))
            .collect::<Vec<_>>()
            .into_iter())
    }
    fn reset_cache(&self, symbol: &Js) -> Result<(), ExchangeError> {
        let key = symbol.to_js_string().map_err(ex)?;
        self.this()
            .get("orderBookCache")
            .and_then(|c| {
                c.set_key(
                    &key,
                    Js::object(vec![("bids", Js::new_map()), ("asks", Js::new_map())]),
                )
            })
            .map_err(ex)
    }
    fn cache(&self, symbol: &Js) -> Result<Option<Rc<Js>>, ExchangeError> {
        let key = symbol.to_js_string().map_err(ex)?;
        let cache = self
            .this()
            .get("orderBookCache")
            .and_then(|c| c.get_key(&key))
            .map_err(ex)?;
        Ok(cache.truthy().then(|| Rc::new(cache)))
    }
    fn required_side(&self, symbol: &Js, side: Side) -> Result<Rc<Js>, ExchangeError> {
        let key = symbol.to_js_string().map_err(ex)?;
        self.this()
            .get("orderBookCache")
            .and_then(|c| c.get_key(&key))
            .and_then(|c| c.get(side_name(side)))
            .map(Rc::new)
            .map_err(ex)
    }
    fn set(&self, map: Rc<Js>, price: f64, quantity: f64) -> Result<(), ExchangeError> {
        map.call_method("set", vec![Js::Number(price), Js::Number(quantity)])
            .map_err(ex)?;
        Ok(())
    }
    fn delete(&self, map: Rc<Js>, price: f64) -> Result<(), ExchangeError> {
        map.call_method("delete", vec![Js::Number(price)])
            .map_err(ex)?;
        Ok(())
    }
    fn is_buy(&self, value: &Js) -> bool {
        value.strict_equals(&Js::str("buy"))
    }
    // ts: `this.parent.store.markets.find((m) => m.symbol === symbol)?.contractSize || 1`.
    fn contract_size_or_one(&self, symbol: &Js) -> Result<Rc<Js>, ExchangeError> {
        let markets = self
            .parent()?
            .get("store")
            .and_then(|s| s.get("markets"))
            .map_err(ex)?;
        for market in markets.array_values()?.unwrap_or_default() {
            if market.get("symbol").map_err(ex)?.strict_equals(symbol) {
                let size = market.get("contractSize").map_err(ex)?;
                return Ok(Rc::new(if size.truthy() { size } else { Js::Number(1.0) }));
            }
        }
        Ok(Rc::new(Js::Number(1.0)))
    }
    fn sorted_entries(&self, cache: &Js, side: Side) -> Result<Vec<(f64, f64)>, ExchangeError> {
        let map = cache.get(side_name(side)).map_err(ex)?;
        let entries = map.call_method("entries", vec![]).map_err(ex)?;
        let mut out = entries
            .iterable_values()
            .map_err(ex)?
            .iter()
            .map(|pair| {
                let price = pair.get("0").and_then(|v| v.to_number()).map_err(ex)?;
                let amount = pair.get("1").and_then(|v| v.to_number()).map_err(ex)?;
                Ok((price, amount))
            })
            .collect::<Result<Vec<_>, ExchangeError>>()?;
        out.sort_by(|a, b| {
            let d = match side {
                Side::Bids => b.0 - a.0,
                Side::Asks => a.0 - b.0,
            };
            d.partial_cmp(&0.0).unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(out)
    }
    fn multiply(&self, amount: f64, contract_size: &Js) -> Result<f64, ExchangeError> {
        Ok(amount * contract_size.to_number().map_err(ex)?)
    }
    fn emit(&self, symbol: Rc<Js>) -> Result<(), ExchangeError> {
        self.call("emitOrderBook", vec![(*symbol).clone()])?;
        Ok(())
    }
    fn for_each(
        &self,
        handlers: Rc<Js>,
        mut build: impl FnMut() -> Rc<RefCell<Book>>,
    ) -> Result<(), ExchangeError> {
        call_each(&handlers, || {
            let book = build();
            let book = book.borrow();
            Ok(Js::object(vec![
                ("bids", levels_js(&book.bids)?),
                ("asks", levels_js(&book.asks)?),
            ]))
        })
    }
}

fn side_name(side: Side) -> &'static str {
    match side {
        Side::Bids => "bids",
        Side::Asks => "asks",
    }
}

fn key_units(key: &Key<Js>) -> Result<Vec<u16>, ExchangeError> {
    match key {
        Key::Value(v) => v.to_js_string().map(|s| s.to_vec()).map_err(ex),
        Key::Units(u) => Ok(u.clone()),
    }
}

fn frame_json(frame: &Frame<Js>) -> Result<RetainedJsonInput, ExchangeError> {
    let symbols = |values: &Vec<Rc<Js>>| -> Result<RetainedJsonInput, ExchangeError> {
        Ok(RetainedJsonInput::Array(
            values
                .iter()
                .map(|v| v.to_js_string().map(|s| topic(&s)).map_err(ex))
                .collect::<Result<_, _>>()?,
        ))
    };
    Ok(match frame {
        Frame::Futures {
            event,
            feed,
            products,
        } => RetainedJsonInput::Object(vec![
            (
                "event".encode_utf16().collect(),
                RetainedJsonInput::Text((*event).encode_utf16().collect()),
            ),
            (
                "feed".encode_utf16().collect(),
                RetainedJsonInput::Text((*feed).encode_utf16().collect()),
            ),
            ("product_ids".encode_utf16().collect(), symbols(products)?),
        ]),
        Frame::SpotBook {
            method,
            channel,
            symbols: s,
            depth,
        } => spot_frame(
            method,
            vec![
                (
                    "channel".encode_utf16().collect(),
                    RetainedJsonInput::Text((*channel).encode_utf16().collect()),
                ),
                ("symbol".encode_utf16().collect(), symbols(s)?),
                (
                    "depth".encode_utf16().collect(),
                    RetainedJsonInput::Float(*depth),
                ),
            ],
        ),
        Frame::SpotOhlc {
            method,
            channel,
            symbols: s,
            interval,
        } => {
            let mut params = vec![
                (
                    "channel".encode_utf16().collect(),
                    RetainedJsonInput::Text((*channel).encode_utf16().collect()),
                ),
                ("symbol".encode_utf16().collect(), symbols(s)?),
            ];
            if let Some(interval) = interval_json(interval) {
                params.push(("interval".encode_utf16().collect(), interval));
            }
            spot_frame(method, params)
        }
    })
}

impl ListenerHost for SocketHost {
    type Callback = Js;
    fn string_units(&self, value: &Js) -> Result<Vec<u16>, ExchangeError> {
        value.to_js_string().map(|s| s.to_vec()).map_err(ex)
    }
    // ts: `INTERVAL[timeframe] || 1` over the Spot interval table.
    fn interval(&self, value: &Js) -> Result<TableHit<f64>, ExchangeError> {
        let key = lossy(&value.to_js_string().map_err(ex)?);
        Ok(
            match safe_cex_num::js_table_lookup(crate::spot_types::INTERVAL, &key) {
                TableHit::Mapped(v) if *v != 0.0 && !v.is_nan() => TableHit::Mapped(*v),
                TableHit::Inherited(member) => TableHit::Inherited(member),
                _ => TableHit::Mapped(1.0),
            },
        )
    }
    // ts: `handlers[key] = [...(handlers[key] || []), callback]`.
    fn append(&self, feed: Feed, key: &Key<Js>, callback: Rc<Js>) -> Result<(), ExchangeError> {
        let map = self.handlers_map(feed)?;
        let key = key_units(key)?;
        let mut list = handler_list(&map, &key)?
            .array_values()?
            .unwrap_or_default();
        list.push((*callback).clone());
        map.set_key(&key, Js::array(list)).map_err(ex)
    }
    fn filter(&self, feed: Feed, key: &Key<Js>, callback: Rc<Js>) -> Result<(), ExchangeError> {
        let map = self.handlers_map(feed)?;
        let key = key_units(key)?;
        let list: Vec<Js> = handler_list(&map, &key)?
            .array_values()?
            .unwrap_or_default()
            .into_iter()
            .filter(|cb| !cb.same_reference(&callback))
            .collect();
        map.set_key(&key, Js::array(list)).map_err(ex)
    }
    fn clear_idle(&self, feed: Feed) -> Result<(), ExchangeError> {
        let (name, map) = match feed {
            Feed::Book => ("book", self.handlers_map(Feed::Book)?),
            Feed::Ohlc => ("ohlc", self.handlers_map(Feed::Ohlc)?),
        };
        self.call("clearFeedStatsIfIdle", vec![Js::str(name), map])?;
        Ok(())
    }
    fn current_length(&self, feed: Feed, key: &Key<Js>) -> Result<Option<usize>, ExchangeError> {
        let key = key_units(key)?;
        length_of(&self.handlers_map(feed)?.get_key(&key).map_err(ex)?)
    }
    fn connected(&self) -> Result<bool, ExchangeError> {
        Ok(self.this().get("isConnected").map_err(ex)?.truthy())
    }
    fn send(&self, frame: Rc<RefCell<Frame<Js>>>) -> Result<(), ExchangeError> {
        let text = safe_cex_num::js_retained_json_stringify(&frame_json(&frame.borrow())?)
            .map_err(|e| ExchangeError::new(e.message))?
            .ok_or_else(|| ExchangeError::new("Undefined WebSocket frame"))?;
        self.call("send", vec![Js::str(&text)])?;
        Ok(())
    }
    fn send_error(&self, prefix: &'static str, error: ExchangeError) -> Result<(), ExchangeError> {
        self.console("error", format!("{prefix} {}", error.message));
        Ok(())
    }
    fn schedule(&self, poll: Rc<Listener<Self>>, delay: u32) -> Result<(), ExchangeError> {
        let fire: Rc<dyn Fn() -> Result<(), Thrown>> =
            Rc::new(move || poll.poll().map_err(Thrown::from));
        self.rt.set_timeout(fire, f64::from(delay));
        Ok(())
    }
}

impl SpotDispatchHost for SocketHost {
    type Message = Js;
    type Item = Js;
    type Symbol = Js;
    type Items = std::vec::IntoIter<Result<Rc<Js>, ExchangeError>>;
    fn array_data(&self, message: &Js) -> Result<Option<Self::Items>, ExchangeError> {
        StreamValues::array_data(self, message)
    }
    fn symbol(&self, item: &Js) -> Result<Rc<Js>, ExchangeError> {
        item.get("symbol").map(Rc::new).map_err(ex)
    }
    fn symbol_truthy(&self, symbol: &Js) -> bool {
        symbol.truthy()
    }
    fn is_snapshot(&self, message: &Js) -> Result<bool, ExchangeError> {
        Ok(message
            .get("type")
            .map_err(ex)?
            .strict_equals(&Js::str("snapshot")))
    }
    fn handle_snapshot(&self, symbol: Rc<Js>, item: Rc<Js>) -> Result<(), ExchangeError> {
        self.call(
            "handleBookSnapshot",
            vec![(*symbol).clone(), (*item).clone()],
        )?;
        Ok(())
    }
    fn handle_delta(&self, symbol: Rc<Js>, item: Rc<Js>) -> Result<(), ExchangeError> {
        self.call("handleBookDelta", vec![(*symbol).clone(), (*item).clone()])?;
        Ok(())
    }
    // ts: `this.parent.refreshBalance()` — not awaited.
    fn refresh_balance(&self) -> Result<(), ExchangeError> {
        self.parent()?
            .call_method("refreshBalance", vec![])
            .map_err(ex)?;
        Ok(())
    }
}

impl AuthHost for SocketHost {
    type Signer = Js;
    fn read(&self, slot: &'static str) -> Result<Rc<Js>, ExchangeError> {
        let value = match slot {
            "pending" => self.this().get("pendingChallenge").map_err(ex)?,
            "original" => self.this().get("originalChallenge").map_err(ex)?,
            "signed" => self.this().get("signedChallenge").map_err(ex)?,
            "remote" => {
                if self.rt.base().signers.remote_signer.is_some() {
                    // Shared hosted-account construction injects the signer and
                    // accountName into the live parent options, after typed input.
                    self.parent()?
                        .get("options")
                        .and_then(|options| options.get("remoteSigner"))
                        .map_err(ex)?
                } else {
                    Js::Undefined
                }
            }
            other => self
                .parent()?
                .get("options")
                .and_then(|o| o.get(other))
                .map_err(ex)?,
        };
        Ok(Rc::new(value))
    }
    fn write(&self, slot: &'static str, value: Option<Rc<Js>>) -> Result<(), ExchangeError> {
        let name = match slot {
            "pending" => "pendingChallenge",
            "original" => "originalChallenge",
            "signed" => "signedChallenge",
            other => other,
        };
        self.this()
            .set(name, value.map(|v| (*v).clone()).unwrap_or(Js::Undefined))
            .map_err(ex)
    }
    fn signer(&self, value: Rc<Js>) -> Result<Rc<Js>, ExchangeError> {
        Ok(value)
    }
    fn nanoid(&self) -> Result<Rc<Js>, ExchangeError> {
        Ok(Rc::new(Js::str(&self.rt.shared.runtime.unique_id())))
    }
    fn sign(
        &self,
        _signer: Rc<Js>,
        request: SignRequest<Js>,
    ) -> Result<impl Future<Output = Result<Rc<Js>, ExchangeError>>, ExchangeError> {
        let signer = self
            .rt
            .base()
            .signers
            .remote_signer
            .clone()
            .ok_or_else(|| ExchangeError::new("Kraken remote signer is unavailable"))?;
        let raw = |v: serde_json::Value| serde_json::value::to_raw_value(&v).ok();
        let ws = safe_cex_types::ws_auth::WsAuthRequest {
            id: Some(serde_json::Value::String(request.id.to_display())),
            ws_auth: Some(safe_cex_types::ws_auth::WsAuth {
                to_sign: Some(request.to_sign.to_display()),
                output_format: Some(serde_json::Value::String(request.output_format.into())),
                extra_data: raw(serde_json::json!({ "challenge": request.challenge.to_display() })),
            }),
            request_type: raw(serde_json::Value::String(request.kind.into())),
        };
        let account_name =
            (!request.account_name.is_nullish()).then(|| request.account_name.to_display());
        Ok(async move {
            let response = signer
                .sign(safe_cex_transport::signing::RemoteSignerRequest::WsAuth {
                    exchange_name: safe_cex_types::ExchangeName::Kraken,
                    account_name,
                    request: ws,
                })
                .await
                .map_err(|e| ExchangeError::new(e.to_string()))?;
            let value = match response {
                safe_cex_transport::signing::RemoteSignerResponse::WsAuth(r) => {
                    serde_json::to_value(r).map_err(|e| ExchangeError::new(e.to_string()))?
                }
                safe_cex_transport::signing::RemoteSignerResponse::Http(_) => {
                    return Err(ExchangeError::new(
                        "Remote signer answered WS auth with HTTP",
                    ));
                }
            };
            Ok(Rc::new(from_json(&value)))
        })
    }
    // ts: `createHash('sha256').update(pendingChallenge, 'utf8').digest()`.
    fn hash_challenge(
        &self,
        challenge: impl FnOnce() -> Result<Rc<Js>, ExchangeError>,
    ) -> Result<Rc<Js>, ExchangeError> {
        use sha2::Digest;
        let challenge = challenge()?;
        let text = lossy(&challenge.to_js_string().map_err(ex)?);
        Ok(Rc::new(super::host::buffer(&sha2::Sha256::digest(
            text.as_bytes(),
        ))))
    }
    fn decode_secret(&self, secret: Rc<Js>) -> Result<Rc<Js>, ExchangeError> {
        let Js::String(s) = &*secret else {
            let mut error = ExchangeError::new(
                "The first argument must be of type string or an instance of Buffer, ArrayBuffer, or Array or an Array-like Object.",
            );
            error.name = "TypeError".into();
            return Err(error);
        };
        Ok(Rc::new(super::host::buffer(&super::host::node_base64(
            &lossy(s),
        ))))
    }
    fn hmac_signature(&self, secret: Rc<Js>, hash: Rc<Js>) -> Result<Rc<Js>, ExchangeError> {
        use base64::Engine;
        use hmac::Mac;
        let mut mac = hmac::Hmac::<sha2::Sha512>::new_from_slice(
            &super::host::bytes_of(&secret).map_err(ex)?,
        )
        .map_err(|e| ExchangeError::new(e.to_string()))?;
        mac.update(&super::host::bytes_of(&hash).map_err(ex)?);
        Ok(Rc::new(Js::str(
            &base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes()),
        )))
    }
    fn emit_error(&self, message: Vec<u16>) -> Result<(), ExchangeError> {
        self.parent()?
            .call_method("emitAndLogError", vec![Js::utf16(&message)])
            .map_err(ex)?;
        Ok(())
    }
    // ts: `${e?.message || e}`.
    fn error_text(&self, error: &ExchangeError) -> Result<Vec<u16>, ExchangeError> {
        Ok(units(&if error.message.is_empty() {
            error.js_error_string()
        } else {
            error.message.clone()
        }))
    }
    fn subscribe(&self) -> Result<(), ExchangeError> {
        self.call("subscribePrivateFeeds", vec![])?;
        Ok(())
    }
    fn send_subscription(&self, s: Subscription<Js>) -> Result<(), ExchangeError> {
        let value = JsJsonValue::Object(vec![
            ("event".into(), JsJsonValue::String(s.event.into())),
            ("api_key".into(), js_json(&s.key)?),
            ("original_challenge".into(), js_json(&s.original)?),
            ("signed_challenge".into(), js_json(&s.signed)?),
            ("feed".into(), JsJsonValue::String(s.feed.into())),
        ]);
        self.call("send", vec![Js::str(&json_text(&value)?)])?;
        Ok(())
    }
}

fn js_json(value: &Js) -> Result<JsJsonValue, ExchangeError> {
    Ok(match value {
        Js::Undefined => JsJsonValue::Undefined,
        Js::Null => JsJsonValue::Null,
        Js::Bool(b) => JsJsonValue::Bool(*b),
        Js::Number(n) => JsJsonValue::Number(*n),
        Js::String(s) => JsJsonValue::String(lossy(s)),
        other => {
            let json = to_json(other).map_err(ex)?;
            safe_cex_num::JsJsonValue::from(json)
        }
    })
}

impl RefreshHost for SocketHost {
    type Value = Js;
    type Timer = Js;
    fn timeout(&self) -> Result<Rc<Js>, ExchangeError> {
        self.this()
            .get("ordersThrottleTimeout")
            .map(Rc::new)
            .map_err(ex)
    }
    fn timer_truthy(&self, timer: &Js) -> bool {
        timer.truthy()
    }
    fn clear_timeout(&self, timer: Rc<Js>) -> Result<(), ExchangeError> {
        self.rt.clear_timeout(&timer);
        Ok(())
    }
    // Futures captures the `fetchThrottledOrders` method; Spot an arrow over fetchAndSetOrders.
    fn schedule(&self, callback: RefreshCallback, delay: u32) -> Result<Rc<Js>, ExchangeError> {
        let fire: Rc<dyn Fn() -> Result<(), Thrown>> = match callback {
            RefreshCallback::Futures => {
                let method = self.this().get("fetchThrottledOrders").map_err(ex)?;
                let this = self.this().clone();
                Rc::new(move || {
                    method.call(this.clone(), vec![])?;
                    Ok(())
                })
            }
            RefreshCallback::Spot => {
                let weak = self.self_ref.borrow().clone();
                let rt = self.rt.clone();
                Rc::new(move || {
                    let Some(host) = weak.upgrade() else {
                        return Ok(());
                    };
                    let body = Box::pin(async move {
                        crate::private_control::spot_refresh_callback(host.as_ref())
                            .await
                            .map_err(Thrown::from)?;
                        Ok(Js::Undefined)
                    });
                    drop(rt.spawn(body));
                    Ok(())
                })
            }
        };
        Ok(Rc::new(self.rt.set_timeout(fire, f64::from(delay))))
    }
    fn set_timeout(&self, timer: Option<Rc<Js>>) -> Result<(), ExchangeError> {
        self.this()
            .set(
                "ordersThrottleTimeout",
                timer.map(|t| (*t).clone()).unwrap_or(Js::Undefined),
            )
            .map_err(ex)
    }
    fn disposed(&self) -> Result<bool, ExchangeError> {
        Ok(ConnectionHost::is_disposed(self))
    }
    fn fetch_orders(
        &self,
    ) -> Result<impl Future<Output = Result<Rc<Js>, ExchangeError>>, ExchangeError> {
        let promise = self
            .parent()?
            .call_method("fetchOrders", vec![])
            .map_err(ex)?;
        Ok(async move {
            await_js(&self.rt.exchange_runtime(), promise)
                .await
                .map(Rc::new)
                .map_err(ex)
        })
    }
    fn current_loaded(&self) -> Result<Rc<Js>, ExchangeError> {
        self.store()?.get("loaded").map(Rc::new).map_err(ex)
    }
    // ts: `this.store.update({ orders, loaded: { ...loaded, orders: true } })`.
    fn update_orders(
        &self,
        orders: Rc<Js>,
        loaded: Rc<Js>,
        orders_loaded: bool,
    ) -> Result<(), ExchangeError> {
        let loaded = loaded.spread_object().map_err(ex)?;
        loaded.set("orders", Js::Bool(orders_loaded)).map_err(ex)?;
        self.store()?
            .call_method(
                "update",
                vec![Js::object(vec![
                    ("orders", (*orders).clone()),
                    ("loaded", loaded),
                ])],
            )
            .map_err(ex)?;
        Ok(())
    }
    fn fetch_and_set_orders(
        &self,
    ) -> Result<impl Future<Output = Result<(), ExchangeError>>, ExchangeError> {
        let promise = self
            .parent()?
            .call_method("fetchAndSetOrders", vec![])
            .map_err(ex)?;
        Ok(async move {
            await_js(&self.rt.exchange_runtime(), promise)
                .await
                .map_err(ex)?;
            Ok(())
        })
    }
}
