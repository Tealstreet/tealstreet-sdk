use safe_cex_venue_lighter::{
    public_ingress::IngressOps,
    sdk_account_api::{AccountObject, AccountValue},
    transaction_promise::{NativePromise, PromiseResolver, TransactionRuntime},
    transaction_signing::{
        AuthTokenEncoding, HostThrownValue, SigningResult, TransactionSigningError,
    },
    ws_public_class::*,
    ws_public_cleanup::PublicCleanupHost,
    ws_public_readiness::PublicReadinessHost,
    ws_public_subscriptions::PublicSubscriptionHost,
    ws_public_watchdogs::PublicWatchdogHost,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, VecDeque},
    rc::{Rc, Weak},
};

type NativeFn = Rc<dyn Fn(&Realm, &AccountValue, &[AccountValue]) -> SigningResult<AccountValue>>;
struct Obj {
    props: RefCell<Vec<(Vec<u16>, AccountValue)>>,
    call: Option<NativeFn>,
    array: bool,
    promise: Option<NativePromise<AccountValue>>,
}
impl AccountObject for Obj {
    fn get(&self, key: &str) -> SigningResult<AccountValue> {
        let key = u(key);
        Ok(self
            .props
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or(AccountValue::Undefined))
    }
}
#[derive(Default)]
struct Runtime {
    queue: RefCell<VecDeque<Box<dyn FnOnce()>>>,
}
impl TransactionRuntime for Runtime {
    fn enqueue_microtask(&self, f: Box<dyn FnOnce()>) {
        self.queue.borrow_mut().push_back(f)
    }
    fn now_millis(&self) -> f64 {
        unreachable!()
    }
    fn auth_token_encoding(&self) -> AuthTokenEncoding {
        unreachable!()
    }
}
struct Realm {
    me: RefCell<Weak<Realm>>,
    runtime: Rc<Runtime>,
    objects: RefCell<Vec<Rc<Obj>>>,
    bindings: RefCell<BTreeMap<String, AccountValue>>,
    trace: RefCell<Vec<String>>,
    owner: RefCell<AccountValue>,
    timers: RefCell<Vec<AccountValue>>,
    defined: RefCell<Vec<String>>,
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn s(x: &str) -> AccountValue {
    AccountValue::String(u(x))
}
fn n(x: f64) -> AccountValue {
    AccountValue::Number(x)
}
fn b(x: bool) -> AccountValue {
    AccountValue::Boolean(x)
}
fn fmt_number(x: f64) -> String {
    if x.is_finite() && x.fract() == 0.0 {
        format!("{}", x as i64)
    } else {
        format!("{x}")
    }
}
fn arg(args: &[AccountValue], i: usize) -> AccountValue {
    args.get(i).cloned().unwrap_or(AccountValue::Undefined)
}
impl Realm {
    fn new() -> Rc<Realm> {
        let realm = Rc::new(Realm {
            me: RefCell::new(Weak::new()),
            runtime: Rc::default(),
            objects: RefCell::default(),
            bindings: RefCell::default(),
            trace: RefCell::default(),
            owner: RefCell::new(AccountValue::Undefined),
            timers: RefCell::default(),
            defined: RefCell::default(),
        });
        *realm.me.borrow_mut() = Rc::downgrade(&realm);
        realm.install_builtins();
        realm
    }
    fn rc(&self) -> Rc<Realm> {
        self.me.borrow().upgrade().unwrap()
    }
    fn alloc(&self, obj: Obj) -> AccountValue {
        let obj = Rc::new(obj);
        self.objects.borrow_mut().push(obj.clone());
        AccountValue::Object(obj)
    }
    fn obj(&self, value: &AccountValue) -> Option<Rc<Obj>> {
        let AccountValue::Object(rc) = value else {
            return None;
        };
        let ptr = Rc::as_ptr(rc) as *const u8;
        self.objects
            .borrow()
            .iter()
            .find(|o| Rc::as_ptr(o) as *const u8 == ptr)
            .cloned()
    }
    fn object(&self, fields: &[(&str, AccountValue)]) -> AccountValue {
        self.alloc(Obj {
            props: RefCell::new(fields.iter().map(|(k, v)| (u(k), v.clone())).collect()),
            call: None,
            array: false,
            promise: None,
        })
    }
    fn arr(&self, values: &[AccountValue]) -> AccountValue {
        let mut props: Vec<_> = values
            .iter()
            .enumerate()
            .map(|(i, v)| (u(&i.to_string()), v.clone()))
            .collect();
        props.push((u("length"), n(values.len() as f64)));
        self.alloc(Obj {
            props: RefCell::new(props),
            call: None,
            array: true,
            promise: None,
        })
    }
    fn native(&self, f: NativeFn) -> AccountValue {
        self.alloc(Obj {
            props: RefCell::new(vec![]),
            call: Some(f),
            array: false,
            promise: None,
        })
    }
    /// Traced stub: records `name(args)` and returns `ret(args)`.
    fn stub(
        &self,
        name: &'static str,
        ret: impl Fn(&Realm, &[AccountValue]) -> SigningResult<AccountValue> + 'static,
    ) -> AccountValue {
        self.native(Rc::new(move |realm, _, args| {
            let shown: Vec<_> = args.iter().map(|a| realm.show(a)).collect();
            realm.log_trace(format!("{name}({})", shown.join(",")));
            ret(realm, args)
        }))
    }
    fn noop(&self, name: &'static str) -> AccountValue {
        self.stub(name, |_, _| Ok(AccountValue::Undefined))
    }
    fn log_trace(&self, entry: String) {
        self.trace.borrow_mut().push(entry);
    }
    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.trace.borrow_mut())
    }
    fn bind(&self, name: &str, value: AccountValue) {
        self.bindings.borrow_mut().insert(name.into(), value);
    }
    fn show(&self, value: &AccountValue) -> String {
        match value {
            AccountValue::Undefined => "undefined".into(),
            AccountValue::Null => "null".into(),
            AccountValue::Boolean(v) => v.to_string(),
            AccountValue::Number(v) => fmt_number(*v),
            AccountValue::String(v) => String::from_utf16_lossy(v),
            AccountValue::Object(_) => {
                let o = self.obj(value).unwrap();
                if o.call.is_some() {
                    return "fn".into();
                }
                if o.promise.is_some() {
                    return "promise".into();
                }
                let props = o.props.borrow();
                if o.array {
                    let items: Vec<_> = props
                        .iter()
                        .filter(|(k, _)| *k != u("length"))
                        .map(|(_, v)| self.show(v))
                        .collect();
                    return format!("[{}]", items.join(","));
                }
                let items: Vec<_> = props
                    .iter()
                    .map(|(k, v)| format!("{}:{}", String::from_utf16_lossy(k), self.show(v)))
                    .collect();
                format!("{{{}}}", items.join(","))
            }
            _ => "?".into(),
        }
    }
    fn own_field(&self, key: &str) -> AccountValue {
        self.get(&self.owner(), &s(key)).unwrap()
    }
    fn shown(&self, key: &str) -> String {
        self.show(&self.own_field(key))
    }
    fn drain(&self) {
        loop {
            let task = self.runtime.queue.borrow_mut().pop_front();
            match task {
                Some(task) => task(),
                None => return,
            }
        }
    }
    fn fire(&self, index: usize) {
        let callback = self.timers.borrow()[index].clone();
        self.invoke(&callback, &AccountValue::Undefined, &[])
            .unwrap();
    }
    fn reject(&self, error: AccountValue) -> AccountValue {
        let promise = NativePromise::settled(self.runtime.clone(), Err(self.throw_value(error)));
        self.promise_value(promise)
    }
    fn resolved(&self, value: AccountValue) -> AccountValue {
        self.promise_value(NativePromise::settled(self.runtime.clone(), Ok(value)))
    }
    fn settled(&self, promise: &AccountValue) -> Option<Result<String, String>> {
        let p = self.obj(promise).unwrap().promise.clone().unwrap();
        let out = Rc::new(RefCell::new(None));
        let sink = out.clone();
        let realm = self.rc();
        p.subscribe(move |r| {
            *sink.borrow_mut() = Some(match r {
                Ok(v) => Ok(realm.show(&v)),
                Err(_) => Err("rejected".into()),
            })
        });
        self.drain();
        out.borrow().clone()
    }
    fn install_builtins(&self) {
        let number = self.native(Rc::new(|_, _, args| {
            Ok(n(match arg(args, 0) {
                AccountValue::Number(v) => v,
                AccountValue::String(v) => String::from_utf16_lossy(&v).parse().unwrap_or(f64::NAN),
                AccountValue::Undefined => f64::NAN,
                AccountValue::Null => 0.0,
                AccountValue::Boolean(v) => v as u8 as f64,
                _ => f64::NAN,
            }))
        }));
        let finite = self.native(Rc::new(|_, _, args| {
            Ok(b(
                matches!(arg(args, 0), AccountValue::Number(v) if v.is_finite()),
            ))
        }));
        let parse_int = self.native(Rc::new(|_, _, args| {
            let AccountValue::String(v) = arg(args, 0) else {
                unreachable!()
            };
            Ok(n(String::from_utf16_lossy(&v).parse().unwrap()))
        }));
        self.set(&number, &s("isFinite"), finite).unwrap();
        self.set(&number, &s("parseInt"), parse_int).unwrap();
        self.bind("Number", number);
        let pick = |f: fn(f64, f64) -> f64| -> NativeFn {
            Rc::new(move |_, _, args| {
                let values: Vec<f64> = args
                    .iter()
                    .map(|a| match a {
                        AccountValue::Number(v) => *v,
                        _ => unreachable!(),
                    })
                    .collect();
                Ok(n(values[1..].iter().fold(values[0], |a, b| f(a, *b))))
            })
        };
        let math = self.object(&[
            ("max", self.native(pick(f64::max))),
            ("min", self.native(pick(f64::min))),
            (
                "round",
                self.native(Rc::new(|_, _, args| {
                    let AccountValue::Number(v) = arg(args, 0) else {
                        unreachable!()
                    };
                    Ok(n(v.round()))
                })),
            ),
        ]);
        self.bind("Math", math);
        let clock = self.object(&[(
            "getCurrentTimeMs",
            self.native(Rc::new(|_, _, _| Ok(n(5000.0)))),
        )]);
        self.bind("virtualClock", clock);
        self.bind(
            "Date",
            self.object(&[("now", self.native(Rc::new(|_, _, _| Ok(n(42.0)))))]),
        );
        self.bind(
            "String",
            self.native(Rc::new(|realm, _, args| Ok(s(&realm.show(&arg(args, 0)))))),
        );
        self.bind(
            "parseFloat",
            self.native(Rc::new(|_, _, args| {
                Ok(n(match arg(args, 0) {
                    AccountValue::String(v) => {
                        String::from_utf16_lossy(&v).parse().unwrap_or(f64::NAN)
                    }
                    AccountValue::Number(v) => v,
                    _ => f64::NAN,
                }))
            })),
        );
        self.bind(
            "Array",
            self.object(&[(
                "isArray",
                self.native(Rc::new(|realm, _, args| {
                    Ok(b(realm.obj(&arg(args, 0)).is_some_and(|o| o.array)))
                })),
            )]),
        );
        self.bind(
            "setTimeout",
            self.native(Rc::new(|realm, _, args| {
                realm.log_trace(format!("setTimeout({})", realm.show(&arg(args, 1))));
                realm.timers.borrow_mut().push(arg(args, 0));
                Ok(n(900.0 + realm.timers.borrow().len() as f64))
            })),
        );
        self.bind("clearTimeout", self.noop("clearTimeout"));
        self.bind(
            "Error",
            self.native(Rc::new(|realm, _, args| {
                Ok(realm.object(&[("message", arg(args, 0))]))
            })),
        );
        self.bind(
            "add",
            self.native(Rc::new(|_, _, args| {
                let (AccountValue::Number(a), AccountValue::Number(b)) =
                    (arg(args, 0), arg(args, 1))
                else {
                    unreachable!()
                };
                Ok(n(a + b))
            })),
        );
        self.bind("calcOrderBookTotal", self.noop("calcOrderBookTotal"));
    }
}
impl PublicSocketHost for Realm {
    fn owner(&self) -> AccountValue {
        self.owner.borrow().clone()
    }
    fn get(&self, object: &AccountValue, key: &AccountValue) -> SigningResult<AccountValue> {
        let key_text = self.show(key);
        match object {
            AccountValue::String(v) => match key_text.as_str() {
                "length" => Ok(n(v.len() as f64)),
                "includes" => Ok(self.native(Rc::new(|realm, this, args| {
                    let hay = realm.show(this);
                    Ok(b(hay.contains(&realm.show(&arg(args, 0)))))
                }))),
                _ => Ok(AccountValue::Undefined),
            },
            AccountValue::Object(_) => {
                let o = self.obj(object).unwrap();
                let found = o
                    .props
                    .borrow()
                    .iter()
                    .find(|(k, _)| *k == u(&key_text))
                    .map(|(_, v)| v.clone());
                if let Some(v) = found {
                    return Ok(v);
                }
                Ok(match key_text.as_str() {
                    "filter" if o.array => self.native(Rc::new(|realm, this, args| {
                        let mut kept = vec![];
                        realm.for_of(this, &mut |v| {
                            if realm
                                .invoke(&arg(args, 0), &AccountValue::Undefined, &[v.clone()])?
                                .truthy()
                            {
                                kept.push(v);
                            }
                            Ok(())
                        })?;
                        Ok(realm.arr(&kept))
                    })),
                    "every" if o.array => self.native(Rc::new(|realm, this, args| {
                        let mut all = true;
                        realm.for_of(this, &mut |v| {
                            all &= realm
                                .invoke(&arg(args, 0), &AccountValue::Undefined, &[v])?
                                .truthy();
                            Ok(())
                        })?;
                        Ok(b(all))
                    })),
                    "finally" if o.promise.is_some() => {
                        let original = o.promise.clone().unwrap();
                        self.native(Rc::new(move |realm, _, args| {
                            let (p, r) = NativePromise::pending(realm.runtime.clone());
                            let cb = arg(args, 0);
                            let owner = realm.rc();
                            original.subscribe(move |res| {
                                owner.invoke(&cb, &AccountValue::Undefined, &[]).unwrap();
                                r.settle_shared(res);
                            });
                            Ok(realm.promise_value(p))
                        }))
                    }
                    _ => AccountValue::Undefined,
                })
            }
            _ => Err(self.throw_value(s("TypeError"))),
        }
    }
    fn set(
        &self,
        object: &AccountValue,
        key: &AccountValue,
        value: AccountValue,
    ) -> SigningResult<()> {
        let key = u(&self.show(key));
        let o = self.obj(object).unwrap();
        let mut props = o.props.borrow_mut();
        match props.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => props.push((key, value)),
        }
        Ok(())
    }
    fn delete(&self, object: &AccountValue, key: &AccountValue) -> SigningResult<()> {
        let key = u(&self.show(key));
        self.obj(object)
            .unwrap()
            .props
            .borrow_mut()
            .retain(|(k, _)| *k != key);
        Ok(())
    }
    fn binding(&self, name: &str) -> SigningResult<AccountValue> {
        Ok(self
            .bindings
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or_else(|| panic!("binding {name}")))
    }
    fn invoke(
        &self,
        callee: &AccountValue,
        this: &AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        let f = self.obj(callee).and_then(|o| o.call.clone());
        match f {
            Some(f) => f(self, this, args),
            None => Err(self.throw_value(s("not callable"))),
        }
    }
    fn construct(
        &self,
        callee: &AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        self.invoke(callee, &AccountValue::Undefined, args)
    }
    fn function(&self, callback: SocketCallback) -> SigningResult<AccountValue> {
        Ok(self.native(Rc::new(move |_, _, args| callback(args))))
    }
    fn literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(self.object(fields))
    }
    fn array(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(self.arr(values))
    }
    fn copy_object(&self, source: &AccountValue) -> SigningResult<AccountValue> {
        let props = self.obj(source).unwrap().props.borrow().clone();
        Ok(self.alloc(Obj {
            props: RefCell::new(props),
            call: None,
            array: false,
            promise: None,
        }))
    }
    fn define(&self, object: &AccountValue, key: &str, value: AccountValue) -> SigningResult<()> {
        self.set(object, &s(key), value)
    }
    fn for_of(
        &self,
        value: &AccountValue,
        visit: &mut dyn FnMut(AccountValue) -> SigningResult<()>,
    ) -> SigningResult<()> {
        let length = match self.get(value, &s("length"))? {
            AccountValue::Number(v) => v as usize,
            _ => unreachable!("arrays only"),
        };
        for i in 0..length {
            visit(self.get(value, &n(i as f64))?)?;
        }
        Ok(())
    }
    fn template(&self, value: &AccountValue) -> SigningResult<Vec<u16>> {
        Ok(u(&self.show(value)))
    }
    fn relational(
        &self,
        op: Relational,
        left: &AccountValue,
        right: &AccountValue,
    ) -> SigningResult<bool> {
        let (AccountValue::Number(l), AccountValue::Number(r)) = (left, right) else {
            unreachable!()
        };
        Ok(match op {
            Relational::Less => l < r,
            Relational::Greater => l > r,
            Relational::LessEqual => l <= r,
        })
    }
    fn error_value(&self, error: TransactionSigningError) -> AccountValue {
        let TransactionSigningError::Thrown(value) = error else {
            unreachable!()
        };
        value
            .value()
            .downcast_ref::<AccountValue>()
            .unwrap()
            .clone()
    }
    fn throw_value(&self, value: AccountValue) -> TransactionSigningError {
        TransactionSigningError::Thrown(HostThrownValue::new(value))
    }
    fn runtime(&self) -> Rc<dyn TransactionRuntime> {
        self.runtime.clone()
    }
    fn await_value(&self, value: AccountValue) -> SigningResult<NativePromise<AccountValue>> {
        Ok(match self.obj(&value).and_then(|o| o.promise.clone()) {
            Some(p) => p,
            None => NativePromise::settled(self.runtime.clone(), Ok(value)),
        })
    }
    fn resolve_return(&self, value: AccountValue, resolver: PromiseResolver<AccountValue>) {
        match self.obj(&value).and_then(|o| o.promise.clone()) {
            Some(p) => p.subscribe(move |r| resolver.settle_shared(r)),
            None => resolver.settle(Ok(value)),
        }
    }
    fn promise_value(&self, promise: NativePromise<AccountValue>) -> AccountValue {
        self.alloc(Obj {
            props: RefCell::new(vec![]),
            call: None,
            array: false,
            promise: Some(promise),
        })
    }
}
fn host(realm: &Rc<Realm>) -> Rc<dyn PublicSocketHost> {
    realm.clone()
}
fn with_owner(realm: &Rc<Realm>, fields: &[(&str, AccountValue)]) {
    let owner = realm.object(fields);
    *realm.owner.borrow_mut() = owner;
}
fn parent(realm: &Rc<Realm>, extra: &[(&str, AccountValue)]) -> AccountValue {
    let mut fields = vec![("log", realm.noop("log")), ("isDisposed", b(false))];
    fields.extend(extra.iter().cloned());
    realm.object(&fields)
}

