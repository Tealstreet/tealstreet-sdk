//! Concrete host traits and target adapters for the private-socket topic,
//! recovery and throttle bodies plus the exchange's private-stream lifecycle.
use crate::binding::{
    globals::arg,
    rt::{Rt, Shared},
    value::{Js, Thrown, from_typed, to_json, to_typed},
};
use crate::numeric::{FiniteNumberInput, to_finite_number};
use crate::private_recovery::{self, PrivateRecoveryHost};
use crate::private_throttles::{self, FetchKind, FetchThrottleCallback, FetchThrottleHost};
use crate::private_topics::{self, PrivateTopicsHost};
use crate::stream_lifecycle::{
    self, PrivateStreamHealthUpdate, StreamLifecycleHost, StreamSocket, StreamTimeout,
    UndefinedProperty,
};
use crate::stream_timers::{
    self, ForcedWarningOptions, ReasonHealthUpdate, StreamTimerCallback, StreamTimerHost,
};
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_store::StoreUpdate;
use safe_cex_transport::{DirectTransport, RoutingOptions, TransportProvider};
use safe_cex_types::{ExchangeName, ExchangeOptions};
use std::rc::{Rc, Weak};

fn bool_field(this: &Js, name: &str) -> Result<bool, Thrown> {
    Ok(this.get(name)?.truthy())
}

fn number_field(this: &Js, name: &str) -> Result<f64, Thrown> {
    this.get(name)?.to_number()
}

/// `Date.now()` as the globals define it.
fn date_now(rt: &Rt) -> f64 {
    rt.now().floor()
}

/// `void promise` / an unawaited async call: a later rejection is reported the
/// way an unhandled rejection is, never dropped.
fn detach(rt: &Rt, value: Js, source: &'static str) {
    let Js::Promise(promise) = value else {
        return;
    };
    match promise.result() {
        Some(Ok(_)) => {}
        Some(Err(error)) => rt.report(&error, source),
        None => {
            let reporter = rt.clone();
            let _ = rt.spawn(Box::pin(async move {
                if let Err(error) = promise.wait().await {
                    reporter.report(&error, source);
                }
                Ok(Js::Undefined)
            }));
        }
    }
}

/// GetIterator's retained iterator and next method. Collection traversal is owned
/// by the shared runtime, including stable Map/Set insertion positions.
pub(crate) struct JsIter {
    iterator: Js,
    next: Js,
    done: bool,
    in_body: bool,
}

impl Drop for JsIter {
    fn drop(&mut self) {
        // These private-topic loops run to exhaustion or return a body error.
        // IteratorClose runs only for the latter, and that original throw wins
        // over an error from return(). Step/value errors do not close.
        if self.in_body
            && let Ok(close) = self.iterator.get("return")
            && !close.is_nullish()
        {
            let _ = close.call(self.iterator.clone(), vec![]);
        }
    }
}

