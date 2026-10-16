use super::cache::{self, N, eq, object, s};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    json_boundaries::JsPayloadHost,
    spot_json::JsString,
    spot_rate_limiter::*,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Task = Pin<Box<dyn Future<Output = ()>>>;
type Cell = Rc<RefCell<Option<Result<N, N>>>>;
pub type NativeFuture = Pin<Box<dyn Future<Output = Result<N, N>>>>;
struct State {
    row: Value,
    tasks: Vec<Task>,
    promises: Vec<(N, Cell)>,
    resolves: BTreeMap<String, Cell>,
    callbacks: BTreeMap<String, SpotLimiterCallback<N>>,
    timers: Vec<(N, N)>,
    clock: usize,
    next: usize,
    token_reads: usize,
    coercions: usize,
    trace: Vec<N>,
    recursive_weight_alias: bool,
}
#[derive(Clone)]
pub struct Host {
    state: Rc<RefCell<State>>,
    pub limiter: N,
    pub weight: N,
    pub original_error: N,
    date: N,
}
pub fn input(v: &Value) -> N {
    match v {
        Value::Null => N::Null,
        Value::Bool(v) => N::B(*v),
        Value::Number(v) => N::Num(v.as_f64().unwrap()),
        Value::String(v) => match v.as_str() {
            "$undefined" => N::U,
            "$nan" => N::Num(f64::NAN),
            "$inf" => N::Num(f64::INFINITY),
            "$-inf" => N::Num(f64::NEG_INFINITY),
            "$-0" => N::Num(-0.),
            _ => s(v),
        },
        Value::Object(_) => object(vec![("nativeWeight", N::B(true))]),
        _ => panic!("limiter input"),
    }
}
pub fn own(v: &N, k: &str) -> N {
    let N::O(o) = v else {
        panic!("native own object")
    };
    o.borrow()
        .fields
        .iter()
        .find(|p| p.0 == k)
        .map_or(N::U, |p| p.1.clone())
}
impl Host {
    pub fn new(row: &Value) -> Self {
        let ctor = row["family"] == "constructor";
        let limiter = if ctor {
            object(vec![])
        } else {
            object(vec![
                ("tokens", input(&row["tokens"])),
                ("lastRefill", input(&row["last"])),
                ("capacity", input(&row["capacity"])),
            ])
        };
        let N::O(o) = &limiter else { unreachable!() };
        o.borrow_mut().prototype = Some(object(vec![
            ("refillTokens", N::Builtin("refill".into())),
            ("waitForToken", N::Builtin("wait".into())),
        ]));
        Self {
            limiter,
            weight: input(
                row.get("weight")
                    .unwrap_or(&Value::String("$undefined".into())),
            ),
            original_error: object(vec![("original", s("limiter failure"))]),
            date: object(vec![("now", N::Builtin("now".into()))]),
            state: Rc::new(RefCell::new(State {
                row: row.clone(),
                tasks: vec![],
                promises: vec![],
                resolves: BTreeMap::new(),
                callbacks: BTreeMap::new(),
                timers: vec![],
                clock: 0,
                next: 0,
                token_reads: 0,
                coercions: 0,
                trace: vec![],
                recursive_weight_alias: true,
            })),
        }
    }
    fn flag(&self, k: &str) -> bool {
        self.state.borrow().row[k].as_bool().unwrap_or(false)
    }
    pub fn record(&self, label: &str, args: &[N]) {
        let mut v = vec![s(label)];
        v.extend_from_slice(args);
        self.state.borrow_mut().trace.push(cache::arr(v));
    }
    pub fn trace(&self) -> N {
        cache::arr(self.state.borrow().trace.clone())
    }
    pub fn timers(&self) -> Vec<(N, N)> {
        self.state.borrow().timers.clone()
    }
    pub fn alias(&self) -> bool {
        self.state.borrow().recursive_weight_alias
    }
    pub fn error(&self) -> N {
        self.original_error.clone()
    }
    pub fn promise(&self, body: NativeFuture) -> N {
        let (v, cell) = self.empty_promise();
        let resolver = self.clone();
        self.start_task(Box::pin(async move {
            *cell.borrow_mut() = Some(resolver.settle_return(body.await).await)
        }));
        v
    }
    fn empty_promise(&self) -> (N, Cell) {
        let v = object(vec![]);
        let c = Rc::new(RefCell::new(None));
        self.state
            .borrow_mut()
            .promises
            .push((v.clone(), c.clone()));
        (v, c)
    }
    fn promise_cell(&self, v: &N) -> Option<Cell> {
        self.state
            .borrow()
            .promises
            .iter()
            .find(|p| eq(&p.0, v))
            .map(|p| p.1.clone())
    }
    pub fn result(&self, v: &N) -> Option<Result<N, N>> {
        self.promise_cell(v).unwrap().borrow().clone()
    }
    pub fn pending_tasks(&self) -> usize {
        self.state.borrow().tasks.len()
    }
    pub fn pump(&self) {
        // Fixed finite microtask observation floor; pending timers remain pending.
        for _ in 0..32 {
            let tasks = std::mem::take(&mut self.state.borrow_mut().tasks);
            let mut pending = vec![];
            let mut cx = Context::from_waker(Waker::noop());
            for mut task in tasks {
                if task.as_mut().poll(&mut cx).is_pending() {
                    pending.push(task)
                }
            }
            self.state.borrow_mut().tasks.extend(pending);
        }
    }
    pub fn fire(&mut self, index: usize) {
        let resolve = self
            .timers()
            .get(index)
            .expect("independent timer iteration")
            .0
            .clone();
        self.record("fire", &[N::Num(index as f64)]);
        self.call(&resolve, &N::U, &[]).unwrap();
        self.pump();
    }
    pub fn replace_recursive(&mut self) {
        let id = if self.flag("recursiveReject") {
            "replacement-reject"
        } else {
            "replacement"
        };
        self.set(
            &self.limiter.clone(),
            "waitForToken",
            &N::Builtin(id.into()),
        )
        .unwrap();
    }
    fn primitive(&mut self, v: &N) -> Result<N, N> {
        if eq(v, &self.weight) && matches!(v, N::O(_)) {
            let (n, mode) = {
                let mut st = self.state.borrow_mut();
                let n = st.coercions;
                st.coercions += 1;
                (n, st.row["weight"].clone())
            };
            self.record("coerce", &[N::Num(n as f64)]);
            if self.flag("coercionThrows") {
                return Err(self.error());
            }
            if self.flag("coercionSetsTokens") && n == 0 {
                self.set(&self.limiter.clone(), "tokens", &N::Num(7.))?;
            }
            let values = mode["values"].as_array().unwrap();
            return Ok(input(
                values.get(n).unwrap_or_else(|| values.last().unwrap()),
            ));
        }
        Ok(v.clone())
    }
}
struct Await {
    h: Host,
    value: N,
    yielded: bool,
}
impl Future for Await {
    type Output = Result<N, N>;
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if !self.yielded {
            self.yielded = true;
            cx.waker().wake_by_ref();
            return Poll::Pending;
        }
        for _ in 0..32 {
            let Some(c) = self.h.promise_cell(&self.value) else {
                return Poll::Ready(Ok(self.value.clone()));
            };
            let r = c.borrow().clone();
            match r {
                None => return Poll::Pending,
                Some(Err(e)) => return Poll::Ready(Err(e)),
                Some(Ok(v)) => self.value = v,
            }
        }
        panic!("native adoption floor exceeded31")
    }
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, mut task: Task) {
        let mut cx = Context::from_waker(Waker::noop());
        if task.as_mut().poll(&mut cx).is_pending() {
            self.state.borrow_mut().tasks.push(task)
        }
    }
    fn settle_return(&self, r: Result<N, N>) -> NativeFuture {
        match r {
            Err(e) => Box::pin(std::future::ready(Err(e))),
            Ok(v) => Box::pin(Await {
                h: self.clone(),
                value: v,
                yielded: false,
            }),
        }
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, t: &JsString) -> N {
        N::S(t.clone())
    }
    fn truthy(&self, v: &N) -> bool {
        !matches!(v, N::U | N::Null | N::B(false))
            && !matches!(v,N::Num(n)if *n==0.||n.is_nan())
            && !matches!(v,N::S(t)if t.0.is_empty())
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        self.record("global", &[s(k)]);
        Ok(if k == "Date" {
            self.date.clone()
        } else {
            N::Builtin(k.into())
        })
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if eq(v, &self.limiter) && self.state.borrow().row["family"] != "constructor" {
            self.record("get", &[s(k)]);
            if k == "refillTokens" && self.flag("refillThrows") {
                return Err(self.error());
            }
            if k == "tokens" {
                let n = {
                    let mut st = self.state.borrow_mut();
                    let n = st.token_reads;
                    st.token_reads += 1;
                    n
                };
                if self.flag("tokenReadChanges") && n == 1 {
                    return Ok(N::Num(9.));
                }
            }
        }
        if self.promise_cell(v).is_some() && k == "then" {
            self.record("get-then", &[]);
            if self.flag("thenThrows") {
                return Err(self.error());
            }
            return Ok(N::Builtin("then".into()));
        }
        JsPayloadHost::get(&mut cache::Host::default(), v, k).map_err(|_| self.error())
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        if eq(v, &self.limiter) && self.state.borrow().row["family"] != "constructor" {
            self.record("set", &[s(k), value.clone()]);
        }
        cache::Host::default()
            .put(v, k, value, true)
            .map_err(|_| self.error())
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn define_data(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        cache::Host::default()
            .put(v, k, value, true)
            .map_err(|_| self.error())
    }
    fn callback(&mut self, _: PingCallback<N>) -> Result<N, N> {
        panic!("not a ping callback")
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("native callee")
        };
        let capture = self.state.borrow().callbacks.get(id).cloned();
        if let Some(c) = capture {
            return run_spot_limiter_callback(self, &c, args);
        }
        let resolve = self.state.borrow().resolves.get(id).cloned();
        if let Some(c) = resolve {
            assert!(matches!(r, N::U), "LIMITER native timer resolve receiver");
            if c.borrow().is_none() {
                *c.borrow_mut() = Some(Ok(args.first().cloned().unwrap_or(N::U)))
            }
            return Ok(N::U);
        }
        match id.as_str() {
            "now" => {
                assert!(eq(r, &self.date), "LIMITER native Date.now receiver");
                let (i, clock) = {
                    let mut st = self.state.borrow_mut();
                    let i = st.clock;
                    st.clock += 1;
                    (i, st.row["clock"].clone())
                };
                if self.flag("clockThrows") {
                    self.record("now-throw", &[]);
                    return Err(self.error());
                }
                let values = clock.as_array().unwrap();
                let v = input(values.get(i).unwrap_or_else(|| values.last().unwrap()));
                self.record("now", &[v.clone()]);
                Ok(v)
            }
            "refill" => {
                assert!(eq(r, &self.limiter), "LIMITER native refill receiver");
                self.record("refill", &[]);
                spot_refill_tokens(self, r)?;
                Ok(N::U)
            }
            "wait" => {
                assert!(eq(r, &self.limiter), "LIMITER native recursive receiver");
                let weight = args.first().cloned().unwrap_or(N::U);
                self.state.borrow_mut().recursive_weight_alias &= if matches!(self.weight, N::O(_))
                {
                    eq(&weight, &self.weight)
                } else {
                    true
                };
                self.record("wait", &[]);
                let task = spot_wait_for_token(self.clone(), r.clone(), weight);
                Ok(self.promise(Box::pin(task)))
            }
            "setTimeout" => {
                assert!(matches!(r, N::U), "LIMITER native timer receiver");
                assert_eq!(args.len(), 2, "LIMITER native timer args");
                let callback = args[0].clone();
                assert!(
                    matches!(&callback,N::Builtin(id)if self.state.borrow().resolves.contains_key(id)),
                    "LIMITER native original resolve function"
                );
                self.record("timer", &[args[1].clone()]);
                if self.flag("timerThrows") {
                    return Err(self.error());
                }
                self.state
                    .borrow_mut()
                    .timers
                    .push((callback, args[1].clone()));
                Ok(N::Num(81.))
            }
            "then" => {
                assert_eq!(args.len(), 1);
                let p = r.clone();
                let callback = args[0].clone();
                let mut h = self.clone();
                Ok(self.promise(Box::pin(async move {
                    let v = h.settle_return(Ok(p)).await?;
                    h.call(&callback, &N::U, &[v])
                })))
            }
            "replacement" | "replacement-reject" => {
                assert!(eq(r, &self.limiter));
                assert_eq!(args.len(), 1);
                self.state.borrow_mut().recursive_weight_alias &= if matches!(self.weight, N::O(_))
                {
                    eq(&args[0], &self.weight)
                } else {
                    true
                };
                self.record("replacement", &[]);
                let r = if id == "replacement-reject" {
                    Err(self.error())
                } else {
                    Ok(s("adopted"))
                };
                Ok(self.promise(Box::pin(std::future::ready(r))))
            }
            _ => panic!("unexpected limiter callee {id}"),
        }
    }
}
impl SpotRateLimiterHost for Host {
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn binary(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        let a = self.primitive(a)?;
        let b = self.primitive(b)?;
        if op == ">=" {
            if let (N::S(a), N::S(b)) = (&a, &b) {
                return Ok(N::B(a.0 >= b.0));
            }
        }
        let (a, b) = (cache::num(&a), cache::num(&b));
        Ok(match op {
            "-" => N::Num(a - b),
            "/" => N::Num(a / b),
            "*" => N::Num(a * b),
            ">=" => N::B(a >= b),
            _ => panic!("native arithmetic"),
        })
    }
    fn construct(&mut self, ctor: &N, args: &[N]) -> Result<N, N> {
        assert!(
            matches!(ctor,N::Builtin(id)if id=="Promise"),
            "LIMITER native Promise ctor"
        );
        assert_eq!(args.len(), 1);
        self.record("construct", &[]);
        let (v, c) = self.empty_promise();
        let id = {
            let mut st = self.state.borrow_mut();
            let id = format!("resolve-{}", st.next);
            st.next += 1;
            st.resolves.insert(id.clone(), c.clone());
            id
        };
        if let Err(e) = self.call(
            &args[0],
            &N::U,
            &[N::Builtin(id), N::Builtin("unused-reject".into())],
        ) {
            if c.borrow().is_none() {
                *c.borrow_mut() = Some(Err(e))
            }
        }
        Ok(v)
    }
    fn limiter_callback(&mut self, c: SpotLimiterCallback<N>) -> Result<N, N> {
        let mut st = self.state.borrow_mut();
        let id = format!("limiter-callback-{}", st.next);
        st.next += 1;
        st.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
}
