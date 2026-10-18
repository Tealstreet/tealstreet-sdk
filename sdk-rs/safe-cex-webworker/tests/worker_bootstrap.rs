#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_exchange::store_delivery::StoreFrame;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_store::Store;
use safe_cex_types::{ExchangeName, ExchangeOptions};
use safe_cex_webworker::chaser_manager::ChaserOrderManager;
use safe_cex_webworker::chaser_manager::ChaserSupervisor;
use safe_cex_webworker::exchange_worker::*;
use safe_cex_webworker::exchange_worker_start::*;
use safe_cex_webworker::payload::StoreWorkerOp;
use safe_cex_webworker::payload::StoreWorkerPayload;
use safe_cex_webworker::postprocessor_registry::ImportedPostprocessors;
use safe_cex_webworker::postprocessor_registry::PostprocessorRegistry;
use safe_cex_webworker::telegram::TelegramHost;
use safe_cex_webworker::worker_ports::WorkerConfigHandler;
use safe_cex_webworker::worker_ports::WorkerPort;
use safe_cex_webworker::worker_ports::WorkerPorts;
use safe_cex_webworker::worker_session::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};
#[derive(Default)]
struct State {
    expected_seed: Cell<Option<f64>>,
    dispose_check: RefCell<Option<Rc<dyn Fn()>>>,
    now: Cell<f64>,
    next: Cell<u64>,
    timers: RefCell<BTreeMap<u64, (f64, Rc<dyn Fn()>)>>,
    events: RefCell<Vec<String>>,
    posts: RefCell<Vec<StoreWorkerPayload>>,
    stores: RefCell<Vec<WorkerStoreMessage>>,
    tasks: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    runtime: Rc<runtime::Runtime>,
    manager: Rc<ChaserOrderManager>,
    registry: RefCell<Option<Rc<WorkerProcessorRegistry>>>,
    handlers: RefCell<Vec<(Weak<dyn WorkerHandler>, WorkerHandlerKind)>>,
    listener: RefCell<Option<Rc<dyn Fn(StoreWorkerPayload) -> ExchangeFuture<'static, ()>>>>,
    worker: RefCell<Weak<ExchangeWorker<Host>>>,
    clock_ready: Rc<Cell<bool>>,
    load_ready: Rc<Cell<bool>>,
    start_ready: Rc<Cell<bool>>,
    clock_error: Cell<bool>,
    load_missing: Cell<bool>,
    completed_starts: Rc<Cell<u32>>,
    replay: Cell<bool>,
    config: Rc<RefCell<Vec<WorkerConfigHandler<Value>>>>,
    replace_handlers: Cell<bool>,
}
struct Host {
    state: Rc<State>,
    self_ref: Weak<Host>,
    proxy: Option<(
        bool,
        Rc<RefCell<String>>,
        Rc<WorkerPorts<StoreWorkerPayload, Host, Host>>,
    )>,
}
impl Host {
    fn new() -> Rc<Self> {
        let state = Rc::new(State::default());
        state.runtime.advance(0);
        state.clock_ready.set(true);
        state.load_ready.set(true);
        state.start_ready.set(true);
        let h = Rc::new_cyclic(|w| Self {
            state: state.clone(),
            self_ref: w.clone(),
            proxy: None,
        });
        let ctor = |name| {
            Rc::new(Ctor {
                name,
                host: Rc::downgrade(&h),
            }) as Rc<dyn ProcessorConstructor>
        };
        *state.registry.borrow_mut() = Some(Rc::new(PostprocessorRegistry::new(
            ImportedPostprocessors {
                displayed_positions: ctor("positions"),
                displayed_margin: ctor("margin"),
                chaser_order_enrichment: ctor("enrichment"),
                orders_risk_profile: ctor("risk"),
                chaser_order: ctor("chaser"),
                twap: ctor("twap"),
                message_handler_stats: ctor("stats"),
            },
        )));
        h
    }
    fn event(&self, e: impl Into<String>) {
        self.state.events.borrow_mut().push(e.into());
    }
    fn drain(&self) {
        for _iteration in 0..32 {
            let tasks = std::mem::take(&mut *self.state.tasks.borrow_mut());
            for mut f in tasks {
                match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    Poll::Pending => self.state.tasks.borrow_mut().push(f),
                    Poll::Ready(r) => r.unwrap(),
                }
            }
        }
    }
    fn fire(&self, id: u64, time: f64) {
        self.state.now.set(time);
        self.state.runtime.advance(time as u64);
        let cb = self.state.timers.borrow_mut().remove(&id).unwrap().1;
        cb();
    }
    fn send(self: &Rc<Self>, payload: StoreWorkerPayload) -> ExchangeFuture<'static, ()> {
        let cb = self.state.listener.borrow().clone().unwrap();
        cb(payload)
    }
    fn forward(&self, p: StoreWorkerPayload) -> Result<(), ExchangeError> {
        if let Some((shared, key, ports)) = &self.proxy {
            self.event(format!("record:{}", op(p.op)));
            if *shared {
                ports.broadcast_with_throttle(&key.borrow(), Rc::new(p), None);
                return Ok(());
            }
        }
        self.event(format!("post:{}", op(p.op)));
        self.state.posts.borrow_mut().push(p);
        Ok(())
    }
}
fn op(op: StoreWorkerOp) -> String {
    serde_json::to_value(op).unwrap().as_str().unwrap().into()
}
fn kind(k: WorkerHandlerKind) -> &'static str {
    match k {
        WorkerHandlerKind::OHLCV => "ohlcv",
        WorkerHandlerKind::OrderBook => "book",
        WorkerHandlerKind::Chaser => "chaser",
        WorkerHandlerKind::Twap => "twap",
        WorkerHandlerKind::Dev => "dev",
        WorkerHandlerKind::Command => "command",
        WorkerHandlerKind::UpdatePositions => "positions",
        WorkerHandlerKind::CallMethod => "method",
        WorkerHandlerKind::UiState => "ui",
    }
}
struct Ctor {
    name: &'static str,
    host: Weak<Host>,
}
impl ProcessorConstructor for Ctor {
    fn construct(
        &self,
        a: ProcessorArguments,
        ex: Option<Rc<dyn Exchange>>,
    ) -> Result<Rc<dyn WorkerPostprocessor>, ExchangeError> {
        let h = self.host.upgrade().unwrap();
        h.event(format!("ctor:{}:{}", self.name, ex.is_some()));
        assert!(a.account_name == "a" || a.account_name.is_empty());
        assert_eq!(a.exchange_name, ExchangeName::Bybit);
        if let Some(w) = h.state.worker.borrow().upgrade() {
            assert!(
                Rc::ptr_eq(&a.options, &w.construction.options),
                "BOOT native processor options identity"
            );
        }
        Ok(Rc::new(Processor {
            name: self.name,
            host: self.host.clone(),
        }))
    }
}
struct Processor {
    name: &'static str,
    host: Weak<Host>,
}
impl WorkerPostprocessor for Processor {
    fn process(&self, _: StoreFrame) -> Result<(), ExchangeError> {
        self.host
            .upgrade()
            .unwrap()
            .event(format!("process:{}", self.name));
        Ok(())
    }
}
struct Handler {
    name: WorkerHandlerKind,
    host: Weak<Host>,
}
impl WorkerHandler for Handler {
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.host
            .upgrade()
            .unwrap()
            .event(format!("dispose:{}", kind(self.name)));
        Ok(())
    }
}
struct Venue {
    base: BaseExchange,
    caps: ExchangeCapabilities,
    state: Rc<State>,
}
impl Exchange for Venue {
    fn dispose(&self) -> Result<(), ExchangeError> {
        if let Some(check) = self.state.dispose_check.borrow().clone() {
            check();
        }
        self.base.dispose()
    }
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.caps
    }
}
struct Factory {
    host: Weak<Host>,
}
impl WorkerExchangeConstructor for Factory {
    fn construct(
        &self,
        name: ExchangeName,
        options: Rc<RefCell<ExchangeOptions>>,
        store: Rc<dyn Store>,
    ) -> Result<Rc<dyn Exchange>, ExchangeError> {
        let h = self.host.upgrade().unwrap();
        h.event("exchange");
        if let Some(w) = h.state.worker.borrow().upgrade() {
            assert!(
                w.exchange().is_none(),
                "BOOT native constructor callback can inspect exchange"
            );
            assert!(
                Rc::ptr_eq(&store, &w.store()),
                "BOOT native constructor exact store"
            );
            assert!(Rc::ptr_eq(&options, &w.construction.options));
        }
        Ok(Rc::new(Venue {
            base: BaseExchange::new(
                name,
                options.borrow().clone(),
                store,
                h.state.runtime.clone(),
                Default::default(),
            ),
            caps: Default::default(),
            state: h.state.clone(),
        }))
    }
}
struct NoChasers;
impl ChaserSupervisor for NoChasers {
    fn construct_supervision(
        &self,
        _: Rc<ChaserOrderManager>,
        _: Rc<safe_cex_webworker::chaser::ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        panic!("unexpected chaser")
    }
    fn construct_cleanup(
        &self,
        _: Rc<ChaserOrderManager>,
        _: Rc<safe_cex_webworker::chaser::ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        panic!("unexpected chaser cleanup")
    }
    fn construct_completion(
        &self,
        _: Rc<safe_cex_webworker::chaser::ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        panic!("unexpected chaser completion")
    }
    fn spawn(&self, _: ExchangeFuture<'static, ()>) {
        panic!("unexpected chaser future")
    }
}
impl WorkerPortTimers for Host {
    type Timer = u64;
    fn now_ms(&self) -> f64 {
        self.state.now.get()
    }
    fn set_timeout(&self, f: Rc<dyn Fn()>, delay: f64) -> u64 {
        let id = self.state.next.get();
        self.state.next.set(id + 1);
        self.state.timers.borrow_mut().insert(id, (delay, f));
        self.event(format!("timer:{}", safe_cex_num::js_number_string(delay)));
        id
    }
    fn clear_timeout(&self, id: &u64) {
        self.state.timers.borrow_mut().remove(id);
        self.event(format!("clear:{id}"));
    }
    fn timer_is_truthy(&self, id: &u64) -> bool {
        *id != 0
    }
}
impl safe_cex_exchange::store_delivery::StoreUpdateThrottleHost for Host {
    fn report_store_timer_error(&self, e: &ExchangeError) {
        self.event(e.message.clone());
    }
}
impl ExchangeWorkerHost for Host {
    fn post_store(&self, m: WorkerStoreMessage) -> Result<(), ExchangeError> {
        self.state.stores.borrow_mut().push(m);
        self.event("store");
        Ok(())
    }
    fn chaser_manager(&self) -> Rc<ChaserOrderManager> {
        self.state.manager.clone()
    }
    fn chaser_supervisor(&self) -> Rc<dyn ChaserSupervisor> {
        Rc::new(NoChasers)
    }
}
impl WorkerBootstrapHost for Host {
    fn construct_clock_start(&self) -> ExchangeFuture<'static, ()> {
        self.event("clock");
        let ready = self.state.clock_ready.clone();
        let failed = self.state.clock_error.get();
        Box::pin(std::future::poll_fn(move |_| {
            if ready.get() {
                Poll::Ready(if failed {
                    Err(ExchangeError::new("clock failed"))
                } else {
                    Ok(())
                })
            } else {
                Poll::Pending
            }
        }))
    }
    fn load_exchange(
        &self,
        _: ExchangeName,
    ) -> Option<ExchangeFuture<'static, Rc<dyn WorkerExchangeConstructor>>> {
        self.event("load");
        if self.state.load_missing.get() {
            return None;
        }
        let ready = self.state.load_ready.clone();
        let factory: Rc<dyn WorkerExchangeConstructor> = Rc::new(Factory {
            host: self.self_ref.clone(),
        });
        Some(Box::pin(std::future::poll_fn(move |_| {
            if ready.get() {
                Poll::Ready(Ok(factory.clone()))
            } else {
                Poll::Pending
            }
        })))
    }
    fn processor_registry(&self) -> Rc<WorkerProcessorRegistry> {
        self.state.registry.borrow().clone().unwrap()
    }
    fn construct_handler(
        &self,
        k: WorkerHandlerKind,
        c: &WorkerConstruction<Self>,
        ex: Rc<dyn Exchange>,
    ) -> Result<Rc<dyn WorkerHandler>, ExchangeError> {
        self.event(format!("handler:{}", kind(k)));
        if let Some(w) = self.state.worker.borrow().upgrade() {
            assert!(Rc::ptr_eq(&ex, &w.exchange().unwrap()));
            assert!(Rc::ptr_eq(&c.settings, &w.construction.settings));
            assert!(Rc::ptr_eq(&c.metadata, &w.construction.metadata));
        }
        let handler: Rc<dyn WorkerHandler> = Rc::new(Handler {
            name: k,
            host: self.self_ref.clone(),
        });
        self.state
            .handlers
            .borrow_mut()
            .push((Rc::downgrade(&handler), k));
        Ok(handler)
    }
    fn invoke_handler(
        &self,
        handler: Rc<dyn WorkerHandler>,
        payload: StoreWorkerPayload,
    ) -> Result<(), ExchangeError> {
        let k = self
            .state
            .handlers
            .borrow()
            .iter()
            .find_map(|(w, k)| w.upgrade().filter(|v| Rc::ptr_eq(v, &handler)).map(|_| *k))
            .unwrap();
        self.event(format!("handle:{}:{}", kind(k), op(payload.op)));
        if self.state.replace_handlers.replace(false) {
            let w = self.state.worker.borrow().upgrade().unwrap();
            let ex = w.exchange().unwrap();
            let first =
                self.construct_handler(WorkerHandlerKind::CallMethod, &w.construction, ex.clone())?;
            let second = self.construct_handler(WorkerHandlerKind::UiState, &w.construction, ex)?;
            *w.handlers.borrow_mut() = Rc::new(RefCell::new(vec![first, second]));
        }
        Ok(())
    }
    fn post_payload(&self, p: StoreWorkerPayload) -> Result<(), ExchangeError> {
        self.forward(p)
    }
    fn post_event(
        &self,
        payload: safe_cex_webworker::data_post::WorkerDataPayload,
    ) -> Result<(), ExchangeError> {
        self.forward(StoreWorkerPayload {
            op: payload.op,
            instance_id: None,
            data: Some(safe_cex_exchange::js_runtime::value::to_json(
                &payload.data,
            )?),
        })
    }
    fn spawn_start_task(&self, f: ExchangeFuture<'static, ()>) {
        self.state.tasks.borrow_mut().push(f)
    }
    fn construct_exchange_start(&self, ex: Rc<dyn Exchange>) -> ExchangeFuture<'static, ()> {
        if let Some(expected) = self.state.expected_seed.get() {
            assert_eq!(
                ex.base().store.serialize().latency.to_bits(),
                expected.to_bits(),
                "SESSION native store seeded inside venue start prefix"
            );
        }
        self.event("start");
        let ready = self.state.start_ready.clone();
        let count = self.state.completed_starts.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if ready.get() {
                count.set(count.get() + 1);
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn error_properties(&self, e: &ExchangeError) -> Value {
        json!({"name":e.name,"message":e.message})
    }
    fn materialize_error(&self, e: &ExchangeError) -> Result<Value, ExchangeError> {
        Ok(json!({"name":e.name,"message":e.message}))
    }
    fn console_error_fallback(
        &self,
        _: &str,
        _: &ExchangeError,
        _: ExchangeError,
    ) -> Result<(), ExchangeError> {
        self.event("fallback");
        Ok(())
    }
    fn console_suppressed_start_error(
        &self,
        _: ExchangeName,
        _: &ExchangeError,
    ) -> Result<(), ExchangeError> {
        self.event("suppressed");
        Ok(())
    }
}
impl WorkerPort<StoreWorkerPayload> for Host {
    type Error = ExchangeError;
    fn post_message(&self, p: &Rc<StoreWorkerPayload>) -> Result<(), ExchangeError> {
        self.forward((**p).clone())
    }
}
impl WorkerSessionHost for Host {
    fn install_message_listener(
        &self,
        l: Rc<dyn Fn(StoreWorkerPayload) -> ExchangeFuture<'static, ()>>,
    ) {
        assert!(self.state.listener.borrow().is_none());
        self.event("listen");
        *self.state.listener.borrow_mut() = Some(l)
    }
    fn worker_context(
        &self,
        shared: bool,
        key: Rc<RefCell<String>>,
        ports: Rc<WorkerPorts<StoreWorkerPayload, Self, Self>>,
    ) -> Rc<Self> {
        self.event(format!("proxy:{shared}"));
        Rc::new_cyclic(|w| Self {
            state: self.state.clone(),
            self_ref: w.clone(),
            proxy: Some((shared, key, ports)),
        })
    }
    fn is_replaying_perf_fixture(&self) -> bool {
        self.state.replay.get()
    }
    fn start_replay(
        &self,
        _: Rc<dyn Fn(StoreWorkerPayload) -> Result<(), ExchangeError>>,
    ) -> Result<(), ExchangeError> {
        self.event("replay");
        Ok(())
    }
    fn configure_perf_fixture(&self, _: Value) -> Result<(), ExchangeError> {
        self.event("configure-perf");
        Ok(())
    }
    fn take_perf_fixture(&self) -> Result<Value, ExchangeError> {
        Ok(json!({"frames":[]}))
    }
    fn set_tick_interval_multipliers(&self, _: Value) -> Result<(), ExchangeError> {
        self.event("tick-config");
        Ok(())
    }
    fn set_ws_diagnostics_enabled(&self, enabled: bool) -> Result<(), ExchangeError> {
        self.event(format!("diagnostics:{enabled}"));
        Ok(())
    }
    fn config_handlers(&self) -> Rc<RefCell<Vec<WorkerConfigHandler<Value>>>> {
        self.state.config.clone()
    }
    fn invoke_config_handler(
        &self,
        h: WorkerConfigHandler<Value>,
        cfg: &Value,
    ) -> Result<(), ExchangeError> {
        h(cfg);
        Ok(())
    }
    fn sync_server_time(&self, t: f64) -> Result<(), ExchangeError> {
        self.event(format!("sync:{}", safe_cex_num::js_number_string(t)));
        Ok(())
    }
    fn console(&self, level: &str, _: &str, _: Option<Value>) -> Result<(), ExchangeError> {
        self.event(format!("console:{level}"));
        Ok(())
    }
    fn telegram_host(&self) -> Rc<dyn TelegramHost> {
        panic!("telegram not requested by this fixture")
    }
}
fn construction(h: Rc<Host>, hosted: bool) -> WorkerConstruction<Host> {
    WorkerConstruction {
        ctx: h,
        account_name: "a".into(),
        exchange_name: ExchangeName::Bybit,
        options: Rc::new(RefCell::new(
            serde_json::from_value(json!({"key":"","secret":"","hostedMode":hosted})).unwrap(),
        )),
        settings: Rc::new(RefCell::new(
            serde_json::from_value(
                json!({"placeOrder":{"tpTriggerType":"mark_price","slTriggerType":"last_price"}}),
            )
            .unwrap(),
        )),
        metadata: Rc::new(RefCell::new(
            serde_json::from_value(json!({"supportedTPSLTriggerTypes":[],"privateProxyIps":[]}))
                .unwrap(),
        )),
        session_key: "s".into(),
        instance_id: Some("i".into()),
    }
}
fn fixture(hosted: bool) -> (Rc<Host>, Rc<ExchangeWorker<Host>>) {
    let h = Host::new();
    let w = ExchangeWorker::new(construction(h.clone(), hosted));
    *h.state.worker.borrow_mut() = Rc::downgrade(&w);
    (h, w)
}
fn payload(op: StoreWorkerOp, data: Option<Value>) -> StoreWorkerPayload {
    StoreWorkerPayload {
        op,
        instance_id: None,
        data,
    }
}
fn poll<T>(f: &mut ExchangeFuture<'static, T>) -> Poll<Result<T, ExchangeError>> {
    f.as_mut().poll(&mut Context::from_waker(Waker::noop()))
}
fn finish<T>(h: &Host, mut f: ExchangeFuture<'static, T>) -> Result<T, ExchangeError> {
    for _ in 0..32 {
        match poll(&mut f) {
            Poll::Ready(r) => return r,
            Poll::Pending => h.drain(),
        }
    }
    panic!("BOOT native unexpected pending at iteration 32")
}
fn source() -> Value {
    let o = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_bootstrap_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "BOOT source oracle {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn startup_success_timeout_cancellation_and_retained_loser() {
    let (h, w) = fixture(false);
    w.handle_payload(payload(StoreWorkerOp::Exec, None))
        .unwrap();
    let mut f = w.start();
    assert!(
        w.cancel_startup.borrow().is_some(),
        "BOOT native cancellation installed synchronously"
    );
    assert_eq!(
        h.state.events.borrow().last().unwrap(),
        "clock",
        "BOOT native clock prefix"
    );
    assert!(poll(&mut f).is_pending());
    let events = h.state.events.borrow().clone();
    let start = events.iter().position(|e| e == "start").unwrap();
    assert_eq!(
        h.state.timers.borrow()[&0].0.to_bits(),
        30_000f64.to_bits(),
        "BOOT native startup timeout milliseconds"
    );
    let timer = events.iter().position(|e| e == "timer:30000").unwrap();
    assert!(start < timer, "BOOT native venue starts before timeout");
    h.drain();
    assert_eq!(poll(&mut f), Poll::Ready(Ok(true)));
    assert!(w.cancel_startup.borrow().is_none());
    assert!(w.message_buffer.borrow().is_empty());
    assert_eq!(h.state.stores.borrow().len(), 1);
    assert!(
        h.state.stores.borrow()[0].data.orders.is_some(),
        "BOOT native forced startup orders baseline"
    );
    assert_eq!(h.state.completed_starts.get(), 1);
    assert_eq!(
        h.state.timers.borrow().len(),
        1,
        "BOOT native timeout loser retained"
    );
    let success = h.state.events.borrow().clone();
    let (h, w) = fixture(false);
    h.state.start_ready.set(false);
    let mut f = w.start();
    assert!(poll(&mut f).is_pending());
    w.dispose().unwrap();
    assert_eq!(poll(&mut f), Poll::Ready(Ok(false)));
    h.drain();
    assert_eq!(
        h.state.tasks.borrow().len(),
        1,
        "BOOT native cancelled venue task retained"
    );
    let disposal_frames = h.state.stores.borrow().len();
    assert_eq!(
        disposal_frames, 1,
        "BOOT native real BaseExchange disposal reset notification"
    );
    h.state.start_ready.set(true);
    h.drain();
    assert_eq!(
        h.state.completed_starts.get(),
        1,
        "BOOT native cancelled loser still settles"
    );
    assert_eq!(
        h.state.stores.borrow().len(),
        disposal_frames,
        "BOOT native cancelled loser produces no startup snapshot"
    );
    let (h, w) = fixture(false);
    h.state.start_ready.set(false);
    let mut f = w.start();
    assert!(poll(&mut f).is_pending());
    h.fire(0, 30_000.);
    assert_eq!(poll(&mut f), Poll::Ready(Ok(false)));
    assert!(w.cancel_startup.borrow().is_none());
    let error = h
        .state
        .posts
        .borrow()
        .iter()
        .find(|p| p.op == StoreWorkerOp::Error)
        .unwrap()
        .data
        .clone()
        .unwrap();
    assert_eq!(
        error[1]["sentryData"]["tags"]["worker_error_phase"], "startup",
        "BOOT native timeout phase tag"
    );
    h.state.tasks.borrow_mut().clear();
    let (h, w) = fixture(false);
    h.state.load_missing.set(true);
    assert!(!finish(&h, w.start()).unwrap());
    assert!(w.exchange().is_none());
    assert!(w.cancel_startup.borrow().is_none());
    let unknown = h
        .state
        .posts
        .borrow()
        .iter()
        .find(|p| p.op == StoreWorkerOp::Error)
        .unwrap()
        .data
        .clone()
        .unwrap();
    assert_eq!(
        unknown[0],
        "Error starting exchange: bybit: Unknown exchange: bybit"
    );
    let src = source();
    assert_eq!(src["successEvents"], json!(success));
    assert_eq!(
        src["timeoutPhase"],
        error[1]["sentryData"]["tags"]["worker_error_phase"]
    );
    assert_eq!(src["unknownMessage"], unknown[0]);
}
#[test]
fn payload_routing_live_handler_arrays_and_buffer_floor() {
    let (h, w) = fixture(false);
    for _ in 0..1000 {
        w.handle_payload(payload(StoreWorkerOp::Exec, None))
            .unwrap();
    }
    assert_eq!(
        w.message_buffer.borrow().len(),
        1000,
        "BOOT native independent buffer ceiling"
    );
    w.handle_payload(payload(StoreWorkerOp::Exec, None))
        .unwrap();
    assert_eq!(w.message_buffer.borrow().len(), 1000);
    assert_eq!(
        h.state.posts.borrow().len(),
        1,
        "BOOT native overflow warning delivered"
    );
    assert_eq!(
        h.state.posts.borrow()[0].data.as_ref().unwrap()[1]["sendToSentry"],
        false
    );
    w.message_buffer.borrow_mut().clear();
    finish(&h, w.start()).unwrap();
    h.state.events.borrow_mut().clear();
    h.state.replace_handlers.set(true);
    w.handle_payload(payload(StoreWorkerOp::Exec, None))
        .unwrap();
    let routing = h.state.events.borrow().clone();
    assert_eq!(
        routing,
        [
            "handle:ohlcv:exec",
            "handler:method",
            "handler:ui",
            "handle:ui:exec"
        ],
        "BOOT native live replacement dispatch"
    );
    w.handle_payload(payload(StoreWorkerOp::Store, Some(json!({"latency":17}))))
        .unwrap();
    assert_eq!(
        w.store().serialize().latency.to_bits(),
        17f64.to_bits(),
        "BOOT native decoded store update"
    );
    w.handle_payload(payload(
        StoreWorkerOp::MergeLoaded,
        Some(json!({"tickers":true})),
    ))
    .unwrap();
    assert!(w.store().loaded().tickers);
    w.handle_payload(payload(
        StoreWorkerOp::MergeMessageHandlerStats,
        Some(json!({"private":1})),
    ))
    .unwrap();
    w.handle_payload(payload(
        StoreWorkerOp::MergeMessageHandlerStats,
        Some(json!({"public":2})),
    ))
    .unwrap();
    assert_eq!(
        w.store()
            .message_handler_stats()
            .as_ref()
            .unwrap()
            .get("private"),
        Some(&json!(1))
    );
    assert_eq!(
        w.store()
            .message_handler_stats()
            .as_ref()
            .unwrap()
            .get("public"),
        Some(&json!(2))
    );
    w.handle_payload(payload(StoreWorkerOp::Deactivate, None))
        .unwrap();
    assert!(w.exchange().is_none());
    assert_eq!(
        h.state.posts.borrow().last().unwrap().op,
        StoreWorkerOp::DeactivateStatus
    );
    assert_eq!(source()["routingEvents"], json!(routing));
}
#[test]
fn hosted_factory_selection_and_exact_event_bridge_registration() {
    let (h, w) = fixture(true);
    finish(&h, w.start()).unwrap();
    let events = h.state.events.borrow().clone();
    assert_eq!(
        events.iter().filter(|e| e.starts_with("ctor:")).count(),
        3,
        "BOOT native hosted dependent constructor count"
    );
    assert_eq!(
        events.iter().filter(|e| e.starts_with("handler:")).count(),
        9,
        "BOOT native nine ordered handlers"
    );
    assert!(w.postprocessors.borrow().borrow().is_empty());
    let exchange = w.exchange().unwrap();
    let log_events = [
        ("error", StoreWorkerOp::Error),
        ("warning", StoreWorkerOp::Warning),
        ("log", StoreWorkerOp::Log),
        ("exec-log", StoreWorkerOp::ExecLog),
        ("exec-result", StoreWorkerOp::ExecResult),
        ("chaser:complete", StoreWorkerOp::ChaserComplete),
        ("grafana-info", StoreWorkerOp::GrafanaInfo),
        ("grafana-error", StoreWorkerOp::GrafanaError),
        ("grafana-pws", StoreWorkerOp::GrafanaPws),
        ("grafana-pws-sub", StoreWorkerOp::GrafanaPwsSub),
        ("grafana-http-request", StoreWorkerOp::GrafanaHttpRequest),
        ("grafana-http-response", StoreWorkerOp::GrafanaHttpResponse),
    ];
    let before = h.state.posts.borrow().len();
    for (name, op) in log_events {
        exchange
            .base()
            .emit(
                name,
                safe_cex_exchange::js_runtime::value::Js::str("payload"),
            )
            .unwrap();
        let post = h.state.posts.borrow().last().unwrap().clone();
        assert_eq!(post.op, op, "BOOT native event op");
        assert_eq!(
            post.data,
            Some(json!(["payload"])),
            "BOOT native single-argument bridge"
        );
    }
    assert_eq!(
        h.state.posts.borrow().len() - before,
        12,
        "BOOT native independent log-event floor"
    );
    exchange
        .base()
        .emit(
            "fill",
            safe_cex_exchange::js_runtime::value::from_json(&json!({"orderId":"o","amount":3})),
        )
        .unwrap();
    assert_eq!(
        h.state.posts.borrow().last().unwrap().op,
        StoreWorkerOp::Fill,
        "BOOT native direct fill bridge op"
    );
    exchange
        .base()
        .emit(
            "ws-reconnected",
            safe_cex_exchange::js_runtime::value::Js::Null,
        )
        .unwrap();
    assert!(h.state.posts.borrow().last().unwrap().data.is_none());
    exchange
        .base()
        .emit("deactivate", safe_cex_exchange::js_runtime::value::Js::Null)
        .unwrap();
    assert!(w.exchange().is_none());
    assert!(exchange.base().is_disposed());
    assert_eq!(
        h.state.posts.borrow().last().unwrap().op,
        StoreWorkerOp::DeactivateStatus
    );
    assert_eq!(
        source()["hostedCtorEvents"],
        json!(
            events
                .iter()
                .filter(|e| e.starts_with("ctor:") || e.starts_with("handler:"))
                .cloned()
                .collect::<Vec<_>>()
        )
    );
}
fn init_payload() -> StoreWorkerPayload {
    payload(
        StoreWorkerOp::Init,
        Some(
            json!({"accountName":"a","exchangeName":"bybit","options":{"key":"","secret":""},"settings":{"placeOrder":{"tpTriggerType":"mark_price","slTriggerType":"last_price"}},"metadata":{"supportedTPSLTriggerTypes":[],"privateProxyIps":[]},"sessionKey":"s","store":{"latency":7}}),
        ),
    )
}
#[test]
fn session_registry_seed_reuse_precedence_mismatch_and_dispose() {
    let h = Host::new();
    h.state.expected_seed.set(Some(7.));
    let module = WorkerModule::new(h.clone());
    let table = module.exchange_workers();
    assert!(
        Rc::ptr_eq(&table, &module.exchange_workers()),
        "SESSION native live registry identity"
    );
    assert!(table.borrow().is_empty());
    let session = module.start(h.clone(), false);
    assert!(h.state.listener.borrow().is_some());
    finish(&h, h.send(payload(StoreWorkerOp::WorkerPing, None))).unwrap();
    assert_eq!(
        h.state.posts.borrow()[0].op,
        StoreWorkerOp::WorkerPong,
        "SESSION native heartbeat before initialization"
    );
    let mut init = init_payload();
    init.instance_id = Some("i".into());
    finish(&h, h.send(init)).unwrap();
    let worker = session.get_worker().unwrap();
    assert!(Rc::ptr_eq(&table.borrow()["a"], &worker));
    assert_eq!(
        worker.store().serialize().latency.to_bits(),
        7f64.to_bits(),
        "SESSION native store seeded before start"
    );
    assert_eq!(
        h.state.stores.borrow().last().unwrap().data.latency,
        Some(7.)
    );
    let before = h.state.events.borrow().len();
    for _ in 0..2 {
        finish(
            &h,
            h.send(StoreWorkerPayload {
                op: StoreWorkerOp::Exec,
                instance_id: Some("stale".into()),
                data: None,
            }),
        )
        .unwrap();
    }
    let after = h.state.events.borrow()[before..].to_vec();
    assert_eq!(
        after,
        ["console:warn"],
        "SESSION native one mismatch warning"
    );
    finish(
        &h,
        h.send(StoreWorkerPayload {
            op: StoreWorkerOp::WorkerPing,
            instance_id: Some("stale".into()),
            data: None,
        }),
    )
    .unwrap();
    assert_eq!(
        h.state.posts.borrow().last().unwrap().op,
        StoreWorkerOp::WorkerPong
    );
    let checked = table.clone();
    *h.state.dispose_check.borrow_mut() = Some(Rc::new(move || {
        assert!(
            checked.borrow().is_empty(),
            "SESSION native map absent inside venue disposal callback"
        )
    }));
    finish(&h, h.send(payload(StoreWorkerOp::Dispose, None))).unwrap();
    assert!(
        table.borrow().is_empty(),
        "SESSION native map removed before disposing"
    );
    assert!(worker.exchange().unwrap().base().is_disposed());
    let anonymous = Host::new();
    anonymous.state.expected_seed.set(Some(7.));
    let anon_module = WorkerModule::new(anonymous.clone());
    let anon_table = anon_module.anonymous_exchange_workers();
    assert!(
        Rc::ptr_eq(&anon_table, &anon_module.anonymous_exchange_workers()),
        "SESSION native live anonymous registry identity"
    );
    let anon_session = anon_module.start(anonymous.clone(), false);
    let mut anon_init = init_payload();
    anon_init.data.as_mut().unwrap()["accountName"] = json!("");
    finish(&anonymous, anonymous.send(anon_init)).unwrap();
    assert_eq!(
        anon_table.borrow().len(),
        1,
        "SESSION native anonymous insertion floor"
    );
    assert!(Rc::ptr_eq(
        &anon_table.borrow()["bybit"],
        &anon_session.get_worker().unwrap()
    ));
    assert!(anon_module.exchange_workers.borrow().is_empty());
    let checked = anon_table.clone();
    *anonymous.state.dispose_check.borrow_mut() = Some(Rc::new(move || {
        assert!(
            checked.borrow().is_empty(),
            "SESSION native anonymous map removed inside disposal"
        )
    }));
    finish(
        &anonymous,
        anonymous.send(payload(StoreWorkerOp::Dispose, None)),
    )
    .unwrap();
    assert!(anon_table.borrow().is_empty());
    let shared = Host::new();
    let module = WorkerModule::new(shared.clone());
    let session = module.start(shared.clone(), true);
    finish(&shared, shared.send(init_payload())).unwrap();
    let old = session.get_worker().unwrap();
    let starts = shared.state.completed_starts.get();
    finish(&shared, shared.send(init_payload())).unwrap();
    assert!(
        Rc::ptr_eq(&old, &session.get_worker().unwrap()),
        "SESSION native shared alive worker reuse"
    );
    assert_eq!(shared.state.completed_starts.get(), starts);
    assert_eq!(
        module.ports.ports.borrow()["account:a"].borrow().len(),
        1,
        "SESSION native deduplicated shared port"
    );
    let replay = Host::new();
    replay.state.replay.set(true);
    let module = WorkerModule::new(replay.clone());
    let _session = module.start(replay.clone(), false);
    finish(&replay, replay.send(init_payload())).unwrap();
    assert!(module.exchange_workers.borrow().is_empty());
    assert_eq!(
        &*replay.state.events.borrow(),
        &["listen", "post:start-status", "replay"],
        "SESSION native replay creates no venue"
    );
    let src = source();
    assert_eq!(src["seededLatency"].as_f64(), Some(7.));
    assert_eq!(src["mismatchWarnings"], 1);
    assert_eq!(src["sharedStarts"].as_f64(), Some(starts as f64));
    assert_eq!(
        src["replayEvents"],
        json!(["listen", "post:start-status", "replay"])
    );
}
