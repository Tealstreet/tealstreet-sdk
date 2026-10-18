use safe_cex_num::numeric_ingress::{js_from_value, js_value_as_f64};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{Order, OrderSide, OrderStatus, OrderType, StoreDataLoaded};
use safe_cex_venue_blofin::private_stream_control::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn corpus() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private_stream_control_oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "maintained source oracle: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            85,
            "independent 85-case floor"
        );
        assert_eq!(v["stoppedAfter"], 85);
        v
    })
}
fn number(value: f64, expected: &Value) {
    assert_eq!(
        value.to_bits(),
        u64::from_str_radix(expected["number"].as_str().unwrap(), 16).unwrap(),
        "native numeric bits"
    );
}
fn plain(value: &Value, expected: &Value) {
    if expected.get("number").is_some() {
        number(js_value_as_f64(value).unwrap(), expected)
    } else {
        match (value, expected) {
            (Value::Array(v), Value::Array(e)) => {
                assert_eq!(v.len(), e.len());
                for (a, b) in v.iter().zip(e) {
                    plain(a, b)
                }
            }
            (Value::Object(v), Value::Object(e)) => {
                assert_eq!(
                    v.keys().collect::<Vec<_>>(),
                    e.keys().collect::<Vec<_>>(),
                    "native object keys"
                );
                for (k, a) in v {
                    plain(a, &e[k])
                }
            }
            _ => assert_eq!(value, expected, "native JSON primitive"),
        }
    }
}
fn order_assert(order: &Order, e: &Value) {
    assert_eq!(order.id, e["id"]);
    assert_eq!(order.symbol, e["symbol"]);
    assert_eq!(order.status, OrderStatus::Open);
    assert_eq!(order.r#type, OrderType::Limit);
    assert_eq!(order.side, OrderSide::Buy);
    assert_eq!(order.reduce_only, false);
    for (a, key) in [
        (order.price.to_number(), "price"),
        (order.amount, "amount"),
        (order.filled, "filled"),
        (order.remaining, "remaining"),
        (order.last_update, "lastUpdate"),
    ] {
        number(a, &e[key]);
    }
}
fn order_list(orders: &[Order], e: &Value) {
    let e = e.as_array().unwrap();
    assert_eq!(orders.len(), e.len(), "native order count");
    for (a, b) in orders.iter().zip(e) {
        order_assert(a, b)
    }
}
type AuthTask = PrivateStreamFuture<Result<(), PrivateAuthError<String>>>;
type Callback = Rc<dyn Fn() -> PrivateStreamFuture<()>>;
struct Host {
    initial: Value,
    task_errors: RefCell<Vec<PrivateAuthError<String>>>,
    trace: RefCell<Vec<Value>>,
    sent: RefCell<Vec<Option<Value>>>,
    parsed: RefCell<Vec<(String, Value)>>,
    requests: RefCell<Vec<RemoteAuthRequest>>,
    store: Rc<DefaultStore>,
    disposed: Rc<Cell<bool>>,
    socket: Cell<bool>,
    next_timer: Cell<u32>,
    timers: RefCell<BTreeMap<u32, Callback>>,
    retained_callbacks: RefCell<Vec<Callback>>,
    tasks: RefCell<Vec<AuthTask>>,
    send_count: Cell<u32>,
    key: Rc<RefCell<String>>,
    passphrase: Rc<RefCell<Option<Value>>>,
    fetched: Vec<Order>,
    created: RefCell<Option<Option<String>>>,
    endpoint: RefCell<Option<String>>,
}
impl Host {
    fn new(i: &Value) -> Self {
        let order: Order = js_from_value(corpus()["order"].clone()).unwrap();
        let mut old = order.clone();
        old.id = "o-old".into();
        old.price = (3.0).into();
        let store = Rc::new(DefaultStore::default());
        Store::update(
            store.as_ref(),
            StoreUpdate {
                orders: Some(vec![old]),
                loaded: Some(StoreDataLoaded {
                    markets: true,
                    tickers: false,
                    balance: true,
                    positions: false,
                    orders: false,
                }),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        Self {
            initial: i.clone(),
            task_errors: RefCell::new(vec![]),
            trace: RefCell::new(vec![]),
            sent: RefCell::new(vec![]),
            parsed: RefCell::new(vec![]),
            requests: RefCell::new(vec![]),
            store,
            disposed: Rc::new(Cell::new(i["disposed"].as_bool().unwrap_or(false))),
            socket: Cell::new(false),
            next_timer: Cell::new(1),
            timers: RefCell::new(BTreeMap::new()),
            retained_callbacks: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            send_count: Cell::new(0),
            key: Rc::new(RefCell::new("k".into())),
            passphrase: Rc::new(RefCell::new(
                if i["passphraseAbsent"].as_bool() == Some(true) {
                    None
                } else {
                    Some(i.get("passphrase").cloned().unwrap_or(json!("p")))
                },
            )),
            fetched: if i["ordersEmpty"].as_bool() == Some(true) {
                vec![]
            } else {
                vec![order]
            },
            created: RefCell::new(None),
            endpoint: RefCell::new(None),
        }
    }
    fn call(&self, label: &str) -> Result<(), String> {
        if self.initial["fail"].as_str() == Some(label) {
            Err(format!("{label} failed"))
        } else {
            Ok(())
        }
    }
    fn event(&self, v: Value) {
        self.trace.borrow_mut().push(v)
    }
    fn handler(&self, kind: &str, data: &Value) -> Result<(), String> {
        self.call(&format!("handler:{kind}"))?;
        self.parsed.borrow_mut().push((kind.into(), data.clone()));
        self.event(json!(["handler", kind]));
        Ok(())
    }
    fn flush(&self) {
        let mut tasks = self.tasks.take();
        let mut context = Context::from_waker(Waker::noop());
        for _ in 0..100 {
            let mut pending = vec![];
            for mut task in tasks {
                match task.as_mut().poll(&mut context) {
                    Poll::Ready(Ok(())) => (),
                    Poll::Ready(Err(error)) => {
                        self.event(json!(["task-error", error.to_string()]));
                        self.task_errors.borrow_mut().push(error);
                    }
                    Poll::Pending => pending.push(task),
                }
            }
            if pending.is_empty() {
                return;
            }
            tasks = pending;
        }
        panic!("poll floor100 exhausted")
    }
}
impl PrivateStreamHost for Host {
    type Error = String;
    type Timer = u32;
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn is_testnet(&self) -> bool {
        self.initial["testnet"].as_bool().unwrap_or(false)
    }
    fn resolve_private_route(&self) -> Result<PrivateRoute, String> {
        self.call("route")?;
        self.event(json!(["route"]));
        Ok(PrivateRoute {
            tunnelled: self.initial["tunnelled"].as_bool().unwrap_or(false),
            host: self.initial["routeHost"].as_str().map(str::to_owned),
        })
    }
    fn create_and_assign_socket(&self, url: Option<&str>) -> Result<(), String> {
        self.call("create")?;
        *self.created.borrow_mut() = Some(url.map(str::to_owned));
        self.event(json!(["create", url]));
        self.socket.set(true);
        Ok(())
    }
    fn add_socket_listener(&self, event: &str) -> Result<(), String> {
        self.call(&format!("listener:{event}"))?;
        self.event(json!(["listener", event]));
        Ok(())
    }
    fn super_dispose(&self) -> Result<(), String> {
        self.call("super")?;
        self.event(json!(["super"]));
        self.disposed.set(true);
        Ok(())
    }
    fn timer_is_truthy(&self, timer: &u32) -> bool {
        *timer != 0
    }
    fn clear_timeout(&self, timer: &u32) -> Result<(), String> {
        self.call("clear")?;
        self.timers.borrow_mut().remove(timer);
        self.event(json!(["clear", timer]));
        Ok(())
    }
    fn set_timeout(&self, callback: Callback, delay_ms: f64) -> Result<u32, String> {
        self.call("timer")?;
        assert_eq!(delay_ms, 2000.0, "native debounce delay");
        let id = self.next_timer.get();
        self.next_timer.set(id + 1);
        self.retained_callbacks.borrow_mut().push(callback.clone());
        self.timers.borrow_mut().insert(id, callback);
        self.event(json!(["timer", id, delay_ms as u64]));
        Ok(id)
    }
    fn send_with_log(&self, frame: &str) -> Result<(), String> {
        let n = self.send_count.get() + 1;
        self.send_count.set(n);
        self.call(&format!("send:{n}"))?;
        self.sent.borrow_mut().push(if frame == "ping" {
            None
        } else {
            Some(serde_json::from_str(frame).unwrap())
        });
        self.event(json!(["send", frame]));
        if self.initial["disposeOnSend"].as_bool() == Some(true) {
            self.disposed.set(true)
        }
        Ok(())
    }
    fn performance_now(&self) -> f64 {
        self.event(json!(["now"]));
        if self.initial["negativeZero"].as_bool() == Some(true) {
            -0.0
        } else {
            123.5
        }
    }
    fn nonce(&self) -> String {
        self.event(json!(["nonce"]));
        "nonce-fixed".into()
    }
    fn timestamp_ms(&self) -> f64 {
        self.event(json!(["timestamp"]));
        if self.initial["timestampNegativeZero"].as_bool() == Some(true) {
            -0.0
        } else if self.initial["timestampNaN"].as_bool() == Some(true) {
            f64::NAN
        } else {
            1710000000123.0
        }
    }
    fn remote_signer_present(&self) -> bool {
        self.initial["remote"].as_bool().unwrap_or(false)
    }
    fn remote_account_name(&self) -> Option<Value> {
        Some(json!("acct"))
    }
    fn request_id(&self) -> String {
        self.event(json!(["request-id"]));
        "request-fixed".into()
    }
    fn sign_remote(
        &self,
        request: RemoteAuthRequest,
    ) -> PrivateStreamStart<RemoteAuthResult, String> {
        self.event(json!(["sign",{"id":request.id,"type":request.kind,"exchangeName":request.exchange_name,"accountName":request.account_name,"wsAuth":{"toSign":request.to_sign,"outputFormat":request.output_format,"extraData":{"timestamp":{"number":format!("{:016x}",request.timestamp.to_bits())},"nonce":request.nonce}}}]));
        self.requests.borrow_mut().push(request);
        if self.initial["remoteSyncThrow"].as_bool() == Some(true) {
            return Err("remote sync failed".into());
        }
        let i = self.initial.clone();
        let key = self.key.clone();
        let phrase = self.passphrase.clone();
        Ok(Box::pin(async move {
            if i["remoteThrow"].as_bool() == Some(true) {
                return Err("remote failed".into());
            }
            if i["mutateCredentials"].as_bool() == Some(true) {
                *key.borrow_mut() = "changed".into();
                *phrase.borrow_mut() = Some(json!("changed-p"));
            }
            Ok(RemoteAuthResult {
                error: i.get("remoteError").cloned(),
                signature: if i["signatureAbsent"].as_bool() == Some(true) {
                    None
                } else {
                    Some(json!("remote-signature"))
                },
            })
        }))
    }
    fn secret(&self) -> String {
        self.initial["secret"].as_str().unwrap_or("s").into()
    }
    fn key(&self) -> String {
        self.key.borrow().clone()
    }
    fn passphrase(&self) -> Option<Value> {
        self.passphrase.borrow().clone()
    }
    fn emit_signing_error(&self, message: &str) -> Result<(), String> {
        self.call("error")?;
        self.event(json!(["error", message]));
        Ok(())
    }
    fn launch_auth(&self, task: PrivateStreamFuture<Result<(), PrivateAuthError<String>>>) {
        self.tasks.borrow_mut().push(task)
    }
    fn track_last_received(&self) -> Result<(), String> {
        self.call("track")?;
        self.event(json!(["track"]));
        Ok(())
    }
    fn diagnostic_receive(&self, data: &str) -> Result<(), String> {
        self.call("diagnostic")?;
        self.event(json!(["diagnostic", data]));
        Ok(())
    }
    fn log_last_log(&self, data: &str) -> Result<(), String> {
        self.call("log")?;
        self.event(json!(["log", data]));
        Ok(())
    }
    fn handle_pong(&self) -> Result<(), String> {
        self.call("pong")?;
        self.event(json!(["pong"]));
        Ok(())
    }
    fn reconnecting_enabled(&self) -> bool {
        self.initial["reconnecting"].as_bool() != Some(false)
    }
    fn fetch_and_set_private_snapshot(&self) -> Result<(), String> {
        self.call("snapshot")?;
        self.event(json!(["snapshot"]));
        Ok(())
    }
    fn handle_order_topic(&self, data: &Value) -> Result<(), String> {
        self.handler("orders", data)
    }
    fn handle_position_topic(&self, data: &Value) -> Result<(), String> {
        self.handler("positions", data)
    }
    fn handle_account_topic(&self, data: &Value) -> Result<(), String> {
        self.handler("account", data)
    }
    fn fetch_orders(&self) -> PrivateStreamStart<Vec<Order>, String> {
        self.event(json!(["fetch"]));
        if self.initial["fetchSyncThrow"].as_bool() == Some(true) {
            return Err("fetch sync failed".into());
        }
        let i = self.initial.clone();
        let disposed = self.disposed.clone();
        let store = self.store.clone();
        let orders = self.fetched.clone();
        Ok(Box::pin(async move {
            if i["fetchFail"].as_bool() == Some(true) {
                return Err("fetch failed".into());
            }
            if i["disposeOnResolve"].as_bool() == Some(true) {
                disposed.set(true)
            }
            if i["changeLoadedOnResolve"].as_bool() == Some(true) {
                let mut loaded = Store::loaded(store.as_ref()).clone();
                loaded.tickers = true;
                loaded.positions = true;
                Store::update(
                    store.as_ref(),
                    StoreUpdate {
                        loaded: Some(loaded),
                        ..Default::default()
                    },
                    false,
                )
                .unwrap();
            }
            Ok(orders)
        }))
    }
    fn store(&self) -> &dyn Store {
        self.store.as_ref()
    }
}
struct Snapshot {
    ping: f64,
    timeout: Option<u32>,
    disposed: bool,
    socket: bool,
    orders: Vec<Order>,
    loaded: StoreDataLoaded,
}
fn snapshot(s: &PrivateStreamState<u32>, h: &Host) -> Snapshot {
    let d = Store::serialize(h.store.as_ref());
    Snapshot {
        ping: s.ping_at.get(),
        timeout: *s.orders_throttle_timeout.borrow(),
        disposed: h.disposed.get(),
        socket: h.socket.get(),
        orders: d.orders,
        loaded: d.loaded,
    }
}
fn snapshot_assert(a: &Snapshot, e: &Value) {
    number(a.ping, &e["ping"]);
    assert_eq!(a.timeout, e["timeout"].as_u64().map(|v| v as u32));
    assert_eq!(a.disposed, e["disposed"].as_bool().unwrap());
    assert_eq!(a.socket, e["socket"].as_bool().unwrap());
    order_list(&a.orders, &e["orders"]);
    for (v, key) in [
        (a.loaded.orders, "orders"),
        (a.loaded.markets, "markets"),
        (a.loaded.tickers, "tickers"),
        (a.loaded.positions, "positions"),
        (a.loaded.balance, "balance"),
    ] {
        assert_eq!(
            v,
            e["loaded"][key].as_bool().unwrap(),
            "native loaded {key}"
        );
    }
}
fn group(name: &str, floor: usize) {
    let cases: Vec<_> = corpus()["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["group"] == name)
        .collect();
    assert_eq!(cases.len(), floor, "independent group floor");
    let mut runs = vec![];
    for c in &cases {
        let host = Rc::new(Host::new(&c["initial"]));
        let state = Rc::new(PrivateStreamState::<u32>::default());
        *state.orders_throttle_timeout.borrow_mut() =
            c["initial"]["timeout"].as_u64().map(|n| n as u32);
        let mut snaps = vec![];
        let mut results = vec![];
        for action in c["actions"].as_array().unwrap() {
            let result = match action[0].as_str().unwrap() {
                "endpoint" => {
                    let url = endpoint(host.as_ref());
                    *host.endpoint.borrow_mut() = Some(url.into());
                    host.event(json!(["endpoint", url]));
                    Ok(())
                }
                "dispose" => dispose(&state, host.as_ref()),
                "connect" => connect_and_subscribe(host.as_ref()),
                "open" => on_open(&state, &host),
                "ping" => ping(&state, host.as_ref()),
                "subscribe" => subscribe(host.as_ref()),
                "message" => on_message(host.as_ref(), action[1].as_str().unwrap()),
                "auth" => {
                    let task = auth(&host);
                    host.launch_auth(task);
                    Ok(())
                }
                "throttle" => throttled_fetch_orders(&state, &host),
                "fetch" => {
                    let task = fetch_throttled_orders(&state, &host);
                    host.launch_auth(Box::pin(async move {
                        task.await;
                        Ok(())
                    }));
                    Ok(())
                }
                "fire" => {
                    let cb = host
                        .timers
                        .borrow_mut()
                        .remove(&(action[1].as_u64().unwrap() as u32));
                    if let Some(cb) = cb {
                        let task = cb();
                        host.launch_auth(Box::pin(async move {
                            task.await;
                            Ok(())
                        }));
                    }
                    Ok(())
                }
                "flush" => {
                    host.flush();
                    Ok(())
                }
                _ => panic!("action"),
            };
            results.push(result.err());
            snaps.push(snapshot(&state, &host));
        }
        for (a, e) in snaps.iter().zip(c["snapshots"].as_array().unwrap()) {
            snapshot_assert(a, e)
        }
        assert_eq!(snaps.len(), c["snapshots"].as_array().unwrap().len());
        assert_eq!(
            results,
            serde_json::from_value::<Vec<Option<String>>>(c["results"].clone()).unwrap(),
            "native result sequence"
        );
        let errors = host.task_errors.borrow();
        let expected = c["taskErrors"].as_array().unwrap();
        assert_eq!(errors.len(), expected.len(), "native task error count");
        for (error, e) in errors.iter().zip(expected) {
            match error {
                PrivateAuthError::Host(message) => {
                    assert_eq!(e["name"], "Error");
                    assert_eq!(message, e["message"].as_str().unwrap());
                }
                PrivateAuthError::Coercion(error) => {
                    assert_eq!(error.name, "TypeError");
                    assert_eq!(error.name, e["name"]);
                    assert_eq!(error.message, e["message"]);
                }
            }
        }
        drop(errors);
        let sent = host.sent.borrow();
        let expected = c["sent"].as_array().unwrap();
        assert_eq!(sent.len(), expected.len(), "native frame count");
        for (a, e) in sent.iter().zip(expected) {
            match a {
                None => assert_eq!(e["ping"], true),
                Some(v) => assert_eq!(v, e, "independently decoded native frame fields"),
            }
        }
        drop(sent);
        let parsed = host.parsed.borrow();
        let expected = c["parsed"].as_array().unwrap();
        if c["initial"]["boundary"] == "surrogate" {
            assert_eq!(expected.len(), 1);
            assert_eq!(expected[0][1]["sourceUnits"], json!([0xd800]));
            assert_eq!(expected[0][1]["sourceLength"], 1);
            assert!(parsed.is_empty(), "native refuses parsed lone surrogate");
        } else if c["initial"]["boundary"] == "depth" {
            assert_eq!(expected.len(), 1);
            assert_eq!(expected[0][1]["sourceDepth"], 130);
            assert_eq!(expected[0][1]["sourceLeaf"], 42);
            assert!(
                parsed.is_empty(),
                "native enforces shared parser depth budget"
            );
        } else {
            assert_eq!(parsed.len(), expected.len());
        }
        for ((kind, value), e) in parsed.iter().zip(expected) {
            assert_eq!(kind, e[0].as_str().unwrap());
            plain(value, &e[1]);
        }
        drop(parsed);
        let requests = host.requests.borrow();
        let expected: Vec<_> = c["trace"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e[0] == "sign")
            .collect();
        assert_eq!(requests.len(), expected.len());
        for (r, e) in requests.iter().zip(expected) {
            assert_eq!(r.kind, e[1]["type"]);
            assert_eq!(r.exchange_name, e[1]["exchangeName"]);
            assert_eq!(r.output_format, e[1]["wsAuth"]["outputFormat"]);
            assert_eq!(r.id, e[1]["id"]);
            assert_eq!(r.account_name.as_ref(), Some(&e[1]["accountName"]));
            assert_eq!(r.to_sign, e[1]["wsAuth"]["toSign"]);
            number(r.timestamp, &e[1]["wsAuth"]["extraData"]["timestamp"]);
            assert_eq!(r.nonce, e[1]["wsAuth"]["extraData"]["nonce"]);
        }
        drop(requests);
        let callbacks = host.retained_callbacks.borrow();
        if callbacks.len() > 1 {
            assert_eq!(c["callbackSame"], true, "source stable arrow identity");
            assert!(
                !Rc::ptr_eq(&callbacks[0], &callbacks[1]),
                "native owns a fresh per-arm callback"
            );
        }
        drop(callbacks);
        if name == "fetch" && c["alias"] == true {
            assert_eq!(
                c["afterAlias"][0]["price"]["number"], "4053400000000000",
                "source price mutated to77"
            );
            let mut old = host.fetched.clone();
            let current = Store::serialize(host.store.as_ref());
            if old.is_empty() {
                let mut added: Order = js_from_value(corpus()["order"].clone()).unwrap();
                added.price = (77.0).into();
                old.push(added);
                assert_eq!(
                    current.orders.len(),
                    0,
                    "native empty fetched array remains owned"
                );
            } else {
                old[0].price = (77.0).into();
                assert_eq!(
                    current.orders[0].price, 11.0,
                    "native Store owns fetched order records"
                );
            }
            assert_eq!(old[0].price, 77.0);
        }
        let expected_endpoint = c["trace"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[0] == "endpoint")
            .map(|e| e[1].as_str().unwrap());
        assert_eq!(host.endpoint.borrow().as_deref(), expected_endpoint);
        let expected_created = c["trace"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[0] == "create")
            .map(|e| e[1].as_str().map(str::to_owned));
        assert_eq!(*host.created.borrow(), expected_created);
        host.timers.borrow_mut().clear();
        host.retained_callbacks.borrow_mut().clear();
        runs.push(host);
    }
    // Every native assertion above precedes call-order / wire comparison below.
    for (host, c) in runs.iter().zip(cases) {
        let expected = c["trace"].as_array().unwrap();
        if c["initial"]["boundary"].is_string() {
            assert_eq!(
                expected.last().unwrap(),
                &json!(["handler", "orders"]),
                "source dispatch at declared boundary"
            );
            assert_eq!(
                *host.trace.borrow(),
                expected[..expected.len() - 1],
                "native shares pre-parse effects but refuses declared boundary"
            );
        } else {
            assert_eq!(*host.trace.borrow(), *expected, "wire and call trace");
        }
    }
}
#[test]
fn native_endpoint() {
    group("endpoint", 2)
}
#[test]
fn native_dispose() {
    group("dispose", 5)
}
#[test]
fn native_connect() {
    group("connect", 8)
}
#[test]
fn native_open() {
    group("open", 6)
}
#[test]
fn native_ping() {
    group("ping", 4)
}
#[test]
fn native_auth() {
    group("auth", 19)
}
#[test]
fn native_subscribe() {
    group("subscribe", 3)
}
#[test]
fn native_message() {
    group("message", 24)
}
#[test]
fn native_throttle() {
    group("throttle", 7)
}
#[test]
fn native_fetch_throttled() {
    group("fetch", 7)
}
