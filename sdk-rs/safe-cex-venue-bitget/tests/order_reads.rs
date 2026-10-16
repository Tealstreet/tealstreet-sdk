use safe_cex_venue_bitget::order_reads::{self as body, *};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    Hole,
    U,
    Null,
    Bool(bool),
    N(f64),
    S(Vec<u16>),
    A(Rc<RefCell<Vec<V>>>),
    O(Rc<RefCell<Vec<(Vec<u16>, V)>>>),
}
fn s(x: &str) -> V {
    V::S(x.encode_utf16().collect())
}
fn a(v: Vec<V>) -> V {
    V::A(Rc::new(RefCell::new(v)))
}
fn o(fields: Vec<(&str, V)>) -> V {
    V::O(Rc::new(RefCell::new(
        fields
            .into_iter()
            .map(|(k, v)| (k.encode_utf16().collect(), v))
            .collect(),
    )))
}
fn text(v: &V) -> String {
    match v {
        V::Hole | V::U => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::N(n) => n.to_string(),
        V::S(s) => String::from_utf16_lossy(s),
        V::A(_) => "".into(),
        V::O(_) => "[object Object]".into(),
    }
}
fn get(v: &V, k: &str) -> V {
    match v {
        V::O(v) => v
            .borrow()
            .iter()
            .find(|(key, _)| key == &k.encode_utf16().collect::<Vec<_>>())
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::Hole | V::U | V::Null => false,
        V::Bool(b) => *b,
        V::N(n) => *n != 0. && !n.is_nan(),
        V::S(t) => !t.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::U, V::U) | (V::Null, V::Null) => true,
        (V::S(a), V::S(b)) => a == b,
        (V::N(a), V::N(b)) => a == b,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::A(a), V::A(b)) => Rc::ptr_eq(a, b),
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn values(v: &V) -> Vec<V> {
    match v {
        V::A(v) => v.borrow().clone(),
        _ => panic!("READ native array required"),
    }
}
fn native(actual: &V, expected: &V, path: &str) {
    match (actual, expected) {
        (V::N(x), V::N(y)) => assert!(
            if y.is_nan() {
                x.is_nan()
            } else {
                x.to_bits() == y.to_bits()
            },
            "READ native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "READ native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "READ native independent array count {path}"
            );
            for (i, (x, y)) in x.iter().zip(y.iter()).enumerate() {
                native(x, y, &format!("{path}.{i}"))
            }
        }
        (V::O(x), V::O(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "READ native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "READ native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        _ => assert!(
            equal(actual, expected),
            "READ native scalar {path}: {actual:?} != {expected:?}"
        ),
    }
}
fn wire(v: &V) -> Json {
    match v {
        V::Hole => json!({"hole":true}),
        V::U => json!({"u":true}),
        V::Null => json!({"null":true}),
        V::Bool(b) => json!({"b":b}),
        V::N(n) => json!({"n":n.to_le_bytes()}),
        V::S(t) => json!({"s":t}),
        V::A(a) => json!({"a":a.borrow().iter().map(wire).collect::<Vec<_>>()}),
        V::O(o) => {
            json!({"o":o.borrow().iter().map(|(k,v)|json!([k,wire(v)])).collect::<Vec<_>>()})
        }
    }
}
#[derive(Clone, Debug)]
struct E {
    identity: Rc<()>,
    message: V,
}
#[derive(Clone, Debug)]
struct Event {
    name: String,
    args: Vec<V>,
}
struct Iter {
    array: V,
    index: usize,
}
struct H {
    op: &'static str,
    scene: &'static str,
    pages: Vec<V>,
    at: Cell<usize>,
    original: Rc<()>,
    events: RefCell<Vec<Event>>,
    running: Cell<bool>,
    post: Cell<bool>,
    loaded: V,
    normal: V,
    algo: V,
    trailing: V,
    uta: V,
    strategy: V,
    positions: V,
    balance: V,
    probe: &'static str,
    probe_owner: V,
    probe_field: &'static str,
    reads: Cell<usize>,
    alternate: V,
    secondary: Rc<()>,
    control: &'static str,
}
const OPS: [&str; 8] = [
    "orders",
    "normal",
    "uta-all",
    "uta-normal",
    "strategy",
    "algo",
    "trailing",
    "reconcile",
];
const SCENES: [&str; 10] = [
    "normal",
    "empty",
    "repeat",
    "missing",
    "cap",
    "reject",
    "api-error",
    "uta",
    "mixed-array",
    "stop",
];
fn raw(i: usize) -> V {
    o(vec![
        ("id", s(&format!("r{i}"))),
        ("kind", s(if i == 0 { "attached" } else { "plain" })),
    ])
}
fn page(op: &str, rows: V, cursor: V) -> V {
    let inner = if op == "uta-normal" {
        o(vec![("list", rows), ("cursor", cursor)])
    } else {
        o(vec![("entrustedList", rows), ("endId", cursor)])
    };
    o(vec![("code", s("00000")), ("data", inner)])
}
fn make_pages(op: &str, sc: &str) -> Vec<V> {
    if sc == "api-error" {
        return vec![o(vec![
            ("code", s("35")),
            ("msg", s("api")),
            ("data", V::Null),
        ])];
    }
    if op == "strategy" {
        return vec![o(vec![
            ("code", s("00000")),
            (
                "data",
                if sc == "empty" {
                    V::Null
                } else if sc == "mixed-array" {
                    a(vec![raw(0)])
                } else {
                    o(vec![("list", a(vec![raw(0), raw(1)]))])
                },
            ),
        ])];
    }
    if sc == "empty" {
        return vec![page(op, a(vec![]), s("c0"))];
    }
    if sc == "cap" {
        return (0..21)
            .map(|i| page(op, a(vec![raw(i)]), s(&format!("c{i}"))))
            .collect();
    }
    if sc == "missing" {
        return vec![page(op, a(vec![raw(0)]), V::U)];
    }
    if sc == "mixed-array" && op == "trailing" {
        return vec![
            page(op, a(vec![raw(0)]), s("c0")),
            o(vec![("data", a(vec![raw(2)]))]),
        ];
    }
    let mut out = vec![
        page(op, a(vec![raw(0)]), s("c0")),
        page(
            op,
            a(vec![raw(1)]),
            if sc == "repeat" { s("c0") } else { s("") },
        ),
    ];
    if sc == "mixed-array" && op == "uta-normal" {
        out[1] = o(vec![
            ("code", s("35")),
            ("msg", s("partial")),
            ("data", V::Null),
        ])
    }
    out
}
impl H {
    fn new(op: &'static str, scene: &'static str) -> Self {
        Self {
            op,
            scene,
            pages: make_pages(op, scene),
            at: Cell::new(0),
            original: Rc::new(()),
            events: RefCell::new(vec![]),
            running: Cell::new(scene == "cap" && op == "reconcile"),
            post: Cell::new(false),
            loaded: o(vec![
                ("orders", V::Bool(false)),
                ("positions", V::Bool(false)),
                ("balance", V::Bool(false)),
                ("other", s("keep")),
            ]),
            normal: a(vec![o(vec![("id", s("n"))])]),
            algo: a(vec![o(vec![("id", s("a"))])]),
            trailing: a(vec![o(vec![("id", s("t"))])]),
            uta: a(vec![o(vec![("id", s("u"))])]),
            strategy: a(vec![o(vec![("id", s("s"))])]),
            positions: a(vec![o(vec![("id", s("p"))])]),
            balance: o(vec![("total", V::N(99.))]),
            probe: "",
            probe_owner: V::U,
            probe_field: "",
            reads: Cell::new(0),
            alternate: V::U,
            secondary: Rc::new(()),
            control: "",
        }
    }
    fn err(&self) -> E {
        E {
            identity: self.original.clone(),
            message: s("original"),
        }
    }
    fn record(&self, name: &str, args: Vec<V>) {
        self.events.borrow_mut().push(Event {
            name: name.into(),
            args,
        })
    }
    fn rejects(&self, name: &str) -> bool {
        if self.op == "reconcile" {
            match self.scene {
                "empty" => true,
                "repeat" => name == "fetchOrders",
                "missing" => name != "fetchOrders",
                _ => false,
            }
        } else {
            self.scene == "reject" && matches!(name, "fetchNormalOrders" | "fetchUtaNormalOrders")
        }
    }
}
impl OrderReadsHost for H {
    type Value = V;
    type Error = E;
    type Iterator = Iter;
    fn receiver(&self, n: &str) -> Result<V, E> {
        self.record(&format!("get:{n}"), vec![]);
        Ok(match n {
            "apiProductType" => s("product"),
            "apiUtaCategory" => s("category"),
            "xhr" => s("client"),
            "usesUtaRouting" => V::Bool(self.scene == "uta"),
            "mapOrder" => s("mapper"),
            "isDisposed" => V::Bool(
                self.control == "initial-disposed" || self.scene == "stop" && self.post.get(),
            ),
            "isPrivateStreamDegraded" => V::Bool(
                !(self.control == "initial-healthy"
                    || self.control == "post-healthy" && self.post.get()),
            ),
            "privateStreamDegradedReconcileRunning" => V::Bool(self.running.get()),
            "store.loaded" => self.loaded.clone(),
            _ => panic!("receiver {n}"),
        })
    }
    fn set_receiver(&self, n: &str, v: V) -> Result<(), E> {
        self.record(&format!("set:{n}"), vec![v.clone()]);
        if self.control == "entry-throw" && truth(&v) || self.control == "clear-throw" && !truth(&v)
        {
            return Err(E {
                identity: self.secondary.clone(),
                message: s("final"),
            });
        }
        self.running.set(truth(&v));
        Ok(())
    }
    fn property(&self, v: &V, n: &str) -> Result<V, E> {
        if equal(v, &self.probe_owner) && n == self.probe_field && !self.probe.is_empty() {
            let i = self.reads.get();
            self.reads.set(i + 1);
            self.record("probe-get", vec![]);
            if i == 1 {
                if self.probe == "throw" {
                    return Err(self.err());
                }
                return Ok(self.alternate.clone());
            }
        }
        if matches!(v, V::Null | V::U) {
            return Err(E {
                identity: Rc::new(()),
                message: s(&format!(
                    "Cannot read properties of {} (reading '{n}')",
                    text(v)
                )),
            });
        }
        if n == "length" {
            if let V::A(v) = v {
                return Ok(V::N(v.borrow().len() as f64));
            }
        }
        Ok(get(v, n))
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::Null | V::U)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        equal(a, b)
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v, V::A(_))
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn string(&self, v: &[u16]) -> V {
        V::S(v.to_vec())
    }
    fn number(&self, n: f64) -> V {
        V::N(n)
    }
    fn boolean(&self, b: bool) -> V {
        V::Bool(b)
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn object(&self, v: Vec<(&str, V)>) -> V {
        o(v)
    }
    fn spread(&self, v: &V) -> Result<V, E> {
        match v {
            V::O(v) => Ok(V::O(Rc::new(RefCell::new(v.borrow().clone())))),
            V::U | V::Null => Ok(o(vec![])),
            _ => Err(self.err()),
        }
    }
    fn assign(&self, v: &V, n: &str, value: V) -> Result<(), E> {
        self.define(v, n, value)
    }
    fn define(&self, v: &V, n: &str, value: V) -> Result<(), E> {
        let V::O(v) = v else { panic!() };
        let mut v = v.borrow_mut();
        let key: Vec<u16> = n.encode_utf16().collect();
        if let Some((_, old)) = v.iter_mut().find(|(k, _)| *k == key) {
            *old = value
        } else {
            v.push((key, value))
        }
        Ok(())
    }
    fn iterator(&self, v: &V) -> Result<Iter, E> {
        if !matches!(v, V::A(_)) {
            return Err(self.err());
        }
        Ok(Iter {
            array: v.clone(),
            index: 0,
        })
    }
    fn next(&self, i: &mut Iter) -> Result<Option<V>, E> {
        let V::A(v) = &i.array else { panic!() };
        let len = v.borrow().len();
        if i.index >= len {
            return Ok(None);
        }
        let v = self.index(&i.array, i.index)?;
        i.index += 1;
        Ok(Some(v))
    }
    fn index(&self, v: &V, i: usize) -> Result<V, E> {
        let V::A(v) = v else { return Err(self.err()) };
        Ok(v.borrow()
            .get(i)
            .filter(|v| !matches!(v, V::Hole))
            .cloned()
            .unwrap_or(V::U))
    }
    fn flat_map(&self, v: V, mapper: V) -> Result<V, E> {
        native(&mapper, &s("mapper"), "native mapper handle");
        self.record("flatMap", vec![v.clone()]);
        let V::A(rows) = &v else {
            return Err(self.err());
        };
        let len = rows.borrow().len();
        let mut out = vec![];
        for i in 0..len {
            let present = rows.borrow().get(i).is_some_and(|v| !matches!(v, V::Hole));
            if !present {
                continue;
            }
            let raw = self.index(&v, i)?;
            self.record("mapRow", vec![raw.clone(), V::N(i as f64)]);
            let id = get(&raw, "id");
            out.push(o(vec![("id", id.clone()), ("info", raw.clone())]));
            if text(&get(&raw, "kind")) == "attached" {
                out.push(o(vec![
                    ("id", s(&format!("{}:leg", text(&id)))),
                    ("info", raw),
                ]));
            }
        }
        Ok(a(out))
    }
    fn call(&self, n: &str, args: Vec<V>) -> Result<OrderReadFuture<'_, V, E>, E> {
        self.record(&format!("call:{n}"), args);
        if self.op == "reconcile" && self.scene == "reject" && n == "fetchPositions" {
            return Err(self.err());
        }
        let v = match n {
            "ensureAccountTypeDetected" => V::U,
            "fetchNormalOrders" => self.normal.clone(),
            "fetchAlgoOrders" => self.algo.clone(),
            "fetchTrailingStopOrders" => self.trailing.clone(),
            "fetchUtaOrders" | "fetchUtaNormalOrders" => self.uta.clone(),
            "fetchUtaStrategyOrders" => self.strategy.clone(),
            "fetchPositions" => self.positions.clone(),
            "fetchOrders" => self.normal.clone(),
            "fetchBalance" => self.balance.clone(),
            _ => panic!("call {n}"),
        };
        let reject = self.rejects(n);
        Ok(Box::pin(async move {
            if reject { Err(self.err()) } else { Ok(v) }
        }))
    }
    fn call_sync(&self, n: &str, args: Vec<V>) -> Result<(), E> {
        self.record(&format!("sync:{n}"), args);
        if self.control == "schedule-throw" && n == "schedulePrivateStreamDegradedReconcile" {
            return Err(E {
                identity: self.secondary.clone(),
                message: s("final"),
            });
        }
        Ok(())
    }
    fn read(
        &self,
        client: V,
        endpoint: &'static str,
        params: V,
    ) -> Result<OrderReadFuture<'_, V, E>, E> {
        native(&client, &s("client"), "native HTTP client");
        self.record(&format!("read:{endpoint}"), vec![params]);
        let i = self.at.get();
        self.at.set(i + 1);
        Ok(Box::pin(async move {
            if self.scene == "reject" {
                Err(self.err())
            } else {
                Ok(self
                    .pages
                    .get(i)
                    .cloned()
                    .unwrap_or_else(|| page(self.op, a(vec![]), V::U)))
            }
        }))
    }
    fn promise_function(&self, settled: bool) -> Result<V, E> {
        self.record(
            if settled {
                "capture:settled"
            } else {
                "capture:all"
            },
            vec![],
        );
        if self.scene == "api-error" {
            return Err(self.err());
        }
        Ok(s(if settled { "settled" } else { "all" }))
    }
    fn promise<'a>(
        &'a self,
        function: V,
        settled: bool,
        inputs: Vec<OrderReadFuture<'a, V, E>>,
    ) -> Result<OrderReadFuture<'a, V, E>, E> {
        native(
            &function,
            &s(if settled { "settled" } else { "all" }),
            "native captured Promise method",
        );
        self.record(
            if settled {
                "promise:settled"
            } else {
                "promise:all"
            },
            vec![],
        );
        Ok(Box::pin(async move {
            let mut out = vec![];
            for future in inputs {
                let r = future.await;
                if settled {
                    out.push(match r {
                        Ok(v) => o(vec![("status", s("fulfilled")), ("value", v)]),
                        Err(_) => o(vec![("status", s("rejected")), ("reason", s("original"))]),
                    })
                } else {
                    out.push(r?)
                }
            }
            self.post.set(true);
            Ok(a(out))
        }))
    }
    fn success(&self, v: &V) -> Result<bool, E> {
        Ok(matches!(text(v).as_str(), "0" | "00000"))
    }
    fn error_message(&self, v: &V, f: &str) -> Result<V, E> {
        let m = get(v, "msg");
        Ok(if truth(&m) { m } else { s(f) })
    }
    fn emit(&self, v: V) -> Result<(), E> {
        self.record("emit", vec![v]);
        Ok(())
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(r) => r,
        Poll::Pending => panic!("settled fixture futures"),
    }
}
fn execute(h: &H) -> Result<V, E> {
    run(async {
        match h.op {
            "orders" => body::fetch_orders(h).await,
            "normal" => body::fetch_normal_orders(h).await,
            "uta-all" => body::fetch_uta_orders(h).await,
            "uta-normal" => body::fetch_uta_normal_orders(h).await,
            "strategy" => body::fetch_uta_strategy_orders(h).await,
            "algo" => body::fetch_algo_orders(h).await,
            "trailing" => body::fetch_trailing_stop_orders(h).await,
            "reconcile" => body::reconcile_private_state_from_rest(h)
                .await
                .map(|()| V::U),
            _ => panic!(),
        }
    })
}
fn ids(h: &H) -> Vec<String> {
    let (op, sc) = (h.op, h.scene);
    match op {
        "orders" => {
            if sc == "uta" {
                vec!["u".into()]
            } else {
                vec!["n".into(), "a".into(), "t".into()]
            }
        }
        "uta-all" => vec!["u".into(), "s".into()],
        "normal" if sc == "uta" => vec!["u".into()],
        _ => {
            if sc == "empty" || sc == "api-error" {
                return vec![];
            }
            if op == "trailing" && sc == "mixed-array" {
                return vec!["r2".into()];
            }
            let count = if sc == "cap" && op != "strategy" {
                20
            } else if sc == "missing" && op != "strategy"
                || sc == "mixed-array" && matches!(op, "strategy" | "uta-normal")
            {
                1
            } else {
                2
            };
            let mut ids = vec!["r0".into(), "r0:leg".into()];
            for i in 1..count {
                ids.push(format!("r{i}"));
            }
            ids
        }
    }
}
fn is_error(h: &H) -> bool {
    h.scene == "reject" || h.scene == "api-error" && matches!(h.op, "uta-all" | "reconcile")
}
fn request_count(h: &H) -> usize {
    if h.op == "normal" && h.scene == "uta" || matches!(h.op, "orders" | "uta-all" | "reconcile") {
        0
    } else if h.op == "strategy" || matches!(h.scene, "empty" | "missing" | "reject" | "api-error")
    {
        1
    } else if h.scene == "cap" {
        20
    } else {
        2
    }
}
fn expected_params(h: &H, i: usize) -> V {
    if h.op == "strategy" {
        return o(vec![("category", s("category")), ("type", s("tpsl"))]);
    }
    let params = if h.op == "uta-normal" {
        o(vec![("category", s("category")), ("limit", s("100"))])
    } else {
        let mut fields = vec![("productType", s("product"))];
        if h.op == "algo" {
            fields.push(("planType", s("profit_loss")))
        } else if h.op == "trailing" {
            fields.push(("planType", s("track_plan")))
        }
        fields.push(("limit", s("200")));
        o(fields)
    };
    if i > 0 {
        h.define(
            &params,
            if h.op == "uta-normal" {
                "cursor"
            } else {
                "idLessThan"
            },
            s(&format!("c{}", i - 1)),
        )
        .unwrap();
    }
    params
}
fn expected_names(h: &H) -> Vec<String> {
    let mut n = vec![];
    let mut push = |v: &str| n.push(v.into());
    match h.op {
        "orders" => {
            push("call:ensureAccountTypeDetected");
            push("get:usesUtaRouting");
            if h.scene == "uta" {
                push("call:fetchUtaOrders")
            } else {
                push("call:fetchNormalOrders");
                if h.scene != "reject" {
                    push("call:fetchAlgoOrders");
                    push("call:fetchTrailingStopOrders")
                }
            }
        }
        "uta-all" => {
            push("capture:all");
            if h.scene != "api-error" {
                push("call:fetchUtaNormalOrders");
                push("call:fetchUtaStrategyOrders");
                push("promise:all")
            }
        }
        "reconcile" => {
            push("get:isDisposed");
            push("get:isPrivateStreamDegraded");
            push("get:privateStreamDegradedReconcileRunning");
            if h.scene == "cap" {
                return n;
            }
            push("set:privateStreamDegradedReconcileRunning");
            push("capture:settled");
            if h.scene != "api-error" {
                push("call:fetchPositions");
                if h.scene != "reject" {
                    push("call:fetchOrders");
                    push("call:fetchBalance");
                    push("promise:settled");
                    push("get:isDisposed");
                    if h.scene != "stop" {
                        push("get:isPrivateStreamDegraded");
                        push("get:store.loaded");
                        if !matches!(h.scene, "empty" | "missing") {
                            push("sync:store.addOrUpdatePositionsFull")
                        }
                        push("sync:store.update")
                    }
                }
            }
            push("set:privateStreamDegradedReconcileRunning");
            push("sync:schedulePrivateStreamDegradedReconcile");
        }
        _ => {
            if h.op == "normal" {
                push("call:ensureAccountTypeDetected");
                push("get:usesUtaRouting");
                if h.scene == "uta" {
                    push("call:fetchUtaNormalOrders");
                    return n;
                }
            }
            for i in 0..request_count(h) {
                if h.op == "strategy" {
                    push("get:xhr");
                    push("get:apiUtaCategory")
                } else {
                    push(if h.op == "uta-normal" {
                        "get:apiUtaCategory"
                    } else {
                        "get:apiProductType"
                    });
                    push("get:xhr")
                }
                push(match h.op {
                    "normal" => "read:ORDERS",
                    "uta-normal" => "read:UTA_ORDERS",
                    "strategy" => "read:UTA_STRATEGY_ORDERS",
                    _ => "read:ALGO_ORDERS",
                });
                if h.op == "uta-normal" && h.scene == "mixed-array" && i == 1 {
                    push("emit")
                }
            }
            if h.scene == "reject" {
                return n;
            }
            if h.scene == "api-error" && matches!(h.op, "uta-normal" | "strategy") {
                push("emit");
                if h.op == "strategy" {
                    return n;
                }
            }
            push("get:mapOrder");
            push("flatMap");
            let raw_count = if ids(h).is_empty() {
                0
            } else {
                ids(h).len() - usize::from(!(h.op == "trailing" && h.scene == "mixed-array"))
            };
            for _ in 0..raw_count {
                push("mapRow")
            }
        }
    }
    n
}
fn output(h: &H, result: &Result<V, E>) -> Json {
    json!({"result":match result{Ok(v)=>json!({"ok":wire(v)}),Err(e)=>json!({"original":Rc::ptr_eq(&e.identity,&h.original),"message":wire(&e.message)})},"events":h.events.borrow().iter().map(|e|json!({"name":e.name,"args":e.args.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>(),"running":h.running.get()})
}
fn input(h: &H) -> Json {
    json!({"op":h.op,"scene":h.scene,"pages":h.pages.iter().map(wire).collect::<Vec<_>>(),"normal":wire(&h.normal),"algo":wire(&h.algo),"trailing":wire(&h.trailing),"uta":wire(&h.uta),"strategy":wire(&h.strategy),"positions":wire(&h.positions),"balance":wire(&h.balance),"loaded":wire(&h.loaded),"probe":h.probe,"field":h.probe_field,"alternate":wire(&h.alternate),"control":h.control})
}
fn source(cases: &[Json], mode: &str) -> Vec<Json> {
    let mut c = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/order-reads-capture.mjs"),
        )
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let w = c
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes());
    let out = c.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "READ source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    w.unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}
fn native_case(h: &H, result: &Result<V, E>) {
    assert_eq!(
        result.is_err(),
        is_error(h),
        "READ native error disposition {}/{}",
        h.op,
        h.scene
    );
    if let Err(e) = result {
        assert!(
            Rc::ptr_eq(&e.identity, &h.original),
            "READ native original error identity"
        );
        native(&e.message, &s("original"), "original error payload");
    } else if h.op == "reconcile" {
        native(result.as_ref().unwrap(), &V::U, "reconcile return");
    } else {
        let actual = values(result.as_ref().unwrap());
        let ids = ids(h);
        assert_eq!(
            actual.len(),
            ids.len(),
            "READ native independent mapped count {}/{}",
            h.op,
            h.scene
        );
        for (v, id) in actual.iter().zip(ids.iter()) {
            native(
                &get(v, "id"),
                &s(id),
                &format!("returned ID {}/{}", h.op, h.scene),
            );
        }
    }
    let names: Vec<_> = h.events.borrow().iter().map(|e| e.name.clone()).collect();
    assert_eq!(
        names,
        expected_names(h),
        "READ native effect order {}/{}",
        h.op,
        h.scene
    );
    let events = h.events.borrow();
    let reads: Vec<_> = events
        .iter()
        .filter(|e| e.name.starts_with("read:"))
        .collect();
    assert_eq!(
        reads.len(),
        request_count(h),
        "READ native request count {}/{}",
        h.op,
        h.scene
    );
    for (i, e) in reads.iter().enumerate() {
        native(
            &e.args[0],
            &expected_params(h, i),
            &format!("request params {}/{}/{}", h.op, h.scene, i),
        );
    }
    if h.op == "reconcile" {
        assert_eq!(
            h.running.get(),
            h.scene == "cap",
            "READ native finally running flag"
        );
        let updates: Vec<_> = events
            .iter()
            .filter(|e| e.name == "sync:store.update")
            .collect();
        let applied = !matches!(h.scene, "cap" | "reject" | "api-error" | "stop");
        assert_eq!(
            updates.len(),
            usize::from(applied),
            "READ native recovery update count"
        );
        if applied {
            let loaded = o(vec![
                ("orders", V::Bool(!matches!(h.scene, "empty" | "repeat"))),
                (
                    "positions",
                    V::Bool(!matches!(h.scene, "empty" | "missing")),
                ),
                ("balance", V::Bool(!matches!(h.scene, "empty" | "missing"))),
                ("other", s("keep")),
            ]);
            let mut fields = vec![];
            if !matches!(h.scene, "empty" | "repeat") {
                fields.push(("orders", h.normal.clone()))
            }
            if !matches!(h.scene, "empty" | "missing") {
                fields.push(("balance", h.balance.clone()))
            }
            fields.push(("loaded", loaded));
            native(&updates[0].args[0], &o(fields), "recovery patch");
            if !matches!(h.scene, "empty" | "repeat") {
                assert!(
                    equal(&get(&updates[0].args[0], "orders"), &h.normal),
                    "READ native recovery orders alias"
                );
            }
            if !matches!(h.scene, "empty" | "missing") {
                assert!(
                    equal(&get(&updates[0].args[0], "balance"), &h.balance),
                    "READ native recovery balance alias"
                );
            }
            assert!(
                !equal(&get(&updates[0].args[0], "loaded"), &h.loaded),
                "READ native loaded copy"
            );
        }
        if applied && !matches!(h.scene, "empty" | "missing") {
            let position_event = events
                .iter()
                .find(|e| e.name == "sync:store.addOrUpdatePositionsFull")
                .unwrap();
            native(
                &position_event.args[0],
                &a(vec![o(vec![("id", s("p"))])]),
                "recovery native positions",
            );
            assert!(
                equal(&position_event.args[0], &h.positions),
                "READ native recovery positions alias"
            );
        }
        native(
            &h.loaded,
            &o(vec![
                ("orders", V::Bool(false)),
                ("positions", V::Bool(false)),
                ("balance", V::Bool(false)),
                ("other", s("keep")),
            ]),
            "original loaded unchanged",
        );
        if !matches!(h.scene, "cap" | "reject" | "api-error") {
            native(
                &events
                    .iter()
                    .find(|e| e.name == "call:fetchBalance")
                    .unwrap()
                    .args[0],
                &o(vec![("preferFast", V::Bool(true))]),
                "recovery fast balance opts",
            );
        }
    } else if !is_error(h) {
        let mapped_rows: Vec<_> = events.iter().filter(|e| e.name == "mapRow").collect();
        for (i, e) in mapped_rows.iter().enumerate() {
            native(&e.args[1], &V::N(i as f64), "native flatMap callback index");
        }
        if !mapped_rows.is_empty() {
            for out in values(result.as_ref().unwrap()) {
                assert!(
                    mapped_rows
                        .iter()
                        .any(|e| equal(&get(&out, "info"), &e.args[0])),
                    "READ native mapped info alias"
                );
            }
        }
    }
}
#[test]
fn native_order_reads_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for op in OPS {
        for scene in SCENES {
            let h = H::new(op, scene);
            let case = input(&h);
            let result = execute(&h);
            native_case(&h, &result);
            cases.push(case);
            outputs.push(output(&h, &result));
        }
    }
    assert_eq!(cases.len(), 80, "READ independent primary floor");
    let src = source(&cases, "primary");
    assert_eq!(src.len(), 80, "READ independent source floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        assert_eq!(a, b, "READ source primary case {i}");
    }
}
#[test]
fn native_pagination_getters_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for (op, field, probe) in [
        ("normal", "entrustedList", "change"),
        ("algo", "endId", "change"),
        ("uta-normal", "data", "change"),
        ("strategy", "data", "throw"),
    ] {
        let mut h = H::new(op, "normal");
        h.probe = probe;
        h.probe_field = field;
        h.probe_owner = if field == "data" {
            h.pages[0].clone()
        } else {
            get(&h.pages[0], "data")
        };
        h.alternate = match op {
            "normal" => a(vec![]),
            "algo" => s("other"),
            "uta-normal" => o(vec![("list", a(vec![raw(9)])), ("cursor", s("unused"))]),
            _ => V::U,
        };
        let case = input(&h);
        let result = execute(&h);
        if probe == "throw" {
            let e = result.as_ref().unwrap_err();
            assert!(
                Rc::ptr_eq(&e.identity, &h.original),
                "READ native guarded getter original error"
            );
            assert_eq!(h.reads.get(), 2, "READ native throwing data getter count");
            assert!(
                !h.events.borrow().iter().any(|e| e.name == "flatMap"),
                "READ native getter error before mapping"
            );
        } else {
            let values = values(result.as_ref().unwrap());
            let ids: Vec<_> = values.iter().map(|v| get(v, "id")).collect();
            native(
                &a(ids),
                &a(match op {
                    "normal" => vec![],
                    "algo" => vec![s("r0"), s("r0:leg"), s("r1")],
                    _ => vec![s("r9"), s("r1")],
                }),
                "getter returned IDs",
            );
            assert_eq!(
                h.reads.get(),
                if op == "normal" { 2 } else { 3 },
                "READ native repeated pagination getter count"
            );
            if op != "normal" {
                let events = h.events.borrow();
                let read = events
                    .iter()
                    .filter(|e| e.name.starts_with("read:"))
                    .nth(1)
                    .unwrap();
                native(
                    &read.args[0],
                    &expected_params(&h, 1),
                    "getter pagination cursor params",
                );
            }
        }
        cases.push(case);
        outputs.push(output(&h, &result));
    }
    let src = source(&cases, "probes");
    assert_eq!(src.len(), 4, "READ independent getter floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        assert_eq!(a, b, "READ source getter case {i}");
    }
}
#[test]
fn native_sparse_strategy_then_source() {
    let mut h = H::new("strategy", "mixed-array");
    let raw = raw(1);
    h.pages = vec![o(vec![
        ("code", s("00000")),
        ("data", a(vec![V::Hole, raw.clone()])),
    ])];
    let case = input(&h);
    let result = execute(&h);
    let v = result.as_ref().unwrap();
    native(
        v,
        &a(vec![o(vec![("id", s("r1")), ("info", raw.clone())])]),
        "sparse strategy decoded result",
    );
    let events = h.events.borrow();
    let callbacks: Vec<_> = events.iter().filter(|e| e.name == "mapRow").collect();
    assert_eq!(
        callbacks.len(),
        1,
        "READ native sparse strategy callback count"
    );
    native(
        &callbacks[0].args[1],
        &V::N(1.),
        "sparse strategy callback index",
    );
    assert!(
        equal(&get(&values(v)[0], "info"), &raw),
        "READ native sparse strategy info alias"
    );
    let output = output(&h, &result);
    let src = source(&[case], "holes");
    assert_eq!(src.len(), 1, "READ independent sparse floor");
    assert_eq!(output, src[0], "READ source sparse strategy");
}
#[test]
fn native_recovery_guards_and_finally_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for control in [
        "initial-disposed",
        "initial-healthy",
        "post-healthy",
        "entry-throw",
        "schedule-throw",
        "clear-throw",
    ] {
        let mut h = H::new(
            "reconcile",
            if matches!(control, "schedule-throw" | "clear-throw") {
                "reject"
            } else {
                "normal"
            },
        );
        h.control = control;
        let case = input(&h);
        let result = execute(&h);
        let throws = control.ends_with("throw");
        assert_eq!(
            result.is_err(),
            throws,
            "READ native recovery controlled disposition"
        );
        if throws {
            let e = result.as_ref().unwrap_err();
            assert!(
                Rc::ptr_eq(&e.identity, &h.secondary),
                "READ native finally override error identity"
            );
            native(&e.message, &s("final"), "finally override payload");
        } else {
            native(result.as_ref().unwrap(), &V::U, "recovery guard result");
        }
        let events = h.events.borrow();
        let names: Vec<_> = events.iter().map(|e| e.name.as_str()).collect();
        let expected = match control {
            "initial-disposed" => vec!["get:isDisposed"],
            "initial-healthy" => vec!["get:isDisposed", "get:isPrivateStreamDegraded"],
            "entry-throw" => vec![
                "get:isDisposed",
                "get:isPrivateStreamDegraded",
                "get:privateStreamDegradedReconcileRunning",
                "set:privateStreamDegradedReconcileRunning",
            ],
            "schedule-throw" => vec![
                "get:isDisposed",
                "get:isPrivateStreamDegraded",
                "get:privateStreamDegradedReconcileRunning",
                "set:privateStreamDegradedReconcileRunning",
                "capture:settled",
                "call:fetchPositions",
                "set:privateStreamDegradedReconcileRunning",
                "sync:schedulePrivateStreamDegradedReconcile",
            ],
            "clear-throw" => vec![
                "get:isDisposed",
                "get:isPrivateStreamDegraded",
                "get:privateStreamDegradedReconcileRunning",
                "set:privateStreamDegradedReconcileRunning",
                "capture:settled",
                "call:fetchPositions",
                "set:privateStreamDegradedReconcileRunning",
            ],
            _ => vec![
                "get:isDisposed",
                "get:isPrivateStreamDegraded",
                "get:privateStreamDegradedReconcileRunning",
                "set:privateStreamDegradedReconcileRunning",
                "capture:settled",
                "call:fetchPositions",
                "call:fetchOrders",
                "call:fetchBalance",
                "promise:settled",
                "get:isDisposed",
                "get:isPrivateStreamDegraded",
                "set:privateStreamDegradedReconcileRunning",
                "sync:schedulePrivateStreamDegradedReconcile",
            ],
        };
        assert_eq!(
            names, expected,
            "READ native recovery guard/finally operation order"
        );
        assert_eq!(
            h.running.get(),
            control == "clear-throw",
            "READ native recovery clear behavior"
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e.name == "sync:store.update")
                .count(),
            0,
            "READ native guard/finally no store update"
        );
        cases.push(case);
        outputs.push(output(&h, &result));
    }
    let src = source(&cases, "recovery");
    assert_eq!(src.len(), 6, "READ independent recovery floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        assert_eq!(a, b, "READ source recovery case {i}");
    }
}
