#[path = "support/error_trace.rs"]
mod error_trace;
#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use indexmap::IndexMap;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange},
    runtime::*,
};
use safe_cex_store::StoreUpdate;
use safe_cex_types::{ExchangeName, FetchedLeverage, PlaceOrderOpts};
use safe_cex_webworker::chaser::ChaserOrderInstance;
use safe_cex_webworker::command_handlers::*;
use safe_cex_webworker::payload::*;
use safe_cex_webworker::task_types::*;
use safe_cex_webworker::twap_instance::*;
use safe_cex_webworker::twap_manager::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
struct Clock {
    inner: runtime::Runtime,
    ids: Cell<u64>,
    logs: RefCell<Vec<(String, String)>>,
}
impl ExchangeRuntime for Clock {
    fn monotonic_now_ms(&self) -> f64 {
        self.inner.monotonic_now_ms()
    }
    fn now_ms(&self) -> f64 {
        self.inner.now_ms()
    }
    fn random(&self) -> f64 {
        0.25
    }
    fn unique_id(&self) -> String {
        let n = self.ids.get();
        self.ids.set(n + 1);
        format!("generated-{n}")
    }
    fn sleep(&self, n: u64) -> ExchangeFuture<'static, ()> {
        self.inner.sleep(n)
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.inner.spawn_local(f)
    }
    fn console_values(&self, l: &str, m: &[safe_cex_exchange::js_runtime::value::Js]) {
        let rendered = safe_cex_exchange::runtime::console_text(m);
        let m = rendered.as_str();
        self.logs.borrow_mut().push((l.into(), m.into()));
    }
    fn is_worker(&self) -> bool {
        true
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(
        &self,
        e: &ExchangeError,
        v: &safe_cex_types::retained_json::RetainedJsonInput,
    ) {
        self.inner.report_exception(e, v)
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker bridge")
    }
}
struct Venue {
    base: BaseExchange,
    caps: ExchangeCapabilities,
    released: Rc<Cell<bool>>,
    events: Rc<RefCell<Vec<String>>>,
}
impl Exchange for Venue {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.caps
    }
    fn fetch_leverage_for_symbol(
        &self,
        symbol: String,
    ) -> ExchangeFuture<'_, Option<FetchedLeverage>> {
        self.events.borrow_mut().push(format!("fetch:{symbol}"));
        Box::pin(std::future::poll_fn(move |_| {
            if self.released.get() {
                Poll::Ready(Ok(Some(FetchedLeverage {
                    buy_leverage: 11.0,
                    sell_leverage: 13.0,
                    is_fetching: None,
                    margin_type: None,
                    position_mode: None,
                })))
            } else {
                Poll::Pending
            }
        }))
    }
}
struct Host {
    clock: Rc<Clock>,
    exchange: Rc<dyn Exchange>,
    manager: Rc<TwapManager>,
    events: Rc<RefCell<Vec<String>>>,
    timers: RefCell<IndexMap<u64, (Rc<dyn Fn()>, f64)>>,
    next: Cell<u64>,
    spawned: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    chasers: RefCell<Vec<Rc<ChaserOrderInstance>>>,
    twaps: RefCell<IndexMap<String, Rc<TwapInstance<Self>>>>,
    posts: Rc<RefCell<Vec<StoreWorkerPayload>>>,
    released: Rc<Cell<bool>>,
    post_failure: Cell<bool>,
    replace_state: Cell<bool>,
    orders: RefCell<Vec<PlaceOrderOpts>>,
}
impl WorkerPortTimers for Host {
    type Timer = u64;
    fn now_ms(&self) -> f64 {
        self.clock.now_ms()
    }
    fn set_timeout(&self, f: Rc<dyn Fn()>, d: f64) -> u64 {
        let n = self.next.get();
        self.next.set(n + 1);
        self.timers.borrow_mut().insert(n, (f, d));
        n
    }
    fn clear_timeout(&self, n: &u64) {
        self.timers.borrow_mut().shift_remove(n);
    }
    fn timer_is_truthy(&self, n: &u64) -> bool {
        *n != 0
    }
}
impl TwapHost for Host {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.clock.clone()
    }
    fn manager(&self) -> Rc<TwapManager> {
        self.manager.clone()
    }
    fn post_state(&self, _: IndexMap<String, SerializedTwap>) -> Result<(), ExchangeError> {
        if self.post_failure.get() {
            Err(ExchangeError::new("post failed"))
        } else {
            Ok(())
        }
    }
    fn place_order(
        &self,
        e: Rc<dyn Exchange>,
        o: PlaceOrderOpts,
    ) -> Result<ExchangeFuture<'static, Vec<String>>, ExchangeError> {
        assert!(
            Rc::ptr_eq(&e, &self.exchange),
            "HANDLER native placement exchange"
        );
        self.events.borrow_mut().push("order".into());
        self.orders.borrow_mut().push(o);
        Ok(Box::pin(async { Ok(vec![]) }))
    }
    fn emit_log(
        &self,
        _: Rc<dyn Exchange>,
        _: String,
        _: &'static str,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }
    fn add_chaser(&self, c: Rc<ChaserOrderInstance>) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push("chaser:add".into());
        self.chasers.borrow_mut().push(c);
        Ok(())
    }
    fn find_chaser(&self, id: &str) -> Option<Rc<ChaserOrderInstance>> {
        self.chasers.borrow().iter().find(|c| c.id() == id).cloned()
    }
    fn remove_chaser(&self, id: &str) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(format!("chaser:remove:{id}"));
        self.chasers.borrow_mut().retain(|c| c.id() != id);
        Ok(())
    }
    fn same_timer(&self, a: &u64, b: &u64) -> bool {
        a == b
    }
    fn spawn(&self, f: ExchangeFuture<'static, ()>) {
        self.spawned.borrow_mut().push(f);
    }
}
impl CommandHandlerHost for Host {
    fn worker_error_post(&self) -> safe_cex_webworker::error_post::WorkerErrorPost {
        let posts = self.posts.clone();
        error_trace::post(Rc::new(move |payload| posts.borrow_mut().push(payload)))
    }
    fn post_payload(&self, p: StoreWorkerPayload) -> Result<(), ExchangeError> {
        self.posts.borrow_mut().push(p);
        Ok(())
    }
    fn cleanup_chaser_exchange(&self, e: Rc<dyn Exchange>, r: &str) -> Result<(), ExchangeError> {
        assert!(Rc::ptr_eq(&e, &self.exchange));
        self.events.borrow_mut().push(format!("cleanup:{r}"));
        Ok(())
    }
    fn add_twap(&self, w: Rc<TwapInstance<Self>>) -> Result<(), ExchangeError> {
        let id = w.id();
        self.events.borrow_mut().push(format!("add:{id}"));
        self.manager.add(w.managed.clone());
        if self.replace_state.get() {
            let mut s = w.state().borrow().clone();
            s.status = TwapStatus::Paused;
            w.managed.borrow_mut().state = Rc::new(RefCell::new(s));
        }
        self.twaps.borrow_mut().insert(id, w);
        Ok(())
    }
    fn get_twap(&self, id: &str) -> Option<Rc<TwapInstance<Self>>> {
        self.events.borrow_mut().push(format!("get:{id}"));
        self.twaps.borrow().get(id).cloned()
    }
}
impl Host {
    fn new() -> Rc<Self> {
        let clock = Rc::new(Clock {
            inner: Default::default(),
            ids: Cell::new(0),
            logs: RefCell::new(vec![]),
        });
        clock.inner.advance(1000);
        let events = Rc::new(RefCell::new(vec![]));
        let released = Rc::new(Cell::new(false));
        let exchange: Rc<dyn Exchange> = Rc::new(Venue {
            base: BaseExchange::new(
                ExchangeName::Bybit,
                serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
                Rc::new(DefaultStore::new()),
                clock.clone(),
                Default::default(),
            ),
            caps: Default::default(),
            released: released.clone(),
            events: events.clone(),
        });
        Rc::new(Self {
            clock,
            exchange,
            events,
            released,
            manager: Rc::default(),
            timers: RefCell::default(),
            next: Cell::new(1),
            spawned: RefCell::default(),
            chasers: RefCell::default(),
            twaps: RefCell::default(),
            posts: Rc::default(),
            post_failure: Cell::new(false),
            replace_state: Cell::new(false),
            orders: RefCell::default(),
        })
    }
    fn context(self: &Rc<Self>) -> Rc<HandlerContext<Self>> {
        Rc::new(HandlerContext{account_name:RefCell::new("account".into()),exchange_name:ExchangeName::Bybit,exchange:RefCell::new(self.exchange.clone()),ctx:self.clone(),settings:Rc::new(RefCell::new(serde_json::from_value(json!({"placeOrder":{"tpTriggerType":"mark_price","slTriggerType":"last_price"}})).unwrap())),metadata:Rc::new(RefCell::new(serde_json::from_value(json!({"supportedTPSLTriggerTypes":[],"privateProxyIps":[],"fetchLeverageOnMarginSymbolChange":true})).unwrap()))})
    }
    fn drain(&self) {
        for mut f in std::mem::take(&mut *self.spawned.borrow_mut()) {
            match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                Poll::Pending => self.spawned.borrow_mut().push(f),
                Poll::Ready(Err(e)) => self
                    .events
                    .borrow_mut()
                    .push(format!("rejected:{}", e.message)),
                Poll::Ready(Ok(())) => {}
            }
        }
    }
    fn release_cycles(&self) {
        self.twaps.borrow_mut().clear();
        self.manager.running_twaps.borrow_mut().clear();
        self.timers.borrow_mut().clear();
        self.spawned.borrow_mut().clear();
    }
}
fn payload(op: StoreWorkerOp, data: Value) -> StoreWorkerPayload {
    StoreWorkerPayload {
        op,
        instance_id: None,
        data: Some(data),
    }
}
fn market() -> Value {
    json!({"id":"BTC","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}})
}
fn options() -> Value {
    json!({"accountName":"account","activeMarket":market(),"size":3,"duration":1,"ordersCount":2,"side":"buy","chaserDelay":100,"reduceOnly":false,"randomnessPercentage":0,"useLimitOrders":false,"useChaserOrders":false})
}
fn state(status: &str) -> Value {
    json!({"accountName":"account","activeMarket":market(),"size":3,"ordersSizes":[1,2],"side":"buy","status":status,"ordersCount":2,"ordersExecuted":0,"nextOrderAt":2000})
}
fn restored(id: &str, status: &str) -> Value {
    json!({"id":id,"options":options(),"state":state(status)})
}
fn oracle() -> Value {
    let r = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/command_handlers_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "HANDLER source oracle: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    serde_json::from_slice(&r.stdout).unwrap()
}
#[test]
fn class_context_and_base_telemetry_are_real() {
    let h = Host::new();
    let c = h.context();
    let ch = ChaserOrderHandler::new(c.clone());
    let tw = TwapHandler::new(c.clone());
    let ui = UiStateHandler::new(c.clone());
    assert!(
        Rc::ptr_eq(&ch.context, &c),
        "CHASER CLASS native context identity"
    );
    assert!(
        Rc::ptr_eq(&tw.context, &c),
        "TWAP CLASS native context identity"
    );
    assert!(
        Rc::ptr_eq(&ui.context, &c),
        "UI CLASS native context identity"
    );
    assert!(Rc::ptr_eq(&ch.context.ctx, &h));
    assert_eq!(tw.context.exchange_name, ExchangeName::Bybit);
    assert!(Rc::ptr_eq(&ui.context.exchange(), &h.exchange));
    assert!(ch.buffer_until_markets_loaded(payload(StoreWorkerOp::ChaserTerminate, json!("x"))));
    assert!(tw.buffer_until_markets_loaded(payload(StoreWorkerOp::TwapPause, json!("x"))));
    assert!(ui.buffer_until_markets_loaded(payload(StoreWorkerOp::UiState, json!({}))));
    h.clock.inner.advance(31000);
    assert_eq!(
        h.posts.borrow().len(),
        3,
        "CLASS native independent timeout count"
    );
    let names = h
        .posts
        .borrow()
        .iter()
        .map(|p| {
            p.data.as_ref().unwrap()[1]["sentryData"]["tags"]["worker_handler"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        names,
        ["ChaserOrderHandler", "TwapHandler", "UiStateHandler"],
        "CLASS native telemetry names"
    );
    let replay = Host::new();
    let rc = replay.context();
    let rch = ChaserOrderHandler::new(rc.clone());
    let rtw = TwapHandler::new(rc.clone());
    let rui = UiStateHandler::new(rc);
    assert!(
        rch.buffer_until_markets_loaded(payload(StoreWorkerOp::ChaserTerminate, json!("buffered")))
    );
    assert!(rtw.buffer_until_markets_loaded(payload(
        StoreWorkerOp::TwapState,
        json!([restored("replay", "idle")])
    )));
    assert!(rui.buffer_until_markets_loaded(payload(
        StoreWorkerOp::UiState,
        json!({"activeSymbols":["BTC"]})
    )));
    replay
        .exchange
        .base()
        .store
        .update(
            StoreUpdate {
                markets: Some(vec![serde_json::from_value(market()).unwrap()]),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    replay.clock.inner.advance(1500);
    assert_eq!(
        &*replay.events.borrow(),
        &["chaser:remove:buffered", "add:replay"],
        "CLASS native Weak replay dispatch"
    );
    assert!(
        replay.manager.has_id("replay"),
        "CLASS native TWAP replay record"
    );
    assert_eq!(
        replay.exchange.base().get_active_ticker_symbols(),
        ["BTC"],
        "CLASS native UI replay symbols"
    );
    assert!(
        replay.posts.borrow().is_empty(),
        "CLASS native loaded replay cancels timeout"
    );
    let source = oracle();
    assert_eq!(source["classes"], json!(names));
    assert_eq!(
        source["replay"],
        json!({"chaser":true,"twap":true,"ui":true})
    );
    replay.release_cycles();
    ch.dispose();
    tw.dispose();
    ui.dispose();
}
#[test]
fn chaser_routes_and_account_fallback_precede_registration() {
    let h = Host::new();
    let c = h.context();
    let handler = ChaserOrderHandler::new(c.clone());
    let mut accounts = vec![];
    for (i, account) in [None, Some(""), Some(" "), Some("explicit")]
        .into_iter()
        .enumerate()
    {
        *c.account_name.borrow_mut() = format!("live-{i}");
        let mut o = json!({"symbol":"BTC","size":2,"side":"sell","maxAttempts":1000,"delay":70,"reduceOnly":true});
        if let Some(a) = account {
            o["accountName"] = json!(a);
        }
        handler
            .handle(&payload(StoreWorkerOp::ChaserStart, o))
            .unwrap();
        let ch = h.chasers.borrow()[i].clone();
        let opts = ch.options();
        let opts = opts.borrow();
        let expected = ["live-0", "live-1", " ", "explicit"][i];
        assert_eq!(
            opts.account_name.as_deref(),
            Some(expected),
            "CHASER native account fallback"
        );
        assert_eq!(
            ch.id(),
            format!("generated-{i}"),
            "CHASER native ctor before add"
        );
        assert_eq!(opts.symbol, "BTC");
        assert_eq!(opts.size, 2.0);
        assert_eq!(opts.side, safe_cex_types::OrderSide::Sell);
        assert_eq!(opts.delay, 70.0);
        assert_eq!(opts.reduce_only, Some(true));
        accounts.push(expected.to_owned());
    }
    handler
        .handle(&payload(
            StoreWorkerOp::ChaserTerminate,
            json!("generated-1"),
        ))
        .unwrap();
    handler
        .handle(&payload(StoreWorkerOp::ChaserTerminateAll, Value::Null))
        .unwrap();
    handler
        .handle(&payload(StoreWorkerOp::TwapPause, json!("unused")))
        .unwrap();
    assert_eq!(h.chasers.borrow().len(), 3, "CHASER native retained count");
    assert_eq!(
        &*h.events.borrow(),
        &[
            "chaser:add",
            "chaser:add",
            "chaser:add",
            "chaser:add",
            "chaser:remove:generated-1",
            "cleanup:Client disconnected while chase was active"
        ],
        "CHASER native routing"
    );
    let source = oracle();
    assert_eq!(source["chaser"]["accounts"], json!(accounts));
    assert_eq!(source["chaser"]["events"], json!(*h.events.borrow()));
}
#[test]
fn twap_restore_and_commands_pin_native_partial_effects() {
    let h = Host::new();
    let handler = TwapHandler::new(h.context());
    handler
        .handle(&payload(StoreWorkerOp::TwapStart, options()))
        .unwrap();
    assert_eq!(
        &*h.events.borrow(),
        &["add:generated-0", "order"],
        "TWAP native add before start"
    );
    let w = h.twaps.borrow()["generated-0"].clone();
    assert_eq!(w.state().borrow().status, TwapStatus::Running);
    assert_eq!(w.options().borrow().active_market.symbol, "BTC");
    assert_eq!(h.orders.borrow()[0].amount, 1.5);
    h.drain();
    handler
        .handle(&payload(StoreWorkerOp::TwapPause, json!("generated-0")))
        .unwrap();
    assert_eq!(
        w.state().borrow().status,
        TwapStatus::Paused,
        "TWAP native pause route"
    );
    handler
        .handle(&payload(StoreWorkerOp::TwapResume, json!("generated-0")))
        .unwrap();
    assert_eq!(
        w.state().borrow().status,
        TwapStatus::Running,
        "TWAP native resume route"
    );
    handler
        .handle(&payload(StoreWorkerOp::TwapCleanup, json!("generated-0")))
        .unwrap();
    assert_eq!(
        w.state().borrow().mark_for_cleanup,
        Some(true),
        "TWAP native cleanup route"
    );
    handler
        .handle(&payload(StoreWorkerOp::TwapStop, json!("generated-0")))
        .unwrap();
    assert_eq!(
        w.state().borrow().status,
        TwapStatus::Completed,
        "TWAP native stop route"
    );
    assert!(!h.manager.has_id("generated-0"));
    handler
        .handle(&payload(StoreWorkerOp::TwapPause, json!("missing")))
        .unwrap();
    // Intentionally stale fixture: get_twap retains an instance already removed by terminate.
    let spawned_before = h.spawned.borrow().len();
    handler
        .handle(&payload(StoreWorkerOp::TwapResume, json!("generated-0")))
        .unwrap();
    assert_eq!(
        h.spawned.borrow().len(),
        spawned_before,
        "TWAP native stale binding no future"
    );
    assert_eq!(
        w.state().borrow().status,
        TwapStatus::Completed,
        "TWAP native stale binding no transition"
    );
    // Known duplicate skips even malformed options. Missing state errors after constructor/add, then stops later rows.
    handler
        .handle(&payload(
            StoreWorkerOp::TwapState,
            json!([restored("dup", "idle")]),
        ))
        .unwrap();
    h.replace_state.set(false);
    h.post_failure.set(true);
    handler.handle(&payload(StoreWorkerOp::TwapState,json!([{"id":"dup","options":null,"state":null},restored("run","running"),{"id":"broken","options":options()},restored("after","idle")]))).unwrap();
    assert!(
        h.manager.has_id("broken"),
        "TWAP native add before missing-state error"
    );
    assert!(
        !h.manager.has_id("after"),
        "TWAP native catch stops later rows"
    );
    let run = h.twaps.borrow()["run"].clone();
    assert_eq!(
        run.state().borrow().status,
        TwapStatus::Running,
        "TWAP native pause rejection still resumes"
    );
    assert_eq!(
        h.spawned.borrow().len(),
        6,
        "TWAP native pause rejection still dispatches resume"
    );
    h.drain();
    assert_eq!(
        h.clock
            .logs
            .borrow()
            .iter()
            .filter(|(l, _)| l == "error")
            .count(),
        1,
        "TWAP native restore catches once"
    );
    h.replace_state.set(true);
    h.post_failure.set(false);
    handler
        .handle(&payload(
            StoreWorkerOp::TwapState,
            json!([restored("swapped", "running")]),
        ))
        .unwrap();
    assert_eq!(
        h.twaps.borrow()["swapped"].state().borrow().status,
        TwapStatus::Running,
        "TWAP native original supplied status drives resume"
    );
    handler
        .handle(&payload(
            StoreWorkerOp::TwapState,
            json!([restored("", "idle"), restored("", "idle")]),
        ))
        .unwrap();
    assert!(
        h.manager.has_id("generated-1") && h.manager.has_id("generated-2"),
        "TWAP native raw empty ID check before generated-ID add"
    );
    let ids = h
        .manager
        .running_twaps
        .borrow()
        .keys()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "dup",
            "run",
            "broken",
            "swapped",
            "generated-1",
            "generated-2"
        ],
        "TWAP native independent restore ID floor"
    );
    let errors = h
        .events
        .borrow()
        .iter()
        .filter(|e| e.starts_with("rejected:"))
        .count();
    assert_eq!(errors, 2, "TWAP native rejected pause and resume futures");
    let source = oracle();
    assert_eq!(source["twap"]["ids"], json!(ids));
    assert_eq!(source["twap"]["runStatus"], json!("running"));
    assert_eq!(source["twap"]["errors"], 1);
    assert_eq!(source["twap"]["rejections"], errors);
    h.release_cycles();
}
#[test]
fn ui_symbol_error_prevents_margin_and_live_metadata_is_read_after_symbols() {
    let h = Host::new();
    let c = h.context();
    let handler = UiStateHandler::new(c.clone());
    h.exchange.base().store.update(StoreUpdate{positions:Some(vec![serde_json::from_value(json!({"id":"p","symbol":"BTC","side":"long","entryPrice":100,"contracts":2,"notional":200,"leverage":5,"unrealizedPnl":0,"liquidationPrice":0})).unwrap()]),..Default::default()},true).unwrap();
    c.metadata
        .borrow_mut()
        .fetch_leverage_on_margin_symbol_change = Some(false);
    let events = h.events.clone();
    let meta = c.metadata.clone();
    h.exchange.base().on(
        "activeTickerSymbols:changed",
        Rc::new(move |_, _| {
            events.borrow_mut().push("symbols".into());
            meta.borrow_mut().fetch_leverage_on_margin_symbol_change = Some(true);
            Ok(())
        }),
    );
    handler
        .handle(&payload(
            StoreWorkerOp::UiState,
            json!({"activeSymbols":["BTC","",7,"BTC"],"activeMarginSymbol":"BTC"}),
        ))
        .unwrap();
    assert_eq!(
        h.exchange.base().get_active_ticker_symbols(),
        ["BTC"],
        "UI native active symbols"
    );
    assert_eq!(
        &*h.events.borrow(),
        &["symbols", "fetch:BTC"],
        "UI native symbols precede margin"
    );
    let fetching = h.exchange.base().store.fetched_leverage()["BTC"].clone();
    assert_eq!(
        fetching.is_fetching,
        Some(true),
        "UI native placeholder before return"
    );
    assert_eq!(fetching.buy_leverage, 0.0);
    assert_eq!(h.exchange.base().store.positions()[0].leverage, 5.0);
    assert_eq!(h.spawned.borrow().len(), 1);
    h.released.set(true);
    h.drain();
    assert_eq!(
        h.exchange.base().store.positions()[0].leverage,
        11.0,
        "UI native fulfilled leverage"
    );
    handler
        .handle(&payload(
            StoreWorkerOp::TwapPause,
            json!({"activeSymbols":["OTHER"],"activeMarginSymbol":"OTHER"}),
        ))
        .unwrap();
    assert_eq!(h.events.borrow().len(), 2, "UI native unrelated op ignored");
    let fail = Host::new();
    let fail_handler = UiStateHandler::new(fail.context());
    fail.exchange.base().on(
        "activeTickerSymbols:changed",
        Rc::new(|_, _| Err(ExchangeError::new("symbols failed"))),
    );
    assert_eq!(
        fail_handler
            .handle(&payload(
                StoreWorkerOp::UiState,
                json!({"activeSymbols":["BTC"],"activeMarginSymbol":"BTC"})
            ))
            .unwrap_err()
            .message,
        "symbols failed"
    );
    assert!(
        fail.exchange.base().store.fetched_leverage().is_empty(),
        "UI native symbol throw prevents margin"
    );
    assert!(fail.events.borrow().is_empty());
    let fast = Host::new();
    fast.released.set(true);
    fast.exchange.base().store.update(StoreUpdate{positions:Some(vec![serde_json::from_value(json!({"id":"p","symbol":"BTC","side":"long","entryPrice":100,"contracts":2,"notional":200,"leverage":5,"unrealizedPnl":0,"liquidationPrice":0})).unwrap()]),..Default::default()},true).unwrap();
    UiStateHandler::new(fast.context())
        .handle(&payload(
            StoreWorkerOp::UiState,
            json!({"activeMarginSymbol":"BTC"}),
        ))
        .unwrap();
    assert_eq!(
        fast.exchange.base().store.positions()[0].leverage,
        11.0,
        "UI native ready settlement before return chosen boundary"
    );
    assert!(fast.spawned.borrow().is_empty());
    let source = oracle();
    assert_eq!(source["ui"]["readyBeforeMicrotask"], 5);
    assert_eq!(source["ui"]["readyAfterMicrotask"], 11);
    assert_eq!(source["ui"]["events"], json!(*h.events.borrow()));
    assert_eq!(source["ui"]["before"], 5);
    assert_eq!(source["ui"]["after"], 11);
    assert_eq!(source["ui"]["failureFetches"], 0);
}
