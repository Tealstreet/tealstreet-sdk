//! Public topic dispatch, ticker/PnL updates and candle/book subscriptions.
use crate::{
    data::{get, truthy},
    exchange::{ApexOmniExchange, failure, float, text},
    lifecycle::Cleanup,
    protocol,
    socket_host::{
        ApexSocketHost, SocketListener, endpoint, number, object, property, send, subscription,
    },
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue as J, add, divide, math_max, math_min};
use safe_cex_store::model_patch::ModelPatch;
use safe_cex_types::{Candle, OHLCVOptions, OrderBook, OrderBookOrders, PositionSide};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

type Handler = Rc<dyn Fn(&J) -> Result<(), ExchangeError>>;
pub struct ApexOmniPublicWebsocket {
    pub parent: Rc<ApexOmniExchange>,
    pub host: Rc<dyn ApexSocketHost>,
    topics: RefCell<Vec<(String, Vec<String>)>>,
    handlers: RefCell<Vec<(String, Vec<(u64, Handler)>)>>,
    next_handler: Cell<u64>,
}
impl ApexOmniPublicWebsocket {
    // ts: ApexOmniPublicWebsocket.constructor
    pub fn new(
        parent: Rc<ApexOmniExchange>,
        host: Rc<dyn ApexSocketHost>,
    ) -> Result<Rc<Self>, ExchangeError> {
        host.initialize(false, false)?;
        let socket = Rc::new(Self {
            parent,
            host,
            topics: RefCell::new(vec![(
                "instrumentInfo".into(),
                vec!["instrumentInfo.all".into()],
            )]),
            handlers: RefCell::new(Vec::new()),
            next_handler: Cell::new(1),
        });
        let weak = Rc::downgrade(&socket);
        socket.register(
            "instrumentInfo",
            Rc::new(move |j| {
                weak.upgrade()
                    .ok_or_else(|| failure("public socket owner released".into()))?
                    .handle_instrument_info_event(j)
            }),
        );
        let host = socket.host.clone();
        socket.register("pong", Rc::new(move |_| host.handle_pong(None)));
        Ok(socket)
    }
    fn register(&self, topic: &str, handler: Handler) -> u64 {
        let id = self.next_handler.get();
        self.next_handler.set(id + 1);
        let mut handlers = self.handlers.borrow_mut();
        if let Some((_, list)) = handlers.iter_mut().find(|(key, _)| key == topic) {
            list.push((id, handler));
        } else {
            handlers.push((topic.into(), vec![(id, handler)]));
        }
        id
    }
    fn remove_handler(&self, topic: &str, id: u64) -> bool {
        let mut handlers = self.handlers.borrow_mut();
        if let Some((_, list)) = handlers.iter_mut().find(|(key, _)| key == topic) {
            list.retain(|(key, _)| *key != id);
            return list.is_empty();
        }
        true
    }
    fn handlers_for(&self, topic: &str) -> Vec<Handler> {
        self.handlers
            .borrow()
            .iter()
            .find(|(key, _)| key == topic)
            .map_or_else(Vec::new, |(_, list)| {
                list.iter().map(|(_, h)| h.clone()).collect()
            })
    }
    fn set_topic(&self, topic: &str) {
        let mut topics = self.topics.borrow_mut();
        if !topics.iter().any(|(key, _)| key == topic) {
            topics.push((topic.into(), vec![topic.into()]));
        }
    }
    fn delete_topic(&self, topic: &str) {
        self.topics.borrow_mut().retain(|(key, _)| key != topic);
    }
    // ts: ApexOmniPublicWebsocket.connectAndSubscribe
    pub fn connect_and_subscribe(
        self: &Rc<Self>,
        _is_reconnect: bool,
    ) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        let default =
            protocol::BASE_WS_URL[usize::from(self.parent.base.options().testnet == Some(true))].1;
        let url = endpoint(default, self.host.now_ms(), &self.host.nanoid());
        self.host.create_socket(&url)?;
        let weak = Rc::downgrade(self);
        self.host
            .add_listener(SocketListener::Open(Rc::new(move || {
                let owner = weak.upgrade();
                Box::pin(async move {
                    owner
                        .ok_or_else(|| failure("public socket owner released".into()))?
                        .on_open()
                })
            })))?;
        let weak = Rc::downgrade(self);
        self.host
            .add_listener(SocketListener::Message(Rc::new(move |raw| {
                weak.upgrade()
                    .ok_or_else(|| failure("public socket owner released".into()))?
                    .on_message(raw)
            })))?;
        self.host.add_listener(SocketListener::Close)
    }
    // ts: ApexOmniPublicWebsocket.onOpen
    pub fn on_open(&self) -> Result<(), ExchangeError> {
        if !self.host.is_disposed() {
            self.subscribe()?;
            self.ping()?;
            if self.host.enable_reconnecting() {
                self.host.emit_reconnected()?;
            }
        }
        Ok(())
    }
    // ts: ApexOmniPublicWebsocket.ping
    pub fn ping(&self) -> Result<(), ExchangeError> {
        if !self.host.is_disposed() {
            self.host.set_ping_at(self.host.monotonic_now_ms())?;
            send(
                self.host.as_ref(),
                object([("op", J::String("ping".into()))]),
                false,
            )?;
        }
        Ok(())
    }
    // ts: ApexOmniPublicWebsocket.subscribe
    pub fn subscribe(&self) -> Result<(), ExchangeError> {
        let topics: Vec<String> = self
            .topics
            .borrow()
            .iter()
            .flat_map(|(_, v)| v.clone())
            .collect();
        for chunk in topics.chunks(50) {
            subscription(self.host.as_ref(), "subscribe", chunk, false)?;
        }
        Ok(())
    }
    // ts: ApexOmniPublicWebsocket.onMessage
    pub fn on_message(&self, raw: &str) -> Result<(), ExchangeError> {
        if self.host.is_disposed() {
            return Ok(());
        }
        self.host.track_last_received()?;
        if raw.contains("op\":\"ping") {
            send(
                self.host.as_ref(),
                object([
                    ("op", J::String("pong".into())),
                    (
                        "args",
                        J::Array(vec![J::String(safe_cex_num::js_number_string(
                            self.host.now_ms(),
                        ))]),
                    ),
                ]),
                false,
            )?;
        }
        if let Some(start) = raw.find("\"topic\":\"") {
            let rest = &raw[start + 9..];
            let Some(end) = rest.find('"') else {
                return Ok(());
            };
            if end == 0 {
                return Ok(());
            }
            let topic = &rest[..end];
            let exact = self.handlers_for(topic);
            let prefix = topic
                .find('.')
                .filter(|i| *i > 0)
                .map_or_else(Vec::new, |i| self.handlers_for(&topic[..i]));
            if exact.is_empty() && prefix.is_empty() {
                return Ok(());
            }
            self.host.record_stats(topic)?;
            let json = J::from(
                safe_cex_transport::how::json_parse(raw)
                    .into_value()
                    .map_err(|error| ExchangeError::new(error.to_string()))?,
            );
            if !truthy(&json) {
                return Ok(());
            }
            for handler in exact.into_iter().chain(prefix) {
                handler(&json)?;
            }
            return Ok(());
        }
        if raw.contains("pong") {
            let handlers = self.handlers_for("pong");
            if !handlers.is_empty() {
                self.host.record_stats("pong")?;
                for handler in handlers {
                    handler(&J::Object(Vec::new()))?;
                }
            }
        }
        Ok(())
    }
    // ts: ApexOmniPublicWebsocket.handleInstrumentInfoEvent
    pub fn handle_instrument_info_event(&self, json: &J) -> Result<(), ExchangeError> {
        for row in crate::data::array(property(json, "data")?).map_err(failure)? {
            let symbol = text(get(row, "s"))?;
            let ticker = self
                .parent
                .base
                .store
                .tickers_by_symbol()
                .get(&symbol)
                .cloned()
                .or_else(|| self.parent.base.store.tickers_by_id().get(&symbol).cloned());
            if let Some(ticker) = ticker {
                let mut patch = ModelPatch::default();
                let mut reference = 0.;
                let mut last = 0.;
                for (source, target) in [
                    ("p", "last"),
                    ("mp", "mark"),
                    ("xp", "index"),
                    ("pr", "percentage"),
                    ("o", "openInterest"),
                    ("fr", "fundingRate"),
                    ("v", "volume"),
                    ("to", "quoteVolume"),
                ] {
                    if truthy(get(row, source)) {
                        let value =
                            float(get(row, source))? * if source == "pr" { 100. } else { 1. };
                        patch.set_number(target, value);
                        if source == "mp" {
                            reference = value;
                        }
                        if source == "p" {
                            last = value;
                        }
                    }
                }
                if !truthy(get(row, "mp")) && truthy(get(row, "xp")) {
                    reference = float(get(row, "xp"))?;
                    patch.set_number("mark", reference);
                }
                self.parent
                    .base
                    .store
                    .update_ticker(&ticker.id, &patch)
                    .map_err(|e| failure(e.to_string()))?;
                if reference == 0. || reference.is_nan() {
                    reference = last;
                }
                if reference != 0. && !reference.is_nan() {
                    let market = self
                        .parent
                        .base
                        .store
                        .markets_by_symbol()
                        .get(&symbol)
                        .cloned();
                    if market.is_none_or(|m| m.is_inverse == Some(true)) {
                        continue;
                    }
                    let positions = self
                        .parent
                        .base
                        .store
                        .positions()
                        .iter()
                        .filter(|p| p.symbol == symbol && p.contracts != 0.)
                        .cloned()
                        .collect::<Vec<_>>();
                    for position in positions {
                        let direction = if position.side == PositionSide::Long {
                            1.
                        } else {
                            -1.
                        };
                        let pnl = direction
                            * (position.contracts * reference
                                - position.contracts * position.entry_price);
                        let mut patch = ModelPatch::default();
                        patch.set_number("unrealizedPnl", pnl);
                        self.parent
                            .base
                            .store
                            .update_position(&position.id, &patch)
                            .map_err(|e| failure(e.to_string()))?;
                    }
                }
            }
        }
        Ok(())
    }
    // ts: ApexOmniPublicWebsocket.listenOHLCV
    pub fn listen_ohlcv(
        self: &Rc<Self>,
        opts: OHLCVOptions,
        callback: Rc<dyn Fn(Candle) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        let interval = match opts.interval {
            safe_cex_types::AnyTimeframe::Variant0(safe_cex_types::SecondsTimeframe::S1) => "1s",
            safe_cex_types::AnyTimeframe::Variant0(safe_cex_types::SecondsTimeframe::S5) => "5s",
            safe_cex_types::AnyTimeframe::Variant0(safe_cex_types::SecondsTimeframe::S15) => "15s",
            safe_cex_types::AnyTimeframe::Variant0(safe_cex_types::SecondsTimeframe::S30) => "30s",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M1) => "1m",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M3) => "3m",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M5) => "5m",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M15) => "15m",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::M30) => "30m",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H1) => "1h",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H2) => "2h",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H4) => "4h",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H6) => "6h",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::H12) => "12h",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::D1) => "1d",
            safe_cex_types::AnyTimeframe::Variant1(safe_cex_types::Timeframe::W1) => "1w",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H3) => "3h",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H5) => "5h",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H7) => "7h",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H8) => "8h",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H9) => "9h",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H10) => "10h",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H11) => "11h",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::H24) => "24h",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::D2) => "2d",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::D3) => "3d",
            safe_cex_types::AnyTimeframe::Variant2(safe_cex_types::ExtendedTimeframe::MN1) => "1M",
        };
        let value = protocol::INTERVAL
            .iter()
            .find(|(key, _)| *key == interval)
            .map(|(_, value)| *value)
            .unwrap_or("undefined");
        let topic = format!("candle.{value}.{}", opts.symbol);
        let secondary = format!("recentlyTrade.H.{}", opts.symbol);
        let candle = Rc::new(RefCell::new(None::<Candle>));
        let state = candle.clone();
        let cb = callback.clone();
        let primary: Handler = Rc::new(move |json| {
            let J::Array(rows) = get(json, "data") else {
                return Ok(());
            };
            let Some(row) = rows
                .first()
                .filter(|r| matches!(r, J::Object(_) | J::Array(_)) && truthy(get(r, "start")))
            else {
                return Ok(());
            };
            let timestamp = number(get(row, "start"))? / 1000.;
            if state
                .borrow()
                .as_ref()
                .is_some_and(|c| c.timestamp > timestamp)
            {
                return Ok(());
            }
            let next = Candle {
                timestamp,
                open: float(get(row, "open"))?,
                high: float(get(row, "high"))?,
                low: float(get(row, "low"))?,
                close: float(get(row, "close"))?,
                volume: float(get(row, "volume"))?,
                last_side: None,
            };
            *state.borrow_mut() = Some(next.clone());
            cb(next)
        });
        let state = candle;
        let host = self.host.clone();
        let trade: Handler = Rc::new(move |json| {
            let Some(mut last) = state.borrow().clone() else {
                return Ok(());
            };
            let J::Array(rows) = get(json, "data") else {
                return Ok(());
            };
            let Some(row) = rows
                .first()
                .filter(|r| matches!(r, J::Object(_) | J::Array(_)))
            else {
                return Ok(());
            };
            if ["T", "p", "v"]
                .iter()
                .any(|k| matches!(get(row, k), J::Undefined))
            {
                return Ok(());
            }
            let start = safe_cex_candle_cache::calendar::get_candle_start_time(
                interval,
                number(get(row, "T"))? / 1000.,
            );
            if last.timestamp > start {
                host.warn(
                    "Received trade with timestamp in the past, ignoring:",
                    get(row, "T"),
                )?;
                return Ok(());
            }
            let J::Number(price) = get(row, "p") else {
                return Err(failure(
                    "typed Candle.close refuses a non-number trade price".into(),
                ));
            };
            if last.timestamp < start {
                last = Candle {
                    timestamp: start,
                    open: last.close,
                    high: last.close,
                    low: last.close,
                    close: last.close,
                    volume: 0.,
                    last_side: None,
                };
            }
            last.high = math_max(last.high, *price);
            last.low = math_min(last.low, *price);
            last.close = *price;
            let volume = get(row, "v");
            let decimals = match volume {
                J::Number(value) => safe_cex_num::after_decimal(*value),
                J::Null | J::Undefined => 2.,
                value => safe_cex_num::after_decimal(text(value)?.as_str()),
            };
            last.volume = add(
                last.volume,
                divide(number(volume)?, *price, Some(math_max(8., decimals))),
            );
            *state.borrow_mut() = Some(last.clone());
            callback(last)
        });
        let entry = Rc::new(CandleSubscription {
            topic: topic.clone(),
            secondary: secondary.clone(),
            primary,
            trade,
            primary_ids: RefCell::new(Vec::new()),
            trade_ids: RefCell::new(Vec::new()),
        });
        self.wait_candle(entry.clone())?;
        let weak = Rc::downgrade(self);
        Ok(Rc::new(move || {
            let owner = weak
                .upgrade()
                .ok_or_else(|| failure("public socket owner released".into()))?;
            for id in entry.primary_ids.borrow().iter() {
                owner.remove_handler(&topic, *id);
            }
            for id in entry.trade_ids.borrow().iter() {
                owner.remove_handler(&secondary, *id);
            }
            let last_primary = owner.handlers_for(&topic).is_empty();
            let last_secondary = owner.handlers_for(&secondary).is_empty();
            if last_primary {
                owner.delete_topic(&topic);
                owner.delete_topic(&secondary);
            }
            if owner.host.is_connected() && last_primary {
                subscription(
                    owner.host.as_ref(),
                    "unsubscribe",
                    std::slice::from_ref(&topic),
                    false,
                )?;
            }
            if owner.host.is_connected() && last_secondary {
                owner.host.cleanup_stats(&topic)?;
                owner.host.cleanup_stats(&secondary)?;
                subscription(
                    owner.host.as_ref(),
                    "unsubscribe",
                    std::slice::from_ref(&secondary),
                    false,
                )?;
            }
            Ok(())
        }))
    }
    fn wait_candle(self: &Rc<Self>, entry: Rc<CandleSubscription>) -> Result<(), ExchangeError> {
        if self.host.is_connected() {
            if !self.host.is_disposed() {
                entry
                    .primary_ids
                    .borrow_mut()
                    .push(self.register(&entry.topic, entry.primary.clone()));
                let secondary = self.parent.base.options().public_data_only != Some(true);
                if secondary {
                    entry
                        .trade_ids
                        .borrow_mut()
                        .push(self.register(&entry.secondary, entry.trade.clone()));
                }
                subscription(
                    self.host.as_ref(),
                    "subscribe",
                    std::slice::from_ref(&entry.topic),
                    false,
                )?;
                if secondary {
                    subscription(
                        self.host.as_ref(),
                        "subscribe",
                        std::slice::from_ref(&entry.secondary),
                        false,
                    )?;
                }
                self.set_topic(&entry.topic);
                if secondary {
                    self.set_topic(&entry.secondary);
                }
            }
        } else {
            let weak = Rc::downgrade(self);
            self.host.set_timeout(
                100,
                Rc::new(move || {
                    let owner = weak.upgrade();
                    let entry = entry.clone();
                    Box::pin(async move {
                        owner
                            .ok_or_else(|| failure("public socket owner released".into()))?
                            .wait_candle(entry)
                    })
                }),
            )?;
        }
        Ok(())
    }
    // ts: ApexOmniPublicWebsocket.listenOrderBook
    pub fn listen_order_book(
        self: &Rc<Self>,
        symbol: &str,
        callback: Rc<dyn Fn(OrderBook) -> Result<(), ExchangeError>>,
    ) -> Result<Cleanup, ExchangeError> {
        let topic = format!("orderBook200.H.{symbol}");
        let book = Rc::new(RefCell::new(OrderBook {
            bids: Vec::new(),
            asks: Vec::new(),
        }));
        let state = book.clone();
        let handler: Handler = Rc::new(move |json| {
            let kind = property(json, "type")?;
            let mut next = state.borrow().clone();
            if kind == &J::String("snapshot".into()) {
                next.bids.clear();
                next.asks.clear();
            }
            if kind == &J::String("snapshot".into()) || kind == &J::String("delta".into()) {
                let data = get(json, "data");
                for (key, bids) in [("a", false), ("b", true)] {
                    let rows = get(data, key);
                    if truthy(rows) {
                        for row in crate::data::array(rows).map_err(failure)? {
                            let J::Array(values) = row else {
                                return Err(failure(
                                    "typed order-book level requires an array".into(),
                                ));
                            };
                            let level = OrderBookOrders {
                                price: float(values.first().unwrap_or(&J::Undefined))?,
                                amount: float(values.get(1).unwrap_or(&J::Undefined))?,
                                total: 0.,
                            };
                            if kind == &J::String("snapshot".into()) {
                                if bids {
                                    next.bids.push(level);
                                } else {
                                    next.asks.push(level);
                                }
                            } else {
                                safe_cex_store::update_order_book::update_order_book(
                                    &mut next, bids, level, true,
                                );
                            }
                        }
                    }
                }
            }
            safe_cex_store::orderbook::calc_order_book_total(&mut next);
            *state.borrow_mut() = next.clone();
            callback(next)
        });
        let entry = Rc::new(BookSubscription {
            topic: topic.clone(),
            handler,
            ids: RefCell::new(Vec::new()),
            timer: Cell::new(None),
            book,
        });
        self.wait_book(entry.clone())?;
        let weak = Rc::downgrade(self);
        Ok(Rc::new(move || {
            let owner = weak
                .upgrade()
                .ok_or_else(|| failure("public socket owner released".into()))?;
            for id in entry.ids.borrow().iter() {
                owner.remove_handler(&topic, *id);
            }
            let last = owner.handlers_for(&topic).is_empty();
            if last {
                owner.delete_topic(&topic);
            }
            owner.host.cleanup_stats(&topic)?;
            entry.book.borrow_mut().bids.clear();
            entry.book.borrow_mut().asks.clear();
            if let Some(timer) = entry.timer.take() {
                owner.host.clear_timeout(timer)?;
            }
            if owner.host.is_connected() && last {
                subscription(
                    owner.host.as_ref(),
                    "unsubscribe",
                    std::slice::from_ref(&topic),
                    false,
                )?;
            }
            Ok(())
        }))
    }
    fn wait_book(self: &Rc<Self>, entry: Rc<BookSubscription>) -> Result<(), ExchangeError> {
        if self.host.is_connected() {
            if !self.host.is_disposed() {
                entry
                    .ids
                    .borrow_mut()
                    .push(self.register(&entry.topic, entry.handler.clone()));
                subscription(
                    self.host.as_ref(),
                    "subscribe",
                    std::slice::from_ref(&entry.topic),
                    false,
                )?;
                self.set_topic(&entry.topic);
            }
        } else {
            let weak = Rc::downgrade(self);
            let target = entry.clone();
            let timer = self.host.set_timeout(
                100,
                Rc::new(move || {
                    let owner = weak.upgrade();
                    let entry = target.clone();
                    Box::pin(async move {
                        owner
                            .ok_or_else(|| failure("public socket owner released".into()))?
                            .wait_book(entry)
                    })
                }),
            )?;
            entry.timer.set(Some(timer));
        }
        Ok(())
    }
}
struct CandleSubscription {
    topic: String,
    secondary: String,
    primary: Handler,
    trade: Handler,
    primary_ids: RefCell<Vec<u64>>,
    trade_ids: RefCell<Vec<u64>>,
}
struct BookSubscription {
    topic: String,
    handler: Handler,
    ids: RefCell<Vec<u64>>,
    timer: Cell<Option<u64>>,
    book: Rc<RefCell<OrderBook>>,
}
