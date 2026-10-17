#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_venue_mexc::{
    exchange_runtime::*,
    method_tasks::{Completion, MethodObserver, Runtime, SessionRuntime, TaskHost},
    order_workflow::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
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
    name: String,
    ex: N,
    events: RefCell<Vec<N>>,
    tasks: RefCell<Vec<Task>>,
    ops: RefCell<Vec<Rc<Job>>>,
    timers: RefCell<Vec<Rc<dyn Fn() -> JsResult<N>>>>,
    failure: N,
    now: Cell<f64>,
    calls: Cell<usize>,
    opts: N,
    orders: N,
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
impl Host {
    fn self_rc(&self) -> Rc<Self> {
        self.me.borrow().upgrade().unwrap()
    }
    fn f(&self, body: impl Fn(N, Vec<N>) -> JsResult<N> + 'static) -> N {
        N::F(Rc::new(body))
    }
    fn event(&self, k: &str, a: Vec<N>) -> JsResult<N, ()> {
        let mut row = vec![N::S(k.into())];
        row.extend(a.into_iter().map(|v| v.snapshot()));
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
    fn finish(&self, mut f: JsPromise<N>) -> JsResult<N> {
        for _ in 0..256 {
            let jobs = self.ops.borrow().clone();
            for j in jobs {
                let f = j.future.borrow_mut().take();
                if let Some(mut f) = f {
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
        let market = N::from(
            json!({"id":"BTC_USDT","info":{"volUnit":if name=="zero-step"{0}else{2}},"precision":{"price":0.5},"limits":{"leverage":{"min":2,"max":50}},"contractSize":if name=="zero-size"{0.}else{0.01}}),
        );
        let opts = N::from(
            json!({"symbol":"BTC/USDT","type":"limit","side":"buy","amount":if name=="minimum"{0.1}else if name=="negative"{-4.}else if name=="zero"{0.}else{5.},"price":12.26}),
        );
        for (case, ty) in [
            ("market", "market"),
            ("plan", "stop_market"),
            ("take-profit", "take_profit_market"),
            ("trailing", "trailing_stop_market"),
        ] {
            if name == case {
                opts.set("type", N::S(ty.into()));
            }
        }
        if name == "post" {
            opts.set("timeInForce", N::S("PostOnly".into()));
        }
        if name == "unknown-tif" {
            opts.set("timeInForce", N::S("unknown".into()));
        }
        if name == "reduce" {
            opts.set("reduceOnly", N::B(true));
            opts.set("side", N::S("sell".into()));
        }
        let second = opts.snapshot();
        second.set("side", N::S("sell".into()));
        if name == "mixed" {
            second.set("type", N::S("stop_market".into()));
        }
        let orders = N::array(if name == "empty" {
            vec![]
        } else {
            vec![opts.clone(), second]
        });
        let ex = N::from(
            json!({"isDisposed":name=="disposed","lastRegularOrderSubmitAt":if name=="string-last"{json!("700")}else{json!(if name=="ready"{0}else{700})},"regularOrderSubmitCooldownUntil":if name=="cooldown"{2000}else{0},"store":{"orders":if matches!(name,"canceled"|"closed"){json!([{"id":"id1","status":name}])}else{json!([])}}}),
        );
        ex.get("store").set(
            "marketsBySymbol",
            if name == "no-market" {
                N::obj(vec![])
            } else {
                N::obj(vec![("BTC/USDT", market)])
            },
        );
        ex.set(
            "nativeBatchOrdersAvailable",
            if name == "disabled" {
                N::B(false)
            } else {
                N::U
            },
        );
        let timers = Rc::new(RefCell::new(IndexMap::new()));
        if name != "missing" {
            timers
                .borrow_mut()
                .insert("BTC/USDT".into(), N::from(json!({"id":11})));
        }
        ex.set("pendingStopSnapshotRefreshTimers", N::M(timers));
        let settings = Rc::new(RefCell::new(IndexMap::new()));
        settings
            .borrow_mut()
            .insert("BTC/USDT".into(), N::from(json!({"marginType":"cross"})));
        ex.set("symbolSettings", N::M(settings));
        let h = Rc::new(Self {
            me: RefCell::new(std::rc::Weak::new()),
            name: name.into(),
            ex,
            events: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
            failure: N::from(
                json!({"message":"original","response":{"data":{"message":"venue","msg":"alternate"}}}),
            ),
            now: Cell::new(1000.),
            calls: Cell::new(0),
            opts,
            orders,
        });
        *h.me.borrow_mut() = Rc::downgrade(&h);
        h.ex.set(
            "regularOrderSubmitQueue",
            h.promise(if name == "predecessor-error" {
                Err(h.failure.clone())
            } else {
                Ok(N::U)
            }),
        );
        h.install(group);
        h
    }
    fn install(&self, group: &str) {
        for (key, tag) in [
            ("emitAndLogError", "emit"),
            ("log", "log"),
            ("getLeverageForOrder", "leverage"),
            ("isRecentlyRejectedRegularOrder", "rejected"),
            ("notePlacedStopLikeOrder", "note"),
            ("waitForRegularOrderSubmitSlot", "slot"),
            ("noteRegularOrderSubmitError", "note"),
            ("resolveCallerClientOrderId", "caller"),
            ("assertCallerClientOrderIds", "caller"),
            ("formatRegularOrderPayload", "format"),
            ("enqueueRegularOrderSubmit", "enqueue"),
            ("assertRegularOrderSubmitSucceeded", "assert"),
            ("assertRegularOrderBatchSucceeded", "assert"),
            ("addOptimisticRegularOrder", "optimistic"),
            ("placePlanOrder", "plan"),
            ("isNativeBatchAccessDeniedError", "denied"),
            ("placeRegularOrdersBatch", "batch"),
            ("placeOrdersSequentially", "sequential"),
            ("placeOrder", "place"),
            ("fetchAndSetOrders", "refresh"),
        ] {
            if key == group {
                continue;
            }
            let h = self.self_rc();
            let key = key.to_owned();
            let tag = tag.to_owned();
            self.method(&self.ex, &key.clone(), move |r, a| {
                assert!(r.same(&h.ex), "native original exchange receiver {key}");
                if matches!(
                    key.as_str(),
                    "getLeverageForOrder" | "resolveCallerClientOrderId" | "placePlanOrder"
                ) {
                    assert!(a[0].same(&h.opts), "native original option alias");
                }
                if matches!(
                    key.as_str(),
                    "assertCallerClientOrderIds"
                        | "placeRegularOrdersBatch"
                        | "placeOrdersSequentially"
                ) {
                    assert!(a[0].same(&h.orders), "native original orders alias");
                }
                let args = if key == "getLeverageForOrder"
                    || key == "resolveCallerClientOrderId"
                    || key == "assertCallerClientOrderIds"
                    || key == "placePlanOrder"
                    || key == "placeRegularOrdersBatch"
                    || key == "enqueueRegularOrderSubmit"
                    || key == "isNativeBatchAccessDeniedError"
                {
                    vec![]
                } else {
                    a.clone()
                };
                if key == "getLeverageForOrder" && h.name == "format-error" {
                    h.event("format", vec![])?;
                    return Err(h.failure.clone());
                }
                let effect = h.event(&tag, args);
                match key.as_str() {
                    "getLeverageForOrder" => {
                        effect?;
                        if h.name == "format-error" {
                            h.event("format", vec![])?;
                            return Err(h.failure.clone());
                        }
                        Ok(N::Num(if h.name == "clamp" { 99. } else { 7. }))
                    }
                    "isRecentlyRejectedRegularOrder" => {
                        effect?;
                        Ok(N::B(h.name == "rejected"))
                    }
                    "formatRegularOrderPayload" => {
                        effect?;
                        Ok(N::obj(vec![
                            ("symbol", N::S("BTC_USDT".into())),
                            ("vol", a[0].get("amount")),
                            ("stpMode", N::Num(9.)),
                        ]))
                    }
                    "waitForRegularOrderSubmitSlot" | "fetchAndSetOrders" => Ok(h.promise(
                        if key == "fetchAndSetOrders" && h.name == "logger-getter" {
                            Err(h.failure.clone())
                        } else {
                            effect.map(|_| N::U)
                        },
                    )),
                    "enqueueRegularOrderSubmit" => {
                        effect?;
                        let value = h.call(a[0].clone(), N::U, vec![])?;
                        Ok(value)
                    }
                    "placePlanOrder" => Ok(h.promise(effect.map(|_| N::S("plan-id".into())))),
                    "isNativeBatchAccessDeniedError" => {
                        effect?;
                        assert!(a[0].same(&h.failure));
                        Ok(N::B(h.name == "denied"))
                    }
                    "placeRegularOrdersBatch" => Ok(h.promise(
                        if matches!(h.name.as_str(), "denied" | "batch-error") {
                            Err(h.failure.clone())
                        } else {
                            effect.map(|_| N::from(json!(["b1", "b2"])))
                        },
                    )),
                    "placeOrdersSequentially" => {
                        Ok(h.promise(effect.map(|_| N::from(json!(["s1", "s2"])))))
                    }
                    "placeOrder" => {
                        let N::A(orders) = &h.orders else {
                            unreachable!()
                        };
                        assert!(
                            orders.borrow().iter().any(|order| order.same(&a[0])),
                            "native original/live sequential option identity"
                        );
                        let n = h.calls.get() + 1;
                        h.calls.set(n);
                        effect?;
                        if h.name == "append" && n == 1 {
                            let extra = h.opts.snapshot();
                            extra.set("side", N::S("sell".into()));
                            extra.set("amount", N::Num(3.));
                            let N::A(values) = &h.orders else {
                                unreachable!()
                            };
                            values.borrow_mut().push(extra);
                        }
                        Ok(h.promise(
                            if h.name == "first-error" && n == 1
                                || matches!(h.name.as_str(), "second-error" | "string-error")
                                    && n == 2
                            {
                                Err(if h.name == "string-error" {
                                    N::S("primitive".into())
                                } else {
                                    h.failure.clone()
                                })
                            } else {
                                Ok(N::array(vec![N::S(format!("id{n}"))]))
                            },
                        ))
                    }
                    _ => {
                        effect?;
                        Ok(N::U)
                    }
                }
            });
        }
        self.method(&self.ex, "isRegularPlaceOrder", move |r, a| {
            assert!(matches!(r, N::U), "native every arrow receiver");
            Ok(N::B(!matches!(
                a[0].get("type").text().as_str(),
                "stop_market" | "take_profit_market" | "trailing_stop_market"
            )))
        });
        let store = self.ex.get("store");
        for (key, tag) in [("addOrUpdateOrder", "add"), ("updateOrder", "update")] {
            let h = self.self_rc();
            let original = store.clone();
            self.method(&store, key, move |r, a| {
                assert!(r.same(&original));
                h.event(tag, a)?;
                Ok(N::U)
            });
        }
        let xhr = N::obj(vec![]);
        let h = self.self_rc();
        let original = xhr.clone();
        let batch = group == "placeRegularOrdersBatch";
        self.method(&xhr, "post", move |r, a| {
            assert!(r.same(&original), "native original post receiver");
            let effect = h.event("post", a);
            let result = effect.and_then(|_| {
                if h.name == "emit-error" {
                    Err(h.failure.clone())
                } else {
                    let inner = if batch {
                        if h.name == "no-id" {
                            json!([])
                        } else {
                            json!([{"orderId":"b1"},{"id":"b2"}])
                        }
                    } else {
                        json!({"orderId":if h.name=="no-id"{""}else{"single-id"}})
                    };
                    Ok(N::from(json!({"data":{"data":inner}})))
                }
            });
            Ok(h.promise(result))
        });
        self.ex.set("xhr", xhr);
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
        if self.name == "logger-getter"
            && key == "message"
            && v.same(&self.failure.get("response").get("data"))
        {
            self.event("error-getter", vec![])?;
            let h = self.self_rc();
            self.method(&self.ex, "log", move |_, a| {
                h.event("replacement-log", a)?;
                Ok(N::U)
            });
        }
        if self.name == "empty-callback" && key == "forEach" && v.same(&self.orders) {
            let h = self.self_rc();
            return Ok(self.f(move |_, a| {
                h.event("custom-foreach", vec![])?;
                h.call(a[0].clone(), N::U, vec![])
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
                            return if catch || cb.len() > 1 {
                                Err(value)
                            } else {
                                Ok(value)
                            };
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
    fn stringify(&self, _: &N) -> JsResult<N, Vec<u16>> {
        panic!("unused wire boundary")
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
        self.field(v, &key.text())
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
            "now" => {
                self.event("now", vec![])?;
                Ok(N::Num(self.now.get()))
            }
            "sleep" => {
                let delay = a[0].clone();
                let result = self.event("sleep", vec![delay.clone()]);
                if result.is_ok() {
                    self.now.set(self.now.get() + Self::number_value(delay));
                }
                Ok(self.promise(result.map(|_| N::U)))
            }
            "uuid" => Ok(N::S("fixture-uuid".into())),
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
            "nativeVolumeToAmount" => {
                let vol = self.import("toNumber", vec![a[0].clone()])?;
                let size = self.import("toNumber", vec![a[1].get("contractSize"), N::Num(1.)])?;
                let size = Self::number_value(size);
                Ok(N::Num(
                    Self::number_value(vol) * if size > 0. { size } else { 1. },
                ))
            }
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
            "extractRows" => {
                let data = self.import("extractData", a)?;
                Ok(if matches!(data, N::A(_)) {
                    data
                } else {
                    N::array(vec![])
                })
            }
            _ => panic!("unbound import {name}"),
        }
    }
    fn constant(&self, name: &str) -> N {
        use safe_cex_venue_mexc::types::*;
        match name {
            "REVERSE_TIME_IN_FORCE" => N::from((*REVERSE_TIME_IN_FORCE).clone()),
            "REVERSE_ORDER_TYPE" => N::from((*REVERSE_ORDER_TYPE).clone()),
            "REVERSE_ORDER_SIDE" => N::from((*REVERSE_ORDER_SIDE).clone()),
            "REVERSE_MARGIN_TYPE" => N::from((*REVERSE_MARGIN_TYPE).clone()),
            "MEXC_REGULAR_ORDER_SUBMIT_GAP_MS" => N::Num(450.),
            "MEXC_SCALED_ORDER_SUBMIT_GAP_MS" | "MEXC_STOP_SNAPSHOT_REFRESH_DELAY_MS" => {
                N::Num(1500.)
            }
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
    fn set_timeout(&self, cb: Rc<dyn Fn() -> JsResult<N>>, delay: N) -> JsResult<N> {
        self.event("timer", vec![delay])?;
        self.timers.borrow_mut().push(cb);
        let timer = N::from(json!({"id":99}));
        let original = timer.clone();
        let h = self.self_rc();
        self.method(&timer, "unref", move |r, _| {
            assert!(r.same(&original), "native unref receiver identity");
            h.event("unref", vec![])?;
            Ok(N::U)
        });
        Ok(timer)
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
                "/tests/order_workflow_oracle.cjs"
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
fn from_tag(v: &Value) -> N {
    if v.get("undefined") == Some(&json!(true)) {
        return N::U;
    }
    if v.get("number") == Some(&json!("NaN")) {
        return N::Num(f64::NAN);
    }
    match v {
        Value::Array(v) => N::array(v.iter().map(from_tag).collect()),
        Value::Object(v) => N::object(v.iter().map(|(k, v)| (k.clone(), from_tag(v))).collect()),
        _ => N::from(v.clone()),
    }
}
fn run(group: &str, count: usize) {
    let rows = oracle()[group].as_array().unwrap();
    assert_eq!(rows.len(), count, "independent {group} source floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let label = format!("{group}/{name}");
        let h = Host::new(group, name);
        let args = from_tag(&row["args"]);
        let mut future: Option<JsPromise<N>> = None;
        let immediate = match group {
            "formatRegularOrderPayload" => format_regular_order_payload(h.clone(), h.opts.clone()),
            "addOptimisticRegularOrder" => {
                add_optimistic_regular_order(h.clone(), h.opts.clone(), N::S("id1".into()))
            }
            "addOptimisticStopOrder" => add_optimistic_stop_order(
                h.clone(),
                h.opts.clone(),
                h.ex.get("store").get("marketsBySymbol").get("BTC/USDT"),
                N::S("id1".into()),
                N::Num(19.24),
                N::S(
                    if name == "plan" {
                        "__mexcPlanOrder"
                    } else {
                        "__mexcStopOrder"
                    }
                    .into(),
                ),
            ),
            "scheduleStopOrderSnapshotRefresh" => {
                schedule_stop_order_snapshot_refresh(h.clone(), N::S("BTC/USDT".into()))
            }
            "waitForRegularOrderSubmitSlot" => {
                future = Some(Box::pin(wait_for_regular_order_submit_slot(h.clone())));
                Ok(N::U)
            }
            "enqueueRegularOrderSubmit" => {
                let cb = h.clone();
                let task = h.f(move |r, _| {
                    assert!(matches!(r, N::U));
                    cb.event("task", vec![])?;
                    if cb.name == "task-sync-error" {
                        return Err(cb.failure.clone());
                    }
                    Ok(
                        cb.promise(if matches!(cb.name.as_str(), "task-error" | "note-error") {
                            Err(cb.failure.clone())
                        } else {
                            Ok(N::from(json!({"ok":4})))
                        }),
                    )
                });
                if name == "double" {
                    let create = |tag: &'static str, n: f64| {
                        let cb = h.clone();
                        h.f(move |r, _| {
                            assert!(matches!(r, N::U));
                            cb.event("task", vec![N::S(tag.into())])?;
                            Ok(cb.promise(Ok(N::from(json!({"ok":n})))))
                        })
                    };
                    let first = enqueue_regular_order_submit(h.clone(), create("first", 4.));
                    let second = enqueue_regular_order_submit(h.clone(), create("second", 5.));
                    let first = h.promise_value(first);
                    let second = h.promise_value(second);
                    let job = h.promise_all(vec![first, second]).unwrap();
                    future = Some(Box::pin(Handle(job)));
                } else {
                    future = Some(Box::pin(enqueue_regular_order_submit(h.clone(), task)));
                }
                Ok(N::U)
            }
            "placeOrder" => {
                future = Some(Box::pin(place_order(h.clone(), h.opts.clone())));
                Ok(N::U)
            }
            "placeOrders" => {
                future = Some(Box::pin(place_orders(h.clone(), h.orders.clone())));
                Ok(N::U)
            }
            "placeOrdersSequentially" => {
                future = Some(Box::pin(place_orders_sequentially(
                    h.clone(),
                    h.orders.clone(),
                    args.get("1"),
                )));
                Ok(N::U)
            }
            "placeRegularOrdersBatch" => {
                future = Some(Box::pin(place_regular_orders_batch(
                    h.clone(),
                    h.orders.clone(),
                )));
                Ok(N::U)
            }
            _ => unreachable!(),
        };
        let result = if let Some(f) = future {
            h.finish(f)
        } else {
            immediate
        };
        let result = if group == "scheduleStopOrderSnapshotRefresh" && result.is_ok() {
            let mut result = result;
            for cb in h.timers.borrow().clone() {
                if let Err(e) = cb() {
                    result = Err(e);
                    break;
                }
            }
            h.finish(Box::pin(async { Ok(N::U) })).unwrap();
            result
        } else {
            result
        };
        match result {
            Ok(v) => {
                assert!(row["error"].is_null(), "{label} unexpected native success");
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
        assert_native(&h.ex.get("lastRegularOrderSubmitAt"), &row["last"], &label);
        assert_native(
            &h.ex.get("nativeBatchOrdersAvailable"),
            &row["available"],
            &label,
        );
        assert_native(
            &h.ex.get("pendingStopSnapshotRefreshTimers"),
            &row["timers"],
            &label,
        );
    }
}
#[test]
fn payload_native_source() {
    run("formatRegularOrderPayload", 10)
}
#[test]
fn slot_native_source() {
    run("waitForRegularOrderSubmitSlot", 5)
}
#[test]
fn queue_native_source() {
    run("enqueueRegularOrderSubmit", 7)
}
#[test]
fn optimistic_regular_native_source() {
    run("addOptimisticRegularOrder", 8)
}
#[test]
fn optimistic_stop_native_source() {
    run("addOptimisticStopOrder", 6)
}
#[test]
fn refresh_native_source() {
    run("scheduleStopOrderSnapshotRefresh", 7)
}
#[test]
fn single_native_source() {
    run("placeOrder", 10)
}
#[test]
fn routing_native_source() {
    run("placeOrders", 8)
}
#[test]
fn sequential_native_source() {
    run("placeOrdersSequentially", 7)
}
#[test]
fn batch_native_source() {
    run("placeRegularOrdersBatch", 8)
}
