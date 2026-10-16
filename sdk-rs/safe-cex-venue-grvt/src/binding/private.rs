//! Private session, command and feed adapters over the shared JS/socket runtime.
use super::runtime::{Host, Shared, thrown, value};
use crate::{
    account_runtime::AccountHost,
    builder_workflow::{JsPromise, JsResult},
    composition::GrvtPrivateWebSocket,
    private_commands::{self, AuthenticationFailureReason, PrivateCommands},
    private_feeds::{self, Predicate, PrivateFeedHost, Visitor},
    private_lifecycle::{self, PrivateLifecycleHost},
    session_host::{Callback, SessionHost},
};
use safe_cex_exchange::{
    js_runtime::{
        ops,
        promise::await_js,
        sockets,
        value::{Js, Thrown, arg},
    },
    runtime::LogSeverity,
};
use std::{
    hash::{Hash, Hasher},
    rc::{Rc, Weak},
};

#[derive(Clone)]
pub(super) struct Key(pub(super) Js);
impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.0.same_value_zero(&other.0)
    }
}
impl Eq for Key {}
impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u8(0)
    }
}
pub(super) struct Private {
    shared: Weak<Shared>,
    pub(super) this: Js,
    state: Rc<PrivateCommands<Js, Key>>,
}
impl Private {
    pub(super) fn new(h: &Host, model: GrvtPrivateWebSocket<Js, Js, Key>) -> Rc<Self> {
        Rc::new(Self {
            shared: Rc::downgrade(&h.0.shared),
            this: model.base,
            state: model.state,
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
    fn parent(&self) -> Result<Js, Thrown> {
        self.this.get("parent")
    }
    fn own(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.this.call_method(name, args)
    }
    fn parent_call(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.parent()?.call_method(name, args)
    }
    fn result(&self, name: &str, args: Vec<Js>) -> JsResult<Js, ()> {
        value(self.own(name, args))?;
        Ok(())
    }
    fn log_value(&self, message: Js, level: Option<LogSeverity>) -> JsResult<Js, ()> {
        let mut args = vec![message];
        if let Some(level) = level {
            args.push(Js::str(level.as_str()));
        }
        value(self.parent_call("log", args))?;
        Ok(())
    }
    fn send(&self, wire: &str) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
        self.own("send", vec![Js::str(wire)])
            .map(|_| ())
            .map_err(|e| e.to_exchange_error())
    }
    fn text(value: &Js) -> Result<String, Thrown> {
        Ok(String::from_utf16_lossy(&value.to_js_string()?))
    }
    fn api(&self, key: &str) -> Result<Option<String>, Thrown> {
        let extras = self.parent()?.get("apiExtras")?;
        if extras.is_nullish() {
            return Ok(None);
        }
        let v = extras.get(key)?;
        if v.truthy() {
            Ok(Some(Self::text(&v)?))
        } else {
            Ok(None)
        }
    }
    fn base_dispose(&self) -> Result<(), Thrown> {
        let h = self.host()?;
        if let Some(core) = sockets::core_of(&h.0.shared.sockets, &self.this)
            && let Some(base) = core.base.get()
        {
            base.dispose();
        }
        Ok(())
    }
    pub(super) fn invoke(self: &Rc<Self>, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.state
            .is_disposed
            .set(self.this.get("isDisposed")?.truthy());
        let a = arg(&args, 0);
        let result = match name {
            "connectAndSubscribe" => private_lifecycle::connect_and_subscribe(self.clone()),
            "checkLastReceived" => private_lifecycle::check_last_received(self.as_ref()),
            "refreshSessionAndReconnect" => {
                private_lifecycle::refresh_session_and_reconnect(self.clone(), reason(&a)?)
            }
            "throttledFetchPositions" => private_lifecycle::throttled_fetch_positions(self.clone()),
            "executePositionFetch" => {
                return Ok(self
                    .host()?
                    .spawn(private_lifecycle::execute_position_fetch(self.clone())));
            }
            "onMessage" => private_feeds::on_message(self.clone(), a),
            "handleOrderUpdate" => private_feeds::handle_order_update(self.clone(), a),
            "handlePositionUpdate" => private_feeds::handle_position_update(self.as_ref(), a),
            "handleFillUpdate" => private_feeds::handle_fill_update(self.as_ref(), a),
            "parseOrder" => return thrown(private_feeds::parse_order(self.as_ref(), a)),
            "parsePosition" => return thrown(private_feeds::parse_position(self.as_ref(), a)),
            "throttledFetchOrders" => private_feeds::throttled_fetch_orders(self.clone()),
            "fetchThrottledOrders" => {
                return Ok(self
                    .host()?
                    .spawn(private_feeds::fetch_throttled_orders(self.clone())));
            }
            "ping" => self
                .state
                .ping(
                    || {
                        Ok(self
                            .host()
                            .map_err(|e| e.to_exchange_error())?
                            .0
                            .shared
                            .runtime
                            .monotonic_now_ms())
                    },
                    |wire| {
                        self.this
                            .set("pingAt", Js::Number(self.state.ping_at.get()))
                            .map_err(|e| e.to_exchange_error())?;
                        self.send(wire)
                    },
                )
                .map_err(|e| Thrown::from(e).value),
            "getWatchdogStaleThresholdMs" => {
                return Ok(Js::Number(
                    private_commands::get_watchdog_stale_threshold_ms(),
                ));
            }
            "onOpen" => self
                .state
                .on_open(
                    |text, level| {
                        thrown(self.log_value(Js::str(text), level))
                            .map_err(|e| e.to_exchange_error())
                    },
                    || {
                        self.own("authenticate", vec![])
                            .map(|_| ())
                            .map_err(|e| e.to_exchange_error())
                    },
                )
                .map_err(|e| Thrown::from(e).value),
            "authenticate" => private_commands::authenticate(
                || self.api("sessionCookie").ok().flatten(),
                || self.api("accountId").ok().flatten(),
                || {
                    self.parent()
                        .and_then(|p| p.get("options"))
                        .and_then(|v| v.get("testnet"))
                        .is_ok_and(|v| v.truthy())
                },
                |text, level| {
                    thrown(self.log_value(Js::str(text), level)).map_err(|e| e.to_exchange_error())
                },
                |_, wire| self.send(wire),
            )
            .map_err(|e| Thrown::from(e).value),
            "resetRetryBudgetAfterHealthySession" => self
                .state
                .reset_retry_budget_after_healthy_session(|| {
                    Ok(self.host().map_err(|e| e.to_exchange_error())?.0.now())
                })
                .map_err(|e| Thrown::from(e).value),
            "failClosedForAuthentication" => self
                .state
                .fail_closed_for_authentication(
                    match reason(&a)? {
                        "proxy authentication rejected" => {
                            AuthenticationFailureReason::ProxyRejected
                        }
                        "GRVT error 1000" => AuthenticationFailureReason::GrvtError1000,
                        _ => AuthenticationFailureReason::RefreshFailed,
                    },
                    |message| {
                        self.parent_call("emitAndLogError", vec![Js::str(message)])
                            .map(|_| ())
                            .map_err(|e| e.to_exchange_error())
                    },
                    || {
                        self.own("dispose", vec![])
                            .map(|_| ())
                            .map_err(|e| e.to_exchange_error())
                    },
                )
                .map_err(|e| Thrown::from(e).value),
            "subscribe" => private_commands::subscribe(
                || {
                    self.parent()
                        .and_then(|p| p.get("grvtTradingAccountId"))
                        .ok()
                        .filter(Js::truthy)
                        .and_then(|v| Self::text(&v).ok())
                },
                |text, level| {
                    thrown(self.log_value(Js::str(text), level)).map_err(|e| e.to_exchange_error())
                },
                |stream, feed| {
                    self.state
                        .subscribe_to_stream(stream, feed, |_, wire| self.send(wire))
                },
            )
            .map_err(|e| Thrown::from(e).value),
            "subscribeToStream" => {
                let feed: Vec<String> =
                    safe_cex_exchange::js_runtime::value::to_typed(&arg(&args, 1))?;
                let id = self
                    .state
                    .subscribe_to_stream(
                        &Self::text(&a)?,
                        Rc::new(std::cell::RefCell::new(feed)),
                        |_, wire| self.send(wire),
                    )
                    .map_err(Thrown::from)?;
                return Ok(Js::Number(id));
            }
            "dispose" => self
                .state
                .dispose(
                    Js::truthy,
                    |timer| {
                        self.host()
                            .map_err(|e| e.to_exchange_error())?
                            .0
                            .clear_timeout(timer);
                        Ok(())
                    },
                    || self.base_dispose().map_err(|e| e.to_exchange_error()),
                )
                .map_err(|e| Thrown::from(e).value),
            _ => {
                return Err(Thrown::error(
                    "Error",
                    &format!("unknown GRVT private member {name}"),
                ));
            }
        };
        thrown(result)?;
        Ok(Js::Undefined)
    }
}
fn reason(v: &Js) -> Result<&'static str, Thrown> {
    match Private::text(v)?.as_str() {
        "proxy authentication rejected" => Ok("proxy authentication rejected"),
        "GRVT error 1000" => Ok("GRVT error 1000"),
        "session refresh failed" => Ok("session refresh failed"),
        _ => Err(Thrown::type_error("Unknown GRVT session refresh reason")),
    }
}
impl SessionHost for Private {
    type Value = Js;
    fn get(&self, v: &Js, key: &str) -> JsResult<Js> {
        value(v.get(key))
    }
    fn set(&self, v: &Js, key: &str, field: Js) -> JsResult<Js, ()> {
        value(v.set(key, field))
    }
    fn call(&self, f: Js, receiver: Js, args: Vec<Js>) -> JsResult<Js> {
        value(f.call(receiver, args))
    }
    fn function(&self, cb: Callback<Js>) -> Js {
        Js::function("grvtPrivateCallback", move |_, args| {
            thrown(cb(arg(&args, 0)))
        })
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn define(&self, v: &Js, key: &str, field: Js) -> JsResult<Js, ()> {
        value(v.define(key, field))
    }
    fn array(&self, values: Vec<Js>) -> Js {
        Js::array(values)
    }
    fn spread(&self, target: &Js, source: Js) -> JsResult<Js, ()> {
        if !source.is_nullish() {
            for (k, v) in value(source.own_entries())? {
                value(target.define_key(&k, v))?;
            }
        }
        Ok(())
    }
    fn string(&self, v: &str) -> Js {
        Js::str(v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn boolean(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn is_string(&self, v: &Js) -> bool {
        v.is_string()
    }
    fn is_function(&self, v: &Js) -> bool {
        v.is_callable()
    }
    fn equals_number(&self, v: &Js, n: f64) -> JsResult<Js, bool> {
        Ok(v.strict_equals(&Js::Number(n)))
    }
    fn template(&self, parts: Vec<Js>) -> JsResult<Js> {
        let mut units = vec![];
        for v in parts {
            units.extend(value(v.to_js_string())?.iter().copied());
        }
        Ok(Js::utf16(&units))
    }
    fn error(&self, message: Js) -> JsResult<Js> {
        value(Self::text(&message)).map(|v| Thrown::error("Error", &v).value)
    }
    fn reject(&self, error: Js) -> Js {
        self.host()
            .expect("live runtime")
            .spawn(Box::pin(async move { Err(error) }))
    }
    fn start(&self, task: JsPromise<Js>) -> JsPromise<Js> {
        match self.host() {
            Ok(h) => SessionHost::start(&h, task),
            Err(e) => Box::pin(async move { Err(e.value) }),
        }
    }
    fn promise(&self, task: JsPromise<Js>) -> Js {
        self.host().expect("live runtime").spawn(task)
    }
    fn await_value(&self, v: Js) -> JsPromise<Js> {
        Box::pin(async move { value(await_js(v).await) })
    }
}
impl PrivateLifecycleHost for Private {
    type FillKey = Key;
    fn state(&self) -> &PrivateCommands<Js, Key> {
        &self.state
    }
    fn now(&self) -> JsResult<Js, f64> {
        Ok(value(self.host())?.0.now())
    }
    fn last_received(&self) -> f64 {
        self.this
            .get("lastReceived")
            .and_then(|v| v.to_number())
            .unwrap_or(f64::NAN)
    }
    fn set_last_received(&self, v: f64) {
        self.this
            .set("lastReceived", Js::Number(v))
            .expect("own data field")
    }
    fn last_received_timer(&self) -> Js {
        self.this.get("lastReceivedTimer").expect("own data field")
    }
    fn ping_sent(&self) -> bool {
        self.this.get("pingSent").is_ok_and(|v| v.truthy())
    }
    fn set_ping_sent(&self, v: bool) {
        self.this
            .set("pingSent", Js::Bool(v))
            .expect("own data field")
    }
    fn iso_date(&self, v: f64) -> JsResult<Js> {
        let date = value(
            value(self.host())?
                .global_call("dayjs", vec![Js::Number(v)])
                .map_err(Thrown::new),
        )?;
        value(date.call_method("toISOString", vec![]))
    }
    fn log(&self, message: Js, level: LogSeverity) -> JsResult<Js, ()> {
        self.log_value(message, Some(level))
    }
    fn clear_timeout(&self, timer: Js) -> JsResult<Js, ()> {
        value(self.host())?.0.clear_timeout(&timer);
        Ok(())
    }
    fn ping(&self) -> JsResult<Js, ()> {
        self.result("ping", vec![])
    }
    fn reconnect(&self) -> JsResult<Js, JsPromise<Js>> {
        let pending = value(self.own("reconnect", vec![]))?;
        Ok(SessionHost::await_value(self, pending))
    }
    fn routing_host(&self) -> JsResult<Js> {
        let h = value(self.host())?;
        let name = safe_cex_types::ExchangeName::Grvt;
        let options = &h.0.shared.extra.options;
        let route =
            h.0.shared
                .runtime
                .transport()
                .ws_routing(&name, &safe_cex_transport::RoutingOptions::from(options));
        Ok(route.ws_private.host.map_or(Js::Undefined, |v| Js::str(&v)))
    }
    fn socket(&self) -> Js {
        self.this.get("ws").expect("own socket field")
    }
    fn set_socket(&self, v: Js) {
        self.this.set("ws", v).expect("own socket field")
    }
    fn create_socket(&self, host: Js) -> JsResult<Js> {
        value(self.own("createWebsocket", vec![host]))
    }
    fn listener(&self, name: &str) -> JsResult<Js> {
        value(self.this.get(match name {
            "open" => "onOpen",
            "message" => "onMessage",
            _ => "onClose",
        }))
    }
    fn emit_error(&self, message: Js) -> JsResult<Js, ()> {
        value(self.parent_call("emitAndLogError", vec![message]))?;
        Ok(())
    }
    fn reset_retry_budget(&self) -> JsResult<Js, ()> {
        self.result("resetRetryBudgetAfterHealthySession", vec![])
    }
    fn fail_closed(&self, reason: &'static str) -> JsResult<Js, ()> {
        self.result("failClosedForAuthentication", vec![Js::str(reason)])
    }
    fn authenticate(&self) -> JsResult<Js, JsPromise<Js>> {
        let pending = value(self.parent_call("authenticate", vec![]))?;
        Ok(SessionHost::await_value(self, pending))
    }
    fn api_extras(&self) -> JsResult<Js> {
        value(value(self.parent())?.get("apiExtras"))
    }
    fn schedule(&self, callback: Rc<dyn Fn()>, delay: f64) -> JsResult<Js> {
        Ok(value(self.host())?.0.set_timeout(
            Rc::new(move || {
                callback();
                Ok(())
            }),
            delay,
        ))
    }
    fn fetch_positions(&self) -> JsResult<Js, JsPromise<Js>> {
        let pending = value(self.parent_call("fetchPositions", vec![]))?;
        Ok(SessionHost::await_value(self, pending))
    }
    fn store(&self) -> JsResult<Js> {
        value(self.this.get("store"))
    }
    fn sync_positions(&self, positions: Js) -> JsResult<Js, ()> {
        value(self.parent_call("syncBalanceUpnlFromPositions", vec![positions]))?;
        Ok(())
    }
}
impl PrivateFeedHost for Private {
    type Value = Js;
    type FillKey = Key;
    fn state(&self) -> &PrivateCommands<Js, Key> {
        &self.state
    }
    fn start(&self, task: JsPromise<Js>) -> JsPromise<Js> {
        SessionHost::start(self, task)
    }
    fn property(&self, v: &Js, key: &str) -> JsResult<Js> {
        value(v.get(key))
    }
    fn indexed(&self, v: &Js, key: &Js) -> JsResult<Js> {
        value(v.get_key(&value(key.to_js_string())?))
    }
    fn string(&self, v: &str) -> Js {
        Js::str(v)
    }
    fn number(&self, v: f64) -> Js {
        Js::Number(v)
    }
    fn boolean(&self, v: bool) -> Js {
        Js::Bool(v)
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn null(&self) -> Js {
        Js::Null
    }
    fn truthy(&self, v: &Js) -> bool {
        v.truthy()
    }
    fn nullish(&self, v: &Js) -> bool {
        v.is_nullish()
    }
    fn is_undefined(&self, v: &Js) -> bool {
        v.is_undefined()
    }
    fn is_number(&self, v: &Js) -> bool {
        matches!(v, Js::Number(_))
    }
    fn is_string(&self, v: &Js) -> bool {
        v.is_string()
    }
    fn is_array(&self, v: &Js) -> bool {
        v.is_array()
    }
    fn equal(&self, a: &Js, b: &Js) -> bool {
        a.strict_equals(b)
    }
    fn loose_number_equal(&self, v: &Js, n: f64) -> JsResult<Js, bool> {
        fn equal(v: &Js, n: f64) -> Result<bool, Thrown> {
            Ok(match v {
                Js::Undefined | Js::Null => false,
                Js::Number(v) => *v == n,
                Js::BigInt(v) => {
                    n.is_finite()
                        && n.fract() == 0.0
                        && v.to_string() == format!("{:.0}", if n == 0.0 { 0.0 } else { n })
                }
                Js::String(_) | Js::Bool(_) => v.to_number()? == n,
                _ => equal(&ops::to_primitive(v)?, n)?,
            })
        }
        value(equal(v, n))
    }
    fn number_value(&self, v: &Js) -> JsResult<Js, f64> {
        AccountHost::number_value(&value(self.host())?, v.clone())
    }
    fn parse_float(&self, v: &Js) -> JsResult<Js, f64> {
        AccountHost::parse_float(&value(self.host())?, v.clone())
    }
    fn parse_int(&self, v: &Js) -> JsResult<Js, f64> {
        AccountHost::parse_int(&value(self.host())?, v.clone())
    }
    fn decimal_pattern(&self, v: &Js) -> JsResult<Js, bool> {
        crate::trading_runtime::TradingHost::decimal_pattern(&value(self.host())?, v)
    }
    fn to_string(&self, v: &Js) -> JsResult<Js> {
        AccountHost::js_string(&value(self.host())?, v.clone())
    }
    fn template(&self, parts: Vec<Js>) -> JsResult<Js> {
        SessionHost::template(self, parts)
    }
    fn uppercase(&self, v: &Js, _expression: &'static str) -> JsResult<Js> {
        value(v.call_method("toUpperCase", vec![]))
    }
    fn keys_join(&self, v: &Js) -> JsResult<Js> {
        AccountHost::keys_join(&value(self.host())?, v.clone())
    }
    fn object(&self, fields: Vec<(&'static str, Js)>) -> Js {
        Js::object(fields)
    }
    fn define(&self, v: &Js, key: &'static str, field: Js) -> JsResult<Js, ()> {
        value(v.define(key, field))
    }
    fn spread(&self, target: &Js, source: &Js) -> JsResult<Js, ()> {
        SessionHost::spread(self, target, source.clone())
    }
    fn find(&self, array: &Js, predicate: &mut Predicate<'_, Js>) -> JsResult<Js> {
        let len = value(array.get("length"))?;
        let len = value(len.to_number())? as usize;
        for i in 0..len {
            let item = value(array.get(&i.to_string()))?;
            if predicate(item.clone())? {
                return Ok(item);
            }
        }
        Ok(Js::Undefined)
    }
    fn filter(&self, array: &Js, predicate: &mut Predicate<'_, Js>) -> JsResult<Js> {
        let len = value(array.get("length"))?;
        let len = value(len.to_number())? as usize;
        let mut out = vec![];
        for i in 0..len {
            let key = i.to_string();
            if array.has_own(&key) {
                let item = value(array.get(&key))?;
                if predicate(item.clone())? {
                    out.push(item)
                }
            }
        }
        Ok(Js::array(out))
    }
    fn for_each(&self, array: &Js, visitor: &mut Visitor<'_, Js>) -> JsResult<Js, ()> {
        let len = value(array.get("length"))?;
        let len = value(len.to_number())? as usize;
        for i in 0..len {
            let key = i.to_string();
            if array.has_own(&key) {
                visitor(value(array.get(&key))?)?;
            }
        }
        Ok(())
    }
    fn append_spread(&self, array: &Js, v: Js) -> JsResult<Js> {
        let host = value(self.host())?;
        let result = Js::array(vec![]);
        AccountHost::append_iterable(&host, &result, array.clone())?;
        AccountHost::append_iterable(&host, &result, v)?;
        Ok(result)
    }
    fn store(&self) -> JsResult<Js> {
        value(self.this.get("store"))
    }
    fn json_parse(&self, data: &Js) -> JsResult<Js> {
        value(self.host())?.global_call("JSON.parse", vec![data.clone()])
    }
    fn now(&self) -> JsResult<Js, f64> {
        PrivateLifecycleHost::now(self)
    }
    fn virtual_now(&self) -> JsResult<Js, f64> {
        AccountHost::virtual_now(&value(self.host())?)
    }
    fn position_id(&self, symbol: &Js) -> JsResult<Js> {
        AccountHost::position_id(&value(self.host())?, symbol.clone())
    }
    fn trigger_metadata(&self, feed: &Js) -> JsResult<Js> {
        AccountHost::trigger_metadata(&value(self.host())?, feed.clone())
    }
    fn time_in_force(&self, feed: &Js) -> JsResult<Js> {
        AccountHost::time_in_force(&value(self.host())?, feed.clone())
    }
    fn order_creation_time(&self, metadata: &Js) -> JsResult<Js> {
        value(self.parent_call("getOrderCreationTimeMs", vec![metadata.clone()]))
    }
    fn record_nonce(&self, id: Js, real: Js, time: Js) -> JsResult<Js, ()> {
        value(self.parent_call("recordNonce", vec![id, real, time]))?;
        Ok(())
    }
    fn fill_key(&self, v: &Js) -> Key {
        Key(v.clone())
    }
    fn log(&self, message: Js, severity: Option<LogSeverity>) -> JsResult<Js, ()> {
        self.log_value(message, severity)
    }
    fn emit_error(&self, message: Js) -> JsResult<Js, ()> {
        PrivateLifecycleHost::emit_error(self, message)
    }
    fn emit_fill(&self, fill: Js) -> JsResult<Js, ()> {
        let emitter = value(value(self.parent())?.get("emitter"))?;
        value(emitter.call_method("emit", vec![Js::str("fill"), fill]))?;
        Ok(())
    }
    fn record_stats(&self, topic: &'static str) -> JsResult<Js, ()> {
        self.result("recordMessageHandlerStats", vec![Js::str(topic)])
    }
    fn track_received(&self) -> JsResult<Js, ()> {
        self.result("trackLastReceived", vec![])
    }
    fn pong(&self) -> JsResult<Js, ()> {
        self.result("handlePongEvent", vec![])
    }
    fn subscribe(&self) -> JsResult<Js, ()> {
        self.result("subscribe", vec![])
    }
    fn refresh_session(&self, reason: &'static str) -> JsResult<Js, ()> {
        self.result("refreshSessionAndReconnect", vec![Js::str(reason)])
    }
    fn update_order_status(&self, selector: Js, status: Js) -> JsResult<Js, ()> {
        let store = value(self.this.get("store"))?;
        value(store.call_method(
            "updateOrder",
            vec![selector, Js::object(vec![("status", status)])],
        ))?;
        Ok(())
    }
    fn remove_order(&self, selector: Js) -> JsResult<Js, ()> {
        let store = value(self.this.get("store"))?;
        value(store.call_method("removeOrder", vec![selector]))?;
        Ok(())
    }
    fn add_or_update_order(&self, order: Js) -> JsResult<Js, ()> {
        let store = value(self.this.get("store"))?;
        value(store.call_method("addOrUpdateOrder", vec![order]))?;
        Ok(())
    }
    fn update_position(&self, existing: Js, changes: Js) -> JsResult<Js, ()> {
        let store = value(self.this.get("store"))?;
        value(store.call_method("updatePosition", vec![existing, changes]))?;
        Ok(())
    }
    fn update_store(&self, changes: Js) -> JsResult<Js, ()> {
        let store = value(self.this.get("store"))?;
        value(store.call_method("update", vec![changes]))?;
        Ok(())
    }
    fn sync_upnl(&self) -> JsResult<Js, ()> {
        value(self.parent_call("syncBalanceUpnlFromPositions", vec![]))?;
        Ok(())
    }
    fn throttle_positions(&self) -> JsResult<Js, ()> {
        self.result("throttledFetchPositions", vec![])
    }
    fn schedule(&self, callback: private_feeds::TimerCallback, delay: f64) -> JsResult<Js> {
        PrivateLifecycleHost::schedule(self, callback, delay)
    }
    fn clear_timeout(&self, timer: &Js) -> JsResult<Js, ()> {
        PrivateLifecycleHost::clear_timeout(self, timer.clone())
    }
    fn fetch_orders(&self) -> JsResult<Js, JsPromise<Js>> {
        let pending = value(self.parent_call("fetchOrders", vec![]))?;
        Ok(SessionHost::await_value(self, pending))
    }
}

#[cfg(test)]
mod numeric_comparison_tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn native_numeric_comparison_keeps_zero_coercion_and_original_errors() {
        let host = Private {
            shared: Weak::new(),
            this: Js::Undefined,
            state: Rc::new(PrivateCommands::default()),
        };
        let equal =
            |value: Js, number| PrivateFeedHost::loose_number_equal(&host, &value, number).unwrap();
        assert!(
            equal(Js::BigInt(Rc::new("0".parse().unwrap())), -0.0),
            "GRVT BIGINT ZERO equality must match source abstract equality"
        );
        assert!(equal(Js::BigInt(Rc::new("0".parse().unwrap())), 0.0));
        assert!(equal(Js::BigInt(Rc::new("1000".parse().unwrap())), 1000.0));
        assert!(equal(Js::str("1e3"), 1000.0));
        assert!(equal(Js::str("0x3e8"), 1000.0));
        assert!(!equal(Js::str("1000tail"), 1000.0));
        assert!(equal(Js::BigInt(Rc::new("7".parse().unwrap())), 7.0));
        assert!(!equal(Js::BigInt(Rc::new("7".parse().unwrap())), 7.5));
        assert!(!equal(
            Js::BigInt(Rc::new("7".parse().unwrap())),
            f64::INFINITY
        ));
        assert!(!equal(Js::Undefined, 0.0));
        assert!(!equal(Js::Null, 0.0));
        assert!(equal(Js::Number(-0.0), 0.0));
        assert!(!equal(Js::Number(f64::NAN), 0.0));
        assert!(equal(Js::Bool(false), 0.0));
        assert!(equal(Js::Bool(true), 1.0));
        assert!(equal(Js::str("07"), 7.0));
        assert!(!equal(Js::str("7tail"), 7.0));

        let calls = Rc::new(Cell::new(0));
        let observed = calls.clone();
        let object = Js::object(vec![(
            "valueOf",
            Js::function("numericValue", move |_, _| {
                observed.set(observed.get() + 1);
                Ok(Js::Number(7.0))
            }),
        )]);
        assert!(equal(object, 7.0));
        assert_eq!(
            calls.get(),
            1,
            "native comparison must preserve coercion effects"
        );

        let marker = Js::object(vec![("code", Js::str("coercion marker"))]);
        let original = marker.clone();
        let object = Js::object(vec![(
            "valueOf",
            Js::function("throwingValue", move |_, _| {
                Err(Thrown::new(marker.clone()))
            }),
        )]);
        let error = PrivateFeedHost::loose_number_equal(&host, &object, 7.0).unwrap_err();
        assert!(
            error.same_value_zero(&original),
            "native coercion must preserve thrown object identity"
        );
    }
}

#[cfg(test)]
mod accepted_clone_tests {
    use super::*;

    #[test]
    fn key_clone_retains_value_identity_and_same_value_zero() {
        let original = Key(Js::object(vec![("marker", Js::Number(7.0))]));
        let cloned = original.clone();
        assert!(
            original == cloned,
            "GRVT Key clone must retain object identity"
        );
        cloned.0.set("marker", Js::Number(9.0)).unwrap();
        let Js::Number(marker) = original.0.get("marker").unwrap() else {
            panic!("GRVT Key clone lost the native field");
        };
        assert_eq!(marker.to_bits(), 9.0_f64.to_bits());
        for value in [Js::Number(f64::NAN), Js::Number(-0.0), Js::str("key")] {
            let key = Key(value);
            assert!(key == key.clone(), "GRVT Key cloned equality");
        }
    }
}
