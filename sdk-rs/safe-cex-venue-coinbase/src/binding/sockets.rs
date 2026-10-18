use super::{
    crypto::CryptoHost,
    error::{exchange_error, failure, from_thrown},
    rt::Host,
    value::{self, to_js},
};
use crate::{
    cdp_signing::{self, CdpSigningReceiver},
    classes::CoinbasePrivateClassHost,
    market_position_reads::ReadMarket,
    order_sync::{PrivateOrderFill, PrivateOrderMarkets, SyncedOrders},
    portfolios_balances::PortfolioFailure,
    private_stream_dispatch::*,
    public_data::PublicData,
    public_market_listeners::*,
    public_market_reads::PublicOHLCVOptionsRoot,
    public_stream_dispatch::*,
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_exchange::{
    js_runtime::{
        promise::{Promise as JsPromise, await_js},
        sockets,
        value::{Js, Thrown, arg, from_json, from_typed, to_json},
    },
    runtime::ExchangeError,
};
use safe_cex_store::StoreUpdate;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

type PublicFailure = Rc<PublicStreamFailure>;
fn public(error: PortfolioFailure) -> PublicFailure {
    let error = exchange_error(&error);
    Rc::new(PublicStreamFailure {
        name: error.name.clone(),
        message: error.message.clone(),
        native: Some(error),
    })
}
fn thrown(error: PublicFailure) -> Thrown {
    match &error.native {
        Some(native) => Thrown::from(native.clone()),
        None => Thrown::error(&error.name, &error.message),
    }
}

#[derive(Clone)]
pub(super) struct SocketHost {
    host: Host,
    socket: Js,
    handles: Rc<RefCell<Vec<(PublicSocket, Js)>>>,
}
impl SocketHost {
    pub(super) fn new(host: Host, is_private: bool) -> Result<Self, PortfolioFailure> {
        let socket = sockets::base_websocket(&host.shared.sockets, host.receiver());
        socket
            .define("ping", Js::function("ping", |_, _| Ok(Js::Undefined)))
            .map_err(from_thrown)?;
        let result = Self {
            host: host.clone(),
            socket: socket.clone(),
            handles: Rc::default(),
        };
        for member in ["connectAndSubscribe", "onOpen", "onMessage"] {
            let host = host.clone();
            socket
                .define(
                    member,
                    Js::function(member, move |_, args| {
                        let class = host
                            .class()
                            .map_err(|error| Thrown::from(exchange_error(&error)))?;
                        if is_private {
                            let class =
                                class.private_websocket.borrow().clone().ok_or_else(|| {
                                    Thrown::error(
                                        "Error",
                                        "Coinbase private socket is not assembled",
                                    )
                                })?;
                            match member {
                                "connectAndSubscribe" => class
                                    .connect_and_subscribe()
                                    .map_err(|e| Thrown::from(exchange_error(&e)))?,
                                "onOpen" => {
                                    let future = class.on_open();
                                    return Ok(Js::Promise(JsPromise::spawn(
                                        &host.runtime(),
                                        Box::pin(async move {
                                            future
                                                .await
                                                .map_err(|e| Thrown::from(exchange_error(&e)))?;
                                            Ok(Js::Undefined)
                                        }),
                                    )));
                                }
                                _ => class
                                    .on_message(&arg(&args, 0).get("data")?.to_display())
                                    .map_err(|e| Thrown::from(exchange_error(&e)))?,
                            }
                        } else {
                            let class =
                                class.public_websocket.borrow().clone().ok_or_else(|| {
                                    Thrown::error(
                                        "Error",
                                        "Coinbase public socket is not assembled",
                                    )
                                })?;
                            match member {
                                "connectAndSubscribe" => {
                                    class.connect_and_subscribe().map_err(thrown)?
                                }
                                "onOpen" => class.on_open().map_err(thrown)?,
                                _ => class
                                    .on_message(&arg(&args, 0).get("data")?.to_display())
                                    .map_err(thrown)?,
                            }
                        }
                        Ok(Js::Undefined)
                    }),
                )
                .map_err(from_thrown)?;
        }
        host.receiver()
            .define(
                if is_private {
                    "privateWebsocket"
                } else {
                    "publicWebsocket"
                },
                socket.clone(),
            )
            .map_err(from_thrown)?;
        (if is_private {
            &host.shared.extra.private_socket
        } else {
            &host.shared.extra.public_socket
        })
        .set(socket)
        .map_err(|_| Host::json_error("Error", "Coinbase socket constructed twice"))?;
        Ok(result)
    }
    fn call(&self, method: &str, args: Vec<Js>) -> Result<Js, PortfolioFailure> {
        self.socket.call_method(method, args).map_err(from_thrown)
    }
    fn open(&self, url: Js) -> Result<PublicSocket, PortfolioFailure> {
        let ws = self.call("createWebsocket", vec![url])?;
        self.socket.set("ws", ws.clone()).map_err(from_thrown)?;
        let frame = Rc::new(RefCell::new(Frame::from_json(
            &json!({"readyState":ws.get("readyState").map_err(from_thrown)?.as_f64().unwrap_or(0.0)}),
        )));
        self.handles.borrow_mut().push((frame.clone(), ws));
        Ok(frame)
    }
    fn handle(&self, frame: &PublicSocket) -> Result<Js, PortfolioFailure> {
        self.handles
            .borrow()
            .iter()
            .find(|(candidate, _)| Rc::ptr_eq(candidate, frame))
            .map(|(_, ws)| ws.clone())
            .ok_or_else(|| Host::json_error("Error", "Unknown Coinbase websocket handle"))
    }
    fn listener(
        &self,
        frame: PublicSocket,
        event: PublicSocketListener,
    ) -> Result<(), PortfolioFailure> {
        let (event, member, state) = match event {
            PublicSocketListener::Open => ("open", "onOpen", Some(1.0)),
            PublicSocketListener::Message => ("message", "onMessage", None),
            PublicSocketListener::Close => ("close", "onClose", Some(3.0)),
        };
        let socket = self.socket.clone();
        let handle = self.handle(&frame)?;
        let callback = Js::function(member, move |_, args| {
            if let Some(state) = state
                && let Frame::Object(fields) = &*frame.borrow()
            {
                fields
                    .borrow_mut()
                    .insert("readyState".encode_utf16().collect(), Frame::Number(state));
            }
            socket.call_method(member, args)
        });
        handle
            .call_method("addEventListener", vec![Js::str(event), callback])
            .map_err(from_thrown)?;
        Ok(())
    }
    fn stringify(&self, payload: Value) -> Result<Js, PortfolioFailure> {
        self.host
            .global("JSON.stringify", vec![from_json(&payload)])
    }
    fn emit(&self, event: &str, payload: Js) -> Result<(), PortfolioFailure> {
        self.host
            .receiver()
            .get("emitter")
            .and_then(|emitter| emitter.call_method("emit", vec![Js::str(event), payload]))
            .map_err(from_thrown)?;
        Ok(())
    }
    fn log_console(&self, severity: &str, args: Vec<Js>) -> Result<(), PortfolioFailure> {
        self.host.global(&format!("console.{severity}"), args)?;
        Ok(())
    }
    fn order_values(
        orders: &SyncedOrders<Frame>,
    ) -> Result<Vec<safe_cex_types::Order>, PortfolioFailure> {
        orders
            .borrow()
            .iter()
            .map(|order| {
                let order = order.borrow();
                let mut fields = order.fields.clone();
                fields.info = (Some(
                    safe_cex_exchange::js_runtime::value::to_retained_input(&to_js(&order.info))
                        .map_err(from_thrown)?,
                ))
                .into();
                Ok(fields)
            })
            .collect()
    }
}
impl PublicStreamHost for SocketHost {
    fn is_disposed(&self) -> bool {
        self.host.base().is_disposed()
    }
    fn testnet(&self) -> bool {
        self.host.shared.extra.options.testnet.unwrap_or(false)
    }
    fn reconnecting_enabled(&self) -> bool {
        self.host.shared.runtime.enable_reconnecting()
    }
    fn create_websocket(&self, url: &'static str) -> Result<PublicSocket, PublicFailure> {
        self.open(Js::str(url)).map_err(public)
    }
    fn add_listener(
        &self,
        frame: PublicSocket,
        event: PublicSocketListener,
    ) -> Result<(), PublicFailure> {
        self.listener(frame, event).map_err(public)
    }
    fn reset_orderbook(&self) -> Result<(), PublicFailure> {
        let class = self.host.class().map_err(public)?;
        if let Some(class) = class.public_websocket.borrow().as_ref() {
            for state in class.listeners.inner_state.borrow().values() {
                let mut state = state.borrow_mut();
                state.updates.clear();
                state.is_snapshot_loaded = false;
            }
        }
        Ok(())
    }
    fn emit_ws_reconnected(&self) -> Result<(), PublicFailure> {
        self.call("emitWsReconnected", vec![])
            .map(|_| ())
            .map_err(public)
    }
    fn markets_symbols(&self) -> Result<Vec<String>, PublicFailure> {
        Ok(self
            .host
            .shared
            .store
            .markets()
            .map_err(|error| public(failure(error.into())))?
            .iter()
            .map(|market| market.symbol.clone())
            .collect())
    }
    fn send_subscription(&self, payload: PublicSubscription) -> Result<(), PublicFailure> {
        let json = self.stringify(json!({"type":payload.r#type,"product_ids":payload.product_ids,"channel":payload.channel})).map_err(public)?;
        self.call("send", vec![json]).map(|_| ()).map_err(public)
    }
    fn track_last_received(&self) -> Result<(), PublicFailure> {
        self.call("trackLastReceived", vec![])
            .map(|_| ())
            .map_err(public)
    }
    fn json_parse(&self, raw: &str) -> Option<Frame> {
        self.host
            .global("JSON.parse", vec![Js::str(raw)])
            .ok()
            .and_then(|value| value::from_js(&value).ok().flatten())
    }
    fn emit_error(&self, message: Option<Frame>) -> Result<(), PublicFailure> {
        self.emit(
            "error",
            message.as_ref().map(to_js).unwrap_or(Js::Undefined),
        )
        .map_err(public)
    }
    fn console_error_text(&self, message: Vec<u16>) -> Result<(), PublicFailure> {
        self.log_console("error", vec![Js::utf16(&message)])
            .map_err(public)
    }
    fn console_error_frame(&self, frame: Frame) -> Result<(), PublicFailure> {
        self.log_console("error", vec![to_js(&frame)])
            .map_err(public)
    }
    fn handle_pong_event(&self) -> Result<(), PublicFailure> {
        self.call("handlePongEvent", vec![])
            .map(|_| ())
            .map_err(public)
    }
    fn record_message_handler_stats(&self, topic: &str) -> Result<(), PublicFailure> {
        self.call("recordMessageHandlerStats", vec![Js::str(topic)])
            .map(|_| ())
            .map_err(public)
    }
}
impl PublicMarketListenerHost for SocketHost {
    fn is_connected(&self) -> bool {
        self.socket
            .get("isConnected")
            .is_ok_and(|value| value.truthy())
    }
    fn public_data_only_option(&self) -> bool {
        self.host
            .shared
            .extra
            .options
            .public_data_only
            .unwrap_or(false)
    }
    fn timeframe_seconds(&self, interval: &str) -> Option<f64> {
        safe_cex_types::TIMEFRAME_TO_SECONDS.get(interval).copied()
    }
    fn daily_candle(&self, symbol: &str) -> Option<PublicData> {
        self.host.daily(symbol)
    }
    fn set_daily_candle(&self, symbol: String, candle: PublicData) -> Result<(), PublicFailure> {
        self.host
            .shared
            .extra
            .daily_candles
            .borrow_mut()
            .insert(symbol, candle);
        Ok(())
    }
    fn utc_end_of_day_ms(&self) -> Result<f64, PublicFailure> {
        Ok(safe_cex_num::js_time_clip(
            (self.host.now() / 86_400_000.0).floor() * 86_400_000.0 + 86_400_000.0 - 1.0,
        ))
    }
    fn fetch_ohlcv_detached(&self, opts: PublicOHLCVOptionsRoot) -> Result<(), PublicFailure> {
        drop(crate::public_market_reads::fetch_ohlcv(
            self.host.public_read_receiver(),
            opts,
        ));
        Ok(())
    }
    fn dayjs_unix(&self, value: Option<Frame>) -> Result<f64, PublicFailure> {
        self.host
            .global("dayjs", value.as_ref().map(to_js).into_iter().collect())
            .and_then(|value| {
                value
                    .call_method("unix", vec![])
                    .and_then(|value| value.to_number())
                    .map_err(from_thrown)
            })
            .map_err(public)
    }
    fn add(&self, left: Option<Frame>, right: f64) -> Result<f64, PublicFailure> {
        self.host
            .global(
                "add",
                vec![
                    left.as_ref().map(to_js).unwrap_or(Js::Undefined),
                    Js::Number(right),
                ],
            )
            .and_then(|value| value.to_number().map_err(from_thrown))
            .map_err(public)
    }
    fn warn(&self, message: &str, value: Option<Frame>) -> Result<(), PublicFailure> {
        self.log_console(
            "warn",
            vec![
                Js::str(message),
                value.as_ref().map(to_js).unwrap_or(Js::Undefined),
            ],
        )
        .map_err(public)
    }
    fn set_timeout(&self, callback: PublicCleanup, delay_ms: f64) -> Result<(), PublicFailure> {
        let callback = Js::function("coinbaseCleanup", move |_, _| {
            callback().map_err(thrown)?;
            Ok(Js::Undefined)
        });
        self.host
            .global("setTimeout", vec![callback, Js::Number(delay_ms)])
            .map(|_| ())
            .map_err(public)
    }
    fn cleanup_message_handler_stats(&self, topic: &str) -> Result<(), PublicFailure> {
        self.call("cleanupMessageHandlerStats", vec![Js::str(topic)])
            .map(|_| ())
            .map_err(public)
    }
    fn nanoid(&self) -> Result<String, PublicFailure> {
        self.host
            .global("nanoid", vec![])
            .map(|value| value.to_display())
            .map_err(public)
    }
    fn update_order_book(
        &self,
        book: PublicListenerBook,
        side: &str,
        level: Rc<RefCell<PublicBookLevel>>,
    ) -> Result<(), PublicFailure> {
        let mut native = native_book(&book);
        let level = level.borrow();
        safe_cex_store::update_order_book::update_order_book(
            &mut native,
            side == "bids",
            safe_cex_types::OrderBookOrders {
                price: level.price,
                amount: level.amount,
                total: level.total,
            },
            true,
        );
        write_book(&book, native);
        Ok(())
    }
    fn sort_order_book(&self, book: PublicListenerBook) -> Result<(), PublicFailure> {
        let mut native = native_book(&book);
        safe_cex_store::orderbook::sort_order_book(&mut native);
        write_book(&book, native);
        Ok(())
    }
    fn calc_order_book_total(&self, book: PublicListenerBook) -> Result<(), PublicFailure> {
        let mut native = native_book(&book);
        safe_cex_store::orderbook::calc_order_book_total(&mut native);
        write_book(&book, native);
        Ok(())
    }
}
pub(super) fn native_book(book: &PublicListenerBook) -> safe_cex_types::OrderBook {
    let book = book.borrow();
    let side = |levels: &PublicBookLevels| {
        levels
            .borrow()
            .iter()
            .map(|level| {
                let level = level.borrow();
                safe_cex_types::OrderBookOrders {
                    price: level.price,
                    amount: level.amount,
                    total: level.total,
                }
            })
            .collect()
    };
    safe_cex_types::OrderBook {
        bids: side(&book.bids),
        asks: side(&book.asks),
    }
}
fn write_book(book: &PublicListenerBook, native: safe_cex_types::OrderBook) {
    let book = book.borrow();
    for (levels, values) in [(&book.bids, native.bids), (&book.asks, native.asks)] {
        // A snapshot can contain several distinct levels at the same price.
        // Consume each retained root once so helper copyback cannot alias them.
        let mut old: Vec<_> = levels.borrow().iter().cloned().map(Some).collect();
        *levels.borrow_mut() = values
            .into_iter()
            .map(|value| {
                let level = old
                    .iter_mut()
                    .find(|level| {
                        level.as_ref().is_some_and(|level| {
                            level.borrow().price.to_bits() == value.price.to_bits()
                        })
                    })
                    .and_then(Option::take)
                    .unwrap_or_else(|| {
                        Rc::new(RefCell::new(PublicBookLevel {
                            price: value.price,
                            amount: value.amount,
                            total: value.total,
                        }))
                    });
                {
                    let mut level = level.borrow_mut();
                    level.amount = value.amount;
                    level.total = value.total;
                }
                level
            })
            .collect();
    }
}
impl PrivateStreamHost for SocketHost {
    fn is_disposed(&self) -> bool {
        self.host.base().is_disposed()
    }
    fn options(&self) -> PrivateOptions {
        let options = self
            .host
            .receiver()
            .get("options")
            .expect("constructed options");
        let remote = options.get("remoteSigner").expect("options data property");
        let remote_signer = if remote.truthy() {
            let host = self.host.clone();
            Some(Rc::new(PrivateRemoteSigner {
                account_name: RefCell::new(
                    remote
                        .get("accountName")
                        .ok()
                        .filter(|value| !value.is_undefined())
                        .and_then(|value| to_json(&value).ok()),
                ),
                sign: Rc::new(move |request| {
                    let request = from_json(
                        &json!({"id":request.id,"type":request.request_type,"exchangeName":request.exchange_name,"accountName":request.account_name,"wsAuth":{"toSign":request.to_sign,"outputFormat":request.output_format,"extraData":{"channel":request.channel}}}),
                    );
                    let invoked = remote
                        .call_method("sign", vec![request])
                        .map_err(from_thrown);
                    let runtime = host.runtime();
                    Ok(host.promise(async move {
                        let result = await_js(&runtime, invoked?).await.map_err(from_thrown)?;
                        if result.is_undefined() {
                            Ok(None)
                        } else {
                            to_json(&result).map(Some).map_err(from_thrown)
                        }
                    }))
                }),
            }))
        } else {
            None
        };
        Rc::new(RefCell::new(PrivateStreamOptions {
            key: Some(Value::String(self.host.shared.extra.options.key.clone())),
            secret: Some(Value::String(self.host.shared.extra.options.secret.clone())),
            remote_signer,
        }))
    }
    fn exchange_name(&self) -> String {
        serde_json::to_value(&self.host.shared.extra.name)
            .expect("name")
            .as_str()
            .expect("name")
            .to_owned()
    }
    fn testnet(&self) -> bool {
        PublicStreamHost::testnet(self)
    }
    fn resolve_private_route(
        &self,
        _exchange_name: String,
        _options: PrivateOptions,
    ) -> Result<PrivateStreamRoute, PortfolioFailure> {
        let route = self
            .host
            .shared
            .runtime
            .transport()
            .ws_routing(
                &self.host.shared.extra.name,
                &(&self.host.shared.extra.options).into(),
            )
            .ws_private;
        Ok(PrivateStreamRoute {
            tunnelled: route.tunnelled,
            host: route.host.map(Value::String),
        })
    }
    fn create_websocket(&self, url: Option<Value>) -> Result<PublicSocket, PortfolioFailure> {
        self.open(url.as_ref().map(from_json).unwrap_or(Js::Undefined))
    }
    fn add_listener(
        &self,
        socket: PublicSocket,
        event: PublicSocketListener,
    ) -> Result<(), PortfolioFailure> {
        self.listener(socket, event)
    }
    fn reset_order_snapshot_assembly(&self) -> Result<(), PortfolioFailure> {
        if let Some(class) = self.host.class()?.private_websocket.borrow().as_ref() {
            class.reset_order_snapshot_assembly();
        }
        Ok(())
    }
    fn close_socket(&self, socket: PublicSocket) -> Result<(), PortfolioFailure> {
        self.handle(&socket)?
            .call_method("close", vec![])
            .map_err(from_thrown)?;
        Ok(())
    }
    fn send_with_log(&self, payload: PrivateSubscription) -> Result<(), PortfolioFailure> {
        let value = self.stringify(
            json!({"type":payload.request_type,"channel":payload.channel,"jwt":payload.jwt}),
        )?;
        self.call("sendWithLog", vec![value])?;
        Ok(())
    }
    fn sign_local(
        &self,
        key: Option<Value>,
        secret: Option<Value>,
    ) -> Promise<Result<Value, PortfolioFailure>> {
        let receiver = Rc::new(CdpSigningReceiver {
            host: CryptoHost {
                runtime: self.host.runtime(),
            },
            cache: self.host.shared.extra.signing_cache.clone(),
            spawn: self.host.shared.extra.spawn.clone(),
        });
        let key = safe_cex_num::js_string(key.as_ref())
            .map_err(|error| Host::json_error(error.name, error.message));
        let secret = safe_cex_num::js_string(secret.as_ref())
            .map_err(|error| Host::json_error(error.name, error.message));
        let result = key.and_then(|key| Ok(cdp_signing::sign(receiver, key, secret?, None)));
        self.host
            .promise(async move { result?.await.map(Value::String) })
    }
    fn track_last_received(&self) -> Result<(), PortfolioFailure> {
        self.call("trackLastReceived", vec![]).map(|_| ())
    }
    fn emit_ws_diagnostic_receive(&self, raw: &str) -> Result<(), PortfolioFailure> {
        self.call("emitWsDiagnosticReceive", vec![Js::str(raw)])
            .map(|_| ())
    }
    fn json_parse(&self, raw: &str) -> Result<Frame, PortfolioFailure> {
        value::from_js(&self.host.global("JSON.parse", vec![Js::str(raw)])?)?
            .ok_or_else(|| Host::json_error("SyntaxError", "Undefined parsed frame"))
    }
    fn mark_private_user_stream_auth_failed(
        &self,
        message: Option<Frame>,
    ) -> Result<(), PortfolioFailure> {
        let message = message.as_ref().map(to_js).map(|value| value.to_display());
        self.host
            .shared
            .extra
            .health
            .mark_private_user_stream_auth_failed(
                self.host.shared.store.as_ref(),
                message.as_deref(),
                |message, severity| self.host.base().log(message, severity).map_err(failure),
                |message| {
                    self.host
                        .base()
                        .emit_args(
                            "warning",
                            &[
                                Js::str(message),
                                Js::object(vec![("forceToast", Js::Bool(true))]),
                            ],
                        )
                        .map_err(failure)
                },
            )
    }
    fn mark_private_user_stream_message_received(&self) -> Result<(), PortfolioFailure> {
        self.host
            .class()?
            .last_private_user_stream_message_at
            .set(self.host.now());
        self.host
            .shared
            .extra
            .health
            .mark_private_user_stream_message_received(
                self.host.shared.store.as_ref(),
                self.host.now(),
            )
            .map_err(|error| Rc::new(error.into()))
    }
    fn emit_and_log_error(&self, message: Option<Frame>) -> Result<(), PortfolioFailure> {
        self.call(
            "emitAndLogError",
            vec![message.as_ref().map(to_js).unwrap_or(Js::Undefined)],
        )
        .map(|_| ())
    }
    fn handle_user_event_orders(&self, event: Frame) -> Result<(), PortfolioFailure> {
        if let Some(class) = self.host.class()?.private_websocket.borrow().as_ref() {
            class.handle_user_event_orders(event)?;
        }
        Ok(())
    }
    fn handle_pong_event(&self) -> Result<(), PortfolioFailure> {
        self.call("handlePongEvent", vec![]).map(|_| ())
    }
}
impl CoinbasePrivateClassHost for SocketHost {
    fn private_order_markets(&self) -> Result<PrivateOrderMarkets, PortfolioFailure> {
        Ok(Rc::new(RefCell::new(
            self.host
                .shared
                .store
                .markets_by_id()
                .map_err(|error| failure(error.into()))?
                .iter()
                .map(|(key, value)| (key.clone(), Rc::new(RefCell::new(value.clone()))))
                .collect(),
        )))
    }
    fn resolve_private_order_market(
        &self,
        product: String,
        markets: PrivateOrderMarkets,
    ) -> Result<Option<ReadMarket>, PortfolioFailure> {
        let markets = markets.borrow();
        let keys = markets
            .keys()
            .map(|key| (key.clone(), Value::String(key.clone())))
            .collect();
        Ok(
            crate::utils::resolve_coinbase_usd_equivalent_market(&product, &keys)
                .and_then(Value::as_str)
                .and_then(|key| markets.get(key))
                .cloned(),
        )
    }
    fn private_order_console_error(
        &self,
        message: &str,
        value: &str,
    ) -> Result<(), PortfolioFailure> {
        self.log_console("error", vec![Js::str(message), Js::str(value)])
    }
    fn private_order_now_ms(&self) -> Result<f64, PortfolioFailure> {
        Ok(safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms())
    }
    fn private_orders_loaded(&self) -> Result<Frame, PortfolioFailure> {
        value::from_js(
            &from_typed(
                &self
                    .host
                    .shared
                    .store
                    .loaded()
                    .map_err(|error| failure(error.into()))?,
            )
            .map_err(from_thrown)?,
        )?
        .ok_or_else(|| failure(ExchangeError::new("Missing loaded state")))
    }
    fn publish_private_order_snapshot(
        &self,
        orders: SyncedOrders<Frame>,
        loaded: Frame,
    ) -> Result<(), PortfolioFailure> {
        self.host
            .shared
            .store
            .update(
                StoreUpdate {
                    orders: Some(Self::order_values(&orders)?),
                    loaded: Some(
                        safe_cex_exchange::js_runtime::value::to_typed(&to_js(&loaded))
                            .map_err(from_thrown)?,
                    ),
                    ..Default::default()
                },
                false,
            )
            .map_err(|error| Rc::new(error.into()))
    }
    fn add_private_orders(&self, orders: SyncedOrders<Frame>) -> Result<(), PortfolioFailure> {
        self.host
            .shared
            .store
            .add_or_update_orders(Self::order_values(&orders)?)
            .map_err(|error| Rc::new(error.into()))
    }
    fn emit_private_order_fill(
        &self,
        fill: PrivateOrderFill<Frame>,
    ) -> Result<(), PortfolioFailure> {
        self.emit(
            "fill",
            Js::object(vec![
                ("id", Js::str(&fill.id)),
                ("side", from_typed(&fill.side).map_err(from_thrown)?),
                ("symbol", Js::str(&fill.symbol)),
                ("price", to_js(&fill.price)),
                ("amount", Js::Number(fill.amount)),
            ]),
        )
    }
}
