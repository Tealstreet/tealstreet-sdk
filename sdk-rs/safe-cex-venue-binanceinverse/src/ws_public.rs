//! COIN-M public socket. Imported Binance dispatch and depth algorithms keep their owners.
use crate::{Rt, Shared};
use safe_cex_exchange::{
    js_runtime::{
        ops,
        promise::await_js,
        scope::scope_of,
        sockets,
        value::{Js, Thrown, arg, from_typed, to_json},
    },
    websocket::BaseWebSocket,
};
use safe_cex_types::OrderBook;
use safe_cex_venue_binance::{
    orderbook,
    protocol_types::BinanceOrderBookApplyResult,
    public_dispatch::{Handler, Handlers, dispatch_binance_message},
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub(crate) fn new(parent: &Rt) -> Result<(Js, Rc<BaseWebSocket>), Thrown> {
    let socket = sockets::base_websocket(&parent.shared.sockets, parent.this.clone());
    socket.define("topics", Js::object(vec![]))?;
    socket.define("nextSubscriptionId", Js::Number(2.))?;
    let weak = Rc::downgrade(&parent.shared);
    for name in [
        "connectAndSubscribe",
        "onOpen",
        "subscribe",
        "onMessage",
        "handleTickerStreamEvents",
        "handleMarkPriceStreamEvents",
        "handleBookTickersStreamEvents",
        "listenOHLCV",
        "listenOrderBook",
    ] {
        let weak = weak.clone();
        socket.define(
            name,
            Js::function(name, move |this, args| {
                let rt = scope_of(&weak)?.with_this(this);
                match name {
                    "connectAndSubscribe" => {
                        if !flag(&rt, "isDisposed")? {
                            let network = if rt.path("parent.options.testnet")?.truthy() {
                                "testnet"
                            } else {
                                "livenet"
                            };
                            let ws = rt.call_path(
                                "createWebsocket",
                                vec![rt.global("BASE_WS_URL")?.get(network)?],
                            )?;
                            rt.this.set("ws", ws.clone())?;
                            for (event, member) in [
                                ("open", "onOpen"),
                                ("message", "onMessage"),
                                ("close", "onClose"),
                            ] {
                                call(
                                    &ws,
                                    "addEventListener",
                                    vec![Js::str(event), rt.this.get(member)?],
                                )?;
                            }
                        }
                    }
                    "onOpen" => {
                        if !flag(&rt, "isDisposed")? {
                            let mut topics =
                                vec![Js::str("!ticker@arr"), Js::str("!markPrice@arr@1s")];
                            if !rt.path("parent.options.publicDataOnly")?.truthy() {
                                topics.push(Js::str("!bookTicker"));
                            }
                            send(
                                &rt,
                                Js::object(vec![
                                    ("id", Js::Number(1.)),
                                    ("method", Js::str("SUBSCRIBE")),
                                    ("params", Js::array(topics)),
                                ]),
                            )?;
                            rt.call_path("resetOrderbook", vec![])?;
                            rt.call_path("subscribe", vec![])?;
                        }
                    }
                    "subscribe" => {
                        let topics = rt.this.get("topics")?;
                        for (key, _) in topics.own_entries()? {
                            let count = topics.get_key(&key)?;
                            if !matches!(&count, Js::Number(n) if n.is_finite())
                                || count.to_number()? <= 0.
                            {
                                continue;
                            }
                            subscription(&rt, "SUBSCRIBE", Js::String(key))?;
                        }
                    }
                    "onMessage" => on_message(&rt, arg(&args, 0).get("data")?)?,
                    "handleTickerStreamEvents" => tickers(
                        &rt,
                        arg(&args, 0),
                        &[("last", "c"), ("percentage", "P"), ("volume", "v")],
                        true,
                    )?,
                    "handleMarkPriceStreamEvents" => tickers(
                        &rt,
                        arg(&args, 0),
                        &[
                            ("mark", "p"),
                            ("index", "i"),
                            ("fundingRate", "r"),
                            ("fundingTime", "T"),
                        ],
                        false,
                    )?,
                    "handleBookTickersStreamEvents" => {
                        tickers(&rt, arg(&args, 0), &[("bid", "b"), ("ask", "a")], false)?
                    }
                    "listenOHLCV" => return candles(&rt, arg(&args, 0), arg(&args, 1)),
                    "listenOrderBook" => return book(&rt, arg(&args, 0), arg(&args, 1)),
                    _ => unreachable!(),
                }
                Ok(Js::Undefined)
            }),
        )?;
    }
    for (topic, method) in [
        ("24hrTicker", "handleTickerStreamEvents"),
        ("markPriceUpdate", "handleMarkPriceStreamEvents"),
        ("bookTicker", "handleBookTickersStreamEvents"),
    ] {
        let weak = weak.clone();
        let this = socket.clone();
        let callback = Js::function(method, move |_, args| {
            scope_of(&weak)?
                .with_this(this.clone())
                .call_path(method, vec![arg(&args, 0)])
        });
        socket
            .get("messageHandlers")?
            .set(topic, Js::array(vec![callback]))?;
    }
    let base = sockets::core_of(&parent.shared.sockets, &socket)
        .expect("new socket core")
        .base()?;
    Ok((socket, base))
}
fn flag(rt: &Rt, name: &str) -> Result<bool, Thrown> {
    Ok(rt.this.get(name)?.truthy())
}
fn call(owner: &Js, method: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    owner.get(method)?.call(owner.clone(), args)
}
fn global(rt: &Rt, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
    rt.global(name)?.call(Js::Undefined, args)
}
fn send(rt: &Rt, value: Js) -> Result<(), Thrown> {
    rt.call_path("send", vec![global(rt, "JSON.stringify", vec![value])?])?;
    Ok(())
}
fn subscription(rt: &Rt, method: &str, topic: Js) -> Result<(), Thrown> {
    let id = rt.this.get("nextSubscriptionId")?.to_number()?;
    rt.this.set("nextSubscriptionId", Js::Number(id + 1.))?;
    send(
        rt,
        Js::object(vec![
            ("method", Js::str(method)),
            ("params", Js::array(vec![topic])),
            ("id", Js::Number(id)),
        ]),
    )
}
fn on_message(rt: &Rt, data: Js) -> Result<(), Thrown> {
    if flag(rt, "isDisposed")? {
        return Ok(());
    }
    rt.call_path("trackLastReceived", vec![])?;
    let handlers = Handlers::default();
    for (key, value) in rt.this.get("messageHandlers")?.own_entries()? {
        let handler = if let Some(callbacks) = value.array_values() {
            let callbacks = callbacks
                .into_iter()
                .map(|callback| {
                    let this = rt.this.clone();
                    Rc::new(move |events| {
                        callback
                            .call(this.clone(), vec![events])
                            .map(|_| ())
                            .map_err(|e| Rc::new(e.to_exchange_error()))
                    }) as safe_cex_venue_binance::public_listeners::Parser
                })
                .collect();
            Handler::List(Rc::new(RefCell::new(callbacks)))
        } else {
            Handler::Value(value)
        };
        handlers.own.borrow_mut().insert(
            String::from_utf16(&key)
                .map_err(|_| Thrown::type_error("unsupported native handler identifier"))?,
            handler,
        );
    }
    let Js::String(text) = data else {
        // Same initial property/call boundary as the imported dispatcher.
        call(&data, "indexOf", vec![Js::str("\"e\":\"")])?;
        return Ok(());
    };
    let text = String::from_utf16(&text)
        .map_err(|_| Thrown::type_error("unsupported native frame text"))?;
    dispatch_binance_message(&handlers, &text, |topic| {
        rt.call_path("recordMessageHandlerStats", vec![Js::str(topic)])
            .map(|_| ())
            .map_err(|e| Rc::new(e.to_exchange_error()))
    })
    .map_err(|e| Thrown::from((*e).clone()))
}
fn tickers(rt: &Rt, events: Js, fields: &[(&str, &str)], quote: bool) -> Result<(), Thrown> {
    let by_symbol = rt.path("parent.store.tickersBySymbol")?;
    for event in events.iterable_values()? {
        let ticker = by_symbol.get_property(&event.get("s")?)?;
        if !ticker.truthy() {
            continue;
        }
        let patch = Js::object(vec![]);
        for (target, source) in fields {
            let value = global(rt, "v", vec![event.clone(), Js::str(source)])?;
            patch.set(target, Js::Number(value.parse_float()?))?;
        }
        if quote {
            let q = global(rt, "v", vec![event.clone(), Js::str("q")])?;
            let q = if q.truthy() {
                q
            } else {
                let q = global(rt, "v", vec![event.clone(), Js::str("Q")])?;
                if q.truthy() { q } else { Js::str("0") }
            };
            patch.set("quoteVolume", Js::Number(q.parse_float()?))?;
        }
        rt.call_path("store.updateTicker", vec![ticker, patch])?;
    }
    Ok(())
}
#[derive(Clone)]
struct Listener {
    weak: Weak<Shared>,
    socket: Js,
    topic: Js,
    handler: String,
    parser: Js,
    timeout: Rc<RefCell<Js>>,
    active: Option<Rc<Cell<bool>>>,
}
impl Listener {
    fn rt(&self) -> Result<Rt, Thrown> {
        Ok(scope_of(&self.weak)?.with_this(self.socket.clone()))
    }
    fn subscribe(&self) -> Result<(), Thrown> {
        let rt = self.rt()?;
        if let Some(active) = &self.active {
            if !active.get() || flag(&rt, "isDisposed")? {
                return Ok(());
            }
        }
        if flag(&rt, "isConnected")? {
            let handlers = rt.this.get("messageHandlers")?;
            let old = handlers.get(&self.handler)?;
            let mut values = if old.truthy() {
                old.iterable_values()?
            } else {
                vec![]
            };
            values.push(self.parser.clone());
            handlers.set(&self.handler, Js::array(values))?;
            let topics = rt.this.get("topics")?;
            let count = topics.get_property(&self.topic)?;
            topics.set_key(
                &self.topic.to_js_string()?,
                ops::add(
                    &if count.truthy() {
                        count
                    } else {
                        Js::Number(0.)
                    },
                    &Js::Number(1.),
                )?,
            )?;
            subscription(&rt, "SUBSCRIBE", self.topic.clone())?;
        } else {
            let listener = self.clone();
            let timer = rt.set_timeout(Rc::new(move || listener.subscribe()), 100.);
            *self.timeout.borrow_mut() = timer;
        }
        Ok(())
    }
    fn remove_parser(&self, delete_empty: bool) -> Result<usize, Thrown> {
        let handlers = self.socket.get("messageHandlers")?;
        let old = handlers.get(&self.handler)?;
        let values = if old.truthy() {
            old.iterable_values()?
        } else {
            vec![]
        };
        let values: Vec<_> = values
            .into_iter()
            .filter(|value| !value.strict_equals(&self.parser))
            .collect();
        let len = values.len();
        if delete_empty && len == 0 {
            handlers.delete(&self.handler.encode_utf16().collect::<Vec<_>>());
        } else {
            handlers.set(&self.handler, Js::array(values))?;
        }
        Ok(len)
    }
}
fn candles(rt: &Rt, options: Js, callback: Js) -> Result<Js, Thrown> {
    let symbol = options.get("symbol")?;
    let interval = options.get("interval")?;
    let lower = call(&symbol, "toLowerCase", vec![])?;
    let topic = Js::str(&format!(
        "{}@kline_{}",
        lower.to_display(),
        interval.to_display()
    ));
    let handler = format!("kline.{}.{}", interval.to_display(), symbol.to_display());
    let parser = Js::function("parser", move |_, args| {
        let events = arg(&args, 0).iterable_values()?;
        let json = events.first().cloned().unwrap_or(Js::Undefined);
        let k = json.get("k")?;
        let actual_symbol = if k.is_nullish() {
            Js::Undefined
        } else {
            k.get("s")?
        };
        if options.get("symbol")?.strict_equals(&actual_symbol)
            && !k.is_nullish()
            && k.get("i")?.strict_equals(&options.get("interval")?)
        {
            callback.call(
                Js::Undefined,
                vec![Js::object(vec![
                    ("timestamp", Js::Number(k.get("t")?.to_number()? / 1000.)),
                    ("open", Js::Number(k.get("o")?.parse_float()?)),
                    ("high", Js::Number(k.get("h")?.parse_float()?)),
                    ("low", Js::Number(k.get("l")?.parse_float()?)),
                    ("close", Js::Number(k.get("c")?.parse_float()?)),
                    ("volume", Js::Number(k.get("v")?.parse_float()?)),
                ])],
            )?;
        }
        Ok(Js::Undefined)
    });
    let active = Rc::new(Cell::new(true));
    let unsubscribed = Cell::new(false);
    let listener = Listener {
        weak: Rc::downgrade(&rt.shared),
        socket: rt.this.clone(),
        topic,
        handler,
        parser,
        timeout: Rc::new(RefCell::new(Js::Null)),
        active: Some(active.clone()),
    };
    listener.subscribe()?;
    Ok(Js::function("unlistenOHLCV", move |_, _| {
        if unsubscribed.replace(true) {
            return Ok(Js::Undefined);
        }
        active.set(false);
        let rt = listener.rt()?;
        let timer = listener.timeout.borrow().clone();
        if timer.truthy() {
            rt.clear_timeout(&timer);
        }
        let left = listener.remove_parser(true)?;
        let topics = rt.this.get("topics")?;
        let old = topics.get_property(&listener.topic)?;
        let next = if old.truthy() { old.to_number()? } else { 0. } - 1.;
        if next > 0. {
            topics.set_key(&listener.topic.to_js_string()?, Js::Number(next))?;
        } else {
            topics.delete(&listener.topic.to_js_string()?);
        }
        if flag(&rt, "isConnected")? && left == 0 {
            rt.call_path(
                "cleanupMessageHandlerStats",
                vec![Js::str(&listener.handler)],
            )?;
            subscription(&rt, "UNSUBSCRIBE", listener.topic.clone())?;
        }
        Ok(Js::Undefined)
    }))
}
struct Depth {
    weak: Weak<Shared>,
    socket: Js,
    symbol: Js,
    callback: Js,
    inner: Js,
    book: RefCell<OrderBook>,
    sync: RefCell<orderbook::BinanceOrderBookSyncState>,
    busy: Cell<bool>,
}
impl Depth {
    fn rt(&self) -> Result<Rt, Thrown> {
        Ok(scope_of(&self.weak)?.with_this(self.socket.clone()))
    }
    fn reset(&self, initial: Option<Js>) -> Result<(), Thrown> {
        let mut book = self.book.borrow_mut();
        book.asks.clear();
        book.bids.clear();
        drop(book);
        self.inner
            .set("updates", Js::array(initial.into_iter().collect()))?;
        self.inner.set("isSnapshotLoaded", Js::Bool(false))?;
        orderbook::reset_binance_order_book_sync_state(&mut self.sync.borrow_mut());
        Ok(())
    }
    fn apply(&self, update: &Js) -> Result<BinanceOrderBookApplyResult, Thrown> {
        let value = to_json(update)?;
        let value = value
            .as_object()
            .ok_or_else(|| Thrown::type_error("depth update must be an object"))?;
        orderbook::apply_binance_depth_update(
            &mut self.book.borrow_mut(),
            value,
            &mut self.sync.borrow_mut(),
            true,
        )
        .map_err(Into::into)
    }
    fn emit(&self) -> Result<(), Thrown> {
        safe_cex_store::orderbook::calc_order_book_total(&mut self.book.borrow_mut());
        let book = from_typed(&*self.book.borrow())?;
        self.callback.call(Js::Undefined, vec![book])?;
        Ok(())
    }
    fn fetch(self: &Rc<Self>) -> Result<(), Thrown> {
        if self.busy.replace(true) {
            return Ok(());
        }
        let rt = self.rt()?;
        let state = self.clone();
        rt.clone().spawn(Box::pin(async move {
            let result = async {
                let result = await_js(rt.call_path(
                    "parent.publicXhr.get",
                    vec![
                        rt.global("ENDPOINTS.ORDERBOOK")?,
                        Js::object(vec![(
                            "params",
                            Js::object(vec![
                                ("symbol", state.symbol.clone()),
                                ("limit", Js::Number(1000.)),
                            ]),
                        )]),
                    ],
                )?)
                .await?;
                let data = result.get("data")?;
                if flag(&rt, "isDisposed")? {
                    return Ok(Js::Undefined);
                }
                let rows = |key: &str| -> Result<Vec<Vec<String>>, Thrown> {
                    data.get(key)?
                        .iterable_values()?
                        .into_iter()
                        .map(|row| {
                            row.iterable_values()?
                                .into_iter()
                                .map(|cell| Ok(String::from_utf16_lossy(&cell.to_js_string()?)))
                                .collect()
                        })
                        .collect()
                };
                orderbook::load_binance_order_book_snapshot(
                    &mut state.book.borrow_mut(),
                    &rows("bids")?,
                    &rows("asks")?,
                    data.get("lastUpdateId")?.to_number()?,
                    &mut state.sync.borrow_mut(),
                );
                let buffered = state.inner.get("updates")?.iterable_values()?;
                let updates: Vec<_> = buffered
                    .iter()
                    .map(|v| {
                        to_json(v)?
                            .as_object()
                            .cloned()
                            .ok_or_else(|| Thrown::type_error("depth update must be object"))
                    })
                    .collect::<Result<_, _>>()?;
                let selected = orderbook::get_buffered_binance_depth_updates(
                    &updates,
                    data.get("lastUpdateId")?.to_number()?,
                )
                .map_err(Thrown::from)?;
                state.inner.set("isSnapshotLoaded", Js::Bool(true))?;
                state.inner.set("updates", Js::array(vec![]))?;
                for update in selected {
                    let value = safe_cex_exchange::js_runtime::value::from_json(
                        &serde_json::Value::Object(update.clone()),
                    );
                    if state.apply(&value)? == BinanceOrderBookApplyResult::Gap {
                        state.reset(Some(value))?;
                        let state = state.clone();
                        rt.set_timeout(Rc::new(move || state.fetch()), 0.);
                        return Ok(Js::Undefined);
                    }
                }
                state.emit()?;
                Ok(Js::Undefined)
            }
            .await;
            state.busy.set(false);
            result
        }));
        Ok(())
    }
    fn message(self: &Rc<Self>, data: Js) -> Result<(), Thrown> {
        if !data.get("s")?.strict_equals(&self.symbol) {
            return Ok(());
        }
        let loaded = self.inner.get("isSnapshotLoaded")?.truthy();
        if !loaded && self.inner.get("updates")?.get("length")?.to_number()? == 0. {
            self.reset(Some(data))?;
            self.fetch()?;
            return Ok(());
        }
        if !loaded {
            call(&self.inner.get("updates")?, "push", vec![data])?;
            return Ok(());
        }
        match self.apply(&data)? {
            BinanceOrderBookApplyResult::Gap => {
                self.reset(Some(data))?;
                self.fetch()?;
            }
            BinanceOrderBookApplyResult::Ignored => (),
            BinanceOrderBookApplyResult::Applied => self.emit()?,
        }
        Ok(())
    }
}
fn book(rt: &Rt, symbol: Js, callback: Js) -> Result<Js, Thrown> {
    let inner_id = global(rt, "nanoid", vec![])?;
    let inner = Js::object(vec![
        ("updates", Js::array(vec![])),
        ("isSnapshotLoaded", Js::Bool(false)),
    ]);
    rt.this
        .get("orderBookInnerState")?
        .set_key(&inner_id.to_js_string()?, inner.clone())?;
    let state = Rc::new(Depth {
        weak: Rc::downgrade(&rt.shared),
        socket: rt.this.clone(),
        symbol: symbol.clone(),
        callback,
        inner,
        book: RefCell::new(OrderBook {
            bids: vec![],
            asks: vec![],
        }),
        sync: RefCell::default(),
        busy: Cell::new(false),
    });
    let parser_state = state.clone();
    let parser = Js::function("parser", move |_, args| {
        let data = arg(&args, 0)
            .iterable_values()?
            .into_iter()
            .next()
            .unwrap_or(Js::Undefined);
        parser_state.message(data)?;
        Ok(Js::Undefined)
    });
    let lower = call(&symbol, "toLowerCase", vec![])?;
    let listener = Listener {
        weak: Rc::downgrade(&rt.shared),
        socket: rt.this.clone(),
        topic: Js::str(&format!("{}@depth", lower.to_display())),
        handler: format!("depthUpdate.{}", symbol.to_display()),
        parser,
        timeout: Rc::new(RefCell::new(Js::Null)),
        active: None,
    };
    listener.subscribe()?;
    Ok(Js::function("unlistenOrderBook", move |_, _| {
        listener.remove_parser(false)?;
        state.reset(None)?;
        let rt = listener.rt()?;
        let timer = listener.timeout.borrow().clone();
        if timer.truthy() {
            rt.clear_timeout(&timer);
        }
        let topics = rt.this.get("topics")?;
        let count = topics.get_property(&listener.topic)?;
        if !count.is_undefined() {
            if count.to_number()? <= 1. {
                topics.delete(&listener.topic.to_js_string()?);
            } else {
                topics.set_key(
                    &listener.topic.to_js_string()?,
                    Js::Number(count.to_number()? - 1.),
                )?;
            }
        }
        if flag(&rt, "isConnected")?
            && listener
                .socket
                .get("messageHandlers")?
                .get(&listener.handler)?
                .get("length")?
                .to_number()?
                == 0.
        {
            rt.call_path(
                "cleanupMessageHandlerStats",
                vec![Js::str(&listener.handler)],
            )?;
            subscription(&rt, "UNSUBSCRIBE", listener.topic.clone())?;
        }
        Ok(Js::Undefined)
    }))
}