#[test]
fn on_close_direct_route_failure_promotes_readonly_and_schedules_reconnect() {
    let realm = Realm::new();
    realm.bind(
        "classifyLighterPublicClose",
        realm.stub("classifyLighterPublicClose", |r, _| {
            Ok(r.object(&[
                ("isRateLimited", b(false)),
                ("isPolicyRestricted", b(false)),
            ]))
        }),
    );
    realm.bind(
        "calculateLighterPublicReconnectDelay",
        realm.stub("calculateLighterPublicReconnectDelay", |_, _| Ok(n(2500.4))),
    );
    realm.bind("WS_MIN_RECONNECTION_DELAY", n(1000.0));
    let ws = realm.object(&[("removeEventListener", realm.noop("removeEventListener"))]);
    let parent = parent(
        &realm,
        &[(
            "syncSocketHealthToStore",
            realm.noop("syncSocketHealthToStore"),
        )],
    );
    with_owner(
        &realm,
        &[
            ("parent", parent),
            ("connectStartedAt", n(1000.0)),
            ("openedSinceLastConnect", b(true)),
            ("hasReceivedMessageSinceConnect", b(false)),
            ("failedConnections", n(1.0)),
            ("connectedViaProxyRoute", b(false)),
            ("useReadonlyDirectRoute", b(false)),
            ("lastReceived", n(5.0)),
            ("lastReceivedTimer", n(77.0)),
            ("reconnecting", b(false)),
            ("pingTimeoutId", AccountValue::Undefined),
            ("pongMissTimeoutId", n(88.0)),
            ("ws", ws),
            ("onOpen", s("onOpenFn")),
            ("onMessage", s("onMessageFn")),
            ("onClose", s("onCloseFn")),
            ("onCloseCount", n(3.0)),
            (
                "recordCloseDiagnostics",
                realm.noop("recordCloseDiagnostics"),
            ),
            (
                "isWsProxyRouteEnabled",
                realm.stub("isWsProxyRouteEnabled", |_, _| Ok(b(false))),
            ),
            (
                "clearPendingSubscribeTimeouts",
                realm.noop("clearPendingSubscribeTimeouts"),
            ),
            (
                "clearAllOrderBookStaleTimeouts",
                realm.noop("clearAllOrderBookStaleTimeouts"),
            ),
            (
                "clearAllOrderBookInitialDataTimeouts",
                realm.noop("clearAllOrderBookInitialDataTimeouts"),
            ),
            (
                "startTickerRestFallback",
                realm.noop("startTickerRestFallback"),
            ),
            ("connectAndSubscribe", realm.noop("connectAndSubscribe")),
        ],
    );
    let event = realm.object(&[("code", n(4000.0)), ("reason", s("bye"))]);
    on_close(host(&realm), &event).unwrap();
    assert_eq!(
        realm.take(),
        [
            "recordCloseDiagnostics(4000,{syntheticIfNoSocket:true})",
            "classifyLighterPublicClose(4000,bye)",
            "isWsProxyRouteEnabled()",
            "log([Lighter] Public WS switching to readonly direct route after 2 direct-route failures.,warning)",
            "isWsProxyRouteEnabled()",
            "clearPendingSubscribeTimeouts()",
            "clearAllOrderBookStaleTimeouts()",
            "clearAllOrderBookInitialDataTimeouts()",
            "startTickerRestFallback(public websocket closed; polling tickers via REST)",
            "clearTimeout(77)",
            "syncSocketHealthToStore()",
            "log([Lighter] Public WS closed (code 4000, reason: bye),warning)",
            "clearTimeout(88)",
            "removeEventListener(open,onOpenFn)",
            "removeEventListener(message,onMessageFn)",
            "removeEventListener(close,onCloseFn)",
            "calculateLighterPublicReconnectDelay({baseDelayMs:1000,failureCount:2,isRateLimited:false,rateLimitReconnectDelayMs:30000})",
            "log([Lighter] Public WS reconnecting in 2500ms...,info)",
            "setTimeout(2500.4)",
        ]
    );
    for (key, value) in [
        ("failedConnections", "2"),
        ("useReadonlyDirectRoute", "true"),
        ("openedSinceLastConnect", "false"),
        ("lastReceived", "0"),
        ("lastReceivedTimer", "undefined"),
        ("pongMissTimeoutId", "undefined"),
        ("ws", "undefined"),
        ("onCloseCount", "4"),
        ("reconnecting", "true"),
    ] {
        assert_eq!(realm.shown(key), value, "{key}");
    }
    realm.fire(0);
    assert_eq!(realm.take(), ["connectAndSubscribe(true)"]);
    assert_eq!(realm.shown("reconnecting"), "false");
    assert_eq!(realm.shown("lastCloseTime"), "42");
}

