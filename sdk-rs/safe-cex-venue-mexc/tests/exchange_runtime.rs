#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_venue_mexc::{
    exchange_runtime::*,
    method_tasks::{Completion, Runtime, SessionRuntime, TaskHost},
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
    loops: RefCell<Vec<Rc<dyn Fn() -> JsResult<N>>>>,
    failure: N,
    balance: N,
    positions: N,
    config: N,
    original_configs: Vec<N>,
    market: N,
    posts: Cell<u32>,
    clears: Cell<u32>,
    appended: Cell<bool>,
    reverse_jobs: RefCell<Vec<(String, Rc<Job>, N)>>,
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
    fn new(group: &str, name: &str) -> Rc<Self> {
        let market = N::from(
            json!({"id":if name=="empty-id"{""}else{"BTC_USDT"},"limits":{"leverage":{"min":2,"max":50}}}),
        );
        let config = N::from(
            json!({"buyLeverage":if name=="zero-leverages"{0}else if name=="clamp"{99}else{7},"sellLeverage":if name=="zero-leverages"{0}else if name=="clamp"{1}else{9},"marginType":"cross","positionMode":"oneway"}),
        );
        if name == "nan" {
            config.set("buyLeverage", N::Num(f64::NAN));
        }
        let settings = Rc::new(RefCell::new(IndexMap::new()));
        if name != "derive" {
            settings
                .borrow_mut()
                .insert("BTC/USDT".into(), config.clone());
        }
        if group == "changePositionMode" {
            let eth = config.snapshot();
            eth.set("buyLeverage", N::Num(8.));
            settings.borrow_mut().insert("ETH/USDT".into(), eth);
        }
        let timers = Rc::new(RefCell::new(IndexMap::new()));
        if name != "empty" {
            timers.borrow_mut().insert("BTC".into(), N::Num(11.));
            timers.borrow_mut().insert("ETH".into(), N::Num(22.));
        }
        let ex = N::from(
            json!({"isDisposed":name=="disposed","isPublicDataOnly":name=="public","accountPositionMode":"oneway","store":{"loaded":{"markets":true,"balance":false,"positions":false,"keep":3},"orders":if name=="empty-orders"{json!([])}else{json!([{"id":"a","symbol":"BTC/USDT"},{"id":"b","symbol":"ETH/USDT"}])}}}),
        );
        ex.get("store").set(
            "marketsBySymbol",
            if matches!(name, "no-market" | "fallback") {
                N::obj(vec![])
            } else {
                N::obj(vec![("BTC/USDT", market.clone())])
            },
        );
        ex.set("symbolSettings", N::M(settings));
        ex.set("pendingStopSnapshotRefreshTimers", N::M(timers));
        if name == "no-match" {
            ex.get("store")
                .get("orders")
                .get("0")
                .set("symbol", N::S("ETH/USDT".into()));
        }
        let N::M(settings) = ex.get("symbolSettings") else {
            unreachable!()
        };
        let original_configs = std::iter::once(config.clone())
            .chain(settings.borrow().values().cloned())
            .collect();
        let h = Rc::new(Self {
            me: RefCell::new(std::rc::Weak::new()),
            name: name.into(),
            ex,
            events: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            loops: RefCell::new(vec![]),
            failure: N::from(
                json!({"marker":47,"message":"original failure","response":{"data":{"message":"venue failure"}}}),
            ),
            balance: N::from(json!({"total":101})),
            positions: N::from(json!([{"id":"position"}])),
            config,
            original_configs,
            market,
            posts: Cell::new(0),
            clears: Cell::new(0),
            appended: Cell::new(false),
            reverse_jobs: RefCell::new(vec![]),
        });
        *h.me.borrow_mut() = Rc::downgrade(&h);
        h.install();
        h
    }
    fn self_rc(&self) -> Rc<Self> {
        self.me.borrow().upgrade().unwrap()
    }
    fn f(&self, body: impl Fn(N, Vec<N>) -> JsResult<N> + 'static) -> N {
        N::F(Rc::new(body))
    }
    fn event(&self, tag: &str, args: Vec<N>) -> JsResult<N, ()> {
        let mut row = vec![N::S(tag.into())];
        row.extend(args.into_iter().map(|v| v.snapshot()));
        self.events.borrow_mut().push(N::array(row));
        if self.name == format!("{tag}-error") || self.name == "emit-error" && tag == "pair" {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
    fn ready(&self, result: JsResult<N>) -> Rc<Job> {
        Rc::new(Job {
            future: RefCell::new(None),
            result: RefCell::new(Some(result)),
        })
    }
    fn promise(&self, result: JsResult<N>) -> N {
        N::P(self.ready(result))
    }
    fn poll_task(task: &Task) {
        let Some(mut future) = task.borrow_mut().take() else {
            return;
        };
        let mut cx = Context::from_waker(Waker::noop());
        if future.as_mut().poll(&mut cx).is_pending() {
            *task.borrow_mut() = Some(future);
        }
    }
    fn finish(&self, mut future: Pin<Box<dyn Future<Output = JsResult<N>>>>) -> JsResult<N> {
        for _ in 0..128 {
            for op in self.ops.borrow().clone() {
                let task = op.future.borrow_mut().take();
                if let Some(mut task) = task {
                    let mut cx = Context::from_waker(Waker::noop());
                    match task.as_mut().poll(&mut cx) {
                        Poll::Ready(r) => *op.result.borrow_mut() = Some(r),
                        Poll::Pending => *op.future.borrow_mut() = Some(task),
                    }
                }
            }
            for task in self.tasks.borrow().clone() {
                Self::poll_task(&task);
            }
            let mut cx = Context::from_waker(Waker::noop());
            if let Poll::Ready(result) = future.as_mut().poll(&mut cx) {
                return result;
            }
        }
        panic!("exchange task unsettled at poll128")
    }
    fn method(&self, target: &N, key: &str, body: impl Fn(N, Vec<N>) -> JsResult<N> + 'static) {
        target.set(key, self.f(body));
    }
    fn install(&self) {
        for (tag, key) in [("balance", "fetchBalance"), ("positions", "fetchPositions")] {
            let h = self.self_rc();
            self.method(&self.ex, key, move |r, _| {
                assert!(r.same(&h.ex), "native original exchange receiver {key}");
                if h.name == format!("{tag}-sync-error") {
                    h.event(&format!("{tag}-sync"), vec![])?;
                    return Err(h.failure.clone());
                }
                let result = h.event(tag, vec![]).map(|_| {
                    if tag == "balance" {
                        h.balance.clone()
                    } else {
                        h.positions.clone()
                    }
                });
                if h.name == "positions-first-reject" && tag == "balance" {
                    return Ok(N::P(Rc::new(Job {
                        future: RefCell::new(None),
                        result: RefCell::new(None),
                    })));
                }
                if h.name == "positions-first-reject" && tag == "positions" {
                    return Ok(h.promise(Err(h.failure.clone())));
                }
                if h.name == "reverse-completion" {
                    let job = Rc::new(Job {
                        future: RefCell::new(None),
                        result: RefCell::new(None),
                    });
                    h.reverse_jobs
                        .borrow_mut()
                        .push((tag.into(), job.clone(), result.unwrap()));
                    return Ok(N::P(job));
                }
                Ok(h.promise(result))
            });
        }
        let h = self.self_rc();
        self.method(&self.ex, "fetchBalanceAndPositions", move |r, _| {
            assert!(r.same(&h.ex));
            let result = h.event("pair", vec![]).and_then(|_| {
                if matches!(h.name.as_str(), "disposed-balance" | "disposed-failure") {
                    h.ex.set("isDisposed", N::B(true));
                }
                if h.name == "disposed-failure" {
                    Err(h.failure.clone())
                } else {
                    Ok(N::obj(vec![
                        ("balance", h.balance.clone()),
                        ("positions", h.positions.clone()),
                    ]))
                }
            });
            Ok(h.promise(result))
        });
        let h = self.self_rc();
        self.method(&self.ex, "fetchAndSetRegularOrders", move |r, _| {
            assert!(r.same(&h.ex));
            let result = h.event("orders", vec![]).map(|_| {
                if h.name == "disposed-orders" {
                    h.ex.set("isDisposed", N::B(true));
                }
                N::U
            });
            Ok(h.promise(result))
        });
        for (tag, key) in [
            ("emit", "emitAndLogError"),
            ("interval", "getTickInterval"),
            ("tick-callback", "tick"),
        ] {
            let h = self.self_rc();
            self.method(&self.ex, key, move |r, a| {
                assert!(r.same(&h.ex));
                h.event(tag, a)?;
                Ok(if tag == "interval" {
                    N::Num(2000.)
                } else {
                    N::U
                })
            });
        }
        let store = self.ex.get("store");
        let h = self.self_rc();
        let identity = store.clone();
        self.method(&store, "addOrUpdatePositionsFull", move |r, a| {
            assert!(r.same(&identity));
            assert!(a[0].same(&h.positions), "native original positions alias");
            h.event("full", a)?;
            Ok(N::U)
        });
        let h = self.self_rc();
        let identity = store.clone();
        self.method(&store, "update", move |r, a| {
            assert!(r.same(&identity));
            assert!(
                a[0].get("balance").same(&h.balance),
                "native original balance alias"
            );
            h.event("update", a.clone())?;
            identity.set("loaded", a[0].get("loaded"));
            Ok(N::U)
        });
        for (tag, key) in [("update", "updateOrdersStatus"), ("setting", "setSetting")] {
            let h = self.self_rc();
            let identity = store.clone();
            self.method(&store, key, move |r, a| {
                assert!(r.same(&identity));
                h.event(tag, a)?;
                Ok(N::U)
            });
        }
        let h = self.self_rc();
        self.method(&self.ex, "deriveSymbolConfig", move |r, a| {
            assert!(r.same(&h.ex));
            h.event("derive", a)?;
            Ok(h.config.clone())
        });
        let h = self.self_rc();
        self.method(&self.ex, "formatChangeLeveragePayload", move |r, a| {
            assert!(r.same(&h.ex));
            assert!(a[1].same(&h.market), "native original market alias");
            let extra = a.get(5).cloned().unwrap_or(N::U);
            h.event(
                "format",
                vec![
                    a[0].clone(),
                    a[2].clone(),
                    a[3].clone(),
                    a[4].clone(),
                    extra.clone(),
                ],
            )?;
            let payload = N::obj(vec![
                ("symbol", h.market.get("id")),
                ("ty", a[2].clone()),
                ("leverage", a[3].clone()),
                ("margin", a[4].clone()),
            ]);
            if h.truthy(&extra) {
                payload.set("extra", extra);
            }
            Ok(payload)
        });
        let h = self.self_rc();
        self.method(&self.ex, "assertCommonSuccess", move |r, a| {
            assert!(r.same(&h.ex));
            h.event("assert", a)?;
            Ok(N::U)
        });
        let h = self.self_rc();
        self.method(&self.ex, "updateFetchedLeverage", move |r, a| {
            assert!(r.same(&h.ex));
            let N::M(map) = h.ex.get("symbolSettings") else {
                panic!("native symbol map")
            };
            assert!(
                map.borrow().get(&a[0].text()).unwrap().same(&a[1]),
                "native stored config alias"
            );
            assert!(
                !h.original_configs.iter().any(|old| old.same(&a[1])),
                "native fresh next config identity"
            );
            h.event("update-config", a)?;
            if h.name == "live-append" && !h.appended.replace(true) {
                let config = h.config.snapshot();
                config.set("buyLeverage", N::Num(4.));
                map.borrow_mut().insert("SOL/USDT".into(), config);
            }
            Ok(N::U)
        });
        let xhr = N::obj(vec![]);
        let h = self.self_rc();
        let identity = xhr.clone();
        self.method(&xhr, "post", move |r, a| {
            assert!(r.same(&identity), "native captured post receiver");
            let n = h.posts.get() + 1;
            h.posts.set(n);
            let result = h.event("post", a).and_then(|_| {
                if h.name == "second-post-error" && n == 2 {
                    return Err(h.failure.clone());
                }
                if h.name == "mutate-orders" {
                    let N::A(orders) = h.ex.get("store").get("orders") else {
                        panic!("orders")
                    };
                    orders
                        .borrow_mut()
                        .push(N::from(json!({"id":"c","symbol":"BTC/USDT"})));
                }
                Ok(N::from(json!({"data":{"success":false,"marker":n}})))
            });
            Ok(h.promise(result))
        });
        self.ex.set("xhr", xhr);
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
                "clear" => {
                    map.borrow_mut().clear();
                    Ok(N::U)
                }
                "values" => Ok(N::Iter(map.clone())),
                _ => Ok(N::U),
            }));
        }
        if let N::A(values) = v {
            if key == "map" || key == "filter" {
                let map = key == "map";
                let h = self.self_rc();
                return Ok(self.f(move |r, a| {
                    let N::A(values) = r else {
                        panic!("array receiver")
                    };
                    let len = values.borrow().len();
                    let mut out = vec![];
                    for i in 0..len {
                        let value = values.borrow().get(i).cloned().unwrap_or(N::U);
                        let result = h.call(a[0].clone(), N::U, vec![value.clone()])?;
                        if map {
                            out.push(result);
                        } else if h.truthy(&result) {
                            out.push(value);
                        }
                    }
                    Ok(N::array(out))
                }));
            }
            if key == "length" {
                return Ok(N::Num(values.borrow().len() as f64));
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
    fn schedule_loop(&self, callback: Rc<dyn Fn() -> JsResult<N>>, delay: N) -> JsResult<N, ()> {
        self.event("loop", vec![delay])?;
        self.loops.borrow_mut().push(callback);
        Ok(())
    }
    fn clear_timeout(&self, timer: N) -> JsResult<N, ()> {
        let n = self.clears.get() + 1;
        self.clears.set(n);
        self.event("clear", vec![timer])?;
        if self.name == "second-clear-error" && n == 2 {
            return Err(self.failure.clone());
        }
        if self.name == "append-timer" && n == 1 {
            let N::M(map) = self.ex.get("pendingStopSnapshotRefreshTimers") else {
                panic!("timer map")
            };
            map.borrow_mut().insert("SOL".into(), N::Num(33.));
        }
        if self.name == "replace-map" && n == 1 {
            let map = Rc::new(RefCell::new(IndexMap::new()));
            map.borrow_mut().insert("NEW".into(), N::Num(99.));
            self.ex.set("pendingStopSnapshotRefreshTimers", N::M(map));
        }
        Ok(())
    }
    fn base_dispose(&self) -> JsResult<N, ()> {
        self.event("base", vec![])?;
        self.ex.set("isDisposed", N::B(true));
        Ok(())
    }
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = std::process::Command::new("node")
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
                "/tests/exchange_runtime_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn from_tag(value: &Value) -> N {
    if value.get("undefined") == Some(&Value::Bool(true)) {
        return N::U;
    }
    if value.get("number") == Some(&json!("NaN")) {
        return N::Num(f64::NAN);
    }
    match value {
        Value::Array(v) => N::array(v.iter().map(from_tag).collect()),
        Value::Object(v) => N::object(v.iter().map(|(k, v)| (k.clone(), from_tag(v))).collect()),
        _ => N::from(value.clone()),
    }
}
fn run_group(group: &str, count: usize) {
    let rows = oracle()[group].as_array().unwrap();
    assert_eq!(rows.len(), count, "independent {group} case floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let h = Host::new(group, name);
        let label = format!("{group}/{name}");
        let args = from_tag(&row["args"]);
        let mut future = None;
        let immediate = match group {
            "fetchBalanceAndPositions" => {
                future = Some(Box::pin(fetch_balance_and_positions(h.clone()))
                    as Pin<Box<dyn Future<Output = JsResult<N>>>>);
                Ok(N::U)
            }
            "tick" => {
                future = Some(Box::pin(tick(h.clone())));
                Ok(N::U)
            }
            "cancelSymbolOrders" => {
                future = Some(Box::pin(cancel_symbol_orders(h.clone(), args.get("0"))));
                Ok(N::U)
            }
            "cancelAllOrders" => {
                future = Some(Box::pin(cancel_all_orders(h.clone())));
                Ok(N::U)
            }
            "setLeverage" => {
                future = Some(Box::pin(set_leverage(
                    h.clone(),
                    args.get("0"),
                    args.get("1"),
                    args.get("2"),
                )));
                Ok(N::U)
            }
            "changePositionMode" => {
                future = Some(Box::pin(change_position_mode(h.clone(), args.get("0"))));
                Ok(N::U)
            }
            "changeMarginType" => {
                future = Some(Box::pin(change_margin_type(
                    h.clone(),
                    args.get("0"),
                    args.get("1"),
                )));
                Ok(N::U)
            }
            _ => dispose(h.as_ref()).map(|()| N::U),
        };
        let initial = N::array(h.events.borrow().clone());
        if name == "reverse-completion" {
            let jobs = h.reverse_jobs.borrow();
            assert_eq!(jobs.len(), 2, "native both leaves start before awaiting");
            for index in [1, 0] {
                let (tag, job, value) = &jobs[index];
                h.event(&format!("{tag}-done"), vec![]).unwrap();
                *job.result.borrow_mut() = Some(Ok(value.clone()));
            }
        }
        let result = if let Some(future) = future {
            h.finish(future)
        } else {
            immediate
        };
        if name == "callback-live" {
            let host = h.clone();
            h.ex.set(
                "tick",
                h.f(move |_, _| {
                    host.event("replacement-tick", vec![])?;
                    Ok(N::U)
                }),
            );
            for callback in h.loops.borrow().clone() {
                callback().unwrap();
            }
        }
        match result {
            Ok(value) => {
                assert!(row["error"].is_null(), "{label} unexpected native success");
                assert_native(&value, &row["result"], &label);
                if group == "fetchBalanceAndPositions" {
                    assert!(
                        value.get("balance").same(&h.balance),
                        "native returned balance identity"
                    );
                    assert!(
                        value.get("positions").same(&h.positions),
                        "native returned positions identity"
                    );
                }
            }
            Err(value) => {
                assert!(
                    !row["error"].is_null(),
                    "{label} unexpected native error {}",
                    value.text()
                );
                assert_native(&value, &row["error"], &label);
                assert_eq!(
                    value.same(&h.failure),
                    row["original"].as_bool().unwrap(),
                    "{label} native original error identity"
                );
            }
        }
        assert_native(
            &N::array(h.original_configs.clone()),
            &row["originalConfigs"],
            &format!("{label} native retained original configs"),
        );
        assert_native(&h.ex.get("accountPositionMode"), &row["mode"], &label);
        assert_native(&h.ex.get("symbolSettings"), &row["settings"], &label);
        assert_native(&h.ex.get("store").get("loaded"), &row["loaded"], &label);
        assert_native(
            &h.ex.get("pendingStopSnapshotRefreshTimers"),
            &row["timers"],
            &label,
        );
        assert_native(&h.ex.get("isDisposed"), &row["disposed"], &label);
        assert_native(&initial, &row["initial"], &format!("{label} eager effects"));
        assert_native(
            &N::array(h.events.borrow().clone()),
            &row["events"],
            &format!("{label} ordered effects"),
        );
    }
}
#[test]
fn balance_pair_native_source() {
    run_group("fetchBalanceAndPositions", 7)
}
#[test]
fn tick_native_source() {
    run_group("tick", 14)
}
#[test]
fn cancel_symbol_native_source() {
    run_group("cancelSymbolOrders", 8)
}
#[test]
fn cancel_all_native_source() {
    run_group("cancelAllOrders", 5)
}
#[test]
fn leverage_native_source() {
    run_group("setLeverage", 12)
}
#[test]
fn position_mode_native_source() {
    run_group("changePositionMode", 10)
}
#[test]
fn margin_type_native_source() {
    run_group("changeMarginType", 12)
}
#[test]
fn dispose_native_source() {
    run_group("dispose", 7)
}