impl PrivateTopicsHost for Rt {
    type Value = Js;
    type Error = Thrown;
    type Iterator = JsIter;
    fn property(&self, value: &Js, name: &str) -> Result<Js, Thrown> {
        value.get(name)
    }
    fn receiver(&self, name: &str) -> Result<Js, Thrown> {
        self.path(name)
    }
    fn set_receiver(&self, name: &str, value: Js) -> Result<(), Thrown> {
        self.set_path(name, value)
    }
    fn call(&self, name: &str, args: Vec<Js>) -> Result<Js, Thrown> {
        self.call_path(name, args)
    }
    fn truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn nullish(&self, value: &Js) -> bool {
        value.is_nullish()
    }
    fn equal(&self, left: &Js, right: &Js) -> bool {
        left.strict_equals(right)
    }
    fn is_array(&self, value: &Js) -> bool {
        value.is_array()
    }
    fn is_object(&self, value: &Js) -> bool {
        value.type_of() == "object"
    }
    fn is_string(&self, value: &Js) -> bool {
        value.is_string()
    }
    fn undefined(&self) -> Js {
        Js::Undefined
    }
    fn string(&self, value: &[u16]) -> Js {
        Js::utf16(value)
    }
    fn number(&self, value: f64) -> Js {
        Js::Number(value)
    }
    fn boolean(&self, value: bool) -> Js {
        Js::Bool(value)
    }
    fn array(&self, values: Vec<Option<Js>>) -> Js {
        Js::holey_array(values)
    }
    fn length(&self, array: &Js) -> Result<usize, Thrown> {
        match array {
            Js::Array(items) => Ok(items.borrow().len()),
            other => Err(Thrown::type_error(&format!(
                "{} is not an array",
                other.to_display()
            ))),
        }
    }
    fn has_index(&self, array: &Js, index: usize) -> Result<bool, Thrown> {
        match array {
            Js::Array(items) => Ok(matches!(items.borrow().get(index), Some(Some(_)))),
            other => Err(Thrown::type_error(&format!(
                "{} is not an array",
                other.to_display()
            ))),
        }
    }
    fn index(&self, array: &Js, index: usize) -> Result<Js, Thrown> {
        array.get(&index.to_string())
    }
    fn iterator(&self, value: &Js) -> Result<JsIter, Thrown> {
        let iterator = value.iterator()?;
        let next = iterator.get("next")?;
        Ok(JsIter {
            iterator,
            next,
            done: false,
            in_body: false,
        })
    }
    fn next(&self, iterator: &mut JsIter) -> Result<Option<Js>, Thrown> {
        if iterator.done {
            return Ok(None);
        }
        iterator.in_body = false;
        let item = iterator.next.call(iterator.iterator.clone(), vec![])?;
        if !item.is_object() {
            return Err(Thrown::type_error("Iterator result is not an object"));
        }
        if item.get("done")?.truthy() {
            iterator.done = true;
            return Ok(None);
        }
        let value = item.get("value")?;
        iterator.in_body = true;
        Ok(Some(value))
    }
    fn object(&self, fields: Vec<(&str, Js)>) -> Js {
        Js::object(fields)
    }
    fn spread(&self, value: &Js) -> Result<Js, Thrown> {
        value.spread_object()
    }
    fn define(&self, object: &Js, name: &str, value: Js) -> Result<(), Thrown> {
        object.define(name, value)
    }
    fn finite(&self, value: &Js, fallback: f64) -> Result<f64, Thrown> {
        Ok(match value {
            Js::Number(n) => to_finite_number(FiniteNumberInput::Number(*n), fallback),
            Js::String(s) => to_finite_number(
                FiniteNumberInput::String(&String::from_utf16_lossy(s)),
                fallback,
            ),
            _ => to_finite_number(FiniteNumberInput::Other, fallback),
        })
    }
    fn parse_float(&self, value: &Js) -> Result<f64, Thrown> {
        value.parse_float()
    }
    // ts: ORDER_STATUS[normalizedStatus]
    fn status_lookup(&self, normalized: &Js) -> Result<Js, Thrown> {
        self.global("ORDER_STATUS")?
            .get_key(&normalized.to_js_string()?)
    }
    fn account_upnl(&self, fallback: f64) -> Result<f64, Thrown> {
        self.call_path("parent.calculateAccountUpnl", vec![Js::Number(fallback)])?
            .to_number()
    }
    // ts: virtualClock.getCurrentTimeMs()
    fn clock_now(&self) -> Result<f64, Thrown> {
        self.global("virtualClock")?
            .call_method("getCurrentTimeMs", vec![])?
            .to_number()
    }
    fn set_of(&self, values: Js) -> Result<Js, Thrown> {
        self.global("Set")?.construct(vec![values], None)
    }
    fn map_set(&self, map: Js, key: Js, value: f64) -> Result<(), Thrown> {
        map.call_method("set", vec![key, Js::Number(value)])
            .map(|_| ())
    }
}

