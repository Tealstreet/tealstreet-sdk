//! Connect existing source topic/lifecycle bodies to the concrete core socket.
use crate::{
    exchange::BybitExchange, public_data_recovery as primary, public_ticker_topics as ticker,
    public_ws_lifecycle as lifecycle,
};
use safe_cex_exchange::{
    exchange::BaseExchange,
    js_runtime::value::Js,
    runtime::{ExchangeError, ExchangeFuture, Listener},
    socket_lifecycle::SocketSlot,
    websocket::{BaseWebSocket, WsRuntime},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

type Topics = Rc<RefCell<Value>>;

pub(super) struct PublicTopics {
    parent: Weak<BybitExchange>,
    runtime: Rc<dyn WsRuntime>,
    category: &'static str,
    active_symbols: bool,
    current: Topics,
    streams: super::streams::Topics,
    active_listener: Option<String>,
    markets_listener: RefCell<Option<String>>,
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn current(topics: &Topics) -> Result<Vec<String>, ExchangeError> {
    let value = ticker::get_current_ticker_topics(|| Ok::<_, ExchangeError>(Some(topics.clone())))?;
    Ok(strings(&value.borrow()))
}
fn set(topics: &Topics, value: Topics) -> Result<(), ExchangeError> {
    *topics.borrow_mut() = value.borrow().clone();
    Ok(())
}
fn sync(parent: &BybitExchange, topics: &Topics) -> Result<(), ExchangeError> {
    ticker::sync_ticker_topics(
        || current(topics),
        || {
            ticker::get_desired_ticker_topics(|| {
                let rt = parent
                    .js
                    .get()
                    .ok_or_else(|| ExchangeError::new("Bybit receiver unavailable"))?;
                let result = rt
                    .this
                    .call_method("getTickerSymbolsForPublicStream", vec![])
                    .and_then(|value| crate::binding::value::to_typed(&value));
                result.map_err(|error| ExchangeError::new(error.message()))
            })
        },
        |value| set(topics, value),
    )?;
    Ok(())
}
fn native_markets(values: &[Js]) -> Result<primary::Markets, ExchangeError> {
    let mut markets = Vec::new();
    for value in values {
        if value.get("option")?.truthy() {
            continue;
        }
        let string = |key| -> Result<String, ExchangeError> {
            Ok(value
                .get(key)?
                .as_str()
                .map(|units| String::from_utf16_lossy(units))
                .unwrap_or_default())
        };
        markets.push(Rc::new(RefCell::new(primary::Market {
            symbol: string("symbol")?,
            quote: string("quote")?,
        })));
    }
    Ok(Rc::new(RefCell::new(markets)))
}
fn send(weak: &Weak<BaseWebSocket>, frame: &str) -> Result<(), ExchangeError> {
    match weak.upgrade() {
        Some(socket) => socket.send(frame),
        None => Ok(()),
    }
}

impl PublicTopics {
    pub(super) fn new(
        parent: Weak<BybitExchange>,
        runtime: Rc<dyn WsRuntime>,
        category: &'static str,
        base: &BaseExchange,
    ) -> Self {
        let current = Rc::new(RefCell::new(json!([])));
        let streams = parent
            .upgrade()
            .expect("the exchange owns public socket construction")
            .subscription_topics(category);
        // Dedicated Spot and Inverse register this before their first dial.
        // The primary optional inverse socket remains category-wide.
        let dedicated_inverse =
            category == "inverse" && base.name == safe_cex_types::ExchangeName::Bybitinverse;
        let active_symbols = category == "spot" || dedicated_inverse;
        let active_listener = if active_symbols {
            let parent = parent.clone();
            let topics = current.clone();
            Some(base.on(
                "activeTickerSymbols:changed",
                Rc::new(move |_, _| {
                    let Some(parent) = parent.upgrade() else {
                        return Ok(());
                    };
                    let Some(socket) = parent.base.get_socket(if dedicated_inverse {
                        SocketSlot::PublicSecondary
                    } else {
                        SocketSlot::Public
                    }) else {
                        return Ok(());
                    };
                    ticker::handle_active_ticker_symbols_changed(
                        || socket.is_disposed(),
                        || current_topics(&topics),
                        || {
                            sync(&parent, &topics)?;
                            current_topics(&topics)
                        },
                        || socket.is_connected(),
                        |frame| socket.send(frame),
                    )
                }),
            ))
        } else {
            None
        };
        Self {
            parent,
            runtime,
            category,
            active_symbols,
            current,
            streams,
            active_listener,
            markets_listener: RefCell::new(None),
        }
    }
    fn start_subscribe(&self, socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let topics = self.current.borrow().clone();
        let sender = Rc::downgrade(socket);
        let runtime = self.runtime.clone();
        let mut subscriptions = serde_json::Map::from_iter([("tickers".into(), topics)]);
        subscriptions.extend(self.streams.borrow().clone());
        // The canonical body sends the first chunk before yielding its async tail.
        let operation = lifecycle::subscribe(
            move || Ok(subscriptions),
            move |frame| send(&sender, frame),
            move |ms| runtime.sleep(ms),
        );
        socket.start_message_operation(operation);
        Ok(())
    }
    fn markets_handler(&self, socket: &Rc<BaseWebSocket>) -> Listener {
        let weak = Rc::downgrade(socket);
        let topics = self.current.clone();
        let runtime = self.runtime.clone();
        let category = self.category;
        let active_symbols = self.active_symbols;
        Rc::new(move |_, payload| {
            let Some(socket) = weak.upgrade() else {
                return Ok(());
            };
            let added = payload.get("added")?.array_values().unwrap_or_default();
            let topics = topics.clone();
            let output_topics = topics.clone();
            let has_ws = socket.has_websocket();
            let disposed = socket.is_disposed();
            let sender = weak.clone();
            let clock = runtime.clone();
            let operation: ExchangeFuture<'static, ()> = if active_symbols {
                Box::pin(ticker::on_markets_changed(
                    added
                        .iter()
                        .map(|market| -> Result<Option<String>, ExchangeError> {
                            Ok(market
                                .get("symbol")?
                                .as_str()
                                .map(|units| String::from_utf16_lossy(units)))
                        })
                        .collect::<Result<Vec<_>, _>>()?
                        .into_iter()
                        .flatten()
                        .collect(),
                    move || has_ws,
                    move || disposed,
                    move || current_topics(&topics),
                    move |value| set(&output_topics, value),
                    move |frame| send(&sender, frame),
                    move |ms| clock.sleep(ms),
                ))
            } else {
                Box::pin(primary::on_markets_changed(
                    native_markets(&added)?,
                    move || Ok(has_ws),
                    move || Ok(disposed),
                    move || Ok(category.into()),
                    move || {
                        Ok(Some(primary::TopicSlot::Many(Rc::new(RefCell::new(
                            current_topics(&topics)?,
                        )))))
                    },
                    move || {
                        Ok(move |value: primary::Topics| {
                            *output_topics.borrow_mut() = json!(*value.borrow());
                            Ok(())
                        })
                    },
                    move |frame| send(&sender, frame),
                    move |ms| Ok(clock.sleep(ms)),
                ))
            };
            socket.start_message_operation(operation);
            Ok(())
        })
    }
    pub(super) fn on_open(&self, socket: Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
        let Some(parent) = self.parent.upgrade() else {
            return Ok(());
        };
        let handler = self.markets_handler(&socket);
        let off = |event: &str, _: Listener| {
            if let Some(id) = self.markets_listener.borrow_mut().take() {
                parent.base.off(event, Some(&id));
            }
            Ok(())
        };
        let on = |event: &str, handler: Listener| {
            *self.markets_listener.borrow_mut() = Some(parent.base.on(event, handler));
            Ok(())
        };
        if self.active_symbols {
            lifecycle::on_open(
                || Ok(socket.is_disposed()),
                || sync(&parent, &self.current),
                || self.start_subscribe(&socket),
                || super::ping(&socket),
                || Ok(handler.clone()),
                off,
                on,
                || self.runtime.enable_reconnecting(),
                || socket.emit_ws_reconnected(),
            )
        } else {
            primary::on_open(
                || Ok(socket.is_disposed()),
                || {
                    let store = socket.store();
                    let markets = store.markets()?;
                    Ok(Rc::new(RefCell::new(
                        markets
                            .iter()
                            .filter(|m| m.option.is_none())
                            .map(|m| {
                                Rc::new(RefCell::new(primary::Market {
                                    symbol: m.symbol.clone(),
                                    quote: m.quote.clone(),
                                }))
                            })
                            .collect(),
                    )))
                },
                || Ok(self.category.into()),
                || {
                    Ok(|value: primary::Topics| {
                        *self.current.borrow_mut() = json!(*value.borrow());
                        Ok(())
                    })
                },
                || self.start_subscribe(&socket),
                || super::ping(&socket),
                || {
                    parent
                        .candle_recovery(&socket, self.category)
                        .arm_existing()
                },
                || Ok(handler.clone()),
                || Ok(off),
                || Ok(on),
                || self.runtime.enable_reconnecting(),
                || socket.emit_ws_reconnected(),
            )
        }
    }
    pub(super) fn dispose(&self) {
        if let Some(parent) = self.parent.upgrade() {
            // Source Spot removes only its active callback; markets callback keeps
            // its disposed guard until the parent emitter is cleared.
            if let Some(id) = &self.active_listener {
                parent.base.off("activeTickerSymbols:changed", Some(id));
            }
        }
    }
}
fn current_topics(topics: &Topics) -> Result<Vec<String>, ExchangeError> {
    current(topics)
}