#[test]
fn on_close_proxy_failure_probes_direct_and_stops_while_reconnecting() {
    let realm = Realm::new();
    realm.bind(
        "classifyLighterPublicClose",
        realm.stub("classifyLighterPublicClose", |r, _| {
            Ok(r.object(&[
                ("isRateLimited", b(false)),
                ("isPolicyRestricted", b(false)),
            ]))
        }),
    );
    let parent = parent(&realm, &[]);
    with_owner(
        &realm,
        &[
            ("parent", parent),
            ("connectStartedAt", n(0.0)),
            ("openedSinceLastConnect", b(true)),
            ("hasReceivedMessageSinceConnect", b(true)),
            ("failedConnections", n(0.0)),
            ("connectedViaProxyRoute", b(true)),
            ("useDirectFallbackRoute", b(false)),
            ("proxyRouteFailureCount", n(2.0)),
            ("directRouteFailureCount", n(4.0)),
            ("directFallbackProbePausedUntil", n(0.0)),
            ("reconnecting", b(true)),
            (
                "recordCloseDiagnostics",
                realm.noop("recordCloseDiagnostics"),
            ),
            (
                "isWsProxyRouteEnabled",
                realm.stub("isWsProxyRouteEnabled", |_, _| Ok(b(true))),
            ),
            (
                "clearPendingSubscribeTimeouts",
                realm.noop("clearPendingSubscribeTimeouts"),
            ),
            (
                "clearAllOrderBookStaleTimeouts",
                realm.noop("clearAllOrderBookStaleTimeouts"),
            ),
            (
                "clearAllOrderBookInitialDataTimeouts",
                realm.noop("clearAllOrderBookInitialDataTimeouts"),
            ),
            (
                "startTickerRestFallback",
                realm.noop("startTickerRestFallback"),
            ),
        ],
    );
    let event = realm.object(&[("code", n(1006.0))]);
    on_close(host(&realm), &event).unwrap();
    assert_eq!(
        realm.take(),
        [
            "recordCloseDiagnostics(1006,{syntheticIfNoSocket:true})",
            "classifyLighterPublicClose(1006,)",
            "isWsProxyRouteEnabled()",
            "isWsProxyRouteEnabled()",
            "log([Lighter] Public WS proxy route failed 3 times; probing direct route as a fallback.,warning)",
            "clearPendingSubscribeTimeouts()",
            "clearAllOrderBookStaleTimeouts()",
            "clearAllOrderBookInitialDataTimeouts()",
            "startTickerRestFallback(public websocket closed; polling tickers via REST)",
            "log([Lighter] Public WS closed (code 1006),warning)",
        ]
    );
    assert_eq!(realm.shown("failedConnections"), "1");
    assert_eq!(realm.shown("proxyRouteFailureCount"), "3");
    assert_eq!(realm.shown("directRouteFailureCount"), "0");
    assert_eq!(realm.shown("useDirectFallbackRoute"), "true");
    assert!(realm.timers.borrow().is_empty());
}