impl PrivateRecoveryHost for Rt {
    type Error = Thrown;
    fn disposed(&self) -> Result<bool, Thrown> {
        bool_field(&self.this, "isDisposed")
    }
    fn date_now(&self) -> Result<f64, Thrown> {
        Ok(date_now(self))
    }
    fn last_recovery_at(&self) -> Result<f64, Thrown> {
        number_field(&self.this, "lastPrivateStreamRecoveryAt")
    }
    fn recovery_cooldown_ms(&self) -> Result<f64, Thrown> {
        number_field(&self.this, "privateStreamRecoveryCooldownMs")
    }
    fn set_last_recovery_at(&self, value: f64) -> Result<(), Thrown> {
        self.this
            .set("lastPrivateStreamRecoveryAt", Js::Number(value))
    }
    // ts: !!resolveTransportRouting(this.parent.name, this.parent.options).wsPrivate.fallback
    fn resolved_private_fallback_truthy(&self) -> Result<bool, Thrown> {
        let parent = self.this.get("parent")?;
        let routing = self.global("resolveTransportRouting")?.call(
            Js::Undefined,
            vec![parent.get("name")?, parent.get("options")?],
        )?;
        Ok(routing.get("wsPrivate")?.get("fallback")?.truthy())
    }
    fn uses_uta_routing(&self) -> Result<bool, Thrown> {
        bool_field(&self.this.get("parent")?, "usesUtaRouting")
    }
    fn using_fallback(&self) -> Result<bool, Thrown> {
        bool_field(&self.this, "useFallbackUrl")
    }
    fn set_using_fallback(&self, value: bool) -> Result<(), Thrown> {
        self.this.set("useFallbackUrl", Js::Bool(value))
    }
    fn log(&self, message: &[u16], severity: LogSeverity) -> Result<(), Thrown> {
        self.call_path(
            "parent.log",
            vec![Js::utf16(message), Js::str(severity.as_str())],
        )
        .map(|_| ())
    }
    fn reconnect(&self) -> Result<(), Thrown> {
        self.call_path("reconnect", vec![]).map(|_| ())
    }
    // ts: this.auth() — async and not awaited.
    fn auth(&self) -> Result<(), Thrown> {
        let pending = self.call_path("auth", vec![])?;
        detach(self, pending, "BitgetPrivateWebsocket.auth");
        Ok(())
    }
    fn ping(&self) -> Result<(), Thrown> {
        self.call_path("ping", vec![]).map(|_| ())
    }
    fn reconnecting_enabled(&self) -> bool {
        safe_cex_store::config::ENABLE_RECONNECTING_WEBSOCKET
    }
    fn launch_orders_fetch(&self) -> Result<(), Thrown> {
        let pending = self.call_path("executeOrdersFetch", vec![])?;
        detach(self, pending, "BitgetPrivateWebsocket.executeOrdersFetch");
        Ok(())
    }
}

fn fetch_field(
    kind: FetchKind,
    positions: &'static str,
    balance: &'static str,
    orders: &'static str,
) -> &'static str {
    match kind {
        FetchKind::Positions => positions,
        FetchKind::Balance => balance,
        FetchKind::Orders => orders,
    }
}

