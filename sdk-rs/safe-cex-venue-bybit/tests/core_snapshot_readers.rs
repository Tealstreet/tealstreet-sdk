use safe_cex_venue_bybit::core_snapshot_readers::{self as api, Operation, SnapshotHost};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::{Rc, Weak},
    sync::{Arc, Mutex},
    task::{Context, Poll, Wake, Waker},
};
type V = Rc<Node>;
#[derive(Debug)]
enum Node {
    U,
    Null,
    Bool(bool),
    Num(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<V>>),
    Obj(RefCell<Vec<(String, V)>>),
    Error {
        name: Vec<u16>,
        message: Vec<u16>,
    },
    Fn {
        owner: V,
        code: String,
        captures: Vec<V>,
    },
    Promise(Promise),
}
#[derive(Debug)]
struct Promise {
    state: RefCell<Option<Result<V, V>>>,
    listeners: RefCell<Vec<Waker>>,
}
enum Job {
    Task(usize),
    Mapper(String),
}
struct TaskWake {
    queue: Arc<Mutex<VecDeque<Job>>>,
    index: usize,
}
impl Wake for TaskWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref()
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.lock().unwrap().push_back(Job::Task(self.index));
    }
}
fn u() -> V {
    Rc::new(Node::U)
}
fn n(v: f64) -> V {
    Rc::new(Node::Num(v))
}
fn b(v: bool) -> V {
    Rc::new(Node::Bool(v))
}
fn t(v: &str) -> V {
    Rc::new(Node::Text(v.encode_utf16().collect()))
}
fn arr(v: Vec<V>) -> V {
    Rc::new(Node::Array(RefCell::new(v)))
}
fn obj(v: Vec<(&str, V)>) -> V {
    Rc::new(Node::Obj(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn equal(a: &V, b: &V) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Node::U, Node::U) | (Node::Null, Node::Null) => true,
        (Node::Bool(a), Node::Bool(b)) => a == b,
        (Node::Num(a), Node::Num(b)) => a == b,
        (Node::Text(a), Node::Text(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn truth(v: &V) -> bool {
    match v.as_ref() {
        Node::U | Node::Null => false,
        Node::Bool(v) => *v,
        Node::Num(v) => *v != 0. && !v.is_nan(),
        Node::Text(v) => !v.is_empty(),
        _ => true,
    }
}
fn field(v: &V, k: &str) -> V {
    match v.as_ref() {
        Node::Obj(o) => o
            .borrow()
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Node::Array(a) => {
            if k == "length" {
                n(a.borrow().len() as f64)
            } else {
                k.parse::<usize>()
                    .ok()
                    .and_then(|i| a.borrow().get(i).cloned())
                    .unwrap_or_else(u)
            }
        }
        Node::Error { name, message } => match k {
            "message" => Rc::new(Node::Text(message.clone())),
            "name" => Rc::new(Node::Text(name.clone())),
            _ => u(),
        },
        _ => u(),
    }
}
fn set_field(v: &V, k: &str, value: V) {
    let Node::Obj(o) = v.as_ref() else {
        panic!("native object assignment")
    };
    let mut o = o.borrow_mut();
    if let Some((_, v)) = o.iter_mut().find(|(n, _)| n == k) {
        *v = value
    } else {
        o.push((k.into(), value))
    }
}
fn items(v: &V) -> Vec<V> {
    let Node::Array(a) = v.as_ref() else {
        panic!("native array")
    };
    a.borrow().clone()
}
fn text(v: &V) -> Vec<u16> {
    match v.as_ref() {
        Node::Text(s) => s.clone(),
        Node::U => "undefined".encode_utf16().collect(),
        Node::Null => "null".encode_utf16().collect(),
        Node::Num(n) => safe_cex_num::js_number_string(*n).encode_utf16().collect(),
        Node::Bool(b) => b.to_string().encode_utf16().collect(),
        _ => "[object Object]".encode_utf16().collect(),
    }
}
fn jsnum(v: &V) -> f64 {
    match v.as_ref() {
        Node::U => f64::NAN,
        Node::Null => 0.,
        Node::Bool(v) => {
            if *v {
                1.
            } else {
                0.
            }
        }
        Node::Num(v) => *v,
        Node::Text(s) => {
            let s = String::from_utf16_lossy(s);
            if s.trim().is_empty() {
                0.
            } else {
                s.trim().parse().unwrap_or(f64::NAN)
            }
        }
        _ => f64::NAN,
    }
}
fn from(v: &Value) -> V {
    match v {
        Value::Null => Rc::new(Node::Null),
        Value::Bool(v) => b(*v),
        Value::Number(v) => n(v.as_f64().unwrap()),
        Value::String(s) => t(s),
        Value::Array(v) => arr(v.iter().map(from).collect()),
        Value::Object(v) => {
            if v.get("fixtureUndefined") == Some(&json!(true)) {
                u()
            } else if v.get("fixtureNan") == Some(&json!(true)) {
                n(f64::NAN)
            } else {
                Rc::new(Node::Obj(RefCell::new(
                    v.iter().map(|(k, v)| (k.clone(), from(v))).collect(),
                )))
            }
        }
    }
}
fn decoded(v: &V) -> Value {
    match v.as_ref() {
        Node::U => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(v) => json!(v),
        Node::Num(v) => {
            json!({"n":if v.is_nan(){"nan".into()}else{format!("{:016x}",v.to_bits())}})
        }
        Node::Text(v) => json!({"s":v}),
        Node::Array(v) => json!({"a":v.borrow().iter().map(decoded).collect::<Vec<_>>()}),
        Node::Obj(v) => {
            json!({"o":v.borrow().iter().map(|(k,v)|json!([k.encode_utf16().collect::<Vec<_>>(),decoded(v)])).collect::<Vec<_>>()})
        }
        Node::Error { name, message } => json!({"error":true,"name":name,"message":message}),
        Node::Fn { code, .. } => json!({"fn":code}),
        Node::Promise(_) => json!({"promise":true}),
    }
}
// Assert decoded variants/fields directly; no serialization/reencoding/roundtrip.
fn assert_native(v: &V, e: &Value, label: &str) {
    match v.as_ref() {
        Node::U => assert_eq!(*e, json!({"u":true}), "{label}: native undefined"),
        Node::Null => assert!(e.is_null(), "{label}: native null"),
        Node::Bool(v) => assert_eq!(Some(*v), e.as_bool(), "{label}: native bool"),
        Node::Num(v) => assert_eq!(
            e["n"].as_str(),
            Some(if v.is_nan() {
                "nan".into()
            } else {
                format!("{:016x}", v.to_bits())
            })
            .as_deref(),
            "{label}: native f64"
        ),
        Node::Text(v) => assert_eq!(
            *v,
            e["s"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u16)
                .collect::<Vec<_>>(),
            "{label}: native UTF16"
        ),
        Node::Array(v) => {
            let e = e["a"].as_array().unwrap();
            assert_eq!(v.borrow().len(), e.len(), "{label}: native array count");
            for (v, e) in v.borrow().iter().zip(e) {
                assert_native(v, e, label)
            }
        }
        Node::Obj(v) => {
            let e = e["o"].as_array().unwrap();
            assert_eq!(v.borrow().len(), e.len(), "{label}: native object count");
            for ((k, v), e) in v.borrow().iter().zip(e) {
                assert_eq!(
                    k.encode_utf16().collect::<Vec<_>>(),
                    e[0].as_array()
                        .unwrap()
                        .iter()
                        .map(|n| n.as_u64().unwrap() as u16)
                        .collect::<Vec<_>>(),
                    "{label}: native key order"
                );
                assert_native(v, &e[1], label)
            }
        }
        Node::Error { name, message } => {
            assert_eq!(e["error"], true, "{label}: native error discriminant");
            assert_eq!(
                *name,
                e["name"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect::<Vec<_>>(),
                "{label}: native error name"
            );
            assert_eq!(
                *message,
                e["message"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_u64().unwrap() as u16)
                    .collect::<Vec<_>>(),
                "{label}: native error message"
            )
        }
        _ => panic!("{label}: unexpected returned native promise/function"),
    }
}
async fn pause() {
    let mut first = true;
    std::future::poll_fn(move |cx| {
        if first {
            first = false;
            cx.waker().wake_by_ref();
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await
}
struct Task {
    body: RefCell<Option<Operation<V, V>>>,
    promise: V,
}
struct Host {
    weak: Weak<Host>,
    input: Value,
    owner: V,
    stores: [V; 2],
    refs: RefCell<Vec<(V, String)>>,
    events: RefCell<Vec<Value>>,
    writes: RefCell<Vec<Value>>,
    tasks: RefCell<Vec<Rc<Task>>>,
    queue: Arc<Mutex<VecDeque<Job>>>,
    primary: V,
    values: BTreeMap<String, V>,
    index: Cell<usize>,
    mapper: RefCell<String>,
    loops: RefCell<Vec<V>>,
    clocks: Cell<usize>,
}
impl Host {
    fn new(i: &Value) -> Rc<Self> {
        let primary = Rc::new(Node::Error {
            name: "Error".encode_utf16().collect(),
            message: ["primary-".encode_utf16().collect::<Vec<_>>(), vec![0xd800]].concat(),
        });
        let orders = arr(vec![obj(vec![("id", t("O0")), ("price", n(17.5))])]);
        let positions = arr(vec![obj(vec![
            ("id", t("P0")),
            ("contracts", n(3.5)),
            ("lastUpdate", n(7.)),
        ])]);
        let raw = arr(vec![obj(vec![("totalEquity", t("53.75"))])]);
        let balance = obj(vec![
            ("total", n(53.75)),
            ("upnl", n(1.25)),
            ("info", obj(vec![("tag", t("BAL"))])),
        ]);
        let formatted = obj(vec![
            ("total", n(67.125)),
            ("upnl", n(2.5)),
            ("info", obj(vec![("tag", t("FORMATTED"))])),
        ]);
        let cached = obj(vec![
            ("total", n(29.25)),
            ("info", obj(vec![("tag", t("CACHE"))])),
        ]);
        let stored = arr(vec![obj(vec![
            ("id", t("S0")),
            ("contracts", n(4.25)),
            ("lastUpdate", n(11.)),
        ])]);
        let merged = arr([items(&positions), items(&stored)].concat());
        let synced = obj(vec![
            ("total", n(29.25)),
            ("upnl", n(8.5)),
            ("info", obj(vec![("tag", t("SYNCED"))])),
        ]);
        let stores = [0, 1].map(|idx| {
            obj(vec![
                ("orders", arr(vec![])),
                ("positions", stored.clone()),
                ("balance", cached.clone()),
                (
                    "loaded",
                    obj(vec![
                        ("sentinel", n(if idx == 0 { 11.5 } else { 91.5 })),
                        ("orders", b(false)),
                        ("positions", b(false)),
                        ("balance", b(false)),
                    ]),
                ),
            ])
        });
        let owner = obj(vec![
            ("store", stores[0].clone()),
            ("isDisposed", b(i["disposed"] == true)),
            (
                "accountType",
                t(i["accountType"].as_str().unwrap_or("CONTRACT")),
            ),
            ("forceUnifiedBalance", b(i["forceUnified"] == true)),
            (
                "lastWebsocketPositionsUpdate",
                i.get("lastWs")
                    .filter(|v| !v.is_null())
                    .map(from)
                    .unwrap_or_else(u),
            ),
            ("needsReinit", b(i["needsReinit"] == true)),
            ("lastReinitAt", n(i["lastReinit"].as_f64().unwrap_or(0.))),
            ("reinitRetryMs", n(i["retryMs"].as_f64().unwrap_or(15000.))),
            ("unifiedMarginStatus", n(0.)),
            ("xhr", obj(vec![])),
        ]);
        let mut refs = vec![
            (primary.clone(), "primary".into()),
            (orders.clone(), "orders".into()),
            (positions.clone(), "positions".into()),
            (raw.clone(), "raw".into()),
            (balance.clone(), "balance".into()),
            (formatted.clone(), "formatted".into()),
            (cached, "cached".into()),
            (stored, "storedPositions".into()),
            (merged.clone(), "merged".into()),
            (synced.clone(), "synced".into()),
            (stores[0].clone(), "store0".into()),
            (stores[1].clone(), "store1".into()),
        ];
        refs.push((items(&orders)[0].clone(), "order0".into()));
        refs.push((items(&positions)[0].clone(), "position0".into()));
        refs.push((
            items(&field(&stores[0], "positions"))[0].clone(),
            "storedPosition0".into(),
        ));
        let values = BTreeMap::from([
            ("fetchOrders".into(), orders.clone()),
            ("fetchPositions".into(), positions.clone()),
            ("fetchBalance".into(), balance),
            ("fetchRawBalance".into(), raw),
            ("fetchAndSetPrivateSnapshotHelper".into(), u()),
            ("fetchOrdersPositionsAndBalance".into(), u()),
            ("waitForRemoteSigner".into(), b(i["signer"] != false)),
            ("fetchOrdersHelper".into(), orders),
            ("fetchPositionsHelper".into(), positions),
            ("formatted".into(), formatted),
            ("merged".into(), merged),
            ("synced".into(), synced),
        ]);
        Rc::new_cyclic(|weak| Self {
            weak: weak.clone(),
            input: i.clone(),
            owner,
            stores,
            refs: RefCell::new(refs),
            events: RefCell::new(vec![]),
            writes: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            primary,
            values,
            index: Cell::new(0),
            mapper: RefCell::new("old".into()),
            loops: RefCell::new(vec![]),
            clocks: Cell::new(0),
        })
    }
    fn h(&self) -> Rc<Self> {
        self.weak.upgrade().unwrap()
    }
    fn observed(&self, v: &V) -> Value {
        if let Some((_, name)) = self.refs.borrow().iter().find(|(r, _)| Rc::ptr_eq(r, v)) {
            return json!({"ref":name});
        }
        match v.as_ref() {
            Node::Array(v) => {
                json!({"a":v.borrow().iter().map(|v|self.observed(v)).collect::<Vec<_>>()})
            }
            Node::Obj(v) => {
                json!({"o":v.borrow().iter().map(|(k,v)|json!([k.encode_utf16().collect::<Vec<_>>(),self.observed(v)])).collect::<Vec<_>>()})
            }
            _ => decoded(v),
        }
    }
    fn rec(&self, name: &str, args: Vec<V>) -> Result<(), V> {
        self.events
            .borrow_mut()
            .push(json!({"name":name,"args":self.observed(&arr(args))}));
        if self.input["fail"] == name {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn function(&self, owner: V, code: &str, captures: Vec<V>) -> V {
        Rc::new(Node::Fn {
            owner,
            code: code.into(),
            captures,
        })
    }
    fn type_error(&self, v: &V, key: &str) -> V {
        Rc::new(Node::Error {
            name: "TypeError".encode_utf16().collect(),
            message: format!(
                "Cannot read properties of {} (reading '{key}')",
                if matches!(v.as_ref(), Node::Null) {
                    "null"
                } else {
                    "undefined"
                }
            )
            .encode_utf16()
            .collect(),
        })
    }
    fn poll_task(&self, index: usize) {
        let task = self.tasks.borrow()[index].clone();
        let body = task.body.borrow_mut().take();
        if let Some(mut body) = body {
            let waker = Waker::from(Arc::new(TaskWake {
                queue: self.queue.clone(),
                index,
            }));
            let mut cx = Context::from_waker(&waker);
            match body.as_mut().poll(&mut cx) {
                Poll::Ready(result) => {
                    let Node::Promise(p) = task.promise.as_ref() else {
                        panic!("promise")
                    };
                    *p.state.borrow_mut() = Some(result);
                    let listeners = std::mem::take(&mut *p.listeners.borrow_mut());
                    for listener in listeners {
                        listener.wake()
                    }
                }
                Poll::Pending => *task.body.borrow_mut() = Some(body),
            }
        }
    }
    fn turn(&self) {
        let job = self.queue.lock().unwrap().pop_front();
        match job {
            Some(Job::Task(index)) => self.poll_task(index),
            Some(Job::Mapper(name)) => {
                self.rec(&name, vec![]).unwrap();
                *self.mapper.borrow_mut() = "new".into()
            }
            None => panic!("SNAPSHOT pending native task has no queued continuation"),
        }
    }
    fn run(&self, p: V) -> Result<V, V> {
        if !matches!(p.as_ref(), Node::Promise(_)) {
            return Ok(p);
        }
        for _ in 0..256 {
            if let Node::Promise(v) = p.as_ref() {
                if let Some(result) = v.state.borrow().clone() {
                    return result;
                }
            }
            self.turn()
        }
        panic!("SNAPSHOT native settlement exceeded 256 jobs")
    }
    fn drain(&self) {
        for _ in 0..256 {
            if self.queue.lock().unwrap().is_empty() {
                assert!(
                    self.tasks
                        .borrow()
                        .iter()
                        .all(|t| t.body.borrow().is_none()),
                    "SNAPSHOT native unresolved owned sibling"
                );
                return;
            }
            self.turn()
        }
        panic!("SNAPSHOT native sibling drain exceeded 256 jobs")
    }
    fn dependency(&self, name: &str, args: Vec<V>) -> V {
        let h = self.h();
        let name = name.to_owned();
        self.launch(Box::pin(async move {
            h.rec(&name, args)?;
            let delay = h.input["delays"][&name].as_u64().unwrap_or(1);
            for _ in 0..delay {
                pause().await
            }
            h.rec(&format!("resolved:{name}"), vec![])?;
            if h.input["disposeAfter"] == name {
                set_field(&h.owner, "isDisposed", b(true))
            }
            if name == "fetchOrdersPositionsAndBalance" {
                if h.input["reinitFails"] == true {
                    h.set(&h.owner, "needsReinit", b(true))?
                }
                if h.input["reinitNan"] == true {
                    h.set(&h.owner, "reinitRetryMs", n(f64::NAN))?
                }
            }
            if h.input["failAsync"] == name {
                return Err(h.primary.clone());
            }
            Ok(h.values[&name].clone())
        }))
    }
    fn root(&self) -> Result<V, V> {
        let h = self.h();
        let owner = self.owner.clone();
        Ok(match self.input["op"].as_str().unwrap() {
            "all" => api::fetch_orders_positions_and_balance(h, owner),
            "snapshot" => api::fetch_and_set_private_snapshot_helper(
                h,
                owner,
                self.input["fetchOrders"] != false,
            ),
            "wrapper" => api::fetch_and_set_private_snapshot(h, owner),
            "tick" => api::tick(h, owner),
            "sync" => {
                api::sync_balance_upnl_from_positions(self, &owner)?;
                u()
            }
            "raw" => api::fetch_raw_balance(h, owner),
            "orders" => api::fetch_orders(h, owner),
            "ordersHelper" => api::fetch_orders_helper(
                h,
                owner,
                t("linear"),
                t(self.input["settle"].as_str().unwrap_or("USDT")),
            ),
            "positions" => api::fetch_positions(
                h,
                owner,
                self.input.get("symbols").map(from).unwrap_or_else(u),
            ),
            "positionsHelper" => api::fetch_positions_helper(
                h,
                owner,
                self.input.get("symbols").map(from).unwrap_or_else(u),
                t(self.input["category"].as_str().unwrap_or("linear")),
                t(self.input["settle"].as_str().unwrap_or("USDT")),
            ),
            _ => panic!("op"),
        })
    }
}
impl SnapshotHost for Host {
    type Value = V;
    type Error = V;
    fn get(&self, owner: &V, key: &str) -> Result<V, V> {
        if matches!(owner.as_ref(), Node::U | Node::Null) {
            return Err(self.type_error(owner, key));
        }
        if key == "loaded" && self.input["swapLoaded"] == true {
            for (i, s) in self.stores.iter().enumerate() {
                if Rc::ptr_eq(s, owner) {
                    self.rec(&format!("loaded{i}"), vec![])?;
                    set_field(&self.owner, "store", self.stores[1].clone())
                }
            }
        }
        if Rc::ptr_eq(owner, &self.owner) && matches!(key, "mapOrder" | "mapPosition") {
            return Ok(self.function(
                owner.clone(),
                &format!("{key}:{}", self.mapper.borrow()),
                vec![],
            ));
        }
        if key == "data" {
            if let Some((_, tag)) = self
                .refs
                .borrow()
                .iter()
                .find(|(v, n)| Rc::ptr_eq(v, owner) && n.starts_with("response"))
            {
                let tag = tag.clone();
                let index = tag.strip_prefix("response").unwrap();
                self.rec(&format!("data{index}"), vec![])?;
                if self.input["mapperMicrotask"] == true {
                    self.queue
                        .lock()
                        .unwrap()
                        .push_back(Job::Mapper("mapperMutation".into()))
                }
            }
        }
        let v = field(owner, key);
        if !matches!(v.as_ref(), Node::U) {
            return Ok(v);
        }
        let method = (Rc::ptr_eq(owner, &self.owner)
            && (self.values.contains_key(key)
                || matches!(
                    key,
                    "emitAndLogError"
                        | "getTickInterval"
                        | "formatBalance"
                        | "getSettleCoinsForCategory"
                        | "getCategoryForSymbol"
                        | "preserveRecentlyPlacedOrders"
                        | "dedupePositionsById"
                        | "reinitOnThrow"
                        | "tick"
                )))
            || (Rc::ptr_eq(owner, &field(&self.owner, "xhr")) && key == "get")
            || (self.stores.iter().any(|s| Rc::ptr_eq(s, owner))
                && matches!(key, "update" | "addOrUpdatePositionsFull"))
            || (matches!(owner.as_ref(), Node::Array(_))
                && matches!(key, "flat" | "map" | "flatMap"))
            || (matches!(owner.as_ref(), Node::Fn { .. }) && key == "bind")
            || (matches!(owner.as_ref(), Node::Promise(_)) && matches!(key, "then" | "catch"));
        Ok(if method {
            self.function(owner.clone(), key, vec![])
        } else {
            v
        })
    }
    fn optional(&self, owner: &V, key: &str) -> Result<V, V> {
        if matches!(owner.as_ref(), Node::U | Node::Null) {
            Ok(u())
        } else {
            self.get(owner, key)
        }
    }
    fn set(&self, owner: &V, key: &str, value: V) -> Result<(), V> {
        if Rc::ptr_eq(owner, &self.owner)
            && matches!(
                key,
                "forceUnifiedBalance"
                    | "needsReinit"
                    | "lastReinitAt"
                    | "reinitRetryMs"
                    | "unifiedMarginStatus"
            )
        {
            self.rec(&format!("set:{key}"), vec![value.clone()])?
        }
        set_field(owner, key, value);
        Ok(())
    }
    fn select(&self, owner: V, key: &str) -> Result<V, V> {
        self.get(&owner, key)
    }
    fn apply(&self, selected: V, args: Vec<V>) -> Result<V, V> {
        let Node::Fn {
            owner,
            code,
            captures,
        } = selected.as_ref()
        else {
            return Err(Rc::new(Node::Error {
                name: "TypeError".encode_utf16().collect(),
                message: "not callable".encode_utf16().collect(),
            }));
        };
        let first = args.first().cloned().unwrap_or_else(u);
        if self.values.contains_key(code) {
            return Ok(self.dependency(code, args));
        }
        if matches!(
            code.as_str(),
            "orders_then"
                | "positions_then"
                | "order_settle"
                | "position_settle"
                | "position_symbol"
                | "ignore"
                | "tick_callback"
        ) {
            return api::dispatch(
                self.h(),
                owner.clone(),
                if code == "tick_callback" {
                    "tick"
                } else {
                    code
                },
                captures.clone(),
                args,
            );
        }
        match code.as_str() {
            "then" | "catch" => {
                let h = self.h();
                let parent = owner.clone();
                let callback = first;
                let catch = code == "catch";
                Ok(self.launch(Box::pin(async move {
                    let result = h.wait(parent).await;
                    match result {
                        Ok(v) if !catch => h.apply(callback, vec![v]),
                        Err(e) if catch => h.apply(callback, vec![e]),
                        other => other,
                    }
                })))
            }
            "bind" => {
                let Node::Fn { code, captures, .. } = owner.as_ref() else {
                    panic!("bind")
                };
                Ok(self.function(first, code, captures.clone()))
            }
            "flat" => {
                let mut out = vec![];
                for row in items(owner) {
                    if matches!(row.as_ref(), Node::Array(_)) {
                        out.extend(items(&row))
                    } else {
                        out.push(row)
                    }
                }
                Ok(arr(out))
            }
            "map" | "flatMap" => {
                let flatten = code == "flatMap";
                let mut out = vec![];
                for row in items(owner) {
                    let mapped = self.apply(first.clone(), vec![row])?;
                    if flatten && matches!(mapped.as_ref(), Node::Array(_)) {
                        out.extend(items(&mapped))
                    } else {
                        out.push(mapped)
                    }
                }
                Ok(arr(out))
            }
            "update" => {
                let index = self
                    .stores
                    .iter()
                    .position(|s| Rc::ptr_eq(s, owner))
                    .unwrap();
                self.rec(&format!("update{index}"), args.clone())?;
                self.writes
                    .borrow_mut()
                    .push(json!({"store":index,"value":decoded(&first)}));
                let Node::Obj(v) = first.as_ref() else {
                    panic!("update")
                };
                for (k, v) in v.borrow().iter() {
                    set_field(owner, k, v.clone())
                }
                Ok(u())
            }
            "addOrUpdatePositionsFull" => {
                let index = self
                    .stores
                    .iter()
                    .position(|s| Rc::ptr_eq(s, owner))
                    .unwrap();
                self.rec(&format!("positionsFull{index}"), args)?;
                set_field(owner, "positions", first);
                Ok(u())
            }
            "emitAndLogError" => {
                self.rec("emit", args)?;
                Ok(u())
            }
            "formatBalance" => {
                self.rec("format", args)?;
                Ok(self.values["formatted"].clone())
            }
            "getTickInterval" => {
                self.rec("interval", args)?;
                Ok(n(500.))
            }
            "getSettleCoinsForCategory" => {
                self.rec("coins", args)?;
                Ok(self
                    .input
                    .get("coins")
                    .map(from)
                    .unwrap_or_else(|| arr(vec![t("USDT"), t("USDC")])))
            }
            "getCategoryForSymbol" => {
                self.rec("category", args)?;
                let mut name = "category.".encode_utf16().collect::<Vec<_>>();
                name.extend(text(&first));
                Ok(Rc::new(Node::Text(name)))
            }
            "preserveRecentlyPlacedOrders" => {
                self.rec("preserve", args)?;
                Ok(first)
            }
            "dedupePositionsById" => {
                self.rec("dedupe", args)?;
                Ok(first)
            }
            "reinitOnThrow" => {
                self.rec("reinit", args)?;
                Ok(first)
            }
            "tick" => {
                self.rec("tickCallback", vec![])?;
                Ok(obj(vec![("ignored", b(true))]))
            }
            "get" => {
                let i = self.index.get();
                self.index.set(i + 1);
                self.rec(&format!("get{i}"), vec![first, field(&args[1], "params")])?;
                let h = self.h();
                let p=self.launch(Box::pin(async move{for _ in 0..h.input["getDelay"].as_u64().unwrap_or(1){pause().await}h.rec(&format!("resolved:get{i}"),vec![])?;if h.input["failAsync"]==format!("get{i}"){return Err(h.primary.clone())}let pages=h.input.get("pages").cloned().unwrap_or(json!([{"retMsg":"OK","retCode":0,"result":{"list":[{"id":format!("r{i}"),"tag":format!("RAW-{i}")}],"nextPageCursor":""}}]));let pages=pages.as_array().unwrap();let data=from(&pages[i.min(pages.len()-1)]);let list=field(&field(&data,"result"),"list");if matches!(list.as_ref(),Node::Array(_)){for(index,row)in items(&list).into_iter().enumerate(){if matches!(row.as_ref(),Node::Obj(_)|Node::Array(_)){h.refs.borrow_mut().push((row,format!("row{i}.{index}")));}}}let response=obj(vec![("data",data)]);h.refs.borrow_mut().push((response.clone(),format!("response{i}")));Ok(response)}));
                self.refs
                    .borrow_mut()
                    .push((p.clone(), format!("request{i}")));
                Ok(p)
            }
            code if code.starts_with("mapOrder:") || code.starts_with("mapPosition:") => {
                self.rec(code, args)?;
                let mut id = if code.starts_with("mapOrder") {
                    "O."
                } else {
                    "P."
                }
                .encode_utf16()
                .collect::<Vec<_>>();
                id.extend(text(&field(&first, "id")));
                let id = Rc::new(Node::Text(id));
                Ok(if code.starts_with("mapOrder") {
                    arr(vec![obj(vec![
                        ("id", id),
                        ("side", t("Buy")),
                        ("price", n(17.5)),
                        ("filled", n(1.25)),
                        ("info", first),
                    ])])
                } else {
                    obj(vec![
                        ("id", id),
                        ("side", t("Long")),
                        ("contracts", n(3.5)),
                        ("info", first),
                    ])
                })
            }
            _ => panic!("unimplemented native host call {code}"),
        }
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        equal(a, b)
    }
    fn undefined(&self) -> V {
        u()
    }
    fn literal(&self, s: &str) -> V {
        t(s)
    }
    fn boolean(&self, v: bool) -> V {
        b(v)
    }
    fn number(&self, v: f64) -> V {
        n(v)
    }
    fn numeric(&self, v: &V) -> Result<f64, V> {
        Ok(jsnum(v))
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> V {
        obj(v)
    }
    fn array(&self, v: Vec<V>) -> V {
        arr(v)
    }
    fn values(&self, v: &V) -> Result<Vec<V>, V> {
        Ok(items(v))
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v.as_ref(), Node::Array(_))
    }
    fn spread(&self, v: V, fields: Vec<(&'static str, V)>) -> Result<V, V> {
        let out = match v.as_ref() {
            Node::Obj(v) => Rc::new(Node::Obj(RefCell::new(v.borrow().clone()))),
            _ => obj(vec![]),
        };
        for (k, v) in fields {
            set_field(&out, k, v)
        }
        Ok(out)
    }
    fn intrinsic(&self, name: &str, args: Vec<V>) -> Result<V, V> {
        match name {
            "null" => Ok(Rc::new(Node::Null)),
            "virtualClock.getCurrentTimeMs" => {
                self.rec("clock", args)?;
                Ok(n(self.input["time"].as_f64().unwrap_or(1000.)))
            }
            "Date.now" => {
                self.rec("now", args)?;
                let i = self.clocks.get();
                self.clocks.set(i + 1);
                let value = if let Some(v) = self.input["clocks"].as_array() {
                    v[i.min(v.len() - 1)].as_f64().unwrap()
                } else {
                    self.input["time"].as_f64().unwrap_or(15000.)
                };
                Ok(n(value))
            }
            "Math.min" => {
                let a = jsnum(&args[0]);
                let b = jsnum(&args[1]);
                Ok(n(if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.min(b)
                }))
            }
            "v" => {
                let v = &args[0];
                if matches!(v.as_ref(), Node::U | Node::Null) {
                    return Ok(u());
                }
                let key = String::from_utf16_lossy(&text(&args[1]));
                if let Node::Obj(fields) = v.as_ref() {
                    let fields = fields.borrow();
                    if let Some((_, v)) = fields.iter().find(|(k, _)| k == &key) {
                        return Ok(v.clone());
                    }
                    let snake = key
                        .chars()
                        .flat_map(|c| {
                            if c.is_uppercase() {
                                vec!['_', c.to_ascii_lowercase()]
                            } else {
                                vec![c]
                            }
                        })
                        .collect::<String>();
                    return Ok(fields
                        .iter()
                        .find(|(k, _)| k == &snake)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_else(u));
                }
                Ok(u())
            }
            "mergeUpdatesUsingLastUpdate" => {
                self.rec("merge", args)?;
                Ok(self.values["merged"].clone())
            }
            "syncUnifiedBalanceUpnlFromPositions" => {
                self.rec("sync", args.clone())?;
                if self.input["swapBalance"] == true {
                    set_field(
                        &field(&self.owner, "store"),
                        "balance",
                        self.values["synced"].clone(),
                    )
                }
                Ok(match self.input["sync"].as_str() {
                    Some("same") => args[0].clone(),
                    Some("null") => Rc::new(Node::Null),
                    Some("zero") => n(0.),
                    _ => self.values["synced"].clone(),
                })
            }
            "loop" => {
                let Node::Fn { owner, code, .. } = args[0].as_ref() else {
                    panic!("loop callback")
                };
                assert!(
                    Rc::ptr_eq(owner, &self.owner),
                    "SNAPSHOT native loop callback owner identity"
                );
                assert_eq!(code, "tick_callback", "SNAPSHOT native loop callback code");
                let event_fn = self.function(owner.clone(), "tick", vec![]);
                self.rec("loop", vec![event_fn, args[1].clone()])?;
                self.loops.borrow_mut().push(args[0].clone());
                Ok(u())
            }
            "mapSeries" => {
                let h = self.h();
                let array = args[0].clone();
                let callback = args[1].clone();
                Ok(self.launch(Box::pin(async move {
                    let mut out = vec![];
                    let mut index = 0;
                    while index < items(&array).len() {
                        let value = h.wait(items(&array)[index].clone()).await?;
                        let p = h.apply(
                            callback.clone(),
                            vec![value, n(index as f64), array.clone()],
                        )?;
                        out.push(h.wait(p).await?);
                        index += 1
                    }
                    Ok(arr(out))
                })))
            }
            "Promise.all" => {
                let values = items(&args[0]);
                Ok(self.launch(Box::pin(std::future::poll_fn(move |cx| {
                    let mut results = vec![];
                    let mut pending = false;
                    for value in &values {
                        match value.as_ref() {
                            Node::Promise(p) => match p.state.borrow().clone() {
                                Some(Err(e)) => return Poll::Ready(Err(e)),
                                Some(Ok(v)) => results.push(v),
                                None => {
                                    pending = true;
                                    p.listeners.borrow_mut().push(cx.waker().clone());
                                }
                            },
                            _ => results.push(value.clone()),
                        }
                    }
                    if pending {
                        Poll::Pending
                    } else {
                        Poll::Ready(Ok(arr(results)))
                    }
                }))))
            }
            _ => panic!("unknown native intrinsic {name}"),
        }
    }
    fn callback(&self, owner: V, code: &'static str, captures: Vec<V>) -> Result<V, V> {
        Ok(self.function(
            owner,
            if code == "tick" {
                "tick_callback"
            } else {
                code
            },
            captures,
        ))
    }
    fn launch(&self, body: Operation<V, V>) -> V {
        let h = self.h();
        let promise = Rc::new(Node::Promise(Promise {
            state: RefCell::new(None),
            listeners: RefCell::new(vec![]),
        }));
        let task = Rc::new(Task {
            promise: promise.clone(),
            body: RefCell::new(Some(Box::pin(async move {
                let value = body.await?;
                if matches!(value.as_ref(), Node::Promise(_)) {
                    h.wait(value).await
                } else {
                    Ok(value)
                }
            }))),
        });
        let index = self.tasks.borrow().len();
        self.tasks.borrow_mut().push(task);
        self.poll_task(index);
        promise
    }
    fn wait(&self, value: V) -> Operation<V, V> {
        let settled = match value.as_ref() {
            Node::Promise(p) => p.state.borrow().is_some(),
            _ => true,
        };
        Box::pin(async move {
            if settled {
                pause().await
            }
            std::future::poll_fn(move |cx| match value.as_ref() {
                Node::Promise(p) => {
                    if let Some(result) = p.state.borrow().clone() {
                        Poll::Ready(result)
                    } else {
                        p.listeners.borrow_mut().push(cx.waker().clone());
                        Poll::Pending
                    }
                }
                _ => Poll::Ready(Ok(value.clone())),
            })
            .await
        })
    }
    fn error(&self, message: V) -> Result<V, V> {
        self.rec("error", vec![message.clone()])?;
        Ok(Rc::new(Node::Error {
            name: "Error".encode_utf16().collect(),
            message: if matches!(message.as_ref(), Node::U) {
                vec![]
            } else {
                text(&message)
            },
        }))
    }
    fn error_value(&self, error: V) -> V {
        error
    }
}
#[test]
fn core_snapshot_readers_native_source_oracle() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/core-snapshot-readers-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SNAPSHOT source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 80, "SNAPSHOT independent scenario floor");
    let mut counts = BTreeMap::new();
    let mut supplemental = vec![];
    for (index, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let op = input["op"].as_str().unwrap();
        *counts.entry(op).or_insert(0) += 1;
        let label = format!("SNAPSHOT {op} iteration {index}");
        let h = Host::new(input);
        let start = h.root();
        let before = h.events.borrow().clone();
        let result = start.and_then(|p| h.run(p));
        h.drain();
        assert_eq!(
            result.is_err(),
            !row["error"].is_null(),
            "{label}: native rejection discriminant"
        );
        match result {
            Ok(v) => {
                assert_native(&v, &row["result"], &label);
                if matches!(op, "orders" | "positions") {
                    for value in items(&v) {
                        assert!(
                            h.refs.borrow().iter().any(|(original, name)| matches!(
                                name.as_str(),
                                "order0" | "position0"
                            ) && Rc::ptr_eq(
                                original, &value
                            )),
                            "{label}: native flattened original row identity"
                        );
                    }
                }
                if matches!(op, "ordersHelper" | "positionsHelper") {
                    for mapped in items(&v) {
                        let info = field(&mapped, "info");
                        assert!(
                            h.refs.borrow().iter().any(
                                |(row, name)| name.starts_with("row") && Rc::ptr_eq(row, &info)
                            ),
                            "{label}: native mapped info original row identity"
                        );
                    }
                }
                assert_eq!(
                    h.observed(&v),
                    row["resultRef"],
                    "{label}: native result allocation identity"
                );
            }
            Err(e) => {
                assert_native(&e, &row["error"], &label);
                assert_eq!(
                    Rc::ptr_eq(&e, &h.primary),
                    row["primary"].as_bool().unwrap(),
                    "{label}: native original error identity"
                );
            }
        }
        for (i, store) in h.stores.iter().enumerate() {
            for key in ["orders", "positions", "balance", "loaded"] {
                assert_native(
                    &field(store, key),
                    &row["stores"][i][key],
                    &format!("{label}: native store{i}.{key}"),
                )
            }
        }
        let state = obj(vec![
            ("disposed", field(&h.owner, "isDisposed")),
            ("forceUnified", field(&h.owner, "forceUnifiedBalance")),
            ("needsReinit", field(&h.owner, "needsReinit")),
            ("lastReinit", field(&h.owner, "lastReinitAt")),
            ("retryMs", field(&h.owner, "reinitRetryMs")),
            ("unified", field(&h.owner, "unifiedMarginStatus")),
            (
                "store",
                n(if Rc::ptr_eq(&field(&h.owner, "store"), &h.stores[0]) {
                    0.
                } else {
                    1.
                }),
            ),
        ]);
        assert_native(
            &state,
            &row["state"],
            &format!("{label}: native exchange state"),
        );
        assert_eq!(
            h.loops.borrow().len(),
            row["loopCount"].as_u64().unwrap() as usize,
            "{label}: native scheduled loop count"
        );
        assert_eq!(
            h.writes.borrow().as_slice(),
            row["writes"].as_array().unwrap().as_slice(),
            "{label}: native store write payloads and order"
        );
        supplemental.push((
            json!(before),
            row["before"].clone(),
            format!("{label}: eager effects"),
        ));
        supplemental.push((
            json!(*h.events.borrow()),
            row["events"].clone(),
            format!("{label}: dependency order and arguments"),
        ));
    }
    assert_eq!(
        counts,
        BTreeMap::from([
            ("all", 8),
            ("snapshot", 8),
            ("wrapper", 4),
            ("tick", 10),
            ("sync", 6),
            ("raw", 10),
            ("orders", 6),
            ("ordersHelper", 10),
            ("positions", 6),
            ("positionsHelper", 12)
        ]),
        "SNAPSHOT independent family floors"
    );
    for (a, e, label) in supplemental {
        assert_eq!(a, e, "{label}")
    }
    eprintln!(
        "SNAPSHOT stopped at iteration 79 (positions snake-case empty page); source floor80, native settlement/drain bound256 turns; no wire assertions"
    );
}

#[path = "support/private_snapshot_refresh.rs"]
mod private_snapshot_refresh_tests;

#[path = "support/local_order_reconciliation.rs"]
mod local_reconciliation_tests;