fn rest_owner(realm: &Rc<Realm>, response: AccountValue) {
    let levels = realm.object(&[
        ("bids", realm.arr(&[n(1.0)])),
        ("asks", realm.arr(&[n(2.0)])),
    ]);
    let api = realm.object(&[(
        "getOrderBookOrders",
        realm.stub("getOrderBookOrders", move |_, _| Ok(response.clone())),
    )]);
    let parent = parent(realm, &[("orderApi", api)]);
    with_owner(
        realm,
        &[
            ("parent", parent),
            ("orderBookRestFallbackInFlight", realm.object(&[])),
            (
                "orderBookRestFallbackCooldownUntilByTopic",
                realm.object(&[]),
            ),
            (
                "isOrderBookRestFallbackCoolingDown",
                realm.stub("isOrderBookRestFallbackCoolingDown", |_, _| Ok(b(false))),
            ),
            (
                "parseRestOrderBookLevels",
                realm.stub("parseRestOrderBookLevels", move |_, _| Ok(levels.clone())),
            ),
            ("applyOrderBookLevels", realm.noop("applyOrderBookLevels")),
            (
                "stopOrderBookRestFallback",
                realm.noop("stopOrderBookRestFallback"),
            ),
        ],
    );
}
fn in_flight(realm: &Rc<Realm>) -> String {
    let record = realm.own_field("orderBookRestFallbackInFlight");
    realm.show(&realm.get(&record, &s("order_book:7")).unwrap())
}