impl FetchThrottleHost for Rt {
    type Error = Thrown;
    fn date_now(&self) -> Result<f64, Thrown> {
        Ok(date_now(self))
    }
    fn disposed(&self) -> Result<bool, Thrown> {
        bool_field(&self.this, "isDisposed")
    }
    fn scheduled(&self, kind: FetchKind) -> Result<bool, Thrown> {
        bool_field(
            &self.this,
            fetch_field(
                kind,
                "positionFetchScheduled",
                "balanceFetchScheduled",
                "ordersFetchScheduled",
            ),
        )
    }
    fn last_fetch_at(&self, kind: FetchKind) -> Result<f64, Thrown> {
        number_field(
            &self.this,
            fetch_field(
                kind,
                "lastPositionFetchTime",
                "lastBalanceFetchTime",
                "lastOrdersFetchTime",
            ),
        )
    }
    fn throttle_ms(&self, kind: FetchKind) -> Result<f64, Thrown> {
        number_field(
            &self.this,
            fetch_field(
                kind,
                "positionFetchThrottleMs",
                "balanceFetchThrottleMs",
                "ordersFetchThrottleMs",
            ),
        )
    }
    fn set_scheduled(&self, kind: FetchKind, value: bool) -> Result<(), Thrown> {
        self.this.set(
            fetch_field(
                kind,
                "positionFetchScheduled",
                "balanceFetchScheduled",
                "ordersFetchScheduled",
            ),
            Js::Bool(value),
        )
    }
    fn pending(&self, kind: FetchKind) -> Result<bool, Thrown> {
        bool_field(
            &self.this,
            fetch_field(
                kind,
                "hasPendingPositionEvents",
                "hasPendingBalanceEvents",
                "hasPendingOrderEvents",
            ),
        )
    }
    fn set_pending(&self, kind: FetchKind, value: bool) -> Result<(), Thrown> {
        self.this.set(
            fetch_field(
                kind,
                "hasPendingPositionEvents",
                "hasPendingBalanceEvents",
                "hasPendingOrderEvents",
            ),
            Js::Bool(value),
        )
    }
    fn launch(&self, kind: FetchKind) -> Result<(), Thrown> {
        let member = fetch_field(
            kind,
            "executePositionFetch",
            "executeBalanceFetch",
            "executeOrdersFetch",
        );
        let pending = self.call_path(member, vec![])?;
        detach(self, pending, "BitgetPrivateWebsocket.throttledFetch");
        Ok(())
    }
    fn set_timeout(
        &self,
        callback: FetchThrottleCallback<Thrown>,
        delay_ms: f64,
    ) -> Result<(), Thrown> {
        Rt::set_timeout(self, callback, delay_ms);
        Ok(())
    }
}

fn timeout_slot(slot: StreamTimeout) -> &'static str {
    match slot {
        StreamTimeout::WriteCheck => "privateStreamWriteCheckTimeout",
        StreamTimeout::DegradedReconcile => "privateStreamDegradedReconcileTimeout",
    }
}

impl StreamLifecycleHost for Rt {
    type Error = Thrown;
    type Timestamp = Js;
    type Timeout = Js;
    // ts: super.dispose() — the BaseExchange member, bridged to the Rust core.
    fn super_dispose(&self) -> Result<(), Thrown> {
        self.base().dispose()?;
        Ok(())
    }
    fn timeout(&self, slot: StreamTimeout) -> Result<Js, Thrown> {
        self.this.get(timeout_slot(slot))
    }
    fn timeout_truthy(&self, value: &Js) -> bool {
        value.truthy()
    }
    fn clear_timeout(&self, value: Js) -> Result<(), Thrown> {
        Rt::clear_timeout(self, &value);
        Ok(())
    }
    fn unset_timeout(&self, slot: StreamTimeout) -> Result<(), Thrown> {
        self.this.set(timeout_slot(slot), Js::Undefined)
    }
    fn dispose_socket(&self, socket: StreamSocket) -> Result<(), Thrown> {
        let path = match socket {
            StreamSocket::Public => "publicWebsocket.dispose",
            StreamSocket::Private => "privateWebsocket.dispose",
            StreamSocket::SecondsTickers => "secondsTickersWebsocket.dispose",
            StreamSocket::ThirdTickers => "thirdTickersWebsocket.dispose",
        };
        self.call_path(path, vec![]).map(|_| ())
    }
    fn set_degraded(&self, value: bool) -> Result<(), Thrown> {
        self.this.set("isPrivateStreamDegraded", Js::Bool(value))
    }
    fn date_now(&self) -> Result<Js, Thrown> {
        Ok(Js::Number(date_now(self)))
    }
    fn set_last_message_at(&self, value: Js) -> Result<(), Thrown> {
        self.this.set("lastPrivateStreamMessageAt", value)
    }
    fn set_warned(&self, value: bool) -> Result<(), Thrown> {
        self.this
            .set("hasWarnedPrivateStreamDegraded", Js::Bool(value))
    }
    // ts: this.store.updateSocketHealth({ privateDegradedReason: undefined }) — an own
    // undefined property drops the serialized field, so the key is removed.
    fn update_socket_health(&self, update: PrivateStreamHealthUpdate) -> Result<(), Thrown> {
        let store = self.shared.store.clone();
        match update.private_degraded_reason {
            UndefinedProperty::Absent => store
                .update_socket_health(&serde_json::Map::new())
                .map_err(store_error),
            UndefinedProperty::PresentUndefined => {
                let mut health = store
                    .socket_health()?
                    .and_then(|current| current.as_object().cloned())
                    .unwrap_or_default();
                health.shift_remove("privateDegradedReason");
                store
                    .update(
                        StoreUpdate {
                            socket_health: Some(serde_json::Value::Object(health)),
                            ..StoreUpdate::default()
                        },
                        false,
                    )
                    .map_err(store_error)
            }
        }
    }
    fn stop_reconcile(&self) -> Result<(), Thrown> {
        self.call_path("stopPrivateStreamDegradedReconcile", vec![])
            .map(|_| ())
    }
    fn reconcile_running_truthy(&self) -> Result<bool, Thrown> {
        bool_field(&self.this, "privateStreamDegradedReconcileRunning")
    }
    fn launch_rest_reconcile(&self) -> Result<(), Thrown> {
        let pending = self.call_path("reconcilePrivateStateFromRest", vec![])?;
        detach(
            self,
            pending,
            "BitgetExchange.reconcilePrivateStateFromRest",
        );
        Ok(())
    }
}

