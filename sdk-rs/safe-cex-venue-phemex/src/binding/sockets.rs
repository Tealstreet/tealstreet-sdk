use super::rt::{Host, optional, rows};
use crate::{private_session as private, public_routing as public, websocket_data as ws};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    scope::scope_of,
    sockets,
    value::{Js, Thrown, arg},
};
use std::{cell::RefCell, rc::Rc};
pub(crate) fn create(parent: &Host, private: bool) -> Result<Js, Thrown> {
    let socket = sockets::base_websocket(&parent.rt.shared.sockets, parent.rt.this.clone());
    let mut h = parent.child(socket.clone());
    let base_dispose = socket.get("dispose")?;
    socket.define("baseDispose", base_dispose)?;
    let weak = Rc::downgrade(&parent.rt.shared);
    for name in [
        "connectAndSubscribe",
        "onOpen",
        "onMessage",
        "ping",
        "dispose",
        "auth",
        "syncPositionsAndBalance",
        "schedulePositionsAndBalanceSync",
        "subscribe",
        "updateSubscriptionLimitWarning",
        "dispatchTopic",
        "getKlineInterval",
        "handleTickerStreamEvents",
        "listenOHLCV",
        "listenOrderBook",
    ] {
        let weak = weak.clone();
        socket.define(
            name,
            Js::function(name, move |this, a| {
                let mut h = Host {
                    rt: scope_of(&weak)?.with_this(this),
                };
                match (private, name) {
                    (true, "connectAndSubscribe") => private::connect_and_subscribe(&mut h)?,
                    (false, "connectAndSubscribe") => public::connect_and_subscribe(&mut h)?,
                    (true, "onOpen") => private::on_open(&mut h)?,
                    (false, "onOpen") => public::on_open(&mut h)?,
                    (true, "ping") => private::ping(&mut h)?,
                    (false, "ping") => public::ping(&mut h)?,
                    (true, "dispose") => private::dispose(&mut h)?,
                    (false, "dispose") => {
                        h.call("baseDispose", vec![])?;
                    }
                    (true, "auth") => {
                        let rt = h.rt.clone();
                        return Ok(rt.spawn(Box::pin(async move {
                            private::auth(&mut h).await?;
                            Ok(Js::Undefined)
                        })));
                    }
                    (true, "syncPositionsAndBalance") => {
                        let started = private::begin_sync(&mut h);
                        let rt = h.rt.clone();
                        return Ok(rt.spawn(Box::pin(async move {
                            if let Ok(started) = started {
                                let _ = private::finish_sync(&mut h, started).await;
                            }
                            Ok(Js::Undefined)
                        })));
                    }
                    (true, "schedulePositionsAndBalanceSync") => {
                        private::schedule_positions_and_balance_sync(
                            &mut h,
                            (!arg(&a, 0).is_undefined()).then(|| arg(&a, 0)),
                        )?
                    }
                    (true, "onMessage") => ws::private_on_message(&mut h, arg(&a, 0).get("data")?)?,
                    (false, "onMessage") => public::on_message(&mut h, arg(&a, 0).get("data")?)?,
                    (false, "subscribe") => public::subscribe(&mut h)?,
                    (false, "updateSubscriptionLimitWarning") => {
                        public::update_subscription_limit_warning(&mut h)?
                    }
                    (false, "dispatchTopic") => public::dispatch_topic(
                        &mut h,
                        &arg(&a, 0).to_js_string()?,
                        arg(&a, 1),
                        arg(&a, 2).truthy(),
                    )?,
                    (false, "getKlineInterval") => {
                        return Ok(public::get_kline_interval(&mut h, arg(&a, 0))?
                            .map_or(Js::Undefined, |s| Js::utf16(&s)));
                    }
                    (false, "handleTickerStreamEvents") => {
                        ws::handle_ticker_stream_events(&mut h, arg(&a, 0))?
                    }
                    (false, "listenOHLCV") => {
                        return listen_candles(&mut h, arg(&a, 0), arg(&a, 1));
                    }
                    (false, "listenOrderBook") => {
                        return listen_book(&mut h, arg(&a, 0), arg(&a, 1));
                    }
                    _ => {
                        return Err(Thrown::type_error(&format!(
                            "unknown Phemex socket member {name}"
                        )));
                    }
                }
                Ok(Js::Undefined)
            }),
        )?;
    }
    socket.define("requestId", Js::Number(1200.0))?;
    socket.define("orderBookInnerState", Js::object(vec![]))?;
    socket.define(
        "resetOrderbook",
        Js::function("resetOrderbook", |this, _| {
            for (_, state) in this.get("orderBookInnerState")?.own_entries()? {
                state.set("updates", Js::array(vec![]))?;
                state.set("isSnapshotLoaded", Js::Bool(false))?;
            }
            Ok(Js::Undefined)
        }),
    )?;
    if private {
        socket.define("positionsAndBalanceSyncTimeout", Js::Undefined)?;
        socket.define("positionsAndBalanceSyncSymbols", Js::array(vec![]))?;
        socket.define("positionsAndBalanceSyncDebounceMs", Js::Number(1000.0))?;
    } else {
        let state = ws::PublicSocket::new(&mut h)?;
        for (k, v) in [
            (
                "hasWarnedSubscriptionLimit",
                Js::Bool(state.has_warned_subscription_limit),
            ),
            ("orderBookSnapshotErrorLogAt", state.snapshot_error_log_at),
            ("messageHandlers", state.message_handlers),
            ("topics", state.topics),
            ("klineIntervalByResolution", Js::Undefined),
        ] {
            socket.define(k, v)?;
        }
    }
    Ok(socket)
}
fn listener(h: &Host, event: &str, member: &'static str) -> Result<(), Thrown> {
    let this = h.rt.this.clone();
    let callback = Js::function(member, move |_, a| this.call_method(member, a));
    h.get("ws")?
        .call_method("addEventListener", vec![Js::str(event), callback])?;
    Ok(())
}
fn next_id(h: &Host) -> Result<f64, Thrown> {
    let n = h.get("requestId")?.to_number()?;
    h.rt.this.set("requestId", Js::Number(n + 1.0))?;
    Ok(n)
}
fn parse_frame(h: &Host, data: Js) -> Result<Js, Thrown> {
    // jsonParse handles native HOW payloads and parses only text frames.
    if matches!(data, Js::String(_)) {
        h.global("JSON.parse", vec![data])
    } else {
        Ok(data)
    }
}
fn serialize(h: &Host, v: Js) -> Result<Vec<u16>, Thrown> {
    Ok(h.global("JSON.stringify", vec![v])?
        .to_js_string()?
        .to_vec())
}
fn parent(h: &Host) -> Result<Host, Thrown> {
    Ok(h.child(h.get("parent")?))
}
pub(crate) fn call(h: &mut Host, name: &'static str, a: Vec<Js>) -> Result<Js, Thrown> {
    let renamed = match name {
        "received" => "trackLastReceived",
        "diagnostic" => "emitWsDiagnosticReceive",
        "pong" => "handlePongEvent",
        "stats" => "recordMessageHandlerStats",
        "cleanup" => "cleanupMessageHandlerStats",
        "sync" => "schedulePositionsAndBalanceSync",
        other => other,
    };
    match name {
        "boundTicker" => {
            let this = h.rt.this.clone();
            Ok(Js::function("handleTickerStreamEvents", move |_, a| {
                this.call_method("handleTickerStreamEvents", a)
            }))
        }
        "parseFrame" => parse_frame(h, arg(&a, 0)),
        "nextId" => Ok(Js::Number(next_id(h)?)),
        "orderStatus" => {
            super::methods::table("ORDER_STATUS")?.get_key(&arg(&a, 0).to_js_string()?)
        }
        "fill" => parent(h)?
            .get("emitter")?
            .call_method("emit", vec![Js::str("fill"), arg(&a, 0)]),
        "error" => {
            parent(h)?.emit(arg(&a, 0))?;
            Ok(Js::Undefined)
        }
        "sendWithLog" | "send" => {
            let s = h.global("JSON.stringify", vec![arg(&a, 0)])?;
            h.call(name, vec![s])
        }
        "warning" => {
            public::update_subscription_limit_warning(h)?;
            Ok(Js::Undefined)
        }
        "handlerCount" => {
            let v = h
                .get("messageHandlers")?
                .get_key(&arg(&a, 0).to_js_string()?)?;
            Ok(Js::Number(if v.is_undefined() {
                0.0
            } else {
                v.get("length")?.to_number()?
            }))
        }
        "addHandler" => {
            let handlers = h.get("messageHandlers")?;
            let key = arg(&a, 0).to_js_string()?;
            let mut list = handlers.get_key(&key)?;
            if list.is_undefined() {
                list = Js::array(vec![]);
                handlers.set_key(&key, list.clone())?;
            }
            list.push(arg(&a, 1))?;
            Ok(Js::Undefined)
        }
        "removeHandler" => {
            let handlers = h.get("messageHandlers")?;
            let key = arg(&a, 0).to_js_string()?;
            let list = handlers.get_key(&key)?;
            if list.is_array() {
                let items = rows(&list)?
                    .into_iter()
                    .filter(|f| !f.strict_equals(&arg(&a, 1)))
                    .collect();
                handlers.set_key(&key, Js::array(items))?;
            }
            Ok(Js::Undefined)
        }
        "callback" => arg(&a, 0).call(Js::Undefined, vec![arg(&a, 1)]),
        "clearTimer" => {
            h.rt.clear_timeout(&arg(&a, 0));
            Ok(Js::Undefined)
        }
        "log" => parent(h)?.call("log", a),
        "dateNow" => Ok(Js::Number(h.rt.now())),
        "processBook" => {
            ws::process_order_book_update(h, arg(&a, 0), arg(&a, 1))?;
            Ok(Js::Undefined)
        }
        "calcTotal" => h.global("calcOrderBookTotal", a),
        "updateByPrice" => h.global(
            "updateOrderBook",
            vec![
                arg(&a, 0),
                arg(&a, 1),
                Js::object(vec![("price", arg(&a, 2)), ("amount", arg(&a, 3))]),
            ],
        ),
        "greater" => Ok(Js::Bool(safe_cex_exchange::js_runtime::ops::greater_than(
            &arg(&a, 0),
            &arg(&a, 1),
        )?)),
        "push" => {
            arg(&a, 0).push(arg(&a, 1))?;
            Ok(Js::Undefined)
        }
        "newParser" => h.get("__newParser"),
        _ => h.call(renamed, a),
    }
}
fn listen_candles(h: &mut Host, opts: Js, cb: Js) -> Result<Js, Thrown> {
    let subscription: Rc<RefCell<Option<ws::CandleSubscription<Js>>>> = Rc::default();
    let target = subscription.clone();
    let host = h.clone();
    let parser = Js::function("parseCandle", move |_, a| {
        if let Some(s) = target.borrow().clone() {
            s.parse(&mut host.clone(), arg(&a, 0))?;
        }
        Ok(Js::Undefined)
    });
    h.rt.this.define("__newParser", parser)?;
    let s = ws::listen_ohlcv(h, opts, cb)?;
    *subscription.borrow_mut() = Some(s.clone());
    h.rt.this
        .delete(&"__newParser".encode_utf16().collect::<Vec<_>>())?;
    let host = h.clone();
    Ok(Js::function("unsubscribe", move |_, _| {
        s.dispose(&mut host.clone())?;
        Ok(Js::Undefined)
    }))
}
fn listen_book(h: &mut Host, symbol: Js, cb: Js) -> Result<Js, Thrown> {
    let subscription: Rc<RefCell<Option<ws::BookSubscription<Js>>>> = Rc::default();
    let target = subscription.clone();
    let host = h.clone();
    let parser = Js::function("parseBook", move |_, a| {
        if let Some(s) = target.borrow().clone() {
            s.parse(&mut host.clone(), arg(&a, 0))?;
        }
        Ok(Js::Undefined)
    });
    h.rt.this.define("__newParser", parser)?;
    let s = ws::listen_order_book(h, symbol, cb)?;
    *subscription.borrow_mut() = Some(s.clone());
    h.rt.this
        .delete(&"__newParser".encode_utf16().collect::<Vec<_>>())?;
    let host = h.clone();
    Ok(Js::function("unsubscribe", move |_, _| {
        s.dispose(&mut host.clone())?;
        Ok(Js::Undefined)
    }))
}
impl public::RoutingHost for Host {
    type Value = Js;
    type Socket = Js;
    type Call = Js;
    type Callbacks = Js;
    type Iterator = std::vec::IntoIter<Js>;
    type Callback = Js;
    type Error = Thrown;
    fn disposed(&mut self) -> Result<bool, Thrown> {
        Ok(self.get("isDisposed")?.truthy())
    }
    fn testnet(&mut self) -> Result<bool, Thrown> {
        Ok(parent(self)?.get("options")?.get("testnet")?.truthy())
    }
    fn create(&mut self, url: &'static str) -> Result<Js, Thrown> {
        self.call("createWebsocket", vec![Js::str(url)])
    }
    fn socket(&mut self, s: Js) -> Result<(), Thrown> {
        self.rt.this.set("ws", s)
    }
    fn listener(&mut self, e: &'static str, m: &'static str) -> Result<(), Thrown> {
        listener(self, e, m)
    }
    fn send_target(&mut self) -> Result<Js, Thrown> {
        self.get("send")
    }
    fn serialize(&mut self, r: public::Request) -> Result<Vec<u16>, Thrown> {
        let params = Js::array(
            r.params
                .into_iter()
                .map(|p| match p {
                    public::Parameter::Text(s) => Js::utf16(&s),
                    public::Parameter::Number(n) => Js::Number(n),
                    public::Parameter::Undefined => Js::Undefined,
                })
                .collect(),
        );
        let mut f = vec![("method", Js::utf16(&r.method)), ("params", params)];
        if r.id_first {
            f.insert(0, ("id", Js::Number(r.id)))
        } else {
            f.push(("id", Js::Number(r.id)))
        }
        serialize(self, Js::object(f))
    }
    fn send(&mut self, f: Js, s: Vec<u16>) -> Result<(), Thrown> {
        f.call(self.rt.this.clone(), vec![Js::utf16(&s)])?;
        Ok(())
    }
    fn reset(&mut self) -> Result<(), Thrown> {
        self.call("resetOrderbook", vec![])?;
        Ok(())
    }
    fn subscribe(&mut self) -> Result<(), Thrown> {
        public::subscribe(self)
    }
    fn ping(&mut self) -> Result<(), Thrown> {
        public::ping(self)
    }
    fn topic_keys(&mut self) -> Result<Vec<Vec<u16>>, Thrown> {
        Ok(self
            .get("topics")?
            .own_keys()
            .into_iter()
            .map(|s| s.to_vec())
            .collect())
    }
    fn number_text(&mut self, s: Option<&[u16]>) -> Result<f64, Thrown> {
        s.map_or(Js::Undefined, Js::utf16).to_number()
    }
    fn topic_values(&mut self) -> Result<Vec<f64>, Thrown> {
        self.get("topics")?
            .own_entries()?
            .into_iter()
            .map(|(_, v)| v.to_number())
            .collect()
    }
    fn warned(&mut self) -> Result<bool, Thrown> {
        Ok(self.get("hasWarnedSubscriptionLimit")?.truthy())
    }
    fn set_warned(&mut self, v: bool) -> Result<(), Thrown> {
        self.rt.this.set("hasWarnedSubscriptionLimit", Js::Bool(v))
    }
    fn warning(&mut self, s: Vec<u16>) -> Result<(), Thrown> {
        parent(self)?.call("log", vec![Js::utf16(&s), Js::str("warning")])?;
        Ok(())
    }
    fn now(&mut self) -> Result<f64, Thrown> {
        Ok(self.rt.now())
    }
    fn set_ping_at(&mut self, n: f64) -> Result<(), Thrown> {
        self.rt.this.set("pingAt", Js::Number(n))
    }
    fn next_request_id(&mut self) -> Result<f64, Thrown> {
        next_id(self)
    }
    fn number_string(&mut self, n: f64) -> Result<Vec<u16>, Thrown> {
        Ok(Js::Number(n).to_js_string()?.to_vec())
    }
    fn callbacks(&mut self, k: &[u16]) -> Result<Option<Js>, Thrown> {
        let v = self.get("messageHandlers")?.get_key(k)?;
        Ok(v.truthy().then_some(v))
    }
    fn nonempty(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.get("length")?.truthy())
    }
    fn stats(&mut self, k: &[u16]) -> Result<(), Thrown> {
        self.call("recordMessageHandlerStats", vec![Js::utf16(k)])?;
        Ok(())
    }
    fn iterate_callbacks(&mut self, v: Js) -> Result<Self::Iterator, Thrown> {
        Ok(rows(&v)?.into_iter())
    }
    fn next_callback(&mut self, i: &mut Self::Iterator) -> Result<Option<Js>, Thrown> {
        Ok(i.next())
    }
    fn invoke(&mut self, f: Js, p: &Js) -> Result<(), Thrown> {
        f.call(Js::Undefined, vec![p.clone()])?;
        Ok(())
    }
    fn number(&mut self, v: Js) -> Result<f64, Thrown> {
        v.to_number()
    }
    fn interval_cache_exists(&mut self) -> Result<bool, Thrown> {
        Ok(!self.get("klineIntervalByResolution")?.is_undefined())
    }
    fn create_interval_cache(&mut self) -> Result<(), Thrown> {
        self.rt
            .this
            .set("klineIntervalByResolution", Js::object(vec![]))
    }
    fn timeframes(&mut self) -> Result<Js, Thrown> {
        parent(self)?.get("timeframes")
    }
    fn timeframe_keys(&mut self, f: &Js) -> Result<Vec<Vec<u16>>, Thrown> {
        Ok(f.own_keys().into_iter().map(|k| k.to_vec()).collect())
    }
    fn timeframe(&mut self, f: &Js, k: &[u16]) -> Result<Js, Thrown> {
        f.get_key(k)
    }
    fn cache_interval(&mut self, n: f64, s: Vec<u16>) -> Result<(), Thrown> {
        self.get("klineIntervalByResolution")?
            .set_key(&Js::Number(n).to_js_string()?, Js::utf16(&s))
    }
    fn cached_interval(&mut self, n: f64) -> Result<Option<Vec<u16>>, Thrown> {
        let v = self
            .get("klineIntervalByResolution")?
            .get_key(&Js::Number(n).to_js_string()?)?;
        Ok(v.as_str().map(|s| s.to_vec()))
    }
    fn received(&mut self) -> Result<(), Thrown> {
        self.call("trackLastReceived", vec![])?;
        Ok(())
    }
    fn parse(&mut self, d: Js) -> Result<Js, Thrown> {
        parse_frame(self, d)
    }
    fn property(&mut self, v: &Js, k: &'static str) -> Result<Js, Thrown> {
        optional(v, k)
    }
    fn truthy(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.truthy())
    }
    fn text(&mut self, v: &Js) -> Result<Option<Vec<u16>>, Thrown> {
        Ok(v.as_str().map(|s| s.to_vec()))
    }
    fn array(&mut self, v: &Js) -> Result<bool, Thrown> {
        Ok(v.is_array())
    }
    fn iterate_values(&mut self, v: Js) -> Result<Self::Iterator, Thrown> {
        Ok(rows(&v)?.into_iter())
    }
    fn next_value(&mut self, i: &mut Self::Iterator) -> Result<Option<Js>, Thrown> {
        Ok(i.next())
    }
    fn index(&mut self, v: &Js, i: usize) -> Result<Js, Thrown> {
        v.get(&i.to_string())
    }
    fn wrap(&mut self, v: Js) -> Result<Js, Thrown> {
        Ok(Js::array(vec![v]))
    }
    fn dispatch(&mut self, k: Vec<u16>, v: Js, each: Option<bool>) -> Result<(), Thrown> {
        public::dispatch_topic(self, &k, v, each.unwrap_or(false))
    }
    fn pong(&mut self) -> Result<(), Thrown> {
        self.call("handlePongEvent", vec![])?;
        Ok(())
    }
}
impl private::SessionHost for Host {
    type Timer = Js;
    type Symbol = Js;
    type Symbols = Js;
    type Socket = Js;
    type Call = Js;
    type Error = Thrown;
    fn timeout(&mut self) -> Result<Option<Js>, Thrown> {
        let v = self.get("positionsAndBalanceSyncTimeout")?;
        Ok(v.truthy().then_some(v))
    }
    fn timeout_value(&mut self) -> Result<Js, Thrown> {
        self.get("positionsAndBalanceSyncTimeout")
    }
    fn clear_timeout(&mut self, t: Js) -> Result<(), Thrown> {
        self.rt.clear_timeout(&t);
        Ok(())
    }
    fn unset_timeout(&mut self) -> Result<(), Thrown> {
        self.rt
            .this
            .set("positionsAndBalanceSyncTimeout", Js::Undefined)
    }
    fn clear_symbols(&mut self) -> Result<(), Thrown> {
        self.rt
            .this
            .set("positionsAndBalanceSyncSymbols", Js::array(vec![]))
    }
    fn base_dispose(&mut self) -> Result<(), Thrown> {
        self.call("baseDispose", vec![])?;
        Ok(())
    }
    fn empty_symbols(&mut self) -> Result<Js, Thrown> {
        Ok(Js::array(vec![]))
    }
    fn symbols_length(&mut self, s: &Js) -> Result<usize, Thrown> {
        Ok(rows(s)?.len())
    }
    fn symbol_at(&mut self, s: &Js, i: usize) -> Result<Option<Js>, Thrown> {
        Ok(s.array_items()
            .ok_or_else(|| Thrown::type_error("symbols.forEach is not a function"))?
            .get(i)
            .cloned()
            .flatten())
    }
    fn add_symbol(&mut self, s: Js) -> Result<(), Thrown> {
        let symbols = self.get("positionsAndBalanceSyncSymbols")?;
        if !rows(&symbols)?.iter().any(|v| v.same_value_zero(&s)) {
            symbols.push(s)?;
        }
        Ok(())
    }
    fn debounce(&mut self) -> Result<f64, Thrown> {
        self.get("positionsAndBalanceSyncDebounceMs")?.to_number()
    }
    fn schedule(&mut self, delay: f64, member: &'static str) -> Result<Js, Thrown> {
        let this = self.rt.this.clone();
        Ok(self.rt.set_timeout(
            Rc::new(move || {
                this.call_method(member, vec![])?;
                Ok(())
            }),
            delay,
        ))
    }
    fn set_timeout(&mut self, t: Js) -> Result<(), Thrown> {
        self.rt.this.set("positionsAndBalanceSyncTimeout", t)
    }
    fn disposed(&mut self) -> Result<bool, Thrown> {
        Ok(self.get("isDisposed")?.truthy())
    }
    fn testnet(&mut self) -> Result<bool, Thrown> {
        Ok(parent(self)?.get("options")?.get("testnet")?.truthy())
    }
    fn create(&mut self, url: &'static str) -> Result<Js, Thrown> {
        self.call("createWebsocket", vec![Js::str(url)])
    }
    fn set_socket(&mut self, s: Js) -> Result<(), Thrown> {
        self.rt.this.set("ws", s)
    }
    fn listener(&mut self, e: &'static str, m: &'static str) -> Result<(), Thrown> {
        listener(self, e, m)
    }
    fn start_auth(&mut self) -> Result<(), Thrown> {
        self.call("auth", vec![])?;
        Ok(())
    }
    fn ping(&mut self) -> Result<(), Thrown> {
        private::ping(self)
    }
    fn now(&mut self) -> Result<f64, Thrown> {
        Ok(self.rt.now())
    }
    fn set_ping_at(&mut self, t: f64) -> Result<(), Thrown> {
        self.rt.this.set("pingAt", Js::Number(t))
    }
    fn send_target(&mut self) -> Result<Js, Thrown> {
        self.get("sendWithLog")
    }
    fn next_id(&mut self) -> Result<f64, Thrown> {
        next_id(self)
    }
    fn send(&mut self, f: Js, s: Vec<u16>) -> Result<(), Thrown> {
        f.call(self.rt.this.clone(), vec![Js::utf16(&s)])?;
        Ok(())
    }
}
fn options(h: &Host) -> Result<Js, Thrown> {
    parent(h)?.get("options")
}
impl private::AuthHost for Host {
    type Value = Js;
    type Signer = Js;
    type Hmac = Js;
    type Request = Js;
    type Reply = Js;
    type Call = Js;
    type Error = Thrown;
    fn clock(&mut self) -> Result<f64, Thrown> {
        self.global("virtualClock.getCurrentTime", vec![])?
            .to_number()
    }
    fn remote(&mut self) -> Result<bool, Thrown> {
        Ok(options(self)?.get("remoteSigner")?.truthy())
    }
    fn signer_target(&mut self) -> Result<Js, Thrown> {
        options(self)?.get("remoteSigner")
    }
    fn nonce(&mut self) -> Result<Js, Thrown> {
        self.global("nanoid", vec![])
    }
    fn account(&mut self) -> Result<Js, Thrown> {
        options(self)?.get("remoteSigner")?.get("accountName")
    }
    fn key(&mut self) -> Result<Js, Thrown> {
        options(self)?.get("key")
    }
    fn key_text(&mut self, k: Js, template: bool) -> Result<Vec<u16>, Thrown> {
        Ok(if !template && k.is_nullish() {
            vec![]
        } else {
            k.to_js_string()?.to_vec()
        })
    }
    fn number_text(&mut self, s: &[u16]) -> Result<f64, Thrown> {
        Js::utf16(s).to_number()
    }
    fn request(
        &mut self,
        id: Js,
        account: Js,
        to_sign: Vec<u16>,
        expiry: f64,
    ) -> Result<Js, Thrown> {
        Ok(Js::object(vec![
            ("id", id),
            ("type", Js::str("ws-auth")),
            ("exchangeName", Js::str("phemex")),
            ("accountName", account),
            (
                "wsAuth",
                Js::object(vec![
                    ("toSign", Js::utf16(&to_sign)),
                    ("outputFormat", Js::str("hex")),
                    (
                        "extraData",
                        Js::object(vec![("expiry", Js::Number(expiry))]),
                    ),
                ]),
            ),
        ]))
    }
    async fn sign(&mut self, s: Js, r: Js) -> Result<Js, Thrown> {
        await_js(&self.rt.exchange_runtime(), s.call_method("sign", vec![r])?).await
    }
    fn reply_error(&mut self, r: &Js) -> Result<bool, Thrown> {
        Ok(r.get("error")?.truthy())
    }
    fn error_text(&mut self, r: &Js) -> Result<Vec<u16>, Thrown> {
        Ok(r.get("error")?.to_js_string()?.to_vec())
    }
    fn emit_error(&mut self, m: Vec<u16>) -> Result<(), Thrown> {
        self.emit(Js::utf16(&m))
    }
    fn signature(&mut self, r: Js) -> Result<Js, Thrown> {
        r.get("wsAuthSignature")
    }
    fn secret(&mut self) -> Result<Js, Thrown> {
        options(self)?.get("secret")
    }
    fn create_hmac(&mut self, s: Js) -> Result<Js, Thrown> {
        self.global("createHmac", vec![Js::str("sha256"), s])
    }
    fn hmac(&mut self, h: Js, m: Vec<u16>) -> Result<Js, Thrown> {
        h.call_method("update", vec![Js::utf16(&m)])?
            .call_method("digest", vec![Js::str("hex")])
    }
    fn send_target(&mut self) -> Result<Js, Thrown> {
        self.get("sendWithLog")
    }
    fn serialize(&mut self, k: Js, s: Js, e: f64, id: f64) -> Result<Vec<u16>, Thrown> {
        serialize(
            self,
            Js::object(vec![
                ("method", Js::str("user.auth")),
                (
                    "params",
                    Js::array(vec![Js::str("API"), k, s, Js::Number(e)]),
                ),
                ("id", Js::Number(id)),
            ]),
        )
    }
    fn send(&mut self, f: Js, s: Vec<u16>) -> Result<(), Thrown> {
        f.call(self.rt.this.clone(), vec![Js::utf16(&s)])?;
        Ok(())
    }
}
impl private::SyncHost for Host {
    type Symbol = Js;
    type Balance = Js;
    type Position = Js;
    type Positions = Js;
    type Account = Js;
    type Id = Js;
    type Error = Thrown;
    type Read = safe_cex_exchange::js_runtime::promise::JsFuture<'static>;
    fn unset_timeout(&mut self) -> Result<(), Thrown> {
        self.rt
            .this
            .set("positionsAndBalanceSyncTimeout", Js::Undefined)
    }
    fn symbols(&mut self) -> Result<Vec<Js>, Thrown> {
        rows(&self.get("positionsAndBalanceSyncSymbols")?)
    }
    fn clear_symbols(&mut self) -> Result<(), Thrown> {
        self.rt
            .this
            .set("positionsAndBalanceSyncSymbols", Js::array(vec![]))
    }
    fn disposed(&mut self) -> Result<bool, Thrown> {
        Ok(self.get("isDisposed")?.truthy())
    }
    type AccountRead = safe_cex_exchange::js_runtime::promise::JsFuture<'static>;
    fn account(&mut self) -> Self::AccountRead {
        let result = parent(self).map(|host| host.invoke("fetchPositionsAndBalance", vec![]));
        Box::pin(async move { result?.await })
    }
    fn parts(&mut self, a: Js) -> Result<(Js, Js), Thrown> {
        Ok((a.get("balance")?, a.get("positions")?))
    }
    fn fetch(&mut self, s: Js) -> Result<Self::Read, Thrown> {
        Ok(parent(self)?.invoke("fetchPositions", vec![Js::array(vec![s])]))
    }
    fn rows(&mut self, p: Js) -> Result<Vec<Js>, Thrown> {
        rows(&p)
    }
    fn id(&mut self, p: &Js) -> Result<Js, Thrown> {
        p.get("id")
    }
    fn same_id(&mut self, a: &Js, b: &Js) -> Result<bool, Thrown> {
        Ok(a.same_value_zero(b))
    }
    fn store_positions(&mut self, p: Vec<Js>) -> Result<(), Thrown> {
        self.get("store")?
            .call_method("addOrUpdatePositionsFull", vec![Js::array(p)])?;
        Ok(())
    }
    fn update(&mut self, b: Js, bl: bool, pl: bool) -> Result<(), Thrown> {
        let loaded = self.store("loaded")?.spread_object()?;
        loaded.define("balance", Js::Bool(bl))?;
        loaded.define("positions", Js::Bool(pl))?;
        self.get("store")?.call_method(
            "update",
            vec![Js::object(vec![("balance", b), ("loaded", loaded)])],
        )?;
        Ok(())
    }
}
