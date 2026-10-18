#[path = "support/canonical_value.rs"]
mod canonical_value;
#[cfg(feature = "native")]
#[path = "support/private_order_aliases.rs"]
mod private_order_aliases;
use canonical_value::category;
use safe_cex_exchange::runtime::*;
use safe_cex_venue_binance::{
    api_factory::RemoteSignCall, class_jobs::NativeFuture, listen_key::*, private_classes::*,
    private_event_value::*, private_events::*, private_session::Venue, request_weight::Failure,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn run<T>(runtime: &Runtime, future: impl Future<Output = T>) -> T {
    let mut f = Box::pin(future);
    for _ in 0..16 {
        if let Poll::Ready(result) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return result;
        }
        runtime.pump();
    }
    panic!("fixture exceeded independent 16-poll bound")
}

struct Task;
impl ExchangeTask for Task {
    fn cancel(&self) {}
}
#[derive(Default)]
struct Runtime {
    now: Cell<f64>,
    monotonic_clock: Cell<Option<f64>>,
    sleep_clock: Rc<Cell<f64>>,
    sleeps: RefCell<Vec<u64>>,
    reported: RefCell<Vec<String>>,
    jobs: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    dial_socket: RefCell<Option<Rc<dyn WsSocket>>>,
}
impl Runtime {
    fn pump(&self) {
        let jobs = std::mem::take(&mut *self.jobs.borrow_mut());
        for mut job in jobs {
            if job
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.jobs.borrow_mut().push(job);
            }
        }
    }
}
impl ExchangeRuntime for Runtime {
    fn now_ms(&self) -> f64 {
        self.now.get()
    }
    fn monotonic_now_ms(&self) -> f64 {
        self.monotonic_clock.get().unwrap_or(22000.0)
    }
    fn sleep(&self, delay: u64) -> ExchangeFuture<'static, ()> {
        self.sleeps.borrow_mut().push(delay);
        let clock = self.sleep_clock.clone();
        let deadline = self.now.get() + delay as f64;
        Box::pin(std::future::poll_fn(move |_| {
            if clock.get() >= deadline {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn spawn_local(&self, f: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.jobs.borrow_mut().push(f);
        Box::new(Task)
    }
    fn console_values(&self, _: &str, _: &[safe_cex_exchange::js_runtime::value::Js]) {}
    fn unique_id(&self) -> String {
        "fixture".into()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        0.5
    }
    fn report_raw_exception(
        &self,
        _: &safe_cex_exchange::js_runtime::value::Js,
        _: &serde_json::Value,
    ) {
        panic!("unexpected raw exception report in this test runtime");
    }
    fn report_exception(&self, error: &ExchangeError, _: &Value) {
        self.reported.borrow_mut().push(error.name.clone());
    }
    fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
        panic!("unexpected worker")
    }
}
#[derive(Default)]
struct Socket {
    listeners: RefCell<Vec<(&'static str, PrivateCallback)>>,
    removed: RefCell<Vec<&'static str>>,
    closed: Cell<usize>,
}
impl NativePrivateSocket for Socket {
    fn remove_listener(
        &self,
        event: &'static str,
        callback: &PrivateCallback,
    ) -> Result<(), Failure> {
        self.removed.borrow_mut().push(event);
        self.listeners
            .borrow_mut()
            .retain(|(name, action)| *name != event || !action.same(callback));
        Ok(())
    }
    fn ready_state(&self) -> Result<f64, Failure> {
        Ok(1.0)
    }
    fn close(&self) -> Result<(), Failure> {
        self.closed.set(self.closed.get() + 1);
        Ok(())
    }
    fn add_listener(&self, event: &'static str, action: PrivateCallback) -> Result<(), Failure> {
        self.listeners.borrow_mut().push((event, action));
        Ok(())
    }
}
#[derive(Default)]
struct Base {
    disposed: Cell<bool>,
    forced: Cell<bool>,
    received: Cell<usize>,
    stats: RefCell<Vec<String>>,
    frames: RefCell<Vec<String>>,
    urls: RefCell<Vec<String>>,
    socket: Rc<Socket>,
    errors: RefCell<Vec<String>>,
    pong: Cell<usize>,
    ping_at: Cell<f64>,
    diagnostic_sends: Cell<usize>,
    diagnostic_frames: RefCell<Vec<String>>,
    diagnostic_failure: RefCell<Option<Failure>>,
    send_failure: RefCell<Option<Failure>>,
}
impl PrivateBase for Base {
    fn disposed(&self) -> bool {
        self.disposed.get()
    }
    fn force_closed(&self) -> bool {
        self.forced.get()
    }
    fn connected(&self) -> bool {
        !self.urls.borrow().is_empty() && !self.disposed.get() && !self.forced.get()
    }
    fn track(&self) -> Result<(), Failure> {
        self.received.set(self.received.get() + 1);
        Ok(())
    }
    fn diagnostic(&self, _: EventValue) -> Result<(), Failure> {
        Ok(())
    }
    fn stats(&self, topic: &str) -> Result<(), Failure> {
        self.stats.borrow_mut().push(topic.into());
        Ok(())
    }
    fn set_ping_at(&self, time: f64) -> Result<(), Failure> {
        self.ping_at.set(time);
        Ok(())
    }
    fn diagnostic_send(&self, payload: &str) -> Result<(), Failure> {
        self.diagnostic_sends.set(self.diagnostic_sends.get() + 1);
        self.diagnostic_frames.borrow_mut().push(payload.into());
        self.diagnostic_failure.borrow().clone().map_or(Ok(()), Err)
    }
    fn pong(&self) -> Result<(), Failure> {
        self.pong.set(self.pong.get() + 1);
        Ok(())
    }
    fn send(&self, payload: String) -> Result<(), Failure> {
        if let Some(error) = self.send_failure.borrow().clone() {
            return Err(error);
        }
        self.frames.borrow_mut().push(payload);
        Ok(())
    }
    fn create(&self, url: String) -> Result<Rc<dyn NativePrivateSocket>, Failure> {
        self.urls.borrow_mut().push(url);
        Ok(self.socket.clone())
    }
    fn on_close(&self) -> Result<(), Failure> {
        Ok(())
    }
    fn force_close(&self) -> Result<(), Failure> {
        self.forced.set(true);
        Ok(())
    }
    fn reconnect(&self) -> Result<(), Failure> {
        Ok(())
    }
    fn dispose(&self) -> Result<(), Failure> {
        self.disposed.set(true);
        self.socket.listeners.borrow_mut().clear();
        Ok(())
    }
    fn emit_error(&self, message: String) -> Result<(), Failure> {
        self.errors.borrow_mut().push(message);
        Ok(())
    }
}
#[derive(Default)]
struct Timers {
    callbacks: RefCell<Vec<(EventValue, CapturedAction, f64)>>,
    cleared: RefCell<Vec<EventValue>>,
}
impl PrivateTimers for Timers {
    fn set(&self, action: CapturedAction, delay: f64) -> Result<EventValue, Failure> {
        let ticket = record([("id", number(self.callbacks.borrow().len() as f64 + 1.0))]);
        self.callbacks
            .borrow_mut()
            .push((ticket.clone(), action, delay));
        Ok(ticket)
    }
    fn clear(&self, ticket: EventValue) -> Result<(), Failure> {
        self.cleared.borrow_mut().push(ticket);
        Ok(())
    }
}
struct Parent {
    remote_sign_call: RefCell<Option<RemoteSignCall>>,
    options: EventValue,
    orders: EventArray,
    positions: EventArray,
    balance: EventValue,
    loaded: EventValue,
    effects: Rc<RefCell<Vec<(PrivateOperation, Vec<EventValue>)>>>,
    version: Cell<f64>,
    stale: Cell<bool>,
    fetched_orders: Cell<usize>,
    fetched_positions: Cell<usize>,
    key_ready: Rc<Cell<bool>>,
    key_value: Rc<RefCell<Option<EventValue>>>,
    keys: RefCell<Vec<&'static str>>,
    applied: RefCell<Vec<EventValue>>,
}
impl PrivateParent for Parent {
    fn options(&self) -> Result<EventValue, Failure> {
        Ok(self.options.clone())
    }
    fn orders(&self) -> Result<EventArray, Failure> {
        Ok(self.orders.clone())
    }
    fn positions(&self) -> Result<EventArray, Failure> {
        Ok(self.positions.clone())
    }
    fn balance(&self) -> Result<EventValue, Failure> {
        Ok(self.balance.clone())
    }
    fn loaded(&self) -> Result<EventValue, Failure> {
        Ok(self.loaded.clone())
    }
    fn capture_store(&self, operation: PrivateOperation) -> Result<PrivateHandler, Failure> {
        let effects = self.effects.clone();
        Ok(Rc::new(move |args| {
            effects.borrow_mut().push((operation, args));
            Ok(())
        }))
    }
    fn fill(&self, payload: EventValue) -> Result<(), Failure> {
        self.effects
            .borrow_mut()
            .push((PrivateOperation::Fill, vec![payload]));
        Ok(())
    }
    fn asset_price(&self, _: &EventValue) -> Result<f64, Failure> {
        Ok(1.0)
    }
    fn mark_position(&self) -> Result<(), Failure> {
        self.version.set(self.version.get() + 1.0);
        Ok(())
    }
    fn position_version(&self) -> Result<f64, Failure> {
        Ok(self.version.get())
    }
    fn paused(&self) -> Result<bool, Failure> {
        Ok(false)
    }
    fn orders_stale(&self) -> Result<bool, Failure> {
        Ok(self.stale.get())
    }
    fn fetch_positions(&self) -> Result<NativeFuture<EventValue>, Failure> {
        self.fetched_positions.set(self.fetched_positions.get() + 1);
        let rows = EventValue::Array(self.positions.clone());
        Ok(Box::pin(async move { Ok(rows) }))
    }
    fn fetch_orders(&self) -> Result<NativeFuture<EventValue>, Failure> {
        self.fetched_orders.set(self.fetched_orders.get() + 1);
        let rows = EventValue::Array(self.orders.clone());
        Ok(Box::pin(async move { Ok(rows) }))
    }
    fn replace_positions(&self, positions: EventValue) -> Result<(), Failure> {
        self.applied.borrow_mut().push(positions);
        Ok(())
    }
    fn post_key(&self, endpoint: &'static str) -> Result<PinKey, Failure> {
        self.keys.borrow_mut().push(endpoint);
        let ready = self.key_ready.clone();
        let value = self.key_value.clone();
        Ok(Box::pin(async move {
            std::future::poll_fn(move |_| {
                if ready.get() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            })
            .await;
            Ok(ListenKeyResponse::Data(ListenKeyData::Present(value)))
        }))
    }
    fn put_key(&self, endpoint: &'static str) -> Result<NativeFuture<()>, Failure> {
        self.keys.borrow_mut().push(endpoint);
        Ok(Box::pin(async { Ok(()) }))
    }
    fn remote_sign(&self, _: EventValue) -> Result<RemoteSignCall, Failure> {
        Ok(self
            .remote_sign_call
            .borrow()
            .as_ref()
            .expect("unexpected signer")
            .clone())
    }
    fn log(&self, _: String, _: LogSeverity) -> Result<(), Failure> {
        Ok(())
    }
}
fn fixture(
    venue: Venue,
) -> (
    Rc<PrivateSocket>,
    Rc<Parent>,
    Rc<Base>,
    Rc<Timers>,
    Rc<Runtime>,
) {
    let parent = Rc::new(Parent {
        remote_sign_call: Default::default(),
        options: record([("key", string("key")), ("secret", string("secret"))]),
        orders: Default::default(),
        positions: Default::default(),
        balance: record([]),
        loaded: record([("tickers", EventValue::Bool(true))]),
        effects: Default::default(),
        version: Cell::new(0.0),
        stale: Cell::new(false),
        fetched_orders: Cell::new(0),
        fetched_positions: Cell::new(0),
        key_ready: Rc::new(Cell::new(true)),
        key_value: Rc::new(RefCell::new(Some(string("key /+")))),
        keys: Default::default(),
        applied: Default::default(),
    });
    let base = Rc::new(Base::default());
    let timers = Rc::new(Timers::default());
    let runtime = Rc::new(Runtime {
        now: Cell::new(20000.0),
        ..Default::default()
    });
    let owner = PrivateSocket::new(
        venue,
        PrivateDependencies {
            parent: parent.clone(),
            base_factory: {
                let base = base.clone();
                let expected = parent.clone();
                Rc::new(move |parent| {
                    assert!(
                        Rc::ptr_eq(&parent, &(expected.clone() as Rc<dyn PrivateParent>)),
                        "native implicit Base constructor receives original parent"
                    );
                    Ok(base.clone())
                })
            },
            timers: timers.clone(),
            runtime: runtime.clone(),
            virtual_now: Rc::new(|| 21000.0),
        },
    )
    .unwrap();
    (owner, parent, base, timers, runtime)
}
#[test]
fn private_classes_bind_defaults_connection_auth_and_reopen() {
    let mut wire = vec![];
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let (owner, parent, base, timers, runtime) = fixture(venue);
        assert!(
            Rc::ptr_eq(&owner.parent, &(parent.clone() as Rc<dyn PrivateParent>)),
            "native private same parent receiver"
        );
        assert_eq!(
            owner.connect.retry_delay.get(),
            5000.0,
            "native private initial retry delay"
        );
        assert_eq!(
            owner.position.throttle_ms.get(),
            if venue == Venue::PM { 500.0 } else { 10000.0 },
            "native private venue position throttle"
        );
        assert!(!owner.position.pending_events.get());
        assert!(!owner.position.scheduled.get());
        assert!(!owner.position.in_flight.get());
        assert_eq!(owner.position.last_fetch_time.get(), 0.0);
        assert!(!owner.connect.connecting.get());
        assert!(!owner.connect.resync_orders.get());
        assert!(!owner.has_opened.get());
        assert_eq!(owner.subscription_attempt.get(), 0.0);
        assert!(!owner.keepalive_failing.keepalive_failing.get());
        run(&runtime, owner.connect_and_subscribe()).unwrap();
        assert_eq!(
            base.urls.borrow().len(),
            1,
            "independent private dial count"
        );
        assert_eq!(
            base.socket
                .listeners
                .borrow()
                .iter()
                .map(|(name, _)| *name)
                .collect::<Vec<_>>(),
            ["message", "close", "open"],
            "native private own bound listeners"
        );
        if venue == Venue::Spot {
            assert_eq!(
                parent.keys.borrow().len(),
                0,
                "independent Spot has no listen-key request"
            );
            assert_eq!(
                timers.callbacks.borrow().len(),
                0,
                "native Spot no listen-key refresh"
            );
        } else {
            assert_eq!(
                *parent.keys.borrow(),
                [if venue == Venue::PM {
                    "/papi/v1/listenKey"
                } else {
                    "/fapi/v1/listenKey"
                }],
                "native parent client endpoint"
            );
            assert_eq!(
                timers.callbacks.borrow()[0].2,
                1800000.0,
                "native connected key refresh delay"
            );
        }
        run(&runtime, owner.on_open()).unwrap();
        let first: Value = serde_json::from_str(&base.frames.borrow()[0]).unwrap();
        if venue == Venue::Spot {
            assert_eq!(
                owner.subscription_attempt.get(),
                1.0,
                "native Spot subscription attempt"
            );
            assert_eq!(first["id"].as_str(), Some("tealstreet-user-data"));
            assert_eq!(
                first["method"].as_str(),
                Some("userDataStream.subscribe.signature")
            );
            assert_eq!(first["params"]["apiKey"].as_str(), Some("key"));
            assert_eq!(first["params"]["timestamp"].as_f64(), Some(21000.0));
            assert_eq!(first["params"]["recvWindow"].as_f64(), Some(5000.0));
            assert_eq!(
                first["params"]["signature"].as_str(),
                Some(
                    safe_cex_crypto::hmac_sha256(
                        b"secret",
                        b"apiKey=key&recvWindow=5000&timestamp=21000",
                        safe_cex_crypto::Encoding::Hex
                    )
                    .as_str()
                ),
                "native Spot signed exact payload"
            );
        } else {
            assert!(owner.has_opened.get());
            assert_eq!(
                base.ping_at.get(),
                22000.0,
                "native private ping uses performance clock"
            );
            assert_eq!(
                base.diagnostic_sends.get(),
                1,
                "native private ping logs after send"
            );
            assert_eq!(first["id"].as_f64(), Some(42.0));
            assert_eq!(first["method"].as_str(), Some("LIST_SUBSCRIPTIONS"));
            run(&runtime, owner.on_open()).unwrap();
            assert_eq!(
                timers.callbacks.borrow().last().unwrap().2,
                10000.0,
                "native reopen binds orders debounce"
            );
        }
        owner.dispose().unwrap();
        assert!(base.disposed.get());
        assert_eq!(
            base.socket.listeners.borrow().len(),
            0,
            "native imported Base disposal removes listeners"
        );
        wire.push((venue, base.urls.borrow()[0].clone()));
    }
    for (venue, url) in wire {
        match venue {
            Venue::Futures => assert!(url.ends_with("?listenKey=key%20%2F%2B")),
            Venue::PM => assert!(url.ends_with("/key /+")),
            Venue::Spot => assert_eq!(url, "wss://ws-api.binance.com:443/ws-api/v3"),
        }
    }
}
#[test]
fn shared_runtime_graph_enters_bound_private_orders_without_projection() {
    use safe_cex_exchange::js_runtime::value::{Js, to_typed};
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let (owner, parent, _, _, _) = fixture(venue);
        let nested = Js::object(vec![("live", Js::Number(1.0))]);
        let data = Js::object(vec![
            ("c", Js::str("client")),
            ("i", Js::Number(42.0)),
            ("X", Js::str("NEW")),
            ("ot", Js::str("LIMIT")),
            ("o", Js::str("LIMIT")),
            ("s", Js::str("BTC")),
            ("S", Js::str("BUY")),
            ("p", Js::Number(f64::INFINITY)),
            ("q", Js::str("-0")),
            ("z", Js::Number(f64::NAN)),
            ("nested", nested.clone()),
        ]);
        let event = if venue == Venue::Spot {
            data.clone()
        } else {
            Js::object(vec![("o", data.clone())])
        };
        let events = Js::array(vec![event]);
        let Js::Array(slots) = &events else {
            unreachable!()
        };
        slots.borrow_mut().insert(0, None);
        owner.handle_events(1, events).unwrap();
        let effects = parent.effects.borrow();
        let adds = effects
            .iter()
            .filter(|(op, _)| *op == PrivateOperation::AddOrder)
            .collect::<Vec<_>>();
        assert_eq!(adds.len(), 1, "SHARED bound forEach skips holes");
        let row = &adds[0].1[0];
        assert_eq!(
            row.get("price").unwrap().as_f64(),
            Some(f64::INFINITY),
            "SHARED source parseFloat preserves infinity"
        );
        assert_eq!(
            row.get("amount").unwrap().as_f64().unwrap().to_bits(),
            (-0.0f64).to_bits(),
            "SHARED source string parseFloat preserves negative zero"
        );
        assert!(
            row.get("filled").unwrap().as_f64().unwrap().is_nan(),
            "SHARED source NaN remains native"
        );
        let info = row.get("info").unwrap();
        assert_eq!(
            info.same_reference(&data),
            venue != Venue::Futures,
            "SHARED source info identity versus shallow spread"
        );
        assert!(
            info.get("nested").unwrap().same_reference(&nested),
            "SHARED nested reference survives source spread"
        );
        nested.set("live", Js::Number(f64::NEG_INFINITY)).unwrap();
        assert_eq!(
            info.get("nested").unwrap().get("live").unwrap().as_f64(),
            Some(f64::NEG_INFINITY),
            "SHARED late input mutation stays live"
        );
        let price: f64 = to_typed(&row.get("price").unwrap()).unwrap();
        assert_eq!(
            price,
            f64::INFINITY,
            "SHARED native typed output keeps infinity"
        );
    }
}

#[test]
fn private_classes_route_events_to_native_bodies_and_eager_refresh() {
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let (owner, parent, base, timers, runtime) = fixture(venue);
        let data = json!({"e":if venue==Venue::Spot{"executionReport"}else{"ORDER_TRADE_UPDATE"},"c":"client","i":42,"X":"NEW","ot":"LIMIT","o":"LIMIT","s":"BTC","S":"BUY","p":"12.3","sp":"1.2","q":"3.6","z":"1.1","R":false});
        let event = if venue == Venue::Spot {
            data.clone()
        } else {
            json!({"e":"ORDER_TRADE_UPDATE","o":data})
        };
        owner.on_message(string(event.to_string())).unwrap();
        assert_eq!(base.received.get(), 1, "independent private received count");
        assert_eq!(
            *base.stats.borrow(),
            ["order"],
            "native private event statistics routing"
        );
        let effects = parent.effects.borrow();
        let adds = effects
            .iter()
            .filter(|(op, _)| *op == PrivateOperation::AddOrder)
            .collect::<Vec<_>>();
        assert_eq!(adds.len(), 1, "independent native new-order call count");
        let row = &adds[0].1[0];
        assert!(
            matches!(row.read("price").unwrap(),EventValue::Number(n)if n==12.3),
            "native bound private order price"
        );
        assert!(
            matches!(row.read("amount").unwrap(),EventValue::Number(n)if n==3.6),
            "native bound private order amount"
        );
        assert!(matches!(row.read("filled").unwrap(),EventValue::Number(n)if n==1.1));
        assert!(matches!(row.read("remaining").unwrap(),EventValue::Number(n)if n==3.6-1.1));
        assert!(matches!(
            category::<safe_cex_types::OrderType>(&(row.read("type").unwrap())),
            safe_cex_types::OrderType::Limit
        ));
        assert!(matches!(row.read("lastUpdate").unwrap(),EventValue::Number(n)if n==21000.0));
        drop(effects);
        if venue != Venue::Spot {
            assert_eq!(
                timers.callbacks.borrow().len(),
                1,
                "independent orders timer count"
            );
            let action = timers.callbacks.borrow()[0].1.clone();
            action.call().unwrap();
            assert_eq!(
                parent.fetched_orders.get(),
                1,
                "native timer eagerly starts parent fetch before returning"
            );
            assert!(matches!(
                *owner.orders_timer.borrow(),
                EventValue::Undefined
            ));
            assert!(
                !parent
                    .effects
                    .borrow()
                    .iter()
                    .any(|(op, _)| *op == PrivateOperation::UpdateStore),
                "native Ready order fetch defers post-await Store update"
            );
            runtime.pump();
            assert!(
                parent.effects.borrow().iter().any(|(op, args)| *op
                    == PrivateOperation::UpdateStore
                    && args[0]
                        .read("loaded")
                        .unwrap()
                        .read("orders")
                        .unwrap()
                        .same(&EventValue::Bool(true))),
                "native repaired orders mark loaded"
            );
            owner.position.pending_events.set(true);
            owner.throttled_positions().unwrap();
            assert_eq!(
                parent.fetched_positions.get(),
                1,
                "native position body starts parent fetch immediately"
            );
            assert_eq!(
                parent.applied.borrow().len(),
                0,
                "native Ready position fetch defers full replacement"
            );
            assert!(
                owner.position.in_flight.get() == (venue == Venue::Futures),
                "native Futures in-flight remains set until queued continuation"
            );
            runtime.pump();
            assert_eq!(
                parent.applied.borrow().len(),
                1,
                "independent position replacement count"
            );
            assert!(
                parent.applied.borrow()[0].same(&EventValue::Array(parent.positions.clone())),
                "native parent position result retains identity"
            );
            assert!(!owner.position.in_flight.get());
            assert_eq!(owner.position.last_fetch_time.get(), 20000.0);
            runtime.pump();
        }
    }
}

#[test]
fn private_class_native_stages_before_actual_source() {
    let mut native = vec![];
    let mut inputs = vec![];
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let (owner, parent, base, timers, runtime) = fixture(venue);
        let initial = if venue == Venue::Spot {
            json!({"retry":null,"throttle":null,"attempt":0})
        } else {
            json!({"retry":owner.connect.retry_delay.get(),"throttle":owner.position.throttle_ms.get(),"attempt":null})
        };
        assert_eq!(
            owner.connect.retry_delay.get(),
            5000.0,
            "native source-floor retry initial"
        );
        assert_eq!(
            owner.position.throttle_ms.get(),
            if venue == Venue::PM { 500.0 } else { 10000.0 },
            "native source-floor throttle"
        );
        assert_eq!(owner.subscription_attempt.get(), 0.0);
        let task = owner.connect_and_subscribe();
        assert_eq!(
            parent.keys.borrow().len(),
            if venue == Venue::Spot { 0 } else { 1 },
            "native constructor/callee synchronous POST prefix"
        );
        if venue != Venue::Spot {
            assert!(
                owner.connect.connecting.get(),
                "native connecting remains true before listen-key reaction"
            );
            assert_eq!(
                base.urls.borrow().len(),
                0,
                "native no dial before queued key continuation"
            );
        }
        run(&runtime, task).unwrap();
        assert_eq!(
            base.urls.borrow().len(),
            1,
            "independent native source-floor dial count"
        );
        let keys = parent.keys.borrow().clone();
        let refresh = timers
            .callbacks
            .borrow()
            .iter()
            .map(|(_, _, delay)| *delay)
            .collect::<Vec<_>>();
        assert_eq!(
            refresh,
            if venue == Venue::Spot {
                vec![]
            } else {
                vec![1800000.0]
            },
            "native source-floor refresh delays"
        );
        let listeners = base
            .socket
            .listeners
            .borrow()
            .iter()
            .map(|(name, _)| *name)
            .collect::<Vec<_>>();
        assert_eq!(
            listeners,
            ["message", "close", "open"],
            "native source-floor bound listener order"
        );
        run(&runtime, owner.on_open()).unwrap();
        let first: Value = serde_json::from_str(&base.frames.borrow()[0]).unwrap();
        if venue == Venue::Spot {
            assert_eq!(
                first["params"]["timestamp"].as_f64(),
                Some(21000.0),
                "native source-floor virtual auth timestamp"
            );
            assert_eq!(first["params"]["apiKey"].as_str(), Some("key"));
            assert_eq!(first["params"]["recvWindow"].as_f64(), Some(5000.0));
            assert_eq!(
                first["params"]["signature"].as_str(),
                Some("8f7a6626e42b033449cae63460d349375b5b06dbb640912c4ea9b342c86c07c1"),
                "native independently fixed HMAC signature"
            );
        } else {
            assert_eq!(first["id"].as_f64(), Some(42.0));
            assert_eq!(first["method"].as_str(), Some("LIST_SUBSCRIPTIONS"));
            assert_eq!(
                base.ping_at.get(),
                22000.0,
                "native source-floor monotonic ping timestamp"
            );
            run(&runtime, owner.on_open()).unwrap();
        }
        let data = json!({"e":if venue==Venue::Spot{"executionReport"}else{"ORDER_TRADE_UPDATE"},"c":"client","i":42,"X":"NEW","ot":"LIMIT","o":"LIMIT","s":"BTC","S":"BUY","p":"12.3","sp":"1.2","q":"3.6","z":"1.1","R":false});
        let event = if venue == Venue::Spot {
            data
        } else {
            json!({"e":"ORDER_TRADE_UPDATE","o":data})
        };
        owner.on_message(string(event.to_string())).unwrap();
        let effects = parent.effects.borrow();
        let rows = effects
            .iter()
            .filter(|(op, _)| *op == PrivateOperation::AddOrder)
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 1, "independent source-floor add-order count");
        let row = rows[0].1[0].clone();
        drop(effects);
        let id = row.read("id").unwrap().text().unwrap();
        assert_eq!(
            id,
            if venue == Venue::Futures {
                "42"
            } else {
                "client"
            },
            "native source-floor order identifier"
        );
        let read = |field: &str| {
            let EventValue::Number(value) = row.get(field).unwrap() else {
                panic!("native numeric order field {field}")
            };
            value
        };
        let (price, amount, filled, remaining, last) = (
            read("price"),
            read("amount"),
            read("filled"),
            read("remaining"),
            read("lastUpdate"),
        );
        assert_eq!(price, 12.3, "native source-floor price");
        assert_eq!(amount, 3.6, "native source-floor amount");
        assert_eq!(filled, 1.1);
        assert_eq!(remaining, 2.5);
        assert_eq!(last, 21000.0, "native source-floor virtual fill clock");
        assert!(matches!(
            category::<safe_cex_types::OrderType>(&(row.read("type").unwrap())),
            safe_cex_types::OrderType::Limit
        ));
        assert_eq!(
            base.received.get(),
            1,
            "independent source-floor receive count"
        );
        assert_eq!(*base.stats.borrow(), ["order"]);
        let (before, after) = if venue == Venue::Spot {
            (Value::Null, Value::Null)
        } else {
            let action = timers.callbacks.borrow().last().unwrap().1.clone();
            assert!(
                action.same(&owner.capture_fetch_throttled()),
                "native orders timer captures exact current arrow identity"
            );
            action.call().unwrap();
            let count = || {
                parent
                    .effects
                    .borrow()
                    .iter()
                    .filter(|(op, _)| *op == PrivateOperation::UpdateStore)
                    .count()
            };
            assert_eq!(
                parent.fetched_orders.get(),
                1,
                "native source-floor eager fetch prefix"
            );
            assert_eq!(
                count(),
                0,
                "native source-floor no Store write before ready-await reaction"
            );
            let before = json!({"fetches":1,"updates":0});
            runtime.pump();
            assert_eq!(
                count(),
                1,
                "independent source-floor queued Store write count"
            );
            let loaded = parent
                .effects
                .borrow()
                .iter()
                .find(|(op, _)| *op == PrivateOperation::UpdateStore)
                .unwrap()
                .1[0]
                .read("loaded")
                .unwrap()
                .read("orders")
                .unwrap();
            assert!(
                loaded.same(&EventValue::Bool(true)),
                "native source-floor loaded flag"
            );
            (before, json!({"fetches":1,"updates":1,"loaded":true}))
        };
        inputs.push(json!({"venue":format!("{venue:?}")}));
        native.push(json!({"venue":format!("{venue:?}"),"sameParent":Rc::ptr_eq(&owner.parent,&(parent.clone()as Rc<dyn PrivateParent>)),"initial":initial,"urls":*base.urls.borrow(),"listeners":listeners,"keys":keys,"refresh":refresh,"first":first,"pingAt":if venue==Venue::Spot{None}else{Some(base.ping_at.get())},"diagnosticSends":base.diagnostic_sends.get(),"row":{"id":id,"price":price,"amount":amount,"filled":filled,"remaining":remaining,"lastUpdate":last,"type":"limit"},"received":base.received.get(),"stats":*base.stats.borrow(),"before":before,"after":after}));
    }
    assert_eq!(
        inputs.len(),
        3,
        "independent private class source floor through case2"
    );
    use std::io::Write;
    let mut child = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/private-classes.mjs"
        ))
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&inputs).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "actual private class oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.len(),
        3,
        "independent actual private class source count"
    );
    fn numeric(v: Value) -> Value {
        match v {
            Value::Number(n) => json!(n.as_f64().unwrap()),
            Value::Array(v) => Value::Array(v.into_iter().map(numeric).collect()),
            Value::Object(v) => {
                Value::Object(v.into_iter().map(|(k, v)| (k, numeric(v))).collect())
            }
            v => v,
        }
    }
    assert_eq!(
        numeric(json!(native)),
        numeric(json!(source)),
        "actual private class bodies after all native field/stage assertions"
    );
}