fn store_error(e: safe_cex_store::StoreMutationError) -> Thrown {
    Thrown::from(safe_cex_exchange::runtime::ExchangeError::from(e))
}

/// JS `a > b`: both operands to primitives, strings compare by code unit, else ToNumber.
fn js_greater(a: &Js, b: &Js) -> Result<bool, Thrown> {
    let primitive = |v: &Js| -> Result<Js, Thrown> {
        Ok(match v {
            Js::Array(_)
            | Js::Object(_)
            | Js::Function(_)
            | Js::Promise(_)
            | Js::Map(_)
            | Js::Set(_) => Js::String(v.to_js_string()?),
            other => other.clone(),
        })
    };
    let (a, b) = (primitive(a)?, primitive(b)?);
    if let (Js::String(a), Js::String(b)) = (&a, &b) {
        return Ok(a[..] > b[..]);
    }
    Ok(a.to_number()? > b.to_number()?)
}

impl StreamTimerHost for Rt {
    type Value = Js;
    fn is_undefined(value: &Js) -> bool {
        value.is_undefined()
    }
    fn string_literal(value: &'static str) -> Js {
        Js::str(value)
    }
    fn boolean_literal(value: bool) -> Js {
        Js::Bool(value)
    }
    fn value_truthy(value: &Js) -> bool {
        value.truthy()
    }
    // ts: this.store.updateSocketHealth({ privateDegradedReason: reason })
    fn update_reason_health(&self, update: ReasonHealthUpdate<'_, Js>) -> Result<(), Thrown> {
        let mut patch = serde_json::Map::new();
        patch.insert(
            "privateDegradedReason".into(),
            to_json(update.private_degraded_reason)?,
        );
        self.shared
            .store
            .update_socket_health(&patch)
            .map_err(store_error)
    }
    fn log_warning(&self, reason: &Js) -> Result<(), Thrown> {
        self.call_path(
            "log",
            vec![reason.clone(), Js::str(LogSeverity::Warning.as_str())],
        )
        .map(|_| ())
    }
    fn recover_missing_private_stream(&self, reason: &Js) -> Result<(), Thrown> {
        self.call_path(
            "privateWebsocket.recoverMissingPrivateStream",
            vec![reason.clone()],
        )
        .map(|_| ())
    }
    fn start_reconcile(&self) -> Result<(), Thrown> {
        self.call_path("startPrivateStreamDegradedReconcile", vec![])
            .map(|_| ())
    }
    fn warned_truthy(&self) -> Result<bool, Thrown> {
        bool_field(&self.this, "hasWarnedPrivateStreamDegraded")
    }
    fn emit_warning(&self, reason: &Js, options: ForcedWarningOptions) -> Result<(), Thrown> {
        let options = Js::object(vec![("forceToast", Js::Bool(options.force_toast))]);
        self.call_path(
            "emitter.emit",
            vec![Js::str("warning"), reason.clone(), options],
        )
        .map(|_| ())
    }
    fn disposed_truthy(&self) -> Result<bool, Thrown> {
        bool_field(&self.this, "isDisposed")
    }
    fn degraded_truthy(&self) -> Result<bool, Thrown> {
        bool_field(&self.this, "isPrivateStreamDegraded")
    }
    fn set_timeout(
        &self,
        callback: StreamTimerCallback<Thrown>,
        delay_ms: u64,
    ) -> Result<Js, Thrown> {
        Ok(Rt::set_timeout(self, callback, delay_ms as f64))
    }
    fn assign_timeout(&self, slot: StreamTimeout, value: Js) -> Result<(), Thrown> {
        self.this.set(timeout_slot(slot), value)
    }
    fn last_message_at(&self) -> Result<Js, Thrown> {
        self.this.get("lastPrivateStreamMessageAt")
    }
    fn is_later(&self, current: &Js, captured: &Js) -> Result<bool, Thrown> {
        js_greater(current, captured)
    }
    fn mark_degraded(&self, reason: Js, force_toast: Js) -> Result<(), Thrown> {
        self.call_path("markPrivateStreamDegraded", vec![reason, force_toast])
            .map(|_| ())
    }
}

