use indexmap::IndexMap;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreMutationError, StoreUpdate};
use safe_cex_types::{Market, OrderSide, OrderStatus, OrderType, StoreSubscriberError};
use safe_cex_venue_coinbase::{
    classes::{CoinbasePrivateClassHost, CoinbasePrivateWebsocket},
    market_position_reads::ReadMarket,
    order_sync::{PrivateOrderFill, PrivateOrderMarkets, SyncedOrderRoot, SyncedOrders},
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    private_stream_dispatch::*,
    public_stream_dispatch::{PublicSocket, PublicSocketListener},
    starbase_auth::CoinbaseStarbasePromise as Promise,
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    error::Error,
    process::Command,
    rc::{Rc, Weak},
};
#[path = "support/capture_metadata.rs"]
mod capture_metadata;
#[path = "support/microtasks.rs"]
mod microtasks;
use microtasks::{Microtasks, completed, is_pending};
type Class = CoinbasePrivateWebsocket<Host>;
type Action = Box<dyn FnOnce()>;
#[derive(Clone)]
enum Trace {
    Received,
    Diagnostic(String),
    Health,
    Error(Option<Frame>),
    Loaded(bool, usize),
    Update(Vec<String>, bool, usize),
    Add(Vec<String>),
    Fill(PrivateOrderFill<Frame>),
    Sign(Option<Value>, Option<Value>, bool, usize),
    Send(PrivateSubscription),
}
struct Host {
    owner: RefCell<Weak<Class>>,
    native_store: RefCell<Option<Rc<DefaultStore>>>,
    publication_failure: RefCell<Option<PortfolioFailure>>,
    trace: RefCell<Vec<Trace>>,
    disposed: Cell<bool>,
    options: PrivateOptions,
    markets: PrivateOrderMarkets,
    loaded: RefCell<Frame>,
    orders: RefCell<SyncedOrders<Frame>>,
    snapshots: RefCell<Vec<SyncedOrders<Frame>>>,
    fills: RefCell<Vec<PrivateOrderFill<Frame>>>,
    parsed: RefCell<Option<Frame>>,
    loaded_hook: RefCell<Option<Action>>,
    store_hook: RefCell<Option<Action>>,
    fill_hook: RefCell<Option<Action>>,
    resolve_hook: RefCell<Option<Action>>,
    signer: RefCell<Promise<Result<Value, PortfolioFailure>>>,
}
impl Host {
    fn class(&self) -> Rc<Class> {
        self.owner.borrow().upgrade().unwrap()
    }
    fn state(&self) -> (bool, usize) {
        let class = self.class();
        let snapshot = class.snapshot.borrow();
        (
            snapshot.has_pending_snapshot(),
            snapshot.pending_raw_order_count(),
        )
    }
    fn action(slot: &RefCell<Option<Action>>) {
        let action = slot.borrow_mut().take();
        if let Some(action) = action {
            action();
        }
    }
    fn record(&self, trace: Trace) {
        self.trace.borrow_mut().push(trace);
    }
}
impl PrivateStreamHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn options(&self) -> PrivateOptions {
        self.options.clone()
    }
    fn exchange_name(&self) -> String {
        "Coinbase".into()
    }
    fn testnet(&self) -> bool {
        false
    }
    fn resolve_private_route(
        &self,
        _: String,
        _: PrivateOptions,
    ) -> Result<PrivateStreamRoute, PortfolioFailure> {
        panic!("unexpected connect route")
    }
    fn create_websocket(&self, _: Option<Value>) -> Result<PublicSocket, PortfolioFailure> {
        panic!("unexpected socket create")
    }
    fn add_listener(
        &self,
        _: PublicSocket,
        _: PublicSocketListener,
    ) -> Result<(), PortfolioFailure> {
        panic!("unexpected socket listener")
    }
    fn reset_order_snapshot_assembly(&self) -> Result<(), PortfolioFailure> {
        panic!("class must reset its own snapshot")
    }
    fn close_socket(&self, _: PublicSocket) -> Result<(), PortfolioFailure> {
        panic!("unexpected class socket close")
    }
    fn send_with_log(&self, value: PrivateSubscription) -> Result<(), PortfolioFailure> {
        self.record(Trace::Send(value));
        Ok(())
    }
    fn sign_local(
        &self,
        key: Option<Value>,
        secret: Option<Value>,
    ) -> Promise<Result<Value, PortfolioFailure>> {
        let (active, count) = self.state();
        self.record(Trace::Sign(key, secret, active, count));
        self.signer.borrow().clone()
    }
    fn track_last_received(&self) -> Result<(), PortfolioFailure> {
        self.record(Trace::Received);
        Ok(())
    }
    fn emit_ws_diagnostic_receive(&self, raw: &str) -> Result<(), PortfolioFailure> {
        self.record(Trace::Diagnostic(raw.into()));
        Ok(())
    }
    fn json_parse(&self, raw: &str) -> Result<Frame, PortfolioFailure> {
        let parsed = Frame::from_json(&serde_json::from_str(raw).unwrap());
        *self.parsed.borrow_mut() = Some(parsed.clone());
        Ok(parsed)
    }
    fn mark_private_user_stream_auth_failed(
        &self,
        _: Option<Frame>,
    ) -> Result<(), PortfolioFailure> {
        panic!("unexpected auth failure")
    }
    fn mark_private_user_stream_message_received(&self) -> Result<(), PortfolioFailure> {
        self.record(Trace::Health);
        Ok(())
    }
    fn emit_and_log_error(&self, message: Option<Frame>) -> Result<(), PortfolioFailure> {
        self.record(Trace::Error(message));
        Ok(())
    }
    fn handle_user_event_orders(&self, _: Frame) -> Result<(), PortfolioFailure> {
        panic!("class must map into its own snapshot")
    }
    fn handle_pong_event(&self) -> Result<(), PortfolioFailure> {
        panic!("unexpected class pong")
    }
}
impl CoinbasePrivateClassHost for Host {
    fn private_order_markets(&self) -> Result<PrivateOrderMarkets, PortfolioFailure> {
        Ok(self.markets.clone())
    }
    fn resolve_private_order_market(
        &self,
        id: String,
        markets: PrivateOrderMarkets,
    ) -> Result<Option<ReadMarket>, PortfolioFailure> {
        assert!(
            Rc::ptr_eq(&markets, &self.markets),
            "class native resolver retains Store market map"
        );
        Self::action(&self.resolve_hook);
        let alias =
            safe_cex_venue_coinbase::utils::get_coinbase_usd_equivalent_alias_product_id(&id);
        let markets = markets.borrow();
        let exact = markets.get(&id).cloned();
        let alias = alias.and_then(|id| markets.get(&id).cloned());
        Ok(if id.ends_with("-USD") && alias.is_some() {
            alias
        } else {
            exact.or(alias)
        })
    }
    fn private_order_console_error(&self, _: &str, _: &str) -> Result<(), PortfolioFailure> {
        panic!("unexpected enum formatter fallback")
    }
    fn private_order_now_ms(&self) -> Result<f64, PortfolioFailure> {
        Ok(37.0)
    }
    fn private_orders_loaded(&self) -> Result<Frame, PortfolioFailure> {
        let (active, count) = self.state();
        self.record(Trace::Loaded(active, count));
        assert!(!active, "class native reset precedes loaded getter");
        assert_eq!(
            count, 0,
            "class native raw count reset precedes loaded getter"
        );
        Self::action(&self.loaded_hook);
        Ok(self.loaded.borrow().clone())
    }
    fn publish_private_order_snapshot(
        &self,
        orders: SyncedOrders<Frame>,
        loaded: Frame,
    ) -> Result<(), PortfolioFailure> {
        *self.orders.borrow_mut() = orders.clone();
        *self.loaded.borrow_mut() = loaded.clone();
        self.snapshots.borrow_mut().push(orders.clone());
        let (active, count) = self.state();
        self.record(Trace::Update(
            orders
                .borrow()
                .iter()
                .map(|o| o.borrow().fields.id.clone())
                .collect(),
            active,
            count,
        ));
        let native_store = self.native_store.borrow().clone();
        if let Some(store) = native_store {
            let native_orders = orders
                .borrow()
                .iter()
                .map(|order| order.borrow().fields.clone())
                .collect();
            let loaded = safe_cex_types::StoreDataLoaded {
                balance: matches!(loaded.get("balance"), Some(Frame::Bool(true))),
                orders: matches!(loaded.get("orders"), Some(Frame::Bool(true))),
                markets: matches!(loaded.get("markets"), Some(Frame::Bool(true))),
                tickers: matches!(loaded.get("tickers"), Some(Frame::Bool(true))),
                positions: matches!(loaded.get("positions"), Some(Frame::Bool(true))),
            };
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    orders: Some(native_orders),
                    loaded: Some(loaded),
                    ..Default::default()
                },
                false,
            )
            .map_err(|error| {
                let failure = Rc::new(PortfolioThrown::from(error));
                *self.publication_failure.borrow_mut() = Some(failure.clone());
                failure
            })?;
        }
        Self::action(&self.store_hook);
        Ok(())
    }
    fn add_private_orders(&self, orders: SyncedOrders<Frame>) -> Result<(), PortfolioFailure> {
        *self.orders.borrow_mut() = orders.clone();
        self.record(Trace::Add(
            orders
                .borrow()
                .iter()
                .map(|o| o.borrow().fields.id.clone())
                .collect(),
        ));
        Self::action(&self.store_hook);
        Ok(())
    }
    fn emit_private_order_fill(
        &self,
        fill: PrivateOrderFill<Frame>,
    ) -> Result<(), PortfolioFailure> {
        self.fills.borrow_mut().push(fill.clone());
        self.record(Trace::Fill(fill));
        Self::action(&self.fill_hook);
        Ok(())
    }
}
fn instance(queue: &Microtasks) -> Rc<Class> {
    let market:Market=serde_json::from_value(json!({"id":"BTC-USD","symbol":"BTC-USDC","base":"BTC","quote":"USD","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":1}},"isSpot":false,"isExpiring":true,"contractSize":0.5})).unwrap();
    let host = Host {
        owner: RefCell::new(Weak::new()),
        native_store: RefCell::new(None),
        publication_failure: RefCell::new(None),
        trace: RefCell::new(Vec::new()),
        disposed: Cell::new(false),
        options: Rc::new(RefCell::new(PrivateStreamOptions {
            key: Some(json!("key")),
            secret: Some(json!("secret")),
            remote_signer: None,
        })),
        markets: Rc::new(RefCell::new(IndexMap::from([(
            "BTC-USDC".into(),
            Rc::new(RefCell::new(market)),
        )]))),
        loaded: RefCell::new(Frame::from_json(
            &json!({"orders":false,"markets":true,"extra":{"retained":true}}),
        )),
        orders: RefCell::new(Rc::new(RefCell::new(Vec::new()))),
        snapshots: RefCell::new(Vec::new()),
        fills: RefCell::new(Vec::new()),
        parsed: RefCell::new(None),
        loaded_hook: RefCell::new(None),
        store_hook: RefCell::new(None),
        fill_hook: RefCell::new(None),
        resolve_hook: RefCell::new(None),
        signer: RefCell::new(Promise::resolved(Ok(json!("jwt")))),
    };
    let class = Rc::new(Class::new(
        host,
        Frame::from_json(&json!({"parent":true})),
        queue.spawner(),
    ));
    *class.receiver.host.owner.borrow_mut() = Rc::downgrade(&class);
    class
}
fn raw(id: &str) -> Value {
    json!({"order_id":id,"client_order_id":format!("client-{id}"),"product_id":"BTC-USD","cumulative_quantity":"1","leaves_quantity":"2","status":"OPEN","order_type":"Limit","order_side":"BUY","limit_price":"10","avg_price":"11"})
}
fn send(class: &Class, kind: &str, orders: Vec<Value>) {
    class
        .on_message(&json!({"channel":"user","events":[{"type":kind,"orders":orders}]}).to_string())
        .unwrap();
}
fn pending(class: &Class) -> Vec<SyncedOrderRoot<Frame>> {
    class
        .snapshot
        .borrow()
        .pending_snapshot_orders()
        .expect("class native pending snapshot map")
}
fn same_frame(a: &Frame, b: &Frame) -> bool {
    match (a, b) {
        (Frame::Object(a), Frame::Object(b)) => Rc::ptr_eq(a, b),
        (Frame::Array(a), Frame::Array(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn set(value: &Frame, key: &str, next: Frame) {
    let Frame::Object(object) = value else {
        panic!("native frame object")
    };
    object
        .borrow_mut()
        .insert(key.encode_utf16().collect(), next);
}
fn bit(value: f64, e: &Value, label: &str) {
    assert_eq!(
        format!("{:016x}", value.to_bits()),
        e["$bits"].as_str().unwrap(),
        "{label}"
    );
}
fn frame(value: &Frame, e: &Value) {
    match value {
        Frame::Undefined => assert_eq!(e["$undefined"], true, "native undefined"),
        Frame::Null => assert!(e.is_null()),
        Frame::Bool(v) => assert_eq!(Some(*v), e.as_bool()),
        Frame::Number(v) => bit(*v, e, "class native Frame number"),
        Frame::String(v) => {
            if let Some(units) = e.get("$utf16") {
                assert_eq!(
                    v.iter().map(|unit| u64::from(*unit)).collect::<Vec<_>>(),
                    units
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|unit| unit.as_u64().unwrap())
                        .collect::<Vec<_>>()
                );
            } else {
                assert!(v.iter().copied().eq(e.as_str().unwrap().encode_utf16()));
            }
        }
        Frame::Array(v) => {
            let v = v.borrow();
            let e = e.as_array().unwrap();
            assert_eq!(v.len(), e.len());
            for (v, e) in v.iter().zip(e) {
                frame(v, e);
            }
        }
        Frame::Object(v) => {
            let v = v.borrow();
            let e = e.as_object().unwrap();
            assert_eq!(v.len(), e.len());
            for (k, e) in e {
                frame(&v[&k.encode_utf16().collect::<Vec<_>>()], e);
            }
        }
    }
}
fn order(root: &SyncedOrderRoot<Frame>, e: &Value) {
    let native = root.borrow();
    let o = &native.fields;
    assert_eq!(o.id, e["id"]);
    assert_eq!(o.parent_id.as_deref(), e["parentId"].as_str());
    assert_eq!(o.symbol, e["symbol"]);
    assert_eq!(o.side, OrderSide::Buy);
    assert_eq!(o.r#type, OrderType::Limit);
    assert_eq!(
        o.status,
        if e["status"] == "closed" {
            OrderStatus::Closed
        } else {
            OrderStatus::Open
        }
    );
    for (v, key) in [
        (o.price.to_number(), "price"),
        (o.amount, "amount"),
        (o.filled, "filled"),
        (o.remaining, "remaining"),
        (o.last_update, "lastUpdate"),
    ] {
        bit(v, &e[key], "class native formatted order scalar");
    }
    bit(
        o.extra.as_ref().unwrap().remaining_native.unwrap(),
        &e["extra"]["remainingNative"],
        "class native placement units",
    );
    assert_eq!(o.reduce_only, false);
    assert!(o.info.is_none(), "raw info lives in retained native handle");
    frame(&native.info, &e["info"]);
}
fn orders(actual: &SyncedOrders<Frame>, e: &Value, floor: usize) {
    let actual = actual.borrow();
    let e = e.as_array().unwrap();
    assert_eq!(actual.len(), floor, "independent class native order floor");
    assert_eq!(e.len(), floor, "independent class source order floor");
    for (a, e) in actual.iter().zip(e) {
        order(a, e);
    }
}
fn fill(actual: &PrivateOrderFill<Frame>, e: &Value) {
    assert_eq!(actual.id, e["id"]);
    assert_eq!(actual.symbol, e["symbol"]);
    assert_eq!(actual.side, OrderSide::Buy);
    bit(actual.amount, &e["amount"], "class native fill amount");
    frame(&actual.price, &e["price"]);
}
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/class-private.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live private class: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    capture_metadata::assert_capture_metadata(&source, 1);
    let mut keys = source
        .as_object()
        .unwrap()
        .keys()
        .filter(|k| !k.starts_with("capture"))
        .map(String::as_str)
        .collect::<Vec<_>>();
    keys.sort();
    assert_eq!(
        keys,
        ["live", "map", "notification", "open", "reentry", "update"],
        "independent source private class scenario content pin"
    );
    source
}
#[test]
fn private_class_dispatch_owns_keyed_snapshots_and_resets_before_reentrant_store_and_signing() {
    let source = source();
    let queue = Microtasks::default();
    let mut boundaries = Vec::new();
    {
        let class = instance(&queue);
        let h = &class.receiver.host;
        send(
            &class,
            "snapshot",
            (0..50)
                .map(|i| raw(if i == 1 { "b" } else { "a" }))
                .collect(),
        );
        assert_eq!(
            class.snapshot.borrow().pending_raw_order_count(),
            50,
            "class native raw50 waits"
        );
        let staged = pending(&class);
        assert_eq!(staged.len(), 2, "class native Map duplicate key floor");
        assert_eq!(
            h.snapshots.borrow().len(),
            0,
            "class native raw50 does not publish"
        );
        let previous = staged[0].clone();
        let previous_b = staged[1].clone();
        let weak = Rc::downgrade(&class);
        *h.fill_hook.borrow_mut() = Some(Box::new(move || {
            let class = weak.upgrade().unwrap();
            pending(&class)[0].borrow_mut().fields.id = "renamed".into();
        }));
        let mut update = raw("a");
        update["status"] = json!("FILLED");
        update["avg_price"] = json!({"native":true});
        send(&class, "update", vec![update]);
        let staged = pending(&class);
        let replacement = staged[0].clone();
        assert!(
            !Rc::ptr_eq(&previous, &replacement),
            "class update replaces staged root"
        );
        assert_eq!(replacement.borrow().fields.id, "renamed");
        assert!(
            Rc::ptr_eq(&staged[1], &previous_b),
            "class staged second root"
        );
        let price = replacement.borrow().info.get("avg_price").unwrap();
        assert!(
            same_frame(&price, &h.fills.borrow()[0].price),
            "class fill retains native avg_price object"
        );
        let Frame::Array(events) = h.parsed.borrow().as_ref().unwrap().get("events").unwrap()
        else {
            panic!()
        };
        let Frame::Array(raw_orders) = events.borrow()[0].get("orders").unwrap() else {
            panic!()
        };
        assert!(
            same_frame(&replacement.borrow().info, &raw_orders.borrow()[0]),
            "class formatted info is exact parsed order object"
        );
        let mut next = raw("a");
        next["limit_price"] = json!("23");
        send(&class, "snapshot", vec![next]);
        let committed = h.orders.borrow().clone();
        assert_eq!(
            committed.borrow().len(),
            2,
            "class Map preserves two captured slots after id mutation"
        );
        assert_eq!(
            committed.borrow()[0].borrow().fields.id,
            "a",
            "class Map retains original captured key after id mutation"
        );
        assert!(
            Rc::ptr_eq(&committed.borrow()[1], &staged[1]),
            "class Map replacement preserves second root and position"
        );
        assert!(
            !Rc::ptr_eq(&committed.borrow()[0], &replacement),
            "class Map old-key replacement supplies new root"
        );
        assert_eq!(
            replacement.borrow().fields.id,
            "renamed",
            "class prior root stays retained"
        );
        assert!(!class.snapshot.borrow().has_pending_snapshot());
        assert_eq!(class.snapshot.borrow().pending_raw_order_count(), 0);
        orders(&committed, &source["map"]["orders"], 2);
        order(&replacement, &source["map"]["staged"]);
        assert_eq!(h.fills.borrow().len(), 1);
        assert_eq!(source["map"]["fills"].as_array().unwrap().len(), 1);
        fill(&h.fills.borrow()[0], &source["map"]["fills"][0]);
        boundaries.push((class, "map"));
    }
    {
        let class = instance(&queue);
        let h = &class.receiver.host;
        let original_loaded = h.loaded.borrow().clone();
        let extra = original_loaded.get("extra").unwrap();
        let weak = Rc::downgrade(&class);
        *h.loaded_hook.borrow_mut() = Some(Box::new(move || {
            let class = weak.upgrade().unwrap();
            assert!(
                !class.snapshot.borrow().has_pending_snapshot(),
                "native loaded callback observes reset"
            );
            send(&class, "snapshot", (0..50).map(|_| raw("nested")).collect());
        }));
        let weak = Rc::downgrade(&class);
        *h.store_hook.borrow_mut() = Some(Box::new(move || {
            let class = weak.upgrade().unwrap();
            assert_eq!(
                class.snapshot.borrow().pending_raw_order_count(),
                50,
                "class native subscriber sees loaded-getter reentry"
            );
            let mut value = raw("nested");
            value["status"] = json!("FILLED");
            send(&class, "update", vec![value]);
        }));
        send(&class, "snapshot", vec![raw("old")]);
        assert_eq!(
            class.snapshot.borrow().pending_raw_order_count(),
            50,
            "class native older flush preserves reentrant snapshot"
        );
        assert_eq!(h.snapshots.borrow().len(), 1);
        assert_eq!(h.fills.borrow().len(), 1);
        let staged = pending(&class)[0].clone();
        send(&class, "snapshot", vec![]);
        assert_eq!(
            h.snapshots.borrow().len(),
            2,
            "class native reentrant snapshot eventually completes"
        );
        assert!(
            Rc::ptr_eq(&h.orders.borrow().borrow()[0], &staged),
            "class native reentrant snapshot retains update root"
        );
        assert!(
            same_frame(&h.loaded.borrow().get("extra").unwrap(), &extra),
            "class native loaded shallow spread retains nested root"
        );
        assert!(
            !same_frame(&h.loaded.borrow(), &original_loaded),
            "class native loaded replaces outer record"
        );
        assert!(!class.snapshot.borrow().has_pending_snapshot());
        assert_eq!(class.snapshot.borrow().pending_raw_order_count(), 0);
        orders(&h.orders.borrow(), &source["reentry"]["orders"], 1);
        assert_eq!(source["reentry"]["snapshots"].as_array().unwrap().len(), 2);
        for (a, e) in h
            .snapshots
            .borrow()
            .iter()
            .zip(source["reentry"]["snapshots"].as_array().unwrap())
        {
            orders(a, e, 1);
        }
        assert_eq!(source["reentry"]["fills"].as_array().unwrap().len(), 1);
        fill(&h.fills.borrow()[0], &source["reentry"]["fills"][0]);
        boundaries.push((class, "reentry"));
    }
    {
        let class = instance(&queue);
        let h = &class.receiver.host;
        send(&class, "snapshot", vec![raw("old")]);
        let event = Frame::from_json(&json!({"type":"update","orders":[raw("live")]}));
        let Frame::Array(original) = event.get("orders").unwrap() else {
            panic!()
        };
        let saved = original.clone();
        let changed = event.clone();
        *h.resolve_hook.borrow_mut() = Some(Box::new(move || {
            let mut next = raw("appended");
            next["cumulative_quantity"] = json!("1\u{fffd}");
            next["limit_price"] = json!("4\u{fffd}");
            let value = Frame::from_json(&next);
            set(
                &value,
                "cumulative_quantity",
                Frame::String(vec![49, 0xd800]),
            );
            set(&value, "limit_price", Frame::String(vec![52, 0xd800]));
            original.borrow_mut().push(value);
            set(
                &changed,
                "type",
                Frame::String("snapshot".encode_utf16().collect()),
            );
            set(
                &changed,
                "orders",
                Frame::Array(Rc::new(RefCell::new(Vec::new()))),
            );
        }));
        class.handle_user_event_orders(event).unwrap();
        let committed = h.orders.borrow().clone();
        assert_eq!(
            committed.borrow().len(),
            2,
            "class native for-of observes appended old array"
        );
        assert_eq!(saved.borrow().len(), 2);
        assert!(
            same_frame(&committed.borrow()[0].borrow().info, &saved.borrow()[0]),
            "class native first info retains original array item"
        );
        assert!(
            same_frame(&committed.borrow()[1].borrow().info, &saved.borrow()[1]),
            "class native appended info retains original array item"
        );
        assert_eq!(
            committed.borrow()[1]
                .borrow()
                .fields
                .price
                .to_number()
                .to_bits(),
            4_f64.to_bits(),
            "class native parseFloat preserves prefix before surrogate"
        );
        assert!(!class.snapshot.borrow().has_pending_snapshot());
        // JSON capture contains the same UTF16 code unit, escaped by JSON.stringify.
        orders(&committed, &source["live"]["orders"], 2);
        boundaries.push((class, "live"));
    }
    {
        let class = instance(&queue);
        let h = &class.receiver.host;
        send(
            &class,
            "snapshot",
            (0..50).map(|_| raw("pending")).collect(),
        );
        let (promise, resolve) = Promise::pending();
        *h.signer.borrow_mut() = promise;
        let mut open = class.on_open();
        assert!(is_pending(&mut open));
        assert_eq!(
            class.receiver.subscription_attempt.get().to_bits(),
            1_f64.to_bits()
        );
        assert!(
            !class.snapshot.borrow().has_pending_snapshot(),
            "class native onOpen resets before suspended signer"
        );
        assert_eq!(class.snapshot.borrow().pending_raw_order_count(), 0);
        assert!(
            matches!(h.trace.borrow().last(), Some(Trace::Sign(_, _, false, 0))),
            "class native signer sees reset before await"
        );
        queue.enqueue(move || resolve.resolve(Ok(json!("jwt"))));
        assert!(
            is_pending(&mut open),
            "class native signer completion remains queued before subscriptions"
        );
        assert!(
            !h.trace.borrow().iter().any(|t| matches!(t, Trace::Send(_))),
            "class native subscriptions wait for queued signer completion"
        );
        queue.drain();
        completed(open).unwrap();
        assert_eq!(
            h.trace
                .borrow()
                .iter()
                .filter(|t| matches!(t, Trace::Send(_)))
                .count(),
            2,
            "class native auth two-channel floor"
        );
        h.disposed.set(true);
        let before = h.trace.borrow().len();
        let disposed = class.on_open();
        queue.drain();
        completed(disposed).unwrap();
        assert_eq!(h.trace.borrow().len(), before);
        assert_eq!(
            class.receiver.subscription_attempt.get().to_bits(),
            1_f64.to_bits()
        );
        boundaries.push((class, "open"));
    }
    {
        let class = instance(&queue);
        let h = &class.receiver.host;
        let weak = Rc::downgrade(&class);
        *h.store_hook.borrow_mut() = Some(Box::new(move || {
            let class = weak.upgrade().unwrap();
            let roots = class.receiver.host.orders.borrow().clone();
            let root = roots.borrow()[0].clone();
            {
                let mut order = root.borrow_mut();
                order.fields.id = "store-id".into();
                order.fields.symbol = "store-symbol".into();
                order.fields.filled = 9.0;
                set(
                    &order.info,
                    "avg_price",
                    Frame::Array(Rc::new(RefCell::new(Vec::new()))),
                );
            }
            roots.borrow_mut().push(root);
        }));
        let weak = Rc::downgrade(&class);
        *h.fill_hook.borrow_mut() = Some(Box::new(move || {
            let class = weak.upgrade().unwrap();
            let roots = class.receiver.host.orders.borrow().clone();
            let root = roots.borrow()[0].clone();
            roots.borrow_mut().push(root.clone());
            root.borrow_mut().fields.id = "fill-id".into();
        }));
        let mut value = raw("updating");
        value["status"] = json!("FILLED");
        send(&class, "update", vec![value]);
        let roots = h.orders.borrow().clone();
        assert_eq!(
            roots.borrow().len(),
            3,
            "class native callback append floor"
        );
        assert!(Rc::ptr_eq(&roots.borrow()[0], &roots.borrow()[1]));
        assert!(Rc::ptr_eq(&roots.borrow()[1], &roots.borrow()[2]));
        assert_eq!(
            h.fills.borrow().len(),
            2,
            "class native fills capture post-Store pre-emitter length"
        );
        let price = roots.borrow()[0].borrow().info.get("avg_price").unwrap();
        assert!(same_frame(&price, &h.fills.borrow()[0].price));
        assert!(same_frame(&price, &h.fills.borrow()[1].price));
        orders(&roots, &source["update"]["orders"], 3);
        assert_eq!(source["update"]["fills"].as_array().unwrap().len(), 2);
        for (a, e) in h
            .fills
            .borrow()
            .iter()
            .zip(source["update"]["fills"].as_array().unwrap())
        {
            fill(a, e);
        }
        assert!(!class.snapshot.borrow().has_pending_snapshot());
        boundaries.push((class, "update"));
    }
    // Native records, numeric bits, retained roots, Map keys and reentrant state above
    // all precede any diagnostic wire comparison.
    for (class, key) in &boundaries {
        let e = source[*key]["trace"].as_array().unwrap();
        let trace = class.receiver.host.trace.borrow();
        assert_eq!(
            trace.len(),
            e.len(),
            "class private combined trace count {key}"
        );
        for (actual, e) in trace.iter().zip(e) {
            match actual {
                Trace::Received => assert_eq!(e["kind"], "received"),
                Trace::Diagnostic(_) => {
                    assert_eq!(e["kind"], "diagnostic");
                }
                Trace::Health => assert_eq!(e["kind"], "health"),
                Trace::Error(value) => {
                    assert_eq!(e["kind"], "error");
                    frame(value.as_ref().unwrap(), &e["message"]);
                }
                Trace::Loaded(active, count) | Trace::Update(_, active, count) => {
                    assert_eq!(
                        e["kind"],
                        if matches!(actual, Trace::Loaded(..)) {
                            "loaded"
                        } else {
                            "update"
                        }
                    );
                    assert_eq!(*active, e["active"].as_bool().unwrap());
                    bit(
                        *count as f64,
                        &e["count"],
                        "class native snapshot count at boundary",
                    );
                    if let Trace::Update(ids, ..) = actual {
                        assert_eq!(
                            ids,
                            &e["ids"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .map(|s| s.as_str().unwrap().to_owned())
                                .collect::<Vec<_>>()
                        );
                    }
                }
                Trace::Add(ids) => {
                    assert_eq!(e["kind"], "add");
                    assert_eq!(
                        ids,
                        &e["ids"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|s| s.as_str().unwrap().to_owned())
                            .collect::<Vec<_>>()
                    );
                }
                Trace::Fill(value) => {
                    assert_eq!(e["kind"], "fill");
                    fill(value, &e["fill"]);
                }
                Trace::Sign(key, secret, active, count) => {
                    assert_eq!(e["kind"], "sign");
                    assert_eq!(key.as_ref(), Some(&e["key"]));
                    assert_eq!(secret.as_ref(), Some(&e["secret"]));
                    assert_eq!(*active, e["active"].as_bool().unwrap());
                    bit(
                        *count as f64,
                        &e["count"],
                        "class native snapshot count at boundary",
                    );
                }
                Trace::Send(value) => {
                    assert_eq!(e["kind"], "send");
                    assert_eq!(value.request_type, e["value"]["type"]);
                    assert_eq!(value.channel, e["value"]["channel"]);
                    assert_eq!(value.jwt, e["value"]["jwt"]);
                }
            }
        }
    }
    // Only now compare every diagnostic wire boundary across scenarios.
    for (class, key) in &boundaries {
        let expected = source[*key]["trace"].as_array().unwrap();
        for (actual, e) in class.receiver.host.trace.borrow().iter().zip(expected) {
            if let Trace::Diagnostic(raw) = actual {
                let left: Value = serde_json::from_str(raw).unwrap();
                let right: Value = serde_json::from_str(e["raw"].as_str().unwrap()).unwrap();
                assert_eq!(left, right, "class diagnostic wire input");
            }
        }
    }
}

#[test]
fn class_snapshot_retains_native_notification_origin_after_commit_and_stops_dispatch() {
    let queue = Microtasks::default();
    let class = instance(&queue);
    let host = &class.receiver.host;
    send(
        &class,
        "snapshot",
        (0..50).map(|_| raw("pending")).collect(),
    );
    let retained = pending(&class)[0].clone();
    let store = Rc::new(DefaultStore::new());
    *host.native_store.borrow_mut() = Some(store.clone());
    let original: Rc<dyn Error> = Rc::new(std::io::Error::other("snapshot listener failed"));
    let subscriber_error = StoreSubscriberError::with_response(
        "snapshot listener failed",
        json!({"status":409,"data":{"message":"native listener metadata"}}),
    )
    .with_cause(original.clone());
    let first_calls = Rc::new(Cell::new(0));
    let later_calls = Rc::new(Cell::new(0));
    let first = first_calls.clone();
    let owner = Rc::downgrade(&class);
    let observed_error = subscriber_error.clone();
    let unsubscribe = Store::subscribe(
        store.as_ref(),
        Box::new(move |data, from_worker| {
            first.set(first.get() + 1);
            assert!(!from_worker, "class snapshot native Store worker flag");
            let class = owner.upgrade().unwrap();
            assert!(
                !class.snapshot.borrow().has_pending_snapshot(),
                "class snapshot reset visible inside subscriber"
            );
            assert_eq!(
                class.snapshot.borrow().pending_raw_order_count(),
                0,
                "class snapshot raw count reset inside subscriber"
            );
            assert_eq!(
                data.orders
                    .iter()
                    .map(|order| order.id.as_str())
                    .collect::<Vec<_>>(),
                ["pending", "one"],
                "class snapshot native committed subscriber order values"
            );
            assert!(
                data.loaded.orders,
                "class snapshot native loaded committed inside subscriber"
            );
            assert!(
                data.loaded.markets,
                "class snapshot loaded record preserves markets field"
            );
            assert!(
                !data.loaded.balance && !data.loaded.tickers && !data.loaded.positions,
                "class snapshot remaining modeled loaded fields"
            );
            Err(observed_error.clone())
        }),
    );
    let later = later_calls.clone();
    let _unsubscribe_later = Store::subscribe(
        store.as_ref(),
        Box::new(move |_, _| {
            later.set(later.get() + 1);
            Ok(())
        }),
    );
    let after_publication = Rc::new(Cell::new(false));
    let after = after_publication.clone();
    *host.store_hook.borrow_mut() = Some(Box::new(move || after.set(true)));
    let failure = class
        .on_message(
            &json!({"channel":"user","events":[
                {"type":"snapshot","orders":[raw("one")]},
                {"type":"update","orders":[raw("must-not-run")]}
            ]})
            .to_string(),
        )
        .expect_err("class snapshot notification failure escapes dispatch");
    assert!(
        Rc::ptr_eq(
            &failure,
            host.publication_failure.borrow().as_ref().unwrap()
        ),
        "class snapshot publication returns the original failure root"
    );
    let Some(StoreMutationError::Notification(notification)) = failure.store_mutation() else {
        panic!("class snapshot native Store notification origin retained");
    };
    assert_eq!(
        notification.subscriber_id(),
        0,
        "class snapshot original Store subscriber identity"
    );
    assert_eq!(
        notification.cause(),
        &subscriber_error,
        "class snapshot native subscriber payload and cause retained"
    );
    assert!(
        std::ptr::eq(notification.cause().source().unwrap(), original.as_ref()),
        "class snapshot original native cause pointer retained"
    );
    assert!(
        std::ptr::eq(
            failure
                .source()
                .unwrap()
                .downcast_ref::<StoreMutationError>()
                .unwrap(),
            failure.store_mutation().unwrap()
        ),
        "class snapshot native error chain retained"
    );
    assert!(
        failure.value.is_none(),
        "native failure is not flattened into an invented JSON throw"
    );
    assert_eq!(
        first_calls.get(),
        1,
        "class snapshot first subscriber called once"
    );
    assert_eq!(
        later_calls.get(),
        0,
        "class snapshot later subscriber suppressed"
    );
    assert!(
        !after_publication.get(),
        "class snapshot later publication work suppressed"
    );
    assert!(
        !class.snapshot.borrow().has_pending_snapshot(),
        "class snapshot assembly stays reset after failure"
    );
    assert_eq!(class.snapshot.borrow().pending_raw_order_count(), 0);
    assert_eq!(
        store
            .orders()
            .unwrap()
            .iter()
            .map(|order| order.id.clone())
            .collect::<Vec<_>>(),
        ["pending", "one"],
        "class snapshot committed Store state survives failure"
    );
    assert!(store.loaded().unwrap().orders);
    assert_eq!(
        host.snapshots.borrow().len(),
        1,
        "class snapshot independent publication floor"
    );
    assert_eq!(
        host.orders.borrow().borrow().len(),
        2,
        "class snapshot retained root floor"
    );
    assert!(
        Rc::ptr_eq(&host.orders.borrow().borrow()[0], &retained),
        "class snapshot preexisting order root retained across failure"
    );
    assert_eq!(host.orders.borrow().borrow()[1].borrow().fields.id, "one");
    assert!(
        host.fills.borrow().is_empty(),
        "class snapshot failed dispatch emits no later fills"
    );
    assert!(
        !host
            .trace
            .borrow()
            .iter()
            .any(|trace| matches!(trace, Trace::Add(_))),
        "class snapshot failure stops following user event"
    );
    unsubscribe();
    send(&class, "snapshot", vec![]);
    assert!(
        store.orders().unwrap().is_empty(),
        "class snapshot can publish after failing subscriber removed"
    );
    assert_eq!(
        later_calls.get(),
        1,
        "class snapshot recovery resumes surviving subscriber"
    );
    assert!(
        after_publication.get(),
        "class snapshot recovery reaches later publication work"
    );
    let source = source();
    assert_eq!(
        source["notification"],
        json!({
            "sameCause":true,"ids":["pending","one"],"loaded":true,
            "active":false,"count":0,"publications":1,"fills":0,"followingEvents":0,
            "subscriberCalls":1,"recovered":true
        }),
        "class snapshot paired source notification floor stops after failure and recovery"
    );
}
