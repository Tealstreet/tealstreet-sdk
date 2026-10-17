//! LighterPublicWebsocket class composition plus its close, REST-fallback,
//! order-book recovery and listener bodies. Every JavaScript operation (Get/Set/
//! delete with ToPropertyKey, binding reads, Call/Construct, closures, iteration,
//! template ToString, relational comparison, await and async return adoption)
//! is a required live host operation, performed in source evaluation order.
//! Out-of-type clock/counter primitives take JS ToNumber/`+`; objects throw
//! TypeError (no host ToPrimitive). Log severities are the source
//! LogSeverity string values, as in the neighbouring public-socket bodies.
use crate::{
    public_ingress::{self, IngressOps},
    sdk_account_api::{
        AccountValue, JsBinary, JsRelation, primitive_binary, primitive_increment,
        primitive_relation, primitive_to_number,
    },
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{SigningResult, TransactionSigningError},
    ws_public,
    ws_public_cleanup::{self, PublicCleanupHost},
    ws_public_readiness::{self, PublicReadinessHost},
    ws_public_subscriptions::{self, PublicSubscriptionHost},
    ws_public_watchdogs::{self, PublicWatchdogHost, WatchdogOptions},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const EARLY_CLOSE_WINDOW_MS: f64 = 12_000.0;
const DIRECT_ROUTE_FAILURES_BEFORE_READONLY: f64 = 2.0;
const PROXY_ROUTE_FAILURES_BEFORE_DIRECT_FALLBACK: f64 = 3.0;
const PROXY_DIRECT_FALLBACK_FAILURE_COOLDOWN_MS: f64 = 120_000.0;
const RATE_LIMIT_RECONNECT_DELAY_MS: f64 = 30_000.0;
const ORDER_BOOK_RECOVERY_COOLDOWN_MS: f64 = 45_000.0;
const ORDER_BOOK_INITIAL_DATA_TIMEOUT_MS: f64 = 1_500.0;
const ORDER_BOOK_REST_FALLBACK_403_COOLDOWN_MS: f64 = 60_000.0;
const TICKER_REST_FALLBACK_403_COOLDOWN_MS: f64 = 60_000.0;
const SUBSCRIBE_RETRY_MS: f64 = 100.0;

/// Fresh JavaScript closure body: receives the actual call arguments.
pub type SocketCallback = Rc<dyn Fn(&[AccountValue]) -> SigningResult<AccountValue>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relational {
    Less,
    Greater,
    LessEqual,
}
/// Live operations for one socket owner. Keys use full ToPropertyKey; binding
/// reads return the module import/global value without calling it.
pub trait PublicSocketHost {
    fn owner(&self) -> AccountValue;
    fn get(&self, object: &AccountValue, key: &AccountValue) -> SigningResult<AccountValue>;
    fn set(
        &self,
        object: &AccountValue,
        key: &AccountValue,
        value: AccountValue,
    ) -> SigningResult<()>;
    fn delete(&self, object: &AccountValue, key: &AccountValue) -> SigningResult<()>;
    fn binding(&self, name: &str) -> SigningResult<AccountValue>;
    /// Call with explicit receiver, including the noncallable TypeError.
    fn invoke(
        &self,
        callee: &AccountValue,
        this: &AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue>;
    fn construct(
        &self,
        callee: &AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue>;
    fn function(&self, callback: SocketCallback) -> SigningResult<AccountValue>;
    /// Fresh ordinary object literal with ordered own data properties.
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue>;
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue>;
    /// Fresh object after source CopyDataProperties (object spread).
    fn copy_object(&self, source: &AccountValue) -> SigningResult<AccountValue>;
    /// CreateDataPropertyOrThrow on a literal under construction.
    fn define(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()>;
    /// GetIterator/for-of with live yields and IteratorClose on visitor errors.
    fn for_of(
        &self,
        value: &AccountValue,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()>;
    /// Template-literal ToString, including original coercion throws.
    fn template(&self, value: &AccountValue) -> SigningResult<Vec<u16>>;
    /// Abstract relational comparison with source operand order.
    fn relational(
        &self,
        op: Relational,
        left: &AccountValue,
        right: &AccountValue,
    ) -> SigningResult<bool>;
    fn error_value(&self, error: TransactionSigningError) -> AccountValue;
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError;
    fn runtime(&self) -> Rc<dyn TransactionRuntime>;
    /// Await: PromiseResolve plus queued continuation.
    fn await_value(&self, value: AccountValue) -> SigningResult<NativePromise<AccountValue>>;
    /// Async return resolution, including thenable adoption.
    fn resolve_return(&self, value: AccountValue, resolver: PromiseResolver<AccountValue>);
    /// Bind this exact native Promise into the realm without copying it.
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue;
}

fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> AccountValue {
    AccountValue::String(u(s))
}
fn num(value: &AccountValue) -> SigningResult<f64> {
    primitive_to_number(value)
}
fn number(n: f64) -> AccountValue {
    AccountValue::Number(n)
}
fn is_number(value: &AccountValue, n: f64) -> bool {
    matches!(value, AccountValue::Number(v) if *v == n)
}
fn is_object(value: &AccountValue) -> bool {
    matches!(value, AccountValue::Object(_))
}
fn strict_eq(a: &AccountValue, b: &AccountValue) -> bool {
    match (a, b) {
        (AccountValue::Undefined, AccountValue::Undefined)
        | (AccountValue::Null, AccountValue::Null)
        | (AccountValue::HtmlDda, AccountValue::HtmlDda) => true,
        (AccountValue::Boolean(a), AccountValue::Boolean(b)) => a == b,
        (AccountValue::Number(a), AccountValue::Number(b)) => a == b,
        (AccountValue::String(a), AccountValue::String(b)) => a == b,
        (AccountValue::BigInt(a), AccountValue::BigInt(b)) => a == b,
        (AccountValue::Object(a), AccountValue::Object(b)) => Rc::ptr_eq(a, b),
        (AccountValue::Function(a), AccountValue::Function(b)) => a.ptr_eq(b),
        (AccountValue::Symbol(a), AccountValue::Symbol(b)) => a.ptr_eq(b),
        _ => false,
    }
}
fn get(h: &dyn PublicSocketHost, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
    h.get(object, &text(key))
}
fn own(h: &dyn PublicSocketHost, key: &str) -> SigningResult<AccountValue> {
    get(h, &h.owner(), key)
}
fn set_own(h: &dyn PublicSocketHost, key: &str, value: AccountValue) -> SigningResult<()> {
    h.set(&h.owner(), &text(key), value)
}
fn optional(
    h: &dyn PublicSocketHost,
    value: &AccountValue,
    key: &str,
) -> SigningResult<AccountValue> {
    if value.is_nullish() {
        Ok(AccountValue::Undefined)
    } else {
        get(h, value, key)
    }
}
fn method(
    h: &dyn PublicSocketHost,
    receiver: &AccountValue,
    key: &str,
    args: impl FnOnce() -> SigningResult<Vec<AccountValue>>,
) -> SigningResult<AccountValue> {
    let callee = get(h, receiver, key)?;
    let args = args()?;
    h.invoke(&callee, receiver, &args)
}
fn own_call(
    h: &dyn PublicSocketHost,
    key: &str,
    args: &[AccountValue],
) -> SigningResult<AccountValue> {
    let args = args.to_vec();
    method(h, &h.owner(), key, || Ok(args))
}
fn global_call(
    h: &dyn PublicSocketHost,
    name: &str,
    args: impl FnOnce() -> SigningResult<Vec<AccountValue>>,
) -> SigningResult<AccountValue> {
    let callee = h.binding(name)?;
    let args = args()?;
    h.invoke(&callee, &AccountValue::Undefined, &args)
}
fn binding_method(
    h: &dyn PublicSocketHost,
    name: &str,
    key: &str,
    args: impl FnOnce() -> SigningResult<Vec<AccountValue>>,
) -> SigningResult<AccountValue> {
    let object = h.binding(name)?;
    method(h, &object, key, args)
}
fn virtual_now(h: &dyn PublicSocketHost) -> SigningResult<AccountValue> {
    binding_method(h, "virtualClock", "getCurrentTimeMs", || Ok(vec![]))
}
enum Part<'a> {
    Text(&'a str),
    Value(AccountValue),
}
fn template(h: &dyn PublicSocketHost, parts: Vec<Part<'_>>) -> SigningResult<AccountValue> {
    let mut out = Vec::new();
    for part in parts {
        match part {
            Part::Text(s) => out.extend(u(s)),
            Part::Value(v) => out.extend(h.template(&v)?),
        }
    }
    Ok(AccountValue::String(out))
}
/// `this.parent.log(<message>, LogSeverity.<severity>)` with the message
/// evaluated after the parent/log Gets.
fn log(
    h: &dyn PublicSocketHost,
    severity: &str,
    message: impl FnOnce() -> SigningResult<AccountValue>,
) -> SigningResult<()> {
    let parent = own(h, "parent")?;
    method(h, &parent, "log", || Ok(vec![message()?, text(severity)]))?;
    Ok(())
}
fn error_text(h: &dyn PublicSocketHost, error: &AccountValue) -> SigningResult<AccountValue> {
    let message = optional(h, error, "message")?;
    if message.truthy() {
        Ok(message)
    } else {
        let error = error.clone();
        global_call(h, "String", || Ok(vec![error]))
    }
}
/// `Number(error?.response?.status ?? error?.status ?? error?.statusCode)`.
fn status_code(h: &dyn PublicSocketHost, error: &AccountValue) -> SigningResult<AccountValue> {
    let callee = h.binding("Number")?;
    let response = optional(h, error, "response")?;
    let mut status = optional(h, &response, "status")?;
    if status.is_nullish() {
        status = optional(h, error, "status")?;
    }
    if status.is_nullish() {
        status = optional(h, error, "statusCode")?;
    }
    h.invoke(&callee, &AccountValue::Undefined, &[status])
}
fn payload(
    h: &dyn PublicSocketHost,
    kind: &str,
    channel: &AccountValue,
) -> SigningResult<AccountValue> {
    h.literal(&[("type", text(kind)), ("channel", channel.clone())])
}
fn send(h: &dyn PublicSocketHost, kind: &str, channel: &AccountValue) -> SigningResult<()> {
    method(h, &h.owner(), "sendPayload", || {
        Ok(vec![payload(h, kind, channel)?])
    })?;
    Ok(())
}
fn handlers(h: &dyn PublicSocketHost, topic: &AccountValue) -> SigningResult<AccountValue> {
    let record = own(h, "messageHandlers")?;
    h.get(&record, topic)
}
fn has_handlers(h: &dyn PublicSocketHost, topic: &AccountValue) -> SigningResult<bool> {
    let list = handlers(h, topic)?;
    Ok(optional(h, &list, "length")?.truthy())
}
/// `this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []), handler]`.
fn append_handler(
    h: &dyn PublicSocketHost,
    topic: &AccountValue,
    handler: &AccountValue,
) -> SigningResult<()> {
    let record = own(h, "messageHandlers")?;
    let current = handlers(h, topic)?;
    let current = if current.truthy() {
        current
    } else {
        h.array(&[])?
    };
    let mut values = Vec::new();
    h.for_of(&current, &mut |v| {
        values.push(v);
        Ok(())
    })?;
    values.push(handler.clone());
    let next = h.array(&values)?;
    h.set(&record, topic, next)
}
/// `this.messageHandlers[topic] = [...(this.messageHandlers[topic] || []).filter((f) => f !== handler)]`.
fn remove_handler(
    h: Rc<dyn PublicSocketHost>,
    topic: &AccountValue,
    handler: &AccountValue,
) -> SigningResult<()> {
    let record = own(h.as_ref(), "messageHandlers")?;
    let current = handlers(h.as_ref(), topic)?;
    let current = if current.truthy() {
        current
    } else {
        h.array(&[])?
    };
    let target = handler.clone();
    let filtered = method(h.as_ref(), &current, "filter", || {
        let predicate = h.function(Rc::new(move |args| {
            let f = args.first().cloned().unwrap_or(AccountValue::Undefined);
            Ok(AccountValue::Boolean(!strict_eq(&f, &target)))
        }))?;
        Ok(vec![predicate])
    })?;
    let mut values = Vec::new();
    h.for_of(&filtered, &mut |v| {
        values.push(v);
        Ok(())
    })?;
    let next = h.array(&values)?;
    h.set(&record, topic, next)
}
fn length_is_zero(h: &dyn PublicSocketHost, list: &AccountValue) -> SigningResult<bool> {
    Ok(!get(h, list, "length")?.truthy())
}
fn set_timeout(
    h: &dyn PublicSocketHost,
    callback: SocketCallback,
    delay: AccountValue,
) -> SigningResult<AccountValue> {
    let callee = h.binding("setTimeout")?;
    let callback = h.function(callback)?;
    h.invoke(&callee, &AccountValue::Undefined, &[callback, delay])
}

type Next = Box<dyn FnOnce(SigningResult<AccountValue>) -> SigningResult<Step>>;
enum Step {
    Done(AccountValue),
    Await(AccountValue, Next),
}
type Catch = Rc<dyn Fn(TransactionSigningError) -> SigningResult<Step>>;
fn caught(step: SigningResult<Step>, handler: Catch) -> SigningResult<Step> {
    match step {
        Err(e) => handler(e),
        Ok(Step::Await(v, next)) => Ok(Step::Await(v, Box::new(move |v| caught(next(v), handler)))),
        done => done,
    }
}
type Cleanup = Rc<dyn Fn() -> SigningResult<()>>;
fn finally(step: SigningResult<Step>, cleanup: Cleanup) -> SigningResult<Step> {
    match step {
        Ok(Step::Await(v, next)) => {
            Ok(Step::Await(v, Box::new(move |v| finally(next(v), cleanup))))
        }
        done => {
            cleanup()?;
            done
        }
    }
}
fn drive(h: Rc<dyn PublicSocketHost>, r: PromiseResolver<AccountValue>, step: SigningResult<Step>) {
    match step {
        Err(e) => r.settle(Err(e)),
        Ok(Step::Done(v)) => h.resolve_return(v, r),
        Ok(Step::Await(v, next)) => match h.await_value(v) {
            Err(e) => drive(h, r, next(Err(e))),
            Ok(p) => p.subscribe(move |v| drive(h, r, next(v.map(|v| (*v).clone())))),
        },
    }
}
fn start(
    h: Rc<dyn PublicSocketHost>,
    work: impl FnOnce() -> SigningResult<Step>,
) -> NativePromise<AccountValue> {
    let (p, r) = NativePromise::pending(h.runtime());
    drive(h, r, work());
    p
}
fn done() -> SigningResult<Step> {
    Ok(Step::Done(AccountValue::Undefined))
}

// ts: LighterPublicWebsocket.onClose
pub fn on_close(h: Rc<dyn PublicSocketHost>, event: &AccountValue) -> SigningResult<()> {
    let hr = h.as_ref();
    let number_ctor = hr.binding("Number")?;
    let code = optional(hr, event, "code")?;
    let code_value = hr.invoke(&number_ctor, &AccountValue::Undefined, &[code])?;
    let finite = binding_method(hr, "Number", "isFinite", || Ok(vec![code_value.clone()]))?;
    let close_code = if finite.truthy() {
        code_value
    } else {
        AccountValue::Undefined
    };
    method(hr, &hr.owner(), "recordCloseDiagnostics", || {
        Ok(vec![
            close_code.clone(),
            hr.literal(&[("syntheticIfNoSocket", AccountValue::Boolean(true))])?,
        ])
    })?;
    let close_reason = if matches!(optional(hr, event, "reason")?, AccountValue::String(_)) {
        get(hr, event, "reason")?
    } else {
        text("")
    };
    let now = virtual_now(hr)?;
    let time_since_connect = if primitive_relation(
        JsRelation::Greater,
        &own(hr, "connectStartedAt")?,
        &number(0.0),
    )? {
        let started = own(hr, "connectStartedAt")?;
        primitive_binary(JsBinary::Subtract, &now, &started)?
    } else {
        number(f64::INFINITY)
    };
    let failed_before_open = !own(hr, "openedSinceLastConnect")?.truthy();
    let closed_before_first_message = own(hr, "openedSinceLastConnect")?.truthy()
        && !own(hr, "hasReceivedMessageSinceConnect")?.truthy();
    let early_close = closed_before_first_message
        && primitive_relation(
            JsRelation::LessEqual,
            &time_since_connect,
            &number(EARLY_CLOSE_WINDOW_MS),
        )?;
    let classification = global_call(hr, "classifyLighterPublicClose", || {
        Ok(vec![close_code.clone(), close_reason.clone()])
    })?;
    let should_count = failed_before_open
        || early_close
        || is_number(&close_code, 1006.0)
        || is_number(&close_code, 1011.0)
        || get(hr, &classification, "isRateLimited")?.truthy()
        || get(hr, &classification, "isPolicyRestricted")?.truthy();
    if should_count {
        let failed = primitive_binary(JsBinary::Add, &own(hr, "failedConnections")?, &number(1.0))?;
        set_own(hr, "failedConnections", failed)?;
    } else if is_number(&close_code, 1000.0) {
        set_own(hr, "failedConnections", AccountValue::Number(0.0))?;
    } else {
        let value = binding_method(hr, "Math", "max", || {
            let failed = own(hr, "failedConnections")?;
            Ok(vec![
                number(0.0),
                primitive_binary(JsBinary::Subtract, &failed, &number(1.0))?,
            ])
        })?;
        set_own(hr, "failedConnections", value)?;
    }
    if !own_call(hr, "isWsProxyRouteEnabled", &[])?.truthy()
        && !own(hr, "connectedViaProxyRoute")?.truthy()
        && !own(hr, "useReadonlyDirectRoute")?.truthy()
        && should_count
        && primitive_relation(
            JsRelation::GreaterEqual,
            &own(hr, "failedConnections")?,
            &number(DIRECT_ROUTE_FAILURES_BEFORE_READONLY),
        )?
    {
        set_own(hr, "useReadonlyDirectRoute", AccountValue::Boolean(true))?;
        log(hr, "warning", || {
            template(
                hr,
                vec![
                    Part::Text("[Lighter] Public WS switching to readonly direct route after "),
                    Part::Value(own(hr, "failedConnections")?),
                    Part::Text(" direct-route failures."),
                ],
            )
        })?;
    }
    if own_call(hr, "isWsProxyRouteEnabled", &[])?.truthy() {
        let received = || -> SigningResult<bool> {
            Ok(own(hr, "hasReceivedMessageSinceConnect")?.truthy()
                || is_number(&close_code, 1000.0))
        };
        if own(hr, "connectedViaProxyRoute")?.truthy() {
            set_own(hr, "directRouteFailureCount", AccountValue::Number(0.0))?;
            if should_count {
                let count = primitive_binary(
                    JsBinary::Add,
                    &own(hr, "proxyRouteFailureCount")?,
                    &number(1.0),
                )?;
                set_own(hr, "proxyRouteFailureCount", count)?;
                let paused_until = own(hr, "directFallbackProbePausedUntil")?;
                let can_probe = primitive_relation(JsRelation::GreaterEqual, &now, &paused_until)?;
                if !own(hr, "useDirectFallbackRoute")?.truthy()
                    && can_probe
                    && primitive_relation(
                        JsRelation::GreaterEqual,
                        &own(hr, "proxyRouteFailureCount")?,
                        &number(PROXY_ROUTE_FAILURES_BEFORE_DIRECT_FALLBACK),
                    )?
                {
                    set_own(hr, "useDirectFallbackRoute", AccountValue::Boolean(true))?;
                    log(hr, "warning", || {
                        template(
                            hr,
                            vec![
                                Part::Text("[Lighter] Public WS proxy route failed "),
                                Part::Value(own(hr, "proxyRouteFailureCount")?),
                                Part::Text(" times; probing direct route as a fallback."),
                            ],
                        )
                    })?;
                }
            } else if received()? {
                set_own(hr, "proxyRouteFailureCount", AccountValue::Number(0.0))?;
                set_own(hr, "useDirectFallbackRoute", AccountValue::Boolean(false))?;
            }
        } else if should_count {
            let count = primitive_binary(
                JsBinary::Add,
                &own(hr, "directRouteFailureCount")?,
                &number(1.0),
            )?;
            set_own(hr, "directRouteFailureCount", count)?;
            set_own(hr, "useDirectFallbackRoute", AccountValue::Boolean(false))?;
            set_own(
                hr,
                "directFallbackProbePausedUntil",
                primitive_binary(
                    JsBinary::Add,
                    &now,
                    &number(PROXY_DIRECT_FALLBACK_FAILURE_COOLDOWN_MS),
                )?,
            )?;
            log(hr, "warning", || {
                let count = own(hr, "directRouteFailureCount")?;
                let minutes = binding_method(hr, "Math", "round", || {
                    Ok(vec![AccountValue::Number(
                        PROXY_DIRECT_FALLBACK_FAILURE_COOLDOWN_MS / 60_000.0,
                    )])
                })?;
                template(
                    hr,
                    vec![
                        Part::Text("[Lighter] Public WS direct fallback failed "),
                        Part::Value(count),
                        Part::Text(
                            " times; returning to proxy route and pausing direct probes for ",
                        ),
                        Part::Value(minutes),
                        Part::Text(" minutes."),
                    ],
                )
            })?;
        } else if received()? {
            set_own(hr, "directRouteFailureCount", AccountValue::Number(0.0))?;
            set_own(hr, "proxyRouteFailureCount", AccountValue::Number(0.0))?;
            set_own(hr, "useDirectFallbackRoute", AccountValue::Boolean(false))?;
        }
    }
    set_own(hr, "openedSinceLastConnect", AccountValue::Boolean(false))?;
    set_own(
        hr,
        "hasReceivedMessageSinceConnect",
        AccountValue::Boolean(false),
    )?;
    own_call(hr, "clearPendingSubscribeTimeouts", &[])?;
    own_call(hr, "clearAllOrderBookStaleTimeouts", &[])?;
    own_call(hr, "clearAllOrderBookInitialDataTimeouts", &[])?;
    own_call(
        hr,
        "startTickerRestFallback",
        &[text("public websocket closed; polling tickers via REST")],
    )?;
    set_own(hr, "lastReceived", AccountValue::Number(0.0))?;
    if own(hr, "lastReceivedTimer")?.truthy() {
        global_call(hr, "clearTimeout", || {
            Ok(vec![own(hr, "lastReceivedTimer")?])
        })?;
        set_own(hr, "lastReceivedTimer", AccountValue::Undefined)?;
    }
    let parent = own(hr, "parent")?;
    let sync = get(hr, &parent, "syncSocketHealthToStore")?;
    if !sync.is_nullish() {
        hr.invoke(&sync, &parent, &[])?;
    }
    if !close_code.is_nullish()
        && !is_number(&close_code, 1000.0)
        && !get(hr, &own(hr, "parent")?, "isDisposed")?.truthy()
    {
        log(hr, "warning", || {
            let mut parts = vec![
                Part::Text("[Lighter] Public WS closed (code "),
                Part::Value(close_code.clone()),
            ];
            if close_reason.truthy() {
                parts.push(Part::Value(template(
                    hr,
                    vec![Part::Text(", reason: "), Part::Value(close_reason.clone())],
                )?));
            } else {
                parts.push(Part::Text(""));
            }
            parts.push(Part::Text(")"));
            template(hr, parts)
        })?;
    }
    if own(hr, "reconnecting")?.truthy() || get(hr, &own(hr, "parent")?, "isDisposed")?.truthy() {
        return Ok(());
    }
    set_own(hr, "reconnecting", AccountValue::Boolean(true))?;
    for timer in ["pingTimeoutId", "pongMissTimeoutId"] {
        if own(hr, timer)?.truthy() {
            global_call(hr, "clearTimeout", || Ok(vec![own(hr, timer)?]))?;
            set_own(hr, timer, AccountValue::Undefined)?;
        }
    }
    for (event_name, handler) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let ws = own(hr, "ws")?;
        if ws.is_nullish() {
            continue;
        }
        let remove = get(hr, &ws, "removeEventListener")?;
        if remove.is_nullish() {
            continue;
        }
        let handler = own(hr, handler)?;
        hr.invoke(&remove, &ws, &[text(event_name), handler])?;
    }
    set_own(hr, "ws", AccountValue::Undefined)?;
    let count = primitive_increment(&own(hr, "onCloseCount")?)?;
    set_own(hr, "onCloseCount", count)?;
    if primitive_relation(
        JsRelation::Greater,
        &own(hr, "onCloseCount")?,
        &number(100.0),
    )? {
        log(hr, "error", || {
            Ok(text(
                "[Lighter] Public WS onClose count > 100, stopping reconnection",
            ))
        })?;
        return Ok(());
    }
    let delay = global_call(hr, "calculateLighterPublicReconnectDelay", || {
        let base = hr.binding("WS_MIN_RECONNECTION_DELAY")?;
        let failures = own(hr, "failedConnections")?;
        let limited = get(hr, &classification, "isRateLimited")?;
        Ok(vec![hr.literal(&[
            ("baseDelayMs", base),
            ("failureCount", failures),
            ("isRateLimited", limited),
            (
                "rateLimitReconnectDelayMs",
                AccountValue::Number(RATE_LIMIT_RECONNECT_DELAY_MS),
            ),
        ])?])
    })?;
    log(hr, "info", || {
        let rounded = binding_method(hr, "Math", "round", || Ok(vec![delay.clone()]))?;
        template(
            hr,
            vec![
                Part::Text("[Lighter] Public WS reconnecting in "),
                Part::Value(rounded),
                Part::Text("ms..."),
            ],
        )
    })?;
    let live = h.clone();
    set_timeout(
        hr,
        Rc::new(move |_| {
            let h = live.as_ref();
            if get(h, &own(h, "parent")?, "isDisposed")?.truthy() {
                return Ok(AccountValue::Undefined);
            }
            set_own(h, "reconnecting", AccountValue::Boolean(false))?;
            let now = binding_method(h, "Date", "now", || Ok(vec![]))?;
            set_own(h, "lastCloseTime", now)?;
            own_call(h, "connectAndSubscribe", &[AccountValue::Boolean(true)])?;
            Ok(AccountValue::Undefined)
        }),
        delay,
    )?;
    Ok(())
}

// ts: LighterPublicWebsocket#fetchAndEmitOrderBookFromRest
pub fn fetch_and_emit_order_book_from_rest(
    h: Rc<dyn PublicSocketHost>,
    topic: AccountValue,
    market_id: AccountValue,
    book: AccountValue,
    callback: AccountValue,
) -> NativePromise<AccountValue> {
    let live = h.clone();
    start(h, move || {
        let hr = live.as_ref();
        if own_call(hr, "isOrderBookRestFallbackCoolingDown", &[topic.clone()])?.truthy() {
            return done();
        }
        let in_flight = own(hr, "orderBookRestFallbackInFlight")?;
        if hr.get(&in_flight, &topic)?.truthy() {
            return done();
        }
        let in_flight = own(hr, "orderBookRestFallbackInFlight")?;
        hr.set(&in_flight, &topic, AccountValue::Boolean(true))?;
        let body = (|| -> SigningResult<Step> {
            let parent = own(hr, "parent")?;
            let api = get(hr, &parent, "orderApi")?;
            let request = get(hr, &api, "getOrderBookOrders")?;
            let id = binding_method(hr, "Number", "parseInt", || {
                Ok(vec![market_id.clone(), AccountValue::Number(10.0)])
            })?;
            let options =
                hr.literal(&[("__suppressExchangeError", AccountValue::Boolean(true))])?;
            let pending = hr.invoke(&request, &api, &[id, AccountValue::Number(100.0), options])?;
            let h = live.clone();
            let book = book.clone();
            let callback = callback.clone();
            Ok(Step::Await(
                pending,
                Box::new(move |result| {
                    let hr = h.as_ref();
                    let payload = result?;
                    let levels = own_call(hr, "parseRestOrderBookLevels", &[payload])?;
                    if !levels.truthy() {
                        return done();
                    }
                    for side in ["bids", "asks"] {
                        method(hr, &hr.owner(), "applyOrderBookLevels", || {
                            Ok(vec![
                                book.clone(),
                                text(side),
                                get(hr, &levels, side)?,
                                text("snapshot"),
                            ])
                        })?;
                    }
                    global_call(hr, "calcOrderBookTotal", || Ok(vec![book.clone()]))?;
                    hr.invoke(&callback, &AccountValue::Undefined, &[book.clone()])?;
                    done()
                }),
            ))
        })();
        let h = live.clone();
        let catch_topic = topic.clone();
        let catch_market = market_id.clone();
        let handler: Catch = Rc::new(move |error| {
            let hr = h.as_ref();
            let error = hr.error_value(error);
            let status = status_code(hr, &error)?;
            if is_number(&status, 403.0) {
                let record = own(hr, "orderBookRestFallbackCooldownUntilByTopic")?;
                let until = primitive_binary(
                    JsBinary::Add,
                    &virtual_now(hr)?,
                    &number(ORDER_BOOK_REST_FALLBACK_403_COOLDOWN_MS),
                )?;
                hr.set(&record, &catch_topic, until)?;
                log(hr, "warning", || {
                    template(
                        hr,
                        vec![
                            Part::Text(
                                "[Lighter] REST orderbook fallback returned 403 for market ",
                            ),
                            Part::Value(catch_market.clone()),
                            Part::Text("; pausing fallback requests."),
                        ],
                    )
                })?;
                own_call(hr, "stopOrderBookRestFallback", &[catch_topic.clone()])?;
                return done();
            }
            log(hr, "warning", || {
                template(
                    hr,
                    vec![
                        Part::Text("[Lighter] REST orderbook fallback failed for market "),
                        Part::Value(catch_market.clone()),
                        Part::Text(": "),
                        Part::Value(error_text(hr, &error)?),
                    ],
                )
            })?;
            done()
        });
        let h = live.clone();
        let cleanup: Cleanup = Rc::new(move || {
            let hr = h.as_ref();
            let in_flight = own(hr, "orderBookRestFallbackInFlight")?;
            hr.set(&in_flight, &topic, AccountValue::Boolean(false))
        });
        finally(caught(body, handler), cleanup)
    })
}

fn ticker_fallback_body(h: Rc<dyn PublicSocketHost>) -> SigningResult<Step> {
    let hr = h.as_ref();
    let body = (|| -> SigningResult<Step> {
        let parent = own(hr, "parent")?;
        let client = get(hr, &parent, "lighterApiClient")?;
        let request = get(hr, &client, "get")?;
        let endpoints = hr.binding("ENDPOINTS")?;
        let path = get(hr, &endpoints, "EXCHANGE_STATS")?;
        let options = hr.literal(&[("__suppressExchangeError", AccountValue::Boolean(true))])?;
        let pending = hr.invoke(&request, &client, &[path, options])?;
        let live = h.clone();
        Ok(Step::Await(
            pending,
            Box::new(move |result| {
                let hr = live.as_ref();
                let payload = result?;
                if optional(hr, &payload, "market_stats")?.truthy()
                    && is_object(&get(hr, &payload, "market_stats")?)
                {
                    own_call(hr, "handleMarketStatsEvent", &[payload.clone()])?;
                }
                let items = own_call(hr, "parseRestTickerItems", &[payload])?;
                hr.for_of(&items, &mut |item| {
                    if !item.truthy() || !is_object(&item) {
                        return Ok(());
                    }
                    let mut market_id = get(hr, &item, "market_id")?;
                    if market_id.is_nullish() {
                        market_id = get(hr, &item, "marketId")?;
                    }
                    method(hr, &hr.owner(), "handleTickerEvent", || {
                        let mut fields = vec![("data", item.clone())];
                        if !matches!(market_id, AccountValue::Undefined) {
                            let channel = template(
                                hr,
                                vec![Part::Text("ticker/"), Part::Value(market_id.clone())],
                            )?;
                            fields.push(("channel", channel));
                        }
                        Ok(vec![hr.literal(&fields)?])
                    })?;
                    Ok(())
                })?;
                own_call(hr, "syncTickerRestFallbackToPriceHydration", &[])?;
                done()
            }),
        ))
    })();
    let live = h.clone();
    let handler: Catch = Rc::new(move |error| {
        let hr = live.as_ref();
        let error = hr.error_value(error);
        let status = status_code(hr, &error)?;
        if is_number(&status, 403.0) {
            let until = primitive_binary(
                JsBinary::Add,
                &virtual_now(hr)?,
                &number(TICKER_REST_FALLBACK_403_COOLDOWN_MS),
            )?;
            set_own(hr, "tickerRestFallbackCooldownUntil", until)?;
            log(hr, "warning", || {
                Ok(text(
                    "[Lighter] REST ticker fallback returned 403; pausing ticker REST fallback to avoid noisy retry loop.",
                ))
            })?;
            own_call(hr, "clearTickerRestFallbackWatchdog", &[])?;
            own_call(
                hr,
                "stopTickerRestFallback",
                &[AccountValue::Boolean(false)],
            )?;
            return done();
        }
        log(hr, "warning", || {
            template(
                hr,
                vec![
                    Part::Text("[Lighter] REST ticker fallback failed: "),
                    Part::Value(error_text(hr, &error)?),
                ],
            )
        })?;
        done()
    });
    caught(body, handler)
}

// ts: LighterPublicWebsocket#fetchAndApplyTickerFallback
pub fn fetch_and_apply_ticker_fallback(h: Rc<dyn PublicSocketHost>) -> NativePromise<AccountValue> {
    let live = h.clone();
    start(h, move || {
        let hr = live.as_ref();
        let parent = own(hr, "parent")?;
        let client = get(hr, &parent, "lighterApiClient")?;
        if !optional(hr, &client, "get")?.truthy() {
            return done();
        }
        if own_call(hr, "isTickerRestFallbackCoolingDown", &[])?.truthy() {
            return done();
        }
        let in_flight = own(hr, "tickerRestFallbackInFlight")?;
        if in_flight.truthy() {
            return Ok(Step::Done(in_flight));
        }
        let inner_host = live.clone();
        let inner = start(live.clone(), move || ticker_fallback_body(inner_host));
        let inner = hr.promise_value(inner);
        let reset_host = live.clone();
        let chained = method(hr, &inner, "finally", || {
            Ok(vec![hr.function(Rc::new(move |_| {
                set_own(
                    reset_host.as_ref(),
                    "tickerRestFallbackInFlight",
                    AccountValue::Undefined,
                )?;
                Ok(AccountValue::Undefined)
            }))?])
        })?;
        set_own(hr, "tickerRestFallbackInFlight", chained)?;
        Ok(Step::Done(own(hr, "tickerRestFallbackInFlight")?))
    })
}

// ts: LighterPublicWebsocket#attemptOrderBookRecovery
pub fn attempt_order_book_recovery(
    h: &dyn PublicSocketHost,
    topic: &AccountValue,
    channel: &AccountValue,
    reason: &AccountValue,
) -> SigningResult<()> {
    if !has_handlers(h, topic)? {
        return Ok(());
    }
    let now = virtual_now(h)?;
    let last = h.get(&own(h, "lastOrderBookRecoveryAtByTopic")?, topic)?;
    let last = if last.truthy() { last } else { number(0.0) };
    let elapsed = primitive_binary(JsBinary::Subtract, &now, &last)?;
    if primitive_relation(
        JsRelation::Less,
        &elapsed,
        &number(ORDER_BOOK_RECOVERY_COOLDOWN_MS),
    )? {
        own_call(
            h,
            "armOrderBookStaleTimeout",
            &[topic.clone(), channel.clone()],
        )?;
        return Ok(());
    }
    let record = own(h, "lastOrderBookRecoveryAtByTopic")?;
    h.set(&record, topic, now)?;
    let attempts = h.get(&own(h, "orderBookRecoveryAttempts")?, topic)?;
    let attempts = if attempts.truthy() {
        attempts
    } else {
        AccountValue::Number(0.0)
    };
    let attempt = primitive_binary(JsBinary::Add, &attempts, &number(1.0))?;
    let record = own(h, "orderBookRecoveryAttempts")?;
    h.set(&record, topic, attempt.clone())?;
    let should_reconnect = !own(h, "isConnected")?.truthy()
        || primitive_relation(JsRelation::GreaterEqual, &attempt, &number(2.0))?;
    let message = |prefix: &str| -> SigningResult<AccountValue> {
        template(
            h,
            vec![
                Part::Text(prefix),
                Part::Value(topic.clone()),
                Part::Text(" ("),
                Part::Value(reason.clone()),
                Part::Text(", attempt "),
                Part::Value(attempt.clone()),
                Part::Text(")."),
            ],
        )
    };
    if should_reconnect {
        log(h, "warning", || {
            message("[Lighter] Public WS order book recovery reconnecting for ")
        })?;
        own_call(h, "reconnect", &[])?;
        return Ok(());
    }
    log(h, "warning", || {
        message("[Lighter] Public WS order book recovery re-subscribing ")
    })?;
    let resubscribe = send(h, "unsubscribe", channel).and_then(|_| send(h, "subscribe", channel));
    if let Err(error) = resubscribe {
        let error = h.error_value(error);
        log(h, "warning", || {
            template(
                h,
                vec![
                    Part::Text("[Lighter] Public WS order book re-subscribe failed for "),
                    Part::Value(topic.clone()),
                    Part::Text(": "),
                    Part::Value(error_text(h, &error)?),
                ],
            )
        })?;
        own_call(h, "reconnect", &[])?;
        return Ok(());
    }
    own_call(
        h,
        "armOrderBookStaleTimeout",
        &[topic.clone(), channel.clone()],
    )?;
    Ok(())
}

struct OhlcvCore {
    host: Rc<dyn PublicSocketHost>,
    opts: AccountValue,
    callback: AccountValue,
    topic: AccountValue,
    last_candle: RefCell<AccountValue>,
}
struct OhlcvListener {
    core: Rc<OhlcvCore>,
    secondary: AccountValue,
    uses_trades: bool,
    active: Cell<bool>,
    retry: RefCell<AccountValue>,
    parser: AccountValue,
    secondary_parser: AccountValue,
}
fn every_finite(h: &dyn PublicSocketHost, values: &[AccountValue]) -> SigningResult<bool> {
    let array = h.array(values)?;
    let every = method(h, &array, "every", || {
        Ok(vec![get(h, &h.binding("Number")?, "isFinite")?])
    })?;
    Ok(every.truthy())
}
/// `Array.isArray(data?.<many>) ? data.<many> : data?.<one> ? [data.<one>] : []`.
fn entries(
    h: &dyn PublicSocketHost,
    data: &AccountValue,
    many: &str,
    one: &str,
) -> SigningResult<AccountValue> {
    let is_array = binding_method(h, "Array", "isArray", || Ok(vec![optional(h, data, many)?]))?;
    if is_array.truthy() {
        return get(h, data, many);
    }
    if optional(h, data, one)?.truthy() {
        return h.array(&[get(h, data, one)?]);
    }
    h.array(&[])
}
fn message_data(h: &dyn PublicSocketHost, message: &AccountValue) -> SigningResult<AccountValue> {
    let data = get(h, message, "data")?;
    Ok(if data.is_nullish() {
        message.clone()
    } else {
        data
    })
}
fn first_present(
    h: &dyn PublicSocketHost,
    value: &AccountValue,
    a: &str,
    b: &str,
) -> SigningResult<AccountValue> {
    let first = get(h, value, a)?;
    if first.is_nullish() {
        get(h, value, b)
    } else {
        Ok(first)
    }
}
/// `Number(raw)`, then source millisecond detection.
fn seconds(
    h: &dyn PublicSocketHost,
    raw: impl FnOnce() -> SigningResult<AccountValue>,
) -> SigningResult<AccountValue> {
    let raw = global_call(h, "Number", || Ok(vec![raw()?]))?;
    if primitive_relation(JsRelation::Greater, &raw, &number(10_000_000_000.0))? {
        primitive_binary(JsBinary::Divide, &raw, &number(1000.0))
    } else {
        Ok(raw)
    }
}
fn parse_float(
    h: &dyn PublicSocketHost,
    value: impl FnOnce() -> SigningResult<AccountValue>,
) -> SigningResult<AccountValue> {
    global_call(h, "parseFloat", || Ok(vec![value()?]))
}
fn candle_parser(state: &OhlcvCore, message: &AccountValue) -> SigningResult<()> {
    let h = state.host.as_ref();
    let data = message_data(h, message)?;
    let candles = entries(h, &data, "candles", "candle")?;
    h.for_of(&candles, &mut |candle| {
        let timestamp = seconds(h, || first_present(h, &candle, "t", "timestamp"))?;
        let mut fields = vec![("timestamp", timestamp.clone())];
        for (name, short, long) in [
            ("open", "o", "open"),
            ("high", "h", "high"),
            ("low", "l", "low"),
            ("close", "c", "close"),
            ("volume", "v", "volume"),
        ] {
            fields.push((
                name,
                parse_float(h, || first_present(h, &candle, short, long))?,
            ));
        }
        let values: Vec<_> = fields.iter().map(|(_, v)| v.clone()).collect();
        if !every_finite(h, &values)? {
            return Ok(());
        }
        let last = state.last_candle.borrow().clone();
        if last.truthy()
            && h.relational(
                Relational::Greater,
                &get(h, &last, "timestamp")?,
                &timestamp,
            )?
        {
            return Ok(());
        }
        own_call(h, "clearOhlcvNoDataTimeout", &[state.topic.clone()])?;
        let candle = h.literal(&fields)?;
        *state.last_candle.borrow_mut() = candle.clone();
        h.invoke(&state.callback, &AccountValue::Undefined, &[candle])?;
        Ok(())
    })
}
fn trade_parser(state: &OhlcvCore, message: &AccountValue) -> SigningResult<()> {
    let h = state.host.as_ref();
    let data = message_data(h, message)?;
    if !state.last_candle.borrow().truthy() {
        return Ok(());
    }
    let trades = entries(h, &data, "trades", "trade")?;
    h.for_of(&trades, &mut |trade| {
        let timestamp = seconds(h, || first_present(h, &trade, "timestamp", "t"))?;
        let start_time = global_call(h, "getCandleStartTime", || {
            Ok(vec![get(h, &state.opts, "interval")?, timestamp.clone()])
        })?;
        let price = parse_float(h, || get(h, &trade, "price"))?;
        let size = parse_float(h, || get(h, &trade, "size"))?;
        if !every_finite(h, &[start_time.clone(), price.clone(), size.clone()])?
            || h.relational(Relational::LessEqual, &price, &AccountValue::Number(0.0))?
            || h.relational(Relational::Less, &size, &AccountValue::Number(0.0))?
        {
            return Ok(());
        }
        let last = state.last_candle.borrow().clone();
        if h.relational(
            Relational::Greater,
            &get(h, &last, "timestamp")?,
            &start_time,
        )? {
            return Ok(());
        }
        if h.relational(Relational::Less, &get(h, &last, "timestamp")?, &start_time)? {
            let mut fields = vec![("timestamp", start_time.clone())];
            for name in ["open", "high", "low", "close"] {
                fields.push((name, get(h, &last, "close")?));
            }
            fields.push(("volume", AccountValue::Number(0.0)));
            *state.last_candle.borrow_mut() = h.literal(&fields)?;
        }
        let last = state.last_candle.borrow().clone();
        let next = h.copy_object(&last)?;
        let high = binding_method(h, "Math", "max", || {
            Ok(vec![get(h, &last, "high")?, price.clone()])
        })?;
        h.define(&next, "high", high)?;
        let low = binding_method(h, "Math", "min", || {
            Ok(vec![get(h, &last, "low")?, price.clone()])
        })?;
        h.define(&next, "low", low)?;
        h.define(&next, "close", price.clone())?;
        let volume = global_call(h, "add", || {
            Ok(vec![get(h, &last, "volume")?, size.clone()])
        })?;
        h.define(&next, "volume", volume)?;
        *state.last_candle.borrow_mut() = next.clone();
        h.invoke(&state.callback, &AccountValue::Undefined, &[next])?;
        Ok(())
    })
}
fn wait_ohlcv(state: Rc<OhlcvListener>) -> SigningResult<()> {
    let h = state.core.host.as_ref();
    let topic = &state.core.topic;
    if !state.active.get() || own(h, "isDisposed")?.truthy() {
        return Ok(());
    }
    if own(h, "isConnected")?.truthy() {
        *state.retry.borrow_mut() = AccountValue::Undefined;
        if state.active.get() && !own(h, "isDisposed")?.truthy() {
            let subscribe_candle = !has_handlers(h, topic)?;
            let subscribe_trades = state.uses_trades && !has_handlers(h, &state.secondary)?;
            append_handler(h, topic, &state.parser)?;
            if state.uses_trades {
                append_handler(h, &state.secondary, &state.secondary_parser)?;
            }
            if subscribe_candle {
                send(h, "subscribe", topic)?;
            }
            own_call(h, "armOhlcvNoDataTimeout", &[topic.clone()])?;
            if subscribe_trades {
                send(h, "subscribe", &state.secondary)?;
            }
            let topics = own(h, "topics")?;
            h.set(&topics, topic, topic.clone())?;
            if state.uses_trades {
                let topics = own(h, "topics")?;
                h.set(&topics, &state.secondary, state.secondary.clone())?;
            }
        }
    } else {
        let retry_state = state.clone();
        let id = set_timeout(
            h,
            Rc::new(move |_| {
                *retry_state.retry.borrow_mut() = AccountValue::Undefined;
                wait_ohlcv(retry_state.clone())?;
                Ok(AccountValue::Undefined)
            }),
            AccountValue::Number(SUBSCRIBE_RETRY_MS),
        )?;
        *state.retry.borrow_mut() = id;
    }
    Ok(())
}
fn stop_ohlcv(state: &OhlcvListener) -> SigningResult<()> {
    let host = state.core.host.clone();
    let h = host.as_ref();
    let topic = &state.core.topic;
    state.active.set(false);
    let retry = state.retry.borrow().clone();
    if retry.truthy() {
        global_call(h, "clearTimeout", || Ok(vec![retry]))?;
        *state.retry.borrow_mut() = AccountValue::Undefined;
    }
    remove_handler(host.clone(), topic, &state.parser)?;
    remove_handler(host.clone(), &state.secondary, &state.secondary_parser)?;
    if length_is_zero(h, &handlers(h, topic)?)? {
        own_call(h, "clearOhlcvNoDataTimeout", &[topic.clone()])?;
        h.delete(&own(h, "topics")?, topic)?;
        if state.uses_trades {
            h.delete(&own(h, "topics")?, &state.secondary)?;
        }
        own_call(h, "cleanupMessageHandlerStats", &[topic.clone()])?;
        own_call(h, "cleanupMessageHandlerStats", &[state.secondary.clone()])?;
    }
    if own(h, "isConnected")?.truthy() && length_is_zero(h, &handlers(h, topic)?)? {
        send(h, "unsubscribe", topic)?;
    }
    if state.uses_trades
        && own(h, "isConnected")?.truthy()
        && length_is_zero(h, &handlers(h, &state.secondary)?)?
    {
        send(h, "unsubscribe", &state.secondary)?;
    }
    Ok(())
}

// ts: LighterPublicWebsocket.listenOHLCV
pub fn listen_ohlcv(
    h: Rc<dyn PublicSocketHost>,
    opts: AccountValue,
    callback: AccountValue,
) -> SigningResult<AccountValue> {
    let hr = h.as_ref();
    let markets = get(
        hr,
        &get(hr, &own(hr, "parent")?, "store")?,
        "marketsBySymbol",
    )?;
    let market = hr.get(&markets, &get(hr, &opts, "symbol")?)?;
    if !market.truthy() {
        let ctor = hr.binding("Error")?;
        let message = template(
            hr,
            vec![
                Part::Text("Market not found for symbol: "),
                Part::Value(get(hr, &opts, "symbol")?),
            ],
        )?;
        let error = hr.construct(&ctor, &[message])?;
        return Err(hr.throw_value(error));
    }
    let topic = global_call(hr, "buildLighterCandleTopic", || {
        let id = get(hr, &market, "id")?;
        let interval = global_call(hr, "resolveLighterInterval", || {
            Ok(vec![get(hr, &opts, "interval")?])
        })?;
        Ok(vec![id, interval])
    })?;
    let secondary = global_call(hr, "buildLighterTradesTopic", || {
        Ok(vec![get(hr, &market, "id")?])
    })?;
    let options = get(hr, &own(hr, "parent")?, "options")?;
    let uses_trades = !get(hr, &options, "publicDataOnly")?.truthy();
    let core = Rc::new(OhlcvCore {
        host: h.clone(),
        opts,
        callback,
        topic,
        last_candle: RefCell::new(AccountValue::Undefined),
    });
    let parser_core = core.clone();
    let parser = hr.function(Rc::new(move |args| {
        candle_parser(&parser_core, &arg(args, 0))?;
        Ok(AccountValue::Undefined)
    }))?;
    let trade_core = core.clone();
    let secondary_parser = hr.function(Rc::new(move |args| {
        trade_parser(&trade_core, &arg(args, 0))?;
        Ok(AccountValue::Undefined)
    }))?;
    let state = Rc::new(OhlcvListener {
        core,
        secondary,
        uses_trades,
        active: Cell::new(true),
        retry: RefCell::new(AccountValue::Undefined),
        parser,
        secondary_parser,
    });
    wait_ohlcv(state.clone())?;
    hr.function(Rc::new(move |_| {
        stop_ohlcv(&state)?;
        Ok(AccountValue::Undefined)
    }))
}

struct BookCore {
    host: Rc<dyn PublicSocketHost>,
    callback: AccountValue,
    market_id: AccountValue,
    incoming: AccountValue,
    subscribe: AccountValue,
    book: AccountValue,
    received: Cell<bool>,
}
struct BookListener {
    core: Rc<BookCore>,
    timeout: RefCell<AccountValue>,
    parser: AccountValue,
}
fn book_parser(state: &BookCore, data: &AccountValue) -> SigningResult<()> {
    let h = state.host.as_ref();
    let mut ob = get(h, data, "order_book")?;
    if !ob.truthy() {
        ob = get(h, data, "data")?;
    }
    if !ob.truthy() {
        return Ok(());
    }
    state.received.set(true);
    let incoming = &state.incoming;
    own_call(h, "clearOrderBookInitialDataTimeout", &[incoming.clone()])?;
    own_call(h, "stopOrderBookRestFallback", &[incoming.clone()])?;
    h.delete(
        &own(h, "orderBookRestFallbackCooldownUntilByTopic")?,
        incoming,
    )?;
    own_call(h, "clearOrderBookStaleTimeout", &[incoming.clone()])?;
    let record = own(h, "orderBookRecoveryAttempts")?;
    h.set(&record, incoming, AccountValue::Number(0.0))?;
    let message_type = global_call(h, "String", || {
        let kind = get(h, data, "type")?;
        Ok(vec![if kind.truthy() { kind } else { text("") }])
    })?;
    let side = |name: &str| -> SigningResult<AccountValue> {
        let is_array = binding_method(h, "Array", "isArray", || Ok(vec![get(h, &ob, name)?]))?;
        if is_array.truthy() {
            get(h, &ob, name)
        } else {
            h.array(&[])
        }
    };
    let bids = side("bids")?;
    let asks = side("asks")?;
    let includes = |needle: &str| -> SigningResult<bool> {
        Ok(method(h, &message_type, "includes", || Ok(vec![text(needle)]))?.truthy())
    };
    let reset = includes("subscribed")?
        || includes("snapshot")?
        || (is_number(&get(h, &get(h, &state.book, "bids")?, "length")?, 0.0)
            && is_number(&get(h, &get(h, &state.book, "asks")?, "length")?, 0.0));
    let mode = if reset { "snapshot" } else { "delta" };
    for (name, levels) in [("bids", bids), ("asks", asks)] {
        own_call(
            h,
            "applyOrderBookLevels",
            &[state.book.clone(), text(name), levels, text(mode)],
        )?;
    }
    global_call(h, "calcOrderBookTotal", || Ok(vec![state.book.clone()]))?;
    h.invoke(
        &state.callback,
        &AccountValue::Undefined,
        &[state.book.clone()],
    )?;
    own_call(
        h,
        "armOrderBookStaleTimeout",
        &[incoming.clone(), state.subscribe.clone()],
    )?;
    Ok(())
}
fn wait_order_book(state: Rc<BookListener>) -> SigningResult<()> {
    let core = state.core.clone();
    let h = core.host.as_ref();
    if !own(h, "isConnected")?.truthy() {
        let retry = state.clone();
        let id = set_timeout(
            h,
            Rc::new(move |_| {
                wait_order_book(retry.clone())?;
                Ok(AccountValue::Undefined)
            }),
            AccountValue::Number(SUBSCRIBE_RETRY_MS),
        )?;
        *state.timeout.borrow_mut() = id;
        return Ok(());
    }
    if own(h, "isDisposed")?.truthy() {
        return Ok(());
    }
    append_handler(h, &core.incoming, &state.parser)?;
    send(h, "subscribe", &core.subscribe)?;
    own_call(
        h,
        "fetchAndEmitOrderBookFromRest",
        &[
            core.incoming.clone(),
            core.market_id.clone(),
            core.book.clone(),
            core.callback.clone(),
        ],
    )?;
    own_call(
        h,
        "armOrderBookStaleTimeout",
        &[core.incoming.clone(), core.subscribe.clone()],
    )?;
    own_call(
        h,
        "clearOrderBookInitialDataTimeout",
        &[core.incoming.clone()],
    )?;
    let record = own(h, "orderBookInitialDataTimeouts")?;
    let timer_core = core.clone();
    let id = set_timeout(
        h,
        Rc::new(move |_| {
            let core = timer_core.as_ref();
            let h = core.host.as_ref();
            h.delete(&own(h, "orderBookInitialDataTimeouts")?, &core.incoming)?;
            if own(h, "isDisposed")?.truthy()
                || core.received.get()
                || !has_handlers(h, &core.incoming)?
            {
                return Ok(AccountValue::Undefined);
            }
            own_call(
                h,
                "startOrderBookRestFallback",
                &[
                    core.incoming.clone(),
                    core.market_id.clone(),
                    core.book.clone(),
                    core.callback.clone(),
                    text("no initial websocket snapshot"),
                ],
            )?;
            Ok(AccountValue::Undefined)
        }),
        AccountValue::Number(ORDER_BOOK_INITIAL_DATA_TIMEOUT_MS),
    )?;
    h.set(&record, &core.incoming, id)?;
    let topics = own(h, "topics")?;
    h.set(&topics, &core.incoming, core.subscribe.clone())
}
fn stop_order_book(state: &BookListener) -> SigningResult<()> {
    let core = state.core.as_ref();
    let host = core.host.clone();
    let h = host.as_ref();
    remove_handler(host.clone(), &core.incoming, &state.parser)?;
    if length_is_zero(h, &handlers(h, &core.incoming)?)? {
        h.delete(&own(h, "topics")?, &core.incoming)?;
    }
    own_call(h, "cleanupMessageHandlerStats", &[core.incoming.clone()])?;
    own_call(
        h,
        "clearOrderBookInitialDataTimeout",
        &[core.incoming.clone()],
    )?;
    own_call(h, "stopOrderBookRestFallback", &[core.incoming.clone()])?;
    h.set(&core.book, &text("asks"), h.array(&[])?)?;
    h.set(&core.book, &text("bids"), h.array(&[])?)?;
    let timeout = state.timeout.borrow().clone();
    if timeout.truthy() {
        global_call(h, "clearTimeout", || Ok(vec![timeout]))?;
        *state.timeout.borrow_mut() = AccountValue::Null;
    }
    if own(h, "isConnected")?.truthy() && length_is_zero(h, &handlers(h, &core.incoming)?)? {
        own_call(h, "clearOrderBookStaleTimeout", &[core.incoming.clone()])?;
        h.delete(&own(h, "orderBookRecoveryAttempts")?, &core.incoming)?;
        h.delete(&own(h, "lastOrderBookRecoveryAtByTopic")?, &core.incoming)?;
        send(h, "unsubscribe", &core.subscribe)?;
    }
    Ok(())
}

// ts: LighterPublicWebsocket.listenOrderBook
pub fn listen_order_book(
    h: Rc<dyn PublicSocketHost>,
    symbol: AccountValue,
    callback: AccountValue,
) -> SigningResult<AccountValue> {
    let hr = h.as_ref();
    let markets = get(
        hr,
        &get(hr, &own(hr, "parent")?, "store")?,
        "marketsBySymbol",
    )?;
    let market = hr.get(&markets, &symbol)?;
    if !market.truthy() {
        log_default(hr, || {
            template(
                hr,
                vec![
                    Part::Text("[Lighter] listenOrderBook market not found for symbol: "),
                    Part::Value(symbol.clone()),
                ],
            )
        })?;
        return hr.function(Rc::new(|_| Ok(AccountValue::Undefined)));
    }
    let market_id = global_call(hr, "String", || Ok(vec![get(hr, &market, "id")?]))?;
    let incoming = template(
        hr,
        vec![Part::Text("order_book:"), Part::Value(market_id.clone())],
    )?;
    let subscribe = template(
        hr,
        vec![Part::Text("order_book/"), Part::Value(market_id.clone())],
    )?;
    let book = hr.literal(&[("bids", hr.array(&[])?), ("asks", hr.array(&[])?)])?;
    let core = Rc::new(BookCore {
        host: h.clone(),
        callback,
        market_id,
        incoming,
        subscribe,
        book,
        received: Cell::new(false),
    });
    let parser_core = core.clone();
    let parser = hr.function(Rc::new(move |args| {
        book_parser(&parser_core, &arg(args, 0))?;
        Ok(AccountValue::Undefined)
    }))?;
    let state = Rc::new(BookListener {
        core,
        timeout: RefCell::new(AccountValue::Null),
        parser,
    });
    wait_order_book(state.clone())?;
    hr.function(Rc::new(move |_| {
        stop_order_book(&state)?;
        Ok(AccountValue::Undefined)
    }))
}
/// `this.parent.log(message)` with the default severity parameter.
fn log_default(
    h: &dyn PublicSocketHost,
    message: impl FnOnce() -> SigningResult<AccountValue>,
) -> SigningResult<()> {
    let parent = own(h, "parent")?;
    method(h, &parent, "log", || Ok(vec![message()?]))?;
    Ok(())
}

/// Per-owner hosts for every already-ported member family, plus the typed
/// projections the pure helper bodies consume.
pub trait PublicSocketContexts {
    fn socket(&self, owner: AccountValue) -> Rc<dyn PublicSocketHost>;
    fn readiness(&self, owner: AccountValue) -> Rc<dyn PublicReadinessHost>;
    fn subscriptions(&self, owner: AccountValue) -> Rc<dyn PublicSubscriptionHost>;
    fn cleanup(&self, owner: AccountValue) -> Rc<dyn PublicCleanupHost>;
    fn watchdogs(&self, owner: AccountValue) -> Rc<dyn PublicWatchdogHost>;
    fn ingress(&self, owner: AccountValue)
    -> Rc<IngressOps<AccountValue, TransactionSigningError>>;
    /// Live `parent.store.tickers` read projected to typed tickers.
    fn store_tickers(&self, owner: AccountValue) -> SigningResult<Vec<safe_cex_types::Ticker>>;
    fn parse_rest_order_book_levels(&self, value: &AccountValue) -> SigningResult<AccountValue>;
    fn parse_rest_ticker_items(&self, value: &AccountValue) -> SigningResult<AccountValue>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PublicMethodMetadata {
    pub name: &'static str,
    pub length: usize,
    pub asynchronous: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublicMethod {
    ShouldEnforcePongMissReconnect,
    GetWatchdogStaleThresholdMs,
    MarketStatsHandler,
    TickerHandler,
    TickersHandler,
    PongHandler,
    IsWsProxyRouteEnabled,
    IsTickerRestFallbackCoolingDown,
    HasHydratedTickerPrice,
    SyncTickerRestFallbackToPriceHydration,
    PrimeTickerRestFallback,
    ConnectAndSubscribe,
    OnOpen,
    OnMarketsChanged,
    OnClose,
    SendPayload,
    ClearOhlcvNoDataTimeout,
    ClearOrderBookStaleTimeout,
    ClearOrderBookInitialDataTimeout,
    ClearAllOrderBookInitialDataTimeouts,
    ClearAllOrderBookStaleTimeouts,
    StopOrderBookRestFallback,
    StopAllOrderBookRestFallback,
    IsOrderBookRestFallbackCoolingDown,
    ParseRestOrderBookLevels,
    FetchAndEmitOrderBookFromRest,
    StartOrderBookRestFallback,
    ClearTickerRestFallbackWatchdog,
    ArmTickerRestFallbackWatchdog,
    ParseRestTickerItems,
    FetchAndApplyTickerFallback,
    StopTickerRestFallback,
    StartTickerRestFallback,
    ClearPendingSubscribeTimeouts,
    ArmOrderBookStaleTimeout,
    IsOrderBookTopic,
    ArmExistingOrderBookTimeouts,
    ArmOhlcvNoDataTimeout,
    ArmExistingOhlcvNoDataTimeouts,
    AttemptOhlcvRecovery,
    MaybeRecoverOhlcvStream,
    AttemptOrderBookRecovery,
    Ping,
    Subscribe,
    SyncBboTickerTopics,
    OnMessage,
    ToOrderBookLevel,
    AggregateOrderBookSnapshotLevels,
    ApplyOrderBookLevels,
    HandleTickerEvent,
    HandleTickersEvent,
    HandleMarketStatsEvent,
    ListenOHLCV,
    ListenOrderBook,
    Dispose,
}
/// Instance arrow fields after the handler-table closures, in source order.
const ARROW_FIELDS: [PublicMethod; 49] = [
    PublicMethod::IsWsProxyRouteEnabled,
    PublicMethod::IsTickerRestFallbackCoolingDown,
    PublicMethod::HasHydratedTickerPrice,
    PublicMethod::SyncTickerRestFallbackToPriceHydration,
    PublicMethod::PrimeTickerRestFallback,
    PublicMethod::ConnectAndSubscribe,
    PublicMethod::OnOpen,
    PublicMethod::OnMarketsChanged,
    PublicMethod::OnClose,
    PublicMethod::SendPayload,
    PublicMethod::ClearOhlcvNoDataTimeout,
    PublicMethod::ClearOrderBookStaleTimeout,
    PublicMethod::ClearOrderBookInitialDataTimeout,
    PublicMethod::ClearAllOrderBookInitialDataTimeouts,
    PublicMethod::ClearAllOrderBookStaleTimeouts,
    PublicMethod::StopOrderBookRestFallback,
    PublicMethod::StopAllOrderBookRestFallback,
    PublicMethod::IsOrderBookRestFallbackCoolingDown,
    PublicMethod::ParseRestOrderBookLevels,
    PublicMethod::FetchAndEmitOrderBookFromRest,
    PublicMethod::StartOrderBookRestFallback,
    PublicMethod::ClearTickerRestFallbackWatchdog,
    PublicMethod::ArmTickerRestFallbackWatchdog,
    PublicMethod::ParseRestTickerItems,
    PublicMethod::FetchAndApplyTickerFallback,
    PublicMethod::StopTickerRestFallback,
    PublicMethod::StartTickerRestFallback,
    PublicMethod::ClearPendingSubscribeTimeouts,
    PublicMethod::ArmOrderBookStaleTimeout,
    PublicMethod::IsOrderBookTopic,
    PublicMethod::ArmExistingOrderBookTimeouts,
    PublicMethod::ArmOhlcvNoDataTimeout,
    PublicMethod::ArmExistingOhlcvNoDataTimeouts,
    PublicMethod::AttemptOhlcvRecovery,
    PublicMethod::MaybeRecoverOhlcvStream,
    PublicMethod::AttemptOrderBookRecovery,
    PublicMethod::Ping,
    PublicMethod::Subscribe,
    PublicMethod::SyncBboTickerTopics,
    PublicMethod::OnMessage,
    PublicMethod::ToOrderBookLevel,
    PublicMethod::AggregateOrderBookSnapshotLevels,
    PublicMethod::ApplyOrderBookLevels,
    PublicMethod::HandleTickerEvent,
    PublicMethod::HandleTickersEvent,
    PublicMethod::HandleMarketStatsEvent,
    PublicMethod::ListenOHLCV,
    PublicMethod::ListenOrderBook,
    PublicMethod::Dispose,
];
impl PublicMethod {
    pub fn metadata(self) -> PublicMethodMetadata {
        use PublicMethod::*;
        let (name, length, asynchronous) = match self {
            ShouldEnforcePongMissReconnect => ("shouldEnforcePongMissReconnect", 0, false),
            GetWatchdogStaleThresholdMs => ("getWatchdogStaleThresholdMs", 0, false),
            MarketStatsHandler | TickerHandler | TickersHandler => ("", 1, false),
            PongHandler => ("", 0, false),
            IsWsProxyRouteEnabled => ("isWsProxyRouteEnabled", 0, false),
            IsTickerRestFallbackCoolingDown => ("isTickerRestFallbackCoolingDown", 0, false),
            HasHydratedTickerPrice => ("hasHydratedTickerPrice", 0, false),
            SyncTickerRestFallbackToPriceHydration => {
                ("syncTickerRestFallbackToPriceHydration", 1, false)
            }
            PrimeTickerRestFallback => ("primeTickerRestFallback", 0, false),
            ConnectAndSubscribe => ("connectAndSubscribe", 0, false),
            OnOpen => ("onOpen", 0, false),
            OnMarketsChanged => ("onMarketsChanged", 1, false),
            OnClose => ("onClose", 1, false),
            SendPayload => ("sendPayload", 1, false),
            ClearOhlcvNoDataTimeout => ("clearOhlcvNoDataTimeout", 1, false),
            ClearOrderBookStaleTimeout => ("clearOrderBookStaleTimeout", 1, false),
            ClearOrderBookInitialDataTimeout => ("clearOrderBookInitialDataTimeout", 1, false),
            ClearAllOrderBookInitialDataTimeouts => {
                ("clearAllOrderBookInitialDataTimeouts", 0, false)
            }
            ClearAllOrderBookStaleTimeouts => ("clearAllOrderBookStaleTimeouts", 0, false),
            StopOrderBookRestFallback => ("stopOrderBookRestFallback", 1, false),
            StopAllOrderBookRestFallback => ("stopAllOrderBookRestFallback", 0, false),
            IsOrderBookRestFallbackCoolingDown => ("isOrderBookRestFallbackCoolingDown", 1, false),
            ParseRestOrderBookLevels => ("parseRestOrderBookLevels", 1, false),
            FetchAndEmitOrderBookFromRest => ("fetchAndEmitOrderBookFromRest", 4, true),
            StartOrderBookRestFallback => ("startOrderBookRestFallback", 5, false),
            ClearTickerRestFallbackWatchdog => ("clearTickerRestFallbackWatchdog", 0, false),
            ArmTickerRestFallbackWatchdog => ("armTickerRestFallbackWatchdog", 0, false),
            ParseRestTickerItems => ("parseRestTickerItems", 1, false),
            FetchAndApplyTickerFallback => ("fetchAndApplyTickerFallback", 0, true),
            StopTickerRestFallback => ("stopTickerRestFallback", 0, false),
            StartTickerRestFallback => ("startTickerRestFallback", 1, false),
            ClearPendingSubscribeTimeouts => ("clearPendingSubscribeTimeouts", 0, false),
            ArmOrderBookStaleTimeout => ("armOrderBookStaleTimeout", 2, false),
            IsOrderBookTopic => ("isOrderBookTopic", 1, false),
            ArmExistingOrderBookTimeouts => ("armExistingOrderBookTimeouts", 0, false),
            ArmOhlcvNoDataTimeout => ("armOhlcvNoDataTimeout", 1, false),
            ArmExistingOhlcvNoDataTimeouts => ("armExistingOhlcvNoDataTimeouts", 0, false),
            AttemptOhlcvRecovery => ("attemptOhlcvRecovery", 2, false),
            MaybeRecoverOhlcvStream => ("maybeRecoverOhlcvStream", 1, false),
            AttemptOrderBookRecovery => ("attemptOrderBookRecovery", 3, false),
            Ping => ("ping", 0, false),
            Subscribe => ("subscribe", 0, false),
            SyncBboTickerTopics => ("syncBboTickerTopics", 0, false),
            OnMessage => ("onMessage", 1, false),
            ToOrderBookLevel => ("toOrderBookLevel", 1, false),
            AggregateOrderBookSnapshotLevels => ("aggregateOrderBookSnapshotLevels", 1, false),
            ApplyOrderBookLevels => ("applyOrderBookLevels", 4, false),
            HandleTickerEvent => ("handleTickerEvent", 1, false),
            HandleTickersEvent => ("handleTickersEvent", 1, false),
            HandleMarketStatsEvent => ("handleMarketStatsEvent", 1, false),
            ListenOHLCV => ("listenOHLCV", 2, false),
            ListenOrderBook => ("listenOrderBook", 2, false),
            Dispose => ("dispose", 0, false),
        };
        PublicMethodMetadata {
            name,
            length,
            asynchronous,
        }
    }
    /// Called by the realm with the arrow's traced lexical owner.
    pub fn invoke(
        self,
        contexts: &dyn PublicSocketContexts,
        owner: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        use PublicMethod::*;
        let undefined = Ok(AccountValue::Undefined);
        let a = |i: usize| arg(args, i);
        match self {
            ShouldEnforcePongMissReconnect => Ok(AccountValue::Boolean(
                ws_public::should_enforce_pong_miss_reconnect(),
            )),
            GetWatchdogStaleThresholdMs => Ok(AccountValue::Number(
                ws_public::get_watchdog_stale_threshold_ms(),
            )),
            MarketStatsHandler => own_call(
                contexts.socket(owner).as_ref(),
                "handleMarketStatsEvent",
                &[a(0)],
            ),
            TickerHandler => own_call(
                contexts.socket(owner).as_ref(),
                "handleTickerEvent",
                &[a(0)],
            ),
            TickersHandler => own_call(
                contexts.socket(owner).as_ref(),
                "handleTickersEvent",
                &[a(0)],
            ),
            PongHandler => own_call(contexts.socket(owner).as_ref(), "handlePongEvent", &[]),
            IsWsProxyRouteEnabled => Ok(AccountValue::Boolean(
                ws_public_subscriptions::is_ws_proxy_route_enabled(
                    contexts.subscriptions(owner).as_ref(),
                )?,
            )),
            IsTickerRestFallbackCoolingDown => {
                let h = contexts.socket(owner);
                let now = virtual_now(h.as_ref())?;
                let until = own(h.as_ref(), "tickerRestFallbackCooldownUntil")?;
                Ok(AccountValue::Boolean(match (&now, &until) {
                    (AccountValue::Number(now), AccountValue::Number(until)) => {
                        ws_public::is_ticker_rest_fallback_cooling_down(*now, *until)
                    }
                    _ => h.relational(Relational::Less, &now, &until)?,
                }))
            }
            HasHydratedTickerPrice => Ok(AccountValue::Boolean(
                ws_public::has_hydrated_ticker_price(&contexts.store_tickers(owner)?),
            )),
            SyncTickerRestFallbackToPriceHydration => {
                let h = contexts.socket(owner.clone());
                let reason = optional_string(h.as_ref(), &a(0))?;
                ws_public_readiness::sync_ticker_rest_fallback_to_price_hydration(
                    contexts.readiness(owner).as_ref(),
                    reason.as_deref(),
                )?;
                undefined
            }
            PrimeTickerRestFallback => {
                ws_public_readiness::prime_ticker_rest_fallback(
                    contexts.readiness(owner).as_ref(),
                )?;
                undefined
            }
            ConnectAndSubscribe => {
                let reconnect = match a(0) {
                    AccountValue::Undefined => None,
                    value => Some(value.truthy()),
                };
                ws_public_subscriptions::connect_and_subscribe(
                    contexts.subscriptions(owner).as_ref(),
                    reconnect,
                )?;
                undefined
            }
            OnOpen => {
                ws_public_readiness::on_open(contexts.readiness(owner).as_ref())?;
                undefined
            }
            OnMarketsChanged => {
                ws_public_subscriptions::on_markets_changed(
                    contexts.subscriptions(owner).as_ref(),
                    &a(0),
                )?;
                undefined
            }
            OnClose => {
                on_close(contexts.socket(owner), &a(0))?;
                undefined
            }
            SendPayload => {
                ws_public_subscriptions::send_payload(
                    contexts.subscriptions(owner).as_ref(),
                    &a(0),
                )?;
                undefined
            }
            ClearOhlcvNoDataTimeout
            | ClearOrderBookStaleTimeout
            | ClearOrderBookInitialDataTimeout
            | StopOrderBookRestFallback => {
                let topic = string(contexts.socket(owner.clone()).as_ref(), &a(0))?;
                let h = contexts.cleanup(owner);
                let body = match self {
                    ClearOhlcvNoDataTimeout => ws_public_cleanup::clear_ohlcv_no_data_timeout,
                    ClearOrderBookStaleTimeout => ws_public_cleanup::clear_order_book_stale_timeout,
                    ClearOrderBookInitialDataTimeout => {
                        ws_public_cleanup::clear_order_book_initial_data_timeout
                    }
                    _ => ws_public_cleanup::stop_order_book_rest_fallback,
                };
                body(h.as_ref(), &topic)?;
                undefined
            }
            ClearAllOrderBookInitialDataTimeouts => {
                ws_public_cleanup::clear_all_order_book_initial_data_timeouts(
                    contexts.cleanup(owner).as_ref(),
                )?;
                undefined
            }
            ClearAllOrderBookStaleTimeouts => {
                ws_public_cleanup::clear_all_order_book_stale_timeouts(
                    contexts.cleanup(owner).as_ref(),
                )?;
                undefined
            }
            StopAllOrderBookRestFallback => {
                ws_public_cleanup::stop_all_order_book_rest_fallback(
                    contexts.cleanup(owner).as_ref(),
                )?;
                undefined
            }
            IsOrderBookRestFallbackCoolingDown => {
                let topic = string(contexts.socket(owner.clone()).as_ref(), &a(0))?;
                Ok(AccountValue::Boolean(
                    ws_public_subscriptions::is_order_book_rest_fallback_cooling_down(
                        contexts.subscriptions(owner).as_ref(),
                        &topic,
                    )?,
                ))
            }
            ParseRestOrderBookLevels => contexts.parse_rest_order_book_levels(&a(0)),
            FetchAndEmitOrderBookFromRest => {
                let h = contexts.socket(owner);
                let promise =
                    fetch_and_emit_order_book_from_rest(h.clone(), a(0), a(1), a(2), a(3));
                Ok(h.promise_value(promise))
            }
            StartOrderBookRestFallback => {
                let h = contexts.socket(owner.clone());
                let topic = string(h.as_ref(), &a(0))?;
                let market_id = string(h.as_ref(), &a(1))?;
                let reason = string(h.as_ref(), &a(4))?;
                ws_public_subscriptions::start_order_book_rest_fallback(
                    contexts.subscriptions(owner),
                    &topic,
                    &market_id,
                    a(2),
                    a(3),
                    &reason,
                )?;
                undefined
            }
            ClearTickerRestFallbackWatchdog => {
                ws_public_readiness::clear_ticker_rest_fallback_watchdog(
                    contexts.readiness(owner).as_ref(),
                )?;
                undefined
            }
            ArmTickerRestFallbackWatchdog => {
                let h = contexts.socket(owner.clone());
                let delay = match a(0) {
                    AccountValue::Undefined => None,
                    value => Some(num(&value)?),
                };
                let reason = match a(1) {
                    AccountValue::Undefined => None,
                    value => Some(string(h.as_ref(), &value)?),
                };
                ws_public_readiness::arm_ticker_rest_fallback_watchdog(
                    contexts.readiness(owner),
                    delay,
                    reason.as_deref(),
                )?;
                undefined
            }
            ParseRestTickerItems => contexts.parse_rest_ticker_items(&a(0)),
            FetchAndApplyTickerFallback => {
                let h = contexts.socket(owner);
                let promise = fetch_and_apply_ticker_fallback(h.clone());
                Ok(h.promise_value(promise))
            }
            StopTickerRestFallback => {
                let log_recovered = match a(0) {
                    AccountValue::Undefined => None,
                    value => Some(value.truthy()),
                };
                ws_public_readiness::stop_ticker_rest_fallback(
                    contexts.readiness(owner).as_ref(),
                    log_recovered,
                )?;
                undefined
            }
            StartTickerRestFallback => {
                let reason = string(contexts.socket(owner.clone()).as_ref(), &a(0))?;
                ws_public_readiness::start_ticker_rest_fallback(
                    contexts.readiness(owner),
                    &reason,
                )?;
                undefined
            }
            ClearPendingSubscribeTimeouts => {
                ws_public_cleanup::clear_pending_subscribe_timeouts(
                    contexts.cleanup(owner).as_ref(),
                )?;
                undefined
            }
            ArmOrderBookStaleTimeout => {
                let topic = string(contexts.socket(owner.clone()).as_ref(), &a(0))?;
                ws_public_watchdogs::arm_order_book_stale_timeout(
                    contexts.watchdogs(owner),
                    &topic,
                    a(1),
                )?;
                undefined
            }
            IsOrderBookTopic => {
                let topic = string(contexts.socket(owner).as_ref(), &a(0))?;
                Ok(AccountValue::Boolean(
                    ws_public_watchdogs::is_order_book_topic(&topic),
                ))
            }
            ArmExistingOrderBookTimeouts => {
                ws_public_watchdogs::arm_existing_order_book_timeouts(
                    contexts.watchdogs(owner).as_ref(),
                )?;
                undefined
            }
            ArmOhlcvNoDataTimeout => {
                let topic = string(contexts.socket(owner.clone()).as_ref(), &a(0))?;
                ws_public_watchdogs::arm_ohlcv_no_data_timeout(contexts.watchdogs(owner), &topic)?;
                undefined
            }
            ArmExistingOhlcvNoDataTimeouts => {
                ws_public_watchdogs::arm_existing_ohlcv_no_data_timeouts(
                    contexts.watchdogs(owner).as_ref(),
                )?;
                undefined
            }
            AttemptOhlcvRecovery => {
                let h = contexts.socket(owner.clone());
                let topic = string(h.as_ref(), &a(0))?;
                let reason = string(h.as_ref(), &a(1))?;
                ws_public_watchdogs::attempt_ohlcv_recovery(
                    contexts.watchdogs(owner).as_ref(),
                    &topic,
                    &reason,
                )?;
                undefined
            }
            MaybeRecoverOhlcvStream => {
                let h = contexts.socket(owner.clone());
                let options = OptionsValue {
                    host: h.clone(),
                    opts: a(0),
                };
                let reason = match a(1) {
                    AccountValue::Undefined => None,
                    value => Some(string(h.as_ref(), &value)?),
                };
                ws_public_watchdogs::maybe_recover_ohlcv_stream(
                    contexts.watchdogs(owner).as_ref(),
                    &options,
                    reason.as_deref(),
                )?;
                undefined
            }
            AttemptOrderBookRecovery => {
                attempt_order_book_recovery(contexts.socket(owner).as_ref(), &a(0), &a(1), &a(2))?;
                undefined
            }
            Ping => {
                ws_public_readiness::ping(contexts.readiness(owner).as_ref())?;
                undefined
            }
            Subscribe => public_ingress::subscribe(contexts.ingress(owner.clone()), owner),
            SyncBboTickerTopics => {
                ws_public_subscriptions::sync_bbo_ticker_topics(
                    contexts.subscriptions(owner).as_ref(),
                )?;
                undefined
            }
            OnMessage => public_ingress::on_message(contexts.ingress(owner.clone()), owner, &a(0)),
            ToOrderBookLevel => {
                public_ingress::to_order_book_level(&contexts.ingress(owner), &a(0))
            }
            AggregateOrderBookSnapshotLevels => public_ingress::aggregate_snapshot_levels(
                contexts.ingress(owner.clone()),
                owner,
                a(0),
            ),
            ApplyOrderBookLevels => public_ingress::apply_book_levels(
                contexts.ingress(owner.clone()),
                owner,
                a(0),
                a(1),
                a(2),
                a(3),
            ),
            HandleTickerEvent => {
                public_ingress::ticker_event(contexts.ingress(owner.clone()), owner, a(0))
            }
            HandleTickersEvent => {
                ws_public_subscriptions::handle_tickers_event(
                    contexts.subscriptions(owner).as_ref(),
                    &a(0),
                )?;
                undefined
            }
            HandleMarketStatsEvent => {
                public_ingress::market_stats_event(contexts.ingress(owner.clone()), owner, a(0))
            }
            ListenOHLCV => listen_ohlcv(contexts.socket(owner), a(0), a(1)),
            ListenOrderBook => listen_order_book(contexts.socket(owner), a(0), a(1)),
            Dispose => {
                ws_public_watchdogs::dispose(contexts.watchdogs(owner).as_ref())?;
                undefined
            }
        }
    }
}
fn arg(args: &[AccountValue], index: usize) -> AccountValue {
    args.get(index).cloned().unwrap_or(AccountValue::Undefined)
}
/// Typed string parameter: strings pass through, other values take ToString.
fn string(h: &dyn PublicSocketHost, value: &AccountValue) -> SigningResult<Vec<u16>> {
    match value {
        AccountValue::String(s) => Ok(s.clone()),
        other => h.template(other),
    }
}
fn optional_string(
    h: &dyn PublicSocketHost,
    value: &AccountValue,
) -> SigningResult<Option<Vec<u16>>> {
    if value.truthy() {
        Ok(Some(string(h, value)?))
    } else {
        Ok(None)
    }
}
struct OptionsValue {
    host: Rc<dyn PublicSocketHost>,
    opts: AccountValue,
}
impl WatchdogOptions for OptionsValue {
    fn symbol(&self) -> SigningResult<Vec<u16>> {
        let symbol = get(self.host.as_ref(), &self.opts, "symbol")?;
        string(self.host.as_ref(), &symbol)
    }
    fn interval(&self) -> SigningResult<AccountValue> {
        get(self.host.as_ref(), &self.opts, "interval")
    }
}

pub type PublicSocketConstructor = fn(
    &dyn PublicSocketRealm,
    AccountValue,
    AccountValue,
    &[AccountValue],
) -> SigningResult<AccountValue>;
/// Required language operations, including exact BaseWebSocket super/newTarget.
pub trait PublicSocketRealm: PublicSocketContexts {
    fn binding_value(&self, name: &str) -> SigningResult<AccountValue>;
    /// Establish the derived constructor, prototype chain and lexical class TDZ.
    fn create_derived_class(
        &self,
        base: AccountValue,
        name: &str,
        length: usize,
        body: PublicSocketConstructor,
    ) -> SigningResult<AccountValue>;
    /// Arrow closure over the traced lexical owner with the body's metadata.
    fn arrow(&self, owner: &AccountValue, body: PublicMethod) -> SigningResult<AccountValue>;
    /// DefineField: writable/enumerable/configurable data property.
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()>;
    fn get_property(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue>;
    fn object_literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue>;
    fn array_literal(&self, values: &[AccountValue]) -> SigningResult<AccountValue>;
    fn finish_class_binding(&self, class: &AccountValue) -> SigningResult<()>;
    /// Exact BaseWebSocket constructor with original receiver/newTarget/throws.
    fn construct_base(
        &self,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue>;
}
const STATICS: [(&str, f64); 6] = [
    ("PROXY_ROUTE_FAILURES_BEFORE_DIRECT_FALLBACK", 3.0),
    ("PROXY_DIRECT_FALLBACK_FAILURE_COOLDOWN_MS", 120_000.0),
    ("DIRECT_ROUTE_FAILURES_BEFORE_READONLY", 2.0),
    ("EARLY_CLOSE_WINDOW_MS", 12_000.0),
    ("RATE_LIMIT_RECONNECT_DELAY_MS", 30_000.0),
    ("QUIET_STREAM_STALE_THRESHOLD_MS", 120_000.0),
];
const LATER_STATICS: [(&str, f64); 12] = [
    ("OHLCV_RECOVERY_RECONNECT_COOLDOWN_MS", 120_000.0),
    ("ORDER_BOOK_STALE_TIMEOUT_MS", 90_000.0),
    ("ORDER_BOOK_RECOVERY_COOLDOWN_MS", 45_000.0),
    ("ORDER_BOOK_INITIAL_DATA_TIMEOUT_MS", 1_500.0),
    ("ORDER_BOOK_REST_FALLBACK_INTERVAL_MS", 2_000.0),
    ("ORDER_BOOK_REST_FALLBACK_403_COOLDOWN_MS", 60_000.0),
    ("TICKER_REST_FALLBACK_CONNECTING_WATCHDOG_MS", 1_500.0),
    ("TICKER_REST_FALLBACK_WATCHDOG_MS", 8_000.0),
    ("TICKER_REST_FALLBACK_INTERVAL_MS", 3_000.0),
    ("TICKER_REST_FALLBACK_403_COOLDOWN_MS", 60_000.0),
    ("MAX_IMMEDIATE_SUBSCRIBE_MESSAGES", 24.0),
    ("SUBSCRIBE_STAGGER_DELAY_MS", 15.0),
];
enum Field {
    Bool(bool),
    Number(f64),
    Text(&'static str),
    Undefined,
    Record,
    Array,
    Arrow(PublicMethod),
}
const DATA_FIELDS: [(&str, Field); 33] = [
    ("recoversAfterOffline", Field::Bool(false)),
    ("bboTickerTopicPrefix", Field::Text("bbo_ticker_")),
    ("treatAnyMessageAsPong", Field::Bool(true)),
    (
        "shouldEnforcePongMissReconnect",
        Field::Arrow(PublicMethod::ShouldEnforcePongMissReconnect),
    ),
    (
        "getWatchdogStaleThresholdMs",
        Field::Arrow(PublicMethod::GetWatchdogStaleThresholdMs),
    ),
    ("failedConnections", Field::Number(0.0)),
    ("openedSinceLastConnect", Field::Bool(false)),
    ("hasEverConnected", Field::Bool(false)),
    ("useProxyRoute", Field::Bool(false)),
    ("useDirectFallbackRoute", Field::Bool(false)),
    ("useReadonlyDirectRoute", Field::Bool(false)),
    ("proxyRouteFailureCount", Field::Number(0.0)),
    ("directRouteFailureCount", Field::Number(0.0)),
    ("directFallbackProbePausedUntil", Field::Number(0.0)),
    ("connectedViaProxyRoute", Field::Bool(false)),
    ("connectStartedAt", Field::Number(0.0)),
    ("hasReceivedMessageSinceConnect", Field::Bool(false)),
    ("ohlcvNoDataTimeouts", Field::Record),
    ("orderBookStaleTimeouts", Field::Record),
    ("orderBookRecoveryAttempts", Field::Record),
    ("lastOrderBookRecoveryAtByTopic", Field::Record),
    ("lastOhlcvRecoveryReconnectAt", Field::Number(0.0)),
    ("pendingSubscribeTimeouts", Field::Array),
    ("orderBookInitialDataTimeouts", Field::Record),
    ("orderBookRestFallbackIntervals", Field::Record),
    ("orderBookRestFallbackInFlight", Field::Record),
    ("orderBookRestFallbackCooldownUntilByTopic", Field::Record),
    ("tickerRestFallbackWatchdog", Field::Undefined),
    ("tickerRestFallbackInterval", Field::Undefined),
    ("tickerRestFallbackInFlight", Field::Undefined),
    ("tickerRestFallbackCooldownUntil", Field::Number(0.0)),
    (
        "hasPrimedTickerRestFallbackForConnection",
        Field::Bool(false),
    ),
    ("hasWarnedTickerRestFallback", Field::Bool(false)),
];

// ts: LighterPublicWebsocket
pub struct LighterPublicWebsocket;
impl LighterPublicWebsocket {
    pub fn define(realm: &dyn PublicSocketRealm) -> SigningResult<AccountValue> {
        let base = realm.binding_value("BaseWebSocket")?;
        let class =
            realm.create_derived_class(base, "LighterPublicWebsocket", 1, Self::construct)?;
        for (key, value) in STATICS {
            realm.define_data(&class, key, AccountValue::Number(value))?;
        }
        let quiet = realm.get_property(&class, "QUIET_STREAM_STALE_THRESHOLD_MS")?;
        realm.define_data(&class, "OHLCV_DIRECT_ROUTE_NO_DATA_TIMEOUT_MS", quiet)?;
        for (key, value) in LATER_STATICS {
            realm.define_data(&class, key, AccountValue::Number(value))?;
        }
        realm.finish_class_binding(&class)?;
        Ok(class)
    }
    // ts: LighterPublicWebsocket.constructor
    pub fn construct(
        realm: &dyn PublicSocketRealm,
        class: AccountValue,
        new_target: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let owner = realm.construct_base(class, new_target, &[arg(args, 0)])?;
        initialize_fields(realm, &owner)?;
        Ok(owner)
    }
}
fn initialize_fields(realm: &dyn PublicSocketRealm, owner: &AccountValue) -> SigningResult<()> {
    for (key, field) in &DATA_FIELDS {
        let value = match field {
            Field::Bool(v) => AccountValue::Boolean(*v),
            Field::Number(v) => AccountValue::Number(*v),
            Field::Text(v) => text(v),
            Field::Undefined => AccountValue::Undefined,
            Field::Record => realm.object_literal(&[])?,
            Field::Array => realm.array_literal(&[])?,
            Field::Arrow(body) => realm.arrow(owner, *body)?,
        };
        realm.define_data(owner, key, value)?;
    }
    let topics = realm.object_literal(&[("marketStatsAll", text("market_stats/all"))])?;
    realm.define_data(owner, "topics", topics)?;
    let mut entries = Vec::new();
    for (key, body) in [
        ("market_stats:all", PublicMethod::MarketStatsHandler),
        ("ticker", PublicMethod::TickerHandler),
        ("tickers", PublicMethod::TickersHandler),
        ("pong", PublicMethod::PongHandler),
    ] {
        let handler = realm.arrow(owner, body)?;
        entries.push((key, realm.array_literal(&[handler])?));
    }
    let handlers = realm.object_literal(&entries)?;
    realm.define_data(owner, "messageHandlers", handlers)?;
    for body in ARROW_FIELDS {
        let value = realm.arrow(owner, body)?;
        realm.define_data(owner, body.metadata().name, value)?;
    }
    Ok(())
}
