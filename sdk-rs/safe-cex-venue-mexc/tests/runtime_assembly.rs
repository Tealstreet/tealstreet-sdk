#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_venue_mexc::{
    exchange_runtime::*,
    method_tasks::{Completion, MethodObserver, Runtime, SessionRuntime, TaskHost},
    order_workflow::*,
    runtime_assembly::*,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
type JsResult<V, T = V> = Result<T, V>;
type JsPromise<V> = Pin<Box<dyn Future<Output = JsResult<V>>>>;
include!("support/exchange_runtime_values.rs");
type Task = Rc<RefCell<Option<Pin<Box<dyn Future<Output = ()>>>>>>;
struct Host {
    me: RefCell<std::rc::Weak<Self>>,
    group: String,
    name: String,
    ex: N,
    events: RefCell<Vec<N>>,
    tasks: RefCell<Vec<Task>>,
    ops: RefCell<Vec<Rc<Job>>>,
    failure: N,
    opts: N,
    order: N,
    patch: N,
    replacement_ids: N,
    market: N,
    position: N,
    leaf: N,
    api: N,
    delegate_callback: RefCell<Option<N>>,
    ctor_bindings: RefCell<IndexMap<String, N>>,
}
impl Completion<N, N> for Job {
    fn poll(&self, _: &mut Context<'_>) -> Poll<JsResult<N>> {
        self.result
            .borrow()
            .clone()
            .map(Poll::Ready)
            .unwrap_or(Poll::Pending)
    }
}
fn fn_same(a: &N, b: &N) -> bool {
    match (a, b) {
        (N::F(a), N::F(b)) => Rc::ptr_eq(a, b),
        _ => a.same(b),
    }
}
fn event_value(v: N) -> N {
    match v {
        N::F(_) => N::from(json!({"callable":true})),
        N::A(v) => N::array(v.borrow().iter().cloned().map(event_value).collect()),
        v => v.snapshot(),
    }
}
impl Host {
    fn self_rc(&self) -> Rc<Self> {
        self.me.borrow().upgrade().unwrap()
    }
    fn f(&self, body: impl Fn(N, Vec<N>) -> JsResult<N> + 'static) -> N {
        N::F(Rc::new(body))
    }
    fn event(&self, k: &str, a: Vec<N>) -> JsResult<N, ()> {
        let mut row = vec![N::S(k.into())];
        row.extend(a.into_iter().map(event_value));
        self.events.borrow_mut().push(N::array(row));
        if self.name == format!("{k}-error") {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
    fn ready(&self, r: JsResult<N>) -> Rc<Job> {
        Rc::new(Job {
            future: RefCell::new(None),
            result: RefCell::new(Some(r)),
        })
    }
    fn promise(&self, r: JsResult<N>) -> N {
        N::P(self.ready(r))
    }
    fn pending(&self, f: JsPromise<N>) -> N {
        let j = Rc::new(Job {
            future: RefCell::new(Some(f)),
            result: RefCell::new(None),
        });
        self.ops.borrow_mut().push(j.clone());
        N::P(j)
    }
    fn poll_task(t: &Task) {
        let Some(mut f) = t.borrow_mut().take() else {
            return;
        };
        if f.as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
        {
            *t.borrow_mut() = Some(f);
        }
    }
    fn drive(&self) {
        let jobs = self.ops.borrow().clone();
        for j in jobs {
            let task = j.future.borrow_mut().take();
            if let Some(mut f) = task {
                match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    Poll::Ready(r) => *j.result.borrow_mut() = Some(r),
                    Poll::Pending => *j.future.borrow_mut() = Some(f),
                }
            }
        }
        let tasks = self.tasks.borrow().clone();
        for t in tasks {
            Self::poll_task(&t);
        }
    }
    fn finish(&self, mut f: JsPromise<N>) -> JsResult<N> {
        for _ in 0..256 {
            self.drive();
            if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                return r;
            }
        }
        panic!("native task unsettled at poll256")
    }
    fn method(&self, target: &N, key: &str, body: impl Fn(N, Vec<N>) -> JsResult<N> + 'static) {
        target.set(key, self.f(body));
    }
    fn number_value(v: N) -> f64 {
        match v {
            N::Num(v) => v,
            N::Null => 0.,
            N::B(v) => {
                if v {
                    1.
                } else {
                    0.
                }
            }
            N::S(v) => v.parse().unwrap_or(f64::NAN),
            _ => f64::NAN,
        }
    }
    fn new(group: &str, name: &str) -> Rc<Self> {
        let opts = N::from(
            json!({"symbol":"BTC/USDT","type":if name=="take-profit"{"take_profit_market"}else{"stop_market"},"side":"sell","amount":if name=="zero"{0.}else if name=="negative"{-5.}else if name=="minimum"{0.1}else if name=="over-max"{51.}else{5.},"price":19.24}),
        );
        if name == "missing-trigger" {
            let N::O(v) = &opts else { unreachable!() };
            v.borrow_mut().shift_remove("price");
        }
        if name == "string-trigger" {
            opts.set("triggerPrice", N::S("19".into()));
        }
        let market = N::from(
            json!({"id":"BTC_USDT","info":{"volUnit":2,"maxVol":50},"precision":{"price":0.5},"limits":{"leverage":{"min":2,"max":50}}}),
        );
        let position = N::from(
            json!({"symbol":"BTC/USDT","side":"long","contracts":if name=="negative-contracts"{-1}else{5},"extra":{"contractsNative":if name=="zero-volume"{0}else{5}},"info":{"positionId":"p1"}}),
        );
        if name == "nan-contracts" {
            position.set("contracts", N::Num(f64::NAN));
        }
        if name == "no-id" {
            let N::O(v) = position.get("info") else {
                unreachable!()
            };
            v.borrow_mut().shift_remove("positionId");
        }
        if name == "info-volume" {
            let N::O(v) = &position else { unreachable!() };
            v.borrow_mut().shift_remove("extra");
            position.get("info").set("holdVol", N::Num(7.));
        }
        let store = N::obj(vec![
            (
                "marketsBySymbol",
                if name == "no-market" {
                    N::obj(vec![])
                } else {
                    N::obj(vec![("BTC/USDT", market.clone())])
                },
            ),
            (
                "positions",
                N::array(if matches!(name, "fetch" | "no-position") {
                    vec![]
                } else {
                    vec![position.clone()]
                }),
            ),
            ("loaded", N::from(json!({"keep":3}))),
        ]);
        let order = N::from(
            json!({"id":"o1","symbol":"BTC/USDT","type":"limit","side":"buy","amount":10,"price":7,"reduceOnly":true,"timeInForce":"PostOnly"}),
        );
        let patch = N::from(match name {
            "explicit" => json!({"amount":3,"price":8}),
            "null-price" => json!({"price":null}),
            _ => json!({}),
        });
        if name == "undefined-amount" {
            patch.set("amount", N::U);
        }
        let ex = N::obj(vec![
            ("store", store),
            ("isDisposed", N::B(name == "disposed")),
            (
                "isPublicDataOnly",
                N::B(matches!(name, "public" | "external-public")),
            ),
            ("accountPositionMode", N::S("hedged".into())),
        ]);
        let settings = Rc::new(RefCell::new(IndexMap::new()));
        if name != "derive" {
            settings.borrow_mut().insert(
                "BTC/USDT".into(),
                N::from(json!({"marginType":"cross","positionMode":"hedged"})),
            );
        }
        ex.set("symbolSettings", N::M(settings));
        let leaf = N::F(Rc::new(|_, _| Ok(N::U)));
        let api = N::F(Rc::new(|_, _| Ok(N::U)));
        let h = Rc::new(Self {
            me: RefCell::new(std::rc::Weak::new()),
            group: group.into(),
            name: name.into(),
            ex,
            events: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            failure: N::from(
                json!({"message":"original","response":{"data":{"message":"venue","msg":"alternate"}}}),
            ),
            opts,
            order,
            patch,
            replacement_ids: N::from(json!(["replacement-id"])),
            market,
            position,
            leaf,
            api,
            delegate_callback: RefCell::new(None),
            ctor_bindings: RefCell::new(IndexMap::new()),
        });
        *h.me.borrow_mut() = Rc::downgrade(&h);
        h.install();
        h
    }
    fn install(&self) {
        let store = self.ex.get("store");
        let h = self.self_rc();
        let original = store.clone();
        self.method(&store, "update", move |r, a| {
            assert!(r.same(&original));
            h.event("update", a.clone())?;
            let N::O(values) = &a[0] else { unreachable!() };
            for (k, v) in values.borrow().iter() {
                original.set(k, v.clone());
            }
            Ok(N::U)
        });
        let h = self.self_rc();
        let original = store.clone();
        self.method(&store, "setSetting", move |r, a| {
            assert!(r.same(&original));
            h.event("setting", a)?;
            Ok(N::U)
        });
        for (key, tag) in [
            ("initExternalPublicData", "external"),
            ("fetchMarkets", "markets"),
            ("fetchTickers", "tickers"),
            ("fetchPositionMode", "mode"),
            ("fetchBalanceAndPositions", "pair"),
            ("fetchOrders", "orders"),
            ("tick", "tick"),
            ("log", "log"),
            ("emitAndLogError", "emit"),
            ("getLeverageForOrder", "leverage"),
            ("deriveSymbolConfig", "derive"),
            ("planOrderTriggerType", "trigger"),
            ("stopOrderTrend", "trend"),
            ("placePositionStopOrder", "position-stop"),
            ("addOptimisticStopOrder", "optimistic"),
            ("scheduleStopOrderSnapshotRefresh", "refresh"),
            ("findOpenPosition", "find"),
            ("nativePositionId", "native-id"),
            ("fetchPositions", "fetch"),
            ("getReplacementOrderAmount", "replacement"),
            ("cancelOrders", "cancel"),
            ("placeOrder", "place"),
        ] {
            let h = self.self_rc();
            self.method(&self.ex, key, move |r, a| {
                assert!(r.same(&h.ex), "native original exchange receiver {key}");
                if matches!(
                    key,
                    "getLeverageForOrder"
                        | "planOrderTriggerType"
                        | "stopOrderTrend"
                        | "placePositionStopOrder"
                        | "addOptimisticStopOrder"
                ) {
                    assert!(a[0].same(&h.opts), "native original option identity");
                }
                if matches!(key, "placePositionStopOrder" | "addOptimisticStopOrder") {
                    assert!(a[1].same(&h.market), "native original market identity");
                }
                if key == "nativePositionId" {
                    assert!(a[0].same(&h.position));
                }
                if key == "getReplacementOrderAmount" {
                    assert!(a[0].same(&h.order));
                }
                if key == "cancelOrders" {
                    assert!(a[0].get("0").same(&h.order));
                }
                if key == "placeOrder" {
                    assert!(
                        !a[0].same(&h.order) && !a[0].same(&h.patch),
                        "native fresh replacement options"
                    );
                }
                let args = if matches!(
                    key,
                    "getLeverageForOrder"
                        | "planOrderTriggerType"
                        | "stopOrderTrend"
                        | "nativePositionId"
                        | "getReplacementOrderAmount"
                ) {
                    vec![]
                } else if key == "placePositionStopOrder" {
                    vec![a[2].clone()]
                } else if key == "addOptimisticStopOrder" {
                    a[2..].to_vec()
                } else {
                    a.clone()
                };
                let effect = h.event(tag, args);
                match key {
                    "initExternalPublicData" => {
                        Ok(h.promise(effect.map(|_| {
                            N::B(matches!(h.name.as_str(), "external" | "external-public"))
                        })))
                    }
                    "fetchMarkets" | "fetchTickers" => {
                        if h.name == format!("disposed-{tag}") {
                            h.ex.set("isDisposed", N::B(true));
                        }
                        Ok(h.promise(effect.map(|_| {
                            N::from(if key == "fetchMarkets" {
                                json!([{"id":1},{"id":2}])
                            } else {
                                json!([{"id":3}])
                            })
                        })))
                    }
                    "fetchPositionMode" => Ok(h.promise(effect.map(|_| N::S("oneway".into())))),
                    "fetchBalanceAndPositions" => {
                        if h.name == "disposed-pair" {
                            h.ex.set("isDisposed", N::B(true));
                        }
                        effect?;
                        Ok(h.promise(Ok({
                            N::from(json!({"balance":{"total":9},"positions":[{"id":"position"}]}))
                        })))
                    }
                    "fetchOrders" => {
                        Ok(h.promise(effect.map(|_| N::from(json!([{"id":"order"}])))))
                    }
                    "tick" => Ok(h.promise(effect.map(|_| N::U))),
                    "getLeverageForOrder" => {
                        effect?;
                        Ok(N::Num(7.))
                    }
                    "deriveSymbolConfig" => {
                        effect?;
                        Ok(N::from(
                            json!({"marginType":"isolated","positionMode":"oneway"}),
                        ))
                    }
                    "planOrderTriggerType" => {
                        effect?;
                        Ok(N::Num(
                            if h.opts.get("type").text() == "take_profit_market" {
                                1.
                            } else {
                                2.
                            },
                        ))
                    }
                    "stopOrderTrend" => {
                        effect?;
                        Ok(N::Num(3.))
                    }
                    "placePositionStopOrder" => {
                        Ok(h.promise(effect.map(|_| N::S("full-id".into()))))
                    }
                    "findOpenPosition" => {
                        effect?;
                        Ok(a[2].get("0"))
                    }
                    "nativePositionId" => {
                        effect?;
                        let id = h.position.get("info").get("positionId");
                        Ok(if h.truthy(&id) { id } else { N::S("".into()) })
                    }
                    "fetchPositions" => {
                        if h.name == "fetch" {
                            let cb = h.clone();
                            h.method(&h.ex, "findOpenPosition", move |_, _| {
                                cb.event("replacement-find", vec![])?;
                                Ok(N::U)
                            });
                        }
                        Ok(h.promise(effect.map(|_| {
                            N::array(if h.name == "no-position" {
                                vec![]
                            } else {
                                vec![h.position.clone()]
                            })
                        })))
                    }
                    "getReplacementOrderAmount" => {
                        effect?;
                        Ok(N::Num(6.))
                    }
                    "cancelOrders" => {
                        if h.name == "mutate-order" {
                            h.order.set("symbol", N::S("CHANGED".into()));
                        }
                        Ok(h.promise(effect.map(|_| N::U)))
                    }
                    "placeOrder" => Ok(h.promise(effect.map(|_| h.replacement_ids.clone()))),
                    _ => {
                        effect?;
                        Ok(N::U)
                    }
                }
            });
        }
        for (key, tag) in [
            ("publicWebsocket", "public-connect"),
            ("privateWebsocket", "private-connect"),
        ] {
            let target = N::obj(vec![]);
            let h = self.self_rc();
            let original = target.clone();
            self.method(&target, "connectAndSubscribe", move |r, _| {
                assert!(r.same(&original));
                h.event(tag, vec![])?;
                Ok(N::U)
            });
            if key == "publicWebsocket" {
                self.leaves(&target);
            }
            self.ex.set(key, target);
        }
        self.ex.set(
            "externalPublicDataProvider",
            if self.name == "external" {
                let provider = N::obj(vec![]);
                self.leaves(&provider);
                provider
            } else {
                N::U
            },
        );
        let xhr = N::obj(vec![]);
        let h = self.self_rc();
        let original = xhr.clone();
        self.method(&xhr, "post", move |r, a| {
            assert!(r.same(&original));
            let effect = h.event("post", a);
            let data = if h.group == "placePlanOrder" {
                json!({"orderId":if h.name=="no-id"{""}else{"plan-id"}})
            } else {
                match h.name.as_str() {
                    "scalar" => json!("scalar-id"),
                    "numeric" => json!(42),
                    "array" => json!([{"id":"array-id"}]),
                    "no-id-response" => json!({}),
                    _ => json!({"id":"stop-id"}),
                }
            };
            Ok(h.promise(effect.map(|_| N::from(json!({"data":{"data":data}})))))
        });
        self.ex.set("xhr", xhr);
    }
    fn leaves(&self, target: &N) {
        for key in [
            "listenOHLCV",
            "listenOrderBook",
            "subscribeCandle",
            "subscribeOrderbook",
        ] {
            let h = self.self_rc();
            let original = target.clone();
            self.method(target, key, move |r, a| {
                assert!(r.same(&original), "native original subscription receiver");
                assert!(
                    fn_same(&a[1], h.delegate_callback.borrow().as_ref().unwrap()),
                    "native original delegated callback identity"
                );
                if h.group == "listenOHLCV" {
                    assert!(a[0].same(&h.opts));
                }
                let mut args = vec![N::S(key.into())];
                args.extend(a);
                h.event("leaf", args)?;
                Ok(h.leaf.clone())
            });
        }
    }
}
impl Runtime for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn truthy(&self, v: &N) -> bool {
        match v {
            N::U | N::Null => false,
            N::B(v) => *v,
            N::Num(v) => *v != 0. && !v.is_nan(),
            N::S(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn same_value(&self, a: &N, b: &N) -> bool {
        self.equal(a, b)
    }
    fn is_text(&self, v: &N, text: &str) -> bool {
        matches!(v,N::S(v)if v==text)
    }
    fn field(&self, v: &N, key: &str) -> JsResult<N> {
        if v.same(&self.patch) && self.name == "inherited" && matches!(key, "amount" | "price") {
            return Ok(N::Num(if key == "amount" { 13. } else { 14. }));
        }
        if v.same(&self.ex) && key == "createXhrWithSecondaryKey" {
            let h = self.self_rc();
            return Ok(self.f(move |r, a| {
                assert!(r.same(&h.ex));
                assert!(a[0].same(&h.ex.get("options")));
                assert!(fn_same(&a[1], &h.api));
                h.event("xhr", vec![])?;
                Ok(N::obj(vec![("marker", N::S("xhr".into()))]))
            }));
        }
        if matches!(v, N::U | N::Null) {
            return Err(error(
                "TypeError",
                &format!("Cannot read properties of {} (reading '{key}')", v.text()),
            ));
        }
        if let N::M(map) = v {
            let map = map.clone();
            let key = key.to_owned();
            return Ok(self.f(move |_, a| match key.as_str() {
                "get" => Ok(map.borrow().get(&a[0].text()).cloned().unwrap_or(N::U)),
                "set" => {
                    map.borrow_mut().insert(a[0].text(), a[1].clone());
                    Ok(N::M(map.clone()))
                }
                "delete" => Ok(N::B(map.borrow_mut().shift_remove(&a[0].text()).is_some())),
                "values" => Ok(N::Iter(map.clone())),
                _ => Ok(N::U),
            }));
        }
        if let N::P(job) = v {
            if matches!(key, "catch" | "then") {
                let job = job.clone();
                let catch = key == "catch";
                let h = self.self_rc();
                return Ok(self.f(move |r, a| {
                    assert!(matches!(r, N::P(_)));
                    let op = job.clone();
                    let cb = a.clone();
                    let host = h.clone();
                    Ok(h.pending(Box::pin(async move {
                        let result = Handle(op).await;
                        let rejected = result.is_err();
                        let (callback, value) = match result {
                            Ok(v) => {
                                if catch {
                                    return Ok(v);
                                } else {
                                    (cb.first().cloned().unwrap_or(N::U), v)
                                }
                            }
                            Err(v) => (
                                cb.get(if catch { 0 } else { 1 }).cloned().unwrap_or(N::U),
                                v,
                            ),
                        };
                        if matches!(callback, N::U) {
                            return if rejected { Err(value) } else { Ok(value) };
                        }
                        let v = host.call(callback, N::U, vec![value])?;
                        Handle(host.resolve_return(v)?).await
                    })))
                }));
            }
        }
        if let N::A(values) = v {
            if key == "length" {
                return Ok(N::Num(values.borrow().len() as f64));
            }
            if matches!(
                key,
                "map" | "filter" | "find" | "every" | "forEach" | "push"
            ) {
                let key = key.to_owned();
                let h = self.self_rc();
                return Ok(self.f(move |r, a| {
                    let N::A(values) = &r else {
                        panic!("array receiver")
                    };
                    if key == "push" {
                        values.borrow_mut().extend(a);
                        return Ok(N::Num(values.borrow().len() as f64));
                    }
                    let len = values.borrow().len();
                    let mut out = vec![];
                    for i in 0..len {
                        let v = values.borrow().get(i).cloned().unwrap_or(N::U);
                        let result = h.call(
                            a[0].clone(),
                            N::U,
                            vec![v.clone(), N::Num(i as f64), r.clone()],
                        )?;
                        match key.as_str() {
                            "map" => out.push(result),
                            "filter" => {
                                if h.truthy(&result) {
                                    out.push(v)
                                }
                            }
                            "find" => {
                                if h.truthy(&result) {
                                    return Ok(v);
                                }
                            }
                            "every" => {
                                if !h.truthy(&result) {
                                    return Ok(N::B(false));
                                }
                            }
                            _ => {}
                        }
                    }
                    Ok(match key.as_str() {
                        "find" | "forEach" => N::U,
                        "every" => N::B(true),
                        _ => N::array(out),
                    })
                }));
            }
        }
        Ok(v.get(key))
    }
    fn abstract_string(&self, v: &N) -> JsResult<N, Vec<u16>> {
        Ok(v.text().encode_utf16().collect())
    }
    fn global_string(&self, v: &N) -> JsResult<N, Vec<u16>> {
        self.abstract_string(v)
    }
    fn log(&self, text: &[u16], severity: &'static str) -> JsResult<N, ()> {
        self.event(
            "log",
            vec![
                N::S(String::from_utf16(text).unwrap()),
                N::S(severity.into()),
            ],
        )
    }
    fn new_error(&self, text: Vec<u16>) -> N {
        error("Error", &String::from_utf16(&text).unwrap())
    }
}
impl SessionRuntime for Host {
    fn text(&self, units: Vec<u16>) -> N {
        N::S(String::from_utf16(&units).unwrap())
    }
    fn nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn json_parse(&self, _: &N) -> JsResult<N> {
        panic!("unused JSON boundary")
    }
    fn stringify(&self, value: &N) -> JsResult<N, Vec<u16>> {
        Ok(stringify(value).encode_utf16().collect())
    }
    fn spread(&self, value: &N) -> JsResult<N> {
        let out = N::obj(vec![]);
        if let N::O(value) = value {
            for (k, v) in value.borrow().iter() {
                out.set(k, v.clone());
            }
        }
        Ok(out)
    }
    fn define_field(&self, v: &N, key: &str, value: N) -> JsResult<N, ()> {
        v.set(key, value);
        Ok(())
    }
    fn set_field(&self, v: &N, key: &str, value: N) -> JsResult<N, ()> {
        v.set(key, value);
        Ok(())
    }
}
impl TaskHost for Host {
    type Operation = Job;
    type Resolution = Job;
    fn resolve_return(&self, value: N) -> JsResult<N, Rc<Job>> {
        Ok(match value {
            N::P(job) => job,
            value => self.ready(Ok(value)),
        })
    }
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        let task = Rc::new(RefCell::new(Some(task)));
        self.tasks.borrow_mut().push(task.clone());
        Self::poll_task(&task);
    }
}
impl ExchangeRuntimeHost for Host {
    fn exchange(&self) -> N {
        self.ex.clone()
    }
    fn call(&self, f: N, receiver: N, args: Vec<N>) -> JsResult<N> {
        if let N::F(f) = f {
            f(receiver, args)
        } else {
            Err(error("TypeError", "not callable"))
        }
    }
    fn function(&self, f: Callback<N, N>) -> N {
        self.f(move |_, a| f(a.first().cloned().unwrap_or(N::U)))
    }
    fn object(&self, fields: Vec<(&str, N)>) -> N {
        N::obj(fields)
    }
    fn array(&self, values: Vec<N>) -> N {
        N::array(values)
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn error_value(&self, v: &N) -> N {
        v.clone()
    }
    fn equal(&self, a: &N, b: &N) -> bool {
        match (a, b) {
            (N::U, N::U) | (N::Null, N::Null) => true,
            (N::S(a), N::S(b)) => a == b,
            (N::Num(a), N::Num(b)) => a == b,
            (N::B(a), N::B(b)) => a == b,
            _ => a.same(b),
        }
    }
    fn indexed(&self, v: &N, key: &N) -> JsResult<N> {
        self.field(v, &String::from_utf16(&self.abstract_string(key)?).unwrap())
    }
    fn destructure_pair(&self, value: N) -> JsResult<N, (N, N)> {
        if let N::Pair(values) = value {
            self.event("entry-close", vec![values[0].clone()])?;
            Ok((values[0].clone(), values[1].clone()))
        } else {
            Ok((self.field(&value, "0")?, self.field(&value, "1")?))
        }
    }
    fn maximum(&self, a: N, b: N) -> JsResult<N> {
        Ok(N::Num(safe_cex_num::math_max(
            Self::number_value(a),
            Self::number_value(b),
        )))
    }
    fn minimum(&self, a: N, b: N) -> JsResult<N> {
        Ok(N::Num(safe_cex_num::math_min(
            Self::number_value(a),
            Self::number_value(b),
        )))
    }
    fn endpoint(&self, name: &str) -> N {
        N::from(safe_cex_venue_mexc::types::ENDPOINTS[name].clone())
    }
    fn to_symbol(&self, value: N) -> JsResult<N> {
        Ok(N::S(safe_cex_venue_mexc::utils::to_mexc_symbol(
            &value.text(),
        )))
    }
    fn omit_undefined(&self, value: N) -> JsResult<N> {
        if let N::O(v) = &value {
            v.borrow_mut().retain(|_, v| !matches!(v, N::U));
        }
        Ok(value)
    }
    fn reverse_position_mode(&self, mode: &N) -> JsResult<N> {
        Ok(N::from(
            safe_cex_venue_mexc::types::REVERSE_POSITION_MODE[mode.text()].clone(),
        ))
    }
    fn promise_all(&self, values: Vec<N>) -> JsResult<N, Rc<Job>> {
        let ops = values
            .into_iter()
            .map(|value| self.resolve_return(value).unwrap())
            .collect::<Vec<_>>();
        let future = Box::pin(async move {
            let values = futures::future::try_join_all(ops.into_iter().map(Handle)).await?;
            Ok(N::array(values))
        });
        let job = Rc::new(Job {
            future: RefCell::new(Some(future)),
            result: RefCell::new(None),
        });
        self.ops.borrow_mut().push(job.clone());
        Ok(job)
    }
    fn for_of(&self, value: N, visitor: &mut Iteration<'_, N, N>) -> JsResult<N, ()> {
        let mut index = 0;
        loop {
            let next = match &value {
                N::M(map) => map.borrow().get_index(index).map(|(k, v)| {
                    if self.name == "iterable-entry" {
                        N::Pair(vec![N::S(k.clone()), v.clone()])
                    } else {
                        N::array(vec![N::S(k.clone()), v.clone()])
                    }
                }),
                N::Iter(map) => map.borrow().get_index(index).map(|(_, v)| v.clone()),
                N::A(v) => v.borrow().get(index).cloned(),
                _ => panic!("native iterable"),
            };
            let Some(value) = next else { break };
            if visitor(value)? {
                break;
            }
            index += 1;
        }
        Ok(())
    }
    fn schedule_loop(&self, _: Rc<dyn Fn() -> JsResult<N>>, _: N) -> JsResult<N, ()> {
        panic!("unused loop")
    }
    fn clear_timeout(&self, timer: N) -> JsResult<N, ()> {
        self.event("clear", vec![timer])
    }
    fn base_dispose(&self) -> JsResult<N, ()> {
        self.event("base", vec![])?;
        self.ex.set("isDisposed", N::B(true));
        Ok(())
    }
}
impl OrderWorkflowHost for Host {
    fn import(&self, name: &str, a: Vec<N>) -> JsResult<N> {
        match name {
            "toNumber" => {
                let fallback = a.get(1).cloned().unwrap_or(N::Num(0.));
                if matches!(a[0], N::U | N::Null) || self.is_text(&a[0], "") {
                    return Ok(fallback);
                }
                let n = safe_cex_num::parse_float(&a[0].text());
                Ok(if n.is_finite() { N::Num(n) } else { fallback })
            }
            "toIntegerString" => Ok(N::S(if matches!(a[0], N::U | N::Null) {
                String::new()
            } else {
                a[0].text()
            })),
            "adjust" => Ok(N::Num(match &a[1] {
                N::S(s) => safe_cex_num::adjust(Self::number_value(a[0].clone()), s.as_str()),
                _ => safe_cex_num::adjust(
                    Self::number_value(a[0].clone()),
                    Self::number_value(a[1].clone()),
                ),
            })),
            "extractData" => {
                let data = if matches!(a[0], N::U | N::Null) {
                    N::U
                } else {
                    a[0].get("data")
                };
                Ok(if matches!(data, N::U | N::Null) {
                    a[0].clone()
                } else {
                    data
                })
            }
            _ => panic!("unused import {name}"),
        }
    }
    fn constant(&self, name: &str) -> N {
        use safe_cex_venue_mexc::types::*;
        match name {
            "REVERSE_ORDER_TYPE" => N::from((*REVERSE_ORDER_TYPE).clone()),
            "REVERSE_ORDER_SIDE" => N::from((*REVERSE_ORDER_SIDE).clone()),
            "REVERSE_MARGIN_TYPE" => N::from((*REVERSE_MARGIN_TYPE).clone()),
            "REVERSE_POSITION_MODE" => N::from((*REVERSE_POSITION_MODE).clone()),
            "GRAFANA_ENDPOINTS_EVERY" => N::from((*GRAFANA_ENDPOINTS_EVERY).clone()),
            "GRAFANA_ENDPOINTS_PERIODIC" => N::from((*GRAFANA_ENDPOINTS_PERIODIC).clone()),
            "MEXC_EXECUTE_CYCLE_SEVEN_DAYS" => N::Num(2.),
            "MEXC_TRIGGER_TREND_LATEST_PRICE" => N::Num(1.),
            _ => panic!("constant {name}"),
        }
    }
    fn function_args(&self, cb: Arguments<N, N>) -> N {
        self.f(move |_, a| cb(a))
    }
    fn promise_value(&self, observer: MethodObserver<N, N>) -> N {
        self.pending(Box::pin(observer))
    }
    fn add(&self, a: N, b: N) -> JsResult<N> {
        Ok(if matches!(a, N::S(_)) || matches!(b, N::S(_)) {
            N::S(format!("{}{}", a.text(), b.text()))
        } else {
            N::Num(Self::number_value(a) + Self::number_value(b))
        })
    }
    fn subtract(&self, a: N, b: N) -> JsResult<N> {
        Ok(N::Num(Self::number_value(a) - Self::number_value(b)))
    }
    fn less(&self, a: N, b: N) -> JsResult<N, bool> {
        Ok(Self::number_value(a) < Self::number_value(b))
    }
    fn is_object(&self, v: &N) -> bool {
        matches!(v, N::O(_) | N::E(_) | N::A(_) | N::M(_) | N::P(_) | N::Null)
    }
    fn property_key(&self, v: N) -> JsResult<N> {
        Ok(N::S(v.text()))
    }
    fn define_key(&self, o: &N, k: N, v: N) -> JsResult<N, ()> {
        o.set(&k.text(), v);
        Ok(())
    }
    fn set_timeout(&self, _: Rc<dyn Fn() -> JsResult<N>>, _: N) -> JsResult<N> {
        panic!("unused timer")
    }
}
fn stringify(v: &N) -> String {
    match v {
        N::U | N::Null => "null".into(),
        N::B(v) => v.to_string(),
        N::Num(v) => {
            if v.is_finite() {
                safe_cex_num::js_number_string(*v)
            } else {
                "null".into()
            }
        }
        N::S(v) => serde_json::to_string(v).unwrap(),
        N::A(v) => format!(
            "[{}]",
            v.borrow()
                .iter()
                .map(stringify)
                .collect::<Vec<_>>()
                .join(",")
        ),
        N::O(v) | N::E(v) => format!(
            "{{{}}}",
            v.borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, N::U | N::F(_)))
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), stringify(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => panic!("unbound JSON value"),
    }
}
impl AssemblyHost for Host {
    fn base_exchange(&self, name: N, opts: N, store: N) -> JsResult<N> {
        self.event("base", vec![name, opts.clone(), store.clone()])?;
        let N::O(fields) = &self.ex else {
            unreachable!()
        };
        *self.ctor_bindings.borrow_mut() = fields
            .borrow()
            .iter()
            .filter(|(_, v)| matches!(v, N::F(_)))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        fields.borrow_mut().clear();
        self.ex.set("options", opts);
        self.ex.set("store", store);
        self.ex.set("isDisposed", N::B(false));
        self.ex.set("isPublicDataOnly", N::B(false));
        Ok(self.ex.clone())
    }
    fn member(&self, name: &'static str, owner: &N) -> JsResult<N> {
        assert!(owner.same(&self.ex));
        let known = self
            .ctor_bindings
            .borrow()
            .get(name)
            .cloned()
            .unwrap_or(N::U);
        Ok(if matches!(known, N::F(_)) {
            known
        } else {
            self.f(move |_, _| Err(error("Error", &format!("unbound fixture member {name}"))))
        })
    }
    fn constructor(&self, name: &str) -> N {
        if name == "createAPI" {
            self.api.clone()
        } else {
            N::S(name.into())
        }
    }
    fn construct(&self, constructor: N, a: Vec<N>) -> JsResult<N> {
        assert!(a[0].same(&self.ex), "native child original parent identity");
        let tag = match constructor.text().as_str() {
            "MexcPublicWebsocket" => "public",
            "MexcPrivateWebsocket" => "private",
            _ => panic!("constructor"),
        };
        self.event(tag, vec![])?;
        Ok(N::obj(vec![("marker", N::S(tag.into()))]))
    }
    fn fresh_map(&self) -> N {
        N::M(Rc::new(RefCell::new(IndexMap::new())))
    }
    fn resolved_promise(&self) -> N {
        self.promise(Ok(N::U))
    }
    fn finite(&self, v: &N) -> bool {
        matches!(v,N::Num(n)if n.is_finite())
    }
    fn is_array(&self, v: &N) -> bool {
        matches!(v, N::A(_))
    }
    fn is_string_or_number(&self, v: &N) -> bool {
        matches!(v, N::S(_) | N::Num(_))
    }
    fn has_property(&self, v: &N, key: &str) -> JsResult<N, bool> {
        if v.same(&self.patch) && self.name == "inherited" {
            return Ok(matches!(key, "amount" | "price"));
        }
        match v {
            N::O(v) => Ok(v.borrow().contains_key(key)),
            _ => Err(error("TypeError", "right-hand side of in is not an object")),
        }
    }
    fn delete_indexed(&self, v: &N, key: &N) -> JsResult<N, ()> {
        let N::O(v) = v else {
            panic!("native indexed delete")
        };
        v.borrow_mut()
            .shift_remove(&String::from_utf16(&self.abstract_string(key)?).unwrap());
        Ok(())
    }
    fn destructure_fields(&self, v: &N, keys: &[&str]) -> JsResult<N, Vec<N>> {
        keys.iter().map(|key| self.field(v, key)).collect()
    }
    fn less_equal(&self, a: N, b: N) -> JsResult<N, bool> {
        Ok(Self::number_value(a) <= Self::number_value(b))
    }
    fn set_indexed(&self, v: &N, key: &N, value: N) -> JsResult<N, ()> {
        if self.name == "setter" {
            self.event("book-set", vec![value])?;
        } else {
            v.set(
                &String::from_utf16(&self.abstract_string(key)?).unwrap(),
                value,
            );
        }
        Ok(())
    }
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = std::process::Command::new("node")
            .env(
                "SAFE_CEX_TYPESCRIPT_PATH",
                std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                    // CI supplies an isolated compiler; this fallback only
                    // serves a local run from the repo root.
                    concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
                }),
            )
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/runtime_assembly_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn run(group: &str, count: usize) {
    let rows = oracle()[group].as_array().unwrap();
    assert_eq!(rows.len(), count, "independent {group} case floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let label = format!("{group}/{name}");
        let h = Host::new(group, name);
        let mut future: Option<JsPromise<N>> = None;
        let result = match group {
            "constructor" => {
                let store = h.ex.get("store");
                let opts = N::from(json!({"key":if name=="second"{"second"}else{"key"}}));
                new_exchange(h.as_ref(), N::S("mexc".into()), opts, store.clone()).map(|v| {
                    assert!(v.same(&h.ex));
                    assert!(v.get("store").same(&store));
                    assert_eq!(
                        EXCHANGE_METHOD_FIELDS.len(),
                        71,
                        "independent native constructor method-field floor"
                    );
                    let unique: std::collections::HashSet<_> =
                        EXCHANGE_METHOD_FIELDS.iter().collect();
                    assert_eq!(unique.len(), 71, "independent native distinct member floor");
                    for key in EXCHANGE_METHOD_FIELDS {
                        assert!(matches!(v.get(key), N::F(_)), "native bound method {key}");
                    }
                    let keys = [
                        "symbolSettings",
                        "recentlyCanceledStopLikeOrderIds",
                        "recentlyRejectedRegularOrderIds",
                        "recentlyPlacedStopLikeOrderIds",
                        "pendingStopSnapshotRefreshTimers",
                    ];
                    let mut maps = vec![];
                    for key in keys {
                        let map = v.get(key);
                        let N::M(values) = &map else {
                            panic!("native owned map {key}")
                        };
                        assert_eq!(values.borrow().len(), 0, "native initial empty map");
                        for original in &maps {
                            assert!(
                                !map.same(original),
                                "native independent field map ownership"
                            );
                        }
                        maps.push(map);
                    }
                    assert!(matches!(v.get("regularOrderSubmitQueue"), N::P(_)));
                    N::obj(vec![
                        ("members", {
                            let N::O(fields) = &v else { unreachable!() };
                            N::array(
                                fields
                                    .borrow()
                                    .iter()
                                    .filter(|(_, v)| matches!(v, N::F(_)))
                                    .map(|(k, _)| N::S(k.clone()))
                                    .collect(),
                            )
                        }),
                        ("mode", v.get("accountPositionMode")),
                        ("last", v.get("lastRegularOrderSubmitAt")),
                        ("cooldown", v.get("regularOrderSubmitCooldownUntil")),
                        ("available", v.get("nativeBatchOrdersAvailable")),
                        ("public", v.get("publicWebsocket").get("marker")),
                        ("private", v.get("privateWebsocket").get("marker")),
                        ("every", v.get("grafanaEndpointsEvery")),
                        ("periodic", v.get("grafanaEndpointsPeriodic")),
                    ])
                })
            }
            "start" => {
                future = Some(Box::pin(start_exchange(h.clone())));
                Ok(N::U)
            }
            "listenOHLCV" | "listenOrderBook" => {
                let cb = h.f(|_, _| Ok(N::U));
                *h.delegate_callback.borrow_mut() = Some(cb.clone());
                let r = if group == "listenOHLCV" {
                    listen_ohlcv(h.as_ref(), h.opts.clone(), cb)
                } else {
                    listen_order_book(h.as_ref(), N::S("BTC/USDT".into()), cb)
                };
                r.map(|v| {
                    assert!(
                        fn_same(&v, &h.leaf),
                        "native delegated unsubscribe identity"
                    );
                    N::from(json!({"callable":true}))
                })
            }
            "placePlanOrder" => {
                future = Some(Box::pin(place_plan_order(h.clone(), h.opts.clone())));
                Ok(N::U)
            }
            "placePositionStopOrder" => {
                future = Some(Box::pin(place_position_stop_order(
                    h.clone(),
                    h.opts.clone(),
                    h.market.clone(),
                    N::Num(19.24),
                )));
                Ok(N::U)
            }
            "updateOrder" => {
                future = Some(Box::pin(update_order(
                    h.clone(),
                    N::obj(vec![
                        ("order", h.order.clone()),
                        ("update", h.patch.clone()),
                    ]),
                )));
                Ok(N::U)
            }
            _ => unreachable!(),
        };
        let result = if let Some(f) = future {
            h.finish(f)
        } else {
            result
        };
        match result {
            Ok(v) => {
                assert!(row["error"].is_null(), "{label} unexpected native success");
                if group == "updateOrder" {
                    assert!(
                        v.same(&h.replacement_ids),
                        "native delegated replacement IDs identity"
                    );
                }
                assert_native(&v, &row["result"], &label)
            }
            Err(v) => {
                assert!(
                    !row["error"].is_null(),
                    "{label} unexpected native error {v:?}"
                );
                assert_native(&v, &row["error"], &label);
                assert_eq!(
                    v.same(&h.failure),
                    row["original"].as_bool().unwrap(),
                    "{label} native original error identity"
                );
            }
        }
        assert_native(
            &N::array(h.events.borrow().clone()),
            &row["events"],
            &format!("{label} native ordered effects"),
        );
        if group != "constructor" {
            assert_native(&h.ex.get("store").get("loaded"), &row["loaded"], &label);
            assert_native(&h.ex.get("accountPositionMode"), &row["mode"], &label);
        }
    }
}
#[test]
fn constructor_native_source() {
    run("constructor", 6)
}
#[test]
fn startup_native_source() {
    run("start", 12)
}
#[test]
fn candle_entry_native_source() {
    run("listenOHLCV", 3)
}
#[test]
fn book_entry_native_source() {
    run("listenOrderBook", 3)
}
#[test]
fn plan_native_source() {
    run("placePlanOrder", 14)
}
#[test]
fn position_stop_native_source() {
    run("placePositionStopOrder", 15)
}
#[test]
fn update_native_source() {
    run("updateOrder", 8)
}