#[test]
fn fetch_and_emit_order_book_from_rest_applies_snapshot_and_clears_in_flight() {
    let realm = Realm::new();
    let payload = realm.object(&[("ok", b(true))]);
    let response = realm.resolved(payload);
    rest_owner(&realm, response);
    let book = realm.object(&[("book", b(true))]);
    let callback = realm.noop("callback");
    let promise = fetch_and_emit_order_book_from_rest(
        host(&realm),
        s("order_book:7"),
        s("7"),
        book,
        callback,
    );
    assert_eq!(in_flight(&realm), "true");
    assert_eq!(
        realm.take(),
        [
            "isOrderBookRestFallbackCoolingDown(order_book:7)",
            "getOrderBookOrders(7,100,{__suppressExchangeError:true})",
        ]
    );
    let promise = realm.promise_value(promise);
    assert_eq!(realm.settled(&promise), Some(Ok("undefined".into())));
    assert_eq!(
        realm.take(),
        [
            "parseRestOrderBookLevels({ok:true})",
            "applyOrderBookLevels({book:true},bids,[1],snapshot)",
            "applyOrderBookLevels({book:true},asks,[2],snapshot)",
            "calcOrderBookTotal({book:true})",
            "callback({book:true})",
        ]
    );
    assert_eq!(in_flight(&realm), "false");
}

#[test]
fn fetch_and_emit_order_book_from_rest_handles_403_and_failures() {
    let realm = Realm::new();
    let error = realm.object(&[("response", realm.object(&[("status", n(403.0))]))]);
    let response = realm.reject(error);
    rest_owner(&realm, response);
    let promise = fetch_and_emit_order_book_from_rest(
        host(&realm),
        s("order_book:7"),
        s("7"),
        n(0.0),
        n(0.0),
    );
    let promise = realm.promise_value(promise);
    assert_eq!(realm.settled(&promise), Some(Ok("undefined".into())));
    assert_eq!(
        realm.take()[2..],
        [
            "log([Lighter] REST orderbook fallback returned 403 for market 7; pausing fallback requests.,warning)",
            "stopOrderBookRestFallback(order_book:7)",
        ]
    );
    let cooldown = realm.own_field("orderBookRestFallbackCooldownUntilByTopic");
    assert_eq!(realm.show(&cooldown), "{order_book:7:65000}");
    assert_eq!(in_flight(&realm), "false");

    let realm = Realm::new();
    let error = realm.object(&[("message", s("boom"))]);
    let response = realm.reject(error);
    rest_owner(&realm, response);
    let promise = fetch_and_emit_order_book_from_rest(
        host(&realm),
        s("order_book:7"),
        s("7"),
        n(0.0),
        n(0.0),
    );
    let promise = realm.promise_value(promise);
    assert_eq!(realm.settled(&promise), Some(Ok("undefined".into())));
    assert_eq!(
        realm.take()[2..],
        ["log([Lighter] REST orderbook fallback failed for market 7: boom,warning)"]
    );
    assert_eq!(in_flight(&realm), "false");
}

fn ticker_owner(realm: &Rc<Realm>, response: AccountValue) {
    realm.bind(
        "ENDPOINTS",
        realm.object(&[("EXCHANGE_STATS", s("/api/v1/exchangeStats"))]),
    );
    let client = realm.object(&[("get", realm.stub("get", move |_, _| Ok(response.clone())))]);
    let items = realm.arr(&[
        realm.object(&[("market_id", n(1.0))]),
        AccountValue::Null,
        realm.object(&[("marketId", AccountValue::Undefined)]),
    ]);
    let parent = parent(realm, &[("lighterApiClient", client)]);
    with_owner(
        realm,
        &[
            ("parent", parent),
            ("tickerRestFallbackInFlight", AccountValue::Undefined),
            ("tickerRestFallbackCooldownUntil", n(0.0)),
            (
                "isTickerRestFallbackCoolingDown",
                realm.stub("isTickerRestFallbackCoolingDown", |_, _| Ok(b(false))),
            ),
            (
                "handleMarketStatsEvent",
                realm.noop("handleMarketStatsEvent"),
            ),
            (
                "parseRestTickerItems",
                realm.stub("parseRestTickerItems", move |_, _| Ok(items.clone())),
            ),
            ("handleTickerEvent", realm.noop("handleTickerEvent")),
            (
                "syncTickerRestFallbackToPriceHydration",
                realm.noop("syncTickerRestFallbackToPriceHydration"),
            ),
            (
                "clearTickerRestFallbackWatchdog",
                realm.noop("clearTickerRestFallbackWatchdog"),
            ),
            (
                "stopTickerRestFallback",
                realm.noop("stopTickerRestFallback"),
            ),
        ],
    );
}

#[test]
fn fetch_and_apply_ticker_fallback_coalesces_and_applies_items() {
    let realm = Realm::new();
    let payload = realm.object(&[("market_stats", realm.object(&[]))]);
    let response = realm.resolved(payload);
    ticker_owner(&realm, response);
    let first = fetch_and_apply_ticker_fallback(host(&realm));
    assert_eq!(realm.shown("tickerRestFallbackInFlight"), "promise");
    let second = fetch_and_apply_ticker_fallback(host(&realm));
    assert_eq!(
        realm.take(),
        [
            "isTickerRestFallbackCoolingDown()",
            "get(/api/v1/exchangeStats,{__suppressExchangeError:true})",
            "isTickerRestFallbackCoolingDown()",
        ]
    );
    let first = realm.promise_value(first);
    let second = realm.promise_value(second);
    assert_eq!(realm.settled(&first), Some(Ok("undefined".into())));
    assert_eq!(realm.settled(&second), Some(Ok("undefined".into())));
    assert_eq!(
        realm.take(),
        [
            "handleMarketStatsEvent({market_stats:{}})",
            "parseRestTickerItems({market_stats:{}})",
            "handleTickerEvent({data:{market_id:1},channel:ticker/1})",
            "handleTickerEvent({data:{marketId:undefined}})",
            "syncTickerRestFallbackToPriceHydration()",
        ]
    );
    assert_eq!(realm.shown("tickerRestFallbackInFlight"), "undefined");
}

#[test]
fn fetch_and_apply_ticker_fallback_pauses_on_403() {
    let realm = Realm::new();
    let error = realm.object(&[("status", n(403.0))]);
    let response = realm.reject(error);
    ticker_owner(&realm, response);
    let promise = fetch_and_apply_ticker_fallback(host(&realm));
    let promise = realm.promise_value(promise);
    assert_eq!(realm.settled(&promise), Some(Ok("undefined".into())));
    assert_eq!(
        realm.take()[2..],
        [
            "log([Lighter] REST ticker fallback returned 403; pausing ticker REST fallback to avoid noisy retry loop.,warning)",
            "clearTickerRestFallbackWatchdog()",
            "stopTickerRestFallback(false)",
        ]
    );
    assert_eq!(realm.shown("tickerRestFallbackCooldownUntil"), "65000");
    assert_eq!(realm.shown("tickerRestFallbackInFlight"), "undefined");
}

fn recovery_owner(
    realm: &Rc<Realm>,
    last: &[(&str, AccountValue)],
    attempts: &[(&str, AccountValue)],
    send_fails: bool,
) {
    let handlers = realm.object(&[("order_book:1", realm.arr(&[n(1.0)]))]);
    let parent = parent(realm, &[]);
    let send = realm.stub("sendPayload", move |r, _| {
        if send_fails {
            Err(r.throw_value(r.object(&[("message", s("closed"))])))
        } else {
            Ok(AccountValue::Undefined)
        }
    });
    with_owner(
        realm,
        &[
            ("parent", parent),
            ("isConnected", b(true)),
            ("messageHandlers", handlers),
            ("lastOrderBookRecoveryAtByTopic", realm.object(last)),
            ("orderBookRecoveryAttempts", realm.object(attempts)),
            (
                "armOrderBookStaleTimeout",
                realm.noop("armOrderBookStaleTimeout"),
            ),
            ("reconnect", realm.noop("reconnect")),
            ("sendPayload", send),
        ],
    );
}