#[test]
fn private_live_disposal_and_raw_key_coercion_at_named_await_stage() {
    for venue in [Venue::Futures, Venue::PM] {
        for forced in [false, true] {
            let (owner, parent, base, _, runtime) = fixture(venue);
            parent.key_ready.set(false);
            *parent.key_value.borrow_mut() = Some(record([
                ("toString", number(7.0)),
                ("valueOf", number(8.0)),
            ]));
            let task = owner.connect_and_subscribe();
            assert!(
                owner.connect.connecting.get(),
                "native suspended key connection prefix"
            );
            if forced {
                base.forced.set(true)
            } else {
                base.disposed.set(true)
            }
            parent.key_ready.set(true);
            run(&runtime, task).unwrap();
            assert_eq!(
                base.urls.borrow().len(),
                0,
                "native live post-await disposal/force-close guard skips malformed-key coercion and dial"
            );
            assert_eq!(
                base.errors.borrow().len(),
                0,
                "independent guarded malformed key emits no listen-key failure"
            );
            assert!(!owner.connect.connecting.get());
            assert_eq!(owner.connect.retry_delay.get(), 5000.0);
        }
    }
    for venue in [Venue::Futures, Venue::PM] {
        let (owner, parent, base, timers, runtime) = fixture(venue);
        *parent.key_value.borrow_mut() = Some(record([
            ("toString", number(7.0)),
            ("valueOf", number(8.0)),
        ]));
        let failure = run(&runtime, owner.connect_and_subscribe()).unwrap_err();
        assert_eq!(
            failure.name, "TypeError",
            "native malformed key coercion fails outside fetch-key catch"
        );
        assert!(
            !owner.connect.connecting.get(),
            "native key finally clears connecting before URL coercion"
        );
        assert_eq!(
            base.errors.borrow().len(),
            0,
            "independent late coercion emits no listen-key unavailable error"
        );
        assert_eq!(
            timers.callbacks.borrow().len(),
            1,
            "independent only refresh timer, no retry after late key coercion"
        );
        assert_eq!(timers.callbacks.borrow()[0].2, 1800000.0);
        assert_eq!(owner.connect.retry_delay.get(), 5000.0);
        assert!(!owner.connect.resync_orders.get());
    }
}
#[test]
fn private_stable_callback_removal_preserves_unrelated_listener_and_releases_receiver() {
    let (owner, _, base, _, runtime) = fixture(Venue::Spot);
    let state = Rc::downgrade(&**owner);
    let other = PrivateCallback::Message(Rc::new(|_| Ok(())));
    base.socket.add_listener("message", other.clone()).unwrap();
    run(&runtime, owner.connect_and_subscribe()).unwrap();
    assert_eq!(
        base.socket.listeners.borrow().len(),
        4,
        "independent three private and one unrelated callbacks"
    );
    let previous = base
        .socket
        .listeners
        .borrow()
        .iter()
        .filter(|(_, f)| !f.same(&other))
        .map(|(name, fn_)| (*name, fn_.clone()))
        .collect::<Vec<_>>();
    // Spot does not detach in its own connect body; test the actual Futures/PM
    // detach adapter separately using a PM connection and a shared socket mock.
    owner.dispose().unwrap();
    drop(previous);
    drop(owner);
    assert!(
        state.upgrade().is_none(),
        "native private default callback cache has no receiver cycle after Base disposal"
    );
    let (owner, _, base, timers, runtime) = fixture(Venue::PM);
    run(&runtime, owner.connect_and_subscribe()).unwrap();
    let other = PrivateCallback::Message(Rc::new(|_| Ok(())));
    base.socket.add_listener("message", other.clone()).unwrap();
    let old = base
        .socket
        .listeners
        .borrow()
        .iter()
        .filter(|(_, f)| !f.same(&other))
        .map(|(name, fn_)| (*name, fn_.clone()))
        .collect::<Vec<_>>();
    run(&runtime, owner.connect_and_subscribe()).unwrap();
    assert_eq!(
        *base.socket.removed.borrow(),
        ["message", "close", "open"],
        "native detach removes own callbacks in source order"
    );
    assert_eq!(
        base.socket.closed.get(),
        1,
        "independent prior-socket close count"
    );
    assert!(
        base.socket
            .listeners
            .borrow()
            .iter()
            .any(|(_, f)| f.same(&other)),
        "native detach preserves unrelated message listener"
    );
    for (name, callback) in old {
        let current = base
            .socket
            .listeners
            .borrow()
            .iter()
            .find(|(n, f)| *n == name && !f.same(&other))
            .unwrap()
            .1
            .clone();
        assert!(
            callback.same(&current),
            "native private arrows remain stable across reconnect"
        );
    }
    assert!(
        timers.callbacks.borrow().len() >= 2,
        "native independent refresh chains started then cancelled by primitive"
    );
}
#[test]
fn private_exported_current_arrow_carries_receiver_and_base_lease() {
    let (owner, parent, base, _, runtime) = fixture(Venue::Spot);
    let state = Rc::downgrade(&**owner);
    let first = owner.capture_connect();
    let second = owner.capture_connect();
    assert!(
        first.same(&second),
        "native captured current connect arrow identity"
    );
    drop(second);
    drop(owner);
    assert!(
        state.upgrade().is_some(),
        "native captured callable retains receiver"
    );
    first.call().unwrap();
    assert_eq!(
        base.urls.borrow().len(),
        1,
        "native saved current arrow operates after class handle drop"
    );
    assert!(Rc::strong_count(&parent) >= 2);
    runtime.pump();
    base.dispose().unwrap();
    drop(first);
    assert!(
        state.upgrade().is_none(),
        "native final callable/base cleanup releases private state"
    );
}

#[test]
fn private_registered_open_propagates_synchronous_original_error() {
    for venue in [Venue::Futures, Venue::PM] {
        let (owner, _, base, _, runtime) = fixture(venue);
        run(&runtime, owner.connect_and_subscribe()).unwrap();
        let marker = Rc::new(ExchangeError::new("open ping marker"));
        *base.send_failure.borrow_mut() = Some(marker.clone());
        let callback = base
            .socket
            .listeners
            .borrow()
            .iter()
            .find(|(name, _)| *name == "open")
            .unwrap()
            .1
            .clone();
        let PrivateCallback::Action(callback) = callback else {
            panic!("open action required")
        };
        let result = callback();
        assert!(
            result.is_err(),
            "native synchronous open callback must propagate failure"
        );
        let actual = result.unwrap_err();
        assert!(
            Rc::ptr_eq(&actual, &marker),
            "native synchronous open callback preserves original error identity"
        );
        assert!(
            !owner.has_opened.get(),
            "native failed ping leaves hasOpened false"
        );
        owner.dispose().unwrap();
    }
}

#[test]
fn private_saved_message_callback_retains_base_lease() {
    let (owner, _, base, timers, runtime) = fixture(Venue::PM);
    run(&runtime, owner.connect_and_subscribe()).unwrap();
    let saved = base
        .socket
        .listeners
        .borrow()
        .iter()
        .find(|(name, _)| *name == "message")
        .unwrap()
        .1
        .clone();
    owner.dispose().unwrap();
    timers.callbacks.borrow_mut().clear();
    base.disposed.set(false);
    let weak = Rc::downgrade(&base);
    drop(owner);
    drop(base);
    assert!(
        weak.upgrade().is_some(),
        "native saved socket callback retains Base"
    );
    let PrivateCallback::Message(callback) = saved else {
        panic!("message callback required")
    };
    callback(string("bad frame")).unwrap();
    assert_eq!(
        weak.upgrade().unwrap().received.get(),
        1,
        "native retained callback still tracks frame"
    );
    drop(callback);
    assert!(
        weak.upgrade().is_none(),
        "native callback release frees Base after disposal"
    );
}

