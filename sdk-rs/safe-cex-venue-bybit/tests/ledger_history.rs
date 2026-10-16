use safe_cex_venue_bybit::{
    core_snapshot_readers::{Operation, SnapshotHost},
    ledger_history as api,
};
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
    Iterator {
        values: V,
        index: Cell<usize>,
    },
    Map(RefCell<Vec<(V, V)>>),
    Set(RefCell<Vec<V>>),
}
#[derive(Debug)]
struct Promise {
    state: RefCell<Option<Result<V, V>>>,
    listeners: RefCell<Vec<Waker>>,
}
type Job = usize;
struct TaskWake {
    queue: Arc<Mutex<VecDeque<Job>>>,
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
        Node::Map(values) if k == "size" => n(values.borrow().len() as f64),
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
        Node::Iterator { .. } | Node::Map(_) | Node::Set(_) => {
            panic!("internal native handle escaped")
        }
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
                assert_native(v, &e[1], &format!("{label}.{k}"))
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
        Node::Fn { code, .. } => assert_eq!(
            code,
            e["fn"].as_str().unwrap(),
            "{label}: native callback code"
        ),
        _ => panic!("{label}: unexpected returned native handle"),
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
    ctx: V,
    opts: V,
    primary: V,
    refs: RefCell<Vec<(V, String)>>,
    events: RefCell<Vec<Value>>,
    tasks: RefCell<Vec<Rc<Task>>>,
    queue: Arc<Mutex<VecDeque<Job>>>,
    index: Cell<usize>,
    closed: V,
    orders: V,
}
impl Host {
    fn new(input: &Value) -> Rc<Self> {
        let primary = Rc::new(Node::Error {
            name: "Error".encode_utf16().collect(),
            message: ["primary-".encode_utf16().collect::<Vec<_>>(), vec![0xd800]].concat(),
        });
        let opts = input.get("opts").map(from).unwrap_or_else(|| {
            obj(vec![
                ("symbol", t("BTC")),
                ("from", n(10.)),
                ("to", n(25.)),
                ("account", t("acct")),
                ("accountName", t("name")),
            ])
        });
        let owner = obj(vec![
            (
                "options",
                if input["inverse"] == true {
                    obj(vec![("enableInverseMarkets", b(true))])
                } else {
                    obj(vec![])
                },
            ),
            ("isPublicDataOnly", b(input["publicOnly"] == true)),
            ("DEFAULT_EXECUTIONS_LOOKBACK_MS", n(100.)),
            (
                "store",
                obj(vec![(
                    "marketsBySymbol",
                    obj(vec![
                        ("BTC", obj(vec![("quote", t("USDC"))])),
                        ("ETH", obj(vec![("quote", t(""))])),
                    ]),
                )]),
            ),
        ]);
        let log = Rc::new(Node::Fn {
            owner: owner.clone(),
            code: "log".into(),
            captures: vec![],
        });
        let xhr = obj(vec![]);
        let get = Rc::new(Node::Fn {
            owner: xhr.clone(),
            code: "get".into(),
            captures: vec![],
        });
        set_field(&xhr, "get", get.clone());
        let xhr2 = obj(vec![("get", get)]);
        set_field(&owner, "xhr", xhr.clone());
        set_field(&owner, "log", log.clone());
        let ctx = obj(vec![
            ("xhr", xhr.clone()),
            ("log", log.clone()),
            (
                "category",
                t(input["category"].as_str().unwrap_or("linear")),
            ),
            (
                "accountType",
                t(input["accountType"].as_str().unwrap_or("UNIFIED")),
            ),
        ]);
        let closed = input.get("closedRows").map(from).unwrap_or_else(|| {
            arr(vec![
                obj(vec![
                    ("orderId", t("O1")),
                    ("symbol", t("BTC")),
                    ("closedTime", n(21.5)),
                ]),
                obj(vec![
                    ("orderId", t("O2")),
                    ("symbol", t("ETH")),
                    ("closedTime", n(13.25)),
                ]),
            ])
        });
        let orders = input.get("orderRows").map(from).unwrap_or_else(|| {
            arr(vec![
                obj(vec![("id", t("O1")), ("positionSide", t("long"))]),
                obj(vec![("id", t("O2")), ("positionSide", t("short"))]),
            ])
        });
        let mut refs = vec![
            (primary.clone(), "primary".into()),
            (opts.clone(), "opts".into()),
            (closed.clone(), "closed".into()),
            (orders.clone(), "orders".into()),
            (xhr, "xhr".into()),
            (xhr2, "xhr2".into()),
            (log, "log".into()),
            (ctx.clone(), "ctx".into()),
        ];
        for (i, row) in items(&closed).into_iter().enumerate() {
            refs.push((row, format!("closed{i}")))
        }
        Rc::new_cyclic(|weak| Self {
            weak: weak.clone(),
            input: input.clone(),
            owner,
            ctx,
            opts,
            primary,
            refs: RefCell::new(refs),
            events: RefCell::new(vec![]),
            tasks: RefCell::new(vec![]),
            queue: Arc::new(Mutex::new(VecDeque::new())),
            index: Cell::new(0),
            closed,
            orders,
        })
    }
    fn h(&self) -> Rc<Self> {
        self.weak.upgrade().unwrap()
    }
    fn function(&self, owner: V, code: &str, captures: Vec<V>) -> V {
        Rc::new(Node::Fn {
            owner,
            code: code.into(),
            captures,
        })
    }
    fn observed(&self, v: &V) -> Value {
        if let Some((_, name)) = self.refs.borrow().iter().find(|(r, _)| Rc::ptr_eq(r, v)) {
            return json!({"ref":name});
        }
        match v.as_ref() {
            Node::Array(a) => {
                json!({"a":a.borrow().iter().map(|v|self.observed(v)).collect::<Vec<_>>()})
            }
            Node::Obj(o) => {
                json!({"o":o.borrow().iter().map(|(k,v)|json!([k.encode_utf16().collect::<Vec<_>>(),self.observed(v)])).collect::<Vec<_>>()})
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
                    for w in std::mem::take(&mut *p.listeners.borrow_mut()) {
                        w.wake()
                    }
                }
                Poll::Pending => *task.body.borrow_mut() = Some(body),
            }
        }
    }
    fn turn(&self) {
        let index = self
            .queue
            .lock()
            .unwrap()
            .pop_front()
            .expect("LEDGER pending task has no continuation");
        self.poll_task(index)
    }
    fn run(&self, value: V) -> Result<V, V> {
        if !matches!(value.as_ref(), Node::Promise(_)) {
            return Ok(value);
        }
        for _ in 0..1024 {
            if let Node::Promise(p) = value.as_ref() {
                if let Some(result) = p.state.borrow().clone() {
                    return result;
                }
            }
            self.turn()
        }
        panic!("LEDGER native settlement exceeded 1024 jobs")
    }
    fn drain(&self) {
        for _ in 0..1024 {
            if self.queue.lock().unwrap().is_empty() {
                assert!(
                    self.tasks
                        .borrow()
                        .iter()
                        .all(|t| t.body.borrow().is_none()),
                    "LEDGER unresolved owned task"
                );
                return;
            }
            self.turn()
        }
        panic!("LEDGER sibling drain exceeded 1024 jobs")
    }
    fn root(&self) -> Result<V, V> {
        let h = self.h();
        let o = self.owner.clone();
        let opts = self.opts.clone();
        let start = self.input.get("start").map(from).unwrap_or_else(|| n(0.));
        let end = self.input.get("end").map(from).unwrap_or_else(|| n(20.));
        let income = self.input.get("incomeType").map(from).unwrap_or_else(u);
        Ok(match self.input["op"].as_str().unwrap() {
            "raw" => api::fetch_closed_position_rows(h, o, opts),
            "positions" => api::fetch_historical_positions(h, o, opts),
            "closed" => api::fetch_closed_pnl(h, o, opts),
            "orders" => api::fetch_historical_orders(h, o, opts),
            "context" => api::income_contexts(self, &o)?,
            "wrapperIncome" => api::fetch_income_history(h, o, opts),
            "wrapperRange" => api::fetch_income_history_range(h, o, start, end, income),
            "income" => api::fetch_bybit_income_history(h, self.ctx.clone(), opts),
            "range" => {
                api::fetch_bybit_income_history_range(h, self.ctx.clone(), start, end, income)
            }
            _ => panic!("op"),
        })
    }
}
impl SnapshotHost for Host {
    type Value = V;
    type Error = V;
    fn get(&self, owner: &V, key: &str) -> Result<V, V> {
        if matches!(owner.as_ref(), Node::Null | Node::U) {
            return Err(self.type_error(owner, key));
        }
        if Rc::ptr_eq(owner, &self.opts) && self.input["observeOpts"] == true {
            self.rec(&format!("opts:{key}"), vec![])?;
        }
        if Rc::ptr_eq(owner, &self.ctx) && self.input["observeCtx"] == true {
            self.rec(&format!("ctx:{key}"), vec![])?;
        }
        if Rc::ptr_eq(owner, &self.owner) {
            match key {
                "xhr" => {
                    self.rec("xhrGet", vec![])?;
                }
                "log" => {
                    self.rec("logGet", vec![])?;
                    if self.input["swapXhrOnLog"] == true {
                        let next = self
                            .refs
                            .borrow()
                            .iter()
                            .find(|(_, name)| name == "xhr2")
                            .unwrap()
                            .0
                            .clone();
                        set_field(&self.owner, "xhr", next)
                    }
                }
                "incomeContexts" => return api::income_contexts(self, owner),
                _ => {}
            }
        }
        let value = field(owner, key);
        if !matches!(value.as_ref(), Node::U) {
            return Ok(value);
        }
        let method = Rc::ptr_eq(owner, &self.owner)
            && matches!(
                key,
                "getCategoryForSymbol"
                    | "getExecutionQueryWindows"
                    | "fetchClosedPositionRows"
                    | "fetchHistoricalOrders"
                    | "emitAndLogError"
                    | "inverseEnabled"
                    | "refuseOptionSymbol"
            )
            || matches!(owner.as_ref(), Node::Array(_)) && matches!(key, "push" | "sort" | "slice")
            || matches!(owner.as_ref(), Node::Map(_)) && matches!(key, "get" | "set")
            || matches!(owner.as_ref(), Node::Set(_)) && matches!(key, "has" | "add");
        Ok(if method {
            self.function(owner.clone(), key, vec![])
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
    fn apply(&self, f: V, args: Vec<V>) -> Result<V, V> {
        let Node::Fn {
            owner,
            code,
            captures,
        } = f.as_ref()
        else {
            panic!("native callable")
        };
        let first = args.first().cloned().unwrap_or_else(u);
        if code.starts_with("history_") {
            if matches!(code.as_str(), "history_quote" | "history_leg") {
                assert!(
                    Rc::ptr_eq(owner, &self.owner),
                    "LEDGER native lexical callback receiver"
                );
            }
            return api::dispatch(self, owner, code, captures, &args);
        }
        match code.as_str() {
            "fetchBybitIncomeHistory" => Ok(api::fetch_bybit_income_history(
                self.h(),
                first,
                args.get(1).cloned().unwrap_or_else(u),
            )),
            "fetchBybitIncomeHistoryRange" => Ok(api::fetch_bybit_income_history_range(
                self.h(),
                first,
                args[1].clone(),
                args[2].clone(),
                args[3].clone(),
            )),
            "refuseOptionSymbol" => api::refuse_option_symbol(self, &first, &args[1]),
            "inverseEnabled" => api::inverse_enabled(self, owner),
            "getCategoryForSymbol" => {
                self.rec("category", args)?;
                let mut value = "category.".encode_utf16().collect::<Vec<_>>();
                value.extend(text(&first));
                Ok(Rc::new(Node::Text(value)))
            }
            "getExecutionQueryWindows" => {
                self.rec("windows", args.clone())?;
                Ok(self.input.get("windows").map(from).unwrap_or_else(|| {
                    arr(vec![obj(vec![
                        ("startTime", args[0].clone()),
                        ("endTime", args[1].clone()),
                    ])])
                }))
            }
            "emitAndLogError" => {
                self.rec("emit", args)?;
                Ok(u())
            }
            "log" => {
                self.rec("log", args)?;
                Ok(u())
            }
            "get" if matches!(owner.as_ref(), Node::Map(_)) => {
                let Node::Map(map) = owner.as_ref() else {
                    unreachable!()
                };
                Ok(map
                    .borrow()
                    .iter()
                    .find(|(key, _)| equal(key, &first))
                    .map(|(_, value)| value.clone())
                    .unwrap_or_else(u))
            }
            "set" => {
                let Node::Map(map) = owner.as_ref() else {
                    panic!("map")
                };
                let mut map = map.borrow_mut();
                if let Some((_, v)) = map.iter_mut().find(|(k, _)| equal(k, &first)) {
                    *v = args[1].clone()
                } else {
                    map.push((first, args[1].clone()))
                }
                Ok(owner.clone())
            }
            "has" => {
                let Node::Set(set) = owner.as_ref() else {
                    panic!("set")
                };
                Ok(b(set.borrow().iter().any(|k| equal(k, &first))))
            }
            "add" => {
                let Node::Set(set) = owner.as_ref() else {
                    panic!("set")
                };
                if !set.borrow().iter().any(|k| equal(k, &first)) {
                    set.borrow_mut().push(first)
                }
                Ok(owner.clone())
            }
            "push" => {
                let Node::Array(array) = owner.as_ref() else {
                    panic!("array")
                };
                array.borrow_mut().extend(args);
                Ok(n(array.borrow().len() as f64))
            }
            "sort" => {
                let mut values = items(owner);
                for i in 1..values.len() {
                    let mut j = i;
                    while j > 0 {
                        let comparison = self.apply(
                            first.clone(),
                            vec![values[j - 1].clone(), values[j].clone()],
                        )?;
                        if jsnum(&comparison) <= 0. || jsnum(&comparison).is_nan() {
                            break;
                        }
                        values.swap(j - 1, j);
                        j -= 1
                    }
                }
                let Node::Array(array) = owner.as_ref() else {
                    panic!("array")
                };
                *array.borrow_mut() = values;
                Ok(owner.clone())
            }
            "slice" => {
                let values = items(owner);
                let end = args
                    .get(1)
                    .filter(|v| !matches!(v.as_ref(), Node::U))
                    .map(jsnum);
                let end = end
                    .map(|v| {
                        if v.is_nan() {
                            0
                        } else if v < 0. {
                            values.len().saturating_sub((-v) as usize)
                        } else {
                            (v as usize).min(values.len())
                        }
                    })
                    .unwrap_or(values.len());
                Ok(arr(values[..end].to_vec()))
            }
            "get" => {
                let i = self.index.get();
                self.index.set(i + 1);
                self.rec(&format!("get{i}"), vec![first, field(&args[1], "params")])?;
                let h = self.h();
                Ok(self.launch(Box::pin(async move{for _ in 0..h.input["delay"].as_u64().unwrap_or(1){pause().await}h.rec(&format!("resolved:get{i}"),vec![])?;if h.input["failAsync"]==format!("get{i}"){return Err(h.primary.clone())}
     let pages=h.input.get("pages").cloned().unwrap_or_else(||json!([{"retMsg":"OK","retCode":0,"result":{"list":[{"id":"A","symbol":"BTC","closedTime":21.5,"createdTime":17.25,"timestamp":13.125,"incomes":[5.5,-2.25]}],"nextPageCursor":""}}]));let pages=pages.as_array().unwrap();let data=from(&pages[i.min(pages.len()-1)]);let rows=field(&field(&data,"result"),"list");if matches!(rows.as_ref(),Node::Array(_)){for(j,row)in items(&rows).into_iter().enumerate(){h.refs.borrow_mut().push((row,format!("row{i}.{j}")))}}Ok(obj(vec![("data",data)]))})))
            }
            "fetchClosedPositionRows" | "fetchHistoricalOrders" => {
                let closed = code == "fetchClosedPositionRows";
                let name = if closed {
                    "closedDependency"
                } else {
                    "ordersDependency"
                };
                self.rec(name, args)?;
                let h = self.h();
                Ok(self.launch(Box::pin(async move {
                    pause().await;
                    h.rec(&format!("resolved:{name}"), vec![])?;
                    if h.input["failAsync"] == name {
                        return Err(h.primary.clone());
                    }
                    if !closed && h.input["swapMarkets"] == true {
                        set_field(
                            &field(&field(&field(&h.owner, "store"), "marketsBySymbol"), "BTC"),
                            "quote",
                            t("DAI"),
                        )
                    }
                    Ok(if closed {
                        h.closed.clone()
                    } else {
                        h.orders.clone()
                    })
                })))
            }
            "sleep" => {
                self.rec("sleep", args)?;
                let h = self.h();
                Ok(self.launch(Box::pin(async move {
                    pause().await;
                    h.rec("resolved:sleep", vec![])?;
                    if h.input["failAsync"] == "sleep" {
                        Err(h.primary.clone())
                    } else {
                        Ok(u())
                    }
                })))
            }
            "mapBybitClosedPnl" => {
                self.rec("mapClosed", args.clone())?;
                if truth(&field(&first, "skip")) {
                    return Ok(u());
                }
                Ok(obj(vec![
                    (
                        "closedTime",
                        null_default(&field(&first, "closedTime"), n(0.)),
                    ),
                    ("account", args[1].clone()),
                    ("info", first),
                ]))
            }
            "mapBybitHistoricalOrder" => {
                self.rec("mapOrder", args.clone())?;
                if truth(&field(&first, "skip")) {
                    return Ok(u());
                }
                Ok(obj(vec![
                    ("id", field(&first, "id")),
                    (
                        "createdTime",
                        null_default(&field(&first, "createdTime"), n(0.)),
                    ),
                    ("positionSide", field(&first, "positionSide")),
                    ("info", first),
                ]))
            }
            "mapBybitTransactionToIncomeRecords" => {
                self.rec("mapIncome", args)?;
                Ok(arr(items(&null_default(
                    &field(&first, "incomes"),
                    arr(vec![]),
                ))
                .into_iter()
                .enumerate()
                .map(|(i, income)| {
                    obj(vec![
                        (
                            "timestamp",
                            null_default(&field(&first, "timestamp"), n(0.)),
                        ),
                        ("income", income),
                        ("term", n(i as f64)),
                        ("info", first.clone()),
                    ])
                })
                .collect()))
            }
            "aggregateBybitClosedPnl" => {
                self.rec("aggregate", args.clone())?;
                let options = &args[1];
                let mut out = vec![];
                for row in items(&first) {
                    let quote = self.apply(
                        field(options, "resolveSettleCurrency"),
                        vec![field(&row, "symbol")],
                    )?;
                    let resolver = field(options, "resolveLeg");
                    let leg = if truth(&resolver) {
                        self.apply(resolver, vec![field(&row, "orderId")])?
                    } else {
                        u()
                    };
                    out.push(obj(vec![
                        ("account", field(options, "account")),
                        ("symbol", field(&row, "symbol")),
                        ("quote", quote),
                        ("leg", leg),
                        ("info", row),
                    ]))
                }
                Ok(arr(out))
            }
            "splitTimeRange" => {
                let from = &args[0];
                let to = &args[1];
                let span = jsnum(&args[2]);
                let mut out = vec![];
                if let (Node::Num(from), Node::Num(to)) = (from.as_ref(), to.as_ref()) {
                    if from.is_finite() && to.is_finite() && to >= from {
                        let mut start = *from;
                        for _ in 0..128 {
                            let end = (start + span).min(*to);
                            out.push(obj(vec![("startTime", n(start)), ("endTime", n(end))]));
                            if end >= *to {
                                break;
                            }
                            start = end + 1.
                        }
                    }
                }
                Ok(arr(out))
            }
            _ => panic!("unknown native call {code}"),
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
    fn object(&self, fields: Vec<(&'static str, V)>) -> V {
        obj(fields)
    }
    fn array(&self, values: Vec<V>) -> V {
        arr(values)
    }
    fn values(&self, array: &V) -> Result<Vec<V>, V> {
        Ok(items(array))
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v.as_ref(), Node::Array(_))
    }
    fn spread(&self, v: V, fields: Vec<(&'static str, V)>) -> Result<V, V> {
        let out = match v.as_ref() {
            Node::Obj(o) => Rc::new(Node::Obj(RefCell::new(o.borrow().clone()))),
            _ => obj(vec![]),
        };
        for (k, v) in fields {
            set_field(&out, k, v)
        }
        Ok(out)
    }
    fn intrinsic(&self, name: &str, args: Vec<V>) -> Result<V, V> {
        if let Some(code) = name.strip_prefix("select:") {
            return Ok(self.function(self.owner.clone(), code, vec![]));
        }
        match name {
            "null" => Ok(Rc::new(Node::Null)),
            "virtualClock.getCurrentTimeMs" => {
                self.rec("clock", args)?;
                Ok(n(self.input["time"].as_f64().unwrap_or(1000.)))
            }
            "subtract" => Ok(n(jsnum(&args[0]) - jsnum(&args[1]))),
            "add" => {
                if matches!(args[0].as_ref(), Node::Text(_))
                    || matches!(args[1].as_ref(), Node::Text(_))
                {
                    let mut value = text(&args[0]);
                    value.extend(text(&args[1]));
                    Ok(Rc::new(Node::Text(value)))
                } else {
                    Ok(n(jsnum(&args[0]) + jsnum(&args[1])))
                }
            }
            "less" => Ok(b(
                if let (Node::Text(a), Node::Text(b)) = (args[0].as_ref(), args[1].as_ref()) {
                    a < b
                } else {
                    jsnum(&args[0]) < jsnum(&args[1])
                },
            )),
            "Math.min" => {
                let a = jsnum(&args[0]);
                let b = jsnum(&args[1]);
                Ok(n(if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.min(b)
                }))
            }
            "template" => Ok(Rc::new(Node::Text(args.iter().flat_map(text).collect()))),
            "String" => Ok(Rc::new(Node::Text(text(&args[0])))),
            "get-key" => {
                let key = String::from_utf16_lossy(&text(&args[1]));
                let value = self.get(&args[0], &key)?;
                if matches!(value.as_ref(), Node::U) && matches!(args[0].as_ref(), Node::Obj(_)) {
                    if key == "__proto__" {
                        return Ok(obj(vec![]));
                    }
                    if matches!(
                        key.as_str(),
                        "constructor"
                            | "toString"
                            | "toLocaleString"
                            | "valueOf"
                            | "hasOwnProperty"
                            | "isPrototypeOf"
                            | "propertyIsEnumerable"
                            | "__defineGetter__"
                            | "__defineSetter__"
                            | "__lookupGetter__"
                            | "__lookupSetter__"
                    ) {
                        return Ok(self.function(args[0].clone(), "unknown", vec![]));
                    }
                }
                Ok(value)
            }
            "omitUndefined" => {
                let Node::Obj(fields) = args[0].as_ref() else {
                    panic!("omit object")
                };
                Ok(Rc::new(Node::Obj(RefCell::new(
                    fields
                        .borrow()
                        .iter()
                        .filter(|(_, v)| !matches!(v.as_ref(), Node::U))
                        .cloned()
                        .collect(),
                ))))
            }
            "Map" => Ok(Rc::new(Node::Map(RefCell::new(vec![])))),
            "Set" => Ok(Rc::new(Node::Set(RefCell::new(vec![])))),
            "iterator" => Ok(Rc::new(Node::Iterator {
                values: args[0].clone(),
                index: Cell::new(0),
            })),
            "iterator.next" => {
                let Node::Iterator { values, index } = args[0].as_ref() else {
                    panic!("iterator")
                };
                let array = items(values);
                let i = index.get();
                if i >= array.len() {
                    Ok(obj(vec![("done", b(true)), ("value", u())]))
                } else {
                    index.set(i + 1);
                    Ok(obj(vec![("done", b(false)), ("value", array[i].clone())]))
                }
            }
            "iterator.close" => Ok(u()),
            "spreadArgs" => Ok(arr(items(&args[0]))),
            "isBybitInverseSymbol" => Ok(b(
                safe_cex_venue_bybit::inverse_symbol::is_bybit_inverse_symbol(
                    &String::from_utf16_lossy(&text(&args[0])),
                ),
            )),
            "isBybitOptionSymbol" => Ok(b(safe_cex_venue_bybit::options::is_bybit_option_symbol(
                &String::from_utf16_lossy(&text(&args[0])),
            ))),
            _ => panic!("unknown native intrinsic {name}"),
        }
    }
    fn callback(&self, owner: V, code: &'static str, captures: Vec<V>) -> Result<V, V> {
        Ok(self.function(owner, code, captures))
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
            message: text(&message),
        }))
    }
    fn error_value(&self, e: V) -> V {
        e
    }
}
fn null_default(v: &V, default: V) -> V {
    if matches!(v.as_ref(), Node::U | Node::Null) {
        default
    } else {
        v.clone()
    }
}
#[test]
fn ledger_history_native_source_oracle() {
    use api::{BybitIncomeContext, IncomeAccountType, IncomeCategory};
    let marker = Rc::new(17u8);
    let context = BybitIncomeContext {
        xhr: marker.clone(),
        log: marker.clone(),
        category: IncomeCategory::Inverse,
        account_type: IncomeAccountType::Contract,
    };
    assert!(
        Rc::ptr_eq(&context.xhr, &marker) && Rc::ptr_eq(&context.log, &marker),
        "LEDGER type original handles"
    );
    assert_eq!(
        context.category.as_str(),
        "inverse",
        "LEDGER type category union"
    );
    assert_eq!(
        context.account_type.as_str(),
        "CONTRACT",
        "LEDGER type account union"
    );
    assert_eq!(
        IncomeCategory::Linear.as_str(),
        "linear",
        "LEDGER type linear union"
    );
    assert_eq!(
        IncomeAccountType::Unified.as_str(),
        "UNIFIED",
        "LEDGER type unified union"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ledger-history-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 95, "LEDGER independent case floor");
    let mut counts = BTreeMap::new();
    let mut supplemental = vec![];
    for (index, row) in rows.iter().enumerate() {
        let input = &row["input"];
        let op = input["op"].as_str().unwrap();
        *counts.entry(op).or_insert(0) += 1;
        let label = format!("LEDGER {op} iteration {index}");
        let h = Host::new(input);
        let started = h.root();
        let before = h.events.borrow().clone();
        let result = started.and_then(|v| h.run(v));
        h.drain();
        assert_eq!(
            result.is_err(),
            !row["error"].is_null(),
            "{label}: native rejection discriminant"
        );
        match result {
            Ok(value) => {
                assert_native(&value, &row["result"], &label);
                if op == "context" {
                    let contexts = items(&value);
                    assert_eq!(
                        contexts.len(),
                        if input["inverse"] == true { 2 } else { 1 },
                        "{label}: native one context per traded category"
                    );
                    for context in contexts {
                        let xhr = field(&context, "xhr");
                        let log = field(&context, "log");
                        let refs = h.refs.borrow();
                        assert!(
                            refs.iter()
                                .any(|(v, name)| name == "xhr" && Rc::ptr_eq(v, &xhr)),
                            "{label}: native captured original xhr"
                        );
                        assert!(
                            refs.iter()
                                .any(|(v, name)| name == "log" && Rc::ptr_eq(v, &log)),
                            "{label}: native original log handle"
                        );
                    }
                }
                if matches!(
                    op,
                    "raw"
                        | "closed"
                        | "orders"
                        | "positions"
                        | "income"
                        | "range"
                        | "wrapperIncome"
                        | "wrapperRange"
                ) {
                    for value in items(&value) {
                        let original = if op == "raw" {
                            value
                        } else {
                            field(&value, "info")
                        };
                        assert!(
                            h.refs
                                .borrow()
                                .iter()
                                .any(|(v, name)| (name.starts_with("row")
                                    || name.starts_with("closed"))
                                    && Rc::ptr_eq(v, &original)),
                            "{label}: native original raw/info row identity"
                        );
                    }
                }
                supplemental.push((
                    h.observed(&value),
                    row["resultRef"].clone(),
                    format!("{label}: supplemental allocation labels"),
                ));
            }
            Err(error) => {
                assert_native(&error, &row["error"], &label);
                assert_eq!(
                    Rc::ptr_eq(&error, &h.primary),
                    row["primary"].as_bool().unwrap(),
                    "{label}: native original rejection identity"
                );
            }
        }
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
            ("raw", 13),
            ("positions", 10),
            ("closed", 15),
            ("orders", 15),
            ("context", 5),
            ("wrapperIncome", 6),
            ("wrapperRange", 5),
            ("income", 16),
            ("range", 10)
        ]),
        "LEDGER independent family floors"
    );
    for (actual, expected, label) in supplemental {
        assert_eq!(actual, expected, "{label}")
    }
    eprintln!(
        "LEDGER floor95 stopped at iteration94 range COMMISSION; native settlement/drain bound1024 jobs; all native assertions precede supplemental comparisons; no wire/roundtrip assertion"
    );
}
