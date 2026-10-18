#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use indexmap::IndexMap;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::*,
};
use safe_cex_store::store::DefaultStore;
use safe_cex_types::{ExchangeName, OrderSide, OrderType, PlaceOrderOpts};
use safe_cex_webworker::chaser::ChaserOrderInstance;
use safe_cex_webworker::task_types::*;
use safe_cex_webworker::twap_instance::*;
use safe_cex_webworker::twap_manager::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
// Test-harness event log, not production state: the variants record what the
// lifecycle did, so bounded enum storage buys nothing here and boxing one
// variant would churn every construction site in this file. The production
// equivalent (ChaserCancel in src/chaser_engine.rs) IS boxed.
#[derive(Clone)]
enum Event {
    Update(TwapState),
    Post(IndexMap<String, (TwapOptions, TwapState)>),
    Order(PlaceOrderOpts),
    Log(String, String),
    Timer(u64, f64),
    Clear(u64),
    Fired(u64),
    Chaser(String, ChaserOrderOptions),
    RemoveChaser(String),
}
struct Clock {
    inner: runtime::Runtime,
    draw: Cell<f64>,
    ids: Cell<u64>,
    random_hook: RefCell<Option<Rc<dyn Fn()>>>,
}
impl ExchangeRuntime for Clock {
    fn monotonic_now_ms(&self) -> f64 {
        self.inner.monotonic_now_ms()
    }
    fn now_ms(&self) -> f64 {
        self.inner.now_ms()
    }
    fn random(&self) -> f64 {
        let hook = self.random_hook.borrow().clone();
        if let Some(hook) = hook {
            hook();
        }
        self.draw.get()
    }
    fn unique_id(&self) -> String {
        let id = self.ids.get();
        self.ids.set(id + 1);
        format!("generated-{id}")
    }
    fn sleep(&self, d: u64) -> ExchangeFuture<'static, ()> {
        self.inner.sleep(d)
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.inner.spawn_local(f)
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn is_worker(&self) -> bool {
        true
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, e: &ExchangeError, v: &Value) {
        self.inner.report_exception(e, v)
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("no worker bridge")
    }
}
struct Venue {
    base: BaseExchange,
    caps: ExchangeCapabilities,
}
impl Exchange for Venue {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.caps
    }
}
struct Host {
    clock: Rc<Clock>,
    manager: Rc<TwapManager>,
    exchange: Rc<dyn Exchange>,
    events: Rc<RefCell<Vec<Event>>>,
    timers: RefCell<IndexMap<u64, (Rc<dyn Fn()>, f64)>>,
    next: Cell<u64>,
    outcome: Cell<&'static str>,
    released: Rc<Cell<bool>>,
    spawned: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    chasers: RefCell<Vec<Rc<ChaserOrderInstance>>>,
    now_reads: RefCell<VecDeque<f64>>,
    post_failure: Cell<bool>,
    remove_hook: RefCell<Option<Rc<dyn Fn(&Rc<ChaserOrderInstance>)>>>,
}
impl WorkerPortTimers for Host {
    type Timer = u64;
    fn now_ms(&self) -> f64 {
        self.now_reads
            .borrow_mut()
            .pop_front()
            .unwrap_or(self.clock.now_ms())
    }
    fn set_timeout(&self, fnc: Rc<dyn Fn()>, delay: f64) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.timers.borrow_mut().insert(id, (fnc, delay));
        self.events.borrow_mut().push(Event::Timer(id, delay));
        id
    }
    fn clear_timeout(&self, id: &u64) {
        self.timers.borrow_mut().shift_remove(id);
        self.events.borrow_mut().push(Event::Clear(*id));
    }
    fn timer_is_truthy(&self, id: &u64) -> bool {
        *id != 0
    }
}
impl TwapHost for Host {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.clock.clone()
    }
    fn manager(&self) -> Rc<TwapManager> {
        self.manager.clone()
    }
    fn post_state(&self, states: IndexMap<String, SerializedTwap>) -> Result<(), ExchangeError> {
        let states = states
            .into_iter()
            .map(|(k, v)| (k, (v.options.borrow().clone(), v.state.borrow().clone())))
            .collect();
        self.events.borrow_mut().push(Event::Post(states));
        if self.post_failure.get() {
            Err(ExchangeError::new("post failed"))
        } else {
            Ok(())
        }
    }
    fn place_order(
        &self,
        exchange: Rc<dyn Exchange>,
        order: PlaceOrderOpts,
    ) -> Result<ExchangeFuture<'static, Vec<String>>, ExchangeError> {
        assert!(
            Rc::ptr_eq(&exchange, &self.exchange),
            "TWAP native venue identity"
        );
        self.events.borrow_mut().push(Event::Order(order));
        let mode = self.outcome.get();
        if mode == "sync-error" {
            return Err(ExchangeError::new("fixture rejection"));
        }
        let released = self.released.clone();
        Ok(Box::pin(std::future::poll_fn(move |_| {
            if matches!(mode, "pending" | "pending-error") && !released.get() {
                Poll::Pending
            } else if matches!(mode, "error" | "pending-error") {
                Poll::Ready(Err(ExchangeError::new("fixture rejection")))
            } else {
                Poll::Ready(Ok(vec![]))
            }
        })))
    }
    fn emit_log(
        &self,
        exchange: Rc<dyn Exchange>,
        message: String,
        severity: &'static str,
    ) -> Result<(), ExchangeError> {
        assert!(Rc::ptr_eq(&exchange, &self.exchange));
        self.events
            .borrow_mut()
            .push(Event::Log(message, severity.into()));
        Ok(())
    }
    fn add_chaser(&self, instance: Rc<ChaserOrderInstance>) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Chaser(
            instance.id().into(),
            instance.options().borrow().clone(),
        ));
        self.chasers.borrow_mut().push(instance);
        Ok(())
    }
    fn find_chaser(&self, id: &str) -> Option<Rc<ChaserOrderInstance>> {
        self.chasers.borrow().iter().find(|c| c.id() == id).cloned()
    }
    fn remove_chaser(&self, id: &str) -> Result<(), ExchangeError> {
        self.events
            .borrow_mut()
            .push(Event::RemoveChaser(id.into()));
        let old = self.find_chaser(id);
        self.chasers.borrow_mut().retain(|c| c.id() != id);
        let hook = self.remove_hook.borrow().clone();
        if let (Some(old), Some(hook)) = (old, hook) {
            hook(&old);
        }
        Ok(())
    }
    fn same_timer(&self, a: &u64, b: &u64) -> bool {
        a == b
    }
    fn spawn(&self, f: ExchangeFuture<'static, ()>) {
        self.spawned.borrow_mut().push(f);
    }
}
impl Host {
    fn new() -> Rc<Self> {
        let clock = Rc::new(Clock {
            inner: Default::default(),
            draw: Cell::new(0.25),
            ids: Cell::new(0),
            random_hook: RefCell::new(None),
        });
        clock.inner.advance(1000);
        let exchange: Rc<dyn Exchange> = Rc::new(Venue {
            base: BaseExchange::new(
                ExchangeName::Bybit,
                serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                Rc::new(DefaultStore::new()),
                clock.clone(),
                Default::default(),
            ),
            caps: Default::default(),
        });
        Rc::new(Self {
            clock,
            manager: Rc::new(TwapManager::default()),
            exchange,
            events: Rc::new(RefCell::new(vec![])),
            timers: RefCell::new(IndexMap::new()),
            next: Cell::new(1),
            outcome: Cell::new("ok"),
            released: Rc::new(Cell::new(false)),
            spawned: RefCell::new(vec![]),
            chasers: RefCell::new(vec![]),
            now_reads: RefCell::new(VecDeque::new()),
            post_failure: Cell::new(false),
            remove_hook: RefCell::new(None),
        })
    }
    fn fire(&self, id: u64) {
        let (callback, _) = self.timers.borrow_mut().shift_remove(&id).unwrap();
        self.events.borrow_mut().push(Event::Fired(id));
        callback();
        self.drain();
    }
    fn drain(&self) {
        let tasks = std::mem::take(&mut *self.spawned.borrow_mut());
        for mut task in tasks {
            if task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.spawned.borrow_mut().push(task);
            }
        }
    }
    fn detach_timers(&self) {
        self.timers.borrow_mut().clear();
        self.spawned.borrow_mut().clear();
    }
}
fn ready<T>(mut future: ExchangeFuture<'static, T>) -> T {
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(Ok(x)) => x,
        Poll::Ready(Err(e)) => panic!("{}", e.message),
        Poll::Pending => panic!("fixture expected settled future"),
    }
}
fn options() -> TwapOptions {
    serde_json::from_value(json!({"accountName":"account","activeMarket":{"id":"BTC","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}},"side":"buy","size":3,"ordersCount":2,"duration":1,"randomnessPercentage":10,"chaserDelay":100,"useChaserOrders":false,"useLimitOrders":false,"reduceOnly":false})).unwrap()
}
fn initial(status: TwapStatus, sizes: &[f64]) -> TwapState {
    let o = options();
    TwapState {
        account_name: o.account_name,
        active_market: o.active_market,
        size: 3.0,
        side: o.side,
        status,
        orders_sizes: sizes.to_vec(),
        orders_count: 2.0,
        orders_executed: 0.0,
        next_order_at: 2000.0,
        origin: None,
        created_at: None,
        started_at: None,
        mark_for_cleanup: None,
    }
}
fn worker(h: &Rc<Host>, status: TwapStatus, sizes: &[f64]) -> Rc<TwapInstance<Host>> {
    let log = h.events.clone();
    let cb: UpdateStateCallback<Host> = Rc::new(move |_, state| {
        log.borrow_mut().push(Event::Update(state.borrow().clone()));
        Ok(())
    });
    let w = TwapInstance::new(
        h.exchange.clone(),
        h.clone(),
        Rc::new(RefCell::new(options())),
        cb,
        Some(Rc::new(RefCell::new(initial(status, sizes)))),
        Some("fixture".into()),
    )
    .unwrap();
    h.manager.add(w.managed.clone());
    w
}
fn bits(n: f64) -> String {
    if n.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", n.to_bits())
    }
}
fn number(n: f64) -> Value {
    json!({"$num":bits(n)})
}
fn status(s: TwapStatus) -> &'static str {
    match s {
        TwapStatus::Idle => "idle",
        TwapStatus::Running => "running",
        TwapStatus::Paused => "paused",
        TwapStatus::Completed => "completed",
    }
}
fn state_profile(s: &TwapState) -> Value {
    let mut v = json!({"status":status(s.status),"ordersSizes":s.orders_sizes.iter().map(|n|number(*n)).collect::<Vec<_>>(),"ordersExecuted":number(s.orders_executed),"nextOrderAt":number(s.next_order_at)});
    if let Some(n) = s.started_at {
        v["startedAt"] = number(n);
    }
    if let Some(b) = s.mark_for_cleanup {
        v["markForCleanup"] = json!(b);
    }
    v
}
fn events_profile(events: &[Event]) -> Value {
    json!(events.iter().map(|e|match e{
 Event::Update(s)=>json!({"update":state_profile(s)}),
 Event::Post(st)=>json!({"post":st.iter().map(|(k,(_,s))|(k.clone(),state_profile(s))).collect::<serde_json::Map<_,_>>()}),
 Event::Order(o)=>{let mut v=json!({"symbol":o.symbol,"side":if o.side==OrderSide::Buy{"buy"}else{"sell"},"type":if o.r#type==OrderType::Limit{"limit"}else{"market"},"amount":number(o.amount),"reduceOnly":o.reduce_only.unwrap()});if let Some(p)=o.price{v["price"]=number(p);}json!({"order":v})},
 Event::Log(m,l)=>json!({"emit":["log",m,l]}),Event::Timer(id,d)=>json!({"timer":number(*id as f64),"delay":number(*d)}),Event::Clear(id)=>json!({"clear":number(*id as f64)}),Event::Fired(id)=>json!({"fired":number(*id as f64)}),
 Event::Chaser(id,o)=>json!({"chaser":id,"size":number(o.size)}),Event::RemoveChaser(id)=>json!({"removeChaser":id}),
 }).collect::<Vec<_>>())
}
fn project_source_state(v: &Value) -> Value {
    let keys = [
        "status",
        "ordersSizes",
        "ordersExecuted",
        "nextOrderAt",
        "startedAt",
        "markForCleanup",
    ];
    let mut m = serde_json::Map::new();
    for k in keys {
        if let Some(v) = v.get(k) {
            m.insert(k.into(), v.clone());
        }
    }
    Value::Object(m)
}
fn project_source_events(v: &Value) -> Value {
    json!(v.as_array().unwrap().iter().map(|e|{
 if let Some(s)=e.get("update"){json!({"update":project_source_state(s)})}
 else if let Some(p)=e.get("post"){json!({"post":p["data"].as_object().unwrap().iter().map(|(k,v)|(k.clone(),project_source_state(&v["state"]))).collect::<serde_json::Map<_,_>>()})}
 else{e.clone()}
 }).collect::<Vec<_>>())
}
fn native_state_assert(
    w: &Rc<TwapInstance<Host>>,
    expected_status: TwapStatus,
    executed: f64,
    sizes: &[f64],
    label: &str,
) {
    let st = w.state();
    let st = st.borrow();
    assert_eq!(st.status, expected_status, "TWAP native status {label}");
    assert_eq!(
        st.orders_executed.to_bits(),
        executed.to_bits(),
        "TWAP native count {label}"
    );
    assert_eq!(
        st.orders_sizes.len(),
        sizes.len(),
        "TWAP native remaining cardinality {label}"
    );
    for (a, b) in st.orders_sizes.iter().zip(sizes) {
        if b.is_nan() {
            assert!(a.is_nan(), "TWAP native NaN remaining");
        } else {
            assert_eq!(
                a.to_bits(),
                b.to_bits(),
                "TWAP native remaining bits {label}"
            );
        }
    }
}
fn check_order(e: &Event, amount: f64) {
    if let Event::Order(o) = e {
        assert_eq!(o.symbol, "BTC", "TWAP native order symbol");
        assert_eq!(o.side, OrderSide::Buy);
        assert_eq!(o.r#type, OrderType::Market);
        assert!(o.price.is_none());
        assert_eq!(o.reduce_only, Some(false));
        assert_eq!(
            o.amount.to_bits(),
            amount.to_bits(),
            "TWAP native slice amount"
        );
        assert!(o.client_order_id.is_none());
        assert!(o.time_in_force.is_none());
        assert!(o.trigger_price.is_none());
        assert!(o.stop_loss.is_none());
        assert!(o.take_profit.is_none());
        assert!(o.max_slippage_pct.is_none());
        assert!(o.close.is_none());
    } else {
        panic!("TWAP native order event");
    }
}
fn oracle(path: &str) -> Value {
    let out = std::process::Command::new("node")
        .arg(format!("{}/tests/{path}", env!("CARGO_MANIFEST_DIR")))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn native_lifecycle_matches_252_source_interleavings() {
    let source = oracle("twap_lifecycle_oracle.mjs");
    let mut native = vec![];
    let sets: [&[f64]; 7] = [
        &[],
        &[0.0, 2.0],
        &[-0.0, 2.0],
        &[f64::NAN, 2.0],
        &[f64::INFINITY, 2.0],
        &[1.0, 2.0],
        &[1.0],
    ];
    for st in [
        TwapStatus::Idle,
        TwapStatus::Running,
        TwapStatus::Paused,
        TwapStatus::Completed,
    ] {
        for sizes in sets {
            for outcome in ["ok", "error", "pending"] {
                for interruption in ["none", "pause", "terminate"] {
                    let h = Host::new();
                    h.outcome.set(outcome);
                    let w = worker(&h, st, sizes);
                    let active = matches!(st, TwapStatus::Idle | TwapStatus::Running);
                    let places = active && sizes.first().is_some_and(|n| *n != 0.0 && !n.is_nan());
                    let tail = if active && !sizes.is_empty() {
                        &sizes[1..]
                    } else {
                        sizes
                    };
                    let mut future = w.start();
                    let polled = future
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()));
                    let pending = places && outcome == "pending";
                    assert_eq!(
                        polled.is_pending(),
                        pending,
                        "TWAP native placement suspension"
                    );
                    let complete = active && tail.is_empty();
                    let before_status = if !active {
                        st
                    } else if pending {
                        TwapStatus::Running
                    } else if complete {
                        TwapStatus::Completed
                    } else {
                        TwapStatus::Running
                    };
                    let before_count = if places && !pending { 1.0 } else { 0.0 };
                    native_state_assert(&w, before_status, before_count, tail, "before");
                    let orders: Vec<_> = h
                        .events
                        .borrow()
                        .iter()
                        .filter(|e| matches!(e, Event::Order(_)))
                        .cloned()
                        .collect();
                    assert_eq!(
                        orders.len(),
                        usize::from(places),
                        "TWAP native before order count"
                    );
                    if places {
                        check_order(&orders[0], sizes[0]);
                    }
                    let before = json!({"state":state_profile(&w.state().borrow()),"events":events_profile(&h.events.borrow()),"timers":h.timers.borrow().len(),"registered":h.manager.has_id("fixture")});
                    if interruption == "pause" {
                        ready(w.pause());
                    }
                    if interruption == "terminate" {
                        ready(w.terminate());
                    }
                    h.released.set(true);
                    let result = match polled {
                        Poll::Ready(r) => r.unwrap(),
                        Poll::Pending => ready(future),
                    };
                    // The two Completed arms are DISTINCT expectations, not a
                    // duplicated branch: terminate retains completed status
                    // (see safe-cex-webworker/CLAUDE.md on the pending TWAP
                    // lifecycle race), while active-and-complete reaches it by
                    // finishing. Collapsing them would stop this harness
                    // distinguishing the two source paths.
                    #[allow(clippy::if_same_then_else)]
                    let final_status = if interruption == "terminate" {
                        TwapStatus::Completed
                    } else if active && complete {
                        TwapStatus::Completed
                    } else if interruption == "pause"
                        && matches!(before_status, TwapStatus::Running)
                    {
                        TwapStatus::Paused
                    } else {
                        before_status
                    };
                    let count = f64::from(places);
                    native_state_assert(&w, final_status, count, tail, "settled");
                    let registered = !complete && interruption != "terminate";
                    assert_eq!(
                        h.manager.has_id("fixture"),
                        registered,
                        "TWAP native manager membership"
                    );
                    let schedules =
                        active && !tail.is_empty() && (pending || interruption == "none");
                    assert_eq!(
                        h.timers.borrow().len(),
                        usize::from(schedules),
                        "TWAP native timer after interruption"
                    );
                    assert_eq!(
                        result,
                        if !active {
                            None
                        } else {
                            Some(!tail.is_empty())
                        },
                        "TWAP native start result"
                    );
                    if places && outcome == "error" {
                        let events = h.events.borrow();
                        let logs: Vec<_> = events
                            .iter()
                            .filter_map(|e| {
                                if let Event::Log(m, l) = e {
                                    Some((m, l))
                                } else {
                                    None
                                }
                            })
                            .collect();
                        assert_eq!(logs.len(), 1, "TWAP native rejection log count");
                        assert_eq!(logs[0].0, "TWAP BTC slice failed: fixture rejection");
                        assert_eq!(logs[0].1, "error");
                    }
                    let settled = json!({"result":result.map(|b|json!(b)).unwrap_or(json!("undefined")),"state":state_profile(&w.state().borrow()),"events":events_profile(&h.events.borrow()),"timers":h.timers.borrow().len(),"registered":h.manager.has_id("fixture")});
                    let timer = h
                        .timers
                        .borrow()
                        .first()
                        .map(|(id, (_, delay))| (*id, *delay));
                    if let Some((id, delay)) = timer {
                        assert_eq!(
                            delay.to_bits(),
                            28500.0_f64.to_bits(),
                            "TWAP native random wait bits"
                        );
                        h.clock.inner.now.set((1000.0 + delay) as u64);
                        h.fire(id);
                    }
                    if schedules
                        && matches!(final_status, TwapStatus::Paused | TwapStatus::Completed)
                    {
                        let orders = h
                            .events
                            .borrow()
                            .iter()
                            .filter(|e| matches!(e, Event::Order(_)))
                            .count();
                        assert_eq!(
                            orders,
                            usize::from(places),
                            "TWAP native paused/completed fired guard"
                        );
                    }
                    if schedules && final_status == TwapStatus::Running {
                        native_state_assert(&w, TwapStatus::Completed, count + 1.0, &[], "fired");
                    } else {
                        native_state_assert(&w, final_status, count, tail, "fired");
                    }
                    let fired = json!({"state":state_profile(&w.state().borrow()),"events":events_profile(&h.events.borrow()),"timers":h.timers.borrow().len(),"registered":h.manager.has_id("fixture")});
                    ready(w.cleanup());
                    assert_eq!(
                        w.state().borrow().mark_for_cleanup,
                        Some(true),
                        "TWAP native cleanup marker"
                    );
                    let cleanup = json!({"state":state_profile(&w.state().borrow()),"events":events_profile(&h.events.borrow())});
                    native.push(json!({"status":status(st),"sizes":sizes.iter().map(|n|bits(*n)).collect::<Vec<_>>(),"outcome":outcome,"interruption":interruption,"before":before,"settled":settled,"fired":fired,"cleanup":cleanup}));
                    h.detach_timers();
                }
            }
        }
    }
    assert_eq!(native.len(), 252, "TWAP native fixed 4x7x3x3 floor");
    assert_eq!(
        source["rows"].as_array().unwrap().len(),
        252,
        "TWAP source fixed floor"
    );
    for (i, row) in native.iter().enumerate() {
        let src = &source["rows"][i];
        for k in ["status", "sizes", "outcome", "interruption"] {
            assert_eq!(row[k], src[k], "TWAP source/native input {i}/{k}");
        }
        for stage in ["before", "settled", "fired", "cleanup"] {
            assert_eq!(
                row[stage]["state"],
                project_source_state(&src[stage]["state"]),
                "TWAP source/native state {i}/{stage}"
            );
            assert_eq!(
                row[stage]["events"],
                project_source_events(&src[stage]["events"]),
                "TWAP source/native effects {i}/{stage}"
            );
            for k in ["result", "timers", "registered"] {
                assert_eq!(
                    row[stage].get(k),
                    src[stage].get(k),
                    "TWAP source/native {i}/{stage}/{k}"
                );
            }
        }
    }
}

#[test]
fn constructor_callback_aliases_and_failure_cutovers() {
    let h = Host::new();
    let o = Rc::new(RefCell::new(options()));
    let state = Rc::new(RefCell::new(initial(TwapStatus::Idle, &[1.0, 2.0])));
    let calls = Rc::new(Cell::new(0));
    let cc = calls.clone();
    let log = h.events.clone();
    let mode = Rc::new(Cell::new("normal"));
    let cm = mode.clone();
    let expected_receiver = Rc::new(Cell::new(std::ptr::null::<TwapInstance<Host>>()));
    let received = expected_receiver.clone();
    let cb: UpdateStateCallback<Host> = Rc::new(move |receiver, s| {
        assert_eq!(
            receiver as *const TwapInstance<Host>,
            received.get(),
            "TWAP native callback receiver identity"
        );
        cc.set(cc.get() + 1);
        assert!(
            Rc::ptr_eq(&s, &receiver.state()),
            "TWAP native callback current state identity"
        );
        log.borrow_mut().push(Event::Update(s.borrow().clone()));
        if cm.get() == "throw" {
            return Err(ExchangeError::new("callback failed"));
        }
        if cm.get() == "replace" {
            receiver.managed.borrow_mut().state =
                Rc::new(RefCell::new(initial(TwapStatus::Paused, &[9.0])));
            receiver.host.manager.remove(&receiver.id());
        }
        Ok(())
    });
    let w = TwapInstance::new(
        h.exchange.clone(),
        h.clone(),
        o.clone(),
        cb.clone(),
        Some(state.clone()),
        Some(String::new()),
    )
    .unwrap();
    expected_receiver.set(Rc::as_ptr(&w));
    assert_eq!(w.id(), "generated-0", "TWAP native empty ID fallback");
    assert_eq!(h.clock.ids.get(), 1);
    assert!(
        Rc::ptr_eq(&w.options(), &o),
        "TWAP native constructor options identity"
    );
    assert!(
        Rc::ptr_eq(&w.state(), &state),
        "TWAP native constructor state identity"
    );
    assert!(
        Rc::ptr_eq(&w.update_state_callback, &cb),
        "TWAP native stored callback identity"
    );
    assert!(Rc::ptr_eq(&w.exchange, &h.exchange));
    assert!(Rc::ptr_eq(&w.host, &h));
    assert_eq!(calls.get(), 0, "TWAP native constructor no callback");
    assert!(w.timer_id.borrow().is_none());
    assert!(w.slice_timers.borrow().is_empty());
    assert!(w.tracked_chaser_order_ids.borrow().is_empty());
    h.manager.add(w.managed.clone());
    assert!(
        Rc::ptr_eq(&h.manager.get_by_id("generated-0").unwrap(), &w.managed),
        "TWAP native manager record identity"
    );
    w.update_state(TwapStateUpdate {
        status: Some(TwapStatus::Running),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    let new = w.state();
    assert!(!Rc::ptr_eq(&new, &state), "TWAP native state replacement");
    assert_eq!(state.borrow().status, TwapStatus::Idle);
    assert_eq!(new.borrow().status, TwapStatus::Running);
    assert!(
        matches!(h.events.borrow()[0], Event::Update(_)),
        "TWAP native callback before post"
    );
    assert!(matches!(h.events.borrow()[1], Event::Post(_)));
    new.borrow_mut().orders_sizes.clear();
    assert_eq!(
        state.borrow().orders_sizes.len(),
        2,
        "TWAP native chosen Vec snapshot boundary"
    );
    mode.set("replace");
    w.update_state(TwapStateUpdate {
        next_order_at: Some(777.0),
        ..Default::default()
    })
    .unwrap();
    assert_eq!(w.state().borrow().orders_sizes, [9.0]);
    assert_eq!(w.state().borrow().status, TwapStatus::Paused);
    if let Event::Post(s) = h.events.borrow().last().unwrap() {
        assert!(
            s.is_empty(),
            "TWAP native post serializes manager after callback"
        );
    } else {
        panic!("post");
    }
    mode.set("throw");
    let before = h.events.borrow().len();
    let error = w
        .update_state(TwapStateUpdate {
            mark_for_cleanup: Some(true),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(error.message, "callback failed");
    assert_eq!(
        h.events.borrow().len(),
        before + 1,
        "TWAP native callback throw prevents post"
    );
    assert_eq!(w.state().borrow().mark_for_cleanup, Some(true));
    mode.set("normal");
    h.post_failure.set(true);
    let error = w
        .update_state(TwapStateUpdate {
            next_order_at: Some(55.0),
            ..Default::default()
        })
        .unwrap_err();
    assert_eq!(error.message, "post failed");
    assert_eq!(
        w.state().borrow().next_order_at.to_bits(),
        55.0_f64.to_bits()
    );
    assert_eq!(calls.get(), 4);
    h.post_failure.set(false);
    let absent = TwapInstance::new(
        h.exchange.clone(),
        h.clone(),
        o.clone(),
        cb.clone(),
        None,
        Some(" ".into()),
    )
    .unwrap();
    assert_eq!(absent.id(), " ");
    assert_eq!(h.clock.ids.get(), 1, "TWAP native whitespace ID retained");
    let s = absent.state();
    let s = s.borrow();
    assert_eq!(s.status, TwapStatus::Idle);
    assert_eq!(s.created_at, Some(1000.0));
    assert_eq!(s.orders_sizes.len(), 2);
    assert_eq!(s.orders_sizes[0].to_bits(), 1.53_f64.to_bits());
    assert_eq!(s.orders_sizes[1].to_bits(), 1.47_f64.to_bits());
    assert_eq!(calls.get(), 4);
    o.borrow_mut().orders_count = f64::INFINITY;
    let error = match TwapInstance::new(h.exchange.clone(), h.clone(), o, cb, None, None) {
        Err(e) => e,
        Ok(_) => panic!("expected range failure"),
    };
    assert_eq!(error.name, "RangeError");
    assert_eq!(
        h.clock.ids.get(),
        2,
        "TWAP native ID generated before state creation error"
    );
    let source = oracle("twap_policy_oracle.mjs");
    assert_eq!(
        source["constructor"],
        json!({"emptyId":"generated-0","stateSame":true,"optionsSame":true,"callbackSame":true,"constructorCalls":0,"oldStatus":"idle","callbackBeforePost":true,"postAfterRemoval":true,"callbackFailurePostCount":0,"committedMarker":true,"arrayAlias":true,"whitespaceId":" ","idBeforeError":true})
    );
}

#[test]
fn pause_resume_clocks_and_termination_preserve_source_order() {
    let h = Host::new();
    let w = worker(&h, TwapStatus::Running, &[1.0]);
    *w.timer_id.borrow_mut() = Some(42);
    ready(w.pause());
    assert_eq!(
        w.state().borrow().status,
        TwapStatus::Paused,
        "TWAP native pause status"
    );
    assert_eq!(
        *w.timer_id.borrow(),
        Some(42),
        "TWAP native pause retains timer property"
    );
    assert!(matches!(
        h.events.borrow().last().unwrap(),
        Event::Clear(42)
    ));
    *h.now_reads.borrow_mut() = VecDeque::from([1000.0, 1001.0]);
    w.state().borrow_mut().next_order_at = 2000.0;
    ready(w.resume());
    assert!(h.now_reads.borrow().is_empty());
    let timer = *w.timer_id.borrow().as_ref().unwrap();
    assert_eq!(
        h.timers.borrow()[&timer].1.to_bits(),
        999.0_f64.to_bits(),
        "TWAP native future resume delay"
    );
    assert_eq!(w.state().borrow().next_order_at, 2000.0);
    assert_eq!(w.state().borrow().status, TwapStatus::Running);
    assert!(
        matches!(h.events.borrow()[3], Event::Timer(_, 999.0)),
        "TWAP native resume schedules before callback"
    );
    ready(w.pause());
    *h.now_reads.borrow_mut() = VecDeque::from([3000.0, 3001.0, 3002.0]);
    ready(w.resume());
    let timer = *w.timer_id.borrow().as_ref().unwrap();
    assert_eq!(
        h.timers.borrow()[&timer].1.to_bits(),
        (-1.0_f64).to_bits(),
        "TWAP native expired resume raw delay"
    );
    assert!(h.now_reads.borrow().is_empty());
    w.slice_timers.borrow_mut().extend([101, 102]);
    ready(w.terminate());
    assert_eq!(
        w.state().borrow().status,
        TwapStatus::Completed,
        "TWAP native terminate status"
    );
    assert!(w.slice_timers.borrow().is_empty());
    assert_eq!(*w.timer_id.borrow(), Some(timer));
    assert!(
        !h.manager.has_id("fixture"),
        "TWAP native terminate manager removal"
    );
    let clears: Vec<_> = h
        .events
        .borrow()
        .iter()
        .filter_map(|e| {
            if let Event::Clear(id) = e {
                Some(*id)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(clears, [42, 1, 2, 101, 102], "TWAP native clear order");
    let prior = h.events.borrow().len();
    w.slice_timers.borrow_mut().push(103);
    ready(w.terminate());
    assert_eq!(
        h.events.borrow().len(),
        prior,
        "TWAP native completed terminate skips clears"
    );
    assert_eq!(*w.slice_timers.borrow(), [103]);
    ready(w.cleanup());
    assert_eq!(w.state().borrow().mark_for_cleanup, Some(true));
    let native = events_profile(&h.events.borrow());
    let source = oracle("twap_policy_oracle.mjs");
    assert_eq!(native, project_source_events(&source["pauseResume"]));
    h.detach_timers();
}

#[test]
fn limit_prices_and_chaser_deadlines_pin_native_values_before_profiles() {
    let price_cases = [
        (OrderSide::Buy, 12.0, 13.0, 10.0, true, Some(12.0)),
        (OrderSide::Sell, 12.0, 13.0, 10.0, true, Some(13.0)),
        (OrderSide::Buy, 0.0, 13.0, 10.0, true, Some(10.0)),
        (OrderSide::Sell, 12.0, f64::NAN, 0.0, true, None),
        (OrderSide::Buy, -12.0, 13.0, 10.0, true, Some(-12.0)),
        (
            OrderSide::Sell,
            12.0,
            f64::INFINITY,
            10.0,
            true,
            Some(f64::INFINITY),
        ),
        (OrderSide::Buy, f64::NAN, 13.0, -0.0, true, None),
        (OrderSide::Buy, 12.0, 13.0, 10.0, false, None),
    ];
    let mut price_rows = vec![];
    for (side, bid, ask, last, present, expected) in price_cases {
        let h = Host::new();
        let w = worker(&h, TwapStatus::Idle, &[]);
        {
            let o = w.options();
            let mut o = o.borrow_mut();
            o.use_limit_orders = true;
            o.side = side;
        }
        if present {
            let mut ticker:safe_cex_types::Ticker=serde_json::from_value(json!({"id":"BTC","symbol":"BTC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
            ticker.bid = bid;
            ticker.ask = ask;
            ticker.last = last;
            h.exchange
                .base()
                .store
                .update(
                    safe_cex_store::StoreUpdate {
                        tickers: Some(vec![ticker]),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
        }
        ready(w.place_order(3.0));
        let events = h.events.borrow();
        assert_eq!(events.len(), 1);
        if let Event::Order(o) = &events[0] {
            assert_eq!(
                o.amount.to_bits(),
                3.0_f64.to_bits(),
                "TWAP native placed amount"
            );
            assert_eq!(o.side, side);
            assert_eq!(
                o.r#type,
                if expected.is_some() {
                    OrderType::Limit
                } else {
                    OrderType::Market
                }
            );
            assert_eq!(
                o.price.map(f64::to_bits),
                expected.map(f64::to_bits),
                "TWAP native chosen limit price"
            );
            assert_eq!(o.symbol, "BTC");
            assert_eq!(o.reduce_only, Some(false));
        } else {
            panic!("expected order");
        }
        price_rows.push(events_profile(&events));
    }
    assert_eq!(price_rows.len(), 8, "TWAP native fixed limit price cases");
    let mut policy_rows = vec![];
    for index in 0..12 {
        let h = Host::new();
        let w = worker(&h, TwapStatus::Idle, &[]);
        let stopped = index < 8 && index / 4 == 1;
        let timeout = if index < 8 && index % 4 >= 2 {
            TwapTimeoutAction::Next
        } else {
            TwapTimeoutAction::Market
        };
        let fail = if index < 8 && index % 2 == 1 {
            TwapTimeoutAction::Next
        } else {
            TwapTimeoutAction::Market
        };
        {
            let o = w.options();
            let mut o = o.borrow_mut();
            o.use_chaser_orders = true;
            o.use_limit_orders = true;
            o.timeout_pct = Some(if index == 11 { f64::NAN } else { 50.0 });
            o.timeout_action = Some(timeout);
            o.fail_action = Some(fail);
        }
        let amount = if index == 10 { f64::NAN } else { 3.0 };
        ready(w.place_order(amount));
        let id = w.tracked_chaser_order_ids.borrow()[0].clone();
        assert_eq!(
            w.tracked_chaser_order_ids.borrow().len(),
            1,
            "TWAP native tracked chaser count"
        );
        let chaser = h.find_chaser(&id).unwrap();
        assert_eq!(id, "generated-0");
        let opt = chaser.options();
        let opt = opt.borrow();
        assert_eq!(opt.max_attempts, 1000.0, "TWAP native chaser max attempts");
        assert_eq!(opt.delay, 100.0);
        assert_eq!(opt.reduce_only, Some(false));
        assert_eq!(opt.account_name, None);
        assert_eq!(opt.side, OrderSide::Buy);
        assert_eq!(opt.symbol, "BTC");
        if index == 10 {
            assert!(opt.size.is_nan());
        } else {
            assert_eq!(
                opt.size.to_bits(),
                amount.to_bits(),
                "TWAP native chaser amount"
            );
        }
        drop(opt);
        assert_eq!(w.slice_timers.borrow().len(), 1);
        let timer = w.slice_timers.borrow()[0];
        let delay = h.timers.borrow()[&timer].1;
        if index == 11 {
            assert!(delay.is_nan(), "TWAP native NaN deadline");
        } else {
            assert_eq!(
                delay.to_bits(),
                15000.0_f64.to_bits(),
                "TWAP native slice deadline"
            );
        }
        if stopped {
            chaser.mark_complete(None);
        }
        if index == 8 {
            h.chasers.borrow_mut().clear();
        }
        if index == 9 {
            chaser.options().borrow_mut().size = 0.0;
            chaser.recalculate_remaining_size();
        }
        // Decisions use captured actions; remainder is captured before the remove hook.
        {
            let o = w.options();
            let mut o = o.borrow_mut();
            o.timeout_action = Some(TwapTimeoutAction::Next);
            o.fail_action = Some(TwapTimeoutAction::Next);
        }
        *h.remove_hook.borrow_mut() = Some(Rc::new(|c| {
            c.options().borrow_mut().size = 0.0;
            c.recalculate_remaining_size();
        }));
        h.fire(timer);
        assert!(
            w.slice_timers.borrow().is_empty(),
            "TWAP native removes slice timer before lookup"
        );
        let removed = index != 8 && index != 9;
        assert_eq!(
            h.find_chaser(&id).is_none(),
            removed || index == 8,
            "TWAP native policy removal"
        );
        let market = removed
            && if stopped {
                fail == TwapTimeoutAction::Market
            } else {
                timeout == TwapTimeoutAction::Market
            };
        let events = h.events.borrow();
        let orders: Vec<_> = events
            .iter()
            .filter(|e| matches!(e, Event::Order(_)))
            .collect();
        assert_eq!(
            orders.len(),
            usize::from(market),
            "TWAP native captured slice action"
        );
        if market {
            if index == 10 {
                if let Event::Order(o) = orders[0] {
                    assert!(o.amount.is_nan(), "TWAP native NaN remainder proceeds");
                }
            } else {
                check_order(orders[0], 3.0);
            }
        }
        policy_rows.push(events_profile(&events));
        drop(events);
        h.detach_timers();
    }
    assert_eq!(policy_rows.len(), 12, "TWAP native fixed deadline cases");
    let source = oracle("twap_policy_oracle.mjs");
    assert_eq!(source["prices"].as_array().unwrap().len(), 8);
    assert_eq!(source["policies"].as_array().unwrap().len(), 12);
    assert_eq!(json!(price_rows), source["prices"]);
    assert_eq!(json!(policy_rows), source["policies"]);
}

#[test]
fn reentrant_rng_and_pending_rejection_read_live_options() {
    let h = Host::new();
    h.outcome.set("pending-error");
    let o = Rc::new(RefCell::new(options()));
    let original = o.clone();
    let log = h.events.clone();
    let cb: UpdateStateCallback<Host> = Rc::new(move |receiver, state| {
        log.borrow_mut().push(Event::Update(state.borrow().clone()));
        if state.borrow().status == TwapStatus::Running {
            state.borrow_mut().orders_executed = 7.0;
            receiver.options().borrow_mut().active_market.symbol = "OTHER".into();
        }
        Ok(())
    });
    let w = TwapInstance::new(
        h.exchange.clone(),
        h.clone(),
        o,
        cb,
        Some(Rc::new(RefCell::new(initial(
            TwapStatus::Idle,
            &[1.0, 2.0],
        )))),
        Some("fixture".into()),
    )
    .unwrap();
    h.manager.add(w.managed.clone());
    *h.clock.random_hook.borrow_mut() = Some(Rc::new(move || {
        let mut o = original.borrow_mut();
        o.duration = 1000.0;
        o.randomness_percentage = 99.0;
        o.active_market.symbol = "RNG".into();
    }));
    let mut future = w.start();
    assert_eq!(
        w.state().borrow().orders_executed.to_bits(),
        7.0_f64.to_bits(),
        "TWAP native idle callback precedes executed capture"
    );
    let events = h.events.borrow();
    assert_eq!(
        events.len(),
        3,
        "TWAP native placement prefix before polling"
    );
    if let Event::Order(o) = &events[2] {
        assert_eq!(
            o.symbol, "OTHER",
            "TWAP native placement reads callback-mutated options"
        );
        assert_eq!(o.amount, 1.0);
    } else {
        panic!("expected prefix order");
    }
    drop(events);
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    ready(w.pause());
    w.options().borrow_mut().active_market.symbol = "AFTER".into();
    h.released.set(true);
    assert_eq!(ready(future), Some(true));
    assert_eq!(w.state().borrow().status, TwapStatus::Paused);
    assert_eq!(
        w.state().borrow().orders_executed.to_bits(),
        8.0_f64.to_bits(),
        "TWAP native captured count survives continuation"
    );
    let id = w.timer_id.borrow().unwrap();
    assert_eq!(
        h.timers.borrow()[&id].1.to_bits(),
        28500.0_f64.to_bits(),
        "TWAP native wait captured before reentrant RNG"
    );
    let logs: Vec<_> = h
        .events
        .borrow()
        .iter()
        .filter_map(|e| {
            if let Event::Log(m, l) = e {
                Some((m.clone(), l.clone()))
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        logs,
        [(
            "TWAP AFTER slice failed: fixture rejection".into(),
            "error".into()
        )],
        "TWAP native rejection reads current symbol"
    );
    let native = events_profile(&h.events.borrow());
    assert_eq!(
        native,
        project_source_events(&oracle("twap_policy_oracle.mjs")["rngRace"])
    );
    h.detach_timers();
}