#[path = "support/class_listener_host.rs"]
mod exchange_listener_host;
use safe_cex_exchange::websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket};
use safe_cex_venue_binance::{exchange_classes::*, private_session::Factory};
impl WsRuntime for Runtime {
    fn transport(&self) -> std::sync::Arc<dyn safe_cex_transport::TransportProvider> {
        std::sync::Arc::new(safe_cex_transport::DirectTransport)
    }
    fn tunnel_host(&self) -> Rc<dyn safe_cex_transport::socket::TunnelHost> {
        panic!("fixture has no tunnel")
    }
    fn dial(
        &self,
        _: String,
        _: Vec<String>,
        _: WebSocketConnectionOptions,
        _: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        match self.dial_socket.borrow().clone() {
            Some(socket) => Box::pin(std::future::ready(Ok(socket))),
            None => Box::pin(std::future::pending()),
        }
    }
    fn headers_capable(&self) -> bool {
        false
    }
    fn enable_reconnecting(&self) -> bool {
        false
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        1000.0
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}
#[derive(Default)]
struct Imports {
    base: Rc<Base>,
    timers: Rc<Timers>,
    guards: RefCell<Vec<safe_cex_venue_binance::request_weight::Fulfilled<EventValue>>>,
    rejected_guards:
        RefCell<Vec<safe_cex_venue_binance::request_weight::Rejected<EventValue, Failure>>>,
    requests: RefCell<Vec<String>>,
    request_args: RefCell<Vec<Option<EventValue>>>,
    suspend_requests: Rc<Cell<bool>>,
    fail_private: Cell<bool>,
    failed_parent: RefCell<Option<std::rc::Weak<dyn PrivateParent>>>,
    header_mode: Cell<usize>,
    header_reads: Rc<RefCell<Vec<String>>>,
    header_failure: RefCell<Option<Failure>>,
    rejected_response: RefCell<Option<Result<Option<EventValue>, Failure>>>,
    routes: RefCell<std::collections::HashMap<&'static str, Value>>,
    loop_intervals: RefCell<Vec<f64>>,
}
fn client(label: &str) -> EventValue {
    record([("label", string(label))])
}
impl ExchangeImports for Imports {
    fn factory(
        &self,
        _: Venue,
        kind: Factory,
        opts: EventValue,
        owner: EventValue,
    ) -> Result<EventValue, Failure> {
        assert!(
            opts.same(&owner.read("options")?),
            "native factory receives canonical options graph"
        );
        Ok(client(match kind {
            Factory::API => "API",
            Factory::PAPI => "PAPI",
            Factory::FAPI => "FAPI",
            Factory::PublicAPI => "PublicAPI",
        }))
    }
    fn split(
        &self,
        venue: Venue,
        opts: EventValue,
        owner: EventValue,
        _: Rc<dyn Fn(EventValue) -> Result<EventValue, Failure>>,
        rate: Option<safe_cex_venue_binance::private_session::RateHook>,
    ) -> Result<EventValue, Failure> {
        assert!(
            opts.same(&owner.read("options")?),
            "native split receives canonical exchange options"
        );
        if let Some(rate) = rate {
            assert_eq!(rate(client("primary"))?.read("label")?.text()?, "primary");
        }
        Ok(if venue == Venue::Spot {
            client("route")
        } else {
            EventValue::array(vec![
                client("route"),
                client("primary"),
                client("secondary"),
            ])
        })
    }
    fn rate_limit(&self, client: EventValue, max: f64) -> Result<EventValue, Failure> {
        assert_eq!(max, 3.0, "native rate limit three requests per second");
        Ok(client)
    }
    fn request(
        &self,
        _: EventValue,
        _: HttpMethod,
        path: &'static str,
        args: Option<EventValue>,
    ) -> Result<safe_cex_venue_binance::private_rest::SnapshotRequestFuture, Failure> {
        self.requests.borrow_mut().push(path.into());
        self.request_args.borrow_mut().push(args);
        let suspended = self.suspend_requests.clone();
        let routed = self.routes.borrow().get(path).cloned();
        Ok(Box::pin(std::future::poll_fn(move |_| {
            if suspended.get() {
                return Poll::Pending;
            }
            Poll::Ready(safe_cex_venue_binance::private_rest::SnapshotCompletion {
                reaction_order: 0,
                result: Ok(routed.clone().map_or_else(
                    || EventValue::Array(Default::default()),
                    EventValue::from_json,
                )),
            })
        })))
    }
    fn register_weight_guard(
        &self,
        _: EventValue,
        good: safe_cex_venue_binance::request_weight::Fulfilled<EventValue>,
        bad: safe_cex_venue_binance::request_weight::Rejected<EventValue, Failure>,
    ) -> Result<(), Failure> {
        self.guards.borrow_mut().push(good);
        self.rejected_guards.borrow_mut().push(bad);
        Ok(())
    }
    fn weight_response_property(
        &self,
        value: &EventValue,
        key: &str,
    ) -> Result<EventValue, Failure> {
        if self.header_mode.get() == 4 && key == "status" {
            return Err(self.header_failure.borrow().as_ref().unwrap().clone());
        }
        value.read(key)
    }
    fn weight_header_get_member(
        &self,
        headers: &EventValue,
    ) -> Result<safe_cex_venue_binance::request_weight::HeaderGetMember, Failure> {
        use safe_cex_venue_binance::request_weight::HeaderGetMember;
        if self.header_mode.get() == 0 {
            return Ok(HeaderGetMember::NotCallable);
        }
        let mut reads = self.header_reads.borrow_mut();
        reads.push("get".into());
        if self.header_mode.get() == 2 && reads.len() == 2 {
            return Err(self.header_failure.borrow().as_ref().unwrap().clone());
        }
        if self.header_mode.get() == 3 && reads.len() == 2 {
            return Ok(HeaderGetMember::NotCallable);
        }
        let headers = headers.clone();
        let reads = self.header_reads.clone();
        Ok(HeaderGetMember::Callable(Rc::new(move |key| {
            reads.borrow_mut().push(format!("call:{key}"));
            assert!(
                matches!(headers.read("receiver")?, EventValue::String(v) if v.as_ref()=="native-header".encode_utf16().collect::<Vec<_>>()),
                "native imported header getter retains original receiver"
            );
            Ok(Some(json!(2.5)))
        })))
    }
    fn weight_error_response(&self, error: &Failure) -> Result<Option<EventValue>, Failure> {
        if let Some(response) = self.rejected_response.borrow().clone() {
            return response;
        }
        Ok(error.response.clone().map(EventValue::from_json))
    }
    fn external_provider(
        &self,
        _: &safe_cex_types::ExchangeOptionsExternalPublicData,
    ) -> Rc<dyn safe_cex_exchange::external::ExternalPublicData> {
        panic!("fixture has no external provider")
    }
    fn private_base(&self, parent: Rc<dyn PrivateParent>) -> Result<Rc<dyn PrivateBase>, Failure> {
        if self.fail_private.get() {
            *self.failed_parent.borrow_mut() = Some(Rc::downgrade(&parent));
            return Err(Rc::new(ExchangeError::new("private constructor control")));
        }
        Ok(self.base.clone())
    }
    fn listener_operations(
        &self,
    ) -> Rc<dyn safe_cex_venue_binance::public_listeners::ListenerHost> {
        exchange_listener_host::Host::new("owner")
    }
    fn timers(&self) -> Rc<dyn PrivateTimers> {
        self.timers.clone()
    }
    fn remote_sign(&self, _: EventValue) -> Result<RemoteSignCall, Failure> {
        panic!("fixture has no remote signer")
    }
    fn virtual_now(&self) -> f64 {
        21000.0
    }
    fn uuid(&self) -> String {
        "fixture".into()
    }
    fn dayjs(&self, v: Option<f64>) -> Result<f64, Failure> {
        Ok(v.unwrap_or(20000.0))
    }
    fn subtract(&self, _: f64, _: f64, _: char) -> Result<f64, Failure> {
        panic!("fixture does not subtract calendar time")
    }
    fn paginate(
        &self,
        _: safe_cex_venue_binance::rest_history::PageCall,
        _: EventValue,
    ) -> Result<NativeFuture<EventValue>, Failure> {
        panic!("fixture does not paginate")
    }
    fn iso(&self, _: EventValue) -> Result<String, Failure> {
        panic!("fixture does not format time")
    }
    fn fold_closed_positions(
        &self,
        _: EventValue,
        _: bool,
    ) -> Result<NativeFuture<EventValue>, Failure> {
        panic!("fixture does not fold")
    }
    fn loop_timer(
        &self,
        _: safe_cex_venue_binance::lifecycle::TickTimer,
        interval: EventValue,
    ) -> Result<(), Failure> {
        self.loop_intervals.borrow_mut().push(interval.number()?);
        Ok(())
    }
}
#[test]
fn exchange_class_quiescent_ownership_releases_after_last_lease() {
    use safe_cex_store::store::DefaultStore;
    for (venue, stem) in [
        (Venue::Futures, "binance"),
        (Venue::PM, "binancepm"),
        (Venue::Spot, "binancespot"),
    ] {
        for dispose in [false, true] {
            let imports = Rc::new(Imports::default());
            let owner = ExchangeOwner::new(
                venue,
                serde_json::from_value(json!(stem)).unwrap(),
                serde_json::from_value(json!({"key":"key", "secret":"secret"})).unwrap(),
                Rc::new(DefaultStore::new()),
                ExchangeDependencies {
                    runtime: Rc::new(Runtime::default()),
                    imports: imports.clone(),
                    signers: Default::default(),
                    remote_signer_token: None,
                    payload_signer_token: None,
                },
            )
            .unwrap();
            let state = Rc::downgrade(&owner.state);
            let socket = Rc::downgrade(owner.public.base());
            let public = Rc::downgrade(&owner.public);
            let private = Rc::downgrade(&owner.private);
            if dispose {
                owner.dispose().unwrap();
            }
            drop(owner);
            assert!(
                state.upgrade().is_none(),
                "native quiescent class state releases: {stem}, dispose={dispose}"
            );
            assert!(
                socket.upgrade().is_none(),
                "native quiescent public socket releases: {stem}, dispose={dispose}"
            );
            assert!(
                public.upgrade().is_none(),
                "native quiescent public owner releases: {stem}"
            );
            assert!(
                private.upgrade().is_none(),
                "native quiescent private owner releases: {stem}"
            );
            assert_eq!(
                imports.guards.borrow().len(),
                if venue == Venue::Futures { 4 } else { 0 },
                "native external imports remain alive after class release"
            );
            if venue == Venue::Futures {
                let guard = imports.guards.borrow()[0].clone();
                let error = guard(record([])).unwrap_err();
                assert_eq!(
                    error.message, "exchange owner released",
                    "native external interceptor alone does not lease class lifetime"
                );
            }
        }
    }
}

#[test]
fn exchange_owners_bind_real_children_clients_and_native_state_before_source() {
    use safe_cex_store::{Store, store::DefaultStore};
    let mut native = vec![];
    for (venue, stem) in [
        (Venue::Futures, "binance"),
        (Venue::PM, "binancepm"),
        (Venue::Spot, "binancespot"),
    ] {
        let runtime = Rc::new(Runtime {
            now: Cell::new(20000.0),
            ..Default::default()
        });
        let imports = Rc::new(Imports::default());
        let store = Rc::new(DefaultStore::new());
        let opts = serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap();
        let owner = ExchangeOwner::new(
            venue,
            serde_json::from_value(json!(stem)).unwrap(),
            opts,
            store.clone(),
            ExchangeDependencies {
                runtime: runtime.clone(),
                imports: imports.clone(),
                signers: Default::default(),
                remote_signer_token: None,
                payload_signer_token: None,
            },
        )
        .unwrap();
        let expected_parent: Rc<dyn safe_cex_exchange::websocket::WsParent> = owner.state.clone();
        assert!(
            Rc::ptr_eq(&owner.public.parent, &expected_parent),
            "native public child retains actual exchange receiver"
        );
        let private_parent: Rc<dyn PrivateParent> = owner.state.clone();
        assert!(
            Rc::ptr_eq(&owner.private.parent, &private_parent),
            "native private child retains actual exchange receiver"
        );
        assert!(
            !owner.public_only(),
            "native key and secret account can sign"
        );
        assert_eq!(
            owner.position_state_version(),
            0.0,
            "native constructor initializes position generation"
        );
        owner.mark_position_state_updated();
        assert_eq!(
            owner.position_state_version(),
            1.0,
            "native websocket version method shares constructor state"
        );
        assert!(
            !owner.closed_positions_share_ledger_source,
            "native Futures closed axis uses separate source"
        );
        let fields: &[(&str, &str)] = match venue {
            Venue::Futures => &[
                ("xhr", "route"),
                ("_primaryXhr", "primary"),
                ("_secondaryXhr", "secondary"),
                ("publicXhr", "PublicAPI"),
                ("unlimitedXHR", "API"),
            ],
            Venue::PM => &[
                ("papiXhr", "route"),
                ("_primaryXhr", "primary"),
                ("_secondaryXhr", "secondary"),
                ("fapiXhr", "FAPI"),
                ("unlimitedXHR", "PAPI"),
                ("publicXhr", "PublicAPI"),
            ],
            Venue::Spot => &[("xhr", "route"), ("unlimitedXHR", "API")],
        };
        for (field, label) in fields {
            assert_eq!(
                owner
                    .clients
                    .get(field)
                    .unwrap()
                    .read("label")
                    .unwrap()
                    .text()
                    .unwrap(),
                *label,
                "native exchange constructor client route {stem}.{field}"
            );
        }
        assert_eq!(
            imports.guards.borrow().len(),
            if venue == Venue::Futures { 4 } else { 0 },
            "independent native interceptor registration floor"
        );
        if venue == Venue::Futures {
            let empty = record([]);
            let fulfilled = imports.guards.borrow()[0](empty.clone())
                .expect("native missing status fulfills unchanged without serialization");
            assert!(
                fulfilled.same(&empty),
                "native fulfilled response keeps original identity"
            );
            let mut original = ExchangeError::new("original guard rejection");
            original.response = Some(json!({}));
            let original = Rc::new(original);
            let rejected = imports.rejected_guards.borrow()[0](original.clone())
                .expect("native missing rejection status returns original rejection future");
            let caught = run(&runtime, rejected).unwrap_err();
            assert!(
                Rc::ptr_eq(&caught, &original),
                "native missing rejection status preserves original error Rc"
            );
            imports.guards.borrow()[0](record([
                ("status", string("429")),
                ("headers", record([("RETRY-AFTER", number(2.5))])),
            ]))
            .unwrap();
            assert_eq!(
                owner.paused_until.get(),
                0.0,
                "native string 429 does not select numeric retry branch"
            );
            imports.guards.borrow()[0](record([(
                "status",
                EventValue::Builtin(
                    safe_cex_num::PrototypeRealm::current()
                        .reference(safe_cex_num::ProtoMember::Constructor),
                ),
            )]))
            .expect("native callable status is compared without serialization");
            assert_eq!(
                owner.paused_until.get(),
                0.0,
                "native callable status leaves reconciliation pause unchanged"
            );
            imports.guards.borrow()[0](record([
                ("status", number(429.0)),
                ("headers", record([("RETRY-AFTER", number(2.5))])),
            ]))
            .unwrap();
            assert_eq!(
                owner.paused_until.get(),
                22500.0,
                "native uppercase header pauses reconciliation 2500ms"
            );
            imports.guards.borrow()[0](record([
                ("status", string("429")),
                (
                    "headers",
                    record([("x-mbx-used-weight-1m", number(1920.0))]),
                ),
            ]))
            .unwrap();
            assert_eq!(
                owner.paused_until.get(),
                80000.0,
                "native nonnumeric status still observes used weight 1920"
            );
        }
        let max = if venue == Venue::PM { 32 } else { 36 };
        let rule = client_rule(venue);
        assert!(
            (rule.is_valid)(&"x".repeat(max)),
            "native maximum legal client ID length"
        );
        assert!(
            !(rule.is_valid)(&"x".repeat(max + 1)),
            "native too-long client ID rejected"
        );
        let before = owner.base.options().key;
        let mut balance = store.balance().clone();
        balance.total = f64::INFINITY;
        balance.free = f64::NAN;
        balance.used = -0.0;
        let mut nested = balance.clone();
        nested.total = f64::NEG_INFINITY;
        balance.currencies = Some([("witness".into(), nested.into())].into_iter().collect());
        Store::update(
            store.as_ref(),
            safe_cex_store::StoreUpdate {
                balance: Some(balance),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        assert!(
            store.balance().total.is_infinite(),
            "native Store keeps Infinity before owner projection"
        );
        let snapshot = PrivateParent::balance(owner.state.as_ref()).unwrap();
        assert!(
            matches!(snapshot.read("total").unwrap(), EventValue::Number(v) if v==f64::INFINITY),
            "native owner Store projection retains positive Infinity"
        );
        assert!(
            matches!(snapshot.read("free").unwrap(),EventValue::Number(v) if v.is_nan()),
            "native Store balance projection retains NaN"
        );
        assert!(
            matches!(snapshot.read("used").unwrap(),EventValue::Number(v) if v==0.0 && v.is_sign_negative()),
            "native Store balance projection retains negative zero"
        );
        let nested = snapshot
            .read("currencies")
            .unwrap()
            .read("witness")
            .unwrap();
        assert!(
            matches!(nested.read("total").unwrap(),EventValue::Number(v) if v==f64::NEG_INFINITY),
            "native nested Store balance retains negative Infinity"
        );
        assert!(
            matches!(nested.read("free").unwrap(),EventValue::Number(v) if v.is_nan()),
            "native nested Store balance retains NaN"
        );
        assert!(
            matches!(nested.read("used").unwrap(),EventValue::Number(v) if v==0.0 && v.is_sign_negative()),
            "native nested Store balance retains negative zero"
        );
        let mut changed = store.balance().clone();
        changed.total = 4.2;
        Store::update(
            store.as_ref(),
            safe_cex_store::StoreUpdate {
                balance: Some(changed),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        assert!(
            matches!(snapshot.read("total").unwrap(), EventValue::Number(v) if v==f64::INFINITY),
            "native previously read balance retains earlier positive Infinity after Store mutation"
        );
        // Native assertions above precede all source/output projection.
        native.push(json!({"stem":stem,"clients":fields.iter().map(|(k,v)|(k.to_string(),json!(v))).collect::<serde_json::Map<_,_>>(),"guards":imports.guards.borrow().len(),"limit":true,"tooLong":false,"sameParent":true,"optionsBefore":before}));
        let leaked = Rc::downgrade(&owner.state);
        drop(expected_parent);
        drop(private_parent);
        owner.dispose().unwrap();
        drop(owner);
        assert!(
            leaked.upgrade().is_none(),
            "native owner graph releases after explicit dispose"
        );
    }
    assert_eq!(
        native.len(),
        3,
        "independent native exchange constructor floor stops at case2"
    );
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-classes.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "actual exchange source constructor assertions: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let mut source: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        source.len(),
        3,
        "independent source constructor floor stops at case2"
    );
    for row in &mut source {
        let object = row.as_object_mut().unwrap();
        assert_eq!(
            object.remove("optionsAfter").unwrap(),
            "changed",
            "CHOSEN source retained caller options alias changes"
        );
        assert_eq!(
            object.remove("balanceAfter").unwrap(),
            4.2,
            "CHOSEN source retained balance alias changes"
        );
    }
    assert_eq!(
        native, source,
        "three actual class constructors after native state assertions"
    );
}

#[test]
fn private_escaped_surrogate_order_callback_loss_is_inventoried() {
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let (owner, _, base, _, _) = fixture(venue);
        let ordinary = if venue == Venue::Spot {
            r#"{"e":"executionReport","X":"ignored","x":"ok"}"#
        } else {
            r#"{"e":"ORDER_TRADE_UPDATE","o":{"X":"ignored"},"x":"ok"}"#
        };
        owner.on_message(string(ordinary)).unwrap();
        assert_eq!(
            *base.stats.borrow(),
            ["order"],
            "native ordinary frame reaches private order dispatch"
        );
        base.stats.borrow_mut().clear();
        let escaped = ordinary.replace("\"ok\"", r#""\ud800""#);
        owner.on_message(string(escaped)).unwrap();
        assert!(
            base.stats.borrow().is_empty(),
            "FORCED native escaped surrogate suppresses private order callback"
        );
    }
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/class-boundary-findings.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "source private boundary assertions: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        source["private"].as_array().unwrap().len(),
        3,
        "independent source private boundary floor through case2"
    );
    for row in source["private"].as_array().unwrap() {
        assert_eq!(row["callbacks"], 1, "FORCED source private callback runs");
        assert_eq!(
            row["unit"], 55296,
            "FORCED source private callback observes lone code unit"
        );
    }
}

#[test]
fn private_bare_dropped_connect_reports_raw_key_coercion_failure() {
    let (owner, parent, _, _, runtime) = fixture(Venue::Futures);
    *parent.key_value.borrow_mut() = Some(record([
        ("toString", number(7.0)),
        ("valueOf", number(8.0)),
    ]));
    drop(owner.connect_and_subscribe());
    for _ in 0..4 {
        runtime.pump();
    }
    assert!(
        !owner.connect.connecting.get(),
        "native bare failed connect still clears connecting finally"
    );
    assert_eq!(
        *runtime.reported.borrow(),
        ["TypeError"],
        "native bare dropped connect reports original coercion rejection exactly once"
    );
    owner.dispose().unwrap();
}

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn saved_spot_open_builds_signer_request_in_its_original_realm_before_source() {
    saved_spot_open_case(false);
}
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn saved_spot_open_preserves_native_prototype_signature_before_source() {
    saved_spot_open_case(true);
}
fn saved_spot_open_case(prototype_signature: bool) {
    use safe_cex_num::{ProtoMember, PrototypeRealm, PrototypeScalar};
    let original = Rc::<PrototypeRealm>::default();
    let _original_binding = original.bind();
    original
        .reference(ProtoMember::Prototype)
        .set("lastMessageTimestamp", PrototypeScalar::Number(1000.0));
    let (owner, parent, base, _, runtime) = fixture(Venue::Spot);
    let requests: Rc<RefCell<Vec<EventValue>>> = Default::default();
    let observed = requests.clone();
    let signature = if prototype_signature {
        let reference = original.reference(ProtoMember::Prototype);
        assert!(
            matches!(
                reference.get("lastMessageTimestamp"),
                safe_cex_num::PrototypeValue::Number(1000.0)
            ),
            "native original prototype signature owns inherited timestamp before encoding"
        );
        EventValue::Builtin(reference)
    } else {
        string("fixture-signature")
    };
    *parent.remote_sign_call.borrow_mut() = Some(Rc::new(move |request| {
        observed.borrow_mut().push(request);
        let response = record([("wsAuthSignature", signature.clone())]);
        Ok(Box::pin(async move { Ok(response) }))
    }));
    parent
        .options
        .write("remoteSigner", record([("accountName", string("fixture"))]));
    run(&runtime, owner.connect_and_subscribe()).unwrap();
    assert_eq!(
        requests.borrow().len(),
        0,
        "native connect does not sign before open"
    );
    let callbacks: Vec<_> = base
        .socket
        .listeners
        .borrow()
        .iter()
        .filter(|(name, _)| *name == "open")
        .map(|(_, callback)| callback.clone())
        .collect();
    assert_eq!(
        callbacks.len(),
        1,
        "native independent saved open listener count"
    );
    let PrivateCallback::Action(saved) = &callbacks[0] else {
        panic!("native saved open is an action")
    };
    let caller = Rc::<PrototypeRealm>::default();
    let _caller_binding = caller.bind();
    caller
        .reference(ProtoMember::Prototype)
        .set("lastMessageTimestamp", PrototypeScalar::Number(2000.0));
    saved().unwrap();
    assert_eq!(
        requests.borrow().len(),
        1,
        "native saved open invokes one signer"
    );
    let request = requests.borrow()[0].clone();
    let EventValue::Number(request_timestamp) = request.read("lastMessageTimestamp").unwrap()
    else {
        panic!("native signer request inherited timestamp is numeric")
    };
    assert_eq!(
        request_timestamp, 1000.0,
        "native saved Spot open constructs signer request in original realm"
    );
    assert!(
        matches!(request.read("type").unwrap(),EventValue::String(value) if value.as_ref()=="ws-auth".encode_utf16().collect::<Vec<_>>()),
        "native signer request type before source"
    );
    assert!(
        Rc::ptr_eq(&PrototypeRealm::current(), &caller),
        "native saved open restores caller realm after synchronous prefix"
    );
    for _ in 0..4 {
        runtime.pump();
    }
    assert!(
        Rc::ptr_eq(&PrototypeRealm::current(), &caller),
        "native saved open restores caller realm after queued completion"
    );
    let EventValue::Number(caller_timestamp) = EventValue::record_object(Default::default())
        .read("lastMessageTimestamp")
        .unwrap()
    else {
        panic!("native caller timestamp remains numeric")
    };
    assert_eq!(
        caller_timestamp, 2000.0,
        "native callback preserves distinct caller realm data"
    );
    assert_eq!(
        base.frames.borrow().len(),
        1,
        "native saved open sends one subscription"
    );
    assert_eq!(
        base.errors.borrow().len(),
        0,
        "native saved open has no signing error"
    );
    assert!(!base.forced.get(), "native saved open does not force close");
    assert!(
        runtime.reported.borrow().is_empty(),
        "native saved open produces no unobserved rejection"
    );
    let frame: Value = serde_json::from_str(&base.frames.borrow()[0]).unwrap();
    assert_eq!(
        frame["method"], "userDataStream.subscribe.signature",
        "native decoded saved-open subscription method"
    );
    assert_eq!(
        frame["params"]["signature"],
        if prototype_signature {
            json!({"lastMessageTimestamp":1000})
        } else {
            json!("fixture-signature")
        },
        "native decoded saved-open signature value"
    );
    let actual = json!({"requestTimestamp":request_timestamp,"currentRealmTimestamp":caller_timestamp,"sentCount":base.frames.borrow().len(),"requestType":request.read("type").unwrap().text().unwrap(),"method":frame["method"],"signature":frame["params"]["signature"],"errorCount":base.errors.borrow().len(),"forceClosed":base.forced.get()});
    let mut command = std::process::Command::new("node");
    command.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/private-realm.mjs"
    ));
    if prototype_signature {
        command.arg("--prototype-signature");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["requestTimestamp", "currentRealmTimestamp"] {
        expected[key] = json!(expected[key].as_f64().unwrap());
    }
    assert_eq!(
        actual, expected,
        "actual source saved Spot callback realm and subscription values"
    );
}

fn class_owner(venue: Venue, imports: Rc<Imports>, runtime: Rc<Runtime>) -> Rc<ExchangeOwner> {
    let stem = match venue {
        Venue::Futures => "binance",
        Venue::PM => "binancepm",
        Venue::Spot => "binancespot",
    };
    ExchangeOwner::new(
        venue,
        serde_json::from_value(json!(stem)).unwrap(),
        serde_json::from_value(json!({"key":"key", "secret":"secret"})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        ExchangeDependencies {
            runtime,
            imports,
            signers: Default::default(),
            remote_signer_token: None,
            payload_signer_token: None,
        },
    )
    .unwrap()
}
#[test]
fn exchange_class_ownership_keeps_child_and_running_operation_leases() {
    use safe_cex_exchange::socket_lifecycle::SocketSlot;
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let imports = Rc::new(Imports::default());
        let runtime = Rc::new(Runtime::default());
        let owner = class_owner(venue, imports.clone(), runtime.clone());
        let state = Rc::downgrade(&owner.state);
        let child = owner.public.clone();
        let private_sibling = Rc::downgrade(&owner.private);
        drop(owner);
        assert!(
            private_sibling.upgrade().is_none(),
            "native retained child does not lease root-owned private sibling: {venue:?}"
        );
        assert!(
            state.upgrade().is_some(),
            "native retained child keeps exchange parent alive: {venue:?}"
        );
        assert!(
            Rc::ptr_eq(
                &state
                    .upgrade()
                    .unwrap()
                    .base
                    .get_socket(SocketSlot::Public)
                    .unwrap(),
                child.base()
            ),
            "native retained child preserves canonical socket registry: {venue:?}"
        );
        drop(child);
        assert!(
            state.upgrade().is_none(),
            "native last child release frees parent: {venue:?}"
        );
        let owner = class_owner(venue, imports.clone(), runtime.clone());
        let state = Rc::downgrade(&owner.state);
        imports.suspend_requests.set(true);
        let operation = owner.invoke(Method::Orders, vec![]);
        drop(owner);
        assert!(
            state.upgrade().is_some(),
            "native running operation leases complete owner: {venue:?}"
        );
        imports.suspend_requests.set(false);
        run(&runtime, operation).unwrap();
        runtime.pump();
        assert!(
            state.upgrade().is_none(),
            "native settled operation releases receiver: {venue:?}"
        );
        let owner = class_owner(venue, imports.clone(), runtime.clone());
        let state = owner.state.clone();
        let public = Rc::downgrade(&owner.public);
        drop(owner);
        assert!(
            public.upgrade().is_none(),
            "native isolated state view does not lease child owner: {venue:?}"
        );
        assert_eq!(
            state.base.get_all_sockets().len(),
            0,
            "native isolated state registry contains zero live socket leases: {venue:?}"
        );
        drop(state);
        imports.fail_private.set(true);
        let result = ExchangeOwner::new(
            venue,
            serde_json::from_value(json!(match venue {
                Venue::Futures => "binance",
                Venue::PM => "binancepm",
                Venue::Spot => "binancespot",
            }))
            .unwrap(),
            serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            ExchangeDependencies {
                runtime: runtime.clone(),
                imports: imports.clone(),
                signers: Default::default(),
                remote_signer_token: None,
                payload_signer_token: None,
            },
        );
        assert!(
            matches!(result, Err(e) if e.message == "private constructor control"),
            "native child construction failure propagates"
        );
        assert!(
            imports
                .failed_parent
                .borrow()
                .as_ref()
                .unwrap()
                .upgrade()
                .is_none(),
            "native failed constructor releases partial parent: {venue:?}"
        );
    }
}
#[test]
fn exchange_class_weight_guards_bind_native_imported_accessors() {
    let runtime = Rc::new(Runtime {
        now: Cell::new(20000.0),
        ..Default::default()
    });
    let imports = Rc::new(Imports::default());
    let owner = class_owner(Venue::Futures, imports.clone(), runtime.clone());
    let good = imports.guards.borrow()[0].clone();
    let bad = imports.rejected_guards.borrow()[0].clone();
    let response = record([
        ("status", number(429.0)),
        (
            "headers",
            record([
                ("receiver", string("native-header")),
                ("retry-after", number(9.0)),
            ]),
        ),
    ]);
    imports.header_mode.set(1);
    assert!(
        good(response.clone()).unwrap().same(&response),
        "native callable header response keeps identity"
    );
    assert_eq!(
        owner.paused_until.get(),
        22500.0,
        "native imported header getter selects 2.5 rather than own 9"
    );
    assert_eq!(
        *imports.header_reads.borrow(),
        ["get", "get", "call:retry-after"],
        "native imported header member is read twice then called once"
    );
    let original = Rc::new(ExchangeError::new("original rejected identity"));
    *imports.rejected_response.borrow_mut() = Some(Ok(Some(response.clone())));
    imports.header_reads.borrow_mut().clear();
    let rejection = bad(original.clone()).unwrap();
    assert!(
        Rc::ptr_eq(&run(&runtime, rejection).unwrap_err(), &original),
        "native rejected accessor preserves original rejection Rc"
    );
    assert_eq!(
        *imports.header_reads.borrow(),
        ["get", "get", "call:retry-after"],
        "native rejected response uses live callable headers"
    );
    let getter_error = Rc::new(ExchangeError::new("native getter failure identity"));
    *imports.header_failure.borrow_mut() = Some(getter_error.clone());
    imports.header_mode.set(2);
    imports.header_reads.borrow_mut().clear();
    assert!(
        Rc::ptr_eq(&good(response.clone()).unwrap_err(), &getter_error),
        "native second get-member failure retains original Rc"
    );
    assert_eq!(
        *imports.header_reads.borrow(),
        ["get", "get"],
        "native throwing second member prevents invocation"
    );
    imports.header_mode.set(3);
    imports.header_reads.borrow_mut().clear();
    let flipped = good(response.clone()).unwrap_err();
    assert_eq!(
        flipped.name, "TypeError",
        "native second member became noncallable TypeError"
    );
    assert_eq!(
        flipped.message, "headers.get is not a function",
        "native noncallable member source message"
    );
    imports.header_mode.set(4);
    assert!(
        Rc::ptr_eq(&good(response.clone()).unwrap_err(), &getter_error),
        "native status getter failure escapes before header lookup"
    );
    *imports.rejected_response.borrow_mut() = Some(Err(getter_error.clone()));
    assert!(
        Rc::ptr_eq(&bad(original.clone()).err().unwrap(), &getter_error),
        "native rejected response getter failure escapes synchronously"
    );
    // Native state and identities above precede source observations below.
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/exchange-class-weight.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "source native imported-accessor corpus succeeds: {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        source,
        json!({"pause":22500,"reads":["get","get","call:retry-after"],"rejectedIdentity":true,"getterIdentity":true,"flip":"TypeError:headers.get is not a function","statusIdentity":true,"responseIdentity":true,"retainedGuardAfterOwnerBindingCleared":true,"cases":7}),
        "source imported accessor witnesses agree after native assertions"
    );
}

fn sdk_class(
    venue: Venue,
    owner: Rc<ExchangeOwner>,
) -> Rc<dyn safe_cex_exchange::exchange::Exchange> {
    use safe_cex_venue_binance::exchange_classes::{
        BinanceExchange, BinancePMExchange, BinanceSpotExchange,
    };
    match venue {
        Venue::Futures => Rc::new(BinanceExchange(owner)),
        Venue::PM => Rc::new(BinancePMExchange(owner)),
        Venue::Spot => Rc::new(BinanceSpotExchange(owner)),
    }
}
#[test]
fn exchange_class_sdk_dispatch_preserves_routes_defaults_and_capability_brands() {
    use safe_cex_exchange::capabilities::{
        BaseCapabilityMethod as B, is_base_capability_fallback as fallback,
    };
    use safe_cex_store::StoreUpdate;
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let imports = Rc::new(Imports::default());
        let runtime = Rc::new(Runtime::default());
        let owner = class_owner(venue, imports.clone(), runtime.clone());
        let exchange = sdk_class(venue, owner.clone());
        assert!(
            !fallback(&exchange.capabilities().start, B::Start),
            "native SDK start capability is bound: {venue:?}"
        );
        assert!(
            !fallback(&exchange.capabilities().place_order, B::PlaceOrder),
            "native SDK single placement capability is bound: {venue:?}"
        );
        assert!(
            !fallback(&exchange.capabilities().place_orders, B::PlaceOrders),
            "native SDK batch placement capability is bound: {venue:?}"
        );
        assert_eq!(
            fallback(
                &exchange.capabilities().fetch_historical_fills,
                B::FetchHistoricalFills
            ),
            venue != Venue::Futures,
            "native SDK fills brand matches actual source override: {venue:?}"
        );
        assert_eq!(
            fallback(
                &exchange.capabilities().fetch_historical_orders,
                B::FetchHistoricalOrders
            ),
            venue != Venue::Futures,
            "native SDK order history brand matches actual source override: {venue:?}"
        );
        assert_eq!(
            fallback(
                &exchange.capabilities().fetch_income_history,
                B::FetchIncomeHistory
            ),
            venue != Venue::Futures,
            "native SDK income brand matches actual source override: {venue:?}"
        );
        assert_eq!(
            fallback(
                &exchange.capabilities().fetch_income_history_range,
                B::FetchIncomeHistoryRange
            ),
            venue != Venue::Futures,
            "native SDK income range brand matches actual source override: {venue:?}"
        );
        assert!(
            fallback(&exchange.capabilities().fetch_closed_pnl, B::FetchClosedPnl),
            "native SDK deriveClosedPositions is separate from inherited closed PnL: {venue:?}"
        );
        let orders = run(
            &runtime,
            exchange.fetch_orders(Some(vec!["source ignores this list".into()])),
        )
        .expect("native SDK fetchOrders reaches concrete body");
        assert_eq!(
            orders.len(),
            0,
            "native empty authored order response returns zero orders: {venue:?}"
        );
        let expected: &[&str] = match venue {
            Venue::Futures => &["/fapi/v1/openOrders", "/fapi/v1/openAlgoOrders"],
            Venue::PM => &[
                "/papi/v1/um/openOrders",
                "/papi/v1/um/conditional/openOrders",
            ],
            Venue::Spot => &["/api/v3/openOrders"],
        };
        assert_eq!(
            *imports.requests.borrow(),
            expected,
            "native SDK fetchOrders request family: {venue:?}"
        );
        imports.requests.borrow_mut().clear();
        imports.request_args.borrow_mut().clear();
        let market = serde_json::from_value(json!({"id":"BTC/USDT:USDT", "symbol":"BTCUSDT", "base":"BTC", "quote":"USDT", "active":true, "precision":{"amount":0.1,"price":0.01},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":20}}})).unwrap();
        let position = serde_json::from_value(json!({"id":"BTCUSDT:long", "symbol":"BTCUSDT", "side":"long", "entryPrice":10,"notional":10,"leverage":2,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0})).unwrap();
        owner
            .base
            .store
            .update(
                StoreUpdate {
                    markets: Some(vec![market]),
                    positions: Some(vec![position]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        if venue == Venue::Spot {
            let error = run(&runtime, exchange.get_account()).unwrap_err();
            assert_eq!(
                error.message, "Not implemented",
                "native Spot retains inherited getAccount"
            );
            let error = run(
                &runtime,
                exchange.set_leverage("BTCUSDT".into(), 7.25, None),
            )
            .unwrap_err();
            assert_eq!(
                error.message, "Not implemented",
                "native Spot retains inherited setLeverage"
            );
            assert_eq!(
                run(&runtime, exchange.fetch_historical_orders(None))
                    .unwrap()
                    .len(),
                0,
                "native Spot inherits empty order history"
            );
            assert_eq!(
                imports.requests.borrow().len(),
                0,
                "native Spot unsupported methods send zero Futures requests"
            );
        } else {
            run(&runtime, exchange.set_all_leverage(7.25))
                .expect("native inherited setAllLeverage dispatches concrete setter");
            assert_eq!(
                *imports.requests.borrow(),
                [if venue == Venue::Futures {
                    "/fapi/v1/leverage"
                } else {
                    "/papi/v1/um/leverage"
                }],
                "native SDK inherited leverage request route: {venue:?}"
            );
            let body = imports.request_args.borrow()[0].clone().unwrap();
            assert!(
                matches!(body.read("symbol").unwrap(), EventValue::String(v) if v.as_ref()=="BTCUSDT".encode_utf16().collect::<Vec<_>>()),
                "native SDK leverage request symbol"
            );
            assert!(
                matches!(body.read("leverage").unwrap(), EventValue::Number(v) if v.to_bits() == 7.25_f64.to_bits()),
                "native SDK leverage request retains exact native 7.25"
            );
            assert_eq!(
                owner.base.store.positions()[0].leverage,
                7.25,
                "native SDK leverage updates canonical Store"
            );
        }
        imports.requests.borrow_mut().clear();
        if venue == Venue::Futures {
            assert_eq!(
                run(&runtime, exchange.fetch_historical_orders(None))
                    .unwrap()
                    .len(),
                0,
                "native missing history options return zero orders"
            );
            assert_eq!(
                imports.requests.borrow().len(),
                0,
                "native missing history symbol sends zero requests"
            );
            let opts = serde_json::from_value(json!({"symbol":"BTCUSDT", "limit":37})).unwrap();
            assert_eq!(
                run(&runtime, exchange.fetch_historical_orders(Some(opts)))
                    .unwrap()
                    .len(),
                0,
                "native authored empty history response"
            );
            assert_eq!(
                *imports.requests.borrow(),
                ["/fapi/v1/allOrders"],
                "native present history options dispatch to source endpoint"
            );
            let request = imports
                .request_args
                .borrow()
                .last()
                .cloned()
                .unwrap()
                .unwrap();
            assert!(
                matches!(request.read("limit").unwrap(), EventValue::Number(v) if v == 37.0),
                "native optional history limit retains 37"
            );
            imports.requests.borrow_mut().clear();
        }
        // Source start reads markets before its disposed check; the real body must run.
        exchange.dispose().unwrap();
        run(&runtime, exchange.base().start(exchange.as_ref()))
            .expect("native SDK branded start reaches concrete disposed body");
        assert_eq!(
            imports.requests.borrow().len(),
            1,
            "native SDK disposed startup performs one source-ordered read: {venue:?}"
        );
    }
}
struct ThrowingExternal {
    error: ExchangeError,
    setup_fails: Cell<bool>,
}
impl safe_cex_exchange::external::ExternalPublicData for ThrowingExternal {
    fn connect(&self) {}
    fn dispose(&self) {}
    fn is_initialized(&self) -> bool {
        true
    }
    fn is_healthy(&self) -> bool {
        true
    }
    fn last_message_timestamp(&self) -> Option<f64> {
        None
    }
    fn listen_ohlcv(
        &self,
        _: safe_cex_types::OHLCVOptions,
        _: Rc<dyn Fn(safe_cex_types::Candle)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        if self.setup_fails.get() {
            return Err(self.error.clone());
        }
        let error = self.error.clone();
        Ok(Box::new(move || Err(error)))
    }
    fn listen_order_book(
        &self,
        _: String,
        _: Rc<dyn Fn(safe_cex_types::OrderBook)>,
    ) -> Result<safe_cex_exchange::runtime::ListenerCleanup, ExchangeError> {
        if self.setup_fails.get() {
            return Err(self.error.clone());
        }
        let error = self.error.clone();
        Ok(Box::new(move || Err(error)))
    }
}
#[test]
fn exchange_class_sdk_external_listener_setup_and_cleanup_keep_errors() {
    let runtime = Rc::new(Runtime::default());
    let owner = class_owner(Venue::Futures, Rc::new(Imports::default()), runtime);
    let exchange = sdk_class(Venue::Futures, owner.clone());
    let mut error = ExchangeError::new("authored external listener error");
    error.code = Some(safe_cex_exchange::js_runtime::value::from_json(&json!(731)));
    let provider = Rc::new(ThrowingExternal {
        error: error.clone(),
        setup_fails: Cell::new(false),
    });
    *owner.base.external_public_data_provider.borrow_mut() = Some(provider.clone());
    let book_cleanup = exchange
        .listen_order_book("BTCUSDT".into(), Rc::new(|_| {}))
        .unwrap();
    assert_eq!(
        book_cleanup().expect_err("native SDK depth cleanup returns provider error"),
        error,
        "native SDK depth cleanup preserves original provider error metadata"
    );
    let opts = || serde_json::from_value(json!({"symbol":"BTCUSDT", "interval":"1m"})).unwrap();
    let candle_cleanup = exchange.listen_ohlcv(opts(), Rc::new(|_| {})).unwrap();
    assert_eq!(
        candle_cleanup().expect_err("native SDK candle cleanup returns provider error"),
        error,
        "native SDK candle cleanup preserves original provider error metadata"
    );
    provider.setup_fails.set(true);
    assert_eq!(
        exchange
            .listen_order_book("BTCUSDT".into(), Rc::new(|_| {}))
            .err()
            .unwrap(),
        error,
        "native SDK depth setup preserves original provider error metadata"
    );
    assert_eq!(
        exchange
            .listen_ohlcv(opts(), Rc::new(|_| {}))
            .err()
            .unwrap(),
        error,
        "native SDK candle setup preserves original provider error metadata"
    );
}
#[test]
fn exchange_class_sdk_worker_confirmation_keeps_store_notification_errors() {
    use safe_cex_exchange::worker::CallableMethodsInWorker;
    let owner = class_owner(
        Venue::Futures,
        Rc::new(Imports::default()),
        Rc::new(Runtime::default()),
    );
    let exchange = sdk_class(Venue::Futures, owner.clone());
    let _subscription = owner.base.store.subscribe(Box::new(move |_, _| {
        Err(safe_cex_types::StoreSubscriberError::new(
            "authored mode notification cause",
        ))
    }));
    let result = exchange.base().after_method_called_in_worker(
        exchange.as_ref(),
        &Value::Null,
        CallableMethodsInWorker::ChangePositionMode,
        &[json!(true)],
    );
    let error =
        result.expect_err("native SDK worker confirmation propagates Store notification rejection");
    assert!(
        owner.base.store.options().is_hedged,
        "native SDK worker mode setting commits before subscriber error"
    );
    assert!(
        !error.store_subscriber_ids.is_empty(),
        "native SDK worker confirmation preserves subscriber metadata"
    );
}

#[test]
fn exchange_class_actual_constructors_bind_watchdog_across_retained_child_lifetime() {
    use safe_cex_exchange::exchange::Exchange;
    use safe_cex_venue_binance::exchange_classes::{
        BinanceExchange, BinancePMExchange, BinanceSpotExchange,
    };
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        for retained_child in [false, true] {
            let runtime = Rc::new(Runtime {
                now: Cell::new(1000.0),
                ..Default::default()
            });
            runtime.sleep_clock.set(1000.0);
            let deps = ExchangeDependencies {
                runtime: runtime.clone(),
                imports: Rc::new(Imports::default()),
                signers: Default::default(),
                remote_signer_token: None,
                payload_signer_token: None,
            };
            let stem = match venue {
                Venue::Futures => "binance",
                Venue::PM => "binancepm",
                Venue::Spot => "binancespot",
            };
            let name = serde_json::from_value(json!(stem)).unwrap();
            let options = serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap();
            let store = Rc::new(safe_cex_store::store::DefaultStore::new());
            let (exchange, owner): (Rc<dyn Exchange>, Rc<ExchangeOwner>) = match venue {
                Venue::Futures => {
                    let v = Rc::new(BinanceExchange::new(name, options, store, deps).unwrap());
                    let owner = v.0.clone();
                    (v, owner)
                }
                Venue::PM => {
                    let v = Rc::new(BinancePMExchange::new(name, options, store, deps).unwrap());
                    let owner = v.0.clone();
                    (v, owner)
                }
                Venue::Spot => {
                    let v = Rc::new(BinanceSpotExchange::new(name, options, store, deps).unwrap());
                    let owner = v.0.clone();
                    (v, owner)
                }
            };
            let state = Rc::downgrade(&owner.state);
            let child = owner.public.clone();
            let root = if retained_child {
                drop(exchange);
                drop(owner);
                None
            } else {
                Some((exchange, owner))
            };
            child.parent.sync_socket_health_to_store().unwrap();
            assert_eq!(
                runtime.jobs.borrow().len(),
                1,
                "native constructor health sync schedules one watchdog: {venue:?}, retained_child={retained_child}"
            );
            assert_eq!(
                *runtime.sleeps.borrow(),
                [10_000],
                "native constructor watchdog awaits authored 10000ms"
            );
            state
                .upgrade()
                .unwrap()
                .base
                .store
                .update(
                    safe_cex_store::StoreUpdate {
                        socket_health: Some(json!({"publicHealthy":false,"privateHealthy":false})),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            runtime.now.set(11000.0);
            runtime.sleep_clock.set(11000.0);
            runtime.pump();
            assert_eq!(
                state.upgrade().unwrap().base.store.socket_health().unwrap()["publicHealthy"],
                json!(true),
                "native watchdog performs real canonical health publication"
            );
            assert_eq!(
                runtime.jobs.borrow().len(),
                1,
                "native watchdog settles and schedules exactly one successor"
            );
            assert_eq!(
                *runtime.sleeps.borrow(),
                [10_000, 10_000],
                "native watchdog successor retains source interval"
            );
            state.upgrade().unwrap().base.dispose().unwrap();
            runtime.now.set(21000.0);
            runtime.sleep_clock.set(21000.0);
            runtime.pump();
            assert_eq!(
                runtime.jobs.borrow().len(),
                0,
                "native disposed class schedules no watchdog successor"
            );
            drop(root);
            drop(child);
            assert!(
                state.upgrade().is_none(),
                "native watchdog never leases released class state"
            );
        }
    }
}

fn sixteen_method_routes(imports: &Imports) {
    let corpus: Value = serde_json::from_str(include_str!("fixtures/public-rest.json")).unwrap();
    let case = |family: &str| {
        corpus
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["family"] == family && c["name"] == "normal")
            .unwrap()["payloads"]
            .clone()
    };
    let (markets, tickers) = (case("markets"), case("tickers"));
    let position = json!({"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"1","notional":"10","entryPrice":"10","markPrice":"10","unRealizedProfit":"0.5","liquidationPrice":"0","leverage":"5","isolatedMargin":"0","marginType":"cross","updateTime":1});
    imports.routes.borrow_mut().extend([
        ("/fapi/v1/exchangeInfo", markets[0]["data"].clone()),
        ("/fapi/v1/leverageBracket", json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":125}]}])),
        ("/papi/v1/um/leverageBracket", json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":125}]}])),
        ("/fapi/v1/ticker/24hr", tickers[0]["data"].clone()),
        ("/fapi/v1/ticker/bookTicker", tickers[1]["data"].clone()),
        ("/fapi/v1/premiumIndex", tickers[2]["data"].clone()),
        ("/fapi/v3/balance", json!([{"accountAlias":"alias-futures","asset":"USDT","balance":"100"}])),
        ("/papi/v1/balance", json!([{"accountAlias":"alias-pm","asset":"USDT","totalWalletBalance":"100","umUnrealizedPNL":"0"}])),
        ("/fapi/v3/account", json!({"totalWalletBalance":"100","availableBalance":"60","totalUnrealizedProfit":"1"})),
        ("/papi/v1/account", json!({"accountEquity":"100","totalAvailableBalance":"60","actualEquity":"100"})),
        ("/api/v3/account", json!({"balances":[{"asset":"USDT","free":"60","locked":"40"}]})),
        ("/fapi/v3/positionRisk", json!([position.clone()])),
        ("/papi/v1/um/positionRisk", json!([position])),
        ("/fapi/v1/symbolConfig", json!([{"symbol":"BTCUSDT","marginType":"CROSSED","leverage":5}])),
        ("/fapi/v1/order", json!({"orderId":9001,"clientOrderId":"native-9001"})),
        ("/papi/v1/um/order", json!({"orderId":9002,"clientOrderId":"native-9002"})),
        ("/api/v3/order", json!({"orderId":9003,"clientOrderId":"native-9003"})),
        ("/api/v3/exchangeInfo", json!({"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","status":"TRADING","baseAssetPrecision":8,"quoteAssetPrecision":8,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.01"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"100"}]}]})),
        ("/api/v3/ticker/24hr", json!([{"symbol":"BTCUSDT","bidPrice":"10.1","askPrice":"10.9","lastPrice":"10.5","priceChangePercent":"1.5","volume":"100","quoteVolume":"1050"}])),
        ("/fapi/v1/trades", json!([{"id":1,"price":"10","qty":"1","quoteQty":"10","time":1000,"isBuyerMaker":false}])),
    ]);
}

/// Observed through the scripted transport. "Not implemented" rows reproduce the
/// TS BaseExchange default each class inherits; see exchange_trait.rs.
const SIXTEEN_METHOD_OUTCOMES: &[(&str, &str, &str, &[&str])] = &[
    (
        "Futures",
        "place_order",
        r#"ok:["9001"]"#,
        &["/fapi/v1/order"],
    ),
    (
        "Futures",
        "change_position_mode",
        r#"ok:null"#,
        &["/fapi/v1/positionSide/dual"],
    ),
    (
        "Futures",
        "start",
        r#"ok:null"#,
        &[
            "/fapi/v1/exchangeInfo",
            "/fapi/v1/leverageBracket",
            "/fapi/v1/ticker/24hr",
            "/fapi/v1/ticker/bookTicker",
            "/fapi/v1/premiumIndex",
            "/fapi/v1/listenKey",
            "/fapi/v1/positionSide/dual",
            "/fapi/v3/positionRisk",
            "/fapi/v1/symbolConfig",
            "/fapi/v3/account",
            "/fapi/v1/openOrders",
            "/fapi/v1/openAlgoOrders",
        ],
    ),
    (
        "Futures",
        "get_account",
        r#"ok:{"userId":"alias-futures"}"#,
        &["/fapi/v3/balance"],
    ),
    (
        "Futures",
        "validate_account",
        r#"ok:{"error":"","balance":{"used":40.0,"free":60.0,"total":100.0,"upnl":1.0}}"#,
        &["/fapi/v3/account"],
    ),
    (
        "Futures",
        "fetch_positions",
        r#"ok:[{"id":"BTCUSDT","symbol":"BTCUSDT","side":"long","entryPrice":10.0,"notional":10.0,"leverage":5.0,"marginType":"cross","positionMode":"oneway","unrealizedPnl":0.5,"contracts":1.0,"liquidationPrice":0.0,"marginSize":2.0,"info":{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"1","notional":"10","entryPrice":"10","markPrice":"10","unRealizedProfit":"0.5","liquidationPrice":"0","leverage":"5","isolatedMargin":"0","marginType":"cross","updateTime":1.0}},{"id":"ETHUSDC","symbol":"ETHUSDC","side":"long","entryPrice":0.0,"notional":0.0,"leverage":0.0,"marginType":"cross","positionMode":"oneway","unrealizedPnl":0.0,"contracts":0.0,"liquidationPrice":0.0,"marginSize":0.0,"info":{"synthetic":true}}]"#,
        &["/fapi/v3/positionRisk", "/fapi/v1/symbolConfig"],
    ),
    (
        "Futures",
        "fetch_orders",
        r#"ok:[]"#,
        &["/fapi/v1/openOrders", "/fapi/v1/openAlgoOrders"],
    ),
    (
        "Futures",
        "fetch_markets",
        r#"ok:[{"id":"BTC/USDT:USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.1,"price":0.25},"limits":{"amount":{"min":0.2,"max":90.0},"leverage":{"min":1.0,"max":125.0}}},{"id":"ETH/USDC:USDC","symbol":"ETHUSDC","base":"ETH","quote":"USDC","active":false,"precision":{"amount":2.0,"price":0.05},"limits":{"amount":{"min":3.0,"max":40.0},"leverage":{"min":1.0,"max":1.0}}}]"#,
        &["/fapi/v1/exchangeInfo", "/fapi/v1/leverageBracket"],
    ),
    (
        "Futures",
        "fetch_tickers",
        r#"ok:[{"id":"BTC/USDT:USDT","symbol":"BTCUSDT","bid":10.1,"ask":10.9,"last":12.3,"mark":12.4,"index":12.5,"percentage":3.6,"openInterest":0.0,"fundingRate":0.002,"fundingTime":8000.0,"volume":27.1,"quoteVolume":44.2}]"#,
        &[
            "/fapi/v1/ticker/24hr",
            "/fapi/v1/ticker/bookTicker",
            "/fapi/v1/premiumIndex",
        ],
    ),
    (
        "Futures",
        "fetch_symbol_max_leverage",
        r#"err:Not implemented"#,
        &[],
    ),
    (
        "Futures",
        "set_leverage",
        r#"ok:null"#,
        &["/fapi/v1/leverage"],
    ),
    (
        "Futures",
        "change_margin_type",
        r#"ok:null"#,
        &["/fapi/v1/marginType"],
    ),
    (
        "Futures",
        "update_order",
        r#"ok:["9001"]"#,
        &["/fapi/v1/order", "/fapi/v1/order"],
    ),
    (
        "Futures",
        "cancel_orders",
        r#"ok:null"#,
        &["/fapi/v1/order"],
    ),
    ("Futures", "schedule_cancel", r#"err:Not implemented"#, &[]),
    (
        "Futures",
        "fetch_public_trades",
        r#"ok:[{"id":"1","symbol":"BTCUSDT","price":10.0,"qty":1.0,"quoteQty":10.0,"timestamp":1000.0,"side":"buy"}]"#,
        &["/fapi/v1/trades"],
    ),
    (
        "PM",
        "place_order",
        r#"ok:["x-s3vFFRQpfixture"]"#,
        &["/papi/v1/um/order"],
    ),
    (
        "PM",
        "change_position_mode",
        r#"ok:null"#,
        &["/papi/v1/um/positionSide/dual"],
    ),
    (
        "PM",
        "start",
        r#"ok:null"#,
        &[
            "/fapi/v1/exchangeInfo",
            "/papi/v1/um/leverageBracket",
            "/fapi/v1/ticker/24hr",
            "/fapi/v1/ticker/bookTicker",
            "/fapi/v1/premiumIndex",
            "/papi/v1/listenKey",
            "/papi/v1/um/positionSide/dual",
            "/papi/v1/um/positionRisk",
            "/papi/v1/balance",
            "/papi/v1/um/openOrders",
            "/papi/v1/um/conditional/openOrders",
        ],
    ),
    (
        "PM",
        "get_account",
        r#"ok:{"userId":"alias-pm"}"#,
        &["/papi/v1/balance"],
    ),
    (
        "PM",
        "validate_account",
        r#"ok:{"error":"","balance":{"used":null,"free":null,"total":100.0,"upnl":0.0,"info":[{"accountAlias":"alias-pm","asset":"USDT","totalWalletBalance":"100","umUnrealizedPNL":"0"}]}}"#,
        &["/papi/v1/account", "/papi/v1/balance"],
    ),
    (
        "PM",
        "fetch_positions",
        r#"ok:[{"id":"BTCUSDT","symbol":"BTCUSDT","side":"long","entryPrice":10.0,"notional":10.0,"leverage":5.0,"marginType":"cross","positionMode":"oneway","unrealizedPnl":0.5,"contracts":1.0,"liquidationPrice":0.0,"marginSize":2.0,"info":{"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"1","notional":"10","entryPrice":"10","markPrice":"10","unRealizedProfit":"0.5","liquidationPrice":"0","leverage":"5","isolatedMargin":"0","marginType":"cross","updateTime":1.0}},{"id":"ETHUSDC","symbol":"ETHUSDC","side":"long","entryPrice":0.0,"notional":0.0,"leverage":10.0,"marginType":"cross","positionMode":"oneway","unrealizedPnl":0.0,"contracts":0.0,"liquidationPrice":0.0}]"#,
        &["/papi/v1/um/positionRisk"],
    ),
    (
        "PM",
        "fetch_orders",
        r#"ok:[]"#,
        &[
            "/papi/v1/um/openOrders",
            "/papi/v1/um/conditional/openOrders",
        ],
    ),
    (
        "PM",
        "fetch_markets",
        r#"ok:[{"id":"BTC/USDT:USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.1,"price":0.25},"limits":{"amount":{"min":0.2,"max":90.0},"leverage":{"min":1.0,"max":125.0}}},{"id":"ETH/USDC:USDC","symbol":"ETHUSDC","base":"ETH","quote":"USDC","active":false,"precision":{"amount":2.0,"price":0.05},"limits":{"amount":{"min":3.0,"max":40.0},"leverage":{"min":1.0,"max":1.0}}}]"#,
        &["/fapi/v1/exchangeInfo", "/papi/v1/um/leverageBracket"],
    ),
    (
        "PM",
        "fetch_tickers",
        r#"ok:[{"id":"BTC/USDT:USDT","symbol":"BTCUSDT","bid":10.1,"ask":10.9,"last":12.3,"mark":12.4,"index":12.5,"percentage":3.6,"openInterest":0.0,"fundingRate":0.002,"fundingTime":8000.0,"volume":27.1,"quoteVolume":44.2}]"#,
        &[
            "/fapi/v1/ticker/24hr",
            "/fapi/v1/ticker/bookTicker",
            "/fapi/v1/premiumIndex",
        ],
    ),
    (
        "PM",
        "fetch_symbol_max_leverage",
        r#"err:Not implemented"#,
        &[],
    ),
    (
        "PM",
        "set_leverage",
        r#"ok:null"#,
        &["/papi/v1/um/leverage"],
    ),
    (
        "PM",
        "change_margin_type",
        r#"err:Portfolio margin is not supported"#,
        &[],
    ),
    (
        "PM",
        "update_order",
        r#"ok:["x-s3vFFRQpfixture"]"#,
        &["/papi/v1/um/order", "/papi/v1/um/order"],
    ),
    ("PM", "cancel_orders", r#"ok:null"#, &["/papi/v1/um/order"]),
    ("PM", "schedule_cancel", r#"err:Not implemented"#, &[]),
    ("PM", "fetch_public_trades", r#"err:Not implemented"#, &[]),
    (
        "Spot",
        "place_order",
        r#"ok:["x-s3vFFRQpfixture"]"#,
        &["/api/v3/order"],
    ),
    (
        "Spot",
        "change_position_mode",
        r#"err:Not implemented"#,
        &[],
    ),
    (
        "Spot",
        "start",
        r#"ok:null"#,
        &[
            "/api/v3/exchangeInfo",
            "/api/v3/ticker/24hr",
            "/api/v3/account",
            "/api/v3/openOrders",
        ],
    ),
    ("Spot", "get_account", r#"err:Not implemented"#, &[]),
    (
        "Spot",
        "validate_account",
        r#"ok:{"error":"","balance":{"used":40.0,"free":60.0,"total":100.0,"upnl":0.0,"info":{"balances":[{"asset":"USDT","free":"60","locked":"40"}]},"currencies":{"USDT":{"used":40.0,"free":60.0,"total":100.0,"upnl":0.0,"info":{"asset":"USDT","free":"60","locked":"40"}}},"coins":{"USDT":{"base":"USDT","amount":100.0,"notional":100.0}}}}"#,
        &["/api/v3/account", "/api/v3/account"],
    ),
    ("Spot", "fetch_positions", r#"ok:[]"#, &["/api/v3/account"]),
    ("Spot", "fetch_orders", r#"ok:[]"#, &["/api/v3/openOrders"]),
    (
        "Spot",
        "fetch_markets",
        r#"ok:[{"id":"BTC/USDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.001,"price":0.01},"limits":{"amount":{"min":0.001,"max":100.0},"leverage":{"min":1.0,"max":1.0}},"isSpot":true}]"#,
        &["/api/v3/exchangeInfo"],
    ),
    (
        "Spot",
        "fetch_tickers",
        r#"ok:[{"id":"BTC/USDT","symbol":"BTCUSDT","bid":10.1,"ask":10.9,"last":10.5,"mark":10.5,"index":10.5,"percentage":1.5,"openInterest":0.0,"fundingRate":0.0,"fundingTime":0.0,"volume":100.0,"quoteVolume":1050.0}]"#,
        &["/api/v3/ticker/24hr"],
    ),
    (
        "Spot",
        "fetch_symbol_max_leverage",
        r#"err:Not implemented"#,
        &[],
    ),
    ("Spot", "set_leverage", r#"err:Not implemented"#, &[]),
    ("Spot", "change_margin_type", r#"err:Not implemented"#, &[]),
    (
        "Spot",
        "update_order",
        r#"ok:["x-s3vFFRQpfixture"]"#,
        &["/api/v3/order", "/api/v3/order"],
    ),
    ("Spot", "cancel_orders", r#"ok:null"#, &["/api/v3/order"]),
    ("Spot", "schedule_cancel", r#"err:Not implemented"#, &[]),
    ("Spot", "fetch_public_trades", r#"err:Not implemented"#, &[]),
];

#[test]
fn exchange_classes_bind_all_sixteen_trait_methods_through_scripted_transport() {
    use safe_cex_exchange::exchange::Exchange;
    use safe_cex_venue_binance::exchange_classes::{
        BinanceExchange, BinancePMExchange, BinanceSpotExchange,
    };
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let runtime = Rc::new(Runtime::default());
        let imports = Rc::new(Imports::default());
        sixteen_method_routes(&imports);
        let deps = ExchangeDependencies {
            runtime: runtime.clone(),
            imports: imports.clone(),
            signers: Default::default(),
            remote_signer_token: None,
            payload_signer_token: None,
        };
        let stem = match venue {
            Venue::Futures => "binance",
            Venue::PM => "binancepm",
            Venue::Spot => "binancespot",
        };
        let name = serde_json::from_value(json!(stem)).unwrap();
        let options = serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap();
        let store = Rc::new(safe_cex_store::store::DefaultStore::new());
        let exchange: Rc<dyn Exchange> = match venue {
            Venue::Futures => Rc::new(BinanceExchange::new(name, options, store, deps).unwrap()),
            Venue::PM => Rc::new(BinancePMExchange::new(name, options, store, deps).unwrap()),
            Venue::Spot => Rc::new(BinanceSpotExchange::new(name, options, store, deps).unwrap()),
        };
        let market = serde_json::from_value(json!({"id":"BTCUSDT", "symbol":"BTCUSDT", "base":"BTC", "quote":"USDT", "active":true, "precision":{"amount":0.1,"price":0.01},"limits":{"amount":{"min":0.1,"max":100},"leverage":{"min":1,"max":20}}})).unwrap();
        exchange
            .base()
            .store
            .update(
                safe_cex_store::StoreUpdate {
                    markets: Some(vec![market]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10}),
        )
        .unwrap();
        let update: safe_cex_types::UpdateOrderOpts = serde_json::from_value(json!({"order":{"id":"1","symbol":"BTCUSDT","type":"limit","side":"buy","price":10,"amount":1,"status":"open","filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0},"update":{"price":11}})).unwrap();
        let resting: safe_cex_types::Order = serde_json::from_value(json!({"id":"77","symbol":"BTCUSDT","type":"limit","side":"buy","price":10,"amount":1,"status":"open","filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0})).unwrap();
        let mut observed = Vec::new();
        macro_rules! probe {
            ($label:literal, $call:expr) => {{
                imports.requests.borrow_mut().clear();
                let outcome = match run(&runtime, $call) {
                    Ok(value) => format!("ok:{}", serde_json::to_string(&value).unwrap()),
                    Err(error) => format!("err:{}", error.message),
                };
                observed.push(($label, outcome, imports.requests.borrow().clone()));
            }};
        }
        let e = exchange.clone();
        probe!("place_order", e.base().place_order(e.as_ref(), order));
        probe!("change_position_mode", e.change_position_mode(true, None));
        probe!("start", e.start());
        probe!("get_account", e.get_account());
        probe!("validate_account", e.validate_account());
        probe!("fetch_positions", e.fetch_positions(None));
        probe!("fetch_orders", e.fetch_orders(None));
        probe!("fetch_markets", e.fetch_markets());
        probe!("fetch_tickers", e.fetch_tickers());
        probe!(
            "fetch_symbol_max_leverage",
            e.fetch_symbol_max_leverage("BTCUSDT".into())
        );
        probe!("set_leverage", e.set_leverage("BTCUSDT".into(), 7.0, None));
        probe!(
            "change_margin_type",
            e.change_margin_type(true, Some("BTCUSDT".into()))
        );
        probe!("update_order", e.update_order(update));
        probe!("cancel_orders", e.cancel_orders(vec![resting.clone()]));
        probe!("schedule_cancel", e.schedule_cancel(None));
        probe!(
            "fetch_public_trades",
            e.fetch_public_trades("BTCUSDT".into(), Some(5.0))
        );
        let expected: Vec<_> = SIXTEEN_METHOD_OUTCOMES
            .iter()
            .filter(|row| row.0 == format!("{venue:?}"))
            .collect();
        assert_eq!(
            expected.len(),
            16,
            "all sixteen trait methods pinned: {venue:?}"
        );
        assert_eq!(
            observed.len(),
            16,
            "all sixteen trait methods called: {venue:?}"
        );
        for ((label, outcome, requests), (_, want_label, want_outcome, want_requests)) in
            observed.iter().zip(expected)
        {
            assert_eq!(label, want_label, "method order: {venue:?}");
            assert_eq!(outcome, want_outcome, "{venue:?} {label} outcome");
            assert_eq!(
                requests, want_requests,
                "{venue:?} {label} transport requests"
            );
        }
    }
}

#[test]
fn private_base_send_with_log_forwards_outbound_text_and_failures() {
    let base = Base::default();
    let receiver: &dyn PrivateBase = &base;
    for text in ["not JSON {", r#"{ "id" : 42 }"#, "", "\0rocket🚀"] {
        receiver.send_with_log(text.into()).unwrap();
        assert_eq!(
            base.frames.borrow().last().unwrap(),
            text,
            "outbound text reaches send without parsing or normalization"
        );
        assert_eq!(
            base.diagnostic_frames.borrow().last().unwrap(),
            text,
            "diagnostics receive the same outbound text"
        );
    }
    assert_eq!(base.frames.borrow().len(), 4);
    assert_eq!(base.diagnostic_sends.get(), 4);
    let send_error = Rc::new(ExchangeError::new("native send refused"));
    *base.send_failure.borrow_mut() = Some(send_error.clone());
    let error = receiver
        .send_with_log("send fails before diagnostics".into())
        .unwrap_err();
    assert!(
        Rc::ptr_eq(&error, &send_error),
        "send error identity is forwarded"
    );
    assert_eq!(
        base.diagnostic_sends.get(),
        4,
        "send failure skips diagnostics"
    );
    assert_eq!(base.frames.borrow().len(), 4);
    *base.send_failure.borrow_mut() = None;
    let diagnostic_error = Rc::new(ExchangeError::new("native diagnostic refused"));
    *base.diagnostic_failure.borrow_mut() = Some(diagnostic_error.clone());
    let error = receiver
        .send_with_log("sent before diagnostic failure".into())
        .unwrap_err();
    assert!(
        Rc::ptr_eq(&error, &diagnostic_error),
        "diagnostic error identity is forwarded"
    );
    assert_eq!(
        base.frames.borrow().last().unwrap(),
        "sent before diagnostic failure"
    );
    assert_eq!(base.diagnostic_sends.get(), 5);
}

#[test]
#[cfg(feature = "native")]
fn native_import_default_weight_property_returns_existing_values() {
    struct UnusedAdapter;
    impl safe_cex_transport::RestAdapter for UnusedAdapter {
        fn execute(
            &self,
            _: safe_cex_transport::RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
            panic!("property forwarding must perform no I/O")
        }
    }
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        Rc::new(Runtime::default()),
        std::sync::Arc::new(UnusedAdapter),
        Rc::new(|_, _| panic!("property forwarding must not construct external data")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let receiver: &dyn ExchangeImports = dependencies.imports.as_ref();
    let child = record([("id", number(-0.0))]);
    let response = record([("headers", child.clone()), ("status", number(429.0))]);
    assert!(
        receiver
            .weight_response_property(&response, "headers")
            .unwrap()
            .same(&child)
    );
    assert!(matches!(
        receiver
            .weight_response_property(&response, "status")
            .unwrap(),
        EventValue::Number(429.0)
    ));
    assert!(matches!(
        receiver
            .weight_response_property(&response, "missing")
            .unwrap(),
        EventValue::Undefined
    ));
    assert_eq!(
        receiver
            .weight_response_property(&EventValue::Null, "status")
            .unwrap_err()
            .name,
        "TypeError"
    );
}

#[cfg(feature = "native")]
fn production_native_owner(venue: Venue) -> Rc<ExchangeOwner> {
    let runtime = Rc::new(Runtime::default());
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        runtime,
        std::sync::Arc::new(IdOnlyAdapter),
        Rc::new(|_, _| panic!("no external provider is configured")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let name = match venue {
        Venue::Futures => "binance",
        Venue::PM => "binancepm",
        Venue::Spot => "binancespot",
    };
    let store = Rc::new(safe_cex_store::store::DefaultStore::new());
    safe_cex_store::Store::update(
        store.as_ref(),
        safe_cex_store::StoreUpdate {
            markets: Some(vec![serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,"precision":{"price":0.1,"amount":0.1},"limits":{"amount":{"min":0.1,"max":1},"leverage":{"min":1,"max":100}}})).unwrap()]),
            tickers: Some(vec![serde_json::from_value(json!({"id":"BTCUSDT","symbol":"BTCUSDT","bid":99,"ask":101,"last":100,"mark":100,"index":100,"percentage":0,"volume":0,"quoteVolume":0,"fundingRate":0,"fundingTime":0,"openInterest":0})).unwrap()]),
            positions: Some(vec![serde_json::from_value(json!({"id":"BTCUSDTshort","symbol":"BTCUSDT","side":"short","entryPrice":100,"notional":300,"leverage":10,"unrealizedPnl":0,"contracts":3,"liquidationPrice":200})).unwrap()]),
            ..Default::default()
        },
        false,
    ).unwrap();
    ExchangeOwner::new(
        venue,
        serde_json::from_value(json!(name)).unwrap(),
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        store,
        dependencies,
    )
    .unwrap()
}

#[test]
#[cfg(feature = "native")]
fn production_native_client_ids_fit_every_family_and_payload_path() {
    use safe_cex_venue_binance::exchange_classes::ExchangeImports;
    // Exercise the actual production import, not Imports::uuid()'s "fixture".
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        Rc::new(Runtime::default()),
        std::sync::Arc::new(IdOnlyAdapter),
        Rc::new(|_, _| panic!("ID generation has no external-data dependency")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let imports: &dyn ExchangeImports = dependencies.imports.as_ref();
    for _ in 0..128 {
        let id = imports.uuid();
        assert!(
            id.len() <= 21,
            "production uuid must be stripped nanoid, got {id:?}"
        );
        assert!(
            id.bytes().all(|b| b.is_ascii_alphanumeric()),
            "source strips both URL-alphabet punctuation characters: {id:?}"
        );
    }
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let owner = production_native_owner(venue);
        let mint_opts: safe_cex_types::PlaceOrderOpts = serde_json::from_value(
            json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":90}),
        )
        .unwrap();
        let minted = owner
            .base
            .mint_client_order_id(&mint_opts)
            .expect("production rule mint must produce a valid venue ID");
        assert!(minted.starts_with("x-s3vFFRQp"));
        assert!(minted.len() <= "x-s3vFFRQp".len() + 21);
        assert!(
            minted["x-s3vFFRQp".len()..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric())
        );
        let check = |row: &EventValue, field: &str, prefixed: bool| {
            let id = row.read(field).unwrap().text().unwrap();
            let suffix = if prefixed {
                id.strip_prefix("x-s3vFFRQp").expect("broker prefix")
            } else {
                &id
            };
            assert!(
                suffix.len() <= 21,
                "native payload suffix is stripped nanoid: {venue:?} {field} {id}"
            );
            assert!(suffix.bytes().all(|b| b.is_ascii_alphanumeric()));
            assert!(
                id.len() <= if venue == Venue::PM { 32 } else { 36 },
                "venue ID length: {venue:?} {field} {id}"
            );
        };
        for (kind, amount, attached, count) in [
            ("limit", 1.0, false, 1),
            ("market", 3.0, false, 3),
            ("limit", 3.0, true, 5),
            ("stop_market", 1.0, false, 1),
            ("take_profit_market", 1.0, false, 1),
        ] {
            let mut options =
                json!({"symbol":"BTCUSDT","type":kind,"side":"buy","amount":amount,"price":90});
            if attached {
                options["stopLoss"] = json!(80);
                options["takeProfit"] = json!(120);
            }
            let result = owner.format_order(EventValue::from_json(options)).unwrap();
            let (regular, algo) = if venue == Venue::Futures {
                (
                    result.read("regular").unwrap(),
                    result.read("algo").unwrap(),
                )
            } else {
                (result, EventValue::array(vec![]))
            };
            assert_eq!(
                regular.array_values().unwrap().len() + algo.array_values().unwrap().len(),
                count,
                "real formatted payload count {venue:?} {kind}"
            );
            for row in regular.array_values().unwrap() {
                check(&row, "newClientOrderId", true);
            }
            for row in algo.array_values().unwrap() {
                check(&row, "clientAlgoId", true);
            }
        }
        let result = owner.format_order(EventValue::from_json(json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":90,"clientOrderId":"caller_kept-123"}))).unwrap();
        let rows = if venue == Venue::Futures {
            result.read("regular").unwrap()
        } else {
            result
        };
        assert_eq!(
            rows.array_values().unwrap()[0]
                .read("newClientOrderId")
                .unwrap()
                .text()
                .unwrap(),
            "caller_kept-123"
        );
        if venue != Venue::Spot {
            let opts = serde_json::from_value(json!({"symbol":"BTCUSDT","type":"trailing_stop_market","side":"buy","amount":1,"price":90})).unwrap();
            let rows = owner.format_trailing(&opts).unwrap();
            assert_eq!(rows.array_values().unwrap().len(), 1);
            check(
                &rows.array_values().unwrap()[0],
                if venue == Venue::PM {
                    "newClientOrderId"
                } else {
                    "clientAlgoId"
                },
                venue != Venue::PM,
            );
            let rows = owner
                .format_order(EventValue::from_json(serde_json::to_value(opts).unwrap()))
                .unwrap();
            let rows = if venue == Venue::Futures {
                rows.read("algo").unwrap()
            } else {
                rows
            };
            assert_eq!(rows.array_values().unwrap().len(), 1);
            check(
                &rows.array_values().unwrap()[0],
                if venue == Venue::PM {
                    "newClientOrderId"
                } else {
                    "clientAlgoId"
                },
                venue != Venue::PM,
            );
        }
    }
}

#[cfg(feature = "native")]
struct IdOnlyAdapter;
#[cfg(feature = "native")]
impl safe_cex_transport::RestAdapter for IdOnlyAdapter {
    fn execute(
        &self,
        _: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        panic!("ID-only production host must perform no HTTP I/O")
    }
}

#[cfg(feature = "native")]
fn native_fill_events(owner: &ExchangeOwner) -> Rc<RefCell<Vec<Value>>> {
    let events = Rc::new(RefCell::new(vec![]));
    let saved = events.clone();
    owner.base.on(
        "fill",
        Rc::new(move |_, payload| {
            let payload = safe_cex_exchange::js_runtime::value::to_json(payload).unwrap();
            saved.borrow_mut().push(payload.clone());
            Ok(())
        }),
    );
    events
}

#[cfg(feature = "native")]
fn native_order_frame(venue: Venue, filled: bool) -> Value {
    let data = json!({"e":"executionReport","c":"native-client","i":42,"X":if filled {"PARTIALLY_FILLED"}else{"NEW"},"x":if filled {"TRADE"}else{"NEW"},"ot":"LIMIT","o":"LIMIT","s":"BTCUSDT","S":"BUY","p":"91.2","ap":"92.3","sp":"0","q":"3","z":if filled {"0.75"}else{"0"},"l":"0.25","R":false});
    if venue == Venue::Spot {
        json!({"subscriptionId":7,"event":data})
    } else {
        json!({"e":"ORDER_TRADE_UPDATE","o":data})
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_private_orders_frames_update_the_real_store() {
    use safe_cex_types::{OrderSide, OrderStatus, OrderType};
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let owner = production_native_owner(venue);
        let fills = native_fill_events(&owner);
        owner
            .private
            .on_message(string(native_order_frame(venue, false).to_string()))
            .unwrap();
        let rows = owner.base.store.orders();
        assert_eq!(
            rows.len(),
            1,
            "one order through real private binding: {venue:?}"
        );
        let order = &rows[0];
        assert_eq!(
            order.id,
            if venue == Venue::Futures {
                "42"
            } else {
                "native-client"
            }
        );
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.status, OrderStatus::Open);
        assert_eq!(order.r#type, OrderType::Limit);
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(
            (
                order.price.to_number(),
                order.amount,
                order.filled,
                order.remaining
            ),
            (91.2, 3.0, 0.0, 3.0)
        );
        assert!(fills.borrow().is_empty(), "NEW must emit no fill");
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_private_fill_frames_emit_and_update_the_real_order() {
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let owner = production_native_owner(venue);
        let fills = native_fill_events(&owner);
        owner
            .private
            .on_message(string(native_order_frame(venue, false).to_string()))
            .unwrap();
        owner
            .private
            .on_message(string(native_order_frame(venue, true).to_string()))
            .unwrap();
        let rows = owner.base.store.orders();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].filled, rows[0].remaining), (0.75, 2.25));
        assert_eq!(rows[0].status, safe_cex_types::OrderStatus::Open);
        assert_eq!(
            *fills.borrow(),
            [
                json!({"id":if venue==Venue::Futures {"42"}else{"native-client"},"side":"buy","symbol":"BTCUSDT","price":if venue==Venue::Spot {91.2}else{92.3},"amount":0.25})
            ],
            "native emitted fill contents: {venue:?}"
        );
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_private_position_frames_update_the_real_store() {
    for venue in [Venue::Futures, Venue::PM] {
        let owner = production_native_owner(venue);
        let fills = native_fill_events(&owner);
        let frame = json!({"e":"ACCOUNT_UPDATE","a":{"P":[{"s":"BTCUSDT","pa":"2","ep":"110","up":"7","ps":"BOTH"}],"B":[]}});
        // Native test time starts at 0: the production throttle schedules its
        // REST repair, allowing this immediate WS mutation to be observed first.
        owner.private.on_message(string(frame.to_string())).unwrap();
        let positions = owner.base.store.positions();
        let p = positions
            .iter()
            .find(|p| {
                p.id == if venue == Venue::Futures {
                    "BTCUSDT"
                } else {
                    "BTCUSDTshort"
                }
            })
            .unwrap();
        assert_eq!(p.symbol, "BTCUSDT");
        assert_eq!(p.side, safe_cex_types::PositionSide::Long);
        assert_eq!(
            (p.contracts, p.entry_price, p.unrealized_pnl),
            (2.0, 110.0, 7.0)
        );
        assert_eq!(
            p.notional,
            if venue == Venue::Futures {
                227.0
            } else {
                220.0
            }
        );
        assert!(fills.borrow().is_empty(), "account frame must emit no fill");
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_pm_position_info_does_not_grow_per_frame() {
    let owner = production_native_owner(Venue::PM);
    let frame = json!({"e":"ACCOUNT_UPDATE","a":{"P":[{
        "s":"BTCUSDT","pa":"2","ep":"110","up":"7","ps":"BOTH"
    }],"B":[]}})
    .to_string();
    let mut first_depth = None;
    for index in 0..8 {
        owner.private.on_message(string(frame.clone())).unwrap();
        let positions = owner.base.store.positions();
        let position = positions.iter().find(|p| p.symbol == "BTCUSDT").unwrap();
        assert_eq!((position.contracts, position.entry_price), (2.0, 110.0));
        let mut info = position.info.as_ref();
        let mut depth = 0;
        while let Some(value) = info {
            depth += 1;
            info = value.get("info");
        }
        let expected = *first_depth.get_or_insert(depth);
        assert_eq!(
            depth, expected,
            "identical PM frame {index} must not append a previous-position info snapshot"
        );
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_pm_position_info_is_the_current_position_without_info() {
    let owner = production_native_owner(Venue::PM);
    for index in 0..16 {
        let amount: f64 = if index % 2 == 0 { 2.0 } else { -3.0 };
        let entry = 110.0 + f64::from(index);
        let upnl = 7.0 - f64::from(index);
        let frame = json!({"e":"ACCOUNT_UPDATE","a":{"P":[{
            "s":"BTCUSDT","pa":amount.to_string(),"ep":entry.to_string(),
            "up":upnl.to_string(),"ps":"BOTH"
        }],"B":[]}});
        owner.private.on_message(string(frame.to_string())).unwrap();
        let positions = owner.base.store.positions();
        let position = positions.iter().find(|p| p.symbol == "BTCUSDT").unwrap();
        assert_eq!(position.entry_price, entry);
        assert_eq!(position.contracts, amount.abs());
        assert_eq!(position.unrealized_pnl, upnl);
        assert_eq!(position.notional, amount.abs() * entry);
        assert_eq!(
            position.side,
            if amount > 0.0 {
                safe_cex_types::PositionSide::Long
            } else {
                safe_cex_types::PositionSide::Short
            }
        );
        let mut expected = serde_json::to_value(position).unwrap();
        expected.as_object_mut().unwrap().remove("info");
        let info = position
            .info
            .as_ref()
            .expect("PM keeps a position snapshot");
        assert!(
            info.get("info").is_none(),
            "snapshot must have exactly one info depth"
        );
        let expected = expected.as_object().unwrap();
        assert_eq!(info.as_object().unwrap().len(), expected.len());
        for (field, value) in expected {
            if let Some(number) = value.as_f64() {
                assert_eq!(
                    info.get(field)
                        .and_then(|value| value.as_f64())
                        .map(f64::to_bits),
                    Some(number.to_bits()),
                    "frame {index}: info numeric field {field} uses the current position"
                );
            } else {
                assert_eq!(
                    info.get(field),
                    Some(&safe_cex_types::retained_json::RetainedJsonInput::from(
                        value.clone()
                    )),
                    "frame {index}: info field {field}"
                );
            }
        }
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_private_balance_frames_follow_each_source_family() {
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let owner = production_native_owner(venue);
        let fills = native_fill_events(&owner);
        let old_balance = owner.base.store.balance().clone();
        let frame = if venue == Venue::Spot {
            json!({"subscriptionId":7,"event":{"e":"outboundAccountPosition","B":[{"a":"USDT","f":"8","l":"2"}]}})
        } else {
            json!({"e":"ACCOUNT_UPDATE","a":{"P":[],"B":[{"a":"USDT","wb":"10","cw":"8","bc":"0"}]}})
        };
        owner.private.on_message(string(frame.to_string())).unwrap();
        let balance = owner.base.store.balance();
        if venue == Venue::Spot {
            assert_eq!(
                (balance.free, balance.used, balance.total, balance.upnl),
                (8.0, 2.0, 10.0, 0.0)
            );
            let currency = balance
                .currencies
                .as_ref()
                .unwrap()
                .get("USDT")
                .unwrap()
                .as_balance()
                .expect("Binance supplies a declared Balance row");
            assert_eq!(
                (currency.free, currency.used, currency.total),
                (8.0, 2.0, 10.0)
            );
            assert_eq!(
                owner.base.store.positions().len(),
                1,
                "spot account topic does not invent derivative positions"
            );
        } else {
            // Both derivatives source handlers leave B to REST reconciliation.
            assert_eq!(
                (balance.free, balance.used, balance.total, balance.upnl),
                (
                    old_balance.free,
                    old_balance.used,
                    old_balance.total,
                    old_balance.upnl
                )
            );
        }
        assert!(owner.base.store.orders().is_empty());
        assert!(fills.borrow().is_empty(), "balance topic must emit no fill");
    }
}

#[cfg(feature = "native")]
fn native_stop_limit_snapshot_case(venue: Venue) {
    use safe_cex_transport::{RestAdapter, RestRequest, RestResponse, RestResponseBody};
    use std::sync::{Arc, Mutex};
    struct OpenOrders {
        calls: Mutex<Vec<String>>,
        unmapped: bool,
    }
    impl RestAdapter for OpenOrders {
        fn execute(
            &self,
            request: RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
            let path = request.url.split('?').next().unwrap().to_owned();
            self.calls.lock().unwrap().push(path.clone());
            let normal = json!({"orderId":11,"clientOrderId":"regular-11","symbol":"BTCUSDT","type":"LIMIT","side":"BUY","price":"90","origQty":"3","executedQty":"1","reduceOnly":false});
            let rows = if path.ends_with("/openOrders") && !path.contains("/conditional/") {
                json!([normal])
            } else if path.ends_with("/openAlgoOrders") {
                json!([
                    {"algoId":22,"symbol":"BTCUSDT","orderType":if self.unmapped {"STOP"}else{"STOP_MARKET"},"side":"SELL","triggerPrice":"80","quantity":"2","reduceOnly":true,"workingType":"MARK_PRICE"},
                    {"algoId":33,"symbol":"BTCUSDT","orderType":if self.unmapped {"TAKE_PROFIT"}else{"TAKE_PROFIT_MARKET"},"side":"SELL","triggerPrice":"120","quantity":"2","reduceOnly":true,"workingType":"MARK_PRICE"}
                ])
            } else if path.ends_with("/conditional/openOrders") {
                json!([
                    {"newClientStrategyId":"stop-22","symbol":"BTCUSDT","strategyType":if self.unmapped {"STOP"}else{"STOP_MARKET"},"side":"SELL","price":"80","origQty":"2","executedQty":"0","reduceOnly":true,"workingType":"MARK_PRICE"},
                    {"newClientStrategyId":"tp-33","symbol":"BTCUSDT","strategyType":if self.unmapped {"TAKE_PROFIT"}else{"TAKE_PROFIT_MARKET"},"side":"SELL","price":"120","origQty":"2","executedQty":"0","reduceOnly":true,"workingType":"MARK_PRICE"}
                ])
            } else {
                panic!("unexpected production request {path}")
            };
            Box::pin(std::future::ready(Ok(RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: RestResponseBody::Encoded(serde_json::to_vec(&rows).unwrap()),
            })))
        }
    }
    for unmapped in [false, true] {
        // Each phase uses a fresh actual host/client rate window.
        let construct = || {
            let runtime = Rc::new(Runtime::default());
            let adapter = Arc::new(OpenOrders {
                calls: Mutex::new(vec![]),
                unmapped,
            });
            let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
                runtime.clone(),
                adapter.clone(),
                Rc::new(|_, _| panic!("no external provider configured")),
                None,
                safe_cex_exchange::native::NativeHttpEvents::default(),
            );
            let store = Rc::new(safe_cex_store::store::DefaultStore::new());
            let owner = ExchangeOwner::new(
                venue,
                serde_json::from_value(json!(if venue == Venue::Futures {
                    "binance"
                } else {
                    "binancepm"
                }))
                .unwrap(),
                serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"}))
                    .unwrap(),
                store,
                dependencies,
            )
            .unwrap();
            (runtime, adapter, owner)
        };
        let (runtime, adapter, owner) = construct();
        let mapped = run(
            &runtime,
            safe_cex_venue_binance::class_jobs::reaction(owner.invoke(Method::Orders, vec![])),
        )
        .unwrap();
        let raw = mapped.array_values().unwrap();
        assert_eq!(raw.len(), 3);
        for row in raw.iter().skip(1) {
            if unmapped {
                assert!(
                    row.has_own("type"),
                    "native mapper keeps own undefined, not absence"
                );
                assert!(matches!(row.read("type").unwrap(), EventValue::Undefined));
            }
        }
        assert_eq!(adapter.calls.lock().unwrap().len(), 2);
        let (runtime, adapter, owner) = construct();
        let exchange = sdk_class(venue, owner.clone());
        let orders = run(&runtime, exchange.fetch_orders(None))
            .expect("unmapped STOP/TAKE_PROFIT must not discard the production snapshot");
        assert_eq!(
            orders.len(),
            3,
            "retain normal plus both unmapped stop-limit rows"
        );
        assert_eq!(
            (
                orders[0].price.to_number(),
                orders[0].amount,
                orders[0].filled,
                orders[0].remaining
            ),
            (90.0, 3.0, 1.0, 2.0)
        );
        for (index, expected_price) in [(1, 80.0), (2, 120.0)] {
            assert_eq!(orders[index].symbol, "BTCUSDT");
            assert_eq!(
                (
                    orders[index].price.to_number(),
                    orders[index].amount,
                    orders[index].filled,
                    orders[index].remaining
                ),
                (expected_price, 2.0, 0.0, 2.0)
            );
            // TS table miss is undefined, never an invented stop-market type.
            let wire = serde_json::to_value(&orders[index]).unwrap();
            if unmapped {
                assert!(matches!(
                    orders[index].r#type,
                    safe_cex_types::ModelField::Undefined
                ));
                assert!(wire.get("type").is_none());
            } else {
                assert_eq!(
                    wire["type"],
                    if index == 1 {
                        "stop_market"
                    } else {
                        "take_profit_market"
                    }
                );
            }
        }
        assert_eq!(adapter.calls.lock().unwrap().len(), 2);
        let (runtime, adapter, owner) = construct();
        let exchange = sdk_class(venue, owner.clone());
        run(&runtime, exchange.fetch_and_set_private_snapshot(1)).unwrap();
        assert!(
            owner.base.store.loaded().orders,
            "real binding snapshot sets loaded.orders"
        );
        assert_eq!(owner.base.store.orders().len(), 3);
        assert_eq!(adapter.calls.lock().unwrap().len(), 2);
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_futures_stop_limit_snapshot_keeps_rows_and_loads() {
    native_stop_limit_snapshot_case(Venue::Futures);
}
#[test]
#[cfg(feature = "native")]
fn production_native_pm_stop_limit_snapshot_keeps_rows_and_loads() {
    native_stop_limit_snapshot_case(Venue::PM);
}

#[cfg(feature = "native")]
fn native_stop_limit_frame_case(venue: Venue) {
    let owner = production_native_owner(venue);
    let fills = native_fill_events(&owner);
    for (index, kind) in ["STOP", "TAKE_PROFIT"].into_iter().enumerate() {
        let event = if venue == Venue::Futures {
            json!({"e":"ALGO_UPDATE","o":{"aid":22+index,"caid":"native-stop","o":kind,"s":"BTCUSDT","S":"SELL","X":"NEW","tp":"80","q":"2","R":true,"wt":"MARK_PRICE"}})
        } else {
            json!({"e":"CONDITIONAL_ORDER_TRADE_UPDATE","so":{"c":format!("native-stop-{index}"),"st":kind,"os":"NEW","s":"BTCUSDT","S":"SELL","p":"0","sp":"80","q":"2","z":"0","R":true,"wt":"MARK_PRICE"}})
        };
        owner
            .private
            .on_message(string(event.to_string()))
            .expect("source-accepted stop-limit private frame must reach the real Store");
    }
    owner
        .private
        .on_message(string(native_order_frame(venue, false).to_string()))
        .unwrap();
    let orders = owner.base.store.orders();
    assert_eq!(
        orders.len(),
        3,
        "both stop-limit rows and subsequent normal frame survive"
    );
    for row in orders.iter().take(2) {
        assert_eq!(
            (row.price.to_number(), row.amount, row.filled, row.remaining),
            (80.0, 2.0, 0.0, 2.0)
        );
        assert!(matches!(row.r#type, safe_cex_types::ModelField::Undefined));
        assert!(serde_json::to_value(row).unwrap().get("type").is_none());
    }
    assert_eq!(orders[2].symbol, "BTCUSDT");
    assert!(fills.borrow().is_empty());
}
#[test]
#[cfg(feature = "native")]
fn production_native_futures_stop_limit_private_frames_keep_rows() {
    native_stop_limit_frame_case(Venue::Futures);
}
#[test]
#[cfg(feature = "native")]
fn production_native_pm_stop_limit_private_frames_keep_rows() {
    native_stop_limit_frame_case(Venue::PM);
}

#[test]
#[cfg(feature = "native")]
fn production_native_private_post_only_rejection_keeps_the_rejection_reason() {
    let owner = production_native_owner(Venue::Futures);
    let fills = native_fill_events(&owner);
    owner
        .private
        .on_message(string(
            native_order_frame(Venue::Futures, false).to_string(),
        ))
        .unwrap();
    let mut frame = native_order_frame(Venue::Futures, false);
    frame["o"]["X"] = json!("CANCELED");
    frame["o"]["r"] = json!("GTX order rejected");
    owner.private.on_message(string(frame.to_string())).unwrap();
    let rows = owner.base.store.orders();
    assert_eq!(
        rows.len(),
        1,
        "production canceled order remains observable"
    );
    assert_eq!(rows[0].status, safe_cex_types::OrderStatus::Canceled);
    assert_eq!(
        rows[0].extra.as_ref().and_then(|extra| extra.reject_reason),
        Some(safe_cex_types::OrderRejectReason::PostOnly),
        "production rejection classifier updates the real Store extra"
    );
    assert!(fills.borrow().is_empty(), "cancel rejection emits no fill");
}

#[cfg(feature = "native")]
#[derive(Default)]
struct ProjectionWire(std::sync::Mutex<Vec<safe_cex_transport::RestRequest>>);
#[cfg(feature = "native")]
impl safe_cex_transport::RestAdapter for ProjectionWire {
    fn execute(
        &self,
        request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        self.0.lock().unwrap().push(request);
        Box::pin(async {
            Ok(safe_cex_transport::RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: safe_cex_transport::RestResponseBody::Encoded(br#"{"ok":true}"#.to_vec()),
            })
        })
    }
}
#[cfg(feature = "native")]
fn projection_host() -> (
    Rc<Runtime>,
    Rc<dyn ExchangeImports>,
    std::sync::Arc<ProjectionWire>,
) {
    let runtime = Rc::new(Runtime::default());
    let wire = std::sync::Arc::new(ProjectionWire::default());
    let deps = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        wire.clone(),
        Rc::new(|_, _| panic!("no external provider")),
        None,
        Default::default(),
    );
    (runtime, deps.imports, wire)
}
#[cfg(feature = "native")]
fn projection_client(imports: &dyn ExchangeImports, kind: Factory) -> EventValue {
    let options = EventValue::from_json(json!({"key":"fixture-key","secret":"fixture-secret"}));
    let owner = record([("name", string("binance")), ("options", options.clone())]);
    imports
        .factory(Venue::Futures, kind, options, owner)
        .unwrap()
}
#[test]
#[cfg(feature = "native")]
fn native_projection_query_bigint_is_rendered_instead_of_dropping_every_parameter() {
    for method in [HttpMethod::Get, HttpMethod::Delete] {
        let (runtime, imports, wire) = projection_host();
        let client = projection_client(imports.as_ref(), Factory::PublicAPI);
        let params = record([
            ("symbol", string("BTCUSDT")),
            (
                "orderId",
                EventValue::BigInt(Rc::new("10".parse().unwrap())),
            ),
        ]);
        let result = run(
            &runtime,
            imports
                .request(client, method, "/projection", Some(params))
                .unwrap(),
        )
        .result;
        result.expect("BigInt query must be admitted by the real production host");
        let calls = wire.0.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            calls[0].url, "/projection?symbol=BTCUSDT&orderId=10",
            "real native query must keep all params and stringify BigInt"
        );
    }
}
#[test]
#[cfg(feature = "native")]
fn native_projection_body_bigint_rejects_before_any_transport_call() {
    for method in [HttpMethod::Post, HttpMethod::Put] {
        let (runtime, imports, wire) = projection_host();
        let client = projection_client(imports.as_ref(), Factory::PublicAPI);
        let data = record([(
            "quantity",
            EventValue::BigInt(Rc::new("10".parse().unwrap())),
        )]);
        let error = run(
            &runtime,
            imports
                .request(client, method, "/projection", Some(data))
                .unwrap(),
        )
        .result
        .expect_err("real native JSON body must reject BigInt before transport");
        assert_eq!(error.name, "TypeError");
        assert!(
            wire.0.lock().unwrap().is_empty(),
            "serialization refusal must send no empty request"
        );
    }
}

#[test]
#[cfg(feature = "native")]
fn native_projection_signing_observes_bigint_query_and_body_before_serialization() {
    use std::io::Write;
    let mut vectors = vec![];
    let mut queries = vec![];
    for method in [HttpMethod::Get, HttpMethod::Post] {
        let (runtime, imports, wire) = projection_host();
        let client = projection_client(imports.as_ref(), Factory::API);
        let payload = record([
            ("symbol", string("BTCUSDT")),
            (
                "orderId",
                EventValue::BigInt(Rc::new("10".parse().unwrap())),
            ),
        ]);
        run(
            &runtime,
            imports
                .request(client, method, "/projection", Some(payload))
                .unwrap(),
        )
        .result
        .expect("source signs BigInt as its decimal query spelling");
        let calls = wire.0.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(
            calls[0]
                .url
                .starts_with("/projection?symbol=BTCUSDT&orderId=10&timestamp="),
            "real native signer must retain request arguments: {}",
            calls[0].url
        );
        assert!(calls[0].url.contains("&signature="));
        assert!(
            calls[0].body.is_none(),
            "source signer moves data to the query"
        );
        let query = calls[0].url.split_once('?').unwrap().1.to_owned();
        let fields: std::collections::HashMap<_, _> = query
            .split('&')
            .map(|pair| pair.split_once('=').unwrap())
            .collect();
        let is_get = matches!(method, HttpMethod::Get);
        let mut config = json!({"method":if is_get {"get"}else{"post"},"url":"/projection"});
        config[if is_get { "params" } else { "data" }] = json!({"symbol":"BTCUSDT","orderId":"10"});
        vectors.push(json!({"mode":if is_get {"bigint-query"}else{"bigint-body"},"lossy":true,"config":config,"options":{"key":"fixture-key","secret":"fixture-secret"},"now":fields["timestamp"].parse::<f64>().unwrap(),"window":fields["recvWindow"].parse::<f64>().unwrap()}));
        queries.push(query);
    }
    // One source capture for the complete signed corpus, using actual TS, qs and HMAC.
    let compiler = std::env::var("SAFE_CEX_TYPESCRIPT_PATH").unwrap_or_else(|_| {
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../node_modules/typescript").into()
    });
    let mut child = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/signing.mjs"))
        .env("SAFE_CEX_TYPESCRIPT_PATH", compiler)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&vectors).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 2, "independent signed BigInt source floor");
    assert_eq!(queries.len(), 2);
    for (native, source) in queries.iter().zip(source) {
        assert_eq!(
            source["query"].as_str(),
            Some(native.as_str()),
            "real signed wire must match actual TS BigInt query/HMAC"
        );
    }
}
#[test]
#[cfg(feature = "native")]
fn native_projection_unsigned_private_body_keeps_json_bigint_refusal() {
    let (runtime, imports, wire) = projection_host();
    let options = EventValue::from_json(json!({"key":"","secret":""}));
    let owner = record([("name", string("binance")), ("options", options.clone())]);
    let client = imports
        .factory(Venue::Futures, Factory::API, options, owner)
        .unwrap();
    let payload = record([(
        "quantity",
        EventValue::BigInt(Rc::new("10".parse().unwrap())),
    )]);
    let error = run(
        &runtime,
        imports
            .request(client, HttpMethod::Post, "/projection", Some(payload))
            .unwrap(),
    )
    .result
    .expect_err("skipped signing must leave original JSON BigInt refusal");
    assert_eq!(error.name, "TypeError");
    assert!(wire.0.lock().unwrap().is_empty());
}

#[cfg(feature = "native")]
fn asset_market(symbol: &str, base: &str) -> Value {
    json!({"symbol":symbol,"baseAsset":base,"quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","status":"TRADING","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"0.01","minQty":"0.01","maxQty":"100"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0.01","minQty":"0.01","maxQty":"100"}]})
}

#[cfg(feature = "native")]
fn market_asset_refusal_case(venue: Venue) {
    use safe_cex_transport::{RestAdapter, RestRequest, RestResponse, RestResponseBody};
    use std::sync::Arc;
    struct Markets(Value);
    impl RestAdapter for Markets {
        fn execute(
            &self,
            request: RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
            assert!(
                request
                    .url
                    .split('?')
                    .next()
                    .unwrap()
                    .ends_with("/exchangeInfo"),
                "{}",
                request.url
            );
            Box::pin(std::future::ready(Ok(RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: RestResponseBody::Encoded(serde_json::to_vec(&self.0).unwrap()),
            })))
        }
    }
    let source = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/public-rest.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .args([
            match venue {
                Venue::Futures => "futures",
                Venue::PM => "pm",
                Venue::Spot => "spot",
            },
            "markets",
            "asset-rows",
        ])
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let source: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(
        source.as_array().unwrap().len(),
        10,
        "source raw asset cases"
    );
    let mut witnesses = 0;
    for (field, wire_field) in [("base", "baseAsset"), ("quote", "quoteAsset")] {
        for value in [
            Some(json!({"asset":"wrong"})),
            Some(json!(false)),
            Some(Value::Null),
            Some(json!(17)),
            None,
        ] {
            for refresh in [false, true] {
                let runtime = Rc::new(Runtime::default());
                let mut bad = asset_market("BADUSDT", "BAD");
                if let Some(value) = &value {
                    bad[wire_field] = value.clone();
                } else {
                    bad.as_object_mut().unwrap().remove(wire_field);
                }
                let deps = safe_cex_venue_binance::native_host::native_dependencies(
                    runtime.clone(),
                    Arc::new(Markets(
                        json!({"symbols":[asset_market("BTCUSDT","BTC"),bad,asset_market("ETHUSDT","ETH")]}),
                    )),
                    Rc::new(|_, _| panic!("no external provider")),
                    None,
                    safe_cex_exchange::native::NativeHttpEvents::default(),
                );
                let owner = ExchangeOwner::new(
                    venue,
                    serde_json::from_value(json!(match venue {
                        Venue::Futures => "binance",
                        Venue::PM => "binancepm",
                        Venue::Spot => "binancespot",
                    }))
                    .unwrap(),
                    serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true}))
                        .unwrap(),
                    Rc::new(safe_cex_store::store::DefaultStore::new()),
                    deps,
                )
                .unwrap();
                let errors = Rc::new(RefCell::new(Vec::new()));
                let capture = errors.clone();
                owner.base.on(
                    "error",
                    Rc::new(move |_, error| {
                        capture.borrow_mut().push(error.clone());
                        Ok(())
                    }),
                );
                let exchange = sdk_class(venue, owner.clone());
                let markets = if refresh {
                    run(&runtime, exchange.refresh_markets()).unwrap();
                    owner.base.store.markets().to_vec()
                } else {
                    run(&runtime, exchange.fetch_markets()).unwrap()
                };
                assert_eq!(
                    markets
                        .iter()
                        .map(|m| m.symbol.as_str())
                        .collect::<Vec<_>>(),
                    ["BTCUSDT", "ETHUSDT"],
                    "{venue:?} bad{field} refresh={refresh}"
                );
                let errors = errors.borrow();
                assert_eq!(errors.len(), 1, "one visible row refusal");
                let message = String::from_utf16(errors[0].as_str().unwrap())
                    .expect("market refusal is a scalar Unicode message");
                let expected = value
                    .as_ref()
                    .map_or_else(|| "undefined".into(), Value::to_string);
                assert!(
                    message.contains("BADUSDT") && message.contains(&format!("{field}={expected}")),
                    "{message}"
                );
                assert_eq!(runtime.reported.borrow().len(), 1);
                witnesses += 1;
                owner.base.dispose().unwrap();
            }
        }
    }
    assert_eq!(witnesses, 20, "typed fetch and refresh per asset/domain");
}

#[test]
#[cfg(feature = "native")]
fn production_native_futures_market_asset_refusals_keep_siblings() {
    market_asset_refusal_case(Venue::Futures);
}
#[test]
#[cfg(feature = "native")]
fn production_native_pm_market_asset_refusals_keep_siblings() {
    market_asset_refusal_case(Venue::PM);
}
#[test]
#[cfg(feature = "native")]
fn production_native_spot_market_asset_refusals_keep_siblings() {
    market_asset_refusal_case(Venue::Spot);
}

#[cfg(feature = "native")]
fn market_field_refusal_case(venue: Venue, leverage: bool) {
    use safe_cex_transport::{RestAdapter, RestRequest, RestResponse, RestResponseBody};
    use std::sync::Arc;
    struct Markets {
        rows: Value,
        brackets: Value,
    }
    impl RestAdapter for Markets {
        fn execute(
            &self,
            request: RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
            let path = request.url.split('?').next().unwrap();
            let body = if path.ends_with("/exchangeInfo") {
                &self.rows
            } else if path.ends_with("/leverageBracket") {
                &self.brackets
            } else {
                panic!("unexpected {path}")
            };
            Box::pin(std::future::ready(Ok(RestResponse {
                status: 200,
                reason: Some("OK".into()),
                headers: Default::default(),
                body: RestResponseBody::Encoded(serde_json::to_vec(body).unwrap()),
            })))
        }
    }
    let values = if leverage {
        vec![
            Some(json!("125")),
            Some(json!({"leverage":125})),
            Some(json!([125])),
            Some(json!(true)),
        ]
    } else {
        vec![
            Some(json!({"symbol":"wrong"})),
            Some(json!(false)),
            Some(Value::Null),
            Some(json!(17)),
            None,
        ]
    };
    let field = if leverage {
        "limits.leverage.max"
    } else {
        "symbol"
    };
    let mut count = 0;
    for value in values {
        for refresh in [false, true] {
            let runtime = Rc::new(Runtime::default());
            let mut bad = asset_market("BADUSDT", "BAD");
            if !leverage {
                if let Some(value) = &value {
                    bad["symbol"] = value.clone();
                } else {
                    bad.as_object_mut().unwrap().remove("symbol");
                }
            }
            let deps = safe_cex_venue_binance::native_host::native_dependencies(
                runtime.clone(),
                Arc::new(Markets {
                    rows: json!({"symbols":[asset_market("BTCUSDT","BTC"),bad,asset_market("ETHUSDT","ETH")]}),
                    brackets: json!([{"symbol":"BADUSDT","brackets":[{"initialLeverage":value}]}]),
                }),
                Rc::new(|_, _| panic!("no external provider")),
                None,
                safe_cex_exchange::native::NativeHttpEvents::default(),
            );
            let owner=ExchangeOwner::new(venue,serde_json::from_value(json!(match venue{Venue::Futures=>"binance",Venue::PM=>"binancepm",Venue::Spot=>"binancespot"})).unwrap(),serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret","publicDataOnly":!leverage})).unwrap(),Rc::new(safe_cex_store::store::DefaultStore::new()),deps).unwrap();
            let errors = Rc::new(RefCell::new(Vec::new()));
            let capture = errors.clone();
            owner.base.on(
                "error",
                Rc::new(move |_, error| {
                    capture.borrow_mut().push(error.clone());
                    Ok(())
                }),
            );
            let exchange = sdk_class(venue, owner.clone());
            let markets = if refresh {
                run(&runtime, exchange.refresh_markets()).unwrap();
                owner.base.store.markets().to_vec()
            } else {
                run(&runtime, exchange.fetch_markets()).unwrap()
            };
            assert_eq!(
                markets
                    .iter()
                    .map(|m| m.symbol.as_str())
                    .collect::<Vec<_>>(),
                ["BTCUSDT", "ETHUSDT"],
                "bad{field} refresh={refresh}"
            );
            let errors = errors.borrow();
            assert_eq!(errors.len(), 1);
            let message = String::from_utf16(errors[0].as_str().unwrap()).unwrap();
            let expected = value
                .as_ref()
                .map_or_else(|| "undefined".into(), Value::to_string);
            assert!(
                message.contains("row 1") && message.contains(&format!("{field}={expected}")),
                "{message}"
            );
            assert_eq!(runtime.reported.borrow().len(), 1);
            owner.base.dispose().unwrap();
            count += 1;
        }
    }
    assert_eq!(count, if leverage { 8 } else { 10 });
}
#[test]
#[cfg(feature = "native")]
fn production_native_futures_market_symbol_refusals_keep_siblings() {
    market_field_refusal_case(Venue::Futures, false);
}
#[test]
#[cfg(feature = "native")]
fn production_native_pm_market_symbol_refusals_keep_siblings() {
    market_field_refusal_case(Venue::PM, false);
}
#[test]
#[cfg(feature = "native")]
fn production_native_spot_market_symbol_refusals_keep_siblings() {
    market_field_refusal_case(Venue::Spot, false);
}
#[test]
#[cfg(feature = "native")]
fn production_native_futures_market_leverage_refusals_keep_siblings() {
    market_field_refusal_case(Venue::Futures, true);
}
#[test]
#[cfg(feature = "native")]
fn production_native_pm_market_leverage_refusals_keep_siblings() {
    market_field_refusal_case(Venue::PM, true);
}

#[test]
#[cfg(feature = "native")]
fn production_market_field_source_preserves_raw_symbol_and_leverage() {
    let mut count = 0;
    for venue in ["futures", "pm", "spot"] {
        let result = std::process::Command::new("node")
            .arg(format!(
                "{}/scripts/public-rest.mjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .args([venue, "markets", "field-rows"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let source: Value = serde_json::from_slice(&result.stdout).unwrap();
        let rows = source.as_array().unwrap();
        assert_eq!(rows.len(), if venue == "spot" { 5 } else { 9 });
        count += rows.len();
    }
    assert_eq!(count, 23, "source raw symbols and leverage cases");
}

#[test]
#[cfg(feature = "native")]
fn production_depth_binding_preserves_the_source_type_error_and_recovers() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::{
        RestAdapter, RestRequest, RestResponse, RestResponseBody, socket::SocketFrame,
    };
    struct DepthHttp(std::sync::atomic::AtomicU64);
    impl RestAdapter for DepthHttp {
        fn execute(
            &self,
            request: RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
            assert!(request.url.split('?').next().unwrap().ends_with("/depth"));
            Box::pin(std::future::ready(Ok(RestResponse {
                status: 200,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(
                    serde_json::to_vec(
                        &json!({"lastUpdateId":self.0.load(std::sync::atomic::Ordering::SeqCst),"bids":[["99","2"]],"asks":[["101","3"]]}),
                    )
                    .unwrap(),
                ),
            })))
        }
    }
    struct DepthSocket {
        events: Rc<RefCell<std::collections::VecDeque<WsEvent>>>,
        sent: RefCell<Vec<String>>,
    }
    impl WsSocket for DepthSocket {
        fn ready_state(&self) -> u8 {
            1
        }
        fn next_event(&self) -> ExchangeFuture<'static, WsEvent> {
            let events = self.events.clone();
            Box::pin(std::future::poll_fn(move |_| {
                events
                    .borrow_mut()
                    .pop_front()
                    .map_or(Poll::Pending, |event| Poll::Ready(Ok(event)))
            }))
        }
        fn send(&self, text: &str) -> Result<(), ExchangeError> {
            self.sent.borrow_mut().push(text.into());
            Ok(())
        }
        fn close(&self) {}
        fn terminate(&self) {}
        fn reconnect(&self) {}
        fn is_reconnecting(&self) -> bool {
            false
        }
    }
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let runtime = Rc::new(Runtime::default());
        let socket = Rc::new(DepthSocket {
            events: Rc::new(RefCell::new(std::collections::VecDeque::from([
                WsEvent::Open,
            ]))),
            sent: RefCell::default(),
        });
        *runtime.dial_socket.borrow_mut() = Some(socket.clone());
        let http = std::sync::Arc::new(DepthHttp(std::sync::atomic::AtomicU64::new(100)));
        let deps = safe_cex_venue_binance::native_host::native_dependencies(
            runtime.clone(),
            http.clone(),
            Rc::new(|_, _| panic!("no external data configured")),
            None,
            safe_cex_exchange::native::NativeHttpEvents::default(),
        );
        let name = match venue {
            Venue::Futures => "binance",
            Venue::PM => "binancepm",
            Venue::Spot => "binancespot",
        };
        let owner = ExchangeOwner::new(
            venue,
            serde_json::from_value(json!(name)).unwrap(),
            serde_json::from_value(json!({"key":"key","secret":"secret"})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            deps,
        )
        .unwrap();
        let exchange = sdk_class(venue, owner.clone());
        owner.public_socket().connect_and_subscribe().unwrap();
        for _ in 0..8 {
            runtime.pump();
        }
        let books = Rc::new(RefCell::new(Vec::new()));
        let seen = books.clone();
        let cleanup = exchange
            .listen_order_book(
                "BTCUSDT".into(),
                Rc::new(move |book| seen.borrow_mut().push(book)),
            )
            .unwrap();
        assert!(
            socket
                .sent
                .borrow()
                .iter()
                .any(|wire| wire.contains("btcusdt@depth"))
        );
        let push = |frame: Value| {
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
            for _ in 0..12 {
                runtime.pump();
            }
        };
        push(
            json!({"e":"depthUpdate","s":"BTCUSDT","U":101,"u":101,"pu":100,"b":[["99","4"]],"a":[]}),
        );
        assert_eq!(
            books.borrow().len(),
            1,
            "native snapshot/buffered delta callback {venue:?}"
        );
        assert_eq!(books.borrow()[0].bids[0].amount, 4.0);
        let malformed =
            json!({"e":"depthUpdate","s":"BTCUSDT","U":102,"u":102,"pu":101,"b":[null],"a":[]});
        let error = owner
            .public_socket()
            .base()
            .on_message(&SocketFrame::Text(malformed.to_string()))
            .unwrap_err();
        assert_eq!(
            error.name, "TypeError",
            "native depth preserves source TypeError {venue:?}"
        );
        assert_eq!(
            error.message,
            "Cannot read properties of null (reading '0')"
        );
        assert_eq!(
            books.borrow().len(),
            1,
            "malformed delta must not emit a book"
        );
        push(
            json!({"e":"depthUpdate","s":"BTCUSDT","U":102,"u":102,"pu":101,"b":[["99","5"]],"a":[]}),
        );
        assert_eq!(
            books.borrow().len(),
            2,
            "next valid delta still reaches listener"
        );
        assert_eq!(books.borrow()[1].bids[0].total, 5.0);
        http.0.store(104, std::sync::atomic::Ordering::SeqCst);
        push(
            json!({"e":"depthUpdate","s":"BTCUSDT","U":105,"u":105,"pu":104,"b":[["99","8"]],"a":[]}),
        );
        assert_eq!(
            books.borrow().len(),
            3,
            "sequence gap fetches a fresh native snapshot"
        );
        assert_eq!(books.borrow()[2].bids[0].amount, 8.0);
        cleanup().unwrap();
        exchange.dispose().unwrap();
    }
}

#[cfg(feature = "native")]
#[path = "../../parity/test-support/ticker_coercion.rs"]
mod ticker_coercion;

#[cfg(feature = "native")]
fn native_ticker_coercion(venue: Venue, name: &str, count: usize) {
    use safe_cex_store::{Store, StoreUpdate};
    let source = ticker_coercion::source(name);
    assert_eq!(source["cases"].as_array().unwrap().len(), count);
    let owner = production_native_owner(venue);
    for case in source["cases"].as_array().unwrap() {
        Store::update(
            owner.base.store.as_ref(),
            StoreUpdate {
                tickers: Some(ticker_coercion::seed(name)),
                positions: Some(vec![]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let socket = if venue != Venue::Spot && case["channel"] != "bookTicker" {
            owner
                .public
                .market_websocket
                .get()
                .expect("production market socket")
        } else {
            &owner.public
        };
        for frame in case["frames"].as_array().unwrap() {
            socket.on_message(frame.as_str().unwrap()).unwrap();
        }
        ticker_coercion::assert_native(&owner.base.store.tickers(), case);
    }
    owner.base.dispose().unwrap();
}

#[test]
#[cfg(feature = "native")]
fn production_ticker_coercion_futures_matches_constructed_typescript() {
    native_ticker_coercion(Venue::Futures, "binance", 80);
}
#[test]
#[cfg(feature = "native")]
fn production_ticker_coercion_pm_matches_constructed_typescript() {
    native_ticker_coercion(Venue::PM, "binancepm", 80);
}
#[test]
#[cfg(feature = "native")]
fn production_ticker_coercion_spot_matches_constructed_typescript() {
    native_ticker_coercion(Venue::Spot, "binancespot", 48);
}

#[test]
#[cfg(feature = "native")]
fn production_native_request_weight_callbacks_observe_success_and_rejection() {
    use safe_cex_transport::{
        RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody,
    };
    use std::sync::{Arc, Mutex};
    struct Response {
        status: u16,
        header: &'static str,
        value: &'static str,
        requests: Arc<Mutex<Vec<RestRequest>>>,
    }
    impl RestAdapter for Response {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.requests.lock().unwrap().push(request);
            let response = RestResponse {
                status: self.status,
                reason: None,
                headers: [(self.header.into(), self.value.into())]
                    .into_iter()
                    .collect(),
                body: RestResponseBody::Encoded(if self.status == 200 {
                    b"[]".to_vec()
                } else {
                    br#"{"code":-1003,"msg":"weight limit"}"#.to_vec()
                }),
            };
            Box::pin(async move { Ok(response) })
        }
    }
    for (status, header, value, duration) in [
        (200, "x-mbx-used-weight-1m", "1920", 60_000.0),
        (429, "retry-after", "2.5", 2_500.0),
        (429, "retry-after", "invalid", 60_000.0),
    ] {
        let runtime = Rc::new(Runtime::default());
        runtime.now.set(1_000.0);
        let requests = Arc::<Mutex<Vec<RestRequest>>>::default();
        let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
            runtime.clone(),
            Arc::new(Response {
                status,
                header,
                value,
                requests: requests.clone(),
            }),
            Rc::new(|_, _| panic!("no external provider is configured")),
            None,
            safe_cex_exchange::native::NativeHttpEvents::default(),
        );
        let imports = dependencies.imports.clone();
        let owner = ExchangeOwner::new(
            Venue::Futures,
            safe_cex_types::ExchangeName::Binance,
            serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            dependencies,
        )
        .unwrap();
        let request = imports
            .request(
                owner.clients.get("xhr").unwrap().clone(),
                safe_cex_venue_binance::exchange_classes::HttpMethod::Get,
                "/fapi/v1/openOrders",
                None,
            )
            .unwrap();
        let result = run(&runtime, request).result;
        if status == 200 {
            assert!(
                matches!(result.unwrap(), EventValue::Array(ref rows) if rows.borrow().is_empty())
            );
        } else {
            let error = result.unwrap_err();
            let response = error
                .response
                .as_ref()
                .expect("the original HTTP response is retained");
            assert_eq!(response["status"], 429);
            assert_eq!(response["data"]["code"], -1003);
        }
        assert_eq!(
            owner.paused_until.get(),
            1_000.0 + duration,
            "native request-weight observer must run on HTTP status {status}"
        );
        assert!(owner.paused());
        let requests = requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let (path, query) = requests[0]
            .url
            .split_once('?')
            .expect("the native signer serializes its query");
        assert_eq!(path, "/fapi/v1/openOrders");
        assert!(query.contains("timestamp="));
        assert!(query.contains("signature="));
        owner.dispose().unwrap();
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_position_fetch_starter_runs_real_http_and_store() {
    use safe_cex_transport::{
        RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody,
    };
    use safe_cex_venue_binance::position_refresh::PositionFetchStarter;
    use std::sync::{Arc, Mutex};
    struct Positions(Arc<Mutex<Vec<String>>>);
    impl RestAdapter for Positions {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.0.lock().unwrap().push(request.url);
            Box::pin(async {
                Ok(RestResponse {
                    status: 200,
                    reason: None,
                    headers: Default::default(),
                    body: RestResponseBody::Encoded(b"[]".to_vec()),
                })
            })
        }
    }
    for delayed in [false, true] {
        let runtime = Rc::new(Runtime::default());
        let requests = Arc::<Mutex<Vec<String>>>::default();
        let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
            runtime.clone(),
            Arc::new(Positions(requests.clone())),
            Rc::new(|_, _| panic!("no external provider is configured")),
            None,
            safe_cex_exchange::native::NativeHttpEvents::default(),
        );
        let store = Rc::new(safe_cex_store::store::DefaultStore::new());
        let owner = ExchangeOwner::new(
            Venue::Futures,
            safe_cex_types::ExchangeName::Binance,
            serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
            store,
            dependencies,
        )
        .unwrap();
        owner
            .private
            .position
            .last_fetch_time
            .set(if delayed { 0.0 } else { -10_000.0 });
        owner.private.position.pending_events.set(true);
        let private = owner.private.clone();
        let positions_before = owner.base.store.update_ids().positions;
        let starter: PositionFetchStarter<Failure> = Rc::new(move || private.throttled_positions());
        starter().unwrap();
        if delayed {
            runtime.pump();
            assert!(owner.private.position.scheduled.get());
            assert!(requests.lock().unwrap().is_empty());
            assert_eq!(owner.base.store.update_ids().positions, positions_before);
            let deadline = owner.private.position.throttle_ms.get();
            assert_eq!(*runtime.sleeps.borrow(), [deadline as u64]);
            runtime.now.set(deadline);
            runtime.sleep_clock.set(deadline);
        }
        for _ in 0..32 {
            runtime.pump();
        }
        let mut paths: Vec<_> = requests
            .lock()
            .unwrap()
            .iter()
            .map(|url| url.split('?').next().unwrap().to_owned())
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            ["/fapi/v1/symbolConfig", "/fapi/v3/positionRisk"],
            "the actual starter reaches both configured native REST requests"
        );
        assert_eq!(
            owner.base.store.update_ids().positions,
            positions_before + 1.0,
            "the actual refresh publishes one complete position snapshot"
        );
        assert!(!owner.private.position.scheduled.get());
        assert!(!owner.private.position.pending_events.get());
        assert!(!owner.private.position.in_flight.get());
        assert_eq!(
            owner.private.position.last_fetch_time.get(),
            runtime.now.get()
        );
        owner.dispose().unwrap();
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_class_future_completes_store_after_held_http() {
    use safe_cex_transport::{
        RestAdapter, RestFuture, RestRequest, RestResponse, RestResponseBody,
    };
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    struct Held {
        released: Arc<AtomicBool>,
        completed: Arc<AtomicUsize>,
        requests: Arc<Mutex<Vec<String>>>,
    }
    impl RestAdapter for Held {
        fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
            self.requests.lock().unwrap().push(request.url);
            let released = self.released.clone();
            let completed = self.completed.clone();
            Box::pin(std::future::poll_fn(move |_| {
                if released.load(Ordering::SeqCst) {
                    completed.fetch_add(1, Ordering::SeqCst);
                    Poll::Ready(Ok(RestResponse {
                        status: 200,
                        reason: None,
                        headers: Default::default(),
                        body: RestResponseBody::Encoded(b"[]".to_vec()),
                    }))
                } else {
                    Poll::Pending
                }
            }))
        }
    }
    let runtime = Rc::new(Runtime::default());
    let released = Arc::new(AtomicBool::new(false));
    let completed = Arc::new(AtomicUsize::new(0));
    let requests = Arc::<Mutex<Vec<String>>>::default();
    let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
        runtime.clone(),
        Arc::new(Held {
            released: released.clone(),
            completed: completed.clone(),
            requests: requests.clone(),
        }),
        Rc::new(|_, _| panic!("no external provider is configured")),
        None,
        safe_cex_exchange::native::NativeHttpEvents::default(),
    );
    let owner = ExchangeOwner::new(
        Venue::Futures,
        safe_cex_types::ExchangeName::Binance,
        serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"})).unwrap(),
        Rc::new(safe_cex_store::store::DefaultStore::new()),
        dependencies,
    )
    .unwrap();
    owner.private.position.last_fetch_time.set(-10_000.0);
    owner.private.position.pending_events.set(true);
    let before = owner.base.store.update_ids().positions;
    owner.private.throttled_positions().unwrap();
    for _ in 0..32 {
        runtime.pump();
    }
    assert_eq!(owner.base.store.update_ids().positions, before);
    assert!(owner.private.position.in_flight.get());
    assert_eq!(requests.lock().unwrap().len(), 2);
    assert_eq!(completed.load(Ordering::SeqCst), 0);
    released.store(true, Ordering::SeqCst);
    for _ in 0..32 {
        runtime.pump();
    }
    assert_eq!(
        owner.base.store.update_ids().positions,
        before + 1.0,
        "the detached native class future publishes its completed position snapshot"
    );
    assert_eq!(completed.load(Ordering::SeqCst), 2);
    assert!(!owner.private.position.in_flight.get());
    assert!(!owner.private.position.pending_events.get());
    assert!(runtime.reported.borrow().is_empty());
    owner.dispose().unwrap();
}

#[test]
#[cfg(feature = "native")]
fn production_native_ticker_capture_aliases_retain_cells_and_replace_generations() {
    use safe_cex_store::{Store, StoreUpdate};
    use safe_cex_venue_binance::ticker_capture::{Capture, Dictionary, TickerReference};

    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        let owner = production_native_owner(venue);
        let capture = Capture::new(owner.base.store.clone());
        let initial: Dictionary = capture.dictionary();
        let row: TickerReference = initial.borrow().get("BTCUSDT").unwrap().clone();
        assert!(
            Rc::ptr_eq(&initial, &capture.dictionary()),
            "same dictionary generation {venue:?}"
        );
        assert_eq!(row.borrow().last.to_bits(), 100.0_f64.to_bits());

        owner
            .public
            .on_message(r#"{"e":"bookTicker","s":"BTCUSDT","b":"-0","a":"123.25"}"#)
            .unwrap();
        let patched: Dictionary = capture.dictionary();
        assert!(
            Rc::ptr_eq(&initial, &patched),
            "patch retains Dictionary identity {venue:?}"
        );
        assert!(
            Rc::ptr_eq(&row, patched.borrow().get("BTCUSDT").unwrap()),
            "patch retains TickerReference identity {venue:?}"
        );
        assert_eq!(
            row.borrow().bid.to_bits(),
            (-0.0_f64).to_bits(),
            "retained row sees native signed zero {venue:?}"
        );
        assert_eq!(row.borrow().ask.to_bits(), 123.25_f64.to_bits());
        assert_eq!(
            owner.base.store.tickers()[0].bid.to_bits(),
            (-0.0_f64).to_bits(),
            "real Store receives native frame {venue:?}"
        );

        let mut replacement = owner.base.store.tickers()[0].clone();
        replacement.id = "fresh-BTCUSDT".into();
        replacement.last = 77.5;
        Store::update(
            owner.base.store.as_ref(),
            StoreUpdate {
                tickers: Some(vec![replacement]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let replaced: Dictionary = capture.dictionary();
        let fresh: TickerReference = replaced.borrow().get("BTCUSDT").unwrap().clone();
        assert!(
            !Rc::ptr_eq(&initial, &replaced),
            "replacement creates Dictionary generation {venue:?}"
        );
        assert!(
            !Rc::ptr_eq(&row, &fresh),
            "replacement creates TickerReference generation {venue:?}"
        );
        assert_eq!(fresh.borrow().id, "fresh-BTCUSDT");
        assert_eq!(fresh.borrow().last.to_bits(), 77.5_f64.to_bits());
        assert_eq!(
            row.borrow().last.to_bits(),
            100.0_f64.to_bits(),
            "captured prior generation retains original fields {venue:?}"
        );

        owner
            .public
            .on_message(r#"{"e":"bookTicker","s":"BTCUSDT","b":"6.25","a":"7.5"}"#)
            .unwrap();
        assert!(Rc::ptr_eq(&replaced, &capture.dictionary()));
        assert_eq!(
            fresh.borrow().bid.to_bits(),
            6.25_f64.to_bits(),
            "native class patches replacement id {venue:?}"
        );
        assert_eq!(
            owner.base.store.tickers()[0].bid.to_bits(),
            6.25_f64.to_bits()
        );
        assert_eq!(
            row.borrow().bid.to_bits(),
            (-0.0_f64).to_bits(),
            "prior row stays detached after replacement {venue:?}"
        );
        owner.base.dispose().unwrap();
    }
}

#[test]
#[cfg(feature = "native")]
fn production_native_public_rest_future_waits_and_preserves_candles_and_rejection() {
    use safe_cex_transport::{RestAdapter, RestRequest, RestResponse, RestResponseBody};
    use std::sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    };
    struct Held {
        release: Arc<AtomicBool>,
        requests: Arc<Mutex<Vec<RestRequest>>>,
        status: u16,
    }
    impl RestAdapter for Held {
        fn execute(
            &self,
            request: RestRequest,
        ) -> safe_cex_transport::RestFuture<'_, RestResponse> {
            self.requests.lock().unwrap().push(request.clone());
            let mut response = Some(RestResponse {
                status: self.status,
                reason: None,
                headers: Default::default(),
                body: RestResponseBody::Encoded(if self.status == 200 {
                    br#"[[60000,"-0","101.25","-2.5","9.75","3.125"],[120000,"7.5","8.25","6.125","7.75","0"]]"#.to_vec()
                } else {
                    br#"{"code":-1121,"msg":"Invalid symbol."}"#.to_vec()
                }),
            });
            Box::pin(std::future::poll_fn(move |_| {
                if self.release.load(Ordering::SeqCst) {
                    let response = response.take().expect("one HTTP settlement");
                    Poll::Ready(if response.status == 200 {
                        Ok(response)
                    } else {
                        Err(safe_cex_transport::RestFailure {
                            code: Some("ERR_BAD_REQUEST".into()),
                            message: "Request failed with status code 400".into(),
                            transport_error: None,
                            response: Some(Box::new(response)),
                            request: Box::new(request.clone()),
                            context: None,
                        })
                    })
                } else {
                    Poll::Pending
                }
            }))
        }
    }
    for venue in [Venue::Futures, Venue::PM, Venue::Spot] {
        for status in [200, 400] {
            let runtime = Rc::new(Runtime::default());
            let release = Arc::new(AtomicBool::new(false));
            let requests = Arc::<Mutex<Vec<RestRequest>>>::default();
            let dependencies = safe_cex_venue_binance::native_host::native_dependencies(
                runtime.clone(),
                Arc::new(Held {
                    release: release.clone(),
                    requests: requests.clone(),
                    status,
                }),
                Rc::new(|_, _| panic!("no external candle provider configured")),
                None,
                safe_cex_exchange::native::NativeHttpEvents::default(),
            );
            let name = match venue {
                Venue::Futures => "binance",
                Venue::PM => "binancepm",
                Venue::Spot => "binancespot",
            };
            let owner = ExchangeOwner::new(
                venue,
                serde_json::from_value(json!(name)).unwrap(),
                serde_json::from_value(json!({"key":"fixture-key","secret":"fixture-secret"}))
                    .unwrap(),
                Rc::new(safe_cex_store::store::DefaultStore::new()),
                dependencies,
            )
            .unwrap();
            let options = EventValue::from_json(
                json!({"symbol":"BTCUSDT","interval":"1m","from":123,"to":180000,"limit":2}),
            );
            let mut future = Box::pin(owner.fetch_ohlcv(options));
            for _ in 0..3 {
                assert!(
                    future
                        .as_mut()
                        .poll(&mut Context::from_waker(Waker::noop()))
                        .is_pending(),
                    "real public REST future must stay pending before HTTP release {venue:?} {status}"
                );
                runtime.pump();
            }
            {
                let requests = requests.lock().unwrap();
                assert_eq!(requests.len(), 1, "one real native GET {venue:?} {status}");
                assert_eq!(requests[0].method, "get");
                let (path, query) = requests[0].url.split_once('?').unwrap();
                assert_eq!(
                    path,
                    if venue == Venue::Spot {
                        "/api/v3/klines"
                    } else {
                        "/fapi/v1/klines"
                    }
                );
                for pair in [
                    "symbol=BTCUSDT",
                    "interval=1m",
                    "startTime=60000",
                    "endTime=180000",
                    "limit=2",
                ] {
                    assert!(
                        query.split('&').any(|part| part == pair),
                        "missing source query {pair}: {query}"
                    );
                }
            }
            release.store(true, Ordering::SeqCst);
            let result = run(&runtime, future);
            if status == 200 {
                let rows = result.unwrap();
                assert_eq!(rows.read("length").unwrap().number().unwrap(), 2.0);
                for (index, expected) in [
                    [60.0, -0.0, 101.25, -2.5, 9.75, 3.125],
                    [120.0, 7.5, 8.25, 6.125, 7.75, 0.0],
                ]
                .into_iter()
                .enumerate()
                {
                    let row = rows.read(&index.to_string()).unwrap();
                    for (key, number) in ["timestamp", "open", "high", "low", "close", "volume"]
                        .into_iter()
                        .zip(expected)
                    {
                        let EventValue::Number(actual) = row.read(key).unwrap() else {
                            panic!("native numeric candle field {key}");
                        };
                        assert_eq!(
                            actual.to_bits(),
                            f64::to_bits(number),
                            "{venue:?} candle {index} {key}"
                        );
                    }
                    assert!(matches!(
                        row.read("lastSide").unwrap(),
                        EventValue::Undefined
                    ));
                }
            } else {
                let error = result.unwrap_err();
                let response = error
                    .response
                    .as_ref()
                    .expect("HTTP rejection retains response");
                assert_eq!(response["status"], 400);
                assert_eq!(response["data"]["code"], -1121);
                assert_eq!(response["data"]["msg"], "Invalid symbol.");
            }
            assert_eq!(
                requests.lock().unwrap().len(),
                1,
                "settlement must not resend"
            );
            owner.dispose().unwrap();
        }
    }
}

#[cfg(feature = "native")]
#[path = "support/order_throttle_binding.rs"]
mod order_throttle_binding;
