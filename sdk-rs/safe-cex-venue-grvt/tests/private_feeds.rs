#![cfg(not(target_arch = "wasm32"))]
use indexmap::IndexMap;
use safe_cex_exchange::runtime::LogSeverity;
use safe_cex_venue_grvt::{
    builder_workflow::{JsPromise, JsResult},
    private_commands::PrivateCommands,
    private_feeds::*,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    hash::{Hash, Hasher},
    pin::Pin,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
#[derive(Clone)]
enum N {
    U,
    E(String),
    Null,
    B(bool),
    Num(f64),
    S(String),
    A(Rc<RefCell<Vec<N>>>),
    O(Rc<RefCell<IndexMap<String, N>>>),
}
impl N {
    fn from(v: Value) -> Self {
        match v {
            Value::Null => Self::Null,
            Value::Bool(v) => Self::B(v),
            Value::Number(v) => Self::Num(v.as_f64().unwrap()),
            Value::String(v) => Self::S(v),
            Value::Array(v) => Self::A(Rc::new(RefCell::new(
                v.into_iter().map(Self::from).collect(),
            ))),
            Value::Object(v) => Self::O(Rc::new(RefCell::new(
                v.into_iter().map(|(k, v)| (k, Self::from(v))).collect(),
            ))),
        }
    }
    fn object(fields: Vec<(&str, N)>) -> Self {
        Self::O(Rc::new(RefCell::new(
            fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        )))
    }
    fn get(&self, key: &str) -> N {
        match self {
            Self::O(v) => v.borrow().get(key).cloned().unwrap_or(Self::U),
            Self::A(v) if key == "length" => Self::Num(v.borrow().len() as f64),
            Self::A(v) => key
                .parse::<usize>()
                .ok()
                .and_then(|i| v.borrow().get(i).cloned())
                .unwrap_or(Self::U),
            _ => Self::U,
        }
    }
    fn set(&self, key: &str, value: N) {
        let Self::O(v) = self else { panic!("object") };
        v.borrow_mut().insert(key.into(), value);
    }
    fn same(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::O(a), Self::O(b)) => Rc::ptr_eq(a, b),
            (Self::A(a), Self::A(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
    fn text(&self) -> String {
        match self {
            Self::E(v) => v.clone(),
            Self::U => "undefined".into(),
            Self::Null => "null".into(),
            Self::B(v) => v.to_string(),
            Self::Num(v) => safe_cex_num::js_number_string(*v),
            Self::S(v) => v.clone(),
            Self::O(_) => "[object Object]".into(),
            Self::A(v) => v
                .borrow()
                .iter()
                .map(|v| {
                    if matches!(v, Self::U | Self::Null) {
                        String::new()
                    } else {
                        v.text()
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
        }
    }
}

fn scalar(v: f64, source: &Value, label: &str) {
    if source["nan"] == true {
        assert!(v.is_nan(), "{label} native NaN");
    } else {
        assert!(source["bits"].is_string(), "{label} native number shape");
        assert_eq!(
            format!("{:016x}", v.to_bits()),
            source["bits"].as_str().unwrap(),
            "{label}"
        );
    }
}
fn native(v: &N, source: &Value, label: &str) {
    match v {
        N::E(_) => panic!("error is inspected separately"),
        N::U => assert_eq!(source["undefined"], true, "{label} native own undefined"),
        N::Null => assert!(source.is_null(), "{label} native null"),
        N::B(v) => assert_eq!(Some(*v), source.as_bool(), "{label} native boolean"),
        N::Num(v) => scalar(*v, source, label),
        N::S(v) => assert_eq!(Some(v.as_str()), source.as_str(), "{label} native string"),
        N::A(v) => {
            let s = source.as_array().expect(label);
            assert_eq!(v.borrow().len(), s.len(), "{label} native array shape");
            for (v, s) in v.borrow().iter().zip(s) {
                native(v, s, label);
            }
        }
        N::O(v) => {
            let s = source.as_object().expect(label);
            let values = v.borrow();
            assert_eq!(values.len(), s.len(), "{label} native object shape");
            for (k, v) in values.iter() {
                assert!(s.contains_key(k), "{label} native own key {k}");
                native(v, &s[k], label);
            }
        }
    }
}
impl N {
    fn tagged(v: &Value) -> Self {
        if v.get("undefined") == Some(&Value::Bool(true)) {
            return Self::U;
        }
        if let Some(bits) = v.get("bits").and_then(Value::as_str) {
            return Self::Num(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()));
        }
        match v {
            Value::Array(v) => Self::A(Rc::new(RefCell::new(v.iter().map(Self::tagged).collect()))),
            Value::Object(v) => Self::O(Rc::new(RefCell::new(
                v.iter()
                    .map(|(k, v)| (k.clone(), Self::tagged(v)))
                    .collect(),
            ))),
            _ => Self::from(v.clone()),
        }
    }
    fn snapshot(&self) -> Self {
        match self {
            Self::A(v) => Self::A(Rc::new(RefCell::new(
                v.borrow().iter().map(Self::snapshot).collect(),
            ))),
            Self::O(v) => Self::O(Rc::new(RefCell::new(
                v.borrow()
                    .iter()
                    .map(|(k, v)| (k.clone(), v.snapshot()))
                    .collect(),
            ))),
            _ => self.clone(),
        }
    }
}
#[derive(Clone)]
struct Key(N);
impl PartialEq for Key {
    fn eq(&self, b: &Self) -> bool {
        match (&self.0, &b.0) {
            (N::U, N::U) | (N::Null, N::Null) => true,
            (N::Num(a), N::Num(b)) => a == b || a.is_nan() && b.is_nan(),
            (N::S(a), N::S(b)) | (N::E(a), N::E(b)) => a == b,
            (N::B(a), N::B(b)) => a == b,
            _ => self.0.same(&b.0),
        }
    }
}
impl Eq for Key {}
impl Hash for Key {
    fn hash<H: Hasher>(&self, h: &mut H) {
        std::mem::discriminant(&self.0).hash(h);
        match &self.0 {
            N::Num(v) => {
                let bits = if *v == 0.0 {
                    0
                } else if v.is_nan() {
                    f64::NAN.to_bits()
                } else {
                    v.to_bits()
                };
                bits.hash(h)
            }
            N::S(v) | N::E(v) => v.hash(h),
            N::B(v) => v.hash(h),
            N::A(v) => (Rc::as_ptr(v) as usize).hash(h),
            N::O(v) => (Rc::as_ptr(v) as usize).hash(h),
            _ => {}
        }
    }
}
struct Later {
    first: bool,
    f: Option<Box<dyn FnOnce() -> JsResult<N>>>,
}
impl Future for Later {
    type Output = JsResult<N>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.first {
            self.first = true;
            Poll::Pending
        } else {
            Poll::Ready(self.f.take().unwrap()())
        }
    }
}
struct Job {
    future: RefCell<Option<JsPromise<N>>>,
    result: RefCell<Option<JsResult<N>>>,
}
struct Handle {
    job: Rc<Job>,
    first: bool,
}
impl Future for Handle {
    type Output = JsResult<N>;
    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.first {
            self.first = true;
            return Poll::Pending;
        }
        self.job
            .result
            .borrow()
            .clone()
            .map(Poll::Ready)
            .unwrap_or(Poll::Pending)
    }
}
struct Host {
    me: RefCell<std::rc::Weak<Host>>,
    name: String,
    state: PrivateCommands<N, Key>,
    store: N,
    events: RefCell<Vec<N>>,
    fills: RefCell<Vec<N>>,
    nonces: RefCell<Vec<N>>,
    timers: RefCell<IndexMap<u64, TimerCallback>>,
    next_timer: Cell<u64>,
    clock: Cell<f64>,
    logs: Cell<usize>,
    jobs: RefCell<Vec<Rc<Job>>>,
}
impl Host {
    fn new(name: &str) -> Rc<Self> {
        let market = N::from(json!({"id":"BTC_USDT_Perp","symbol":"BTC/USDT"}));
        let store = N::object(vec![
            ("markets", N::A(Rc::new(RefCell::new(vec![market.clone()])))),
            (
                "marketsById",
                N::object(vec![("BTC_USDT_Perp", market.clone())]),
            ),
            ("orders", N::from(json!([{"id":"old"}]))),
            (
                "positions",
                N::from(
                    json!([{"symbol":"BTC/USDT","contracts":2,"notional":202,"unrealizedPnl":5,"liquidationPrice":40},{"symbol":"BTC/USDT","contracts":-3,"notional":303,"unrealizedPnl":6,"liquidationPrice":30},{"symbol":"ETH/USDT","contracts":1}]),
                ),
            ),
            (
                "loaded",
                N::from(json!({"markets":true,"orders":false,"positions":false})),
            ),
        ]);
        if name == "numeric-instrument" {
            store.get("marketsById").set("7", market);
        }
        if ["missing-market", "zero-missing-market"].contains(&name) {
            store.set("marketsById", N::object(vec![]));
            store.set("markets", N::from(json!([])));
        }
        if name == "new" {
            store.set("positions", N::from(json!([])));
        }
        if name == "empty-clear" {
            store.set("orders", N::from(json!([])));
        }
        let s = Rc::new(Self {
            me: RefCell::new(std::rc::Weak::new()),
            name: name.into(),
            state: PrivateCommands::default(),
            store,
            events: RefCell::new(vec![]),
            fills: RefCell::new(vec![]),
            nonces: RefCell::new(vec![]),
            timers: RefCell::new(IndexMap::new()),
            next_timer: Cell::new(1),
            clock: Cell::new(1000.0),
            logs: Cell::new(0),
            jobs: RefCell::new(vec![]),
        });
        *s.me.borrow_mut() = Rc::downgrade(&s);
        s.state.authenticated.set(true);
        s.state.is_disposed.set(name == "disposed");
        s.state.authentication_failed.set(name == "failed");
        if ["existing", "schedule-error"].contains(&name) {
            *s.state.orders_throttle_timeout.borrow_mut() = Some(N::Num(9.0));
            s.timers.borrow_mut().insert(9, Rc::new(|| {}));
        }
        if name == "falsy" {
            *s.state.orders_throttle_timeout.borrow_mut() = Some(N::Num(0.0));
        }
        if name == "capacity" {
            for i in 0..500 {
                s.state
                    .processed_fill_ids
                    .borrow_mut()
                    .insert(Key(N::S(format!("old-{i}"))));
            }
        }
        s
    }
    fn event(&self, values: Vec<N>) {
        self.events.borrow_mut().push(N::A(Rc::new(RefCell::new(
            values.into_iter().map(|v| v.snapshot()).collect(),
        ))));
    }
    fn named(&self, name: &str) {
        self.event(vec![N::S(name.into())]);
    }
    fn poll(&self, j: &Rc<Job>) {
        let Some(mut f) = j.future.borrow_mut().take() else {
            return;
        };
        match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(v) => *j.result.borrow_mut() = Some(v),
            Poll::Pending => *j.future.borrow_mut() = Some(f),
        }
    }
    fn drive(&self) {
        for _ in 0..1000 {
            let jobs = self.jobs.borrow().clone();
            if jobs.iter().all(|j| j.result.borrow().is_some()) {
                return;
            }
            for j in jobs {
                self.poll(&j);
            }
        }
        panic!("PRIVATE owned task did not settle");
    }
    fn truth(v: &N) -> bool {
        match v {
            N::U | N::Null => false,
            N::B(v) => *v,
            N::Num(v) => *v != 0.0 && !v.is_nan(),
            N::S(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn num(v: &N) -> f64 {
        match v {
            N::Num(v) => *v,
            N::U => f64::NAN,
            N::Null => 0.0,
            N::B(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            _ => safe_cex_num::number_from_string(&v.text()),
        }
    }
    fn null_default(v: N, fallback: impl FnOnce() -> N) -> N {
        if matches!(v, N::U | N::Null) {
            fallback()
        } else {
            v
        }
    }
    fn trigger(&self, feed: &N) -> N {
        let metadata = Self::null_default(feed.get("metadata"), || feed.get("m"));
        let trigger = Self::null_default(metadata.get("trigger"), || metadata.get("t"));
        if !Self::truth(&trigger) {
            return N::U;
        }
        let kind = Self::null_default(trigger.get("trigger_type"), || trigger.get("tt"));
        if !["STOP_LOSS", "TAKE_PROFIT"].contains(&kind.text().as_str()) {
            return N::U;
        }
        let t = Self::null_default(trigger.get("tpsl"), || trigger.get("t"));
        let raw = Self::null_default(t.get("trigger_price"), || t.get("tp"));
        let num = Self::num(&raw);
        let by = Self::null_default(t.get("trigger_by"), || t.get("tb"));
        let by = match by.text().as_str() {
            "LAST" => N::S("last_price".into()),
            "MARK" => N::S("mark_price".into()),
            "INDEX" => N::S("index_price".into()),
            _ => N::U,
        };
        let result = N::object(vec![
            ("type", kind),
            (
                "triggerPrice",
                if num.is_finite() { N::Num(num) } else { N::U },
            ),
            ("triggerType", by),
        ]);
        if let v @ N::B(_) = Self::null_default(t.get("close_position"), || t.get("cp")) {
            result.set("close", v);
        }
        if let v @ N::B(_) = Self::null_default(t.get("is_split_position"), || t.get("is")) {
            result.set("isSplitPosition", v);
        }
        result
    }
}
impl PrivateFeedHost for Host {
    type Value = N;
    type FillKey = Key;
    fn state(&self) -> &PrivateCommands<N, Key> {
        &self.state
    }
    fn start(&self, f: JsPromise<N>) -> JsPromise<N> {
        let j = Rc::new(Job {
            future: RefCell::new(Some(f)),
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(j.clone());
        self.poll(&j);
        Box::pin(Handle {
            job: j,
            first: false,
        })
    }
    fn property(&self, v: &N, k: &str) -> JsResult<N> {
        if matches!(v, N::U | N::Null) {
            Err(N::E(format!(
                "TypeError: Cannot read properties of {} (reading '{k}')",
                v.text()
            )))
        } else {
            Ok(v.get(k))
        }
    }
    fn indexed(&self, v: &N, k: &N) -> JsResult<N> {
        self.property(v, &k.text())
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
        Self::truth(v)
    }
    fn nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn is_number(&self, v: &N) -> bool {
        matches!(v, N::Num(_))
    }
    fn is_string(&self, v: &N) -> bool {
        matches!(v, N::S(_))
    }
    fn is_array(&self, v: &N) -> bool {
        matches!(v, N::A(_))
    }
    fn equal(&self, a: &N, b: &N) -> bool {
        match (a, b) {
            (N::Num(a), N::Num(b)) => a == b,
            _ => Key(a.clone()) == Key(b.clone()),
        }
    }
    fn loose_number_equal(&self, v: &N, num: f64) -> JsResult<N, bool> {
        Ok(!matches!(v, N::U | N::Null) && Self::num(v) == num)
    }
    fn number_value(&self, v: &N) -> JsResult<N, f64> {
        Ok(Self::num(v))
    }
    fn parse_float(&self, v: &N) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_float(&v.text()))
    }
    fn parse_int(&self, v: &N) -> JsResult<N, f64> {
        Ok(safe_cex_num::parse_int(&v.text(), None))
    }
    fn decimal_pattern(&self, v: &N) -> JsResult<N, bool> {
        let text = v.text();
        let text = text.as_str();
        let mut parts = text.split('.');
        let a = parts.next().unwrap();
        let b = parts.next();
        Ok(parts.next().is_none()
            && match b {
                None => !a.is_empty() && a.bytes().all(|c| c.is_ascii_digit()),
                Some(b) => {
                    !b.is_empty()
                        && b.bytes().all(|c| c.is_ascii_digit())
                        && a.bytes().all(|c| c.is_ascii_digit())
                }
            })
    }
    fn to_string(&self, v: &N) -> JsResult<N> {
        Ok(N::S(v.text()))
    }
    fn template(&self, v: Vec<N>) -> JsResult<N> {
        let mut text = String::new();
        for value in v {
            if let N::O(o) = &value {
                if o.borrow().contains_key("toString") {
                    return Err(N::E(
                        "TypeError: Cannot convert object to primitive value".into(),
                    ));
                }
            }
            text.push_str(&value.text());
        }
        Ok(N::S(text))
    }
    fn uppercase(&self, v: &N, expression: &'static str) -> JsResult<N> {
        if let N::S(s) = v {
            Ok(N::S(s.to_uppercase()))
        } else {
            Err(N::E(format!(
                "TypeError: {expression}.toUpperCase is not a function"
            )))
        }
    }
    fn keys_join(&self, v: &N) -> JsResult<N> {
        self.named("keys");
        let N::O(o) = v else { panic!() };
        Ok(N::S(
            o.borrow().keys().cloned().collect::<Vec<_>>().join(","),
        ))
    }
    fn object(&self, v: Vec<(&'static str, N)>) -> N {
        N::object(v)
    }
    fn define(&self, v: &N, k: &'static str, f: N) -> JsResult<N, ()> {
        v.set(k, f);
        Ok(())
    }
    fn spread(&self, target: &N, source: &N) -> JsResult<N, ()> {
        if let N::O(s) = source {
            for (k, v) in s.borrow().iter() {
                target.set(k, v.clone());
            }
        }
        Ok(())
    }
    fn find(&self, a: &N, f: &mut Predicate<'_, N>) -> JsResult<N> {
        let N::A(a) = a else { panic!("array") };
        let len = a.borrow().len();
        for i in 0..len {
            let v = a.borrow().get(i).cloned().unwrap_or(N::U);
            if f(v.clone())? {
                return Ok(v);
            }
        }
        Ok(N::U)
    }
    fn filter(&self, a: &N, f: &mut Predicate<'_, N>) -> JsResult<N> {
        let N::A(a) = a else { panic!("array") };
        let len = a.borrow().len();
        let mut out = vec![];
        for i in 0..len {
            let v = a.borrow().get(i).cloned();
            if let Some(v) = v {
                if f(v.clone())? {
                    out.push(v);
                }
            }
        }
        Ok(N::A(Rc::new(RefCell::new(out))))
    }
    fn for_each(&self, a: &N, f: &mut Visitor<'_, N>) -> JsResult<N, ()> {
        let N::A(a) = a else { panic!() };
        let len = a.borrow().len();
        for i in 0..len {
            let v = a.borrow().get(i).cloned();
            if let Some(v) = v {
                f(v)?;
            }
        }
        Ok(())
    }
    fn append_spread(&self, a: &N, v: N) -> JsResult<N> {
        let N::A(a) = a else { panic!() };
        let mut out = a.borrow().clone();
        out.push(v);
        Ok(N::A(Rc::new(RefCell::new(out))))
    }
    fn store(&self) -> JsResult<N> {
        Ok(self.store.clone())
    }
    fn json_parse(&self, v: &N) -> JsResult<N> {
        Ok(serde_json::from_str::<Value>(&v.text())
            .map(N::from)
            .unwrap_or(N::Null))
    }
    fn now(&self) -> JsResult<N, f64> {
        self.named("now");
        Ok(1700000000000.0)
    }
    fn virtual_now(&self) -> JsResult<N, f64> {
        self.named("virtual-now");
        let v = self.clock.get();
        self.clock.set(v + 1.0);
        Ok(v)
    }
    fn position_id(&self, s: &N) -> JsResult<N> {
        Ok(s.clone())
    }
    fn trigger_metadata(&self, f: &N) -> JsResult<N> {
        Ok(self.trigger(f))
    }
    fn time_in_force(&self, f: &N) -> JsResult<N> {
        let po = Self::null_default(f.get("post_only"), || f.get("po"));
        if !matches!(po, N::U | N::B(_)) {
            return Ok(N::U);
        }
        let kind = Self::null_default(f.get("time_in_force"), || f.get("ti"));
        Ok(match kind.text().as_str() {
            "GOOD_TILL_TIME" => N::S(
                if Self::truth(&po) {
                    "PostOnly"
                } else {
                    "GoodTillCancel"
                }
                .into(),
            ),
            "IMMEDIATE_OR_CANCEL" if !Self::truth(&po) => N::S("ImmediateOrCancel".into()),
            "FILL_OR_KILL" if !Self::truth(&po) => N::S("FillOrKill".into()),
            _ => N::U,
        })
    }
    fn order_creation_time(&self, m: &N) -> JsResult<N> {
        self.named("creation-time");
        Ok(if Self::truth(&m.get("create_time")) {
            N::Num((safe_cex_num::parse_int(&m.get("create_time").text(), None) / 1e6).floor())
        } else {
            N::U
        })
    }
    fn record_nonce(&self, id: N, real: N, time: N) -> JsResult<N, ()> {
        self.event(vec![
            N::S("nonce".into()),
            id.clone(),
            real.clone(),
            time.clone(),
        ]);
        self.nonces
            .borrow_mut()
            .push(N::object(vec![("id", id), ("real", real), ("time", time)]));
        if self.name == "nonce-error" {
            Err(N::E("Error: nonce-fail".into()))
        } else {
            Ok(())
        }
    }
    fn fill_key(&self, v: &N) -> Key {
        Key(v.clone())
    }
    fn log(&self, m: N, severity: Option<LogSeverity>) -> JsResult<N, ()> {
        self.event(vec![
            N::S("log".into()),
            m,
            severity.map(|s| N::S(s.as_str().into())).unwrap_or(N::U),
        ]);
        self.logs.set(self.logs.get() + 1);
        if self.name == "catch-log-error" && self.logs.get() == 1 {
            Err(N::E("Error: catch-log-fail".into()))
        } else {
            Ok(())
        }
    }
    fn emit_error(&self, m: N) -> JsResult<N, ()> {
        self.event(vec![N::S("error".into()), m]);
        Ok(())
    }
    fn emit_fill(&self, f: N) -> JsResult<N, ()> {
        self.event(vec![N::S("emit".into()), N::S("fill".into())]);
        self.fills.borrow_mut().push(f);
        if self.name == "emit-error-retry" && self.fills.borrow().len() == 1 {
            Err(N::E("Error: emit-fail".into()))
        } else {
            Ok(())
        }
    }
    fn record_stats(&self, t: &'static str) -> JsResult<N, ()> {
        self.event(vec![N::S("stats".into()), N::S(t.into())]);
        Ok(())
    }
    fn track_received(&self) -> JsResult<N, ()> {
        self.named("track");
        if self.name == "track-error" {
            Err(N::E("Error: track-fail".into()))
        } else {
            Ok(())
        }
    }
    fn pong(&self) -> JsResult<N, ()> {
        self.named("pong");
        Ok(())
    }
    fn subscribe(&self) -> JsResult<N, ()> {
        self.named("subscribe");
        Ok(())
    }
    fn refresh_session(&self, r: &'static str) -> JsResult<N, ()> {
        self.event(vec![N::S("refresh".into()), N::S(r.into())]);
        Ok(())
    }
    fn update_order_status(&self, s: N, t: N) -> JsResult<N, ()> {
        self.event(vec![N::S("status".into()), s, t]);
        Ok(())
    }
    fn remove_order(&self, s: N) -> JsResult<N, ()> {
        self.event(vec![N::S("remove".into()), s.clone()]);
        if self.name == "remove-error" {
            return Err(N::E("Error: remove-fail".into()));
        }
        let N::A(orders) = self.store.get("orders") else {
            panic!()
        };
        let kept = orders
            .borrow()
            .iter()
            .filter(|o| !self.equal(&o.get("id"), &s.get("id")))
            .cloned()
            .collect();
        self.store.set("orders", N::A(Rc::new(RefCell::new(kept))));
        Ok(())
    }
    fn add_or_update_order(&self, o: N) -> JsResult<N, ()> {
        self.event(vec![N::S("add-order".into()), o.clone()]);
        let N::A(a) = self.store.get("orders") else {
            panic!()
        };
        a.borrow_mut().push(o);
        Ok(())
    }
    fn update_position(&self, p: N, c: N) -> JsResult<N, ()> {
        self.event(vec![N::S("patch-position".into()), p.clone(), c.clone()]);
        self.spread(&p, &c)?;
        if self.name == "subscriber-mutation"
            && self
                .events
                .borrow()
                .iter()
                .filter(|e| e.get("0").text() == "patch-position")
                .count()
                == 1
        {
            self.store.set("positions", N::from(json!([])));
        }
        Ok(())
    }
    fn update_store(&self, c: N) -> JsResult<N, ()> {
        self.event(vec![N::S("store".into()), c.clone()]);
        self.spread(&self.store, &c)
    }
    fn sync_upnl(&self) -> JsResult<N, ()> {
        self.named("sync");
        Ok(())
    }
    fn throttle_positions(&self) -> JsResult<N, ()> {
        self.named("throttle-positions");
        Ok(())
    }
    fn schedule(&self, f: TimerCallback, d: f64) -> JsResult<N> {
        self.event(vec![N::S("schedule".into()), N::Num(d)]);
        if self.name == "schedule-error" {
            return Err(N::E("Error: schedule-fail".into()));
        }
        let id = self.next_timer.get();
        self.next_timer.set(id + 1);
        self.timers.borrow_mut().insert(id, f);
        Ok(N::Num(id as f64))
    }
    fn clear_timeout(&self, t: &N) -> JsResult<N, ()> {
        self.event(vec![N::S("clear".into()), t.clone()]);
        self.timers
            .borrow_mut()
            .shift_remove(&(Self::num(t) as u64));
        Ok(())
    }
    fn fetch_orders(&self) -> JsResult<N, JsPromise<N>> {
        self.named("fetch");
        let h = self.me.borrow().upgrade().unwrap();
        Ok(Box::pin(Later {
            first: false,
            f: Some(Box::new(move || {
                if ["fetch-error", "catch-log-error"].contains(&h.name.as_str()) {
                    return Err(N::E("Error: fetch-fail".into()));
                }
                if h.name == "loaded-mutation" {
                    h.store.get("loaded").set("extra", N::B(true));
                    h.store.set("orders", N::from(json!([])));
                }
                if h.name == "dispose-during" {
                    h.state.is_disposed.set(true);
                }
                Ok(N::from(if h.name.starts_with("empty-") {
                    json!([])
                } else {
                    json!([{"id":"new"}])
                }))
            })),
        }))
    }
}
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private-feeds-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "PRIVATE source oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn cases(g: &str) -> Vec<&'static str> {
    match g {
        "decimal" => vec![
            "number",
            "decimal",
            "fraction",
            "zero",
            "negative",
            "exponent",
            "trailing-dot",
            "space",
            "nan",
            "infinity",
            "null",
            "boolean",
            "array",
            "newline",
            "overflow",
        ],
        "order" => vec![
            "full",
            "lite",
            "placeholder",
            "missing-state",
            "missing-market",
            "missing-id",
            "numeric-instrument",
            "market",
            "false-aliases",
            "stop",
            "take",
            "nan-amount",
            "nonce-error",
            "numeric-no-market",
        ],
        "position" => vec![
            "full",
            "lite",
            "zero",
            "missing-size",
            "invalid-size",
            "missing-market",
            "numeric-instrument",
            "missing-instrument",
            "null-feed",
            "numeric-no-market",
        ],
        "orderHandler" => vec![
            "open",
            "closed",
            "canceled",
            "no-feed",
            "parse-null",
            "remove-error",
            "raw-id-object",
        ],
        "positionHandler" => vec![
            "existing",
            "new",
            "zero",
            "zero-lite",
            "parse-null",
            "no-feed",
            "zero-missing-market",
            "subscriber-mutation",
        ],
        "fillHandler" => vec![
            "full",
            "lite",
            "invalid-size",
            "duplicate",
            "numeric-string",
            "unknown-then-known",
            "emit-error-retry",
            "fallback",
            "capacity",
            "missing-client",
            "fallback-object",
            "log-object",
        ],
        "message" => vec![
            "disposed",
            "failed",
            "pong",
            "ping",
            "malformed",
            "auth-success",
            "proxy-error",
            "confirmation-precedence",
            "nested-1000",
            "nested-string",
            "numeric-error",
            "noise",
            "string-noise",
            "whitelist",
            "unknown-stream",
            "order",
            "position",
            "fill",
            "track-error",
        ],
        "debounce" => vec![
            "fresh",
            "existing",
            "falsy",
            "disposed",
            "schedule-error",
            "fire-callback",
        ],
        _ => vec![
            "full",
            "disposed",
            "empty-keep",
            "empty-clear",
            "fetch-error",
            "loaded-mutation",
            "dispose-during",
            "catch-log-error",
        ],
    }
}
fn count(g: &str, i: usize, kind: &str) -> usize {
    match kind {
        "fills" => match g {
            "fillHandler" => [1, 1, 0, 1, 3, 1, 1, 1, 1, 1, 0, 0][i],
            "message" => usize::from(i == 17),
            _ => 0,
        },
        "nonces" => match g {
            "order" => [1, 1, 0, 1, 0, 0, 1, 1, 1, 1, 1, 1, 1, 0][i],
            "orderHandler" => [1, 1, 1, 0, 0, 1, 0][i],
            "message" => usize::from(i == 15),
            _ => 0,
        },
        "dedup" => match g {
            "fillHandler" => [1, 1, 0, 1, 3, 2, 1, 1, 500, 1, 0, 1][i],
            "message" => usize::from(i == 17),
            _ => 0,
        },
        "timers" => match g {
            "orderHandler" => [1, 1, 1, 0, 0, 0, 0][i],
            "positionHandler" => usize::from(i == 0),
            "message" => usize::from(i == 15),
            "debounce" => [1, 1, 1, 1, 0, 0][i],
            _ => 0,
        },
        _ => panic!(),
    }
}
fn array(v: Vec<N>) -> N {
    N::A(Rc::new(RefCell::new(v)))
}
fn invoke(h: &Rc<Host>, g: &str, v: &N) -> JsResult<N> {
    match g {
        "decimal" => {
            parse_positive_finite_decimal(h.as_ref(), v).map(|v| v.map(N::Num).unwrap_or(N::U))
        }
        "order" => parse_order(h.as_ref(), v.clone()),
        "position" => parse_position(h.as_ref(), v.clone()),
        "orderHandler" => handle_order_update(h.clone(), v.clone()).map(|()| N::U),
        "positionHandler" => handle_position_update(h.as_ref(), v.clone()).map(|()| N::U),
        "fillHandler" => {
            handle_fill_update(h.as_ref(), v.clone())?;
            if ["duplicate", "emit-error-retry"].contains(&h.name.as_str()) {
                handle_fill_update(h.as_ref(), v.clone())?;
            }
            if h.name == "numeric-string" {
                v.get("feed").set("trade_id", N::Num(1.0));
                handle_fill_update(h.as_ref(), v.clone())?;
                v.get("feed").set("trade_id", N::S("1".into()));
                handle_fill_update(h.as_ref(), v.clone())?;
            }
            if h.name == "unknown-then-known" {
                v.get("feed").set("trade_id", N::S("unknown".into()));
                v.get("feed").set("instrument", N::S("missing".into()));
                handle_fill_update(h.as_ref(), v.clone())?;
                v.get("feed")
                    .set("instrument", N::S("BTC_USDT_Perp".into()));
                handle_fill_update(h.as_ref(), v.clone())?;
            }
            Ok(N::U)
        }
        "message" => on_message(h.clone(), v.clone()).map(|()| N::U),
        "debounce" => throttled_fetch_orders(h.clone()).map(|()| N::U),
        _ => {
            drop(fetch_throttled_orders(h.clone()));
            Ok(N::U)
        }
    }
}
fn matrix(g: &str) {
    let names = cases(g);
    let rows = corpus()[g].as_array().unwrap();
    let floor = match g {
        "decimal" => 15,
        "order" => 14,
        "position" => 10,
        "orderHandler" => 7,
        "positionHandler" => 8,
        "fillHandler" => 12,
        "message" => 19,
        "debounce" => 6,
        _ => 8,
    };
    assert_eq!(names.len(), floor, "PRIVATE independent case floor");
    assert_eq!(rows.len(), floor, "PRIVATE source case floor");
    for (i, row) in rows.iter().enumerate() {
        assert_eq!(row["name"], names[i], "PRIVATE case order");
        let h = Host::new(names[i]);
        let v = N::tagged(&row["input"]);
        let mut result = invoke(&h, g, &v);
        native(
            &array(h.events.borrow().clone()),
            &row["initial"],
            "PRIVATE native eager prefix",
        );
        if g == "debounce" && names[i] == "fire-callback" {
            let callback = h.timers.borrow_mut().shift_remove_index(0).unwrap().1;
            callback();
        }
        h.drive();
        if g == "fetch" {
            result = h.jobs.borrow()[0].result.borrow().clone().unwrap();
        }
        match result {
            Ok(value) => {
                assert!(row["error"].is_null(), "PRIVATE unexpected native success");
                native(
                    &value,
                    &row["result"],
                    match g {
                        "decimal" => "DECIMAL native value",
                        "order" => "ORDER native record",
                        "position" => "POSITION native record",
                        _ => "HANDLER native result",
                    },
                );
                if g == "order" && matches!(&value, N::O(_)) {
                    assert!(
                        value.get("info").same(&v),
                        "ORDER native original feed identity"
                    );
                    assert_eq!(
                        row["infoIdentity"], true,
                        "ORDER source original feed identity"
                    );
                    let N::O(o) = &value else { panic!() };
                    assert_eq!(
                        o.borrow().len(),
                        if ["stop", "take"].contains(&names[i]) {
                            15
                        } else {
                            13
                        },
                        "ORDER independent native field count"
                    );
                }
                if g == "position" && matches!(&value, N::O(_)) {
                    let N::O(o) = &value else { panic!() };
                    assert_eq!(
                        o.borrow().len(),
                        9,
                        "POSITION independent native field count"
                    );
                }
            }
            Err(N::E(e)) => {
                let (name, message) = e.split_once(": ").unwrap();
                assert_eq!(
                    Some(name),
                    row["error"]["name"].as_str(),
                    "PRIVATE native error name"
                );
                assert_eq!(
                    Some(message),
                    row["error"]["message"].as_str(),
                    "PRIVATE native error message"
                );
            }
            Err(_) => panic!("PRIVATE unsupported raw fixture exception"),
        }
        for (k, values) in [
            ("fills", h.fills.borrow().clone()),
            ("nonces", h.nonces.borrow().clone()),
            (
                "dedup",
                h.state
                    .processed_fill_ids
                    .borrow()
                    .iter()
                    .map(|k| k.0.clone())
                    .collect(),
            ),
        ] {
            let expected = count(g, i, k);
            assert_eq!(
                values.len(),
                expected,
                "PRIVATE independent native {k} count"
            );
            assert_eq!(
                row[k].as_array().unwrap().len(),
                expected,
                "PRIVATE independent source {k} count"
            );
            native(
                &array(values),
                &row[k],
                match k {
                    "fills" => "FILL native emitted value",
                    "nonces" => "ORDER native nonce effect",
                    _ => "FILL native ordered dedup keys",
                },
            );
        }
        assert_eq!(
            h.timers.borrow().len(),
            count(g, i, "timers"),
            "TIMER independent native count"
        );
        assert_eq!(
            row["timers"].as_u64().unwrap() as usize,
            count(g, i, "timers"),
            "TIMER independent source count"
        );
        native(
            &h.state
                .orders_throttle_timeout
                .borrow()
                .clone()
                .unwrap_or(N::U),
            &row["timer"],
            "TIMER native slot",
        );
        native(
            &h.store.get("orders"),
            &row["orders"],
            "STORE native orders",
        );
        native(
            &h.store.get("positions"),
            &row["positions"],
            "STORE native positions",
        );
        native(
            &h.store.get("loaded"),
            &row["loaded"],
            "STORE native live loaded flags",
        );
        assert_eq!(
            row["pending"],
            h.state.has_pending_position_events.get(),
            "POSITION native pending flag"
        );
        assert_eq!(
            row["disposed"],
            h.state.is_disposed.get(),
            "PRIVATE native disposed state"
        );
        assert_eq!(
            row["authenticated"],
            h.state.authenticated.get(),
            "PRIVATE native authenticated state"
        );
        native(
            &h.state
                .private_stream_healthy_at
                .get()
                .map(N::Num)
                .unwrap_or(N::U),
            &row["healthy"],
            "PRIVATE native healthy timestamp",
        );
        native(
            &array(h.events.borrow().clone()),
            &row["events"],
            "PRIVATE native operation order",
        );
        h.timers.borrow_mut().clear();
    }
}
#[test]
fn positive_decimal_native() {
    matrix("decimal")
}
#[test]
fn order_parser_native() {
    matrix("order")
}
#[test]
fn position_parser_native() {
    matrix("position")
}
#[test]
fn order_handler_native() {
    matrix("orderHandler")
}
#[test]
fn position_handler_native() {
    matrix("positionHandler")
}
#[test]
fn fill_handler_native() {
    matrix("fillHandler")
}
#[test]
fn message_native() {
    matrix("message")
}
#[test]
fn order_debounce_native() {
    matrix("debounce")
}
#[test]
fn order_fetch_native() {
    matrix("fetch")
}
