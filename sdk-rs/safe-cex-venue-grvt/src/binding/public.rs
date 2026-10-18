//! Public subscription and lifecycle adapters over the shared socket core.
use super::runtime::{Host, Shared};
use crate::{
    composition::GrvtPublicWebSocket,
    public_feeds::{
        self, FeedCleanup, FeedKind, FeedRegistry, LevelProjector, MarketRef, PublicFeedHost,
        TickerChanges,
    },
    public_lifecycle::{self, AddedMarkets, SocketEvent, TickerBatchHost},
    public_subscriptions::{PublicSubscriptions, RpcRequest},
};
use safe_cex_exchange::{
    js_runtime::{
        ops, sockets,
        value::{Js, Prop, Thrown, arg, from_typed, to_typed, utf16},
    },
    runtime::ExchangeError,
};
use safe_cex_types::{Market, OHLCVOptions, OrderBookOrders, Position};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub(super) fn err(e: Thrown) -> ExchangeError {
    e.to_exchange_error()
}
fn ex<T>(v: Result<T, ExchangeError>) -> Result<T, Thrown> {
    v.map_err(Into::into)
}
fn text(v: &Js) -> Result<String, Thrown> {
    Ok(String::from_utf16_lossy(&v.to_js_string()?))
}
pub(super) struct Public {
    shared: Weak<Shared>,
    pub(super) this: Js,
    state: Rc<PublicSubscriptions<u64>>,
    feeds: Rc<FeedRegistry<Js>>,
    next_timer: Cell<u64>,
    timers: RefCell<indexmap::IndexMap<u64, Js>>,
}
impl Public {
    pub(super) fn new(h: &Host, model: GrvtPublicWebSocket<Js, u64>) -> Rc<Self> {
        Rc::new(Self {
            shared: Rc::downgrade(&h.0.shared),
            this: model.base,
            state: model.state,
            feeds: Rc::new(FeedRegistry::default()),
            next_timer: Cell::new(0),
            timers: RefCell::default(),
        })
    }
    fn host(&self) -> Result<Host, Thrown> {
        Ok(Host::from_shared(
            self.shared
                .upgrade()
                .ok_or_else(|| Thrown::error("Error", "exchange runtime disposed"))?,
            self.this.clone(),
        ))
    }
    fn call(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.this.call_method(name, args)
    }
    fn parent(&self) -> Result<Js, Thrown> {
        self.this.get("parent")
    }
    fn send(&self, _request: &RpcRequest, wire: &str) -> Result<(), ExchangeError> {
        self.call("send", vec![Js::str(wire)])
            .map(|_| ())
            .map_err(err)
    }
    fn clear(&self, id: &u64) -> Result<(), ExchangeError> {
        let timer = self.timers.borrow_mut().shift_remove(id);
        if let Some(timer) = timer {
            self.host().map_err(err)?.0.clear_timeout(&timer);
        }
        Ok(())
    }
    fn disposed(&self) -> bool {
        self.this.get("isDisposed").is_ok_and(|v| v.truthy())
    }
    fn connected(&self) -> bool {
        self.this.get("isConnected").is_ok_and(|v| v.truthy())
    }
    fn emit(&self, message: &str) -> Result<(), ExchangeError> {
        self.call("emitAndLogError", vec![Js::str(message)])
            .map(|_| ())
            .map_err(err)
    }
    fn topic_views(&self) -> Result<(), Thrown> {
        let topics = self.this.get("topics")?;
        let keys = self
            .feeds
            .topics
            .borrow()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for key in keys {
            let feeds = self.feeds.clone();
            let read = key.clone();
            topics.define_accessor(
                &key,
                Some(Rc::new(move |_| {
                    Ok(Js::Number(
                        feeds
                            .topics
                            .borrow()
                            .get(&read)
                            .copied()
                            .unwrap_or(f64::NAN),
                    ))
                })),
                None,
            )?;
        }
        Ok(())
    }
    pub(super) fn invoke(self: &Rc<Self>, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        let a = arg(&args, 0);
        let send = |r: &RpcRequest, w: &str| self.send(r, w);
        match name {
            "connectAndSubscribe" => {
                let slot = RefCell::new(None::<Rc<Js>>);
                ex(public_lifecycle::connect_and_subscribe(
                    || self.disposed(),
                    || {
                        self.parent()
                            .and_then(|p| p.get("options"))
                            .and_then(|v| v.get("testnet"))
                            .is_ok_and(|v| v.truthy())
                    },
                    &slot,
                    |event| {
                        Rc::new(
                            self.this
                                .get(match event {
                                    SocketEvent::Open => "onOpen",
                                    SocketEvent::Message => "onMessage",
                                    SocketEvent::Close => "onClose",
                                })
                                .expect("assembled member"),
                        )
                    },
                    |url| {
                        let ws = self
                            .call("createWebsocket", vec![Js::str(url)])
                            .map_err(err)?;
                        self.this.set("ws", ws.clone()).map_err(err)?;
                        Ok(Rc::new(ws))
                    },
                    |ws, event, listener| {
                        ws.call_method(
                            "addEventListener",
                            vec![
                                Js::str(match event {
                                    SocketEvent::Open => "open",
                                    SocketEvent::Message => "message",
                                    SocketEvent::Close => "close",
                                }),
                                listener.clone(),
                            ],
                        )
                        .map(|_| ())
                        .map_err(err)
                    },
                ))?;
                Ok(Js::Undefined)
            }
            "onOpen" => {
                ex(public_lifecycle::on_open(
                    || Rc::new(self.this.get("onMarketsChanged").expect("assembled member")),
                    || self.disposed(),
                    || {
                        self.call("trackLastReceived", vec![])
                            .map(|_| ())
                            .map_err(err)
                    },
                    || self.call("resubscribe", vec![]).map(|_| ()).map_err(err),
                    || {
                        self.call("subscribeToAllTickers", vec![])
                            .map(|_| ())
                            .map_err(err)
                    },
                    |event, listener| {
                        self.this
                            .get("emitter")
                            .and_then(|v| {
                                v.call_method("off", vec![Js::str(event), listener.clone()])
                            })
                            .map(|_| ())
                            .map_err(err)
                    },
                    |event, listener| {
                        self.this
                            .get("emitter")
                            .and_then(|v| {
                                v.call_method("on", vec![Js::str(event), listener.clone()])
                            })
                            .map(|_| ())
                            .map_err(err)
                    },
                ))?;
                Ok(Js::Undefined)
            }
            "onMarketsChanged" => {
                let added: Vec<Market> = to_typed(&a.get("added")?)?;
                let added = RefCell::new(
                    added
                        .into_iter()
                        .map(|m| Rc::new(RefCell::new(m)))
                        .collect(),
                );
                ex(public_lifecycle::on_markets_changed(
                    &self.state,
                    &added,
                    || self.this.get("ws").is_ok_and(|v| v.truthy()),
                    || self.disposed(),
                    || self.connected(),
                    send,
                    |message| self.emit(message),
                ))?;
                Ok(Js::Undefined)
            }
            "resubscribe" => {
                ex(public_lifecycle::resubscribe(&self.state, send))?;
                Ok(Js::Undefined)
            }
            "subscribeToAllTickers" => {
                ex(public_lifecycle::subscribe_to_all_tickers(
                    self.state.clone(),
                    self.clone(),
                ))?;
                Ok(Js::Undefined)
            }
            "cancelTickerBatches" => {
                ex(self.state.cancel_ticker_batches(|id| self.clear(id)))?;
                Ok(Js::Undefined)
            }
            "ensureTickerSubscription" => Ok(ex(self.state.ensure_ticker_subscription(
                &text(&a)?,
                || self.disposed(),
                || self.connected(),
                send,
            ))?
            .map_or(Js::Undefined, Js::Number)),
            "subscribeToKline" => Ok(Js::Number(ex(self.state.subscribe_to_kline(
                &text(&a)?,
                &text(&arg(&args, 1))?,
                if arg(&args, 2).is_undefined() {
                    None
                } else {
                    Some(text(&arg(&args, 2))?)
                }
                .as_deref(),
                send,
            ))?)),
            "subscribeToOrderbook" => Ok(Js::Number(ex(self.state.subscribe_to_orderbook(
                &text(&a)?,
                arg(&args, 1).as_f64(),
                arg(&args, 2).as_f64(),
                send,
            ))?)),
            "subscribeToTicker" => Ok(Js::Number(ex(self.state.subscribe_to_ticker(
                &text(&a)?,
                arg(&args, 1).as_f64(),
                if arg(&args, 2).is_undefined() {
                    None
                } else {
                    Some(text(&arg(&args, 2))?)
                }
                .as_deref(),
                send,
            ))?)),
            "unsubscribeFromKline" => {
                ex(self.state.unsubscribe_from_kline(a.to_number()?, send))?;
                Ok(Js::Undefined)
            }
            "unsubscribeFromOrderbook" => {
                ex(self.state.unsubscribe_from_orderbook(a.to_number()?, send))?;
                Ok(Js::Undefined)
            }
            "unsubscribeFromTicker" => {
                ex(self
                    .state
                    .unsubscribe_from_ticker(a.to_number()?, send, |id| self.clear(id)))?;
                Ok(Js::Undefined)
            }
            "mapIntervalToGrvt" => Ok(Js::str(
                &PublicFeedHost::map_interval(self.as_ref(), &text(&a)?).map_err(Thrown::from)?,
            )),
            "onMessage" => {
                ex(public_feeds::on_message(
                    self.as_ref(),
                    &text(&a.get("data")?)?,
                ))?;
                Ok(Js::Undefined)
            }
            "handleKlineUpdate" => {
                ex(public_feeds::handle_kline_update(self.as_ref(), a))?;
                Ok(Js::Undefined)
            }
            "handleOrderbookUpdate" => {
                ex(public_feeds::handle_orderbook_update(self.as_ref(), a))?;
                Ok(Js::Undefined)
            }
            "handleTickerUpdate" => {
                ex(public_feeds::handle_ticker_update(self.as_ref(), a))?;
                Ok(Js::Undefined)
            }
            "listenOHLCV" => {
                let opts: OHLCVOptions = to_typed(&a)?;
                let callback = arg(&args, 1);
                let cleanup = ex(public_feeds::listen_ohlcv(
                    self.clone(),
                    &opts.symbol,
                    &text(&from_typed(&opts.interval)?)?,
                    Rc::new(move |c| {
                        callback
                            .call(Js::Undefined, vec![from_typed(&c).map_err(err)?])
                            .map(|_| ())
                            .map_err(err)
                    }),
                ))?;
                Ok(Js::function("cleanup", move |_, _| {
                    ex(cleanup())?;
                    Ok(Js::Undefined)
                }))
            }
            "listenOrderBook" => {
                let callback = arg(&args, 1);
                let cleanup = ex(public_feeds::listen_order_book(
                    self.clone(),
                    &text(&a)?,
                    Rc::new(move |book| {
                        callback
                            .call(Js::Undefined, vec![from_typed(&book).map_err(err)?])
                            .map(|_| ())
                            .map_err(err)
                    }),
                ))?;
                Ok(Js::function("cleanup", move |_, _| {
                    ex(cleanup())?;
                    Ok(Js::Undefined)
                }))
            }
            "dispose" => {
                ex(public_lifecycle::dispose(
                    || Rc::new(self.this.get("onMarketsChanged").expect("assembled member")),
                    &self.state,
                    |id| self.clear(id),
                    |event, listener| {
                        self.this
                            .get("emitter")
                            .and_then(|v| {
                                v.call_method("off", vec![Js::str(event), listener.clone()])
                            })
                            .map(|_| ())
                            .map_err(err)
                    },
                    || {
                        let h = self.host().map_err(err)?;
                        let core = sockets::core_of(&h.0.shared.sockets, &self.this)
                            .expect("constructed socket core");
                        if let Some(base) = core.base.get() {
                            base.dispose();
                        }
                        Ok(())
                    },
                ))?;
                Ok(Js::Undefined)
            }
            _ => Err(Thrown::error(
                "Error",
                &format!("unknown GRVT public member {name}"),
            )),
        }
    }
}
impl TickerBatchHost<u64> for Public {
    fn is_disposed(&self) -> bool {
        self.disposed()
    }
    fn is_connected(&self) -> bool {
        self.connected()
    }
    fn markets(&self) -> Rc<AddedMarkets> {
        let h = self.host().expect("live runtime");
        Rc::new(RefCell::new(
            h.0.shared
                .store
                .markets()
                .iter()
                .cloned()
                .map(|v| Rc::new(RefCell::new(v)))
                .collect(),
        ))
    }
    fn clear(&self, id: &u64) -> Result<(), ExchangeError> {
        self.clear(id)
    }
    fn schedule(
        &self,
        delay: f64,
        callback: public_lifecycle::TimerCallback,
    ) -> Result<u64, ExchangeError> {
        let id = self.next_timer.get() + 1;
        self.next_timer.set(id);
        let callback = Rc::new(RefCell::new(callback));
        let timer = self
            .host()
            .map_err(err)?
            .0
            .set_timeout(Rc::new(move || ex((callback.borrow_mut())())), delay);
        self.timers.borrow_mut().insert(id, timer);
        Ok(id)
    }
    fn send(&self, r: &RpcRequest, w: &str) -> Result<(), ExchangeError> {
        self.send(r, w)
    }
    fn emit(&self, message: &str) -> Result<(), ExchangeError> {
        self.emit(message)
    }
}
impl PublicFeedHost for Public {
    type Value = Js;
    fn property(&self, v: &Js, key: &str) -> Result<Js, ExchangeError> {
        v.get(key).map_err(err)
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn string_type(&self, v: &Js) -> bool {
        v.is_string()
    }
    fn number_type(&self, v: &Js) -> bool {
        matches!(v, Js::Number(_))
    }
    fn string(&self, v: &Js) -> Result<String, ExchangeError> {
        text(v).map_err(err)
    }
    fn number(&self, v: &Js) -> Result<f64, ExchangeError> {
        v.to_number().map_err(err)
    }
    fn float(&self, v: &Js) -> Result<f64, ExchangeError> {
        Ok(safe_cex_num::parse_float(&text(v).map_err(err)?))
    }
    fn integer(&self, v: &Js) -> Result<f64, ExchangeError> {
        Ok(safe_cex_num::parse_int(&text(v).map_err(err)?, None))
    }
    fn string_value(&self, v: &str) -> Js {
        Js::str(v)
    }
    fn split(&self, v: &Js, separator: &str) -> Result<Vec<String>, ExchangeError> {
        let parts = v
            .call_method("split", vec![Js::str(separator)])
            .map_err(err)?;
        parts
            .array_values()
            .expect("split array")
            .iter()
            .map(|v| text(v).map_err(err))
            .collect()
    }
    fn call_map(
        &self,
        method: Js,
        receiver: Js,
        projector: LevelProjector<Js>,
    ) -> Result<Js, ExchangeError> {
        method
            .call(
                receiver,
                vec![Js::function("projectLevel", move |_, args| {
                    ex(projector(arg(&args, 0), arg(&args, 1), arg(&args, 2)))
                })],
            )
            .map_err(err)
    }
    fn index_less_than(&self, index: f64, limit: &Js) -> Result<bool, ExchangeError> {
        ops::less_than(&Js::Number(index), limit).map_err(err)
    }
    fn level_value(&self, price: f64, amount: f64, total: f64) -> Js {
        Js::object(vec![
            ("price", Js::Number(price)),
            ("amount", Js::Number(amount)),
            ("total", Js::Number(total)),
        ])
    }
    fn book_levels(&self, v: Js) -> Result<Vec<OrderBookOrders>, ExchangeError> {
        let refuse =
            || ExchangeError::new("Orderbook callback requires dense own-data numeric levels");
        let items = v.array_items().ok_or_else(refuse)?;
        let mut levels = vec![];
        for level in items {
            let Js::Object(level) = level.ok_or_else(refuse)? else {
                return Err(refuse());
            };
            let props = level.props.borrow();
            let number = |key: &str| match props.get(&utf16(key)) {
                Some(Prop::Data(Js::Number(v))) => Ok(*v),
                _ => Err(refuse()),
            };
            levels.push(OrderBookOrders {
                price: number("price")?,
                amount: number("amount")?,
                total: number("total")?,
            });
        }
        Ok(levels)
    }
    fn empty_array(&self) -> Js {
        Js::array(vec![])
    }
    fn json_parse(&self, data: &str) -> Result<Option<Js>, ExchangeError> {
        let h = self.host().map_err(err)?;
        h.0.global("JSON.parse")
            .and_then(|f| f.call(Js::Undefined, vec![Js::str(data)]))
            .map(Some)
            .map_err(err)
    }
    fn disposed(&self) -> bool {
        self.disposed()
    }
    fn connected(&self) -> bool {
        self.connected()
    }
    fn track_received(&self) -> Result<(), ExchangeError> {
        self.call("trackLastReceived", vec![])
            .map(|_| ())
            .map_err(err)
    }
    fn dispatch(&self, kind: FeedKind, message: Js) -> Result<(), ExchangeError> {
        self.call(
            match kind {
                FeedKind::Book => "handleOrderbookUpdate",
                FeedKind::Candle => "handleKlineUpdate",
                FeedKind::Ticker => "handleTickerUpdate",
            },
            vec![message],
        )
        .map(|_| ())
        .map_err(err)
    }
    fn emit_error(&self, message: &str) -> Result<(), ExchangeError> {
        self.emit(message)
    }
    fn registry(&self) -> &FeedRegistry<Js> {
        &self.feeds
    }
    fn record_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.call("recordMessageHandlerStats", vec![Js::str(topic)])
            .map(|_| ())
            .map_err(err)
    }
    fn cleanup_stats(&self, topic: &str) -> Result<(), ExchangeError> {
        self.call("cleanupMessageHandlerStats", vec![Js::str(topic)])
            .map(|_| ())
            .map_err(err)
    }
    fn market_by_instrument(&self, instrument: &Js) -> Result<Option<Js>, ExchangeError> {
        let h = self.host().map_err(err)?;
        let values = h.0.shared.store.markets();
        for m in values.iter() {
            if from_typed(&m.id).map_err(err)?.strict_equals(instrument) {
                return from_typed(m).map(Some).map_err(err);
            }
        }
        Ok(None)
    }
    fn market_by_symbol(&self, symbol: &str) -> Result<Option<MarketRef>, ExchangeError> {
        let h = self.host().map_err(err)?;
        Ok(h.0
            .shared
            .store
            .markets()
            .iter()
            .find(|m| m.symbol == symbol)
            .cloned()
            .map(|m| Rc::new(RefCell::new(m))))
    }
    fn current_ticker(&self, id: &Js) -> Result<Option<Js>, ExchangeError> {
        let map = self
            .parent()
            .and_then(|p| p.get("store"))
            .and_then(|v| v.get("tickersById"))
            .map_err(err)?;
        let result = map.get_key(&id.to_js_string().map_err(err)?).map_err(err)?;
        Ok((!result.is_undefined()).then_some(result))
    }
    fn positions(&self) -> Result<Rc<RefCell<Vec<Option<Rc<RefCell<Position>>>>>>, ExchangeError> {
        let h = self.host().map_err(err)?;
        Ok(Rc::new(RefCell::new(
            h.0.shared
                .store
                .positions()
                .iter()
                .cloned()
                .map(|p| Some(Rc::new(RefCell::new(p))))
                .collect(),
        )))
    }
    fn update_ticker(&self, id: &Js, changes: &TickerChanges<Js>) -> Result<(), ExchangeError> {
        let patch = Js::object(vec![]);
        for (key, value) in [
            ("symbol", changes.symbol.clone()),
            ("info", changes.info.clone()),
        ] {
            if let Some(value) = value {
                patch.define(key, value).map_err(err)?;
            }
        }
        for (key, value) in [
            ("mark", changes.mark),
            ("index", changes.index),
            ("last", changes.last),
            ("bid", changes.bid),
            ("ask", changes.ask),
            ("fundingRate", changes.funding_rate),
            ("openInterest", changes.open_interest),
            ("volume", changes.volume),
            ("quoteVolume", changes.quote_volume),
            ("percentage", changes.percentage),
            ("fundingTime", changes.funding_time),
        ] {
            if let Some(value) = value {
                patch.define(key, Js::Number(value)).map_err(err)?;
            }
        }
        self.parent()
            .and_then(|p| p.get("store"))
            .and_then(|v| {
                v.call_method(
                    "updateTicker",
                    vec![Js::object(vec![("id", id.clone())]), patch],
                )
            })
            .map(|_| ())
            .map_err(err)
    }
    fn sync_upnl(&self) -> Result<(), ExchangeError> {
        self.parent()
            .and_then(|p| p.call_method("syncBalanceUpnlFromPositions", vec![]))
            .map(|_| ())
            .map_err(err)
    }
    fn map_interval(&self, interval: &str) -> Result<String, ExchangeError> {
        match crate::public_subscriptions::map_interval_to_grvt(interval) {
            safe_cex_num::TableHit::Mapped(v) => Ok(v),
            safe_cex_num::TableHit::Missing => Ok(interval.into()),
            safe_cex_num::TableHit::Inherited(_) => Err(ExchangeError::new(
                "Inherited interval is outside the native wire contract",
            )),
        }
    }
    fn subscribe(
        &self,
        kind: FeedKind,
        instrument: &str,
        interval: Option<&str>,
    ) -> Result<f64, ExchangeError> {
        self.topic_views().map_err(err)?;
        let result = match kind {
            FeedKind::Book => self.call("subscribeToOrderbook", vec![Js::str(instrument)]),
            FeedKind::Candle => self.call(
                "subscribeToKline",
                vec![Js::str(instrument), interval.map_or(Js::Undefined, Js::str)],
            ),
            FeedKind::Ticker => self.call("subscribeToTicker", vec![Js::str(instrument)]),
        }
        .map_err(err)?;
        result.to_number().map_err(err)
    }
    fn unsubscribe(&self, kind: FeedKind, id: f64) -> Result<(), ExchangeError> {
        self.call(
            match kind {
                FeedKind::Book => "unsubscribeFromOrderbook",
                FeedKind::Candle => "unsubscribeFromKline",
                FeedKind::Ticker => "unsubscribeFromTicker",
            },
            vec![Js::Number(id)],
        )
        .map(|_| ())
        .map_err(err)
    }
    fn schedule(&self, callback: FeedCleanup, delay: f64) -> Result<(), ExchangeError> {
        self.host()
            .map_err(err)?
            .0
            .set_timeout(Rc::new(move || ex(callback())), delay);
        Ok(())
    }
}