#[test]
fn attempt_order_book_recovery_cooldown_resubscribe_and_reconnect() {
    let (topic, channel, reason) = (s("order_book:1"), s("order_book/1"), s("stale"));
    let realm = Realm::new();
    recovery_owner(&realm, &[("order_book:1", n(4000.0))], &[], false);
    attempt_order_book_recovery(realm.as_ref(), &topic, &channel, &reason).unwrap();
    assert_eq!(
        realm.take(),
        ["armOrderBookStaleTimeout(order_book:1,order_book/1)"]
    );

    let realm = Realm::new();
    recovery_owner(&realm, &[("order_book:1", n(-100_000.0))], &[], false);
    attempt_order_book_recovery(realm.as_ref(), &topic, &channel, &reason).unwrap();
    assert_eq!(
        realm.take(),
        [
            "log([Lighter] Public WS order book recovery re-subscribing order_book:1 (stale, attempt 1).,warning)",
            "sendPayload({type:unsubscribe,channel:order_book/1})",
            "sendPayload({type:subscribe,channel:order_book/1})",
            "armOrderBookStaleTimeout(order_book:1,order_book/1)",
        ]
    );
    assert_eq!(
        realm.shown("lastOrderBookRecoveryAtByTopic"),
        "{order_book:1:5000}"
    );
    assert_eq!(realm.shown("orderBookRecoveryAttempts"), "{order_book:1:1}");

    let realm = Realm::new();
    recovery_owner(
        &realm,
        &[("order_book:1", n(-100_000.0))],
        &[("order_book:1", n(1.0))],
        false,
    );
    attempt_order_book_recovery(realm.as_ref(), &topic, &channel, &reason).unwrap();
    assert_eq!(
        realm.take(),
        [
            "log([Lighter] Public WS order book recovery reconnecting for order_book:1 (stale, attempt 2).,warning)",
            "reconnect()",
        ]
    );

    let realm = Realm::new();
    recovery_owner(&realm, &[("order_book:1", n(-100_000.0))], &[], true);
    attempt_order_book_recovery(realm.as_ref(), &topic, &channel, &reason).unwrap();
    assert_eq!(
        realm.take()[1..],
        [
            "sendPayload({type:unsubscribe,channel:order_book/1})",
            "log([Lighter] Public WS order book re-subscribe failed for order_book:1: closed,warning)",
            "reconnect()",
        ]
    );
}

fn listener_owner(realm: &Rc<Realm>, connected: bool) {
    let markets = realm.object(&[
        ("BTC", realm.object(&[("id", n(1.0))])),
        ("ETH", realm.object(&[("id", n(2.0))])),
    ]);
    let store = realm.object(&[("marketsBySymbol", markets)]);
    let options = realm.object(&[("publicDataOnly", b(false))]);
    let parent = parent(realm, &[("store", store), ("options", options)]);
    realm.bind(
        "buildLighterCandleTopic",
        realm.stub("buildLighterCandleTopic", |r, args| {
            Ok(s(&format!(
                "candle/{}/{}",
                r.show(&args[0]),
                r.show(&args[1])
            )))
        }),
    );
    realm.bind(
        "resolveLighterInterval",
        realm.native(Rc::new(|_, _, _| Ok(s("1m")))),
    );
    realm.bind(
        "buildLighterTradesTopic",
        realm.native(Rc::new(|r, _, args| {
            Ok(s(&format!("trade/{}", r.show(&args[0]))))
        })),
    );
    realm.bind(
        "getCandleStartTime",
        realm.stub("getCandleStartTime", |_, _| Ok(n(1_700_000_000.0))),
    );
    let mut fields = vec![
        ("parent", parent),
        ("isConnected", b(connected)),
        ("isDisposed", b(false)),
    ];
    for name in [
        "sendPayload",
        "armOhlcvNoDataTimeout",
        "clearOhlcvNoDataTimeout",
        "cleanupMessageHandlerStats",
        "fetchAndEmitOrderBookFromRest",
        "armOrderBookStaleTimeout",
        "clearOrderBookInitialDataTimeout",
        "startOrderBookRestFallback",
        "applyOrderBookLevels",
        "stopOrderBookRestFallback",
        "clearOrderBookStaleTimeout",
    ] {
        let name: &'static str = name;
        fields.push((name, realm.noop(name)));
    }
    fields.extend([
        ("messageHandlers", realm.object(&[])),
        ("topics", realm.object(&[])),
        ("orderBookInitialDataTimeouts", realm.object(&[])),
        (
            "orderBookRestFallbackCooldownUntilByTopic",
            realm.object(&[("order_book:2", n(1.0))]),
        ),
        ("orderBookRecoveryAttempts", realm.object(&[])),
        ("lastOrderBookRecoveryAtByTopic", realm.object(&[])),
    ]);
    with_owner(realm, &fields);
}
fn handler(realm: &Rc<Realm>, topic: &str) -> AccountValue {
    let handlers = realm.own_field("messageHandlers");
    let list = realm.get(&handlers, &s(topic)).unwrap();
    realm.get(&list, &n(0.0)).unwrap()
}

#[test]
fn listen_ohlcv_subscribes_parses_candles_and_trades_then_unsubscribes() {
    let realm = Realm::new();
    listener_owner(&realm, true);
    let opts = realm.object(&[("symbol", s("BTC")), ("interval", s("1m"))]);
    let callback = realm.noop("callback");
    let stop = listen_ohlcv(host(&realm), opts, callback).unwrap();
    assert_eq!(
        realm.take(),
        [
            "buildLighterCandleTopic(1,1m)",
            "sendPayload({type:subscribe,channel:candle/1/1m})",
            "armOhlcvNoDataTimeout(candle/1/1m)",
            "sendPayload({type:subscribe,channel:trade/1})",
        ]
    );
    assert_eq!(
        realm.shown("topics"),
        "{candle/1/1m:candle/1/1m,trade/1:trade/1}"
    );
    let candle = realm.object(&[
        ("t", n(1_700_000_000_000.0)),
        ("o", s("1")),
        ("h", s("2")),
        ("l", s("0.5")),
        ("c", s("1.5")),
        ("v", s("10")),
    ]);
    let message = realm.object(&[("data", realm.object(&[("candles", realm.arr(&[candle]))]))]);
    realm
        .invoke(
            &handler(&realm, "candle/1/1m"),
            &AccountValue::Undefined,
            &[message],
        )
        .unwrap();
    assert_eq!(
        realm.take(),
        [
            "clearOhlcvNoDataTimeout(candle/1/1m)",
            "callback({timestamp:1700000000,open:1,high:2,low:0.5,close:1.5,volume:10})",
        ]
    );
    let trade = realm.object(&[
        ("timestamp", n(1_700_000_030_000.0)),
        ("price", s("3")),
        ("size", s("2")),
    ]);
    let message = realm.object(&[("trade", trade)]);
    realm
        .invoke(
            &handler(&realm, "trade/1"),
            &AccountValue::Undefined,
            &[message],
        )
        .unwrap();
    assert_eq!(
        realm.take(),
        [
            "getCandleStartTime(1m,1700000030)",
            "callback({timestamp:1700000000,open:1,high:3,low:0.5,close:3,volume:12})",
        ]
    );
    realm.invoke(&stop, &AccountValue::Undefined, &[]).unwrap();
    assert_eq!(
        realm.take(),
        [
            "clearOhlcvNoDataTimeout(candle/1/1m)",
            "cleanupMessageHandlerStats(candle/1/1m)",
            "cleanupMessageHandlerStats(trade/1)",
            "sendPayload({type:unsubscribe,channel:candle/1/1m})",
            "sendPayload({type:unsubscribe,channel:trade/1})",
        ]
    );
    assert_eq!(realm.shown("topics"), "{}");
    assert_eq!(
        realm.shown("messageHandlers"),
        "{candle/1/1m:[],trade/1:[]}"
    );
}

