use safe_cex_venue_cryptocom::{
    native_bindings::BindingOps,
    runtime::{self as port, RuntimeOps},
    socket_handlers::{SocketCallback, SocketFuture, SocketOps},
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::{Pin, pin},
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
type V = Rc<Native>;
type E = V;
enum Native {
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    Symbol(String),
    O(RefCell<Vec<(V, V)>>),
    A(RefCell<Vec<V>>),
    F(String, usize, SocketCallback<V, E>),
    Builtin(String),
    Promise(RefCell<PromiseState>),
    Deferred(String, V),
    Regex(String, String),
    Class {
        name: String,
        length: usize,
        base: V,
        body: Rc<dyn Fn(V, V, Vec<V>) -> Result<V, E>>,
    },
}
fn u() -> V {
    Rc::new(Native::U)
}
fn s(x: &str) -> V {
    Rc::new(Native::S(x.encode_utf16().collect()))
}
fn n(x: f64) -> V {
    Rc::new(Native::N(x))
}
fn b(x: bool) -> V {
    Rc::new(Native::B(x))
}
fn array(x: Vec<V>) -> V {
    Rc::new(Native::A(RefCell::new(x)))
}
fn object(x: Vec<(&str, V)>) -> V {
    Rc::new(Native::O(RefCell::new(
        x.into_iter().map(|(k, v)| (s(k), v)).collect(),
    )))
}
fn keys(x: Vec<(V, V)>) -> V {
    let out = object(vec![]);
    for (k, v) in x {
        put(&out, k, v);
    }
    if let Native::O(fields) = out.as_ref() {
        fields
            .borrow_mut()
            .sort_by_key(|(k, _)| matches!(k.as_ref(), Native::Symbol(_)));
    }
    out
}
fn text(v: &V) -> String {
    match v.as_ref() {
        Native::U => "undefined".into(),
        Native::Null => "null".into(),
        Native::S(x) => String::from_utf16_lossy(x),
        Native::N(x) => safe_cex_num::js_number_string(*x),
        Native::B(x) => x.to_string(),
        _ => "[object Object]".into(),
    }
}
fn number(v: &V) -> f64 {
    match v.as_ref() {
        Native::N(x) => *x,
        Native::U => f64::NAN,
        Native::Null => 0.,
        Native::B(x) => {
            if *x {
                1.
            } else {
                0.
            }
        }
        Native::S(x) => String::from_utf16_lossy(x).parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn truthy(v: &V) -> bool {
    match v.as_ref() {
        Native::U | Native::Null => false,
        Native::N(x) => *x != 0. && !x.is_nan(),
        Native::B(x) => *x,
        Native::S(x) => !x.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Native::U, Native::U) | (Native::Null, Native::Null) => true,
        (Native::B(a), Native::B(b)) => a == b,
        (Native::N(a), Native::N(b)) => a == b,
        (Native::S(a), Native::S(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn raw(v: &V, key: &V) -> V {
    match v.as_ref() {
        Native::O(fields) => fields
            .borrow()
            .iter()
            .find(|(k, _)| equal(k, key))
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Native::A(a) => {
            let key = text(key);
            if key == "length" {
                n(a.borrow().len() as f64)
            } else {
                key.parse::<usize>()
                    .ok()
                    .and_then(|i| a.borrow().get(i).cloned())
                    .unwrap_or_else(u)
            }
        }
        _ => u(),
    }
}
fn get(v: &V, key: &str) -> V {
    raw(v, &s(key))
}
fn put(v: &V, key: V, value: V) {
    let Native::O(fields) = v.as_ref() else {
        panic!("native object assignment")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| equal(k, &key)) {
        *v = value
    } else {
        fields.push((key, value));
    }
}
fn values(v: &V) -> Vec<V> {
    let Native::A(a) = v.as_ref() else {
        panic!("native array")
    };
    a.borrow().clone()
}
fn decode(v: &Value) -> V {
    match v {
        Value::Null => Rc::new(Native::Null),
        Value::Bool(x) => b(*x),
        Value::Number(x) => n(x.as_f64().unwrap()),
        Value::String(x) => s(x),
        Value::Array(x) => array(x.iter().map(decode).collect()),
        Value::Object(x) => {
            if x.get("$undefined") == Some(&json!(true)) {
                return u();
            }
            if let Some(x) = x.get("$number").and_then(Value::as_str) {
                return n(match x {
                    "NaN" => f64::NAN,
                    "Infinity" => f64::INFINITY,
                    "-Infinity" => f64::NEG_INFINITY,
                    "-0" => -0.,
                    _ => panic!("number tag"),
                });
            }
            keys(x.iter().map(|(k, v)| (s(k), decode(v))).collect())
        }
    }
}
enum PromiseState {
    Running(SocketFuture<V, E>),
    Done(Result<V, E>),
}
fn context() -> Context<'static> {
    Context::from_waker(Waker::noop())
}
fn run(f: SocketFuture<V, E>) -> Result<V, E> {
    let mut f = pin!(f);
    let mut cx = context();
    for _ in 0..100 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("NATIVE-ASYNC-POLL-FLOOR")
}
struct Yield(bool);
impl Future for Yield {
    type Output = ();
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            Poll::Ready(())
        } else {
            self.0 = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}
struct PromiseWait(V);
impl Future for PromiseWait {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let Native::Promise(state) = self.0.as_ref() else {
            panic!("promise")
        };
        let mut state = state.borrow_mut();
        match &mut *state {
            PromiseState::Done(v) => Poll::Ready(v.clone()),
            PromiseState::Running(f) => {
                let result = f.as_mut().poll(cx);
                if let Poll::Ready(v) = &result {
                    *state = PromiseState::Done(v.clone());
                }
                result
            }
        }
    }
}
struct Host {
    tasks: RefCell<Vec<V>>,
    spec: Value,
    tags: RefCell<Vec<(V, String)>>,
    events: RefCell<Vec<V>>,
    timers: RefCell<Vec<(V, V)>>,
    updates: RefCell<Vec<V>>,
    logs: RefCell<Vec<V>>,
    api_config: RefCell<Vec<V>>,
    api_factory: RefCell<Option<V>>,
    signed: RefCell<Vec<V>>,
    defined: RefCell<Vec<(String, V)>>,
    root: V,
    store: V,
    options: V,
    markets: V,
    tickers: V,
    positions: V,
    balance: V,
    orders: V,
    xhr: V,
    phase: V,
    every: V,
    periodic: V,
    rule: V,
    limited: V,
    request: RefCell<Option<V>>,
    response: RefCell<Option<V>>,
    rejection: RefCell<Option<V>>,
    retry: RefCell<Option<V>>,
    last_tick: RefCell<Option<V>>,
    target: RefCell<Option<V>>,
    getter: RefCell<Option<Rc<dyn Fn(V) -> Result<V, E>>>>,
    mode: RefCell<Option<port::NativeMethod<V, E>>>,
    regexes: RefCell<Vec<V>>,
    weak_members: RefCell<Vec<V>>,
}
fn json(v: &V) -> String {
    match v.as_ref() {
        Native::Null => "null".into(),
        Native::B(x) => x.to_string(),
        Native::N(x) if x.is_finite() => {
            serde_json::to_string(&serde_json::Number::from_f64(*x).unwrap())
                .unwrap()
                .trim_end_matches(".0")
                .to_string()
        }
        Native::N(_) => "null".into(),
        Native::S(_) => serde_json::to_string(&text(v)).unwrap(),
        Native::A(items) => format!(
            "[{}]",
            items
                .borrow()
                .iter()
                .map(|item| if matches!(item.as_ref(), Native::U) {
                    "null".into()
                } else {
                    json(item)
                })
                .collect::<Vec<_>>()
                .join(",")
        ),
        Native::O(fields) => format!(
            "{{{}}}",
            fields
                .borrow()
                .iter()
                .filter(|(_, value)| !matches!(value.as_ref(), Native::U))
                .map(|(key, value)| format!("{}:{}", json(key), json(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => panic!("NATIVE-JSON-FIXTURE-BOUNDARY"),
    }
}
impl Host {
    fn tag(&self, v: V, label: &str) -> V {
        self.tags.borrow_mut().push((v.clone(), label.into()));
        v
    }
    fn label(&self, v: &V) -> Option<String> {
        self.tags
            .borrow()
            .iter()
            .find(|(x, _)| Rc::ptr_eq(x, v))
            .map(|(_, s)| s.clone())
    }
    fn event(&self, name: &str, args: Vec<V>) -> Result<(), E> {
        let mut fields = vec![s(name)];
        fields.extend(args);
        self.events.borrow_mut().push(array(fields));
        if self.spec["fail"] == name {
            Err(self.phase.clone())
        } else {
            Ok(())
        }
    }
    fn property(&self, v: &V, key: &str) -> Result<V, E> {
        if let Some(tag) = self.label(v) {
            if [
                "root",
                "store",
                "options",
                "config",
                "response-input",
                "retry-error",
                "xhr",
                "interceptors",
                "request",
                "response",
                "public",
                "private",
                "Math",
                "JSON",
                "weakset",
                "BASE_URLS",
                "LogSeverity",
                "PositionMode",
            ]
            .contains(&tag.as_str())
            {
                self.event(&format!("get:{tag}:{key}"), vec![])?;
            }
        }
        match v.as_ref() {
            Native::S(_) => Ok(match key {
                "replace" => Rc::new(Native::Builtin("string:replace".into())),
                _ => u(),
            }),
            Native::A(_) => Ok(if key == "map" {
                Rc::new(Native::Builtin("array:map".into()))
            } else {
                get(v, key)
            }),
            Native::O(_) => Ok(get(v, key)),
            _ => Err(self.phase.clone()),
        }
    }
    fn assign(&self, v: &V, key: &str, value: V) -> Result<(), E> {
        if let Some(tag) = self.label(v) {
            if ["root", "store", "config", "response-input", "options"].contains(&tag.as_str()) {
                self.event(&format!("set:{tag}:{key}"), vec![])?;
            }
        }
        put(v, s(key), value);
        Ok(())
    }
    fn promise(&self, mut future: SocketFuture<V, E>) -> V {
        let state = match future.as_mut().poll(&mut context()) {
            Poll::Ready(v) => PromiseState::Done(v.and_then(|value| {
                if matches!(value.as_ref(), Native::O(_)) {
                    let then = self.property(&value, "then")?;
                    assert!(
                        matches!(then.as_ref(), Native::U),
                        "NATIVE-PROMISE-FIXTURE-BOUNDARY"
                    );
                }
                Ok(value)
            })),
            Poll::Pending => PromiseState::Running(future),
        };
        {
            let promise = Rc::new(Native::Promise(RefCell::new(state)));
            self.tasks.borrow_mut().push(promise.clone());
            promise
        }
    }
    fn await_value(self: &Rc<Self>, value: V) -> SocketFuture<V, E> {
        let h = self.clone();
        Box::pin(async move {
            Yield(false).await;
            match value.as_ref() {
                Native::Deferred(name, v) => {
                    h.event(&format!("resolve:{name}"), vec![])?;
                    if h.spec["disposeAfter"] == name.as_str() {
                        put(&h.root, s("isDisposed"), b(true));
                    }
                    if h.spec["reject"] == name.as_str() {
                        Err(h.phase.clone())
                    } else {
                        Ok(v.clone())
                    }
                }
                Native::Promise(_) => {
                    let output = PromiseWait(value).await?;
                    if matches!(output.as_ref(), Native::O(_)) {
                        let then = h.property(&output, "then")?;
                        assert!(
                            matches!(then.as_ref(), Native::U),
                            "NATIVE-AWAIT-FIXTURE-BOUNDARY"
                        );
                    }
                    Ok(output)
                }
                _ => Ok(value),
            }
        })
    }
    fn global(self: &Rc<Self>, name: &str) -> Result<V, E> {
        self.event(&format!("global:{name}"), vec![])?;
        Ok(match name {
            "createAPI" => {
                if self.api_factory.borrow().is_none() {
                    let h = self.clone();
                    let factory = Rc::new(Native::F(
                        "createAPI".into(),
                        2,
                        Rc::new(move |args| {
                            port::create_api(h.ops(), args[0].clone(), args[1].clone())
                        }),
                    ));
                    self.api_factory.replace(Some(factory));
                }
                self.api_factory.borrow().as_ref().unwrap().clone()
            }
            "BASE_URLS" => self.tag(
                object(vec![
                    ("TESTNET", s("https://uat-api.3ona.co/exchange/v1")),
                    ("PRODUCTION", s("https://api.crypto.com/exchange/v1")),
                ]),
                name,
            ),
            "Math" => self.tag(
                object(vec![("min", Rc::new(Native::Builtin("global:min".into())))]),
                name,
            ),
            "JSON" => self.tag(
                object(vec![(
                    "stringify",
                    Rc::new(Native::Builtin("global:stringify".into())),
                )]),
                name,
            ),
            "LogSeverity" => self.tag(object(vec![("Error", s("error"))]), name),
            "PositionMode" => self.tag(object(vec![("Hedged", s("hedged"))]), name),
            "ENDPOINTS" => object(vec![("ACCOUNTS", s("/private/get-accounts"))]),
            "GRAFANA_ENDPOINTS_EVERY" => self.every.clone(),
            "GRAFANA_ENDPOINTS_PERIODIC" => self.periodic.clone(),
            _ => Rc::new(Native::Builtin(format!("global:{name}"))),
        })
    }
    fn invoke(self: &Rc<Self>, callee: V, receiver: V, args: Vec<V>) -> Result<V, E> {
        match callee.as_ref() {
            Native::F(_, _, f) => f(args),
            Native::Builtin(name) if name == "string:replace" => Ok(s(&text(&receiver).replacen(
                &text(&args[0]),
                &text(&args[1]),
                1,
            ))),
            Native::Builtin(name) if name == "array:map" => {
                let f = args[0].clone();
                let mut out = vec![];
                for (v_index, v) in values(&receiver).into_iter().enumerate() {
                    out.push(self.invoke(
                        f.clone(),
                        u(),
                        vec![v, n(v_index as f64), receiver.clone()],
                    )?);
                }
                Ok(array(out))
            }
            Native::Builtin(name) => {
                self.event(
                    &format!("call:{name}"),
                    if name.starts_with("root:")
                        && name != "root:log"
                        && name != "root:emitAndLogError"
                        && name != "root:createXhrWithSecondaryKey"
                        || name.starts_with("public:")
                        || name.starts_with("private:")
                    {
                        args.clone()
                    } else {
                        vec![]
                    },
                )?;
                let first = || args.first().cloned().unwrap_or_else(u);
                match name.as_str() {
                    "root:initExternalPublicData"
                    | "root:fetchMarkets"
                    | "root:fetchTickers"
                    | "root:fetchPositions"
                    | "root:fetchBalance"
                    | "root:fetchOrders"
                    | "root:tick" => {
                        assert!(Rc::ptr_eq(&receiver, &self.root), "NATIVE-RECEIVER");
                        let key = name.strip_prefix("root:").unwrap();
                        let value = match key {
                            "initExternalPublicData" => b(self.spec["external"] == true),
                            "fetchMarkets" => self.markets.clone(),
                            "fetchTickers" => self.tickers.clone(),
                            "fetchPositions" => self.positions.clone(),
                            "fetchBalance" => self.balance.clone(),
                            "fetchOrders" => self.orders.clone(),
                            _ => u(),
                        };
                        let promise = self.tag(
                            Rc::new(Native::Deferred(key.into(), value)),
                            &format!("promise:{key}"),
                        );
                        if key == "tick" {
                            self.last_tick.replace(Some(promise.clone()));
                        }
                        Ok(promise)
                    }
                    "root:log" | "root:emitAndLogError" => {
                        assert!(Rc::ptr_eq(&receiver, &self.root));
                        self.logs.borrow_mut().push(array(args));
                        Ok(u())
                    }
                    "store:update" => {
                        assert!(Rc::ptr_eq(&receiver, &self.store));
                        let patch = first();
                        self.updates.borrow_mut().push(patch.clone());
                        if let Native::O(fields) = patch.as_ref() {
                            for (k, v) in fields.borrow().iter() {
                                put(&self.store, k.clone(), v.clone());
                            }
                        }
                        Ok(u())
                    }
                    "request:use" => {
                        assert_eq!(
                            self.label(&receiver).as_deref(),
                            Some("request"),
                            "NATIVE-INTERCEPTOR-RECEIVER"
                        );
                        self.request.replace(Some(first()));
                        Ok(self.tag(object(vec![]), "ignored:use"))
                    }
                    "response:use" => {
                        assert_eq!(
                            self.label(&receiver).as_deref(),
                            Some("response"),
                            "NATIVE-INTERCEPTOR-RECEIVER"
                        );
                        self.response.replace(Some(first()));
                        self.rejection.replace(Some(args[1].clone()));
                        Ok(self.tag(object(vec![]), "ignored:use"))
                    }
                    "global:min" => Ok(n(number(&args[0]).min(number(&args[1])))),
                    "global:stringify" => Ok(s(&json(&first()))),
                    "weakset:has" | "weakset:add" => {
                        assert_eq!(self.label(&receiver).as_deref(), Some("weakset"));
                        let value = first();
                        let mut members = self.weak_members.borrow_mut();
                        if name == "weakset:add" {
                            members.push(value);
                            return Ok(receiver);
                        }
                        Ok(b(members.iter().any(|member| Rc::ptr_eq(member, &value))))
                    }
                    "global:signatureRejectedResponse" => {
                        assert!(
                            matches!(first().as_ref(), Native::Builtin(name) if name == "global:isCryptocomSignatureRejection"),
                            "NATIVE-SIGNATURE-PREDICATE"
                        );
                        Ok(Rc::new(Native::Builtin("global:signatureCondition".into())))
                    }
                    "global:signatureCondition" => Ok(self
                        .spec
                        .get("signature")
                        .map(decode)
                        .unwrap_or_else(|| b(false))),
                    "global:resendOnSignatureRejection" => {
                        assert!(Rc::ptr_eq(&args[0], &self.xhr), "NATIVE-RESEND-CLIENT");
                        assert!(
                            matches!(args[2].as_ref(), Native::Builtin(name) if name == "global:isCryptocomSignatureRejection"),
                            "NATIVE-RESEND-PREDICATE"
                        );
                        Ok(if self.spec["resend"] == true {
                            self.tag(object(vec![]), "resent")
                        } else {
                            u()
                        })
                    }
                    "global:sumBy" => {
                        assert!(Rc::ptr_eq(&args[0], &self.positions));
                        Ok(self
                            .spec
                            .get("sumReturn")
                            .map(decode)
                            .unwrap_or_else(|| n(7.)))
                    }
                    "global:setTimeout" => {
                        let delay = args[1].clone();
                        self.event(&format!("timer:{}", text(&delay)), vec![])?;
                        self.timers.borrow_mut().push((first(), delay.clone()));
                        if number(&delay) == 1000. {
                            self.invoke(first(), u(), vec![])?;
                        }
                        Ok(self.tag(object(vec![]), "timer-return"))
                    }
                    "global:safeCexAxiosCreate" => {
                        assert!(
                            Rc::ptr_eq(&args[1], &self.options) && Rc::ptr_eq(&args[2], &self.root),
                            "NATIVE-API-INPUTS"
                        );
                        self.api_config.borrow_mut().push(first());
                        Ok(self.xhr.clone())
                    }
                    "global:retry" => {
                        assert!(Rc::ptr_eq(&args[0], &self.xhr));
                        assert!(equal(&get(&args[1], "retries"), &n(3.)), "NATIVE-RETRIES");
                        self.retry.replace(Some(get(&args[1], "retryCondition")));
                        Ok(self.tag(object(vec![]), "ignored:retry"))
                    }
                    "global:installTransportAdapter" => {
                        assert!(
                            Rc::ptr_eq(&args[0], &self.xhr) && Rc::ptr_eq(&args[1], &self.options)
                        );
                        assert_eq!(text(&args[2]), "Crypto.com");
                        Ok(self.tag(object(vec![]), "ignored:adapter"))
                    }
                    "global:isNetworkError" => Ok(self
                        .spec
                        .get("network")
                        .map(decode)
                        .unwrap_or_else(|| b(true))),
                    "global:isPublicEndpoint" => Ok(b(
                        safe_cex_venue_cryptocom::api::is_public_endpoint(&text(&first())),
                    )),
                    "global:isNonRetryablePrivateWrite" => {
                        let value = first();
                        let url = text(&value);
                        Ok(b(
                            safe_cex_venue_cryptocom::api::is_non_retryable_private_write(
                                if matches!(value.as_ref(), Native::U) {
                                    None
                                } else {
                                    Some(&url)
                                },
                            ),
                        ))
                    }
                    "global:isRecord" => Ok(b(matches!(first().as_ref(), Native::O(_)))),
                    "global:parseJsonObject" => Ok(serde_json::from_str::<Value>(&text(&first()))
                        .ok()
                        .filter(Value::is_object)
                        .map(|v| decode(&v))
                        .unwrap_or_else(u)),
                    "global:isAuthenticatedEnvelope" => {
                        let value = first();
                        Ok(b(matches!(value.as_ref(), Native::O(_))
                            && equal(&get(&value, "method"), &args[1])
                            && equal(&get(&value, "api_key"), &args[2])
                            && matches!(get(&value, "params").as_ref(), Native::O(_))
                            && matches!(get(&value, "sig").as_ref(), Native::S(_))
                            && matches!(get(&value, "nonce").as_ref(), Native::N(_))))
                    }
                    "global:generateAuthenticatedRequest" => {
                        self.signed.borrow_mut().push(array(args.clone()));
                        Ok(self.tag(
                            object(vec![
                                ("method", args[0].clone()),
                                ("params", args[1].clone()),
                                ("api_key", args[2].clone()),
                                ("secret", args[3].clone()),
                            ]),
                            "signed",
                        ))
                    }
                    "global:shouldSuppressValidationAuthErrorEmission" => Ok(b(text(&args[1])
                        == "40101"
                        && equal(
                            &self.property(&args[0], "suppressCryptocomValidationErrorEmission")?,
                            &b(true),
                        ))),
                    "global:formatCryptocomApiError" => {
                        let response = get(&first(), "response");
                        let data = get(&response, "data");
                        if matches!(data.as_ref(), Native::U) {
                            Ok(u())
                        } else {
                            Ok(s(&format!(
                                "Crypto.com proxy/transport error ({}): {}",
                                text(&get(&response, "status")),
                                text(&data)
                            )))
                        }
                    }
                    "global:patternClientOrderIdRule" => {
                        let Native::Regex(pattern, flags) = args[1].as_ref() else {
                            panic!("NATIVE-REGEXP")
                        };
                        self.event("pattern", vec![s(pattern), s(flags)])?;
                        assert_eq!(pattern, "^.{1,36}$", "NATIVE-PATTERN");
                        assert_eq!(flags, "");
                        self.regexes.borrow_mut().push(args[1].clone());
                        Ok(self.rule.clone())
                    }
                    "global:rateLimit" => {
                        assert!(Rc::ptr_eq(&args[0], &self.xhr));
                        let max = get(&args[1], "maxRPS");
                        assert!(equal(&max, &n(50.)), "CRYPTOCOM-RUNTIME-NATIVE-CTOR-RATE");
                        self.event("rate", vec![max])?;
                        Ok(self.limited.clone())
                    }
                    "root:createXhrWithSecondaryKey" => {
                        assert!(
                            Rc::ptr_eq(&receiver, &self.root),
                            "NATIVE-CONSTRUCTOR-HELPER-RECEIVER"
                        );
                        assert!(Rc::ptr_eq(&args[0], &self.options));
                        assert!(
                            Rc::ptr_eq(&args[1], self.api_factory.borrow().as_ref().unwrap()),
                            "NATIVE-CREATE-API-CALLABLE"
                        );
                        if self.spec["constructorAPI"] == true {
                            let x = self.invoke(
                                args[1].clone(),
                                u(),
                                vec![args[0].clone(), receiver.clone()],
                            )?;
                            assert!(Rc::ptr_eq(&x, &self.xhr), "NATIVE-CONSTRUCTOR-API-RESULT");
                        }
                        let limited =
                            self.invoke(get(&args[2], "rateLimit"), u(), vec![self.xhr.clone()])?;
                        assert!(Rc::ptr_eq(&limited, &self.limited));
                        Ok(self.xhr.clone())
                    }
                    name if name.starts_with("public:") || name.starts_with("private:") => {
                        let side = name.split(':').next().unwrap();
                        assert_eq!(
                            self.label(&receiver).as_deref(),
                            Some(side),
                            "NATIVE-SOCKET-RECEIVER"
                        );
                        let key = name.split(':').nth(1).unwrap();
                        Ok(self.tag(object(vec![]), &format!("ignored:{key}")))
                    }
                    _ => panic!("unhandled native dependency {name}"),
                }
            }
            _ => panic!("not callable"),
        }
    }
    fn construct(self: &Rc<Self>, ctor: V, args: Vec<V>) -> Result<V, E> {
        let Native::Builtin(name) = ctor.as_ref() else {
            panic!("ctor")
        };
        let name = name.strip_prefix("global:").unwrap();
        self.event(&format!("construct:{name}"), vec![])?;
        match name {
            "Promise" => {
                let resolver = Rc::new(Native::F("resolve".into(), 1, Rc::new(|_| Ok(u()))));
                self.invoke(args[0].clone(), u(), vec![resolver])?;
                Ok(self.promise(Box::pin(async { Ok(u()) })))
            }
            "Error" => Ok(self.tag(
                object(vec![("message", args[0].clone())]),
                "error:constructed",
            )),
            "WeakSet" => Ok(self.tag(
                object(vec![
                    ("has", Rc::new(Native::Builtin("weakset:has".into()))),
                    ("add", Rc::new(Native::Builtin("weakset:add".into()))),
                ]),
                "weakset",
            )),
            "CryptocomPublicWebSocket" | "CryptocomPrivateWebSocket" => {
                assert!(Rc::ptr_eq(&args[0], &self.root));
                Ok(self.tag(
                    object(vec![]),
                    if name == "CryptocomPublicWebSocket" {
                        "public-created"
                    } else {
                        "private-created"
                    },
                ))
            }
            _ => panic!("unknown ctor"),
        }
    }
    fn ops(self: &Rc<Self>) -> Rc<RuntimeOps<V, E>> {
        let host = self.clone();
        let th = self.clone();
        let h = self.clone();
        let gh = self.clone();
        let ih = self.clone();
        let sh = self.clone();
        let aw = self.clone();
        let spread = self.clone();
        let value = Rc::new(SocketOps {
            undefined: Rc::new(u),
            boolean: Rc::new(b),
            number: Rc::new(n),
            literal: Rc::new(s),
            object: Rc::new(object),
            array: Rc::new(array),
            get: Rc::new(move |v, k| h.property(v, k)),
            index: Rc::new(move |v, k| ih.property(v, &text(k))),
            key: Rc::new(|v| Ok(v.clone())),
            set: Rc::new(move |v, k, x| sh.assign(v, k, x)),
            set_index: Rc::new(|v, k, x| {
                put(v, k.clone(), x);
                Ok(())
            }),
            delete: Rc::new(|_, _| Ok(true)),
            global: Rc::new(move |name| gh.global(name)),
            invoke: Rc::new(move |f, r, a| host.invoke(f, r, a)),
            function: Rc::new(|name, len, body| Rc::new(Native::F(name.into(), len, body))),
            truthy: Rc::new(truthy),
            nullish: Rc::new(|v| matches!(v.as_ref(), Native::U | Native::Null)),
            equal: Rc::new(equal),
            is_number: Rc::new(|v| matches!(v.as_ref(), Native::N(_))),
            has: Rc::new(|v, k| Ok(!matches!(get(v, k).as_ref(), Native::U))),
            numeric: Rc::new(|v| Ok(number(v))),
            text: Rc::new(move |v| {
                Ok(if Rc::ptr_eq(v, &th.phase) {
                    "PHASE".encode_utf16().collect()
                } else {
                    text(v).encode_utf16().collect()
                })
            }),
            utf16: Rc::new(|v| Rc::new(Native::S(v))),
            error_value: Rc::new(|e| e),
            spread: Rc::new(|v| Ok(values(v))),
            spread_object: Rc::new(move |v| {
                if let Native::O(fields) = v.as_ref() {
                    let fields = fields.borrow().clone();
                    fields
                        .iter()
                        .map(|(k, _)| {
                            Ok((
                                k.clone(),
                                if matches!(k.as_ref(), Native::Symbol(_)) {
                                    raw(v, k)
                                } else {
                                    spread.property(v, &text(k))?
                                },
                            ))
                        })
                        .collect()
                } else {
                    Ok(vec![])
                }
            }),
            object_keys: Rc::new(keys),
            tuple_two: Rc::new(|v| {
                let values = values(v);
                Ok((
                    values.first().cloned().unwrap_or_else(u),
                    values.get(1).cloned().unwrap_or_else(u),
                ))
            }),
            await_value: Rc::new(move |v| aw.await_value(v)),
        });
        let ct = self.clone();
        let pr = self.clone();
        let df = self.clone();
        let sc = self.clone();
        let proto = self.clone();
        let getter = self.clone();
        let binding_value = value.clone();
        let bc = self.clone();
        let bp = self.clone();
        let binding = Rc::new(BindingOps {
            null: Rc::new(|| Rc::new(Native::Null)),
            is_string: Rc::new(|v| matches!(v.as_ref(), Native::S(_))),
            value: binding_value,
            binary: Rc::new(|op, l, r| {
                Ok(match op {
                    "ToNumeric" => n(number(&l)),
                    "++" => n(number(&l) + 1.),
                    ">" => b(number(&l) > number(&r)),
                    ">=" => b(number(&l) >= number(&r)),
                    "<" => b(number(&l) < number(&r)),
                    "<=" => b(number(&l) <= number(&r)),
                    "+" => n(number(&l) + number(&r)),
                    "-" => n(number(&l) - number(&r)),
                    "/" => n(number(&l) / number(&r)),
                    _ => panic!("unexercised native operator {op}"),
                })
            }),
            construct: Rc::new(move |ctor, args| bc.construct(ctor, args)),
            throw_value: Rc::new(|v| v),
            promise: Rc::new(move |f| bp.promise(f)),
            iterator: Rc::new(|v| {
                assert!(
                    matches!(v.as_ref(), Native::A(_)),
                    "witness iterator supports native arrays"
                );
                Ok(object(vec![("source", v), ("offset", n(0.))]))
            }),
            next: Rc::new(|record| {
                let source = get(record, "source");
                let Native::A(rows) = source.as_ref() else {
                    panic!("array iterator")
                };
                let offset = number(&get(record, "offset")) as usize;
                let v = rows.borrow().get(offset).cloned();
                put(record, s("offset"), n((offset + 1) as f64));
                Ok(v)
            }),
            close: Rc::new(|_| Ok(())),
            close_throw: Rc::new(|_, e| e),
        });
        Rc::new(RuntimeOps {
            value,
            is_string: Rc::new(|v| matches!(v.as_ref(), Native::S(_))),
            regexp: Rc::new(|pattern, flags| {
                Ok(Rc::new(Native::Regex(pattern.into(), flags.into())))
            }),
            binary: Rc::new(|op, left, right| {
                Ok(match op {
                    ">" => b(number(&left) > number(&right)),
                    "%" => n(number(&left) % number(&right)),
                    "+" => {
                        if matches!(left.as_ref(), Native::S(_))
                            || matches!(right.as_ref(), Native::S(_))
                        {
                            s(&(text(&left) + &text(&right)))
                        } else {
                            n(number(&left) + number(&right))
                        }
                    }
                    _ => panic!("operator"),
                })
            }),
            construct: Rc::new(move |ctor, args| ct.construct(ctor, args)),
            throw_value: Rc::new(|v| v),
            promise: Rc::new(move |f| pr.promise(f)),
            define: Rc::new(move |r, k, v| {
                df.event(&format!("define:{k}"), vec![])?;
                df.defined.borrow_mut().push((k.into(), v.clone()));
                put(r, s(k), v);
                Ok(())
            }),
            superclass: Rc::new(|active| {
                let Native::Class { base, .. } = active.as_ref() else {
                    panic!("class")
                };
                Ok(base.clone())
            }),
            super_construct: Rc::new(move |base, target, args| {
                assert!(
                    matches!(base.as_ref(),Native::Builtin(name)if name=="global:BaseExchange")
                );
                assert!(
                    Rc::ptr_eq(&target, sc.target.borrow().as_ref().unwrap()),
                    "NATIVE-NEWTARGET"
                );
                assert_eq!(args.len(), 3, "NATIVE-SUPER-ARGUMENT-FLOOR");
                assert!(
                    Rc::ptr_eq(&args[1], &sc.options) && Rc::ptr_eq(&args[2], &sc.store),
                    "NATIVE-SUPER-ORIGINAL-ARGS"
                );
                sc.event("super", vec![])?;
                Ok(sc.root.clone())
            }),
            derive: Rc::new(|name, base, length, body| {
                Ok(Rc::new(Native::Class {
                    name: name.into(),
                    length,
                    base,
                    body,
                }))
            }),
            prototype_method: Rc::new(move |_, key, len, body| {
                assert_eq!(key, "getPositionMode");
                assert_eq!(len, 0);
                proto.mode.replace(Some(body));
                Ok(())
            }),
            prototype_getter: Rc::new(move |_, key, body| {
                assert_eq!(key, "suppressCryptocomValidationErrorEmission");
                getter.getter.replace(Some(body));
                Ok(())
            }),
            binding,
        })
    }
}
fn host(spec: &Value) -> Rc<Host> {
    let options = object(vec![("key", s("KEY")), ("secret", s("SECRET"))]);
    if let Some(o) = spec["options"].as_object() {
        for (k, v) in o {
            put(&options, s(k), decode(v));
        }
    }
    let symbol = Rc::new(Native::Symbol("loaded".into()));
    let loaded = keys(vec![(s("old"), b(true)), (symbol, s("retained"))]);
    let store = object(vec![
        ("loaded", loaded),
        ("markets", array(vec![])),
        ("update", Rc::new(Native::Builtin("store:update".into()))),
    ]);
    let root = object(vec![
        ("name", s("Crypto.com")),
        ("isDisposed", b(spec["disposed"] == true)),
        ("isPublicDataOnly", b(spec["publicOnly"] == true)),
        (
            "_suppressCryptocomValidationErrorEmission",
            b(spec["suppress"] == true),
        ),
        (
            "suppressCryptocomValidationErrorEmission",
            b(spec["suppress"] == true),
        ),
        ("store", store.clone()),
    ]);
    for key in [
        "initExternalPublicData",
        "fetchMarkets",
        "fetchTickers",
        "fetchPositions",
        "fetchBalance",
        "fetchOrders",
        "tick",
        "log",
        "emitAndLogError",
        "createXhrWithSecondaryKey",
    ] {
        put(
            &root,
            s(key),
            Rc::new(Native::Builtin(format!("root:{key}"))),
        );
    }
    let request = object(vec![(
        "use",
        Rc::new(Native::Builtin("request:use".into())),
    )]);
    let response = object(vec![(
        "use",
        Rc::new(Native::Builtin("response:use".into())),
    )]);
    let interceptors = object(vec![
        ("request", request.clone()),
        ("response", response.clone()),
    ]);
    let xhr = object(vec![("interceptors", interceptors.clone())]);
    let h = Rc::new(Host {
        spec: spec.clone(),
        tasks: RefCell::new(vec![]),
        tags: RefCell::new(vec![]),
        events: RefCell::new(vec![]),
        timers: RefCell::new(vec![]),
        updates: RefCell::new(vec![]),
        logs: RefCell::new(vec![]),
        api_config: RefCell::new(vec![]),
        api_factory: RefCell::new(None),
        signed: RefCell::new(vec![]),
        defined: RefCell::new(vec![]),
        root,
        store,
        options,
        markets: array(vec![
            object(vec![
                ("id", s("BTC")),
                ("info", object(vec![("underlyingSymbol", s("INDEX"))])),
            ]),
            object(vec![("id", s("ETH")), ("info", Rc::new(Native::Null))]),
        ]),
        tickers: array(vec![object(vec![]), object(vec![]), object(vec![])]),
        positions: if spec["emptyPositions"] == true {
            array(vec![])
        } else {
            array(vec![
                object(vec![("unrealizedPnl", n(3.))]),
                object(vec![("unrealizedPnl", n(4.))]),
            ])
        },
        balance: object(vec![("total", n(8.))]),
        orders: array(vec![object(vec![])]),
        xhr,
        phase: object(vec![("message", s("boom"))]),
        every: array(vec![]),
        periodic: array(vec![]),
        rule: object(vec![]),
        limited: object(vec![]),
        request: RefCell::new(None),
        response: RefCell::new(None),
        rejection: RefCell::new(None),
        retry: RefCell::new(None),
        last_tick: RefCell::new(None),
        target: RefCell::new(None),
        getter: RefCell::new(None),
        mode: RefCell::new(None),
        regexes: RefCell::new(vec![]),
        weak_members: RefCell::new(vec![]),
    });
    for (v, name) in [
        (&h.root, "root"),
        (&h.store, "store"),
        (&h.options, "options"),
        (&h.markets, "markets"),
        (&h.tickers, "tickers"),
        (&h.positions, "positions"),
        (&h.balance, "balance"),
        (&h.orders, "orders"),
        (&h.xhr, "xhr"),
        (&h.phase, "error:phase"),
        (&h.every, "grafana-every"),
        (&h.periodic, "grafana-periodic"),
        (&h.rule, "rule"),
        (&h.limited, "limited"),
        (&request, "request"),
        (&response, "response"),
        (&interceptors, "interceptors"),
    ] {
        h.tag(v.clone(), name);
    }
    for side in ["public", "private"] {
        let socket = object(vec![]);
        for key in [
            "connectAndSubscribe",
            "subscribeToOrderUpdates",
            "subscribeToPositionUpdates",
            "subscribeToBalanceUpdates",
            "subscribeToMarketData",
        ] {
            put(
                &socket,
                s(key),
                Rc::new(Native::Builtin(format!("{side}:{key}"))),
            );
        }
        h.tag(socket.clone(), side);
        put(&h.root, s(&(side.to_owned() + "Websocket")), socket);
    }
    h
}
fn expect(h: &Host, native: &V, expected: &Value, where_: &str) {
    let failure = || format!("CRYPTOCOM-RUNTIME-NATIVE-VALUE {where_}");
    match expected["kind"].as_str().unwrap() {
        "undefined" => assert!(matches!(native.as_ref(), Native::U), "{}", failure()),
        "null" => assert!(matches!(native.as_ref(), Native::Null), "{}", failure()),
        "bool" => {
            let Native::B(v) = native.as_ref() else {
                panic!("{}", failure())
            };
            assert_eq!(*v, expected["value"].as_bool().unwrap(), "{}", failure());
        }
        "number" => {
            let Native::N(v) = native.as_ref() else {
                panic!("{}", failure())
            };
            let bits = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
            let number = f64::from_bits(bits);
            if number.is_nan() {
                assert!(v.is_nan(), "{}", failure());
            } else {
                assert_eq!(v.to_bits(), bits, "{}", failure());
            }
        }
        "string" => {
            let Native::S(v) = native.as_ref() else {
                panic!("{}", failure())
            };
            let units = expected["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u16)
                .collect::<Vec<_>>();
            assert_eq!(v, &units, "{}", failure());
        }
        "symbol" => {
            let Native::Symbol(v) = native.as_ref() else {
                panic!("{}", failure())
            };
            assert_eq!(v, expected["tag"].as_str().unwrap(), "{}", failure());
        }
        "opaque" => assert_eq!(
            h.label(native).as_deref(),
            expected["tag"].as_str(),
            "{}",
            failure()
        ),
        "function" => {
            let Native::F(name, len, _) = native.as_ref() else {
                panic!("{}", failure())
            };
            assert_eq!(name, expected["name"].as_str().unwrap(), "{}", failure());
            assert_eq!(
                *len,
                expected["length"].as_u64().unwrap() as usize,
                "{}",
                failure()
            );
        }
        "array" => {
            let Native::A(values) = native.as_ref() else {
                panic!("{}", failure())
            };
            let values = values.borrow();
            let wanted = expected["values"].as_array().unwrap();
            assert_eq!(
                values.len(),
                wanted.len(),
                "CRYPTOCOM-RUNTIME-NATIVE-ARRAY {where_}"
            );
            for (i, (value, want)) in values.iter().zip(wanted).enumerate() {
                expect(h, value, want, &format!("{where_}[{i}]"));
            }
        }
        "object" => {
            let Native::O(fields) = native.as_ref() else {
                panic!("{}", failure())
            };
            let fields = fields.borrow();
            let wanted = expected["fields"].as_array().unwrap();
            assert_eq!(
                fields.len(),
                wanted.len(),
                "CRYPTOCOM-RUNTIME-NATIVE-FIELDS {where_}"
            );
            for (i, ((key, value), want)) in fields.iter().zip(wanted).enumerate() {
                expect(h, key, &want[0], &format!("{where_}.key[{i}]"));
                expect(h, value, &want[1], &format!("{where_}.value[{i}]"));
            }
        }
        _ => panic!("expected kind"),
    }
}
struct Measured {
    h: Rc<Host>,
    outcome: Result<V, E>,
    immediate: V,
    class_fields: Option<Vec<(String, V)>>,
}
fn execute(spec: &Value) -> Measured {
    let h = host(spec);
    let ops = h.ops();
    let mut immediate = array(vec![]);
    let mut class_fields = None;
    let outcome = (|| match spec["op"].as_str().unwrap() {
        "start" | "tick" => {
            let future = if spec["op"] == "start" {
                port::start(ops.clone(), h.root.clone())
            } else {
                port::tick(
                    ops.clone(),
                    h.root.clone(),
                    spec.get("count").map(decode).unwrap_or_else(u),
                )
            };
            let promise = h.promise(future);
            immediate = array(h.events.borrow().clone());
            let result = run(h.await_value(promise));
            if result.is_ok() && spec["op"] == "tick" {
                let timer = h.timers.borrow().last().cloned();
                if let Some((callback, delay)) = timer {
                    if number(&delay) == 10000. {
                        let returned = h.invoke(callback, u(), vec![])?;
                        assert!(
                            Rc::ptr_eq(&returned, h.last_tick.borrow().as_ref().unwrap()),
                            "NATIVE-TIMER-ORIGINAL-RETURN"
                        );
                        h.event("timer-return-original", vec![b(true)])?;
                    }
                }
            }
            result
        }
        "api" => {
            let xhr = port::create_api(ops.clone(), h.options.clone(), h.root.clone())?;
            assert!(Rc::ptr_eq(&xhr, &h.xhr), "NATIVE-FACTORY-XHR");
            immediate = array(h.events.borrow().clone());
            match spec["mode"].as_str() {
                Some("request") => {
                    let config = object(vec![]);
                    if let Some(o) = spec["config"].as_object() {
                        for key in ["url", "data"] {
                            if let Some(v) = o.get(key) {
                                put(&config, s(key), decode(v));
                            }
                        }
                    }
                    h.tag(config.clone(), "config");
                    let returned = h.invoke(
                        h.request.borrow().as_ref().unwrap().clone(),
                        u(),
                        vec![config.clone()],
                    )?;
                    assert!(
                        matches!(returned.as_ref(), Native::Promise(_)),
                        "NATIVE-ASYNC-REQUEST"
                    );
                    immediate = array(h.events.borrow().clone());
                    let output = run(h.await_value(returned))?;
                    assert!(!Rc::ptr_eq(&output, &config), "NATIVE-REQUEST-FRESH-CONFIG");
                    Ok(output)
                }
                Some("response") => {
                    let response = h.tag(decode(&spec["response"]), "response-input");
                    let returned = h.invoke(
                        h.response.borrow().as_ref().unwrap().clone(),
                        u(),
                        vec![response.clone()],
                    )?;
                    assert!(
                        matches!(returned.as_ref(), Native::Promise(_)),
                        "NATIVE-ASYNC-RESPONSE"
                    );
                    immediate = array(h.events.borrow().clone());
                    let output = run(h.await_value(returned))?;
                    if spec["resend"] == true {
                        assert_eq!(h.label(&output).as_deref(), Some("resent"), "NATIVE-RESEND");
                    } else {
                        assert!(Rc::ptr_eq(&output, &response), "NATIVE-RESPONSE-ORIGINAL");
                    }
                    if spec["repeat"] == true {
                        let again = h.invoke(
                            h.response.borrow().as_ref().unwrap().clone(),
                            u(),
                            vec![response.clone()],
                        )?;
                        let again = run(h.await_value(again))?;
                        assert!(Rc::ptr_eq(&again, &response), "NATIVE-SECOND-PASS");
                        h.event("second-pass", vec![])?;
                    }
                    h.event("response-data", vec![get(&response, "data")])?;
                    Ok(output)
                }
                Some("rejection") => {
                    let error = h.tag(decode(&spec["errorInput"]), "error:input");
                    let returned = h.invoke(
                        h.rejection.borrow().as_ref().unwrap().clone(),
                        u(),
                        vec![error],
                    )?;
                    assert!(
                        matches!(returned.as_ref(), Native::Promise(_)),
                        "NATIVE-ASYNC-REJECTION"
                    );
                    immediate = array(h.events.borrow().clone());
                    run(h.await_value(returned))
                }
                Some("retry") => {
                    let error = h.tag(decode(&spec["errorInput"]), "retry-error");
                    h.invoke(h.retry.borrow().as_ref().unwrap().clone(), u(), vec![error])
                }
                _ => Ok(xhr),
            }
        }
        "class" => {
            let class = port::define_exchange(ops.clone())?;
            let Native::Class {
                name,
                length,
                base,
                body,
            } = class.as_ref()
            else {
                panic!("NATIVE-CLASS")
            };
            assert_eq!(
                name, "CryptocomExchange",
                "CRYPTOCOM-RUNTIME-NATIVE-CLASS-NAME"
            );
            assert_eq!(*length, 3, "NATIVE-CLASS-LENGTH");
            assert!(matches!(base.as_ref(),Native::Builtin(n)if n=="global:BaseExchange"));
            let target = if spec["subclass"] == true {
                Rc::new(Native::Builtin("Child".into()))
            } else {
                class.clone()
            };
            h.target.replace(Some(target.clone()));
            let out = body(
                class.clone(),
                target.clone(),
                vec![
                    s("Crypto.com"),
                    h.options.clone(),
                    h.store.clone(),
                    s("ignored"),
                ],
            )?;
            assert!(Rc::ptr_eq(&out, &h.root), "NATIVE-SUPER-RECEIVER");
            assert!(
                Rc::ptr_eq(&get(&h.root, "xhr"), &h.xhr),
                "NATIVE-CONSTRUCTOR-XHR-ASSIGNMENT"
            );
            assert_eq!(
                h.label(&get(&h.root, "publicWebsocket")).as_deref(),
                Some("public-created"),
                "NATIVE-CONSTRUCTOR-PUBLIC-ASSIGNMENT"
            );
            assert_eq!(
                h.label(&get(&h.root, "privateWebsocket")).as_deref(),
                Some("private-created"),
                "NATIVE-CONSTRUCTOR-PRIVATE-ASSIGNMENT"
            );
            let getter = h.getter.borrow().as_ref().unwrap().clone();
            assert!(equal(&getter(h.root.clone())?, &b(false)));
            let original = object(vec![]);
            let other = object(vec![(
                "_suppressCryptocomValidationErrorEmission",
                original.clone(),
            )]);
            let before = h.events.borrow().len();
            assert!(
                Rc::ptr_eq(&getter(other)?, &original),
                "NATIVE-GETTER-DYNAMIC-RECEIVER"
            );
            h.events.borrow_mut().truncate(before);
            let mode = h.mode.borrow().as_ref().unwrap().clone();
            let promise = mode(h.root.clone(), vec![])?;
            assert!(
                equal(&run(h.await_value(promise))?, &s("hedged")),
                "NATIVE-POSITION-MODE"
            );
            class_fields = Some(h.defined.borrow().clone());
            if spec["twice"] == true {
                let first_start = get(&h.root, "start");
                h.defined.borrow_mut().clear();
                let out2 = body(
                    class.clone(),
                    target,
                    vec![s("Crypto.com"), h.options.clone(), h.store.clone()],
                )?;
                assert!(Rc::ptr_eq(&out2, &h.root));
                assert!(
                    !Rc::ptr_eq(&first_start, &get(&h.root, "start")),
                    "NATIVE-FRESH-ARROW"
                );
                let regexes = h.regexes.borrow();
                assert_eq!(regexes.len(), 2);
                assert!(!Rc::ptr_eq(&regexes[0], &regexes[1]), "NATIVE-FRESH-REGEXP");
                h.event("second-instance", vec![])?;
            }
            Ok(out)
        }
        _ => panic!("unknown fixture"),
    })();
    Measured {
        h,
        outcome,
        immediate,
        class_fields,
    }
}
#[test]
fn runtime_bodies_match_maintained_typescript_native() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!("runtime_fixtures.json")).unwrap();
    assert_eq!(fixtures.len(), 74, "NATIVE-INDEPENDENT-FIXTURE-FLOOR");
    assert_eq!(fixtures[73]["name"], "two-fresh-instances");
    let mut command = Command::new("node");
    command
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/runtime_oracle.cjs"
        ))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "maintained-source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let observed: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        observed["rows"].as_array().unwrap().len(),
        74,
        "SOURCE-INDEPENDENT-FIXTURE-FLOOR"
    );
    let mut native = vec![];
    // All decoded-native assertions for all fixtures precede the effect-trace comparisons.
    for (spec, expected) in fixtures.iter().zip(observed["rows"].as_array().unwrap()) {
        let measured = execute(spec);
        let h = &measured.h;
        let name = spec["name"].as_str().unwrap();
        let expected_error = spec.get("error").and_then(Value::as_str);
        match &measured.outcome {
            Ok(value) => {
                assert!(expected_error.is_none(), "NATIVE-EXPECTED-ERROR {name}");
                if spec["op"] == "start" || spec["op"] == "tick" {
                    assert!(matches!(value.as_ref(), Native::U), "NATIVE-VOID {name}");
                }
                if spec["mode"] == "retry" {
                    assert!(
                        equal(value, &decode(&spec["expectedRetry"])),
                        "NATIVE-RETRY {name}"
                    );
                }
                expect(h, value, &expected["output"], &format!("{name}.output"));
            }
            Err(error) => {
                let marker = expected_error
                    .unwrap_or_else(|| panic!("NATIVE-UNEXPECTED-ERROR {name}: {}", text(error)));
                if marker == "phase" {
                    assert!(Rc::ptr_eq(error, &h.phase), "NATIVE-ORIGINAL-ERROR {name}");
                } else if marker == "constructed" {
                    assert_eq!(h.label(error).as_deref(), Some("error:constructed"));
                    assert_eq!(
                        text(&get(error, "message")),
                        spec["message"].as_str().unwrap(),
                        "NATIVE-CONSTRUCTED-MESSAGE {name}"
                    );
                } else {
                    assert_eq!(
                        h.label(error).as_deref(),
                        Some("error:input"),
                        "NATIVE-ORIGINAL-INPUT-ERROR"
                    );
                }
                expect(h, error, &expected["thrown"], &format!("{name}.error"));
            }
        }
        assert_eq!(
            h.updates.borrow().len(),
            spec["expectedCounts"][0].as_u64().unwrap() as usize,
            "NATIVE-INDEPENDENT-UPDATE-COUNT {name}"
        );
        assert_eq!(
            h.logs.borrow().len(),
            spec["expectedCounts"][1].as_u64().unwrap() as usize,
            "NATIVE-INDEPENDENT-LOG-COUNT {name}"
        );
        assert_eq!(
            h.timers.borrow().len(),
            spec["expectedCounts"][2].as_u64().unwrap() as usize,
            "NATIVE-INDEPENDENT-TIMER-COUNT {name}"
        );
        for (_, delay) in h.timers.borrow().iter() {
            let value = if spec["op"] == "start" { 1000. } else { 10000. };
            assert!(
                equal(delay, &n(value)),
                "CRYPTOCOM-RUNTIME-NATIVE-TIMER {name}"
            );
        }
        for config in h.api_config.borrow().iter() {
            let wanted = spec["options"]["extra"]
                .get("recvWindow")
                .map(decode)
                .filter(truthy)
                .unwrap_or_else(|| n(30000.));
            assert!(
                equal(&get(config, "timeout"), &wanted),
                "CRYPTOCOM-RUNTIME-NATIVE-API-TIMEOUT {name}"
            );
            assert_eq!(
                text(&get(&get(config, "headers"), "Content-Type")),
                "application/json"
            );
        }
        for patch in h.updates.borrow().iter() {
            for key in ["markets", "tickers", "positions", "balance", "orders"] {
                let value = get(patch, key);
                if !matches!(value.as_ref(), Native::U) {
                    let original = match key {
                        "markets" => &h.markets,
                        "tickers" => &h.tickers,
                        "positions" => &h.positions,
                        "balance" => &h.balance,
                        _ => &h.orders,
                    };
                    assert!(
                        Rc::ptr_eq(&value, original),
                        "NATIVE-UPDATE-ORIGINAL-{key} {name}"
                    );
                }
            }
            let flags = get(patch, "loaded");
            let Native::O(fields) = flags.as_ref() else {
                panic!("loaded")
            };
            assert!(
                fields.borrow().iter().any(
                    |(k, v)| matches!(k.as_ref(),Native::Symbol(tag)if tag=="loaded")
                        && equal(v, &s("retained"))
                ),
                "NATIVE-LOADED-SYMBOL {name}"
            );
        }
        expect(
            h,
            &array(h.updates.borrow().clone()),
            &expected["updates"],
            &format!("{name}.updates"),
        );
        expect(
            h,
            &array(h.logs.borrow().clone()),
            &expected["logs"],
            &format!("{name}.logs"),
        );
        expect(
            h,
            &array(h.api_config.borrow().clone()),
            &expected["apiConfig"],
            &format!("{name}.apiConfig"),
        );
        expect(
            h,
            &array(h.signed.borrow().clone()),
            &expected["signed"],
            &format!("{name}.signed"),
        );
        expect(
            h,
            &get(&h.balance, "upnl"),
            &expected["balance"],
            &format!("{name}.balance"),
        );
        if let Some(fields) = &measured.class_fields {
            assert_eq!(fields.len(), 36, "NATIVE-INDEPENDENT-CLASS-FIELD-FLOOR");
            assert_eq!(fields[35].0, "mapCryptocomTimeInForce");
            let wanted = expected["classMeta"]["fields"].as_array().unwrap();
            assert_eq!(wanted.len(), 36);
            for ((key, value), field) in fields.iter().zip(wanted) {
                assert_eq!(key, field[0].as_str().unwrap());
                expect(h, value, &field[1], &format!("{name}.field.{key}"));
                assert_eq!(field[2], true);
                assert_eq!(field[3], true);
                assert_eq!(field[4], true);
            }
            assert_eq!(expected["classMeta"]["getter"], json!([false, true]));
            assert_eq!(
                expected["classMeta"]["mode"],
                json!(["getPositionMode", 0, false, true, true])
            );
        }
        native.push(measured);
    }
    for (measured, expected) in native.iter().zip(observed["rows"].as_array().unwrap()) {
        let name = expected["name"].as_str().unwrap();
        expect(
            &measured.h,
            &measured.immediate,
            &expected["immediate"],
            &format!("{name}.before-return"),
        );
        expect(
            &measured.h,
            &array(measured.h.events.borrow().clone()),
            &expected["events"],
            &format!("{name}.events"),
        );
    }
    println!(
        "RUNTIME native floor74 stops index73 two-fresh-instances; application member bridge tested separately by native_bindings"
    );
}
#[test]
fn constructed_exchange_get_account_calls_concrete_body_with_lexical_receiver() {
    let measured = execute(&json!({"op": "class"}));
    let h = &measured.h;
    let constructed = measured
        .outcome
        .as_ref()
        .unwrap_or_else(|error| panic!("NATIVE-ASSEMBLED-ACCOUNT-CONSTRUCTION {}", text(error)));
    assert!(
        Rc::ptr_eq(constructed, &h.root),
        "NATIVE-ASSEMBLED-ACCOUNT-INSTANCE"
    );
    let bound = get(constructed, "getAccount");
    assert!(
        matches!(bound.as_ref(), Native::F(name, 0, _) if name == "getAccount"),
        "NATIVE-ASSEMBLED-ACCOUNT-DEFINED-FUNCTION"
    );
    let defined = h
        .defined
        .borrow()
        .iter()
        .find(|(name, _)| name == "getAccount")
        .expect("constructor must define getAccount")
        .1
        .clone();
    assert!(
        Rc::ptr_eq(&bound, &defined),
        "NATIVE-ASSEMBLED-ACCOUNT-ORIGINAL-FIELD"
    );

    let uuid = object(vec![("opaqueUuid", b(true))]);
    let expected_uuid = uuid.clone();
    let response = object(vec![(
        "data",
        object(vec![("this_account", object(vec![("uuid", uuid)]))]),
    )]);
    let calls = Rc::new(RefCell::new(Vec::<Vec<V>>::new()));
    let observed = calls.clone();
    let endpoint: Vec<u16> = "/private/get-accounts".encode_utf16().collect();
    let post = Rc::new(Native::F(
        "post".into(),
        2,
        Rc::new(move |args| {
            assert_eq!(args.len(), 2, "NATIVE-ASSEMBLED-ACCOUNT-POST-ARITY");
            assert!(
                matches!(args[0].as_ref(), Native::S(units) if units == &endpoint),
                "NATIVE-ASSEMBLED-ACCOUNT-ENDPOINT"
            );
            let Native::O(fields) = args[1].as_ref() else {
                panic!("NATIVE-ASSEMBLED-ACCOUNT-PARAMS-TYPE")
            };
            assert!(
                fields.borrow().is_empty(),
                "NATIVE-ASSEMBLED-ACCOUNT-EMPTY-PARAMS"
            );
            observed.borrow_mut().push(args);
            Ok(response.clone())
        }),
    ));
    put(&h.xhr, s("post"), post);
    let other_receiver = object(vec![(
        "xhr",
        object(vec![(
            "post",
            Rc::new(Native::F(
                "wrongPost".into(),
                2,
                Rc::new(|_| panic!("NATIVE-ASSEMBLED-ACCOUNT-WRONG-RECEIVER")),
            )),
        )]),
    )]);
    let returned = h
        .invoke(bound, other_receiver, vec![])
        .unwrap_or_else(|error| panic!("NATIVE-ASSEMBLED-ACCOUNT-INVOKE {}", text(&error)));
    assert!(
        matches!(returned.as_ref(), Native::Promise(_)),
        "NATIVE-ASSEMBLED-ACCOUNT-PROMISE"
    );
    assert_eq!(
        calls.borrow().len(),
        1,
        "NATIVE-ASSEMBLED-ACCOUNT-EAGER-POST"
    );
    let output = run(h.await_value(returned))
        .unwrap_or_else(|error| panic!("NATIVE-ASSEMBLED-ACCOUNT-RESULT {}", text(&error)));
    assert!(
        Rc::ptr_eq(&get(&output, "userId"), &expected_uuid),
        "NATIVE-ASSEMBLED-ACCOUNT-UUID-IDENTITY"
    );
    assert!(
        matches!(get(&output, "affiliateId").as_ref(), Native::Null),
        "NATIVE-ASSEMBLED-ACCOUNT-AFFILIATE-NULL"
    );
    let Native::O(fields) = output.as_ref() else {
        panic!("NATIVE-ASSEMBLED-ACCOUNT-RESULT-TYPE")
    };
    assert_eq!(
        fields.borrow().len(),
        2,
        "NATIVE-ASSEMBLED-ACCOUNT-FIELD-COUNT"
    );
    assert_eq!(
        calls.borrow().len(),
        1,
        "NATIVE-ASSEMBLED-ACCOUNT-POST-COUNT"
    );
}

#[test]
fn runtime_promise_ownership_survives_observer_drop() {
    for op in ["start", "tick"] {
        for stage in ["before", "between", "after"] {
            let h = host(&json!({"op":op}));
            let future = if op == "start" {
                port::start(h.ops(), h.root.clone())
            } else {
                port::tick(h.ops(), h.root.clone(), u())
            };
            let observer = h.promise(future);
            if stage == "between" {
                let mut pending = pin!(PromiseWait(observer.clone()));
                let mut suspended = false;
                for _ in 0..20 {
                    let poll = pending.as_mut().poll(&mut context());
                    assert!(poll.is_pending(), "NATIVE-DROP-BETWEEN-AWAITS");
                    if h.updates.borrow().len() == 1 {
                        suspended = true;
                        break;
                    }
                }
                assert!(suspended, "NATIVE-DROP-PARTIAL-UPDATE");
            }
            if stage == "after" {
                assert!(run(Box::pin(PromiseWait(observer.clone()))).is_ok());
            }
            drop(observer);
            let owned = h.tasks.borrow().clone();
            assert!(!owned.is_empty(), "NATIVE-PROMISE-OWNERSHIP");
            for promise in owned {
                assert!(
                    run(Box::pin(PromiseWait(promise))).is_ok(),
                    "NATIVE-DROP-CONTINUATION"
                );
            }
            assert_eq!(
                h.updates.borrow().len(),
                3,
                "NATIVE-DROP-EXACTLY-ONCE-COMPLETION {op} {stage}"
            );
            let flags = get(&h.store, "loaded");
            for key in if op == "start" {
                ["markets", "tickers", "orders"]
            } else {
                ["positions", "balance", "orders"]
            } {
                assert!(
                    equal(&get(&flags, key), &b(true)),
                    "NATIVE-DROP-LOADED-FLAG {key}"
                );
            }
            h.tasks.borrow_mut().clear();
        }
    }
    println!(
        "RUNTIME observer-drop floor6 stops tick after; native promise-provider protocol only"
    );
}
#[test]
fn runtime_oracle_rejects_typescript_mutation() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/runtime_oracle.cjs"
        ))
        .env("SAFE_CEX_CRYPTOCOM_RUNTIME_MUTATION", "timeout")
        .output()
        .unwrap();
    assert!(!output.status.success(), "NATIVE-ORACLE-MUTATION-MUST-FAIL");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("CRYPTOCOM-RUNTIME-SOURCE-API-TIMEOUT"),
        "NATIVE-ORACLE-MUTATION-NAMED-ASSERTION: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
