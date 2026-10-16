//! BybitExchange.start: native SnapshotHost witness against the maintained source body.
use safe_cex_venue_bybit::{
    core_snapshot_readers::{Operation, SnapshotHost},
    primary_startup,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::VecDeque,
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
    Error { name: Vec<u16>, message: Vec<u16> },
    Fn { owner: V, code: String },
    Promise(Promise),
}
#[derive(Debug)]
struct Promise {
    state: RefCell<Option<Result<V, V>>>,
    listeners: RefCell<Vec<Waker>>,
}
struct TaskWake {
    queue: Arc<Mutex<VecDeque<usize>>>,
    index: usize,
}
impl Wake for TaskWake {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref()
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.queue.lock().unwrap().push_back(self.index);
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
fn units(v: &str) -> Vec<u16> {
    v.encode_utf16().collect()
}
fn arr(v: Vec<V>) -> V {
    Rc::new(Node::Array(RefCell::new(v)))
}
fn obj(v: Vec<(&str, V)>) -> V {
    Rc::new(Node::Obj(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn error(message: Vec<u16>) -> V {
    Rc::new(Node::Error {
        name: units("Error"),
        message,
    })
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
            .find(|(name, _)| name == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(u),
        Node::Array(a) if k == "length" => n(a.borrow().len() as f64),
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
    match o.iter_mut().find(|(name, _)| name == k) {
        Some((_, slot)) => *slot = value,
        None => o.push((k.into(), value)),
    }
}
fn items(v: &V) -> Vec<V> {
    let Node::Array(a) = v.as_ref() else {
        panic!("native array")
    };
    a.borrow().clone()
}
// String(value) on the fixture domain.
fn js_string(v: &V) -> Vec<u16> {
    match v.as_ref() {
        Node::Text(s) => s.clone(),
        Node::U => units("undefined"),
        Node::Null => units("null"),
        Node::Num(v) => units(&safe_cex_num::js_number_string(*v)),
        Node::Bool(v) => units(&v.to_string()),
        Node::Error { name, message } if message.is_empty() => name.clone(),
        Node::Error { name, message } => [name.clone(), units(": "), message.clone()].concat(),
        _ => units("[object Object]"),
    }
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
const DEPENDENCIES: [&str; 9] = [
    "virtualClockStart",
    "fetchFees",
    "loadAccountMarketOverlay",
    "initExternalPublicData",
    "fetchMarginAccountInfos",
    "fetchAndCacheApiKeyInfo",
    "fetchMarkets",
    "fetchTickersWithoutParse",
    "fetchOrdersPositionsAndBalance",
];
struct Host {
    weak: Weak<Host>,
    input: Value,
    owner: V,
    store: V,
    global: V,
    primary: V,
    rejection: V,
    markets: V,
    raw_tickers: V,
    tickers: V,
    fees: V,
    refs: Vec<(V, &'static str)>,
    events: RefCell<Vec<Value>>,
    tasks: RefCell<Vec<Rc<Task>>>,
    queue: Arc<Mutex<VecDeque<usize>>>,
}
impl Host {
    fn new(input: &Value) -> Rc<Self> {
        let primary = error([units("primary-"), vec![0xd800]].concat());
        let empty_error = error(vec![]);
        let rejection = match input["probeReject"].as_str() {
            Some("empty-string") => t(""),
            Some("empty-error") => empty_error.clone(),
            _ => primary.clone(),
        };
        let markets = arr(vec![
            obj(vec![("symbol", t("BTCUSDT"))]),
            obj(vec![("symbol", t("ETHUSDT"))]),
        ]);
        let raw_tickers = arr(vec![obj(vec![
            ("symbol", t("BTCUSDT")),
            ("lastPrice", t("1")),
        ])]);
        let tickers = arr(vec![obj(vec![("symbol", t("BTCUSDT")), ("last", n(1.))])]);
        let fees = obj(vec![("linear", b(true))]);
        let loaded = obj(vec![
            ("sentinel", n(7.5)),
            ("markets", b(false)),
            ("tickers", b(false)),
            ("balance", b(false)),
            ("positions", b(false)),
            ("orders", b(false)),
        ]);
        let store = obj(vec![("loaded", loaded)]);
        let socket = || obj(vec![]);
        let (public, private, inverse) = (socket(), socket(), socket());
        let options = obj(vec![
            ("key", t(if input["noKey"] == true { "" } else { "key" })),
            (
                "secret",
                t(if input["noSecret"] == true {
                    ""
                } else {
                    "secret"
                }),
            ),
            (
                "remoteSigner",
                if input["remoteSigner"] == true {
                    obj(vec![("signer", b(true))])
                } else {
                    u()
                },
            ),
            ("payloadSigner", u()),
        ]);
        let owner = obj(vec![
            ("batchLimit", n(3.)),
            ("isPublicDataOnly", b(input["publicOnly"] == true)),
            ("needsAccountMarketOverlay", b(input["overlay"] == true)),
            ("isDisposed", b(false)),
            ("store", store.clone()),
            ("publicWebsocket", public.clone()),
            ("privateWebsocket", private.clone()),
            (
                "inversePublicWebsocket",
                if input["inverse"] == true {
                    inverse.clone()
                } else {
                    u()
                },
            ),
            ("options", options),
        ]);
        let global = obj(vec![
            ("virtualClock", obj(vec![])),
            ("Promise", obj(vec![])),
            ("LogSeverity", obj(vec![("Warning", t("warning"))])),
        ]);
        let refs = vec![
            (primary.clone(), "primary"),
            (empty_error, "emptyError"),
            (markets.clone(), "markets"),
            (raw_tickers.clone(), "rawTickers"),
            (tickers.clone(), "tickers"),
            (fees.clone(), "fees"),
            (store.clone(), "store"),
            (public, "public"),
            (private, "private"),
            (inverse, "inverse"),
        ];
        Rc::new_cyclic(|weak| Self {
            weak: weak.clone(),
            input: input.clone(),
            owner,
            store,
            global,
            primary,
            rejection,
            markets,
            raw_tickers,
            tickers,
            fees,
            refs,
            events: RefCell::default(),
            tasks: RefCell::default(),
            queue: Arc::default(),
        })
    }
    fn h(&self) -> Rc<Self> {
        self.weak.upgrade().unwrap()
    }
    fn label(&self, v: &V) -> Option<&'static str> {
        self.refs
            .iter()
            .find(|(r, _)| Rc::ptr_eq(r, v))
            .map(|(_, name)| *name)
    }
    fn atom(&self, v: &V) -> Value {
        match v.as_ref() {
            Node::U => json!({"u":true}),
            Node::Null => Value::Null,
            Node::Bool(v) => json!(v),
            Node::Num(v) => {
                json!({"n":if v.is_nan(){"nan".into()}else{format!("{:016x}",v.to_bits())}})
            }
            Node::Text(v) => json!({"s":v}),
            _ if self.label(v).is_some() => json!({"ref":self.label(v)}),
            Node::Error { name, message } => json!({"error":true,"name":name,"message":message}),
            Node::Array(v) => {
                json!({"a":v.borrow().iter().map(|v|self.atom(v)).collect::<Vec<_>>()})
            }
            Node::Obj(v) => {
                json!({"o":v.borrow().iter().map(|(k,v)|json!([units(k),self.atom(v)])).collect::<Vec<_>>()})
            }
            Node::Fn { .. } => json!({"fn":"unknown"}),
            Node::Promise(_) => json!({"o":[]}),
        }
    }
    fn rec(&self, name: &str, args: Vec<V>) -> Result<(), V> {
        self.events
            .borrow_mut()
            .push(json!({"name":name,"args":self.atom(&arr(args))}));
        if self.input["fail"] == name {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn function(&self, owner: V, code: &str) -> V {
        Rc::new(Node::Fn {
            owner,
            code: code.into(),
        })
    }
    fn poll_task(&self, index: usize) {
        let task = self.tasks.borrow()[index].clone();
        let Some(mut body) = task.body.borrow_mut().take() else {
            return;
        };
        let waker = Waker::from(Arc::new(TaskWake {
            queue: self.queue.clone(),
            index,
        }));
        match body.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Ready(result) => {
                let Node::Promise(p) = task.promise.as_ref() else {
                    panic!("promise")
                };
                *p.state.borrow_mut() = Some(result);
                for listener in std::mem::take(&mut *p.listeners.borrow_mut()) {
                    listener.wake()
                }
            }
            Poll::Pending => *task.body.borrow_mut() = Some(body),
        }
    }
    fn settle(&self, p: &V) -> Result<V, V> {
        for _ in 0..512 {
            if let Node::Promise(v) = p.as_ref()
                && let Some(result) = v.state.borrow().clone()
            {
                return result;
            }
            let job = self.queue.lock().unwrap().pop_front();
            self.poll_task(job.expect("STARTUP pending native start has no queued continuation"));
        }
        panic!("STARTUP native settlement exceeded 512 jobs")
    }
    fn drain(&self) {
        for _ in 0..512 {
            let job = self.queue.lock().unwrap().pop_front();
            let Some(job) = job else { return };
            self.poll_task(job)
        }
        panic!("STARTUP native drain exceeded 512 jobs")
    }
    fn dependency(&self, name: &str, args: Vec<V>) -> V {
        let h = self.h();
        let name = name.to_owned();
        self.launch(Box::pin(async move {
            h.rec(&name, args.clone())?;
            for _ in 0..h.input["delays"][&name].as_u64().unwrap_or(1) {
                pause().await
            }
            h.rec(&format!("resolved:{name}"), vec![])?;
            if h.input["disposeAfter"] == name.as_str() {
                set_field(&h.owner, "isDisposed", b(true))
            }
            if name == "loadAccountMarketOverlay" {
                let inner = h.apply(args[0].clone(), vec![])?;
                h.wait(inner).await?;
            }
            if name == "fetchMarginAccountInfos" && !h.input["probeReject"].is_null() {
                return Err(h.rejection.clone());
            }
            if name == "fetchAndCacheApiKeyInfo" && h.input["cacheFail"] == true {
                return Err(h.primary.clone());
            }
            if h.input["failAsync"] == name.as_str() {
                return Err(h.primary.clone());
            }
            Ok(match name.as_str() {
                "fetchFees" => h.fees.clone(),
                "initExternalPublicData" => b(h.input["external"] == true),
                "fetchMarkets" => h.markets.clone(),
                "fetchTickersWithoutParse" => h.raw_tickers.clone(),
                _ => u(),
            })
        }))
    }
}
impl SnapshotHost for Host {
    type Value = V;
    type Error = V;
    fn get(&self, owner: &V, key: &str) -> Result<V, V> {
        if matches!(owner.as_ref(), Node::U | Node::Null) {
            return Err(Rc::new(Node::Error {
                name: units("TypeError"),
                message: units(&format!(
                    "Cannot read properties of undefined (reading '{key}')"
                )),
            }));
        }
        let value = field(owner, key);
        if !matches!(value.as_ref(), Node::U) {
            return Ok(value);
        }
        let label = self.label(owner);
        let method = (Rc::ptr_eq(owner, &self.owner)
            && (DEPENDENCIES.contains(&key)
                || matches!(key, "log" | "parseTickers" | "tick" | "emitAndLogError")))
            || (Rc::ptr_eq(owner, &self.store) && key == "update")
            || (matches!(label, Some("public" | "private" | "inverse"))
                && key == "connectAndSubscribe")
            || (Rc::ptr_eq(owner, &field(&self.global, "virtualClock")) && key == "start")
            || (Rc::ptr_eq(owner, &field(&self.global, "Promise")) && key == "all")
            || (Rc::ptr_eq(owner, &self.global) && matches!(key, "bybitFeeOverlay" | "String"))
            || (matches!(owner.as_ref(), Node::Promise(_)) && matches!(key, "then" | "catch"));
        Ok(if method {
            self.function(owner.clone(), key)
        } else {
            value
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
        set_field(owner, key, value);
        Ok(())
    }
    fn select(&self, owner: V, key: &str) -> Result<V, V> {
        self.get(&owner, key)
    }
    fn apply(&self, selected: V, args: Vec<V>) -> Result<V, V> {
        let Node::Fn { owner, code } = selected.as_ref() else {
            return Err(Rc::new(Node::Error {
                name: units("TypeError"),
                message: units("not callable"),
            }));
        };
        let first = args.first().cloned().unwrap_or_else(u);
        if code.starts_with("start_") {
            return primary_startup::start_callback(self.h(), owner.clone(), code, args);
        }
        if DEPENDENCIES.contains(&code.as_str()) {
            return Ok(self.dependency(code, args));
        }
        match code.as_str() {
            "start" => Ok(self.dependency("virtualClockStart", args)),
            "log" => self.rec("log", args).map(|_| u()),
            "tick" => self.rec("tick", args).map(|_| u()),
            "emitAndLogError" => self.rec("emit", args).map(|_| u()),
            "bybitFeeOverlay" => self.rec("feeOverlay", args).map(|_| u()),
            "parseTickers" => {
                self.rec("parseTickers", args)?;
                Ok(self.tickers.clone())
            }
            "connectAndSubscribe" => {
                self.rec(&format!("connect:{}", self.label(owner).unwrap()), args)?;
                Ok(u())
            }
            "update" => {
                self.rec("update", args)?;
                let Node::Obj(fields) = first.as_ref() else {
                    panic!("update")
                };
                for (k, v) in fields.borrow().iter() {
                    set_field(&self.store, k, v.clone())
                }
                Ok(u())
            }
            "String" => Ok(Rc::new(Node::Text(js_string(&first)))),
            "then" | "catch" => {
                let h = self.h();
                let parent = owner.clone();
                let (fulfilled, rejected) = if code == "then" {
                    (Some(first), args.get(1).cloned())
                } else {
                    (None, Some(first))
                };
                Ok(self.launch(Box::pin(async move {
                    match h.wait(parent).await {
                        Ok(v) => match fulfilled {
                            Some(callback) => h.apply(callback, vec![v]),
                            None => Ok(v),
                        },
                        Err(e) => match rejected {
                            Some(callback) => h.apply(callback, vec![e]),
                            None => Err(e),
                        },
                    }
                })))
            }
            "all" => {
                let values = items(&first);
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
        match v.as_ref() {
            Node::Num(v) => Ok(*v),
            _ => panic!("startup reads no other numeric"),
        }
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
            "globalThis" => Ok(self.global.clone()),
            "null" => Ok(Rc::new(Node::Null)),
            "template" => Ok(Rc::new(Node::Text(js_string(&args[0])))),
            "string.concat" => Ok(Rc::new(Node::Text(
                args.iter().flat_map(js_string).collect(),
            ))),
            "iterator.bindings" => {
                let count = self.numeric(&args[1])? as usize;
                let mut values = items(&args[0]);
                values.resize_with(count, u);
                Ok(arr(values))
            }
            _ => panic!("unknown native intrinsic {name}"),
        }
    }
    fn callback(&self, owner: V, code: &'static str, _: Vec<V>) -> Result<V, V> {
        Ok(self.function(owner, code))
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
                Node::Promise(p) => match p.state.borrow().clone() {
                    Some(result) => Poll::Ready(result),
                    None => {
                        p.listeners.borrow_mut().push(cx.waker().clone());
                        Poll::Pending
                    }
                },
                _ => Poll::Ready(Ok(value.clone())),
            })
            .await
        })
    }
    fn error(&self, message: V) -> Result<V, V> {
        Ok(error(js_string(&message)))
    }
    fn error_value(&self, error: V) -> V {
        error
    }
}
fn source_rows() -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/primary-startup-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "STARTUP source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
// The CHOSEN detached cache-failure report is the only native-only effect.
fn without_cache_report(events: &[Value], label: &str) -> Vec<Value> {
    let reports = events.iter().filter(|e| e["name"] == "emit").count();
    let expected = usize::from(label.contains("\"cacheFail\":true"));
    assert_eq!(
        reports, expected,
        "{label}: native cache-failure report count"
    );
    if expected == 1 {
        let report = events.iter().find(|e| e["name"] == "emit").unwrap();
        assert_eq!(
            report["args"],
            json!({"a":[{"ref":"primary"}]}),
            "{label}: native cache-failure reports the original rejected value"
        );
    }
    events
        .iter()
        .filter(|e| e["name"] != "emit")
        .cloned()
        .collect()
}
#[test]
fn primary_startup_native_source_witness() {
    let rows = source_rows();
    assert_eq!(rows.len(), 22, "STARTUP independent scenario floor");
    for (index, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let label = format!("STARTUP case {index} {input}");
        let h = Host::new(input);
        let started = primary_startup::start(h.clone(), h.owner.clone());
        let result = h.settle(&started);
        h.drain();
        match &result {
            Ok(_) => assert!(
                row["error"].is_null(),
                "{label}: native start resolved where source rejected"
            ),
            Err(e) => {
                assert!(
                    !row["error"].is_null(),
                    "{label}: native start rejected where source resolved"
                );
                assert_eq!(
                    Rc::ptr_eq(e, &h.primary),
                    row["primary"].as_bool().unwrap(),
                    "{label}: native original rejection identity"
                );
            }
        }
        if let Ok(v) = &result {
            assert_eq!(h.atom(v), row["result"], "{label}: native start result");
        }
        assert_eq!(
            h.atom(&field(&h.owner, "batchLimit")),
            json!({"n":format!("{:016x}",row["batchLimit"].as_f64().unwrap().to_bits())}),
            "{label}: native batchLimit"
        );
        assert_eq!(
            h.atom(&field(&h.store, "loaded")),
            row["loaded"],
            "{label}: native loaded flags"
        );
        assert_eq!(
            h.atom(&field(&h.store, "markets")),
            row["storeMarkets"],
            "{label}: native stored markets identity"
        );
        assert_eq!(
            h.atom(&field(&h.store, "tickers")),
            row["storeTickers"],
            "{label}: native stored tickers identity"
        );
        let events = h.events.borrow().clone();
        assert_eq!(
            without_cache_report(&events, &label),
            row["events"].as_array().unwrap().clone(),
            "{label}: ordered effects and arguments"
        );
    }
}
#[test]
fn primary_startup_typescript_controls_fail_by_name() {
    for (control, assertion) in [
        ("inverse-skip", "ordered effects and arguments"),
        ("probe-message", "ordered effects and arguments"),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "primary_startup_native_source_witness",
                "--nocapture",
            ])
            .env("BYBIT_STARTUP_TS_CONTROL", control)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{control}: {text}");
        assert!(text.contains(assertion), "{control}: {text}");
    }
}