#[test]
fn listen_ohlcv_retries_until_connected_and_cancels_retry() {
    let realm = Realm::new();
    listener_owner(&realm, false);
    let opts = realm.object(&[("symbol", s("BTC")), ("interval", s("1m"))]);
    let stop = listen_ohlcv(host(&realm), opts, n(0.0)).unwrap();
    assert_eq!(realm.take()[1..], ["setTimeout(100)"]);
    realm.invoke(&stop, &AccountValue::Undefined, &[]).unwrap();
    assert_eq!(realm.take()[0], "clearTimeout(901)");
    let missing = realm.object(&[("symbol", s("XRP"))]);
    let Err(error) = listen_ohlcv(host(&realm), missing, n(0.0)) else {
        panic!("missing market must throw")
    };
    assert_eq!(
        realm.show(&realm.error_value(error)),
        "{message:Market not found for symbol: XRP}"
    );
}

#[test]
fn listen_order_book_primes_parses_and_unsubscribes() {
    let realm = Realm::new();
    listener_owner(&realm, true);
    let callback = realm.noop("callback");
    let stop = listen_order_book(host(&realm), s("ETH"), callback).unwrap();
    assert_eq!(
        realm.take(),
        [
            "sendPayload({type:subscribe,channel:order_book/2})",
            "fetchAndEmitOrderBookFromRest(order_book:2,2,{bids:[],asks:[]},fn)",
            "armOrderBookStaleTimeout(order_book:2,order_book/2)",
            "clearOrderBookInitialDataTimeout(order_book:2)",
            "setTimeout(1500)",
        ]
    );
    assert_eq!(
        realm.shown("orderBookInitialDataTimeouts"),
        "{order_book:2:901}"
    );
    assert_eq!(realm.shown("topics"), "{order_book:2:order_book/2}");
    realm.fire(0);
    assert_eq!(
        realm.take(),
        [
            "startOrderBookRestFallback(order_book:2,2,{bids:[],asks:[]},fn,no initial websocket snapshot)"
        ]
    );
    assert_eq!(realm.shown("orderBookInitialDataTimeouts"), "{}");
    let levels = realm.object(&[("bids", realm.arr(&[n(9.0)])), ("asks", s("bad"))]);
    let frame = realm.object(&[("type", s("update/order_book")), ("order_book", levels)]);
    realm
        .invoke(
            &handler(&realm, "order_book:2"),
            &AccountValue::Undefined,
            &[frame],
        )
        .unwrap();
    assert_eq!(
        realm.take(),
        [
            "clearOrderBookInitialDataTimeout(order_book:2)",
            "stopOrderBookRestFallback(order_book:2)",
            "clearOrderBookStaleTimeout(order_book:2)",
            "applyOrderBookLevels({bids:[],asks:[]},bids,[9],snapshot)",
            "applyOrderBookLevels({bids:[],asks:[]},asks,[],snapshot)",
            "calcOrderBookTotal({bids:[],asks:[]})",
            "callback({bids:[],asks:[]})",
            "armOrderBookStaleTimeout(order_book:2,order_book/2)",
        ]
    );
    assert_eq!(
        realm.shown("orderBookRestFallbackCooldownUntilByTopic"),
        "{}"
    );
    assert_eq!(realm.shown("orderBookRecoveryAttempts"), "{order_book:2:0}");
    // A later timer firing after data arrived must not start the REST fallback.
    realm.fire(0);
    assert!(realm.take().is_empty());
    realm.invoke(&stop, &AccountValue::Undefined, &[]).unwrap();
    assert_eq!(
        realm.take(),
        [
            "cleanupMessageHandlerStats(order_book:2)",
            "clearOrderBookInitialDataTimeout(order_book:2)",
            "stopOrderBookRestFallback(order_book:2)",
            "clearOrderBookStaleTimeout(order_book:2)",
            "sendPayload({type:unsubscribe,channel:order_book/2})",
        ]
    );
    assert_eq!(realm.shown("topics"), "{}");
    assert_eq!(realm.shown("orderBookRecoveryAttempts"), "{}");
    let unknown = listen_order_book(host(&realm), s("DOGE"), n(0.0)).unwrap();
    assert_eq!(
        realm.take(),
        ["log([Lighter] listenOrderBook market not found for symbol: DOGE)"]
    );
    assert_eq!(realm.show(&unknown), "fn");
}

impl PublicSocketContexts for Realm {
    fn socket(&self, owner: AccountValue) -> Rc<dyn PublicSocketHost> {
        *self.owner.borrow_mut() = owner;
        self.rc()
    }
    fn readiness(&self, _: AccountValue) -> Rc<dyn PublicReadinessHost> {
        unimplemented!()
    }
    fn subscriptions(&self, _: AccountValue) -> Rc<dyn PublicSubscriptionHost> {
        unimplemented!()
    }
    fn cleanup(&self, _: AccountValue) -> Rc<dyn PublicCleanupHost> {
        unimplemented!()
    }
    fn watchdogs(&self, _: AccountValue) -> Rc<dyn PublicWatchdogHost> {
        unimplemented!()
    }
    fn ingress(&self, _: AccountValue) -> Rc<IngressOps<AccountValue, TransactionSigningError>> {
        unimplemented!()
    }
    fn store_tickers(&self, _: AccountValue) -> SigningResult<Vec<safe_cex_types::Ticker>> {
        Ok(vec![])
    }
    fn parse_rest_order_book_levels(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        assert!(matches!(value, AccountValue::String(_)));
        Ok(AccountValue::Undefined)
    }
    fn parse_rest_ticker_items(&self, value: &AccountValue) -> SigningResult<AccountValue> {
        assert!(matches!(value, AccountValue::String(_)));
        self.array_literal(&[])
    }
}
impl PublicSocketRealm for Realm {
    fn binding_value(&self, name: &str) -> SigningResult<AccountValue> {
        Ok(s(name))
    }
    fn create_derived_class(
        &self,
        base: AccountValue,
        name: &str,
        length: usize,
        _: PublicSocketConstructor,
    ) -> SigningResult<AccountValue> {
        self.log_trace(format!(
            "class {name}/{length} extends {}",
            self.show(&base)
        ));
        Ok(self.object(&[]))
    }
    fn arrow(&self, owner: &AccountValue, body: PublicMethod) -> SigningResult<AccountValue> {
        let owner = owner.clone();
        let meta = body.metadata();
        let value = self.native(Rc::new(move |realm, _, args| {
            body.invoke(realm, owner.clone(), args)
        }));
        self.set(
            &value,
            &s("meta"),
            s(&format!(
                "{}/{}/{}",
                meta.name, meta.length, meta.asynchronous
            )),
        )?;
        Ok(value)
    }
    fn define_data(
        &self,
        object: &AccountValue,
        key: &str,
        value: AccountValue,
    ) -> SigningResult<()> {
        let shown = match self.obj(&value) {
            Some(o) if o.call.is_some() => self.show(&self.get(&value, &s("meta"))?),
            _ => self.show(&value),
        };
        self.defined.borrow_mut().push(format!("{key}={shown}"));
        PublicSocketHost::set(self, object, &s(key), value)
    }
    fn get_property(&self, object: &AccountValue, key: &str) -> SigningResult<AccountValue> {
        PublicSocketHost::get(self, object, &s(key))
    }
    fn object_literal(&self, fields: &[(&str, AccountValue)]) -> SigningResult<AccountValue> {
        Ok(self.object(fields))
    }
    fn array_literal(&self, values: &[AccountValue]) -> SigningResult<AccountValue> {
        Ok(self.arr(values))
    }
    fn finish_class_binding(&self, _: &AccountValue) -> SigningResult<()> {
        self.log_trace("finish".into());
        Ok(())
    }
    fn construct_base(
        &self,
        _: AccountValue,
        _: AccountValue,
        args: &[AccountValue],
    ) -> SigningResult<AccountValue> {
        self.log_trace(format!("super({})", self.show(&arg(args, 0))));
        Ok(self.object(&[("parent", arg(args, 0))]))
    }
}

