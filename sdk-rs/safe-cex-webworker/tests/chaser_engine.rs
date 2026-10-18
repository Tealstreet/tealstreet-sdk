#[path = "../../safe-cex-exchange/tests/support/runtime.rs"]
mod runtime;
use safe_cex_exchange::delivery_timers::WorkerPortTimers;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    client_order_id::{ClientOrderIdRule, ClientOrderLookup},
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::{ExchangeError, ExchangeFuture},
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Order, OrderTimeInForce, OrderType, PlaceOrderOpts};
use safe_cex_webworker::chaser::ChaserOrderInstance;
use safe_cex_webworker::chaser_engine::*;
use safe_cex_webworker::chaser_manager::ChaserOrderManager;
use safe_cex_webworker::task_types::ChaserCompleteEvent;
use safe_cex_webworker::task_types::ChaserOrderOptions;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::{Rc, Weak},
    task::{Context, Poll, Waker},
};
fn ready<T: 'static>(r: Result<T, ExchangeError>) -> ExchangeFuture<'static, T> {
    Box::pin(async move { r })
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
#[derive(Default)]
struct Host {
    now: Cell<f64>,
    next: Cell<u64>,
    timers: RefCell<BTreeMap<u64, (f64, Rc<dyn Fn()>)>>,
    events: RefCell<Vec<String>>,
    places: RefCell<Vec<PlaceOrderOpts>>,
    replies: RefCell<VecDeque<Result<Vec<String>, ExchangeError>>>,
    lookups: RefCell<VecDeque<Result<ClientOrderLookup, ExchangeError>>>,
    lookup_calls: Cell<usize>,
    cancels: RefCell<Vec<Vec<ChaserCancel>>>,
    completes: RefCell<Vec<ChaserCompleteEvent>>,
    tasks: RefCell<Vec<ExchangeFuture<'static, ()>>>,
    cancel_released: Rc<Cell<bool>>,
    assert_cleanup_prefix: Cell<bool>,
    fill_mode: Cell<&'static str>,
    sleep_stop: Cell<bool>,
    instance: RefCell<Weak<ChaserOrderInstance>>,
    runtime: Rc<runtime::Runtime>,
}
impl Host {
    fn new() -> Rc<Self> {
        let h = Rc::new(Self::default());
        h.next.set(1);
        h.cancel_released.set(true);
        h.runtime.advance(0);
        h
    }
    fn drain(&self) {
        for _iteration in 0..32 {
            let tasks = std::mem::take(&mut *self.tasks.borrow_mut());
            for mut f in tasks {
                match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    Poll::Pending => self.tasks.borrow_mut().push(f),
                    Poll::Ready(r) => r.unwrap(),
                }
            }
        }
    }
    fn fire(&self, id: u64, time: f64) {
        self.now.set(time);
        self.runtime.advance(time as u64);
        let callback = self.timers.borrow_mut().remove(&id).unwrap().1;
        callback();
    }
}
impl WorkerPortTimers for Host {
    type Timer = u64;
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn set_timeout(&self, f: Rc<dyn Fn()>, delay: f64) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.timers.borrow_mut().insert(id, (delay, f));
        id
    }
    fn clear_timeout(&self, id: &u64) {
        self.timers.borrow_mut().remove(id);
    }
    fn timer_is_truthy(&self, id: &u64) -> bool {
        *id != 0
    }
}
impl ChaserHost for Host {
    fn construct_place(
        &self,
        _: Rc<dyn Exchange>,
        o: PlaceOrderOpts,
    ) -> ExchangeFuture<'static, Vec<String>> {
        self.places.borrow_mut().push(o);
        let result = self.replies.borrow_mut().pop_front().unwrap_or(Ok(vec![]));
        ready(result)
    }
    fn construct_lookup(
        &self,
        _: Rc<dyn Exchange>,
        symbol: String,
        id: String,
    ) -> ExchangeFuture<'static, ClientOrderLookup> {
        assert_eq!(symbol, "BTC");
        assert_eq!(id, "cid");
        self.lookup_calls.set(self.lookup_calls.get() + 1);
        ready(
            self.lookups
                .borrow_mut()
                .pop_front()
                .unwrap_or(Ok(ClientOrderLookup::NotFound { definitive: false })),
        )
    }
    fn construct_cancel(
        &self,
        _: Rc<dyn Exchange>,
        orders: Vec<ChaserCancel>,
    ) -> ExchangeFuture<'static, ()> {
        if let Some(i) = self
            .instance
            .borrow()
            .upgrade()
            .filter(|_| self.assert_cleanup_prefix.get())
        {
            assert!(
                i.current_order_ids().is_empty(),
                "CHASER native IDs cleared before cleanup cancellation"
            );
        }
        self.cancels.borrow_mut().push(orders);
        let release = self.cancel_released.clone();
        Box::pin(std::future::poll_fn(move |_| {
            if release.get() {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn fill_lookup(&self, _: Rc<dyn Exchange>) -> Option<FillLookup> {
        match self.fill_mode.get() {
            "partial" => Some(Rc::new(|_, _| ready(Ok(vec![("o".into(), 1.)])))),
            "empty" => Some(Rc::new(|_, _| ready(Ok(vec![])))),
            "error" => Some(Rc::new(|_, _| {
                ready(Err(ExchangeError::new("fills failed")))
            })),
            _ => None,
        }
    }
    fn construct_sleep(&self, _: Rc<dyn Exchange>, delay: f64) -> ExchangeFuture<'static, ()> {
        self.events
            .borrow_mut()
            .push(format!("sleep:{}", safe_cex_num::js_number_string(delay)));
        self.now.set(self.now.get() + delay);
        self.runtime.advance(self.now.get() as u64);
        if self.sleep_stop.replace(false) {
            self.instance
                .borrow()
                .upgrade()
                .unwrap()
                .mark_complete(Some("user"));
        }
        ready(Ok(()))
    }
    fn error_properties(&self, e: &ExchangeError) -> Value {
        json!({"name":e.name,"message":e.message,"code":e.code.as_ref().map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap()),"response":e.response})
    }
    fn console_error(&self, e: &ExchangeError) -> Result<(), ExchangeError> {
        self.events
            .borrow_mut()
            .push(format!("console:{}", e.js_error_string()));
        Ok(())
    }
    fn emit_completion(
        &self,
        _: Rc<dyn Exchange>,
        e: ChaserCompleteEvent,
    ) -> Result<(), ExchangeError> {
        self.completes.borrow_mut().push(e);
        Ok(())
    }
    fn spawn(&self, f: ExchangeFuture<'static, ()>) {
        self.tasks.borrow_mut().push(f)
    }
}
fn order(id: &str, price: f64) -> Order {
    let mut o:Order=serde_json::from_value(json!({"id":id,"symbol":"BTC","status":"open","type":"limit","side":"buy","price":0,"amount":3,"filled":0,"remaining":3,"reduceOnly":false,"lastUpdate":0})).unwrap();
    o.price = price.into();
    o
}
fn fixture(
    name: ExchangeName,
    mint: bool,
    size: f64,
) -> (Rc<Host>, Rc<ChaserEngine<Host>>, Rc<ChaserOrderInstance>) {
    let host = Host::new();
    let store = Rc::new(DefaultStore::new());
    let market=serde_json::from_value(json!({"id":"m","symbol":"BTC","base":"BTC","quote":"USD","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":0,"max":100},"leverage":{"min":1,"max":100}}})).unwrap();
    let ticker=serde_json::from_value(json!({"id":"t","symbol":"BTC","bid":100,"ask":102,"last":101,"mark":101,"index":101,"percentage":0,"openInterest":0,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![market]),
            tickers: Some(vec![ticker]),
            socket_health: Some(json!({"privateInitialized":true})),
            ..Default::default()
        },
        true,
    )
    .unwrap();
    let mut settings = ExchangeSettings::default();
    if mint {
        settings.client_order_id_rule = Some(ClientOrderIdRule {
            label: "fixture".into(),
            requirement: "cid".into(),
            is_valid: Rc::new(|id| id == "cid"),
            is_honorable_shape: None,
            mint: Some(Rc::new(|| "cid".into())),
        });
    }
    let ex: Rc<dyn Exchange> = Rc::new(Venue {
        base: BaseExchange::new(
            name,
            serde_json::from_value(json!({"key":"","secret":""})).unwrap(),
            store,
            host.runtime.clone(),
            settings,
        ),
        caps: Default::default(),
    });
    for event in ["log", "warning", "error"] {
        let h = host.clone();
        ex.base().on(
            event,
            Rc::new(move |name, data| {
                h.events.borrow_mut().push(format!(
                    "{name}:{}",
                    String::from_utf16_lossy(data.as_str().unwrap())
                ));
                Ok(())
            }),
        );
    }
    let mut opts:ChaserOrderOptions=serde_json::from_value(json!({"symbol":"BTC","size":3,"side":"buy","maxAttempts":10,"delay":100,"reduceOnly":false})).unwrap();
    opts.size = size;
    *host.runtime.id.borrow_mut() = Some("i".into());
    let instance = Rc::new(ChaserOrderInstance::new(ex, Rc::new(RefCell::new(opts))).unwrap());
    *host.instance.borrow_mut() = Rc::downgrade(&instance);
    let manager = ChaserOrderManager::new();
    manager.instances().borrow_mut().push(instance.clone());
    let engine = ChaserEngine::new(host.clone(), manager);
    (host, engine, instance)
}
fn settle<T>(mut f: ExchangeFuture<'static, T>) -> Result<T, ExchangeError> {
    for _ in 0..32 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return r;
        }
    }
    panic!("CHASER native unexpected pending after 32 polls")
}
fn source() -> Value {
    let o = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_engine_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "CHASER source oracle: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}
#[test]
fn placement_retry_ceiling_and_stopped_lost_lookup() {
    let (h, e, i) = fixture(ExchangeName::Bybit, false, 3.);
    i.mark_complete(Some("user"));
    assert!(settle(e.try_place_orders(i, 1.)).unwrap().is_empty());
    assert_eq!(
        h.places.borrow().len(),
        0,
        "CHASER native stopped zero placements"
    );
    let (h, e, i) = fixture(ExchangeName::Bybit, false, 3.);
    h.replies
        .borrow_mut()
        .extend([Ok(vec![]), Ok(vec!["o".into()])]);
    let ids = settle(e.try_place_orders(i.clone(), 1.)).unwrap();
    assert_eq!(ids, ["o"]);
    assert_eq!(
        h.places.borrow().len(),
        2,
        "CHASER native retry placement count"
    );
    assert_eq!(
        h.places.borrow()[0].price.unwrap().to_bits(),
        100f64.to_bits()
    );
    assert_eq!(
        h.places.borrow()[1].price.unwrap().to_bits(),
        99f64.to_bits()
    );
    for o in h.places.borrow().iter() {
        assert_eq!(o.amount.to_bits(), 3f64.to_bits());
        assert_eq!(o.r#type, OrderType::Limit);
        assert_eq!(o.time_in_force, Some(OrderTimeInForce::PostOnly));
    }
    assert_eq!(i.min_tick_distance(), 1.);
    assert!(!i.fresh_order());
    let retry_events = h.events.borrow().clone();
    let (h, e, i) = fixture(ExchangeName::Bybit, true, 3.);
    h.replies
        .borrow_mut()
        .push_back(Err(ExchangeError::new("timeout")));
    h.lookups.borrow_mut().extend([
        Ok(ClientOrderLookup::NotFound { definitive: false }),
        Ok(ClientOrderLookup::Found(Box::new(order("late", 100.)))),
    ]);
    h.sleep_stop.set(true);
    assert!(
        settle(e.try_place_orders(i.clone(), 1.))
            .unwrap()
            .is_empty()
    );
    assert_eq!(h.places.borrow().len(), 1);
    assert_eq!(h.places.borrow()[0].client_order_id.as_deref(), Some("cid"));
    assert_eq!(
        h.lookup_calls.get(),
        2,
        "CHASER native stopped final lookup"
    );
    assert_eq!(
        h.cancels.borrow().len(),
        1,
        "CHASER native late placement cancellation"
    );
    assert!(
        matches!(&h.cancels.borrow()[0][0],ChaserCancel::ById{id,symbol} if id=="late"&&symbol=="BTC")
    );
    assert_eq!(i.state().borrow().stop_reason, "user");
    let lost_events = h.events.borrow().clone();
    let mut ceilings = vec![];
    for (name, expected) in [(ExchangeName::Binance, 19), (ExchangeName::Bybit, 9)] {
        let (h, e, i) = fixture(name, false, 3.);
        assert!(
            settle(e.try_place_orders(i.clone(), 1.))
                .unwrap()
                .is_empty()
        );
        h.drain();
        assert_eq!(
            h.places.borrow().len(),
            expected,
            "CHASER native independent retry ceiling"
        );
        assert_eq!(h.completes.borrow().len(), 1);
        assert!(e.manager.instances().borrow().is_empty());
        ceilings.push(expected);
    }
    let (h, e, i) = fixture(ExchangeName::Bybit, false, f64::INFINITY);
    h.replies.borrow_mut().push_back(Ok(vec!["o".into()]));
    settle(e.try_place_orders(i, 1.)).unwrap();
    assert_eq!(
        h.places.borrow()[0].amount.to_bits(),
        f64::INFINITY.to_bits(),
        "CHASER native nonfinite placement amount"
    );
    let src = source();
    assert_eq!(src["retryEvents"], json!(retry_events));
    assert_eq!(src["lostEvents"], json!(lost_events));
    assert_eq!(src["ceilings"], json!(ceilings));
    assert_eq!(src["infiniteAmount"], "Infinity");
}
#[test]
fn cleanup_prefix_identity_live_cancel_targets_and_single_completion() {
    let (h, e, i) = fixture(ExchangeName::Bybit, false, 3.);
    h.assert_cleanup_prefix.set(true);
    i.mark_complete(Some("user"));
    i.set_current_order_ids(vec!["a".into(), "b".into()]);
    i.exchange
        .base()
        .store
        .update(
            StoreUpdate {
                orders: Some(vec![order("a", 100.)]),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    e.manager.instances().borrow_mut().push(i.clone());
    let weak = Rc::downgrade(&i);
    i.register_disposer(Box::new(move || {
        let i = weak.upgrade().unwrap();
        i.options().borrow_mut().symbol = "NEW".into();
        Ok(())
    }));
    h.cancel_released.set(false);
    let mut first = e.cleanup_instance(i.clone());
    assert_eq!(
        e.manager.instances().borrow().len(),
        1,
        "CHASER native cleanup synchronous splice"
    );
    assert!(
        i.current_order_ids().is_empty(),
        "CHASER native cleanup synchronous ID clear"
    );
    assert_eq!(h.cancels.borrow().len(), 1);
    assert!(
        matches!(&h.cancels.borrow()[0][0],ChaserCancel::Full(o) if o.id=="a"&&o.symbol=="BTC")
    );
    let second = e.cleanup_instance(i.clone());
    settle(second).unwrap();
    assert!(
        h.completes.borrow().is_empty(),
        "CHASER native concurrent nonowner cannot complete early"
    );
    assert!(
        first
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    h.cancel_released.set(true);
    settle(first).unwrap();
    assert_eq!(h.cancels.borrow().len(), 2);
    assert!(
        matches!(&h.cancels.borrow()[1][0],ChaserCancel::ById{id,symbol} if id=="b"&&symbol=="NEW")
    );
    assert_eq!(
        h.completes.borrow().len(),
        1,
        "CHASER native exactly one completion"
    );
    let event = h.completes.borrow()[0].clone();
    assert_eq!(event.symbol, "NEW");
    assert_eq!(event.stop_reason, "user");
    assert_eq!(event.size.to_bits(), 3f64.to_bits());
    assert_eq!(event.filled.to_bits(), 0f64.to_bits());
    assert!(!i.claim_complete_emission());
    assert!(e.manager.instances().borrow().is_empty());
    let (h, e, i) = fixture(ExchangeName::Bybitdemo, false, 3.);
    i.options().borrow_mut().client_id = Some("awaited".into());
    i.set_current_order_ids(vec!["o".into()]);
    let position: safe_cex_types::Position=serde_json::from_value(json!({"id":"p","symbol":"BTC","side":"long","entryPrice":100,"contracts":2,"notional":200,"leverage":1,"unrealizedPnl":0,"liquidationPrice":0})).unwrap();
    i.exchange
        .base()
        .store
        .update(
            StoreUpdate {
                positions: Some(vec![position]),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    h.fill_mode.set("empty");
    settle(e.emit_complete(i)).unwrap();
    assert_eq!(
        h.completes.borrow()[0].filled.to_bits(),
        0f64.to_bits(),
        "CHASER native successful empty REST suppresses position fallback"
    );
    let src = source();
    let expected: ChaserCompleteEvent =
        serde_json::from_value(src["cleanupEvent"].clone()).unwrap();
    assert_eq!(event, expected);
    assert_eq!(src["emptyRestFilled"].as_f64(), Some(0.));
}
#[test]
fn monitor_tracks_mutations_ticker_queue_and_two_missing_confirmations() {
    let (h, e, i) = fixture(ExchangeName::Grvt, false, 3.);
    h.replies.borrow_mut().push_back(Ok(vec!["o".into()]));
    let starting = e.process_instance(i.clone());
    assert!(
        h.events.borrow()[0].contains("processInstance starting"),
        "CHASER native process log prefix"
    );
    assert!(h.places.borrow().is_empty());
    settle(starting).unwrap();
    assert_eq!(i.current_order_ids(), ["o"]);
    let notify = |orders: Vec<Order>, orders_id: Value, positions_id: Value| {
        i.exchange
            .base()
            .emit(
                "update",
                safe_cex_exchange::js_runtime::value::from_json(&json!({"orders":orders,"updateIds":{"orders":orders_id,"positions":positions_id}})),
            )
            .unwrap();
    };
    notify(vec![order("o", 100.)], json!(1), json!(1));
    assert!(
        i.pending_orders_update(),
        "CHASER native onUpdate synchronous pending prefix"
    );
    h.drain();
    assert!(!i.pending_orders_update());
    assert_eq!(h.cancels.borrow().len(), 0);
    notify(vec![], json!(2), json!(1));
    assert!(
        !i.is_stopped(),
        "CHASER native first missing remains active"
    );
    assert!(
        h.events.borrow().last().unwrap().contains("missing once"),
        "CHASER native missing prefix before poll"
    );
    h.drain();
    h.now.set(1010.);
    notify(vec![], json!(2), json!(1));
    assert_eq!(
        h.timers.borrow().len(),
        1,
        "CHASER native bounded ticker tail"
    );
    let timer = *h.timers.borrow().keys().next().unwrap();
    assert_eq!(h.timers.borrow()[&timer].0.to_bits(), 40f64.to_bits());
    h.fire(timer, 1050.);
    h.drain();
    assert!(
        !i.is_stopped(),
        "CHASER native ticker does not confirm missing orders"
    );
    notify(vec![], json!(2), json!(2));
    h.drain();
    assert!(
        !i.is_stopped(),
        "CHASER native position does not confirm missing orders"
    );
    notify(vec![], json!(3), json!(2));
    assert!(
        i.is_stopped(),
        "CHASER native second distinct order mutation stops synchronously"
    );
    assert!(i.current_order_ids().is_empty());
    assert!(e.manager.instances().borrow().is_empty());
    h.drain();
    assert_eq!(h.cancels.borrow().len(), 1);
    assert_eq!(h.completes.borrow().len(), 1);
    assert_eq!(
        h.completes.borrow()[0].stop_reason,
        "Tracked orders disappeared from store"
    );
    let count = h.events.borrow().len();
    notify(vec![], json!(4), json!(2));
    assert_eq!(
        h.events.borrow().len(),
        count,
        "CHASER native listeners detached"
    );
    assert!(h.tasks.borrow().is_empty());
    let (hp, ep, ip) = fixture(ExchangeName::Bybitdemo, false, 3.);
    hp.replies.borrow_mut().push_back(Ok(vec!["p".into()]));
    let changing = ip.clone();
    ip.exchange.base().on(
        "log",
        Rc::new(move |_, data| {
            if data
                .as_str()
                .is_some_and(|s| String::from_utf16_lossy(s).contains("processInstance starting"))
            {
                changing.options().borrow_mut().max_attempts = 11.;
            }
            Ok(())
        }),
    );
    let process = ep.process_instance(ip.clone());
    ip.exchange
        .base()
        .store
        .update(
            StoreUpdate {
                socket_health: Some(json!({"privateInitialized":false})),
                ..Default::default()
            },
            true,
        )
        .unwrap();
    settle(process).unwrap();
    assert_eq!(
        hp.places.borrow().len(),
        1,
        "CHASER native grace health captured in construction prefix"
    );
    assert_eq!(
        ip.options().borrow().max_attempts,
        11.,
        "CHASER native log listener can reenter options"
    );
    ip.mark_complete(Some("end fixture"));
    settle(ep.cleanup_instance(ip.clone())).unwrap();
    ip.exchange.base().off("log", None);
    let expected = source();
    assert_eq!(expected["monitorReason"], i.state().borrow().stop_reason);
    assert_eq!(expected["monitorEvents"], json!(h.events.borrow().clone()));
}

#[test]
fn monitor_reprices_after_authoritative_fills_and_collapses_queued_checks() {
    let (h, e, i) = fixture(ExchangeName::Grvt, false, 3.);
    h.fill_mode.set("partial");
    h.replies
        .borrow_mut()
        .extend([Ok(vec!["o".into()]), Ok(vec!["n".into()])]);
    settle(e.process_instance(i.clone())).unwrap();
    h.cancel_released.set(false);
    let notify = |id: Value| {
        i.exchange
            .base()
            .emit(
                "update",
                safe_cex_exchange::js_runtime::value::from_json(
                    &json!({"orders":[order("o",99.)],"updateIds":{"orders":id,"positions":1}}),
                ),
            )
            .unwrap()
    };
    notify(json!(1));
    h.drain();
    assert_eq!(
        h.cancels.borrow().len(),
        1,
        "CHASER native reprice cancellation count"
    );
    assert!(matches!(&h.cancels.borrow()[0][0],ChaserCancel::Full(o) if o.id=="o"));
    assert_eq!(
        i.current_order_ids(),
        ["o"],
        "CHASER native reprice retains IDs during pending cancellation"
    );
    notify(json!(2));
    h.drain();
    assert!(i.pending_orders_update());
    assert_eq!(
        h.places.borrow().len(),
        1,
        "CHASER native no replacement before cancel settles"
    );
    h.cancel_released.set(true);
    h.drain();
    assert_eq!(
        h.places.borrow().len(),
        2,
        "CHASER native replacement count"
    );
    assert_eq!(
        i.filled_size().to_bits(),
        1f64.to_bits(),
        "CHASER native authoritative fills applied"
    );
    assert_eq!(
        h.places.borrow()[1].amount.to_bits(),
        2f64.to_bits(),
        "CHASER native replacement subtracts REST fill"
    );
    assert_eq!(i.current_order_ids(), ["n"]);
    assert!(!i.pending_orders_update());
    assert_eq!(
        h.timers.borrow().len(),
        1,
        "CHASER native concurrent mutations collapse to one tail"
    );
    let timer = *h.timers.borrow().keys().next().unwrap();
    assert_eq!(h.timers.borrow()[&timer].0.to_bits(), 50f64.to_bits());
    h.fire(timer, 2050.);
    h.drain();
    assert!(h.timers.borrow().is_empty());
    assert_eq!(h.places.borrow().len(), 2);
    let reprice_events = h.events.borrow().clone();
    i.mark_complete(Some("user"));
    settle(e.cleanup_instance(i)).unwrap();

    let (h, e, i) = fixture(ExchangeName::Grvt, false, 3.);
    h.replies.borrow_mut().push_back(Ok(vec!["o".into()]));
    settle(e.process_instance(i.clone())).unwrap();
    i.exchange
        .base()
        .emit(
            "update",
            safe_cex_exchange::js_runtime::value::from_json(
                &json!({"orders":[order("o",99.)],"updateIds":{"orders":1,"positions":1}}),
            ),
        )
        .unwrap();
    h.drain();
    assert_eq!(
        h.places.borrow().len(),
        1,
        "CHASER native unavailable fills stop before replacement"
    );
    assert_eq!(h.cancels.borrow().len(), 2);
    assert_eq!(
        i.state().borrow().stop_reason,
        "Failed to reconcile fills after cancellation"
    );
    assert_eq!(h.completes.borrow().len(), 1);
    let unavailable_reason = i.state().borrow().stop_reason.clone();

    let (h, e, i) = fixture(ExchangeName::Grvt, false, 3.);
    h.replies.borrow_mut().push_back(Ok(vec!["o".into()]));
    settle(e.process_instance(i.clone())).unwrap();
    for (id, symbol) in [("other", "BTC"), ("o", "OTHER")] {
        i.exchange
            .base()
            .emit(
                "fill",
                safe_cex_exchange::js_runtime::value::from_json(
                    &json!({"orderId":id,"symbol":symbol,"amount":3}),
                ),
            )
            .unwrap();
        assert_eq!(i.filled_size().to_bits(), 0f64.to_bits());
    }
    i.exchange
        .base()
        .emit(
            "fill",
            safe_cex_exchange::js_runtime::value::from_json(
                &json!({"orderId":"o","symbol":"BTC","amount":3}),
            ),
        )
        .unwrap();
    assert!(
        i.is_stopped(),
        "CHASER native matching fill stops in callback prefix"
    );
    assert!(i.current_order_ids().is_empty());
    h.drain();
    assert_eq!(h.completes.borrow().len(), 1);
    assert_eq!(h.completes.borrow()[0].filled.to_bits(), 3f64.to_bits());
    let fill_reason = i.state().borrow().stop_reason.clone();

    let (h, e, i) = fixture(ExchangeName::Grvt, false, 3.);
    h.replies.borrow_mut().push_back(Ok(vec!["o".into()]));
    settle(e.process_instance(i.clone())).unwrap();
    for id in [json!(1), json!("1"), Value::Null] {
        i.exchange
            .base()
            .emit(
                "update",
                safe_cex_exchange::js_runtime::value::from_json(
                    &json!({"orders":[order("o",100.)],"updateIds":{"orders":id,"positions":1}}),
                ),
            )
            .unwrap();
        assert!(
            i.pending_orders_update(),
            "CHASER native strict update ID preserves number/string/null distinction"
        );
        h.drain();
    }
    i.exchange
        .base()
        .emit(
            "update",
            safe_cex_exchange::js_runtime::value::from_json(
                &json!({"orders":[order("o",100.)],"updateIds":{"orders":null,"positions":1}}),
            ),
        )
        .unwrap();
    assert!(!i.pending_orders_update());
    i.mark_complete(Some("user"));
    settle(e.cleanup_instance(i)).unwrap();
    let src = source();
    assert_eq!(src["repriceEvents"], json!(reprice_events));
    assert_eq!(src["unavailableReason"], unavailable_reason);
    assert_eq!(src["fillReason"], fill_reason);
}
