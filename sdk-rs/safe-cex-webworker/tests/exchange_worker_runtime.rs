#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_exchange::store_delivery::StoreFrame;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_types::ExchangeName;
use safe_cex_webworker::chaser::ChaserOrderInstance;
use safe_cex_webworker::chaser_manager::ChaserOrderManager;
use safe_cex_webworker::chaser_manager::ChaserSupervisor;
use safe_cex_webworker::exchange_worker::*;
use safe_cex_webworker::payload::StoreWorkerOp;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
struct UnusedSupervisor;
impl ChaserSupervisor for UnusedSupervisor {
    fn construct_supervision(
        &self,
        _: Rc<ChaserOrderManager>,
        _: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        panic!("unexpected heavy supervision")
    }
    fn construct_cleanup(
        &self,
        _: Rc<ChaserOrderManager>,
        _: Rc<ChaserOrderInstance>,
    ) -> ExchangeFuture<'static, ()> {
        panic!("unexpected heavy cleanup")
    }
    fn construct_completion(&self, _: Rc<ChaserOrderInstance>) -> ExchangeFuture<'static, ()> {
        panic!("unexpected heavy completion")
    }
    fn spawn(&self, _: ExchangeFuture<'static, ()>) {
        panic!("unexpected heavy task")
    }
}
struct Host {
    now: Cell<f64>,
    next: Cell<u64>,
    timers: RefCell<BTreeMap<u64, (f64, Rc<dyn Fn()>)>>,
    events: RefCell<Vec<String>>,
    posts: RefCell<Vec<WorkerStoreMessage>>,
    fail: Cell<bool>,
    assert_timer_before_post: Cell<bool>,
    manager: Rc<ChaserOrderManager>,
}
impl Host {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            now: Cell::new(0.),
            next: Cell::new(0),
            timers: RefCell::default(),
            events: RefCell::default(),
            posts: RefCell::default(),
            fail: Cell::new(false),
            assert_timer_before_post: Cell::new(false),
            manager: ChaserOrderManager::new(),
        })
    }
    fn fire(&self, id: u64, time: f64) {
        self.now.set(time);
        let cb = self.timers.borrow_mut().remove(&id).unwrap().1;
        cb();
    }
}
impl WorkerPortTimers for Host {
    type Timer = u64;
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn set_timeout(&self, cb: Rc<dyn Fn()>, delay: f64) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.timers.borrow_mut().insert(id, (delay, cb));
        id
    }
    fn clear_timeout(&self, id: &u64) {
        self.timers.borrow_mut().remove(id);
    }
    fn timer_is_truthy(&self, id: &u64) -> bool {
        *id != 0
    }
}
impl safe_cex_exchange::store_delivery::StoreUpdateThrottleHost for Host {
    fn report_store_timer_error(&self, e: &ExchangeError) {
        self.events.borrow_mut().push(e.message.clone());
    }
}
impl ExchangeWorkerHost for Host {
    fn post_store(&self, m: WorkerStoreMessage) -> Result<(), ExchangeError> {
        if self.assert_timer_before_post.replace(false) {
            assert!(
                self.timers.borrow().contains_key(&0),
                "WORKER native timer armed inside leading callback"
            );
        }
        self.events.borrow_mut().push("post".into());
        self.posts.borrow_mut().push(m);
        if self.fail.get() {
            Err(ExchangeError::new("post failed"))
        } else {
            Ok(())
        }
    }
    fn chaser_manager(&self) -> Rc<ChaserOrderManager> {
        self.manager.clone()
    }
    fn chaser_supervisor(&self) -> Rc<dyn ChaserSupervisor> {
        Rc::new(UnusedSupervisor)
    }
}
fn fixture(mult: Option<f64>) -> (Rc<Host>, Rc<ExchangeWorker<Host>>) {
    let h = Host::new();
    let mut options: safe_cex_types::ExchangeOptions =
        serde_json::from_value(json!({"key":"","secret":"","tuning":{}})).unwrap();
    options
        .tuning
        .as_mut()
        .unwrap()
        .data_update_throttle_multiplier = mult;
    let construction = WorkerConstruction {
        ctx: h.clone(),
        account_name: "a".into(),
        exchange_name: ExchangeName::Bybit,
        options: Rc::new(RefCell::new(options)),
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
    };
    let options = construction.options.clone();
    let settings = construction.settings.clone();
    let metadata = construction.metadata.clone();
    let w = ExchangeWorker::new(construction);
    assert!(
        Rc::ptr_eq(&options, &w.construction.options),
        "WORKER native original options identity"
    );
    assert!(
        Rc::ptr_eq(&settings, &w.construction.settings),
        "WORKER native original settings identity"
    );
    assert!(
        Rc::ptr_eq(&metadata, &w.construction.metadata),
        "WORKER native original metadata identity"
    );
    (h, w)
}
fn frame(w: &ExchangeWorker<Host>, latency: f64) -> StoreFrame {
    let mut d = w.store().serialize();
    d.latency = latency;
    Rc::new(RefCell::new(d))
}
fn source() -> Value {
    let o = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/exchange_worker_runtime_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "WORKER source oracle {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
struct Processor(Rc<dyn Fn(StoreFrame) -> Result<(), ExchangeError>>);
impl WorkerPostprocessor for Processor {
    fn process(&self, d: StoreFrame) -> Result<(), ExchangeError> {
        (self.0)(d)
    }
}
struct Handler(Rc<dyn Fn() -> Result<(), ExchangeError>>);
impl WorkerHandler for Handler {
    fn dispose(&self) -> Result<(), ExchangeError> {
        (self.0)()
    }
}
fn handler(f: impl Fn() -> Result<(), ExchangeError> + 'static) -> Rc<dyn WorkerHandler> {
    Rc::new(Handler(Rc::new(f)))
}
struct Venue {
    base: BaseExchange,
    caps: ExchangeCapabilities,
    host: Rc<Host>,
    label: &'static str,
    fail: Cell<bool>,
}
impl Exchange for Venue {
    type PlaceOpts = safe_cex_types::PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.caps
    }
    fn dispose(&self) -> Result<(), ExchangeError> {
        self.host.events.borrow_mut().push(self.label.into());
        if self.fail.get() {
            Err(ExchangeError::new("venue failed"))
        } else {
            self.base.dispose()?;
            Ok(())
        }
    }
}
fn venue(w: &ExchangeWorker<Host>, label: &'static str) -> Rc<Venue> {
    let runtime = Rc::new(runtime::Runtime::default());
    runtime.advance(0);
    Rc::new(Venue {
        base: BaseExchange::new(
            ExchangeName::Bybit,
            w.construction.options.borrow().clone(),
            w.store(),
            runtime,
            Default::default(),
        ),
        caps: Default::default(),
        host: w.construction.ctx.clone(),
        label,
        fail: Cell::new(false),
    })
}
#[test]
fn construction_retains_handles_and_binary64_throttle_configuration() {
    let (h, w) = fixture(None);
    assert!(
        Rc::ptr_eq(&h, &w.construction.ctx),
        "WORKER native ctx identity"
    );
    assert!(
        Rc::ptr_eq(&w.store(), &w.store()),
        "WORKER native store identity"
    );
    assert_eq!(w.construction.account_name, "a");
    assert_eq!(w.construction.exchange_name, ExchangeName::Bybit);
    assert_eq!(w.construction.session_key, "s");
    assert_eq!(w.construction.instance_id.as_deref(), Some("i"));
    assert!(w.exchange().is_none());
    assert!(w.cancel_startup.borrow().is_none());
    assert_eq!(
        w.startup_token.get().to_bits(),
        0f64.to_bits(),
        "WORKER native initial token"
    );
    assert_eq!(
        w.throttled_store_update.wait_ms, 250.,
        "WORKER native default wait"
    );
    assert!(w.message_buffer.borrow().is_empty());
    assert!(w.handlers.borrow().borrow().is_empty());
    assert!(w.postprocessors.borrow().borrow().is_empty());
    assert!(
        w.exchange_dependent_postprocessors
            .borrow()
            .borrow()
            .is_empty()
    );
    assert!(w.sentry_report_cooldowns.borrow().is_empty());
    assert_eq!(
        w.handler_kinds,
        [
            WorkerHandlerKind::OHLCV,
            WorkerHandlerKind::OrderBook,
            WorkerHandlerKind::Chaser,
            WorkerHandlerKind::Twap,
            WorkerHandlerKind::Dev,
            WorkerHandlerKind::Command,
            WorkerHandlerKind::UpdatePositions,
            WorkerHandlerKind::CallMethod,
            WorkerHandlerKind::UiState
        ]
    );
    let ex: Rc<dyn Exchange> = venue(&w, "venue");
    *w.exchange_handle.borrow_mut() = Some(ex.clone());
    assert!(
        Rc::ptr_eq(&ex, &w.exchange().unwrap()),
        "WORKER native exchange identity"
    );
    let (_, other) = fixture(None);
    assert!(!Rc::ptr_eq(&w.store(), &other.store()));
    let inputs = [
        None,
        Some(0.),
        Some(-0.),
        Some(f64::NAN),
        Some(2.),
        Some(-2.),
        Some(f64::INFINITY),
    ];
    let expected = [250., 250., 250., 250., 500., -500., f64::INFINITY];
    for (m, e) in inputs.into_iter().zip(expected) {
        let (_, v) = fixture(m);
        assert_eq!(
            v.throttled_store_update.wait_ms.to_bits(),
            e.to_bits(),
            "WORKER native multiplier truthiness"
        );
    }
    let cancel: StartupCancellation = Rc::new(|| Ok(()));
    *w.cancel_startup.borrow_mut() = Some(cancel.clone());
    assert!(
        Rc::ptr_eq(&cancel, w.cancel_startup.borrow().as_ref().unwrap()),
        "WORKER native cancel identity"
    );
    let opts = w.construction.options.clone();
    opts.borrow_mut().hosted_mode = Some(true);
    assert_eq!(w.construction.options.borrow().hosted_mode, Some(true));
    assert_eq!(
        source()["construction"],
        json!({"token":0,"exchange":null,"waits":[250,250,250,250,500,-500,"Infinity"],"handlers":9,"sameStore":true,"freshStore":true})
    );
}
#[test]
fn throttle_leading_trailing_maxwait_rollback_and_cancel_zero() {
    let (h, w) = fixture(None);
    h.assert_timer_before_post.set(true);
    let a = frame(&w, 1.);
    w.throttled_store_update.call(a).unwrap();
    assert_eq!(h.posts.borrow().len(), 1, "WORKER native leading delivery");
    assert!(
        h.timers.borrow().contains_key(&0),
        "WORKER native timer before leading delivery"
    );
    h.now.set(100.);
    let last = frame(&w, 2.);
    w.throttled_store_update.call(last.clone()).unwrap();
    last.borrow_mut().latency = 3.;
    h.fire(0, 200.);
    assert_eq!(
        h.posts.borrow().len(),
        1,
        "WORKER native early timer rescheduled"
    );
    assert_eq!(h.timers.borrow()[&1].0, 50.);
    h.fire(1, 250.);
    assert_eq!(
        h.posts.borrow()[1].data.latency,
        Some(3.),
        "WORKER native trailing latest object identity"
    );
    h.now.set(300.);
    w.throttled_store_update.call(frame(&w, 4.)).unwrap();
    h.now.set(500.);
    w.throttled_store_update.call(frame(&w, 5.)).unwrap();
    assert_eq!(
        h.posts.borrow()[2].data.latency,
        Some(5.),
        "WORKER native maxwait synchronous delivery"
    );
    h.now.set(400.);
    w.throttled_store_update.call(frame(&w, 6.)).unwrap();
    assert_eq!(
        h.posts.borrow()[3].data.latency,
        Some(6.),
        "WORKER native clock rollback delivery"
    );
    w.throttled_store_update.cancel();
    assert!(h.timers.borrow().is_empty());
    let (h0, w0) = fixture(None);
    w0.throttled_store_update.call(frame(&w0, 7.)).unwrap();
    w0.throttled_store_update.cancel();
    assert!(
        h0.timers.borrow().is_empty(),
        "WORKER native cancellation of zero timer"
    );
    h0.now.set(1.);
    w0.throttled_store_update.call(frame(&w0, 8.)).unwrap();
    assert_eq!(
        h0.posts.borrow().len(),
        2,
        "WORKER native cancel resets leading edge"
    );
    h.fail.set(true);
    h.now.set(800.);
    assert!(w.throttled_store_update.call(frame(&w, 9.)).is_err());
    assert_eq!(h.posts.borrow().len(), 5);
    assert_eq!(
        h.timers.borrow().len(),
        1,
        "WORKER native thrown leading retains timer"
    );
    h.fail.set(false);
    h.now.set(801.);
    w.throttled_store_update.call(frame(&w, 10.)).unwrap();
    let id = *h.timers.borrow().keys().next().unwrap();
    h.fire(id, 1050.);
    assert_eq!(h.posts.borrow()[5].data.latency, Some(10.));
    let p: Vec<f64> = h
        .posts
        .borrow()
        .iter()
        .map(|m| m.data.latency.unwrap())
        .collect();
    assert_eq!(
        p,
        [1., 3., 5., 6., 9., 10.],
        "WORKER native throttle independent profile"
    );
    let (hr, wr) = fixture(None);
    let weak = Rc::downgrade(&wr);
    let time = hr.clone();
    wr.postprocessors
        .borrow()
        .borrow_mut()
        .push(Rc::new(Processor(Rc::new(move |data| {
            if data.borrow().latency == 1. {
                let worker = weak.upgrade().unwrap();
                assert!(
                    time.timers.borrow().contains_key(&0),
                    "WORKER native reentry sees armed leading timer"
                );
                time.now.set(100.);
                worker.throttled_store_update.call(frame(&worker, 2.))?;
                worker.throttled_store_update.cancel();
            }
            Ok(())
        }))));
    wr.throttled_store_update.call(frame(&wr, 1.)).unwrap();
    assert!(
        hr.timers.borrow().is_empty(),
        "WORKER native callback cancel clears queued reentry"
    );
    assert_eq!(hr.posts.borrow().len(), 1);
    hr.now.set(101.);
    wr.throttled_store_update.call(frame(&wr, 3.)).unwrap();
    let reentry: Vec<f64> = hr
        .posts
        .borrow()
        .iter()
        .map(|m| m.data.latency.unwrap())
        .collect();
    assert_eq!(
        reentry,
        [1., 3.],
        "WORKER native callback reentry cancel profile"
    );
    let source_reentry: Vec<f64> = source()["throttleReentry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert_eq!(source_reentry, reentry);
    let source_values: Vec<f64> = source()["throttle"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect();
    assert_eq!(source_values, p);
}
#[test]
fn store_pipeline_live_arrays_failure_cutovers_and_forced_snapshot() {
    let (h, w) = fixture(None);
    let d = frame(&w, -0.);
    let expected = d.clone();
    let hw = h.clone();
    let ww = Rc::downgrade(&w);
    w.exchange_dependent_postprocessors
        .borrow()
        .borrow_mut()
        .push(Rc::new(Processor(Rc::new(move |data| {
            assert!(
                Rc::ptr_eq(&data, &expected),
                "WORKER native dependent original frame"
            );
            hw.events.borrow_mut().push("dependent".into());
            data.borrow_mut().latency = 17.;
            let h = hw.clone();
            *ww.upgrade()
                .unwrap()
                .exchange_dependent_postprocessors
                .borrow_mut() = Rc::new(RefCell::new(vec![
                Rc::new(Processor(Rc::new(|_| panic!("old index must not replay")))),
                Rc::new(Processor(Rc::new(move |data| {
                    h.events.borrow_mut().push("replacement".into());
                    data.borrow_mut().latency = 23.;
                    Ok(())
                }))),
            ]));
            Ok(())
        }))));
    let hw = h.clone();
    let expected = d.clone();
    w.postprocessors
        .borrow()
        .borrow_mut()
        .push(Rc::new(Processor(Rc::new(move |data| {
            assert!(
                Rc::ptr_eq(&data, &expected),
                "WORKER native independent same frame"
            );
            assert_eq!(
                data.borrow().latency,
                23.,
                "WORKER native dependent before independent"
            );
            hw.events.borrow_mut().push("independent".into());
            data.borrow_mut().latency = f64::NEG_INFINITY;
            Ok(())
        }))));
    w.store_update(d, None).unwrap();
    assert_eq!(
        &*h.events.borrow(),
        &["dependent", "replacement", "independent", "post"],
        "WORKER native live processor order"
    );
    assert_eq!(h.posts.borrow()[0].op, StoreWorkerOp::Store);
    assert_eq!(
        h.posts.borrow()[0].data.latency.unwrap().to_bits(),
        f64::NEG_INFINITY.to_bits(),
        "WORKER native nonfinite payload"
    );
    assert!(h.posts.borrow()[0].data.markets.is_some());
    assert!(h.posts.borrow()[0].data.orders.is_some());
    *w.exchange_dependent_postprocessors.borrow_mut() = Rc::default();
    *w.postprocessors.borrow_mut() = Rc::default();
    h.fail.set(true);
    assert!(w.store_update(frame(&w, 4.), Some(true)).is_err());
    h.fail.set(false);
    w.store_update(frame(&w, 5.), None).unwrap();
    assert!(
        h.posts.borrow()[2].data.markets.is_none(),
        "WORKER native post error commits baseline"
    );
    assert!(h.posts.borrow()[2].data.orders.is_none());
    w.send_snapshot().unwrap();
    assert!(
        h.posts.borrow()[3].data.markets.is_some(),
        "WORKER native snapshot forces baseline"
    );
    assert!(h.posts.borrow()[3].data.orders.is_some());
    let before = h.posts.borrow().len();
    w.postprocessors
        .borrow()
        .borrow_mut()
        .push(Rc::new(Processor(Rc::new(|_| {
            Err(ExchangeError::new("processor failed"))
        }))));
    assert_eq!(
        w.store_update(frame(&w, 6.), None).unwrap_err().message,
        "processor failed"
    );
    assert_eq!(
        h.posts.borrow().len(),
        before,
        "WORKER native processor failure prevents post"
    );
    *w.postprocessors.borrow_mut() = Rc::default();
    let ex = venue(&w, "venue");
    ex.base.dispose().unwrap();
    *w.exchange_handle.borrow_mut() = Some(ex);
    w.throttled_store_update.call(frame(&w, 7.)).unwrap();
    let before = h.posts.borrow().len();
    w.send_snapshot().unwrap();
    assert!(
        h.timers.borrow().is_empty(),
        "WORKER native snapshot cancels before disposed return"
    );
    assert_eq!(h.posts.borrow().len(), before);
    assert_eq!(
        source()["pipeline"],
        json!({"events":["dependent","replacement","independent","post"],"latency":"-Infinity","postErrorOmitted":true,"snapshotFull":true,"processorFailureNoPost":true,"disposedNoPost":true})
    );
}
#[test]
fn dispose_token_cancel_exchange_and_foreach_retained_array() {
    let (h, w) = fixture(None);
    let ex = venue(&w, "venue");
    *w.exchange_handle.borrow_mut() = Some(ex.clone());
    let hw = h.clone();
    let ww = Rc::downgrade(&w);
    *w.cancel_startup.borrow_mut() = Some(Rc::new(move || {
        let w = ww.upgrade().unwrap();
        assert_eq!(
            w.startup_token.get(),
            1.,
            "WORKER native token before cancel"
        );
        hw.events.borrow_mut().push("cancel".into());
        *w.cancel_startup.borrow_mut() = Some(Rc::new(|| panic!("replacement must be cleared")));
        Ok(())
    }));
    let old = w.handlers.borrow().clone();
    let hw = h.clone();
    let ww = Rc::downgrade(&w);
    let old_hook = old.clone();
    old.borrow_mut().push(handler(move || {
        hw.events.borrow_mut().push("first".into());
        old_hook
            .borrow_mut()
            .push(handler(|| panic!("append beyond initial length")));
        *ww.upgrade().unwrap().handlers.borrow_mut() = Rc::new(RefCell::new(vec![handler(|| {
            panic!("replacement array must not iterate")
        })]));
        Ok(())
    }));
    let hw = h.clone();
    old.borrow_mut().push(handler(move || {
        hw.events.borrow_mut().push("second".into());
        Ok(())
    }));
    w.dispose().unwrap();
    assert_eq!(
        &*h.events.borrow(),
        &["cancel", "venue", "first", "second"],
        "WORKER native dispose order"
    );
    assert!(w.cancel_startup.borrow().is_none());
    assert!(ex.base.is_disposed());
    assert!(
        Rc::ptr_eq(&w.exchange().unwrap(), &(ex.clone() as Rc<dyn Exchange>)),
        "WORKER native disposed exchange retained"
    );
    assert_eq!(
        old.borrow().len(),
        3,
        "WORKER native old handler array retained"
    );
    assert!(
        !Rc::ptr_eq(&old, &w.handlers.borrow()),
        "WORKER native reset replaces handler array"
    );
    assert!(w.handlers.borrow().borrow().is_empty());
    assert!(w.postprocessors.borrow().borrow().is_empty());
    let (hf, wf) = fixture(None);
    let ef = venue(&wf, "venue");
    *wf.exchange_handle.borrow_mut() = Some(ef.clone());
    let cancel: StartupCancellation = Rc::new(|| Err(ExchangeError::new("cancel failed")));
    *wf.cancel_startup.borrow_mut() = Some(cancel.clone());
    assert_eq!(wf.dispose().unwrap_err().message, "cancel failed");
    assert_eq!(wf.startup_token.get(), 1.);
    assert!(
        Rc::ptr_eq(&cancel, wf.cancel_startup.borrow().as_ref().unwrap()),
        "WORKER native failed cancel retained"
    );
    assert!(!ef.base.is_disposed());
    assert!(hf.events.borrow().is_empty());
    wf.cancel_startup.borrow_mut().take();
    ef.fail.set(true);
    assert_eq!(wf.dispose().unwrap_err().message, "venue failed");
    assert!(!ef.base.is_disposed());
    ef.fail.set(false);
    let oldf = wf.handlers.borrow().clone();
    oldf.borrow_mut()
        .push(handler(|| Err(ExchangeError::new("handler failed"))));
    assert_eq!(
        wf.clear_runtime_pipeline().unwrap_err().message,
        "handler failed"
    );
    assert!(
        Rc::ptr_eq(&oldf, &wf.handlers.borrow()),
        "WORKER native handler failure prevents resets"
    );
    let (_, wr) = fixture(None);
    let first = venue(&wr, "first venue");
    let second = venue(&wr, "second venue");
    *wr.exchange_handle.borrow_mut() = Some(first.clone());
    let second_hook = second.clone();
    let weak = Rc::downgrade(&wr);
    *wr.cancel_startup.borrow_mut() = Some(Rc::new(move || {
        *weak.upgrade().unwrap().exchange_handle.borrow_mut() = Some(second_hook.clone());
        Ok(())
    }));
    wr.dispose().unwrap();
    assert!(!first.base.is_disposed());
    assert!(
        second.base.is_disposed(),
        "WORKER native exchange reread after cancel"
    );
    // A real native manager warning failure aborts exchange disposal and all resets.
    let (hm, wm) = fixture(None);
    let em = venue(&wm, "venue");
    let exchange: Rc<dyn Exchange> = em.clone();
    *wm.exchange_handle.borrow_mut() = Some(exchange.clone());
    let options = serde_json::from_value(json!({"symbol":"BTC","size":3,"side":"buy","maxAttempts":10,"delay":100,"reduceOnly":false})).unwrap();
    let instance = Rc::new(ChaserOrderInstance::new(
        exchange,
        Rc::new(RefCell::new(options)),
    ));
    hm.manager.instances().borrow_mut().push(instance.clone());
    em.base.on(
        "warning",
        Rc::new(|_, _| Err(ExchangeError::new("warning failed"))),
    );
    let handlers = wm.handlers.borrow().clone();
    handlers.borrow_mut().push(handler(|| {
        panic!("cleanup error must prevent handler disposal")
    }));
    assert_eq!(wm.dispose().unwrap_err().message, "warning failed");
    assert!(
        instance.is_stopped(),
        "WORKER native manager mark precedes warning error"
    );
    assert!(
        !em.base.is_disposed(),
        "WORKER native manager error prevents venue disposal"
    );
    assert!(
        Rc::ptr_eq(&handlers, &wm.handlers.borrow()),
        "WORKER native manager error prevents pipeline reset"
    );
    for token in [f64::NAN, f64::INFINITY, 9007199254740992.] {
        let (_, v) = fixture(None);
        v.startup_token.set(token);
        v.dispose().unwrap();
        let result = v.startup_token.get();
        if token.is_nan() {
            assert!(result.is_nan())
        } else {
            assert_eq!(
                result.to_bits(),
                token.to_bits(),
                "WORKER native binary64 token increment"
            )
        }
    }
    assert_eq!(
        source()["dispose"],
        json!({"events":["cancel","venue","first","second"],"oldLength":3,"retainedExchange":true,"cancelThrowRetained":true,"handlerThrowRetained":true,"replacementDisposed":true})
    );
    old.borrow_mut().clear();
}
