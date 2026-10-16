#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_venue_grvt::{
    account_runtime::*,
    builder_workflow::{JsPromise, JsResult},
    composition::*,
    private_commands::PrivateCommands,
    public_subscriptions::PublicSubscriptions,
    session_host::*,
    signing_runtime::*,
    trading_runtime::*,
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
include!("support/assembly_values.rs");
struct Host {
    me: RefCell<std::rc::Weak<Host>>,
    group: String,
    name: String,
    ex: N,
    opts: N,
    pages: N,
    response: N,
    events: RefCell<Vec<N>>,
    jobs: RefCell<Vec<Rc<Job>>>,
    failure: N,
    builder_failure: RefCell<N>,
    post_count: Cell<usize>,
    wallet_count: Cell<usize>,
    getter_reads: Cell<usize>,
    appended: Cell<bool>,
    banks: RefCell<Vec<(N, Fields<N>)>>,
    created: RefCell<Vec<N>>,
    public_states: RefCell<Vec<Rc<PublicSubscriptions<u32>>>>,
    private_states: RefCell<Vec<Rc<PrivateCommands<u32, String>>>>,
}
impl SessionHost for Host {
    type Value = N;
    fn get(&self, v: &N, key: &str) -> JsResult<N> {
        if matches!(v, N::U | N::Null) {
            return Err(error(
                "TypeError",
                &format!("Cannot read properties of {} (reading '{key}')", v.text()),
            ));
        }
        if let N::Slot(get, _) = v {
            return self.get(&get(), key);
        }
        if let Some((_, fields)) = self.banks.borrow().iter().find(|(base, _)| base.same(v)) {
            if let Some(value) = fields.borrow().get(key).cloned() {
                return Ok(match value {
                    N::Slot(get, _) => get(),
                    value => value,
                });
            }
        }
        if self.name == "late-lite-error" && key == "m" && v.same(&self.response) {
            let n = self.getter_reads.get() + 1;
            self.getter_reads.set(n);
            return Ok(if n == 1 { N::U } else { self.string("late") });
        }
        if self.group == "fills"
            && self.name == "iterator-append"
            && key == "co"
            && v.same(&self.pages.get("0").get("r").get("0"))
            && !self.appended.replace(true)
        {
            self.event(vec![self.string("append-fill")]);
            Self::push(
                &self.pages.get("0").get("r"),
                N::from(json!({"co":"42","s":4})),
            );
        }
        if let N::MapView(get, set) = v {
            let get = get.clone();
            let set = set.clone();
            if key == "size" {
                return Ok(self.number(get().len() as f64));
            }
            let name = key.to_owned();
            return Ok(self.f(move |receiver, a| match name.as_str() {
                "get" => Ok(get().get(&a[0].text()).cloned().unwrap_or(N::U)),
                "set" => {
                    set(a[0].text(), a[1].clone());
                    Ok(receiver)
                }
                _ => Ok(N::U),
            }));
        }
        if let N::SetView(get) = v {
            let get = get.clone();
            if key == "size" {
                return Ok(self.number(get().len() as f64));
            }
            if key == "has" {
                let h = self.self_rc();
                return Ok(self.f(move |_, a| Ok(N::B(get().iter().any(|v| h.equal(v, &a[0]))))));
            }
        }
        if key == "size" {
            match v {
                N::M(m) => return Ok(self.number(m.borrow().len() as f64)),
                N::Set(s) => return Ok(self.number(s.borrow().len() as f64)),
                _ => {}
            }
        }
        if let N::M(map) = v {
            let map = map.clone();
            let k = key.to_owned();
            return Ok(self.f(move |_, a| match k.as_str() {
                "get" => Ok(map.borrow().get(&a[0].text()).cloned().unwrap_or(N::U)),
                "set" => {
                    map.borrow_mut().insert(a[0].text(), a[1].clone());
                    Ok(N::M(map.clone()))
                }
                _ => Ok(N::U),
            }));
        }
        if let N::Set(set) = v {
            let set = set.clone();
            let k = key.to_owned();
            let h = self.self_rc();
            return Ok(self.f(move |_, a| match k.as_str() {
                "has" => Ok(N::B(set.borrow().iter().any(|v| h.equal(v, &a[0])))),
                "add" => {
                    if !set.borrow().iter().any(|v| h.equal(v, &a[0])) {
                        set.borrow_mut().push(a[0].clone());
                    }
                    Ok(N::Set(set.clone()))
                }
                _ => Ok(N::U),
            }));
        }
        if key == "toString" {
            let h = self.self_rc();
            return Ok(self.f(move |v, _| {
                if h.name == "number-string-error" && matches!(v, N::Num(_)) {
                    Err(error("Error", "number-string-fail"))
                } else {
                    Ok(N::S(v.text()))
                }
            }));
        }
        if key == "startsWith" {
            return Ok(self.f(|v, a| Ok(N::B(v.text().starts_with(&a[0].text())))));
        }
        let own = v.get(key);
        if !matches!(own, N::U) {
            return Ok(own);
        }

        if let N::A(v) = v {
            if key == "length" {
                return Ok(N::Num(v.borrow().len() as f64));
            }
        }
        match (v, key) {
            (N::S(_), "toUpperCase" | "toLowerCase") => {
                let upper = key == "toUpperCase";
                Ok(self.f(move |receiver, _| {
                    let N::S(v) = receiver else {
                        panic!("string receiver")
                    };
                    Ok(N::S(if upper {
                        v.to_uppercase()
                    } else {
                        v.to_lowercase()
                    }))
                }))
            }
            (N::A(_), "push" | "map" | "filter" | "find" | "every") => {
                let h = self.self_rc();
                let key = key.to_owned();
                Ok(self.f(move |receiver, args| {
                    let N::A(values) = &receiver else {
                        panic!("array receiver")
                    };
                    if key == "push" {
                        for arg in args {
                            values.borrow_mut().push(arg);
                        }
                        return Ok(N::Num(values.borrow().len() as f64));
                    }
                    let len = values.borrow().len();
                    let mut result = vec![];
                    for i in 0..len {
                        let value = values.borrow().get(i).cloned().unwrap_or(N::U);
                        let mapped = h.call(args[0].clone(), N::U, vec![value.clone()])?;
                        if key == "map" {
                            result.push(mapped);
                        } else if key == "every" {
                            if !h.truthy(&mapped) {
                                return Ok(N::B(false));
                            }
                        } else if h.truthy(&mapped) {
                            if key == "find" {
                                return Ok(value);
                            }
                            result.push(value);
                        }
                    }
                    Ok(if key == "every" {
                        N::B(true)
                    } else if key == "find" {
                        N::U
                    } else {
                        N::array(result)
                    })
                }))
            }
            _ => Ok(N::U),
        }
    }
    fn set(&self, v: &N, k: &str, field: N) -> JsResult<N, ()> {
        if let Some((_, fields)) = self.banks.borrow().iter().find(|(base, _)| base.same(v)) {
            let previous = fields.borrow().get(k).cloned();
            if let Some(N::Slot(_, set)) = previous {
                set(field);
                return Ok(());
            }
            let key = fields.borrow().keys().copied().find(|key| *key == k);
            if let Some(key) = key {
                fields.borrow_mut().insert(key, field);
                return Ok(());
            }
        }
        v.set(k, field);
        Ok(())
    }
    fn call(&self, f: N, receiver: N, args: Vec<N>) -> JsResult<N> {
        match f {
            N::F(f) => f(receiver, args),
            _ => Err(error("TypeError", "not callable")),
        }
    }
    fn function(&self, f: Callback<N>) -> N {
        self.f(move |_, args| f(args.first().cloned().unwrap_or(N::U)))
    }
    fn object(&self, fields: Vec<(&str, N)>) -> N {
        N::obj(fields)
    }
    fn define(&self, v: &N, key: &str, field: N) -> JsResult<N, ()> {
        v.set(key, field);
        Ok(())
    }
    fn array(&self, values: Vec<N>) -> N {
        N::array(values)
    }
    fn spread(&self, target: &N, source: N) -> JsResult<N, ()> {
        if let N::O(v) = source {
            for (k, v) in v.borrow().iter() {
                target.set(k, v.clone());
            }
        }
        Ok(())
    }
    fn string(&self, v: &str) -> N {
        N::S(v.into())
    }
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn undefined(&self) -> N {
        N::U
    }
    fn null(&self) -> N {
        N::Null
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
    fn nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn is_array(&self, v: &N) -> bool {
        matches!(v, N::A(_))
    }
    fn is_string(&self, v: &N) -> bool {
        matches!(v, N::S(_))
    }
    fn is_function(&self, v: &N) -> bool {
        matches!(v, N::F(_))
    }
    fn equals_number(&self, v: &N, n: f64) -> JsResult<N, bool> {
        Ok(self.number_value(v.clone())? == n)
    }
    fn template(&self, values: Vec<N>) -> JsResult<N> {
        Ok(N::S(values.iter().map(N::text).collect()))
    }
    fn error(&self, message: N) -> JsResult<N> {
        Ok(error("Error", &message.text()))
    }
    fn reject(&self, error: N) -> N {
        N::P(Rc::new(Job {
            future: RefCell::new(None),
            result: RefCell::new(Some(Err(error))),
        }))
    }
    fn start(&self, task: JsPromise<N>) -> JsPromise<N> {
        let wrapped = Box::pin(async move {
            let mut value = task.await?;
            loop {
                if let N::P(job) = value {
                    value = Handle(job).await?;
                } else {
                    return Ok(value);
                }
            }
        });
        let job = Rc::new(Job {
            future: RefCell::new(Some(wrapped)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        self.poll_job(&job);
        Box::pin(Handle(job))
    }
    fn promise(&self, task: JsPromise<N>) -> N {
        let job = Rc::new(Job {
            future: RefCell::new(Some(task)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        self.poll_job(&job);
        N::P(job)
    }
    fn await_value(&self, value: N) -> JsPromise<N> {
        Box::pin(async move {
            Later {
                first: false,
                callback: Some(Box::new(|| Ok(N::U))),
            }
            .await?;
            match value {
                N::P(job) => Handle(job).await,
                value => Ok(value),
            }
        })
    }
}
#[allow(unused_variables)]
impl AccountHost for Host {
    fn binding(&self) -> Rc<Self> {
        self.self_rc()
    }
    fn set_indexed(&self, target: &N, key: N, value: N) -> JsResult<N, ()> {
        self.set(target, &key.text(), value)
    }
    fn number_value(&self, value: N) -> JsResult<N, f64> {
        Ok(match value {
            N::Num(v) => v,
            N::U => f64::NAN,
            N::Null => 0.,
            N::B(v) => {
                if v {
                    1.
                } else {
                    0.
                }
            }
            N::S(v) => {
                if v.trim().is_empty() {
                    0.
                } else {
                    v.trim().parse().unwrap_or(f64::NAN)
                }
            }
            _ => f64::NAN,
        })
    }
    fn greater_than_zero(&self, value: N) -> JsResult<N, bool> {
        Ok(self.number_value(value)? > 0.)
    }
    fn symbol_set(&self, values: N) -> JsResult<N> {
        panic!("unused old-lane hook")
    }
    fn has_symbol(&self, set: &N, value: N) -> JsResult<N, bool> {
        panic!("unused old-lane hook")
    }
    fn append_iterable(&self, target: &N, values: N) -> JsResult<N, ()> {
        self.for_of(values, &mut |value| {
            Self::push(target, value);
            Ok(false)
        })
    }
    fn exchange(&self) -> N {
        self.ex.clone()
    }
    fn endpoint(&self, name: &str) -> N {
        self.string(if name == "FILL_HISTORY" {
            "/lite/v1/fill_history"
        } else {
            panic!("endpoint")
        })
    }
    fn severity(&self, severity: LogSeverity) -> N {
        self.string(match severity {
            LogSeverity::Debug => "debug",
            LogSeverity::Info => "info",
            LogSeverity::Warning => "warning",
            LogSeverity::Error => "error",
        })
    }
    fn is_undefined(&self, value: &N) -> bool {
        matches!(value, N::U)
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
    fn indexed(&self, value: &N, key: &N) -> JsResult<N> {
        self.get(value, &key.text())
    }
    fn parse_float(&self, value: N) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_float(&value.text()))
    }
    fn parse_int(&self, value: N) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_int(&value.text(), None))
    }
    fn js_string(&self, value: N) -> JsResult<N> {
        Ok(self.string(&value.text()))
    }
    fn add(&self, a: N, b: N) -> JsResult<N> {
        Ok(if matches!(a, N::S(_)) || matches!(b, N::S(_)) {
            self.string(&format!("{}{}", a.text(), b.text()))
        } else {
            self.number(self.number_value(a)? + self.number_value(b)?)
        })
    }
    fn now(&self) -> JsResult<N, f64> {
        self.event(vec![self.string("now")]);
        Ok(100000.)
    }
    fn virtual_now(&self) -> JsResult<N, f64> {
        panic!("unused old-lane hook")
    }
    fn for_of(&self, value: N, visitor: &mut Iteration<'_, N>) -> JsResult<N, ()> {
        let mut i = 0;
        loop {
            let value = match &value {
                N::A(v) | N::Set(v) => v.borrow().get(i).cloned(),
                _ => panic!("iterable"),
            };
            let Some(value) = value else { break };
            if visitor(value)? {
                break;
            }
            i += 1;
        }
        Ok(())
    }
    fn loop_after(&self, callback: Rc<dyn Fn() -> JsResult<N>>, delay: f64) -> JsResult<N, ()> {
        panic!("unused old-lane hook")
    }
    fn sleep(&self, delay: f64) -> JsResult<N, JsPromise<N>> {
        panic!("unused old-lane hook")
    }
    fn builder_address(&self, testnet: N) -> JsResult<N> {
        panic!("unused old-lane hook")
    }
    fn fee_to_raw(&self, fee: N) -> JsResult<N, f64> {
        panic!("unused old-lane hook")
    }
    fn builder_info(&self, testnet: N) -> JsResult<N> {
        panic!("unused old-lane hook")
    }
    fn builder_error(&self, opts: N) -> JsResult<N> {
        panic!("unused old-lane hook")
    }
    fn position_id(&self, symbol: N) -> JsResult<N> {
        panic!("unused old-lane hook")
    }
    fn trigger_metadata(&self, order: N) -> JsResult<N> {
        panic!("unused old-lane hook")
    }
    fn time_in_force(&self, order: N) -> JsResult<N> {
        panic!("unused old-lane hook")
    }
    fn keys_join(&self, value: N) -> JsResult<N> {
        panic!("unused old-lane hook")
    }
}
impl Host {
    fn new(group: &str, name: &str) -> Rc<Self> {
        let ex = N::from(
            json!({"options":{"secret":"0101010101010101010101010101010101010101010101010101010101010101","testnet":name=="testnet"},"apiExtras":{"subAccountId":"sub"},"wallet":{"address":"0xWallet"},"grvtTradingAccountId":if name=="hex-account"{"0x10"}else{"17"},"store":{"marketsBySymbol":{"BTC/USDT":{"id":"BTC_USDT_Perp","precision":{"amount":0.01,"price":0.5},"info":{"ih":"0x030501","bd":if name=="negative-decimals"{-3}else{3},"qd":6}}}}}),
        );
        ex.set(
            "_nonceToOrderId",
            N::M(Rc::new(RefCell::new(IndexMap::new()))),
        );
        ex.set(
            "_nonceCreationTimes",
            N::M(Rc::new(RefCell::new(IndexMap::new()))),
        );
        let opts = N::from(
            json!({"symbol":"BTC/USDT","side":"buy","type":"limit","price":101.25,"amount":2.345}),
        );
        for (case, key, value) in [
            ("market", "type", N::S("market".into())),
            ("sell", "side", N::S("sell".into())),
            ("immediate", "timeInForce", N::S("ImmediateOrCancel".into())),
            ("fill-kill", "timeInForce", N::S("FillOrKill".into())),
            ("post-only", "timeInForce", N::S("PostOnly".into())),
            ("reduce", "reduceOnly", N::S("truthy".into())),
            ("zero-size", "amount", N::Num(0.001)),
            ("nan-size", "amount", N::Num(f64::NAN)),
        ] {
            if name == case {
                opts.set(key, value);
            }
        }
        if matches!(name, "stop" | "take" | "trigger-missing") {
            opts.set(
                "type",
                N::S(
                    if name == "take" {
                        "take_profit_market"
                    } else {
                        "stop_market"
                    }
                    .into(),
                ),
            );
            if name != "trigger-missing" {
                opts.set("triggerPrice", N::Num(99.));
                opts.set("slTriggerType", N::S("mark_price".into()));
                opts.set("tpTriggerType", N::S("index_price".into()));
                opts.set("close", N::B(true));
            } else {
                Self::delete(&opts, "price");
            }
        }
        if name == "attached-both" {
            opts.set("stopLoss", N::Num(80.));
            opts.set("takeProfit", N::Num(120.));
        }
        if name == "no-wallet" {
            ex.set("wallet", N::U);
        }
        if name == "no-market" {
            ex.get("store").set("marketsBySymbol", N::obj(vec![]));
        }
        if name == "no-account" || group == "fills" && name == "fallback-account" {
            ex.set("grvtTradingAccountId", N::U);
            if group == "fills" && name == "no-account" {
                ex.set("apiExtras", N::obj(vec![]));
            }
        }
        let response = N::from(match name {
            "response-error" => json!({"c":1001,"m":"denied"}),
            "http-error" => json!({"status":429}),
            "nested-error" => json!({"data":{"message":"nested","c":8}}),
            "late-lite-error" => json!({"c":7}),
            _ => json!({"r":{"oi":"real","bf":"0.02"}}),
        });
        let pages = Self::pages(name);
        let h = Rc::new(Self {
            me: RefCell::new(std::rc::Weak::new()),
            group: group.into(),
            name: name.into(),
            ex,
            opts,
            pages,
            response,
            events: RefCell::new(vec![]),
            jobs: RefCell::new(vec![]),
            failure: error("Error", "io-fail"),
            builder_failure: RefCell::new(N::U),
            post_count: Cell::new(0),
            wallet_count: Cell::new(0),
            getter_reads: Cell::new(0),
            appended: Cell::new(false),
            banks: RefCell::new(vec![]),
            created: RefCell::new(vec![]),
            public_states: RefCell::new(vec![]),
            private_states: RefCell::new(vec![]),
        });
        *h.me.borrow_mut() = Rc::downgrade(&h);
        h.install();
        h
    }
    fn pages(name: &str) -> N {
        let mut pages =
            N::from(json!([{"r":[{"co":"42","s":"1.25"},{"client_order_id":42,"size":2}],"n":""}]));
        let replacement = match name {
            "mapping" | "mapping-collision" | "real-ambiguity" => {
                Some(json!([{"r":[{"oi":"real","s":2}],"n":""}]))
            }
            "contradictory-client" => Some(json!([{"r":[{"co":"else","oi":"real","s":2}],"n":""}])),
            "multiple-clients" => {
                Some(json!([{"r":[{"co":"42","client_order_id":"43","s":2}],"n":""}]))
            }
            "two-pages" => Some(
                json!([{"r":[{"co":"42","s":1}],"n":"p2"},{"result":[{"metadata":{"client_order_id":"42"},"size":"2"}],"next":""}]),
            ),
            "repeat-cursor" => {
                Some(json!([{"r":[{"co":"42","s":1}],"n":"p"},{"r":[{"co":"42","s":2}],"n":"p"}]))
            }
            "missing-cursor" => Some(json!([{"r":[{"co":"42","s":1}]}])),
            "invalid-cursor" => Some(json!([{"r":[],"n":7}])),
            "own-n-wins" => Some(json!([{"r":[],"next":""}])),
            "invalid-shape" => Some(json!([null])),
            "no-array" => Some(json!([{"r":{},"n":""}])),
            _ => None,
        };
        if let Some(v) = replacement {
            pages = N::from(v);
        }
        if name == "own-n-wins" {
            pages.get("0").set("n", N::U);
        }
        if name == "invalid-amounts" {
            let values = vec![
                N::Num(0.),
                N::Num(-1.),
                N::Num(f64::NAN),
                N::Num(f64::INFINITY),
                N::S("1e2".into()),
                N::S(" 2".into()),
                N::S("2.".into()),
                N::S("-2".into()),
                N::S("0".into()),
                N::S(".5".into()),
                N::S("1\n".into()),
            ];
            pages.get("0").set(
                "r",
                N::array(
                    values
                        .into_iter()
                        .map(|s| N::obj(vec![("co", N::S("42".into())), ("s", s)]))
                        .collect(),
                ),
            );
        }
        if name == "overflow" {
            pages.get("0").set(
                "r",
                N::array(vec![
                    N::obj(vec![
                        ("co", N::S("42".into())),
                        ("s", N::Num(f64::MAX))
                    ]);
                    2
                ]),
            );
        }
        if matches!(name, "page20-empty" | "page20-nonempty") {
            pages=N::array((0..20).map(|i|N::from(json!({"r":[{"co":"42","s":1}],"n":if i==19&&name=="page20-empty"{"".into()}else{format!("p{i}")}}))).collect());
        }
        pages
    }
    fn delete(v: &N, key: &str) {
        if let N::O(v) = v {
            v.borrow_mut().shift_remove(key);
        }
    }
    fn push(v: &N, value: N) {
        if let N::A(v) = v {
            v.borrow_mut().push(value);
        } else {
            panic!("native array")
        }
    }
    fn self_rc(&self) -> Rc<Self> {
        self.me.borrow().upgrade().unwrap()
    }
    fn f(&self, callback: impl Fn(N, Vec<N>) -> JsResult<N> + 'static) -> N {
        N::F(Rc::new(callback))
    }
    fn event(&self, args: Vec<N>) {
        self.events
            .borrow_mut()
            .push(N::array(args.iter().map(N::snapshot).collect()));
    }
    fn poll_job(&self, j: &Rc<Job>) {
        let Some(mut f) = j.future.borrow_mut().take() else {
            return;
        };
        let mut cx = Context::from_waker(Waker::noop());
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => *j.result.borrow_mut() = Some(v),
            Poll::Pending => *j.future.borrow_mut() = Some(f),
        }
    }
    fn finish(&self, mut task: JsPromise<N>) -> JsResult<N> {
        for _ in 0..128 {
            for job in self.jobs.borrow().clone() {
                self.poll_job(&job);
            }
            let mut cx = Context::from_waker(Waker::noop());
            if let Poll::Ready(v) = task.as_mut().poll(&mut cx) {
                return v;
            }
        }
        panic!("task did not settle within 128 polls")
    }
    // The minified bodies here put `}if` and `}{` straight after a guard
    // `return`, which possible_missing_else reads as a dropped `else`. There is
    // no else to add: the earlier branch returns.
    #[allow(clippy::possible_missing_else)]
    fn install(&self) {
        let h = self.self_rc();
        self.ex.set(
            "log",
            self.f(move |_, a| {
                h.event(vec![h.string("log"), a[0].clone(), a[1].clone()]);
                if h.name == "success-log-error"
                    && a[0].text().starts_with("Order created successfully")
                    || h.name == "log-error" && a[0].text().starts_with("fetchFillsForOrderIds")
                {
                    Err(error("Error", "log-fail"))
                } else {
                    Ok(N::U)
                }
            }),
        );
        let h = self.self_rc();
        self.ex.set(
            "emitAndLogError",
            self.f(move |r, a| {
                assert!(r.same(&h.ex), "placement error receiver");
                h.event(vec![h.string("error"), a[0].clone()]);
                Ok(N::U)
            }),
        );
        let h = self.self_rc();
        self.ex.set(
            "resolveCallerClientOrderId",
            self.f(move |r, _| {
                assert!(r.same(&h.ex), "placement caller receiver");
                h.event(vec![h.string("caller")]);
                if h.name == "caller-error" {
                    return Err(h.failure.clone());
                }
                Ok(match h.name.as_str() {
                    "caller-id" => h.string("99"),
                    "empty-caller-id" => h.string(""),
                    _ => N::U,
                })
            }),
        );
        let h = self.self_rc();
        self.ex.set(
            "getInstrumentSigningMetadata",
            self.f(move |r, a| {
                assert!(r.same(&h.ex));
                h.event(vec![h.string("metadata")]);
                if h.name == "metadata-error" {
                    Err(h.failure.clone())
                } else {
                    Ok(a[0].get("info"))
                }
            }),
        );
        self.ex.set(
            "mapTimeInForceToEnum",
            self.f(|_, a| {
                Ok(N::Num(match a[0].text().as_str() {
                    "IMMEDIATE_OR_CANCEL" => 3.,
                    "FILL_OR_KILL" => 4.,
                    _ => 1.,
                }))
            }),
        );
        let h = self.self_rc();
        self.ex.set("getBuilderInfo",self.f(move|r,_|{assert!(r.same(&h.ex));h.event(vec![h.string("builder")]);let h=h.clone();Ok(h.clone().promise(Box::pin(async move{if h.name=="builder-error"{let e=error("GrvtBuilderUnauthorizedError","GRVT builder authorization cap (100) is below required (300). Please re-authorize.");e.set("code",h.string("GRVT_BUILDER_UNAUTHORIZED"));e.set("reason",h.string("cap-too-low"));e.set("requiredRaw",h.number(300.));e.set("authorizedRaw",h.number(100.));*h.builder_failure.borrow_mut()=e.clone();return Err(e);}if h.name=="builder-mutation"{h.opts.set("type",h.string("market"));h.opts.set("side",h.string("sell"));h.opts.set("timeInForce",h.string("PostOnly"));h.opts.set("reduceOnly",h.boolean(true));h.opts.set("triggerPrice",h.number(98.));h.ex.set("grvtTradingAccountId",h.string("19"));h.ex.get("wallet").set("address",h.string("0xLive"));h.ex.get("options").set("testnet",h.boolean(true));h.ex.get("options").set("secret",h.string("changed"));h.ex.get("store").get("marketsBySymbol").get("BTC/USDT").set("id",h.string("LIVE"));}Ok(N::from(json!({"builder":"0xbuilder","builderFee":"0.03","builderFeeInt":300})))})))}));
        let h = self.self_rc();
        self.ex.set(
            "recordNonce",
            self.f(move |r, a| {
                assert!(r.same(&h.ex));
                h.event(vec![
                    h.string("nonce"),
                    a[0].clone(),
                    a[1].clone(),
                    a[2].clone(),
                ]);
                if h.name == "record-error" {
                    Err(h.failure.clone())
                } else {
                    Ok(N::U)
                }
            }),
        );
        let emitter = N::obj(vec![]);
        let h = self.self_rc();
        let identity = emitter.clone();
        emitter.set(
            "emit",
            self.f(move |r, a| {
                assert!(r.same(&identity));
                h.event(vec![h.string("emit"), a[0].clone(), a[1].clone()]);
                if h.name == "emit-error" {
                    Err(h.failure.clone())
                } else {
                    Ok(N::U)
                }
            }),
        );
        self.ex.set("emitter", emitter);
        let tdg = N::obj(vec![]);
        let h = self.self_rc();
        let identity = tdg.clone();
        tdg.set(
            "createOrder",
            self.f(move |r, a| {
                assert!(r.same(&identity), "placement captured create receiver");
                h.event(vec![h.string("create"), a[0].clone(), a[1].clone()]);
                Ok(h.response.clone())
            }),
        );
        let client = N::obj(vec![("tdgClient", tdg.clone())]);
        let h = self.self_rc();
        client.set(
            "authenticatedEndpoint",
            self.f(move |_, _| {
                h.event(vec![h.string("endpoint")]);
                if h.name == "endpoint-mutation" {
                    tdg.set(
                        "createOrder",
                        h.f(|_, _| Err(error("Error", "replaced-create"))),
                    );
                }
                Ok(N::from(json!({"token":"endpoint"})))
            }),
        );
        self.ex.set(
            "grvtClient",
            if self.name == "no-client" {
                N::U
            } else {
                client
            },
        );
        let xhr = N::obj(vec![]);
        let h = self.self_rc();
        let identity = xhr.clone();
        xhr.set(
            "post",
            self.f(move |r, a| {
                assert!(r.same(&identity), "fills post receiver");
                h.event(vec![h.string("post"), a[0].clone(), a[1].clone()]);
                if matches!(h.name.as_str(), "post-error" | "log-error") {
                    return Ok(h.reject(h.failure.clone()));
                }
                let i = h.post_count.get();
                h.post_count.set(i + 1);
                Ok(N::obj(vec![("data", h.pages.get(&i.to_string()))]))
            }),
        );
        self.ex.set("tradingXhr", xhr);
        if self.group == "fills" {
            let times = self.ex.get("_nonceCreationTimes");
            if let N::M(times) = times {
                match self.name.as_str() {
                    "known-time" => {
                        times.borrow_mut().insert("42".into(), N::Num(350000.));
                    }
                    "null-time" => {
                        times.borrow_mut().insert("42".into(), N::Null);
                    }
                    "unknown-time" => {
                        times.borrow_mut().insert("42".into(), N::U);
                    }
                    _ => {}
                }
            }
            if matches!(
                self.name.as_str(),
                "mapping" | "mapping-collision" | "contradictory-client" | "real-ambiguity"
            ) {
                if let N::M(ids) = self.ex.get("_nonceToOrderId") {
                    ids.borrow_mut().insert("42".into(), self.string("real"));
                    if self.name == "mapping-collision" {
                        ids.borrow_mut().insert("43".into(), self.string("real"));
                    }
                }
            }
        }
    }
    fn ids(&self) -> N {
        match self.name.as_str() {
            "empty" => N::array(vec![]),
            "bad-input" => N::Null,
            "duplicate-ids" => N::from(json!(["42", "42"])),
            "numeric-ids" => N::from(json!([42])),
            "mapping-collision" => N::from(json!(["42", "43"])),
            "real-ambiguity" => N::from(json!(["42", "real"])),
            _ => N::from(json!(["42"])),
        }
    }
}
impl TradingHost for Host {
    fn parse_int_decimal(&self, value: N) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_int(&value.text(), Some(10)))
    }
    fn random(&self) -> JsResult<N, f64> {
        self.event(vec![self.string("random")]);
        Ok(0.123456789)
    }
    fn adjust(&self, value: N, step: N) -> JsResult<N, f64> {
        Ok(safe_cex_num::adjust(
            self.number_value(value)?,
            self.number_value(step)?,
        ))
    }
    fn pow(&self, base: f64, exponent: N) -> JsResult<N, f64> {
        Ok(safe_cex_num::math_pow(base, self.number_value(exponent)?))
    }
    fn sdk_domain(&self) -> N {
        N::from(source()["constants"]["domain"].clone())
    }
    fn sdk_order_types(&self) -> N {
        N::from(source()["constants"]["orderTypes"].clone())
    }
    fn sdk_sign(&self, key: N, typed: N) -> JsResult<N> {
        self.event(vec![self.string("sdk-sign"), key, typed]);
        if self.name == "sign-error" {
            Err(self.failure.clone())
        } else {
            Ok(self.string(&signature()))
        }
    }
    fn sdk_decode(&self, signature: N) -> JsResult<N> {
        self.event(vec![self.string("sdk-decode"), signature]);
        if self.name == "decode-error" {
            Err(self.failure.clone())
        } else {
            Ok(decoded())
        }
    }
    fn is_builder_error(&self, e: &N) -> bool {
        e.get("name").text() == "GrvtBuilderUnauthorizedError"
    }
    fn new_map(&self) -> JsResult<N> {
        Ok(N::M(Rc::new(RefCell::new(IndexMap::new()))))
    }
    fn new_set(&self, values: N) -> JsResult<N> {
        let set = Rc::new(RefCell::new(vec![]));
        self.for_of(values, &mut |value| {
            if !set.borrow().iter().any(|v| self.equal(v, &value)) {
                set.borrow_mut().push(value);
            }
            Ok(false)
        })?;
        Ok(N::Set(set))
    }
    fn has_own(&self, value: &N, key: &str) -> JsResult<N, bool> {
        Ok(match value {
            N::O(v) => v.borrow().contains_key(key),
            _ => false,
        })
    }
    fn is_object(&self, value: &N) -> bool {
        matches!(value, N::O(_) | N::A(_) | N::M(_) | N::Set(_))
    }
    fn is_number(&self, value: &N) -> bool {
        matches!(value, N::Num(_))
    }
    fn decimal_pattern(&self, value: &N) -> JsResult<N, bool> {
        let text = value.text();
        let mut split = text.split('.');
        let first = split.next().unwrap();
        let last = split.next();
        Ok((!first.is_empty() || last.is_some())
            && first.bytes().all(|b| b.is_ascii_digit())
            && last.is_none_or(|v| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
            && split.next().is_none())
    }
    fn first_set_value(&self, values: N) -> JsResult<N> {
        if let N::Set(v) = values {
            Ok(v.borrow().first().cloned().unwrap_or(N::U))
        } else {
            panic!("set")
        }
    }
    fn minimum(&self, values: N) -> JsResult<N, f64> {
        let mut min = f64::INFINITY;
        self.for_of(values, &mut |value| {
            min = safe_cex_num::math_min(min, self.number_value(value)?);
            Ok(false)
        })?;
        Ok(min)
    }
}
fn signature() -> String {
    format!("0x{}{}1b", "11".repeat(32), "22".repeat(32))
}
fn decoded() -> N {
    N::obj(vec![
        ("r", N::S(format!("0x{}", "11".repeat(32)))),
        ("s", N::S(format!("0x{}", "22".repeat(32)))),
        ("v", N::Num(27.)),
    ])
}
impl SigningHost for Host {
    fn undefined_argument(&self, value: &N) -> bool {
        matches!(value, N::U)
    }
    fn new_wallet(&self, key: N) -> JsResult<N> {
        if self.group == "key" {
            return safe_cex_crypto::ethereum::sign_digest(&[0; 32], &key.text())
                .map(|_| N::obj(vec![]))
                .map_err(|_| error("Error", "invalid-key"));
        }
        self.event(vec![self.string("wallet"), key]);
        let count = self.wallet_count.get() + 1;
        self.wallet_count.set(count);
        if self.name == "invalid-key"
            || matches!(
                self.name.as_str(),
                "wallet-second-error" | "wallet-catch-log-error"
            ) && count == 2
        {
            Err(error("Error", "wallet-fail"))
        } else {
            Ok(N::from(json!({"address":"0xWallet"})))
        }
    }
    fn legacy_domain(&self, testnet: bool) -> N {
        N::from(json!({"name":"Grvt","version":"0","chainId":if testnet{"325"}else{"324"}}))
    }
    fn legacy_order_types(&self) -> N {
        N::from(source()["constants"]["legacyTypes"].clone())
    }
    fn signature_from(&self, sig: N) -> JsResult<N> {
        self.event(vec![self.string("signature-from"), sig]);
        if self.name == "decode-error" {
            Err(self.failure.clone())
        } else {
            Ok(decoded())
        }
    }
}
impl CompositionHost for Host {
    type Timer = u32;
    type PrivateTimer = u32;
    type FillKey = String;
    fn base_exchange(&self, name: N, opts: N, store: N) -> JsResult<N> {
        self.event(vec![self.string("base-exchange"), name.clone()]);
        let base = N::obj(vec![
            ("name", name),
            ("options", opts),
            ("store", store),
            ("isDisposed", N::B(false)),
        ]);
        let h = self.self_rc();
        let identity = base.clone();
        base.set(
            "log",
            self.f(move |r, a| {
                assert!(r.same(&identity), "constructor log receiver");
                h.event(vec![h.string("log"), a[0].clone(), a[1].clone()]);
                let msg = a[0].text();
                let failure =
                    if h.name == "wallet-log-error" && msg.starts_with("Wallet initialized") {
                        Some("wallet-log-fail")
                    } else if h.name == "wallet-catch-log-error"
                        && msg.starts_with("Failed to initialize wallet")
                    {
                        Some("wallet-catch-log-fail")
                    } else if h.name == "sdk-log-error" && msg == "GRVT SDK client initialized" {
                        Some("sdk-log-fail")
                    } else {
                        None
                    };
                match failure {
                    Some(e) => Err(error("Error", e)),
                    None => Ok(N::U),
                }
            }),
        );
        self.created.borrow_mut().push(base.clone());
        Ok(base)
    }
    fn base_socket(&self, parent: N) -> JsResult<N> {
        self.event(vec![self.string("base-socket")]);
        if self.name == "base-error" {
            return Err(self.failure.clone());
        }
        let base = N::obj(vec![
            ("parent", parent),
            ("isDisposed", N::B(false)),
            ("pingAt", N::Num(0.)),
        ]);
        self.created.borrow_mut().push(base.clone());
        Ok(base)
    }
    fn attach_fields(&self, instance: &N, fields: Fields<N>) {
        self.banks.borrow_mut().push((instance.clone(), fields));
    }
    fn rule_value(&self, rule: Rc<ClientOrderIdRule>) -> N {
        N::obj(vec![
            ("label", self.string("GRVT")),
            (
                "requirement",
                self.string("be a canonical decimal integer between 1 and 9223372036854775807"),
            ),
            (
                "isValid",
                self.f(move |_, a| Ok(N::B(rule.is_valid(&a[0].text())))),
            ),
        ])
    }
    fn nonce_ids_view(&self, ids: NonceIds) -> N {
        let owned = ids.clone();
        N::MapView(
            Rc::new(move || {
                owned
                    .borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), N::S(v.clone())))
                    .collect()
            }),
            Rc::new(move |key, value| {
                let N::S(value) = value else {
                    panic!("native typed nonce id")
                };
                ids.borrow_mut().insert(key, value);
            }),
        )
    }
    fn nonce_times_view(&self, times: NonceTimes) -> N {
        let owned = times.clone();
        N::MapView(
            Rc::new(move || {
                owned
                    .borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.map(N::Num).unwrap_or(N::U)))
                    .collect()
            }),
            Rc::new(move |key, value| {
                let value = match value {
                    N::Num(v) => Some(v),
                    N::U => None,
                    _ => panic!("native typed creation time"),
                };
                times.borrow_mut().insert(key, value);
            }),
        )
    }
    fn public_field(&self, state: Rc<PublicSubscriptions<u32>>, key: &str) -> N {
        if !self
            .public_states
            .borrow()
            .iter()
            .any(|s| Rc::ptr_eq(s, &state))
        {
            self.public_states.borrow_mut().push(state.clone());
        }
        match key {
            "requestId" => {
                let s = state.clone();
                N::Slot(
                    Rc::new(move || N::Num(s.request_id.get())),
                    Rc::new(move |v| {
                        let N::Num(v) = v else { panic!("number state") };
                        state.request_id.set(v)
                    }),
                )
            }
            "tickerBatchGeneration" => {
                let s = state.clone();
                N::Slot(
                    Rc::new(move || N::Num(s.ticker_batch_generation.get())),
                    Rc::new(move |v| {
                        let N::Num(v) = v else { panic!("number state") };
                        state.ticker_batch_generation.set(v)
                    }),
                )
            }
            "subscriptions" => N::Slot(
                Rc::new(move || {
                    assert!(state.subscriptions.borrow().is_empty());
                    N::M(Rc::new(RefCell::new(IndexMap::new())))
                }),
                Rc::new(|_| panic!("typed subscriptions")),
            ),
            "tickerSubscriptions" => {
                let owned = state.clone();
                N::MapView(
                    Rc::new(move || {
                        owned
                            .ticker_subscriptions
                            .borrow()
                            .iter()
                            .map(|(k, v)| (k.clone(), N::Num(*v)))
                            .collect()
                    }),
                    Rc::new(move |k, v| {
                        let N::Num(v) = v else { panic!("ticker id") };
                        state.ticker_subscriptions.borrow_mut().insert(k, v);
                    }),
                )
            }
            "tickerBatchTimers" => N::SetView(Rc::new(move || {
                state
                    .ticker_batch_timers
                    .values()
                    .into_iter()
                    .map(|v| N::Num(v as f64))
                    .collect()
            })),
            "topics" => N::obj(vec![]),
            _ => panic!("public field"),
        }
    }
    fn private_field(&self, state: Rc<PrivateCommands<u32, String>>, key: &str) -> N {
        if !self
            .private_states
            .borrow()
            .iter()
            .any(|s| Rc::ptr_eq(s, &state))
        {
            self.private_states.borrow_mut().push(state.clone());
            let base = self.created.borrow().last().unwrap().clone();
            let s = state.clone();
            let t = state.clone();
            base.set(
                "isDisposed",
                N::Slot(
                    Rc::new(move || N::B(s.is_disposed.get())),
                    Rc::new(move |v| {
                        let N::B(v) = v else { panic!("bool") };
                        t.is_disposed.set(v)
                    }),
                ),
            );
            let s = state.clone();
            let t = state.clone();
            base.set(
                "pingAt",
                N::Slot(
                    Rc::new(move || N::Num(s.ping_at.get())),
                    Rc::new(move |v| {
                        let N::Num(v) = v else { panic!("number") };
                        t.ping_at.set(v)
                    }),
                ),
            );
        }
        macro_rules! boolean {
            ($field:ident) => {{
                let s = state.clone();
                N::Slot(
                    Rc::new(move || N::B(s.$field.get())),
                    Rc::new(move |v| {
                        let N::B(v) = v else { panic!("bool state") };
                        state.$field.set(v)
                    }),
                )
            }};
        }
        macro_rules! number {
            ($field:ident) => {{
                let s = state.clone();
                N::Slot(
                    Rc::new(move || N::Num(s.$field.get())),
                    Rc::new(move |v| {
                        let N::Num(v) = v else { panic!("number state") };
                        state.$field.set(v)
                    }),
                )
            }};
        }
        match key {
            "reportedMissingProxyHost" => boolean!(reported_missing_proxy_host),
            "treatAnyMessageAsPong" => N::B(true),
            "subscriptions" => N::Slot(
                Rc::new(move || {
                    assert!(state.subscriptions.borrow().is_empty());
                    N::M(Rc::new(RefCell::new(IndexMap::new())))
                }),
                Rc::new(|_| panic!("typed subscriptions")),
            ),
            "requestId" => number!(request_id),
            "authenticated" => boolean!(authenticated),
            "authenticationFailed" => boolean!(authentication_failed),
            "sessionRefreshInFlight" => boolean!(session_refresh_in_flight),
            "sessionRefreshAttempts" => number!(session_refresh_attempts),
            "privateStreamHealthyAt" => N::Slot(
                Rc::new(move || {
                    state
                        .private_stream_healthy_at
                        .get()
                        .map(N::Num)
                        .unwrap_or(N::U)
                }),
                Rc::new(|_| panic!("health typed value")),
            ),
            "lastPositionFetchTime" => number!(last_position_fetch_time),
            "positionFetchThrottleMs" => number!(position_fetch_throttle_ms),
            "positionFetchScheduled" => boolean!(position_fetch_scheduled),
            "hasPendingPositionEvents" => boolean!(has_pending_position_events),
            "ordersThrottleTimeout" => N::Slot(
                Rc::new(move || {
                    state
                        .orders_throttle_timeout
                        .borrow()
                        .map(|v| N::Num(v as f64))
                        .unwrap_or(N::U)
                }),
                Rc::new(|_| panic!("timer typed value")),
            ),
            "processedFillIds" => N::SetView(Rc::new(move || {
                state
                    .processed_fill_ids
                    .borrow()
                    .iter()
                    .cloned()
                    .map(N::S)
                    .collect()
            })),
            _ => panic!("private field"),
        }
    }
    fn member(&self, class: &'static str, name: &'static str, instance: &N) -> N {
        let expected_class = if self.fields(instance).contains_key("name") {
            "exchange"
        } else if self.fields(instance).contains_key("tickerSubscriptions") {
            "public"
        } else {
            "private"
        };
        assert_eq!(
            class, expected_class,
            "native member factory class binding {name}"
        );
        assert!(
            instance.same(self.created.borrow().last().unwrap()),
            "native member factory owner binding {class}.{name}"
        );
        let h = self.self_rc();
        let base = instance.clone();
        self.f(move |_, a| match (class, name) {
            ("public", "mapIntervalToGrvt") => {
                use safe_cex_num::TableHit;
                match safe_cex_venue_grvt::public_subscriptions::map_interval_to_grvt(&a[0].text())
                {
                    TableHit::Mapped(s) => Ok(N::S(s)),
                    _ => panic!("mapped interval"),
                }
            }
            ("private", "getWatchdogStaleThresholdMs") => Ok(N::Num(
                safe_cex_venue_grvt::private_commands::get_watchdog_stale_threshold_ms(),
            )),
            ("exchange", "recordNonce") => {
                let _ = (&h, &base);
                panic!("test invokes owned record_nonce directly")
            }
            _ => {
                panic!("uninvoked configured member {class}.{name}; not a claimed integration gate")
            }
        })
    }
    fn prototype_members(&self, _: &N, _: &IndexMap<&'static str, N>) {}
    fn grafana(&self, periodic: bool) -> N {
        N::from(source()["constants"][if periodic { "periodic" } else { "every" }].clone())
    }
    fn api(&self, kind: &'static str, opts: N, instance: N) -> JsResult<N> {
        assert!(
            opts.same(&instance.get("options")),
            "original options alias"
        );
        assert!(instance.same(&self.created.borrow()[0]), "api parent alias");
        self.event(vec![self.string("api"), self.string(kind)]);
        if self.name == "api-error" && kind == "edge" {
            Err(self.failure.clone())
        } else {
            Ok(N::from(json!({"kind":kind})))
        }
    }
    fn rate_limit(&self, instance: N, config: N) -> JsResult<N> {
        self.event(vec![
            self.string("rate"),
            instance.get("kind"),
            config.get("maxRPS"),
        ]);
        Ok(instance)
    }
    fn sdk_env(&self, testnet: bool) -> N {
        self.string(if testnet { "testnet" } else { "production" })
    }
    fn sdk_client(&self, config: N) -> JsResult<N> {
        let base = self.created.borrow()[0].clone();
        assert!(
            config
                .get("customAxios")
                .same(&self.get(&base, "tradingXhr")?),
            "sdk trading alias"
        );
        assert!(
            config
                .get("customEdgeAxios")
                .same(&self.get(&base, "edgeXhr")?),
            "sdk edge alias"
        );
        self.event(vec![self.string("sdk"), config]);
        if self.name == "sdk-error" {
            Err(self.failure.clone())
        } else {
            Ok(N::from(json!({"tag":"client"})))
        }
    }
    fn socket(&self, private: bool, parent: N) -> JsResult<N> {
        if private {
            new_private_socket(self, parent).map(|s| s.base)
        } else {
            new_public_socket(self, parent).map(|s| s.base)
        }
    }
    fn same_string(&self, a: &N, b: &N) -> bool {
        a.text() == b.text()
    }
    fn severity(&self, severity: LogSeverity) -> N {
        AccountHost::severity(self, severity)
    }
}
impl Host {
    fn fields(&self, base: &N) -> IndexMap<String, N> {
        let mut fields = match base {
            N::O(v) => v.borrow().clone(),
            _ => panic!("base"),
        };
        if let Some((_, bank)) = self.banks.borrow().iter().find(|(v, _)| v.same(base)) {
            for (k, v) in bank.borrow().iter() {
                fields.insert((*k).into(), v.clone());
            }
        }
        fields
    }
    fn assert_instance(&self, base: &N, expected: &Value, label: &str, ancestors: &mut Vec<N>) {
        if ancestors.iter().any(|v| v.same(base)) {
            assert_eq!(expected["reference"], true, "{label} native parent cycle");
            return;
        }
        ancestors.push(base.clone());
        let fields = self.fields(base);
        let fields: Vec<_> = fields
            .iter()
            .filter(|(_, v)| !matches!(v, N::F(_)))
            .collect();
        let source = expected.as_object().expect(label);
        assert_eq!(
            fields.len(),
            source.len(),
            "{label} native instance field count"
        );
        for (k, v) in fields {
            assert!(source.contains_key(k), "{label} native field {k}");
            if self.banks.borrow().iter().any(|(b, _)| b.same(v)) {
                self.assert_instance(v, &source[k], label, ancestors);
            } else {
                assert_native(v, &source[k], label);
            }
        }
        ancestors.pop();
    }
    fn constructor_opts(&self) -> N {
        let opts = N::from(json!({"testnet":self.name=="testnet"}));
        if !matches!(
            self.name.as_str(),
            "bare" | "subaccount" | "api-error" | "record-state" | "rules"
        ) {
            opts.set("secret", self.string("key"));
            opts.set("key", self.string("api"));
        }
        if self.name == "subaccount" {
            opts.set("subaccount", self.string("sub"));
        }
        if self.name == "signer-match" {
            opts.set("grvtSignerAddress", self.string("0xWALLET"));
        }
        if self.name == "signer-mismatch" {
            opts.set("grvtSignerAddress", self.string("0xOther"));
        }
        opts
    }
}
fn source() -> &'static Value {
    static SOURCE: OnceLock<Value> = OnceLock::new();
    SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .env(
                "SAFE_CEX_TYPESCRIPT_PATH",
                std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
                    // CI supplies an isolated compiler; this fallback only
                    // serves a local run from the repo root.
                    concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").to_owned()
                }),
            )
            .arg(format!(
                "{}/tests/runtime-assembly-source.cjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source oracle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn assert_result(h: &Host, result: JsResult<N>, expected: &Value, label: &str) {
    match result {
        Ok(v) => {
            assert!(
                expected["error"].is_null(),
                "{label} native unexpected success"
            );
            assert_native(&v, &expected["result"], label);
        }
        Err(e) => {
            assert!(
                !expected["error"].is_null(),
                "{label} native unexpected error {}",
                e.text()
            );
            if h.name == "builder-error" {
                assert!(
                    e.same(&h.builder_failure.borrow()),
                    "place/builder-error native typed rejection identity"
                );
                assert_eq!(
                    expected["builderOriginal"], true,
                    "source typed error identity"
                );
            }
            assert_native(&e, &expected["error"], label);
            assert_eq!(
                e.same(&h.failure),
                expected["original"].as_bool().unwrap(),
                "{label} native rejection identity"
            );
        }
    }
}
fn run_trading(group: &str, count: usize) {
    let rows = source()[group].as_array().unwrap();
    assert_eq!(rows.len(), count, "independent {group} case floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let label = format!("{group}/{name}");
        let h = Host::new(group, name);
        let task = if group == "place" {
            place_order(h.clone(), h.opts.clone())
        } else {
            fetch_fills_for_order_ids(h.clone(), h.ids())
        };
        assert_native(
            &N::array(h.events.borrow().clone()),
            &row["initial"],
            &format!("{label} eager effects"),
        );
        let result = h.finish(task);
        assert_result(&h, result, row, &label);
        assert_native(
            &N::array(h.events.borrow().clone()),
            &row["events"],
            &format!("{label} ordered effects"),
        );
        assert_native(&h.ex.get("grvtTradingAccountId"), &row["account"], &label);
        assert_native(&h.ex.get("wallet"), &row["wallet"], &label);
    }
}
#[test]
fn placement_native_source() {
    run_trading("place", 37)
}
#[test]
fn fills_native_source() {
    run_trading("fills", 29)
}
#[test]
fn signing_native_source() {
    let rows = source()["sign"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "independent sign case floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let h = Host::new("sign", name);
        let wallet = N::from(json!({"address":"0xBefore"}));
        let payload = N::from(json!({"nonce":7,"expiration":"exp"}));
        let host = h.clone();
        let identity = wallet.clone();
        let live = payload.clone();
        wallet.set(
            "signTypedData",
            h.f(move |r, a| {
                assert!(r.same(&identity), "sign native wallet receiver");
                assert!(a[2].same(&live), "sign native payload alias");
                host.event(vec![
                    host.string("sign"),
                    a[0].clone(),
                    a[1].clone(),
                    a[2].clone(),
                ]);
                if host.name == "sign-error" {
                    return Err(host.failure.clone());
                }
                if host.name == "live-payload" {
                    identity.set("address", host.string("0xAfter"));
                    live.set("nonce", host.number(23.));
                    live.set("expiration", host.string("live"));
                }
                Ok(host.string(&signature()))
            }),
        );
        let testnet = match name {
            "default" => None,
            "undefined" => Some(N::U),
            "null" => Some(N::Null),
            _ => Some(N::B(false)),
        };
        let task = sign_order(h.clone(), wallet, payload, testnet);
        let label = format!("sign/{name}");
        assert_native(
            &N::array(h.events.borrow().clone()),
            &row["initial"],
            &label,
        );
        assert_result(&h, h.finish(task), row, &label);
        assert_native(&N::array(h.events.borrow().clone()), &row["events"], &label);
    }
}
#[test]
fn private_key_native_source() {
    let rows = source()["key"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "independent key case floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let h = Host::new("key", name);
        let key = match name {
            "valid-prefix" => N::S(format!("0x{}", "01".repeat(32))),
            "valid-bare" => N::S("01".repeat(32)),
            "zero" => N::S(format!("0x{}", "00".repeat(32))),
            "max" => N::S(format!("0x{}", "ff".repeat(32))),
            "order" => {
                N::S("0xfffffffffffffffffffffffffffffffebaaedce6af48a03bbfd25e8cd0364141".into())
            }
            "short" => N::S("0x01".into()),
            "junk" => N::S("garbage".into()),
            _ => N::Null,
        };
        assert_eq!(
            validate_private_key(h.as_ref(), key),
            row["result"].as_bool().unwrap(),
            "key/{name} native scalar validity"
        );
    }
}
fn run_constructor(group: &str, count: usize) {
    let rows = source()[group].as_array().unwrap();
    assert_eq!(rows.len(), count, "independent constructor case floor");
    for row in rows {
        let name = row["name"].as_str().unwrap();
        let label = format!("{group}/{name}");
        let h = Host::new(group, name);
        let parent = N::from(json!({"tag":"parent"}));
        let opts = h.constructor_opts();
        let store = N::from(json!({"tag":"store"}));
        let mut rules = vec![];
        let result = if group == "exchange" {
            new_exchange(h.as_ref(), h.string("grvt"), opts.clone(), store.clone()).map(|owner| {
                assert!(
                    owner.base.get("options").same(&opts),
                    "exchange native original options"
                );
                assert!(
                    owner.base.get("store").same(&store),
                    "exchange native original store"
                );
                assert_eq!(
                    owner.fields.borrow().len(),
                    53,
                    "exchange native own declaration floor"
                );
                assert_eq!(
                    owner.methods.len(),
                    43,
                    "exchange native method floor including prototype"
                );
                for key in ["publicWebsocket", "privateWebsocket"] {
                    assert!(
                        h.get(&owner.base, key)
                            .unwrap()
                            .get("parent")
                            .same(&owner.base),
                        "exchange native child parent"
                    );
                }
                if name == "record-state" {
                    let retained = h.get(&owner.base, "_nonceToOrderId").unwrap();
                    let retained_times = h.get(&owner.base, "_nonceCreationTimes").unwrap();
                    owner.record_nonce("42", Some("real"), Some(1200.));
                    owner.record_nonce("42", Some("0x00"), Some(900.));
                    assert_native(
                        &retained,
                        &json!({"map":[["42","real"]]}),
                        "exchange native retained nonce view",
                    );
                    assert_native(
                        &retained_times,
                        &json!({"map":[["42",900]]}),
                        "exchange native retained creation view",
                    );
                    let set = h.get(&retained, "set").unwrap();
                    let written = h
                        .call(
                            set,
                            retained.clone(),
                            vec![h.string("temp"), h.string("write")],
                        )
                        .unwrap();
                    assert!(
                        written.same(&retained),
                        "exchange native Map.set returns retained receiver"
                    );
                    assert_eq!(
                        owner
                            .nonce_to_order_id
                            .borrow()
                            .get("temp")
                            .map(String::as_str),
                        Some("write"),
                        "exchange native view write aliases owner"
                    );
                    owner.nonce_to_order_id.borrow_mut().shift_remove("temp");
                    assert_eq!(
                        owner
                            .nonce_to_order_id
                            .borrow()
                            .get("42")
                            .map(String::as_str),
                        Some("real"),
                        "exchange native record cache"
                    );
                    assert_eq!(
                        owner.nonce_creation_times.borrow().get("42"),
                        Some(&Some(900.)),
                        "exchange native earliest nonce"
                    );
                }
                if name == "rules" {
                    rules = [
                        "1",
                        "9223372036854775807",
                        "9223372036854775808",
                        "0",
                        "01",
                        "1e3",
                        "-1",
                    ]
                    .iter()
                    .map(|v| N::B(owner.client_order_id_rule.is_valid(v)))
                    .collect();
                    assert_eq!(rules.len(), 7, "independent rule floor");
                }
                for key in EXCHANGE_MEMBERS.iter().chain(EXCHANGE_PROTOTYPE_MEMBERS) {
                    assert!(
                        matches!(owner.member(key), Some(N::F(_))),
                        "exchange native member binding {key}"
                    );
                }
                owner.base
            })
        } else if group == "public" {
            new_public_socket(h.as_ref(), parent.clone()).map(|owner| {
                assert!(
                    owner.parent.same(&parent) && owner.base.get("parent").same(&parent),
                    "public native parent alias"
                );
                assert_eq!(
                    owner.fields.borrow().len(),
                    27,
                    "public native own declaration floor"
                );
                assert_eq!(owner.methods.len(), 21, "public native method floor");
                if name == "state-binding" {
                    h.set(&owner.base, "requestId", N::Num(7.)).unwrap();
                    assert_eq!(
                        owner.state.request_id.get(),
                        7.,
                        "public native shared request state"
                    );
                    let retained = h.get(&owner.base, "tickerSubscriptions").unwrap();
                    owner
                        .state
                        .ticker_subscriptions
                        .borrow_mut()
                        .insert("BTC".into(), 9.);
                    assert_native(
                        &retained,
                        &json!({"map":[["BTC",9]]}),
                        "public native retained ticker view",
                    );
                    owner.state.ticker_subscriptions.borrow_mut().clear();
                    let timers = h.get(&owner.base, "tickerBatchTimers").unwrap();
                    owner.state.ticker_batch_timers.insert(17);
                    assert_native(
                        &timers,
                        &json!({"set":[17]}),
                        "public native retained timer view",
                    );
                    owner.state.ticker_batch_timers.remove(&17);
                    owner.state.ticker_batch_generation.set(11.);
                    assert_native(
                        &h.get(&owner.base, "tickerBatchGeneration").unwrap(),
                        &json!(11),
                        "public native generation alias",
                    );
                    owner.state.ticker_batch_generation.set(0.);
                }
                if name == "member-binding" {
                    let member = owner.member("mapIntervalToGrvt").unwrap();
                    let value = h
                        .call(
                            member,
                            N::from(json!({"foreign":true})),
                            vec![h.string("1m")],
                        )
                        .unwrap();
                    h.event(vec![
                        h.string("bound-member"),
                        h.string("mapIntervalToGrvt"),
                        value,
                    ]);
                }
                owner.base
            })
        } else {
            new_private_socket(h.as_ref(), parent.clone()).map(|owner| {
                assert!(
                    owner.parent.same(&parent) && owner.base.get("parent").same(&parent),
                    "private native parent alias"
                );
                assert_eq!(
                    owner.fields.borrow().len(),
                    37,
                    "private native own declaration floor"
                );
                assert_eq!(owner.methods.len(), 22, "private native method floor");
                if name == "state-binding" {
                    h.set(&owner.base, "requestId", N::Num(7.)).unwrap();
                    assert_eq!(
                        owner.state.request_id.get(),
                        7.,
                        "private native shared request state"
                    );
                    let retained = h.get(&owner.base, "processedFillIds").unwrap();
                    owner
                        .state
                        .processed_fill_ids
                        .borrow_mut()
                        .insert("fill".into());
                    assert_native(
                        &retained,
                        &json!({"set":["fill"]}),
                        "private native retained fill view",
                    );
                    owner.state.processed_fill_ids.borrow_mut().clear();
                    owner.state.is_disposed.set(true);
                    assert_native(
                        &h.get(&owner.base, "isDisposed").unwrap(),
                        &json!(true),
                        "private native inherited disposal alias",
                    );
                    owner.state.is_disposed.set(false);
                    owner.state.ping_at.set(42.);
                    assert_native(
                        &h.get(&owner.base, "pingAt").unwrap(),
                        &json!(42),
                        "private native inherited ping alias",
                    );
                    owner.state.ping_at.set(0.);
                }
                if name == "member-binding" {
                    let value = h
                        .call(
                            owner.member("getWatchdogStaleThresholdMs").unwrap(),
                            N::from(json!({"foreign":true})),
                            vec![],
                        )
                        .unwrap();
                    h.event(vec![
                        h.string("bound-member"),
                        h.string("getWatchdogStaleThresholdMs"),
                        value,
                    ]);
                }
                owner.base
            })
        };
        match result {
            Ok(base) => {
                assert!(
                    row["error"].is_null(),
                    "{label} native constructor unexpected success"
                );
                h.assert_instance(&base, &row["result"], &label, &mut vec![]);
                let keys = h.fields(&base).keys().cloned().map(N::S).collect();
                assert_native(
                    &N::array(keys),
                    &row["keys"],
                    &format!("{label} native field order"),
                );
                let methods = h
                    .fields(&base)
                    .iter()
                    .filter(|(_, v)| matches!(v, N::F(_)))
                    .map(|(k, _)| N::S(k.clone()))
                    .collect();
                assert_native(
                    &N::array(methods),
                    &row["methodNames"],
                    &format!("{label} native method order"),
                );
            }
            Err(e) => assert_result(&h, Err(e), row, &label),
        }
        assert_native(&N::array(rules), &row["rules"], &label);
        assert_native(
            &N::array(h.events.borrow().clone()),
            &row["events"],
            &format!("{label} native constructor effects"),
        );
    }
}
#[test]
fn exchange_constructor_native_source() {
    run_constructor("exchange", 15)
}
#[test]
fn public_constructor_native_source() {
    run_constructor("public", 4)
}
#[test]
fn private_constructor_native_source() {
    run_constructor("private", 4)
}