#[test]
fn class_defines_statics_and_constructor_initializes_fields_in_source_order() {
    let realm = Realm::new();
    let class = LighterPublicWebsocket::define(realm.as_ref()).unwrap();
    assert_eq!(
        realm.take(),
        [
            "class LighterPublicWebsocket/1 extends BaseWebSocket",
            "finish"
        ]
    );
    let statics = std::mem::take(&mut *realm.defined.borrow_mut());
    assert_eq!(
        statics,
        [
            "PROXY_ROUTE_FAILURES_BEFORE_DIRECT_FALLBACK=3",
            "PROXY_DIRECT_FALLBACK_FAILURE_COOLDOWN_MS=120000",
            "DIRECT_ROUTE_FAILURES_BEFORE_READONLY=2",
            "EARLY_CLOSE_WINDOW_MS=12000",
            "RATE_LIMIT_RECONNECT_DELAY_MS=30000",
            "QUIET_STREAM_STALE_THRESHOLD_MS=120000",
            "OHLCV_DIRECT_ROUTE_NO_DATA_TIMEOUT_MS=120000",
            "OHLCV_RECOVERY_RECONNECT_COOLDOWN_MS=120000",
            "ORDER_BOOK_STALE_TIMEOUT_MS=90000",
            "ORDER_BOOK_RECOVERY_COOLDOWN_MS=45000",
            "ORDER_BOOK_INITIAL_DATA_TIMEOUT_MS=1500",
            "ORDER_BOOK_REST_FALLBACK_INTERVAL_MS=2000",
            "ORDER_BOOK_REST_FALLBACK_403_COOLDOWN_MS=60000",
            "TICKER_REST_FALLBACK_CONNECTING_WATCHDOG_MS=1500",
            "TICKER_REST_FALLBACK_WATCHDOG_MS=8000",
            "TICKER_REST_FALLBACK_INTERVAL_MS=3000",
            "TICKER_REST_FALLBACK_403_COOLDOWN_MS=60000",
            "MAX_IMMEDIATE_SUBSCRIBE_MESSAGES=24",
            "SUBSCRIBE_STAGGER_DELAY_MS=15",
        ]
    );
    let parent_value = realm.object(&[("log", realm.noop("log"))]);
    let owner =
        LighterPublicWebsocket::construct(realm.as_ref(), class.clone(), class, &[parent_value])
            .unwrap();
    assert_eq!(realm.take(), ["super({log:fn})"]);
    let fields = std::mem::take(&mut *realm.defined.borrow_mut());
    assert_eq!(fields.len(), 84);
    assert_eq!(
        fields[..6],
        [
            "recoversAfterOffline=false",
            "bboTickerTopicPrefix=bbo_ticker_",
            "treatAnyMessageAsPong=true",
            "shouldEnforcePongMissReconnect=shouldEnforcePongMissReconnect/0/false",
            "getWatchdogStaleThresholdMs=getWatchdogStaleThresholdMs/0/false",
            "failedConnections=0",
        ]
    );
    assert_eq!(
        fields[33..37],
        [
            "topics={marketStatsAll:market_stats/all}",
            "messageHandlers={market_stats:all:[fn],ticker:[fn],tickers:[fn],pong:[fn]}",
            "isWsProxyRouteEnabled=isWsProxyRouteEnabled/0/false",
            "isTickerRestFallbackCoolingDown=isTickerRestFallbackCoolingDown/0/false",
        ]
    );
    assert!(fields.contains(
        &"fetchAndEmitOrderBookFromRest=fetchAndEmitOrderBookFromRest/4/true".to_string()
    ));
    assert_eq!(fields[83], "dispose=dispose/0/false");
    let names: Vec<_> = fields
        .iter()
        .map(|f| f.split('=').next().unwrap().to_string())
        .collect();
    let mut unique = names.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "each field defined once");

    // Arrow dispatch reaches the owner-bound bodies.
    let handle = realm.noop("handleMarketStatsEvent");
    realm
        .set(&owner, &s("handleMarketStatsEvent"), handle)
        .unwrap();
    let handlers = realm.get(&owner, &s("messageHandlers")).unwrap();
    let list = realm.get(&handlers, &s("market_stats:all")).unwrap();
    let first = realm.get(&list, &n(0.0)).unwrap();
    realm
        .invoke(&first, &AccountValue::Undefined, &[s("frame")])
        .unwrap();
    assert_eq!(realm.take(), ["handleMarketStatsEvent(frame)"]);
    let threshold = realm
        .get(&owner, &s("getWatchdogStaleThresholdMs"))
        .unwrap();
    assert_eq!(
        realm.show(
            &realm
                .invoke(&threshold, &AccountValue::Undefined, &[])
                .unwrap()
        ),
        "120000"
    );
    let cooling = realm
        .get(&owner, &s("isTickerRestFallbackCoolingDown"))
        .unwrap();
    realm
        .set(&owner, &s("tickerRestFallbackCooldownUntil"), n(6000.0))
        .unwrap();
    assert_eq!(
        realm.show(
            &realm
                .invoke(&cooling, &AccountValue::Undefined, &[])
                .unwrap()
        ),
        "true"
    );
    let levels = realm.get(&owner, &s("parseRestOrderBookLevels")).unwrap();
    let parsed = realm
        .invoke(&levels, &AccountValue::Undefined, &[s("payload")])
        .unwrap();
    assert_eq!(realm.show(&parsed), "undefined");
    let hydrated = realm.get(&owner, &s("hasHydratedTickerPrice")).unwrap();
    assert_eq!(
        realm.show(
            &realm
                .invoke(&hydrated, &AccountValue::Undefined, &[])
                .unwrap()
        ),
        "false"
    );
}