/// `resolveTransportRouting(name, options)` from transport/registry. The Bitget
/// binding installs no tunnel transport, so the default direct provider resolves.
fn resolve_transport_routing(args: &[Js]) -> Result<Js, Thrown> {
    let name: ExchangeName = to_typed(&arg(args, 0))?;
    let options: ExchangeOptions =
        safe_cex_exchange::js_runtime::base_exchange::exchange_options(&arg(args, 1))?;
    from_typed(&DirectTransport.ws_routing(&name, &RoutingOptions::from(&options)))
}

pub(crate) fn register(globals: &Js, _weak: Weak<Shared>) {
    if !globals.has_own("resolveTransportRouting") {
        let _ = globals.define(
            "resolveTransportRouting",
            Js::function("resolveTransportRouting", |_, a| {
                resolve_transport_routing(&a)
            }),
        );
    }
}

type Body = Rc<dyn Fn(&Rt, Vec<Js>) -> Result<Js, Thrown>>;

fn unit(result: Result<(), Thrown>) -> Result<Js, Thrown> {
    result.map(|_| Js::Undefined)
}

fn adapter(owner: &str, name: &str) -> Option<Body> {
    let body: Body = match (owner, name) {
        ("BitgetExchange", "dispose") => Rc::new(|rt, _| unit(stream_lifecycle::dispose(rt))),
        ("BitgetExchange", "markPrivateStreamMessageReceived") => {
            Rc::new(|rt, _| unit(stream_lifecycle::mark_private_stream_message_received(rt)))
        }
        ("BitgetExchange", "markPrivateStreamDegraded") => Rc::new(|rt, a| {
            unit(stream_timers::mark_private_stream_degraded(
                rt,
                arg(&a, 0),
                arg(&a, 1),
            ))
        }),
        ("BitgetExchange", "startPrivateStreamDegradedReconcile") => Rc::new(|rt, _| {
            unit(stream_lifecycle::start_private_stream_degraded_reconcile(
                rt,
            ))
        }),
        ("BitgetExchange", "stopPrivateStreamDegradedReconcile") => {
            Rc::new(|rt, _| unit(stream_lifecycle::stop_private_stream_degraded_reconcile(rt)))
        }
        ("BitgetExchange", "schedulePrivateStreamDegradedReconcile") => Rc::new(|rt, _| {
            unit(stream_timers::schedule_private_stream_degraded_reconcile(
                Rc::new(rt.clone()),
            ))
        }),
        ("BitgetExchange", "schedulePrivateStreamWriteCheck") => Rc::new(|rt, _| {
            unit(stream_timers::schedule_private_stream_write_check(Rc::new(
                rt.clone(),
            )))
        }),
        ("BitgetPrivateWebsocket", "markWsPositions") => Rc::new(|rt, a| {
            unit(private_topics::mark_ws_positions(
                rt,
                arg(&a, 0),
                arg(&a, 1),
            ))
        }),
        ("BitgetPrivateWebsocket", "onOpen") => {
            Rc::new(|rt, _| unit(private_recovery::on_open(rt)))
        }
        // ts: `${reason}` — ToString of the argument is side-effect free here.
        ("BitgetPrivateWebsocket", "recoverMissingPrivateStream") => Rc::new(|rt, a| {
            let reason = arg(&a, 0).to_js_string()?;
            unit(private_recovery::recover_missing_private_stream(
                rt, &reason,
            ))
        }),
        ("BitgetPrivateWebsocket", "handleOrderTopic") => {
            Rc::new(|rt, a| unit(private_topics::handle_order_topic(rt, arg(&a, 0))))
        }
        ("BitgetPrivateWebsocket", "handleUtaOrderTopic") => {
            Rc::new(|rt, a| unit(private_topics::handle_uta_order_topic(rt, arg(&a, 0))))
        }
        ("BitgetPrivateWebsocket", "handleAlgoOrdersTopic") => {
            Rc::new(|rt, a| unit(private_topics::handle_algo_orders_topic(rt, arg(&a, 0))))
        }
        ("BitgetPrivateWebsocket", "handlePositionsTopic") => {
            Rc::new(|rt, a| unit(private_topics::handle_positions_topic(rt, arg(&a, 0))))
        }
        ("BitgetPrivateWebsocket", "handleUtaPositionsTopic") => {
            Rc::new(|rt, a| unit(private_topics::handle_uta_positions_topic(rt, arg(&a, 0))))
        }
        ("BitgetPrivateWebsocket", "handleAccountTopic") => {
            Rc::new(|rt, a| unit(private_topics::handle_account_topic(rt, arg(&a, 0))))
        }
        ("BitgetPrivateWebsocket", "handleUtaAccountTopic") => {
            Rc::new(|rt, a| unit(private_topics::handle_uta_account_topic(rt, arg(&a, 0))))
        }
        ("BitgetPrivateWebsocket", "throttledFetchPositions") => Rc::new(|rt, _| {
            unit(private_throttles::throttled_fetch_positions(Rc::new(
                rt.clone(),
            )))
        }),
        ("BitgetPrivateWebsocket", "throttledFetchBalance") => Rc::new(|rt, _| {
            unit(private_throttles::throttled_fetch_balance(Rc::new(
                rt.clone(),
            )))
        }),
        ("BitgetPrivateWebsocket", "throttledFetchOrders") => Rc::new(|rt, _| {
            unit(private_throttles::throttled_fetch_orders(Rc::new(
                rt.clone(),
            )))
        }),
        _ => return None,
    };
    Some(body)
}

/// Every member here is a synchronous TS arrow field bound to the receiver
/// captured at construction.
pub(crate) fn bind(
    rt: &Rt,
    owner: &str,
    name: &str,
    _targets: &[&str],
) -> Result<Option<Js>, Thrown> {
    let Some(body) = adapter(owner, name) else {
        return Ok(None);
    };
    let rt = rt.clone();
    Ok(Some(Js::function(name, move |_, args| body(&rt, args))))
}
