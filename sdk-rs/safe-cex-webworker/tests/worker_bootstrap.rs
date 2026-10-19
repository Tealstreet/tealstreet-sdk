#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_exchange::js_runtime::value::{Js, from_typed};
use safe_cex_exchange::runtime::ExchangeRuntime;
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
use safe_cex_webworker::perf_fixture::{PerfFixture, PerfFixtureHost};
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
    native_posts: RefCell<Vec<Js>>,
    handled_native: RefCell<Vec<Js>>,
    knob_inputs: RefCell<Vec<(&'static str, Js)>>,
    config_error: RefCell<Option<ExchangeError>>,
    console_args: RefCell<Vec<Js>>,
    stores: RefCell<Vec<RecordedStoreMessage>>,
    tasks: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    runtime: Rc<runtime::Runtime>,
    manager: Rc<ChaserOrderManager>,
    registry: RefCell<Option<Rc<WorkerProcessorRegistry>>>,
    handlers: RefCell<Vec<(Weak<dyn WorkerHandler>, WorkerHandlerKind)>>,
    listener: RefCell<Option<Rc<dyn Fn(Js) -> ExchangeFuture<'static, ()>>>>,
    worker: RefCell<Weak<ExchangeWorker<Host>>>,
    clock_ready: Rc<Cell<bool>>,
    load_ready: Rc<Cell<bool>>,
    start_ready: Rc<Cell<bool>>,
    clock_error: Cell<bool>,
    clock_waker: RefCell<Option<Waker>>,
    load_waker: RefCell<Option<Waker>>,
    start_waker: RefCell<Option<Waker>>,
    load_missing: Cell<bool>,
    completed_starts: Rc<Cell<u32>>,
    config: Rc<RefCell<Vec<WorkerConfigHandler<Js>>>>,
    replace_handlers: Cell<bool>,
    fail_handler_dispose: Cell<bool>,
}
struct Host {
    state: Rc<State>,
    self_ref: Weak<Host>,
    proxy: Option<(
        bool,
        Rc<RefCell<String>>,
        Rc<WorkerPorts<Js, Host, Host>>,
        Rc<PerfFixture<Host>>,
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
            self.state.runtime.advance(self.state.runtime.now.get());
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
        cb(from_typed(&payload).unwrap())
    }
    fn forward(&self, p: StoreWorkerPayload) -> Result<(), ExchangeError> {
        self.state.posts.borrow_mut().push(p.clone());
        self.forward_native(from_typed(&p)?)
    }
    fn forward_native(&self, value: Js) -> Result<(), ExchangeError> {
        let name = String::from_utf16_lossy(&value.get("op")?.to_js_string()?);
        if let Some((shared, key, ports, perf)) = &self.proxy {
            self.event(format!("record:{name}"));
            perf.record(&value)?;
            if *shared {
                ports.broadcast_with_throttle(&key.borrow(), Rc::new(value), None);
                return Ok(());
            }
        }
        if name != "store" {
            self.event(format!("post:{name}"));
        }
        self.state.native_posts.borrow_mut().push(value.clone());
        Ok(())
    }
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
        if self
            .host
            .upgrade()
            .unwrap()
            .state
            .fail_handler_dispose
            .replace(false)
        {
            return Err(ExchangeError::new("handler dispose failed"));
        }
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
struct RecordedStoreMessage {
    #[allow(dead_code)]
    op: safe_cex_webworker::payload::StoreWorkerOp,
    data: safe_cex_store::StoreUpdate,
}
impl ExchangeWorkerHost for Host {
    fn post_store(&self, m: WorkerStoreMessage) -> Result<(), ExchangeError> {
        self.forward_native(Js::object(vec![
            ("op", from_typed(&m.op)?),
            ("data", m.data.clone()),
        ]))?;
        self.state.stores.borrow_mut().push(RecordedStoreMessage {
            op: m.op,
            data: safe_cex_exchange::js_runtime::value::to_typed_preserving_undefined(&m.data)
                .unwrap(),
        });
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
    fn runtime(&self) -> Rc<dyn safe_cex_exchange::runtime::ExchangeRuntime> {
        self.state.runtime.clone()
    }
    fn construct_clock_start(&self) -> ExchangeFuture<'static, ()> {
        self.event("clock");
        let ready = self.state.clock_ready.clone();
        let failed = self.state.clock_error.get();
        let state = self.state.clone();
        Box::pin(std::future::poll_fn(move |cx| {
            if ready.get() {
                Poll::Ready(if failed {
                    Err(ExchangeError::new("clock failed"))
                } else {
                    Ok(())
                })
            } else {
                *state.clock_waker.borrow_mut() = Some(cx.waker().clone());
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
        let state = self.state.clone();
        Some(Box::pin(std::future::poll_fn(move |cx| {
            if ready.get() {
                Poll::Ready(Ok(factory.clone()))
            } else {
                *state.load_waker.borrow_mut() = Some(cx.waker().clone());
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
        payload: Js,
    ) -> Result<(), ExchangeError> {
        self.state.handled_native.borrow_mut().push(payload.clone());
        let k = self
            .state
            .handlers
            .borrow()
            .iter()
            .find_map(|(w, k)| w.upgrade().filter(|v| Rc::ptr_eq(v, &handler)).map(|_| *k))
            .unwrap();
        self.event(format!(
            "handle:{}:{}",
            kind(k),
            String::from_utf16_lossy(&payload.get("op")?.to_js_string()?)
        ));
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
        self.forward_native(Js::object(vec![
            ("op", from_typed(&payload.op)?),
            ("data", payload.data),
        ]))
    }

    fn spawn_start_task(&self, f: ExchangeFuture<'static, ()>) {
        self.state.tasks.borrow_mut().push(f)
    }
    fn construct_exchange_start(&self, ex: Rc<dyn Exchange>) -> ExchangeFuture<'static, ()> {
        if let Some(expected) = self.state.expected_seed.get() {
            assert_eq!(
                ex.base().store.serialize().unwrap().latency.to_bits(),
                expected.to_bits(),
                "SESSION native store seeded inside venue start prefix"
            );
        }
        self.event("start");
        let ready = self.state.start_ready.clone();
        let count = self.state.completed_starts.clone();
        let state = self.state.clone();
        Box::pin(std::future::poll_fn(move |cx| {
            if ready.get() {
                count.set(count.get() + 1);
                Poll::Ready(Ok(()))
            } else {
                *state.start_waker.borrow_mut() = Some(cx.waker().clone());
                Poll::Pending
            }
        }))
    }
    fn error_properties(&self, e: &ExchangeError) -> Js {
        safe_cex_exchange::js_runtime::value::Thrown::from(e.clone()).value
    }
    fn materialize_error(&self, e: &ExchangeError) -> Result<Js, ExchangeError> {
        let raw = self.error_properties(e);
        Ok(Js::object(vec![
            ("name", raw.get("name")?),
            ("message", raw.get("message")?),
        ]))
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
impl WorkerPort<Js> for Host {
    type Error = ExchangeError;
    fn post_message(&self, p: &Rc<Js>) -> Result<(), ExchangeError> {
        self.event(format!(
            "post:{}",
            String::from_utf16_lossy(&p.get("op")?.to_js_string()?)
        ));
        self.state.native_posts.borrow_mut().push((**p).clone());
        Ok(())
    }
}
impl PerfFixtureHost for Host {
    fn report_perf_timer_error(&self, error: &ExchangeError) {
        self.state
            .runtime
            .report_exception(error, &json!({"source":"perfTimer"}).into());
    }
}
impl WorkerSessionHost for Host {
    fn install_message_listener(
        &self,
        l: Rc<dyn Fn(Js) -> ExchangeFuture<'static, ()>>,
    ) -> Box<dyn FnOnce()> {
        assert!(self.state.listener.borrow().is_none());
        self.event("listen");
        *self.state.listener.borrow_mut() = Some(l);
        let state = Rc::downgrade(&self.state);
        Box::new(move || {
            if let Some(state) = state.upgrade() {
                state.listener.borrow_mut().take();
            }
        })
    }
    fn worker_context(
        &self,
        shared: bool,
        key: Rc<RefCell<String>>,
        ports: Rc<WorkerPorts<Js, Self, Self>>,
        perf: Rc<PerfFixture<Self>>,
    ) -> Rc<Self> {
        self.event(format!("proxy:{shared}"));
        Rc::new_cyclic(|w| Self {
            state: self.state.clone(),
            self_ref: w.clone(),
            proxy: Some((shared, key, ports, perf)),
        })
    }
    fn set_tick_interval_multipliers(&self, value: Js) -> Result<(), ExchangeError> {
        self.state.knob_inputs.borrow_mut().push(("tick", value));
        self.event("tick-config");
        Ok(())
    }
    fn set_ws_diagnostics_enabled(&self, enabled: bool) -> Result<(), ExchangeError> {
        self.event(format!("diagnostics:{enabled}"));
        Ok(())
    }
    fn config_handlers(&self) -> Rc<RefCell<Vec<WorkerConfigHandler<Js>>>> {
        self.state.config.clone()
    }
    fn invoke_config_handler(
        &self,
        h: WorkerConfigHandler<Js>,
        cfg: &Js,
    ) -> Result<(), ExchangeError> {
        h(cfg);
        match self.state.config_error.borrow_mut().take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
    fn sync_server_time(&self, t: f64) -> Result<(), ExchangeError> {
        self.event(format!("sync:{}", safe_cex_num::js_number_string(t)));
        Ok(())
    }
    fn console(&self, level: &str, _: &str, value: Option<Js>) -> Result<(), ExchangeError> {
        if let Some(value) = value {
            self.state.console_args.borrow_mut().push(value);
        }
        self.event(format!("console:{level}"));
        Ok(())
    }
    fn shutdown_context(&self) -> Result<(), ExchangeError> {
        let tasks = std::mem::take(&mut *self.state.tasks.borrow_mut());
        drop(tasks);
        let tasks = std::mem::take(&mut *self.state.runtime.tasks.borrow_mut());
        drop(tasks);
        self.state.timers.borrow_mut().clear();
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
fn native_payload(op: StoreWorkerOp, data: Option<Value>) -> Js {
    from_typed(&payload(op, data)).unwrap()
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
    w.handle_payload(native_payload(StoreWorkerOp::Exec, None))
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
    h.drain();
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
    h.drain();
    w.dispose().unwrap();
    assert!(poll(&mut f).is_pending());
    h.drain();
    assert_eq!(poll(&mut f), Poll::Ready(Ok(false)));
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
    h.drain();
    h.fire(0, 30_000.);
    assert!(poll(&mut f).is_pending());
    h.drain();
    assert_eq!(poll(&mut f), Poll::Ready(Ok(false)));
    assert!(w.cancel_startup.borrow().is_none());
    let error = h
        .state
        .native_posts
        .borrow()
        .iter()
        .find(|p| {
            p.get("op").unwrap().to_js_string().unwrap().as_ref()
                == "error".encode_utf16().collect::<Vec<_>>().as_slice()
        })
        .map(|p| safe_cex_exchange::js_runtime::value::to_json(&p.get("data").unwrap()).unwrap())
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
        .native_posts
        .borrow()
        .iter()
        .find(|p| {
            p.get("op").unwrap().to_js_string().unwrap().as_ref()
                == "error".encode_utf16().collect::<Vec<_>>().as_slice()
        })
        .map(|p| safe_cex_exchange::js_runtime::value::to_json(&p.get("data").unwrap()).unwrap())
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
        w.handle_payload(native_payload(StoreWorkerOp::Exec, None))
            .unwrap();
    }
    assert_eq!(
        w.message_buffer.borrow().len(),
        1000,
        "BOOT native independent buffer ceiling"
    );
    w.handle_payload(native_payload(StoreWorkerOp::Exec, None))
        .unwrap();
    assert_eq!(w.message_buffer.borrow().len(), 1000);
    assert_eq!(
        h.state.native_posts.borrow().len(),
        1,
        "BOOT native overflow warning delivered"
    );
    assert_eq!(
        safe_cex_exchange::js_runtime::value::to_json(
            &h.state.native_posts.borrow()[0].get("data").unwrap()
        )
        .unwrap()[1]["sendToSentry"],
        false
    );
    w.message_buffer.borrow_mut().clear();
    finish(&h, w.start()).unwrap();
    h.state.events.borrow_mut().clear();
    h.state.replace_handlers.set(true);
    w.handle_payload(native_payload(StoreWorkerOp::Exec, None))
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
    w.handle_payload(native_payload(
        StoreWorkerOp::Store,
        Some(json!({"latency":17})),
    ))
    .unwrap();
    assert_eq!(
        w.store().serialize().unwrap().latency.to_bits(),
        17f64.to_bits(),
        "BOOT native decoded store update"
    );
    w.handle_payload(native_payload(
        StoreWorkerOp::MergeLoaded,
        Some(json!({"tickers":true})),
    ))
    .unwrap();
    assert!(w.store().loaded().unwrap().tickers);
    w.handle_payload(native_payload(
        StoreWorkerOp::MergeMessageHandlerStats,
        Some(json!({"private":1})),
    ))
    .unwrap();
    w.handle_payload(native_payload(
        StoreWorkerOp::MergeMessageHandlerStats,
        Some(json!({"public":2})),
    ))
    .unwrap();
    assert_eq!(
        w.store()
            .message_handler_stats()
            .unwrap()
            .as_ref()
            .unwrap()
            .get("private")
            .and_then(Value::as_f64)
            .map(f64::to_bits),
        Some((1f64).to_bits())
    );
    assert_eq!(
        w.store()
            .message_handler_stats()
            .unwrap()
            .as_ref()
            .unwrap()
            .get("public")
            .and_then(Value::as_f64)
            .map(f64::to_bits),
        Some((2f64).to_bits())
    );
    let stats = safe_cex_exchange::js_runtime::store::facade(w.store())
        .unwrap()
        .get("messageHandlerStats")
        .unwrap();
    let bits = |key: &str| {
        let Js::Number(value) = stats.get(key).unwrap() else {
            panic!("native message-handler stat {key} must be a number");
        };
        format!("{:016x}", value.to_bits())
    };
    let profile = json!({
        "privateBits": bits("private"),
        "publicBits": bits("public"),
        "keys": stats.own_keys().iter().map(|key| String::from_utf16(key).unwrap()).collect::<Vec<_>>(),
        "wire": String::from_utf16(&safe_cex_exchange::js_runtime::globals::stringify_json(&stats).unwrap().unwrap()).unwrap(),
    });
    let expected = source();
    assert_eq!(profile, expected["routingStats"]);
    w.handle_payload(native_payload(StoreWorkerOp::Deactivate, None))
        .unwrap();
    assert!(w.exchange().is_none());
    assert_eq!(
        h.state.posts.borrow().last().unwrap().op,
        StoreWorkerOp::DeactivateStatus
    );
    assert_eq!(expected["routingEvents"], json!(routing));
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
    let before = h.state.native_posts.borrow().len();
    for (name, op) in log_events {
        exchange
            .base()
            .emit(
                name,
                safe_cex_exchange::js_runtime::value::Js::str("payload"),
            )
            .unwrap();
        let post = h.state.native_posts.borrow().last().unwrap().clone();
        assert!(
            post.get("op")
                .unwrap()
                .strict_equals(&from_typed(&op).unwrap()),
            "BOOT native event op"
        );
        assert_eq!(
            safe_cex_exchange::js_runtime::value::to_json(&post.get("data").unwrap()).unwrap(),
            json!(["payload"]),
            "BOOT native single-argument bridge"
        );
    }
    assert_eq!(
        h.state.native_posts.borrow().len() - before,
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
        safe_cex_exchange::js_runtime::value::to_typed::<StoreWorkerOp>(
            &h.state
                .native_posts
                .borrow()
                .last()
                .unwrap()
                .get("op")
                .unwrap()
        )
        .unwrap(),
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
    assert!(
        !h.state
            .native_posts
            .borrow()
            .last()
            .unwrap()
            .has_own("data")
    );
    exchange
        .base()
        .emit("deactivate", safe_cex_exchange::js_runtime::value::Js::Null)
        .unwrap();
    assert!(w.exchange().is_none());
    assert!(exchange.base().is_disposed());
    assert_eq!(
        safe_cex_exchange::js_runtime::value::to_typed::<StoreWorkerOp>(
            &h.state
                .native_posts
                .borrow()
                .last()
                .unwrap()
                .get("op")
                .unwrap()
        )
        .unwrap(),
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
    assert!(
        h.state.native_posts.borrow()[0]
            .get("op")
            .unwrap()
            .strict_equals(&Js::str("__worker_pong__")),
        "SESSION native heartbeat before initialization"
    );
    let mut init = init_payload();
    init.instance_id = Some("i".into());
    finish(&h, h.send(init)).unwrap();
    let worker = session.get_worker().unwrap();
    assert!(Rc::ptr_eq(&table.borrow()["a"], &worker));
    assert_eq!(
        worker.store().serialize().unwrap().latency.to_bits(),
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
    assert!(
        h.state
            .native_posts
            .borrow()
            .last()
            .unwrap()
            .get("op")
            .unwrap()
            .strict_equals(&Js::str("__worker_pong__"))
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
    let module = WorkerModule::new(replay.clone());
    module
        .perf
        .configure(Js::object(vec![("mode", Js::str("replay"))]))
        .unwrap();
    let _session = module.start(replay.clone(), false);
    finish(&replay, replay.send(init_payload())).unwrap();
    assert!(module.exchange_workers.borrow().is_empty());
    assert_eq!(
        &*replay.state.events.borrow(),
        &["listen", "post:start-status"],
        "SESSION native replay creates no venue"
    );
    let src = source();
    assert_eq!(src["seededLatency"].as_f64(), Some(7.));
    assert_eq!(src["mismatchWarnings"], 1);
    assert_eq!(src["sharedStarts"].as_f64(), Some(starts as f64));
    assert_eq!(src["replayEvents"], json!(["listen", "post:start-status"]));
}

fn startup_timing_source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_bootstrap_oracle.mjs"
        ))
        .arg("--startup-timing")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "STARTUP source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn release_clock(h: &Host) {
    h.state.clock_ready.set(true);
    if let Some(waker) = h.state.clock_waker.borrow_mut().take() {
        waker.wake();
    }
}
#[test]
fn startup_dropped_waiter_keeps_clock_loader_and_snapshot_running() {
    let source = startup_timing_source();
    let mut profiles = Vec::new();
    for mode in ["success", "failure", "dispose", "loader", "venue"] {
        let (h, w) = fixture(false);
        h.state.clock_ready.set(matches!(mode, "loader" | "venue"));
        h.state.load_ready.set(mode != "loader");
        h.state.start_ready.set(mode != "venue");
        h.state.clock_error.set(mode == "failure");
        let waiter = w.start();
        assert!(w.cancel_startup.borrow().is_some());
        assert_eq!(h.state.events.borrow().last().unwrap(), "clock");
        drop(waiter);
        h.drain();
        assert_eq!(w.exchange().is_some(), mode == "venue");
        if mode == "dispose" {
            w.dispose().unwrap();
        }
        match mode {
            "loader" => {
                h.state.load_ready.set(true);
                if let Some(waker) = h.state.load_waker.borrow_mut().take() {
                    waker.wake();
                }
            }
            "venue" => {
                h.state.start_ready.set(true);
                if let Some(waker) = h.state.start_waker.borrow_mut().take() {
                    waker.wake();
                }
            }
            _ => release_clock(&h),
        }
        h.drain();
        profiles.push(json!({"mode":mode, "completed":h.state.completed_starts.get(),
            "stores":h.state.stores.borrow().len(), "errors":h.state.native_posts.borrow().iter().filter(|p|p.get("op").unwrap().to_js_string().unwrap().as_ref()=="error".encode_utf16().collect::<Vec<_>>().as_slice()).count(),
            "exchange":w.exchange().is_some(), "cancelPending":w.cancel_startup.borrow().is_some()}));
    }
    assert_eq!(
        Value::Array(profiles),
        source["direct"],
        "STARTUP dropped waiter retains actual worker continuation"
    );
}
#[test]
fn startup_ready_clock_yields_before_loader_and_caller_settlement() {
    let (h, w) = fixture(false);
    let mut waiter = w.start();
    assert_eq!(h.state.events.borrow().last().unwrap(), "clock");
    assert!(poll(&mut waiter).is_pending());
    assert!(
        !h.state.events.borrow().iter().any(|e| e == "load"),
        "STARTUP ready clock must yield before loader"
    );
    h.drain();
    assert_eq!(finish(&h, waiter), Ok(true));
    assert_eq!(h.state.completed_starts.get(), 1);
}
#[test]
fn session_dropped_init_waiter_keeps_start_and_failure_cleanup_running() {
    let source = startup_timing_source();
    let mut profiles = Vec::new();
    for missing in [false, true] {
        let h = Host::new();
        h.state.load_missing.set(missing);
        h.state.expected_seed.set(Some(7.));
        let module = WorkerModule::new(h.clone());
        let session = module.start(h.clone(), false);
        let init = payload(
            StoreWorkerOp::Init,
            Some(
                json!({"accountName":"a", "exchangeName":"bybit", "options":{"key":"","secret":""},
            "settings":{"placeOrder":{"tpTriggerType":"mark_price","slTriggerType":"last_price"}},
            "metadata":{"supportedTPSLTriggerTypes":[],"privateProxyIps":[]},"sessionKey":"s","store":{"latency":7}}),
            ),
        );
        let waiter = h.send(init);
        let worker = session.get_worker().unwrap();
        assert_eq!(
            worker.store().serialize().unwrap().latency.to_bits(),
            7f64.to_bits()
        );
        drop(waiter);
        h.drain();
        profiles.push(json!({"missing":missing,"completed":h.state.completed_starts.get(),"stores":h.state.stores.borrow().len(),
            "present":session.get_worker().is_some(),"cancelPending":worker.cancel_startup.borrow().is_some()}));
    }
    assert_eq!(
        Value::Array(profiles),
        source["session"],
        "STARTUP abandoned listener retains init continuation and failed-worker cleanup"
    );
}

fn session_native_source() -> Value {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_bootstrap_oracle.mjs"
        ))
        .arg("--session-native")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SESSION source native ingress: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
#[test]
fn session_module_owns_installed_listener_after_start_return_is_dropped() {
    let source = session_native_source();
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    drop(module.start(host.clone(), false));
    let result = finish(&host, host.send(payload(StoreWorkerOp::WorkerPing, None)));
    assert!(
        result.is_ok(),
        "SESSION module-owned listener must survive dropped start return: {result:?}"
    );
    assert_eq!(
        host.state.native_posts.borrow().len(),
        source["lifetimePongs"].as_u64().unwrap() as usize
    );
}
#[test]
fn session_array_config_reaches_registered_host_handler() {
    let source = session_native_source();
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    let _session = module.start(host.clone(), false);
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    host.state
        .config
        .borrow_mut()
        .push(Rc::new(move |_| count.set(count.get() + 1)));
    let result = finish(
        &host,
        host.send(payload(StoreWorkerOp::WorkerConfig, Some(json!([])))),
    );
    assert!(
        result.is_ok(),
        "SESSION array config admission matches source: {result:?}"
    );
    assert_eq!(calls.get(), source["arrayCalls"].as_u64().unwrap());
}
#[test]
fn session_raw_config_retains_native_values_through_listener() {
    use safe_cex_exchange::js_runtime::value::Js;
    let source = session_native_source();
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    let _session = module.start(host.clone(), false);
    let seen = Rc::new(RefCell::new(Vec::new()));
    let observations = seen.clone();
    host.state.config.borrow_mut().push(Rc::new(move |cfg| {
        observations.borrow_mut().push(cfg.clone())
    }));
    let cfg = Js::object(vec![
        ("custom", Js::Number(f64::NAN)),
        ("present", Js::Undefined),
        ("text", Js::String(vec![0xd800, 88, 0xdc00].into())),
    ]);
    let mut deep = Js::Undefined;
    for _ in 0..150 {
        deep = Js::array(vec![deep]);
    }
    cfg.define("deep", deep).unwrap();
    let original = cfg.clone();
    let wire = Js::object(vec![("op", Js::str("__workerConfig")), ("data", cfg)]);
    let listener = host.state.listener.borrow().clone().unwrap();
    finish(&host, listener(wire)).unwrap();
    let received = seen.borrow()[0].clone();
    assert!(
        received.strict_equals(&original),
        "SESSION raw config receiver identity"
    );
    let mut deep = received.get("deep").unwrap();
    for _ in 0..150 {
        deep = deep.get("0").unwrap();
    }
    assert!(
        deep.is_undefined(),
        "SESSION unread deep config reaches callback intact"
    );
    let custom = received.get("custom").unwrap();
    assert!(
        matches!(custom,Js::Number(n) if n.is_nan()),
        "SESSION raw config must retain native NaN"
    );
    assert!(
        received.has_own("present"),
        "SESSION config retains own undefined"
    );
    assert!(received.get("present").unwrap().is_undefined());
    let text = received.get("text").unwrap();
    let units = text.as_str().unwrap().to_vec();
    assert_eq!(units, vec![0xd800, 88, 0xdc00]);
    assert_eq!(
        json!({"nan":true,"undefinedOwn":true,"textUnits":units}),
        source["raw"]
    );
}

#[test]
fn session_module_drop_detaches_callbacks_and_disposes_held_startup() {
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    let session = module.start(host.clone(), false);
    host.state.start_ready.set(false);
    drop(host.send(init_payload()));
    host.drain();
    let worker = session.get_worker().unwrap();
    let exchange = worker.exchange().unwrap();
    let weak = Rc::downgrade(&worker);
    drop(module);
    assert!(
        host.state.listener.borrow().is_none(),
        "SESSION module teardown unregisters listener"
    );
    assert!(
        exchange.base().is_disposed(),
        "SESSION module teardown disposes actual exchange"
    );
    assert!(worker.cancel_startup.borrow().is_none());
    assert!(worker.handlers.borrow().borrow().is_empty());
    assert!(
        host.state.timers.borrow().is_empty(),
        "SESSION teardown removes timeout losers"
    );
    assert!(host.state.tasks.borrow().is_empty());
    assert!(
        host.state.runtime.tasks.borrow().is_empty(),
        "SESSION teardown cancels canonical jobs"
    );
    let stores = host.state.stores.borrow().len();
    host.state.start_ready.set(true);
    host.drain();
    assert_eq!(
        host.state.stores.borrow().len(),
        stores,
        "SESSION no late startup snapshot after teardown"
    );
    assert!(session.get_worker().is_none());
    assert!(
        finish(
            &host,
            session.handle(Js::object(vec![("op", Js::str("__worker_ping__"))]))
        )
        .is_err()
    );
    drop(worker);
    assert!(
        weak.upgrade().is_none(),
        "SESSION shutdown releases worker continuation ownership"
    );
}
#[test]
fn session_raw_dispatch_preserves_unknown_ops_and_buffered_object_identity() {
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    let session = module.start(host.clone(), false);
    finish(&host, host.send(init_payload())).unwrap();
    let wire = Js::object(vec![
        ("op", Js::str("external-op")),
        (
            "data",
            Js::object(vec![
                ("value", Js::Number(f64::INFINITY)),
                ("own", Js::Undefined),
            ]),
        ),
    ]);
    let listener = host.state.listener.borrow().clone().unwrap();
    finish(&host, listener(wire.clone())).unwrap();
    assert_eq!(
        host.state.handled_native.borrow().len(),
        session_native_source()["dispatch"].as_u64().unwrap() as usize
    );
    for received in host.state.handled_native.borrow().iter() {
        assert!(received.strict_equals(&wire));
    }
    let worker = session.get_worker().unwrap();
    assert!(worker.exchange().is_some());
    let (other, buffered) = fixture(false);
    let payload = wire.clone();
    buffered.handle_payload(payload.clone()).unwrap();
    assert!(buffered.message_buffer.borrow()[0].strict_equals(&payload));
    payload
        .get("data")
        .unwrap()
        .set("value", Js::Number(f64::NEG_INFINITY))
        .unwrap();
    finish(&other, buffered.start()).unwrap();
    assert_eq!(
        other.state.handled_native.borrow().len(),
        session_native_source()["buffered"].as_u64().unwrap() as usize
    );
    for received in other.state.handled_native.borrow().iter() {
        assert!(received.strict_equals(&payload));
        assert!(
            matches!(received.get("data").unwrap().get("value").unwrap(), Js::Number(value) if value.to_bits()==f64::NEG_INFINITY.to_bits())
        );
    }
}

#[test]
fn session_replay_init_skips_unread_constructor_inputs() {
    let source = session_native_source();
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    module
        .perf
        .configure(Js::object(vec![("mode", Js::str("replay"))]))
        .unwrap();
    let _session = module.start(host.clone(), false);
    let message = Js::object(vec![("op", Js::str("init")), ("data", Js::object(vec![]))]);
    let listener = host.state.listener.borrow().clone().unwrap();
    finish(&host, listener(message)).unwrap();
    assert!(module.exchange_workers.borrow().is_empty());
    assert!(module.anonymous_exchange_workers.borrow().is_empty());
    assert_eq!(
        serde_json::json!(*host.state.events.borrow()),
        source["replayEvents"]
    );
}

#[test]
fn session_config_keeps_setter_arguments_live_order_and_original_throw() {
    use safe_cex_exchange::js_runtime::value::Thrown;
    let source = session_native_source();
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    let _session = module.start(host.clone(), false);
    let tick = Js::object(vec![("bybit", Js::Number(f64::INFINITY))]);
    let replacement = Js::object(vec![("mode", Js::str("off")), ("present", Js::Undefined)]);
    let cfg = Js::object(vec![
        ("tickIntervalMultipliers", tick.clone()),
        ("wsDiagnosticsEnabled", Js::Number(1.)),
        ("perfFixture", Js::Null),
    ]);
    let cause = Js::object(vec![("marker", Js::Number(f64::NAN))]);
    *host.state.config_error.borrow_mut() = Some(Thrown::new(cause.clone()).to_exchange_error());
    let weak = Rc::downgrade(&host);
    let expected = cfg.clone();
    let next = replacement.clone();
    host.state.config.borrow_mut().push(Rc::new(move |value| {
        assert!(value.strict_equals(&expected));
        value.set("perfFixture", next.clone()).unwrap();
        let host = weak.upgrade().unwrap();
        host.event("config:first");
        let weak = Rc::downgrade(&host);
        host.state.config.borrow_mut().push(Rc::new(move |_| {
            weak.upgrade().unwrap().event("config:second")
        }));
    }));
    let listener = host.state.listener.borrow().clone().unwrap();
    finish(
        &host,
        listener(Js::object(vec![
            ("op", Js::str("__workerConfig")),
            ("data", cfg),
        ])),
    )
    .unwrap();
    let inputs = host.state.knob_inputs.borrow();
    assert_eq!(inputs.len(), 1);
    assert_eq!(inputs[0].0, "tick");
    assert!(inputs[0].1.strict_equals(&tick));
    assert!(
        matches!(inputs[0].1.get("bybit").unwrap(), Js::Number(value) if value.to_bits()==f64::INFINITY.to_bits())
    );
    assert!(!module.perf.is_recording());
    assert!(!module.perf.is_replaying());
    assert!(replacement.has_own("present"));
    assert!(
        host.state.console_args.borrow()[0].strict_equals(&cause),
        "SESSION config console keeps original non-Error throw"
    );
    assert_eq!(json!(*host.state.events.borrow()), source["configOrder"]);
}
#[test]
fn session_perf_dump_uses_native_port_before_any_json_projection() {
    let source = session_native_source();
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    let _session = module.start(host.clone(), false);
    let value = Js::object(vec![
        ("version", Js::Number(1.)),
        (
            "frames",
            Js::array(vec![Js::object(vec![
                ("atMs", Js::Number(-0.)),
                (
                    "msg",
                    Js::object(vec![("nan", Js::Number(f64::NAN)), ("own", Js::Undefined)]),
                ),
            ])]),
        ),
    ]);
    module
        .perf
        .take()
        .unwrap()
        .get("frames")
        .unwrap()
        .set("0", value.get("frames").unwrap().get("0").unwrap())
        .unwrap();
    let listener = host.state.listener.borrow().clone().unwrap();
    finish(
        &host,
        listener(Js::object(vec![("op", Js::str("__perfFixtureDump"))])),
    )
    .unwrap();
    let posted = host.state.native_posts.borrow()[0].get("data").unwrap();
    assert!(
        posted
            .get("frames")
            .unwrap()
            .strict_equals(&module.perf.take().unwrap().get("frames").unwrap())
    );
    let frame = posted.get("frames").unwrap().get("0").unwrap();
    assert!(
        matches!(frame.get("atMs").unwrap(),Js::Number(number) if number.to_bits()==(-0f64).to_bits())
    );
    let msg = frame.get("msg").unwrap();
    assert!(msg.has_own("own"));
    assert!(msg.get("own").unwrap().is_undefined());
    assert!(matches!(msg.get("nan").unwrap(),Js::Number(number) if number.is_nan()));
    assert_eq!(source["dumpNative"], json!(true));
}

#[test]
fn session_context_teardown_continues_after_handler_failure() {
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    let session = module.start(host.clone(), false);
    finish(&host, host.send(init_payload())).unwrap();
    let worker = session.get_worker().unwrap();
    host.state.events.borrow_mut().clear();
    host.state.fail_handler_dispose.set(true);
    drop(module);
    let disposed = host
        .state
        .events
        .borrow()
        .iter()
        .filter(|event| event.starts_with("dispose:"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        disposed,
        vec![
            "dispose:ohlcv",
            "dispose:book",
            "dispose:chaser",
            "dispose:twap",
            "dispose:dev",
            "dispose:command",
            "dispose:positions",
            "dispose:method",
            "dispose:ui"
        ],
        "SESSION native context must dispose every handler exactly once after failure"
    );
    assert!(worker.handlers.borrow().borrow().is_empty());
    assert!(worker.postprocessors.borrow().borrow().is_empty());
    assert!(
        worker
            .exchange_dependent_postprocessors
            .borrow()
            .borrow()
            .is_empty()
    );
    assert!(host.state.listener.borrow().is_none());
    assert!(host.state.runtime.tasks.borrow().is_empty());
    assert_eq!(host.state.runtime.exceptions.borrow().len(), 1);
    assert_eq!(
        host.state.runtime.exceptions.borrow()[0].0.message,
        "handler dispose failed"
    );
}

#[test]
fn normal_worker_dispose_still_stops_at_the_source_handler_throw() {
    let source = session_native_source();
    let (host, worker) = fixture(false);
    finish(&host, worker.start()).unwrap();
    host.state.events.borrow_mut().clear();
    host.state.fail_handler_dispose.set(true);
    let error = worker.dispose().unwrap_err();
    assert_eq!(error.message, "handler dispose failed");
    let disposed = host
        .state
        .events
        .borrow()
        .iter()
        .filter(|event| event.starts_with("dispose:"))
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        json!({"disposed":disposed,"handlers":worker.handlers.borrow().borrow().len(),"processors":worker.postprocessors.borrow().borrow().len(),"dependent":worker.exchange_dependent_postprocessors.borrow().borrow().len()}),
        source["normalDisposeThrow"],
        "SESSION native context policy must not change normal source dispose failure ordering"
    );
}

#[test]
fn config_and_fixture_carriers_reach_real_listener_without_legacy_validation() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_bootstrap_oracle.mjs"
        ))
        .arg("--config-carriers")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CONFIG-CARRIER source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let tokens = [
        "-0", "-0.0", "0", "1e400", "-1e400", "1e-400", "-1e-400", "1.25", "1e3", "-1e-4000",
    ];
    assert_eq!(source["tokens"], json!(tokens));
    assert_eq!(source["bits"].as_array().unwrap().len(), 10);
    let host = Host::new();
    let module = WorkerModule::new(host.clone());
    let _session = module.start(host.clone(), false);
    let listener = host.state.listener.borrow().clone().unwrap();
    let seen = Rc::new(RefCell::new(Vec::<Js>::new()));
    let received = seen.clone();
    host.state
        .config
        .borrow_mut()
        .push(Rc::new(move |cfg| received.borrow_mut().push(cfg.clone())));
    let mut configs = 0;
    let mut dumps = 0;
    let mut bit_checks = 0;
    for (index, token) in tokens.iter().enumerate() {
        let number = safe_cex_num::number_from_string(token);
        let expected_bits =
            u64::from_str_radix(source["bits"][index].as_str().unwrap(), 16).unwrap();
        assert_eq!(number.to_bits(), expected_bits);
        let msg = Js::object(vec![
            ("value", Js::Number(number)),
            ("own", Js::Undefined),
            ("text", Js::String(vec![0xd800, 88, 0xdc00].into())),
        ]);
        let frame = Js::object(vec![("atMs", Js::Number(number)), ("msg", msg.clone())]);
        let fixture = Js::object(vec![
            ("version", Js::Number(2.0)),
            ("recordedAtMs", Js::Number(number)),
            ("durationMs", Js::Number(number)),
            ("frames", Js::array(vec![frame.clone()])),
        ]);
        let perf = Js::object(vec![
            ("mode", Js::str("external-mode")),
            ("intervalMs", Js::Number(number)),
            ("fixture", fixture.clone()),
            ("loop", Js::Null),
        ]);
        let tick = Js::object(vec![("bybit", Js::Number(number))]);
        let cfg = Js::object(vec![
            ("tickIntervalMultipliers", tick.clone()),
            ("perfFixture", perf.clone()),
        ]);
        finish(
            &host,
            listener(Js::object(vec![
                ("op", Js::str("__workerConfig")),
                ("data", cfg.clone()),
            ])),
        )
        .unwrap();
        assert!(
            seen.borrow().last().unwrap().strict_equals(&cfg),
            "CONFIG-CARRIER callback identity"
        );
        let inputs = host.state.knob_inputs.borrow();
        assert_eq!(inputs.len(), index + 1);
        assert!(
            inputs[index].1.strict_equals(&tick),
            "CONFIG-CARRIER tick identity"
        );
        assert!(
            seen.borrow()
                .last()
                .unwrap()
                .get("perfFixture")
                .unwrap()
                .strict_equals(&perf),
            "CONFIG-CARRIER perf argument identity"
        );
        drop(inputs);
        configs += 1;
        module
            .perf
            .configure(Js::object(vec![("mode", Js::str("off"))]))
            .unwrap();
        module
            .perf
            .configure(Js::object(vec![("mode", Js::str("record"))]))
            .unwrap();
        host.state.now.set(number);
        module.perf.record(&Js::Null).unwrap();
        let frames = module.perf.take().unwrap().get("frames").unwrap();
        frames.set("0", frame.clone()).unwrap();
        finish(
            &host,
            listener(Js::object(vec![("op", Js::str("__perfFixtureDump"))])),
        )
        .unwrap();
        let posts = host.state.native_posts.borrow();
        assert_eq!(posts.len(), index + 1);
        let actual = posts[index].get("data").unwrap();
        assert!(
            actual.get("frames").unwrap().strict_equals(&frames),
            "CONFIG-CARRIER returned frames identity"
        );
        let actual_frame = actual.get("frames").unwrap().get("0").unwrap();
        assert!(actual_frame.strict_equals(&frame));
        let actual_msg = actual_frame.get("msg").unwrap();
        assert!(actual_msg.strict_equals(&msg));
        assert!(actual_msg.has_own("own"));
        assert!(actual_msg.get("own").unwrap().is_undefined());
        assert_eq!(
            actual_msg.get("text").unwrap().as_str().unwrap().as_ref(),
            &[0xd800, 88, 0xdc00]
        );
        for value in [
            tick.get("bybit").unwrap(),
            perf.get("intervalMs").unwrap(),
            actual.get("recordedAtMs").unwrap(),
            actual.get("durationMs").unwrap(),
            actual_frame.get("atMs").unwrap(),
            actual_msg.get("value").unwrap(),
        ] {
            assert!(
                matches!(value, Js::Number(n) if n.to_bits() == expected_bits),
                "CONFIG-CARRIER native bits {token}"
            );
            bit_checks += 1;
        }
        dumps += 1;
    }
    for value in [
        Js::Null,
        Js::Bool(true),
        Js::str("untyped"),
        Js::array(vec![]),
        Js::object(vec![
            ("mode", Js::str("off")),
            ("fixture", Js::Null),
            ("intervalMs", Js::Null),
            ("loop", Js::Null),
        ]),
    ] {
        let before = host.state.knob_inputs.borrow().len();
        finish(
            &host,
            listener(Js::object(vec![
                ("op", Js::str("__workerConfig")),
                ("data", Js::object(vec![("perfFixture", value.clone())])),
            ])),
        )
        .unwrap();
        let inputs = host.state.knob_inputs.borrow();
        assert_eq!(inputs.len(), before);
        assert!(
            seen.borrow()
                .last()
                .unwrap()
                .get("perfFixture")
                .unwrap()
                .strict_equals(&value),
            "CONFIG-CARRIER native non-record forwarding"
        );
        configs += 1;
    }
    assert_eq!(bit_checks, 60);
    assert_eq!(configs, 15);
    assert_eq!(dumps, 10);
    assert_eq!(source["configs"], json!(configs));
    assert_eq!(source["dumps"], json!(dumps));
}

#[test]
fn native_perf_record_dump_replay_matches_maintained_listener_roundtrip() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_bootstrap_oracle.mjs"
        ))
        .arg("--perf-service")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PERF source roundtrip: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let host = Host::new();
    host.state.next.set(1);
    let module = WorkerModule::new(host.clone());
    let session = module.start(host.clone(), false);
    let listener = host.state.listener.borrow().clone().unwrap();
    finish(
        &host,
        listener(Js::object(vec![
            ("op", Js::str("__workerConfig")),
            (
                "data",
                Js::object(vec![(
                    "perfFixture",
                    Js::object(vec![("mode", Js::str("record"))]),
                )]),
            ),
        ])),
    )
    .unwrap();
    finish(&host, host.send(init_payload())).unwrap();
    let shared = Js::object(vec![("v", Js::Number(f64::from_bits(0xfff8000000000042)))]);
    let input = Js::object(vec![
        ("n", Js::Number(17.)),
        ("shared", shared.clone()),
        ("again", shared),
        ("own", Js::Undefined),
        ("text", Js::utf16(&[0xd800, 88, 0xdc00])),
        (
            "nums",
            Js::array(vec![
                Js::Number(f64::INFINITY),
                Js::Number(f64::NEG_INFINITY),
                Js::Number(-0.),
            ]),
        ),
    ]);
    input.set("self", input.clone()).unwrap();
    session
        .get_worker()
        .unwrap()
        .exchange()
        .unwrap()
        .base()
        .emit("fill", input.clone())
        .unwrap();
    input.set("n", Js::Number(42.)).unwrap();
    finish(
        &host,
        listener(Js::object(vec![("op", Js::str("__perfFixtureDump"))])),
    )
    .unwrap();
    let fixture = host
        .state
        .native_posts
        .borrow()
        .iter()
        .find(|p| {
            p.get("op")
                .unwrap()
                .strict_equals(&Js::str("__perfFixtureDump"))
        })
        .unwrap()
        .get("data")
        .unwrap();
    let frames = fixture.get("frames").unwrap();
    let count = frames.get("length").unwrap().to_number().unwrap() as usize;
    assert_eq!(
        count,
        source["frames"].as_u64().unwrap() as usize,
        "PERF listener must record actual outbound frames"
    );
    let ops: Vec<_> = (0..count)
        .map(|i| {
            frames
                .get(&i.to_string())
                .unwrap()
                .get("msg")
                .unwrap()
                .get("op")
                .unwrap()
                .to_display()
        })
        .collect();
    assert_eq!(json!(ops), source["ops"]);
    let fill = (0..count)
        .map(|i| frames.get(&i.to_string()).unwrap().get("msg").unwrap())
        .find(|p| p.get("op").unwrap().strict_equals(&Js::str("fill")))
        .unwrap();
    assert!(
        matches!(fill.get("data").unwrap().get("n").unwrap(), Js::Number(n) if n.to_bits()==17_f64.to_bits()),
        "PERF captured payload cannot follow later Store/event mutation"
    );
    let data = fill.get("data").unwrap();
    assert!(data.get("self").unwrap().strict_equals(&data));
    assert!(
        data.get("shared")
            .unwrap()
            .strict_equals(&data.get("again").unwrap())
    );
    assert!(
        data.get("shared")
            .unwrap()
            .get("v")
            .unwrap()
            .as_f64()
            .unwrap()
            .is_nan()
    );
    assert_eq!(
        data.get("shared")
            .unwrap()
            .get("v")
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        f64::NAN.to_bits(),
        "PERF native listener matches Node clone NaN payload canonicalization"
    );
    assert!(data.has_own("own"));
    assert!(data.get("own").unwrap().is_undefined());
    assert_eq!(
        data.get("text").unwrap().as_str().unwrap().as_ref(),
        &[0xd800, 88, 0xdc00]
    );
    for (i, expected) in [f64::INFINITY, f64::NEG_INFINITY, -0.].iter().enumerate() {
        assert_eq!(
            data.get("nums")
                .unwrap()
                .get(&i.to_string())
                .unwrap()
                .as_f64()
                .unwrap()
                .to_bits(),
            expected.to_bits()
        );
    }
    assert_eq!(
        fixture.get("recordedAtMs").unwrap().as_f64(),
        source["recordedAtMs"].as_f64()
    );
    assert_eq!(
        fixture.get("durationMs").unwrap().as_f64(),
        source["durationMs"].as_f64()
    );
    input
        .delete(&safe_cex_exchange::js_runtime::value::utf16("self"))
        .unwrap();
    drop(module);
    let replay = Host::new();
    replay.state.next.set(1);
    let replay_module = WorkerModule::new(replay.clone());
    let _session = replay_module.start(replay.clone(), false);
    let receive = replay.state.listener.borrow().clone().unwrap();
    finish(
        &replay,
        receive(Js::object(vec![
            ("op", Js::str("__workerConfig")),
            (
                "data",
                Js::object(vec![(
                    "perfFixture",
                    Js::object(vec![
                        ("mode", Js::str("replay")),
                        ("fixture", fixture),
                        ("intervalMs", Js::Number(7.)),
                        ("loop", Js::Bool(false)),
                    ]),
                )]),
            ),
        ])),
    )
    .unwrap();
    finish(
        &replay,
        receive(Js::object(vec![
            ("op", Js::str("init")),
            ("data", Js::object(vec![])),
        ])),
    )
    .unwrap();
    assert!(replay_module.exchange_workers.borrow().is_empty());
    let base = replay.state.native_posts.borrow().len();
    let mut delays = vec![];
    for index in 0..count {
        let timers = replay.state.timers.borrow();
        assert_eq!(timers.len(), 1, "PERF only one owned metronome timer");
        let (&id, (delay, _)) = timers.iter().next().unwrap();
        delays.push(*delay);
        drop(timers);
        replay.fire(id, 500. + index as f64 * 7.);
        let posts = replay.state.native_posts.borrow();
        assert_eq!(posts.len(), base + index + 1);
        assert!(
            posts[base + index]
                .strict_equals(&frames.get(&index.to_string()).unwrap().get("msg").unwrap()),
            "PERF replay retains the captured frame reference"
        );
    }
    assert!(replay.state.timers.borrow().is_empty());
    assert_eq!(
        delays,
        source["delays"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_f64().unwrap())
            .collect::<Vec<_>>()
    );
    assert!(!replay.state.events.borrow().contains(&"load".to_string()));
    data.delete(&safe_cex_exchange::js_runtime::value::utf16("self"))
        .unwrap();
}

#[test]
fn native_perf_cap_loop_and_module_disposal_match_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_bootstrap_oracle.mjs"
        ))
        .arg("--perf-rules")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let host = Host::new();
    host.state.next.set(1);
    let module = WorkerModule::new(host.clone());
    let session = module.start(host.clone(), false);
    let receive = host.state.listener.borrow().clone().unwrap();
    let configure = |cfg| {
        finish(
            &host,
            receive(Js::object(vec![
                ("op", Js::str("__workerConfig")),
                ("data", Js::object(vec![("perfFixture", cfg)])),
            ])),
        )
        .unwrap()
    };
    configure(Js::object(vec![("mode", Js::str("record"))]));
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    let bad = Js::object(vec![]);
    bad.define_accessor(
        "bad",
        Some(Rc::new(move |_| {
            count.set(count.get() + 1);
            Err(safe_cex_exchange::js_runtime::value::Thrown::error(
                "Error",
                "uncloneable getter",
            ))
        })),
        None,
    )
    .unwrap();
    module.perf.record(&bad).unwrap();
    let count = reads.clone();
    let good = Js::object(vec![]);
    good.define_accessor(
        "n",
        Some(Rc::new(move |_| {
            count.set(count.get() + 1);
            Ok(Js::Number(17.))
        })),
        None,
    )
    .unwrap();
    module.perf.record(&good).unwrap();
    let first = module.perf.take().unwrap();
    let frames = first.get("frames").unwrap();
    assert_eq!(frames.get("length").unwrap().as_f64(), Some(1.));
    assert_eq!(
        frames
            .get("0")
            .unwrap()
            .get("msg")
            .unwrap()
            .get("n")
            .unwrap()
            .as_f64(),
        Some(17.)
    );
    assert_eq!(reads.get(), source["reads"].as_u64().unwrap() as u32);
    assert!(frames.strict_equals(&module.perf.take().unwrap().get("frames").unwrap()));
    assert_eq!(source["alias"], true);
    frames.set("length", Js::Number(19_999.)).unwrap();
    frames
        .set(
            "19998",
            Js::object(vec![("atMs", Js::Number(0.)), ("msg", Js::Null)]),
        )
        .unwrap();
    module.perf.record(&Js::Null).unwrap();
    assert_eq!(frames.get("length").unwrap().as_f64(), Some(20_000.));
    module.perf.record(&bad).unwrap();
    assert_eq!(reads.get(), 2);
    assert_eq!(
        module
            .perf
            .take()
            .unwrap()
            .get("frames")
            .unwrap()
            .get("length")
            .unwrap()
            .as_f64(),
        source["cap"].as_f64()
    );
    configure(Js::object(vec![("mode", Js::str("off"))]));
    configure(Js::object(vec![("mode", Js::str("record"))]));
    module.perf.record(&Js::Null).unwrap();
    assert_eq!(
        module
            .perf
            .take()
            .unwrap()
            .get("frames")
            .unwrap()
            .get("length")
            .unwrap()
            .as_f64(),
        source["reset"].as_f64()
    );
    assert_eq!(frames.get("length").unwrap().as_f64(), Some(20_000.));
    let fixture = Js::object(vec![(
        "frames",
        Js::array(vec![
            Js::object(vec![
                ("atMs", Js::Number(900.)),
                ("msg", Js::object(vec![("id", Js::Number(1.))])),
            ]),
            Js::object(vec![
                ("atMs", Js::Number(-900.)),
                ("msg", Js::object(vec![("id", Js::Number(2.))])),
            ]),
        ]),
    )]);
    configure(Js::object(vec![
        ("mode", Js::str("replay")),
        ("fixture", fixture),
        ("intervalMs", Js::Number(0.)),
        ("loop", Js::Bool(true)),
    ]));
    finish(
        &host,
        receive(Js::object(vec![
            ("op", Js::str("init")),
            ("data", Js::object(vec![])),
        ])),
    )
    .unwrap();
    let mut ids = vec![];
    let mut delays = vec![];
    for i in 0..3 {
        let (&id, (delay, _)) = host
            .state
            .timers
            .borrow()
            .iter()
            .next()
            .map(|(id, p)| (id, p.clone()))
            .unwrap();
        delays.push(delay);
        host.fire(id, 100. + i as f64);
        ids.push(
            host.state
                .native_posts
                .borrow()
                .last()
                .unwrap()
                .get("id")
                .unwrap()
                .as_f64()
                .unwrap(),
        );
    }
    assert_eq!(
        ids,
        source["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        delays,
        source["delays"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect::<Vec<_>>()
    );
    let service = module.perf.clone();
    drop(module);
    assert!(
        host.state.timers.borrow().is_empty(),
        "PERF module disposal releases metronome despite retained session/service"
    );
    assert!(host.state.listener.borrow().is_none());
    assert!(!service.is_replaying());
    assert!(service.configure(Js::Null).is_err());
    assert!(session.get_worker().is_none());
}

#[test]
fn worker_logging_retains_native_extra_graphs_at_actual_post() {
    let (h, w) = fixture(false);
    let shared = Js::object(vec![("value", Js::Number(f64::INFINITY))]);
    shared.define("self", shared.clone()).unwrap();
    let extra = Js::object(vec![
        ("first", shared.clone()),
        ("second", shared.clone()),
        ("missing", Js::Undefined),
        ("zero", Js::Number(-0.0)),
    ]);
    extra.define_key(&[0xd800], Js::utf16(&[0xdc00])).unwrap();
    let mut deep = Js::Number(17.0);
    for _ in 0..150 {
        deep = Js::object(vec![("unread", deep)]);
    }
    extra.define("deep", deep.clone()).unwrap();
    w.report_error(
        "native-extra",
        &ExchangeError::new("source-message"),
        Some(&extra),
        true,
    )
    .unwrap();
    let post = h.state.native_posts.borrow().last().unwrap().clone();
    let sent = post
        .get("data")
        .unwrap()
        .get("1")
        .unwrap()
        .get("sentryData")
        .unwrap()
        .get("extra")
        .unwrap();
    assert!(sent.get("first").unwrap().same_reference(&shared));
    assert!(sent.get("second").unwrap().same_reference(&shared));
    assert!(shared.get("self").unwrap().same_reference(&shared));
    assert!(sent.has_own("missing"));
    assert!(sent.get("missing").unwrap().is_undefined());
    assert_eq!(
        sent.get("zero").unwrap().to_number().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        sent.get("first")
            .unwrap()
            .get("value")
            .unwrap()
            .to_number()
            .unwrap()
            .to_bits(),
        f64::INFINITY.to_bits()
    );
    assert_eq!(
        sent.get_key(&[0xd800])
            .unwrap()
            .to_js_string()
            .unwrap()
            .as_ref(),
        &[0xdc00]
    );
    assert!(sent.get("deep").unwrap().same_reference(&deep));
    w.log_message(
        safe_cex_webworker::worker_logging::WorkerLogLevel::Info,
        "native-context",
        Some(&extra),
    )
    .unwrap();
    let context = h
        .state
        .native_posts
        .borrow()
        .last()
        .unwrap()
        .get("data")
        .unwrap()
        .get("context")
        .unwrap();
    assert!(context.get("first").unwrap().same_reference(&shared));
    assert!(context.has_own("missing"));
    assert!(context.get("missing").unwrap().is_undefined());
    assert_eq!(
        context
            .get_key(&[0xd800])
            .unwrap()
            .to_js_string()
            .unwrap()
            .as_ref(),
        &[0xdc00]
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_error_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_str(
        &String::from_utf8(output.stdout)
            .unwrap()
            .replace("\\ud800", "SURROGATE_WITNESS"),
    )
    .unwrap();
    let native = json!({"first":sent.get("first").unwrap().same_reference(&shared),"second":sent.get("second").unwrap().same_reference(&shared),"self":shared.get("self").unwrap().same_reference(&shared),"missing":sent.has_own("missing")&&sent.get("missing").unwrap().is_undefined(),"zero":format!("{:016x}",sent.get("zero").unwrap().to_number().unwrap().to_bits()),"infinity":format!("{:016x}",sent.get("first").unwrap().get("value").unwrap().to_number().unwrap().to_bits()),"text":sent.get_key(&[0xd800]).unwrap().to_js_string().unwrap()[0],"deep":sent.get("deep").unwrap().same_reference(&deep)});
    assert_eq!(
        native, source["nativeGraph"],
        "WORKER-LOG actual worker/source native graph post"
    );
    for _ in 0..1000 {
        w.handle_payload(Js::object(vec![("op", Js::str("buffered"))]))
            .unwrap();
    }
    let raw_ops = [
        Js::Undefined,
        Js::utf16(&[0xd800]),
        Js::Number(f64::INFINITY),
        Js::Number(-0.0),
        shared.clone(),
    ];
    for op in raw_ops {
        w.handle_payload(Js::object(vec![("op", op.clone())]))
            .unwrap();
        let posted = h
            .state
            .native_posts
            .borrow()
            .last()
            .unwrap()
            .get("data")
            .unwrap()
            .get("1")
            .unwrap()
            .get("sentryData")
            .unwrap()
            .get("extra")
            .unwrap();
        assert!(
            posted.has_own("droppedPayload"),
            "WORKER-LOG overflow always owns original op"
        );
        let actual = posted.get("droppedPayload").unwrap();
        if let Js::Number(n) = &op {
            assert_eq!(actual.to_number().unwrap().to_bits(), n.to_bits());
        } else {
            assert!(
                actual.strict_equals(&op),
                "WORKER-LOG overflow retained original op"
            );
        }
    }
    // Break the fixture cycle after native identity observations.
    shared.set("self", Js::Null).unwrap();
}
