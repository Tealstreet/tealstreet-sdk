use safe_cex_venue_bybit::{
    hyro_private_session::{SessionFuture, SessionHost},
    spot_order_writes::{self as api, SpotOrderHost},
};
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
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    A(Rc<RefCell<Vec<V>>>),
    O(Rc<RefCell<Vec<(String, V)>>>),
    F(Box<V>, String),
    P(Box<Result<V, E>>),
}
#[derive(Clone, Debug)]
struct E(V);
fn s(v: &str) -> V {
    V::S(v.encode_utf16().collect())
}
fn a(v: Vec<V>) -> V {
    V::A(Rc::new(RefCell::new(v)))
}
fn o(v: Vec<(&str, V)>) -> V {
    V::O(Rc::new(RefCell::new(
        v.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    )))
}
fn text(v: &V) -> String {
    match v {
        V::S(v) => String::from_utf16_lossy(v),
        V::N(n) => safe_cex_num::js_number_string(*n),
        V::U => "undefined".into(),
        V::Null => "null".into(),
        V::B(b) => b.to_string(),
        _ => "[object Object]".into(),
    }
}
fn num(v: &V) -> f64 {
    match v {
        V::N(n) => *n,
        V::Null => 0.,
        V::B(b) => {
            if *b {
                1.
            } else {
                0.
            }
        }
        _ => text(v).parse().unwrap_or(f64::NAN),
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::U | V::Null => false,
        V::B(v) => *v,
        V::N(v) => *v != 0. && !v.is_nan(),
        V::S(v) => !v.is_empty(),
        _ => true,
    }
}
fn eq(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::U, V::U) | (V::Null, V::Null) => true,
        (V::S(a), V::S(b)) => a == b,
        (V::N(a), V::N(b)) => a == b,
        (V::B(a), V::B(b)) => a == b,
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        (V::A(a), V::A(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn get(v: &V, k: &str) -> V {
    match v {
        V::O(v) => v
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        V::A(v) if k == "length" => V::N(v.borrow().len() as f64),
        V::A(v) => k
            .parse::<usize>()
            .ok()
            .and_then(|i| v.borrow().get(i).cloned())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
fn put(v: &V, k: &str, f: V) {
    let V::O(v) = v else {
        panic!("SPOT native object")
    };
    let mut v = v.borrow_mut();
    if let Some((_, old)) = v.iter_mut().find(|(key, _)| key == k) {
        *old = f
    } else {
        v.push((k.into(), f))
    }
}
fn vals(v: &V) -> Vec<V> {
    let V::A(v) = v else {
        panic!("SPOT native array")
    };
    v.borrow().clone()
}
fn native(v: &V, expected: &V, path: &str) {
    match (v, expected) {
        (V::N(a), V::N(b)) => assert_eq!(a.to_bits(), b.to_bits(), "SPOT native numeric {path}"),
        (V::S(a), V::S(b)) => assert_eq!(a, b, "SPOT native UTF16 {path}"),
        (V::A(a), V::A(b)) => {
            let a = a.borrow();
            let b = b.borrow();
            assert_eq!(
                a.len(),
                b.len(),
                "SPOT native independent array count {path}"
            );
            for (i, (a, b)) in a.iter().zip(b.iter()).enumerate() {
                native(a, b, &format!("{path}.{i}"))
            }
        }
        (V::O(a), V::O(b)) => {
            let a = a.borrow();
            let b = b.borrow();
            assert_eq!(
                a.len(),
                b.len(),
                "SPOT native independent field count {path}"
            );
            for ((ka, a), (kb, b)) in a.iter().zip(b.iter()) {
                assert_eq!(ka, kb, "SPOT native field order {path}");
                native(a, b, &format!("{path}.{ka}"))
            }
        }
        _ => assert!(
            eq(v, expected),
            "SPOT native scalar {path}: {v:?} != {expected:?}"
        ),
    }
}
fn wire(v: &V) -> Json {
    match v {
        V::U => json!({"u":true}),
        V::Null => json!({"null":true}),
        V::B(b) => json!({"b":b}),
        V::N(n) => json!({"n":n.to_le_bytes()}),
        V::S(s) => json!({"s":s}),
        V::A(a) => json!({"a":a.borrow().iter().map(wire).collect::<Vec<_>>()}),
        V::O(o) => {
            json!({"o":o.borrow().iter().map(|(k,v)|json!([k.encode_utf16().collect::<Vec<_>>(),wire(v)])).collect::<Vec<_>>()})
        }
        _ => panic!("wire only decoded values"),
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(f);
    for _ in 0..100 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("SPOT fixture did not settle")
}
struct H {
    original_error: V,
    retmsg_reads: Cell<usize>,
    scene: &'static str,
    global: V,
    owner: V,
    events: RefCell<Vec<(String, Vec<V>)>>,
    requests: RefCell<Vec<(String, V)>>,
    recorded: RefCell<Vec<V>>,
    updated: RefCell<Vec<V>>,
    retired: RefCell<Vec<V>>,
    placed: RefCell<Vec<V>>,
}
impl H {
    fn err(&self) -> E {
        E(self.original_error.clone())
    }
    fn event(&self, k: &str, args: Vec<V>) {
        self.events.borrow_mut().push((k.into(), args));
    }
    fn new(scene: &'static str) -> Self {
        let globals = o(vec![
            ("parseFloat", V::F(Box::new(V::U), "parseFloat".into())),
            ("OrderStatus", o(vec![("Open", s("open"))])),
            (
                "OrderTimeInForce",
                o(vec![("GoodTillCancel", s("GoodTillCancel"))]),
            ),
            (
                "OrderType",
                o(vec![
                    ("Conditional", s("conditional")),
                    ("Limit", s("limit")),
                    ("StopLoss", s("stop_market")),
                    ("TakeProfit", s("take_profit_market")),
                ]),
            ),
            ("OrderSide", o(vec![("Buy", s("buy")), ("Sell", s("sell"))])),
            (
                "PositionSide",
                o(vec![("Long", s("long")), ("Short", s("short"))]),
            ),
            (
                "OrderStopTriggerType",
                o(vec![
                    ("LastPrice", s("last_price")),
                    ("MarkPrice", s("mark_price")),
                ]),
            ),
            (
                "ENDPOINTS",
                o(vec![
                    ("SET_TRADING_STOP", s("SET_TRADING_STOP")),
                    ("CANCEL_ORDERS", s("CANCEL_ORDERS")),
                    ("CANCEL_SYMBOL_ORDERS", s("CANCEL_SYMBOL_ORDERS")),
                ]),
            ),
            ("Math", o(vec![])),
            ("Object", o(vec![])),
            ("virtualClock", o(vec![])),
        ]);
        let market = o(vec![(
            "precision",
            o(vec![("amount", V::N(0.1)), ("price", V::N(0.5))]),
        )]);
        let ticker = o(vec![("last", V::N(100.))]);
        let owner = o(vec![
            ("xhr", s("regular")),
            ("unlimitedXHR", s("fast")),
            (
                "store",
                o(vec![
                    (
                        "marketsBySymbol",
                        o(if scene == "no-market" {
                            vec![]
                        } else {
                            vec![("BTC", market)]
                        }),
                    ),
                    (
                        "tickersBySymbol",
                        o(if scene == "no-ticker" {
                            vec![]
                        } else {
                            vec![("BTC", ticker)]
                        }),
                    ),
                    (
                        "orders",
                        a(vec![
                            order("normal", "limit", "buy"),
                            order("protect", "stop_loss", "buy"),
                            order("else", "limit", "sell"),
                        ]),
                    ),
                    (
                        "positions",
                        a(vec![o(vec![
                            ("symbol", s("BTC")),
                            ("side", s("short")),
                            ("contracts", V::N(if scene == "zero" { 0. } else { 3. })),
                        ])]),
                    ),
                ]),
            ),
        ]);
        Self {
            retmsg_reads: Cell::new(0),
            original_error: o(vec![("message", s("original"))]),
            scene,
            global: globals,
            owner,
            events: RefCell::default(),
            requests: RefCell::default(),
            recorded: RefCell::default(),
            updated: RefCell::default(),
            retired: RefCell::default(),
            placed: RefCell::default(),
        }
    }
}
fn order(id: &str, kind: &str, side: &str) -> V {
    o(vec![
        ("id", s(id)),
        ("symbol", s(if id == "else" { "ETH" } else { "BTC" })),
        ("type", s(kind)),
        ("side", s(side)),
        ("filled", V::N(5.)),
        ("amount", V::N(7.)),
        ("price", V::N(80.)),
        ("remaining", V::N(2.)),
    ])
}
fn opts(scene: &str) -> V {
    o(vec![
        ("symbol", s("BTC")),
        (
            "type",
            s(if scene == "tp" {
                "take_profit_market"
            } else if scene == "conditional" {
                "conditional"
            } else {
                "stop_market"
            }),
        ),
        ("side", s("buy")),
        ("amount", V::N(if scene == "zero" { 0. } else { 1.27 })),
        ("price", V::N(90.)),
        ("triggerPrice", V::N(70.)),
        ("reduceOnly", V::B(true)),
        ("slTriggerType", s("last_price")),
        ("tpTriggerType", s("mark_price")),
    ])
}
impl SessionHost for H {
    type Value = V;
    type Error = E;
    fn get(&self, v: &V, k: &str) -> Result<V, E> {
        if matches!(v, V::U | V::Null) {
            return Err(self.err());
        }
        if k == "retMsg" {
            let reads = self.retmsg_reads.get();
            self.retmsg_reads.set(reads + 1);
            if self.scene == "emit-capture" && reads > 0 {
                return Err(E(o(vec![("message", s("getter failure"))])));
            }
        }
        Ok(get(v, k))
    }
    fn optional(&self, v: &V, k: &str) -> Result<V, E> {
        if matches!(v, V::U | V::Null) {
            Ok(V::U)
        } else {
            self.get(v, k)
        }
    }
    fn set(&self, v: &V, k: &str, f: V) -> Result<(), E> {
        put(v, k, f);
        Ok(())
    }
    fn define(&self, v: &V, k: &str, f: V) -> Result<(), E> {
        self.set(v, k, f)
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        eq(a, b)
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn literal(&self, v: &str) -> V {
        s(v)
    }
    fn boolean(&self, v: bool) -> V {
        V::B(v)
    }
    fn number(&self, v: f64) -> V {
        V::N(v)
    }
    fn numeric(&self, v: &V) -> Result<f64, E> {
        Ok(num(v))
    }
    fn units(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(if let V::S(v) = v {
            v.clone()
        } else {
            text(v).encode_utf16().collect()
        })
    }
    fn text(&self, v: Vec<u16>) -> V {
        V::S(v)
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> V {
        o(v)
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn global(&self) -> V {
        self.global.clone()
    }
    fn select(&self, owner: V, key: &str) -> Result<V, E> {
        if self.scene == "emit-capture" && key == "emitAndLogError" {
            return Err(self.err());
        }
        if matches!(owner, V::U | V::Null) {
            return Err(self.err());
        }
        Ok(V::F(Box::new(owner), key.into()))
    }
    fn receiver_call(&self, owner: V, callable: V, args: Vec<V>) -> Result<V, E> {
        let V::F(_, k) = callable else { panic!() };
        self.apply(V::F(Box::new(owner), k), args)
    }
    fn apply(&self, selected: V, args: Vec<V>) -> Result<V, E> {
        let V::F(owner, key) = selected else {
            panic!("SPOT native selected callable")
        };
        match key.as_str() {
            "parseFloat" => {
                assert!(
                    matches!(owner.as_ref(), V::U),
                    "SPOT native bare parse receiver"
                );
                Ok(V::N(safe_cex_num::parse_float(&text(&args[0]))))
            }
            "getCurrentTimeMs" => Ok(V::N(1234.)),
            "max" => Ok(V::N(num(&args[0]).max(num(&args[1])))),
            "min" => Ok(V::N(num(&args[0]).min(num(&args[1])))),
            "adjust" => Ok(V::N(safe_cex_num::adjust(num(&args[0]), num(&args[1])))),
            "endsWith" => {
                let V::S(v) = owner.as_ref() else {
                    return Err(self.err());
                };
                let V::S(suffix) = &args[0] else { panic!() };
                Ok(V::B(v.ends_with(suffix)))
            }
            "flat" => {
                let mut out = vec![];
                for v in vals(&owner) {
                    if matches!(v, V::A(_)) {
                        out.extend(vals(&v))
                    } else {
                        out.push(v)
                    }
                }
                Ok(a(out))
            }
            "push" => {
                let V::A(v) = owner.as_ref() else { panic!() };
                v.borrow_mut().extend(args);
                Ok(V::N(v.borrow().len() as f64))
            }
            "entries" => {
                let V::O(v) = &args[0] else { panic!() };
                Ok(a(v
                    .borrow()
                    .iter()
                    .map(|(k, v)| a(vec![s(k), v.clone()]))
                    .collect()))
            }
            "chunk" => Ok(a(vals(&args[0])
                .chunks(num(&args[1]) as usize)
                .map(|lot| a(lot.to_vec()))
                .collect())),
            "v" => Ok(get(&args[0], &text(&args[1]))),
            "isBybitOrderAlreadyGoneCancelMessage" => Ok(V::B({
                let v = if truth(&args[0]) {
                    text(&args[0]).to_lowercase()
                } else {
                    String::new()
                };
                v.contains("order not exists") || v.contains("too late to cancel")
            })),
            "shouldUseLocalRestOrderLifecycleFallback" => Ok(V::B(self.scene != "nonlocal")),
            "getStopOrderPositionIdx" => {
                self.event(&key, vec![]);
                Ok(V::N(0.))
            }
            "recordRecentlyPlacedOrder" => {
                self.recorded.borrow_mut().push(args[0].clone());
                Ok(V::U)
            }
            "addOrUpdateOrders" => {
                self.updated.borrow_mut().push(args[0].clone());
                Ok(V::U)
            }
            "retireCancelledOrders" => {
                self.event(&key, vec![]);
                self.retired.borrow_mut().push(args[0].clone());
                Ok(V::U)
            }
            "emitAndLogError" => {
                self.event(&key, args);
                Ok(V::U)
            }
            "markOrderSeen" => {
                self.event(&key, args);
                Ok(V::U)
            }
            "assertCallerClientOrderIds" => {
                self.event(&key, vec![]);
                Ok(V::U)
            }
            "getOrderExecutionParams" => {
                self.event(&key, vec![get(&args[0], "id")]);
                if self.scene == "preflight-reject" {
                    Err(self.err())
                } else {
                    Ok(V::U)
                }
            }
            "placeOrder" => {
                self.event(&key, vec![get(&args[0], "id")]);
                if self.scene == "place-reject" {
                    Ok(V::P(Box::new(Err(self.err()))))
                } else {
                    Ok(a(vec![get(&args[0], "id")]))
                }
            }
            "placeOrders" => {
                self.event(&key, vec![]);
                self.placed.borrow_mut().push(args[0].clone());
                Ok(a(vec![]))
            }
            "cancelOrdersHelper" => {
                self.event(&key, vec![]);
                native(&args[1], &s("spot"), "delegate category");
                self.retired.borrow_mut().push(args[0].clone());
                Ok(V::U)
            }
            "sleep" => {
                self.event(&key, args);
                Ok(V::U)
            }
            "post" => {
                let endpoint = text(&args[0]);
                let client = text(&owner);
                assert_eq!(
                    client,
                    if endpoint == "SET_TRADING_STOP" {
                        "regular"
                    } else {
                        "fast"
                    },
                    "SPOT native HTTP client"
                );
                self.requests.borrow_mut().push((endpoint, args[1].clone()));
                if self.scene == "reject" {
                    return Ok(V::P(Box::new(Err(self.err()))));
                }
                let msg = if self.scene.starts_with("api-error") || self.scene == "emit-capture" {
                    "bad"
                } else if self.scene == "gone" {
                    "ORDER NOT EXISTS"
                } else {
                    "OK"
                };
                let errors = if self.scene == "partial-error" {
                    vec![o(vec![("code", V::N(9.)), ("msg", s("partial bad"))])]
                } else if self.scene == "partial-gone" {
                    vec![o(vec![
                        ("code", V::N(9.)),
                        ("msg", s("too late to cancel")),
                    ])]
                } else {
                    vec![]
                };
                Ok(o(vec![(
                    "data",
                    o(vec![
                        ("retMsg", s(msg)),
                        ("result", o(vec![("orderId", s("native-id"))])),
                        ("retExtInfo", o(vec![("list", a(errors))])),
                    ]),
                )]))
            }
            _ => panic!("SPOT native call {key}"),
        }
    }
    fn wait(&self, v: V) -> SessionFuture<'_, V, E> {
        Box::pin(async move { if let V::P(result) = v { *result } else { Ok(v) } })
    }
    fn intrinsic(&self, name: &str, args: Vec<V>) -> Result<V, E> {
        Ok(match name {
            "null" => V::Null,
            "keyGet" => get(&args[0], &text(&args[1])),
            "template" => s(&text(&args[0])),
            "subtract" => V::N(num(&args[0]) - num(&args[1])),
            "greaterEqual" => V::B(num(&args[0]) >= num(&args[1])),
            "greater" => V::B(num(&args[0]) > num(&args[1])),
            "typeof" => s(if matches!(args[0], V::N(_)) {
                "number"
            } else {
                "string"
            }),
            "missingMarket" => s(&format!("Market {} not found", text(&args[0]))),
            "missingTicker" => s(&format!("Ticker {} not found", text(&args[0]))),
            "destructure2" => args[0].clone(),
            "copyOwnDataProperties" => {
                let V::O(v) = &args[1] else { panic!() };
                for (k, v) in v.borrow().iter() {
                    put(&args[0], k, v.clone())
                }
                V::U
            }
            _ => panic!("SPOT native intrinsic {name}"),
        })
    }
    fn allocate(&self, _: V) -> Result<V, E> {
        panic!("unused")
    }
    fn callback(&self, _: V, _: &'static str, _: Vec<V>) -> Result<V, E> {
        panic!("unused")
    }
    fn promise(&self) -> Result<(V, V, V), E> {
        panic!("unused")
    }
    fn error(&self, msg: V) -> Result<E, E> {
        let value = if matches!(msg, V::U) {
            String::new()
        } else {
            text(&msg)
        };
        Ok(E(o(vec![("message", s(&value))])))
    }
    fn error_value(&self, e: E) -> V {
        e.0
    }
    fn normalize_error(&self, e: E) -> Result<V, E> {
        Ok(e.0)
    }
}
fn array_method(method: V, expected: &str) -> Vec<V> {
    let V::F(owner, key) = method else { panic!() };
    assert_eq!(key, expected, "SPOT native selected array method");
    vals(&owner)
}
impl SpotOrderHost for H {
    fn spread(&self, v: V) -> Result<V, E> {
        if let V::O(v) = v {
            Ok(V::O(Rc::new(RefCell::new(v.borrow().clone()))))
        } else {
            Ok(o(vec![]))
        }
    }
    fn map<F>(&self, m: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<V, E>,
    {
        Ok(a(array_method(m, "map")
            .into_iter()
            .enumerate()
            .map(|(i, v)| f(v, i))
            .collect::<Result<Vec<_>, _>>()?))
    }
    fn filter<F>(&self, m: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        let mut out = vec![];
        for (i, v) in array_method(m, "filter").into_iter().enumerate() {
            if f(v.clone(), i)? {
                out.push(v)
            }
        }
        Ok(a(out))
    }
    fn some<F>(&self, m: V, mut f: F) -> Result<bool, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        for (i, v) in array_method(m, "some").into_iter().enumerate() {
            if f(v, i)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn find<F>(&self, m: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, usize) -> Result<bool, E>,
    {
        for (i, v) in array_method(m, "find").into_iter().enumerate() {
            if f(v.clone(), i)? {
                return Ok(v);
            }
        }
        Ok(V::U)
    }
    fn reduce<F>(&self, m: V, mut acc: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, V) -> Result<V, E>,
    {
        for v in array_method(m, "reduce") {
            acc = f(acc, v)?
        }
        Ok(acc)
    }
    fn for_of<F>(&self, v: V, mut f: F) -> Result<(), E>
    where
        F: FnMut(V) -> Result<(), E>,
    {
        for v in vals(&v) {
            f(v)?
        }
        Ok(())
    }
    fn series<'a, F>(&'a self, callee: V, v: V, mut f: F) -> SessionFuture<'a, V, E>
    where
        F: FnMut(V, usize) -> SessionFuture<'a, V, E> + 'a,
    {
        let V::F(owner, key) = callee else {
            panic!("SPOT native series callee")
        };
        assert!(eq(&owner, &self.global), "SPOT native series owner");
        assert!(
            matches!(key.as_str(), "mapSeries" | "forEachSeries"),
            "SPOT native selected series method"
        );
        Box::pin(async move {
            let mut out = vec![];
            let mut i = 0;
            while i < vals(&v).len() {
                let entry = self.wait(vals(&v)[i].clone()).await?;
                out.push(f(entry, i).await?);
                i += 1;
            }
            Ok(if key == "mapSeries" { a(out) } else { V::U })
        })
    }
}
fn source(cases: Json) -> Json {
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/spot-order-writes-oracle.mjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(cases.to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "SPOT source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
const GROUPS: [(&str, &[&str]); 9] = [
    ("record", &["normal", "conditional", "zero", "payload"]),
    ("local", &["normal", "nonlocal", "string", "negative"]),
    ("guard", &["normal", "sell", "zero", "too-small"]),
    ("batch", &["normal", "preflight-reject", "place-reject"]),
    (
        "stop",
        &["normal", "tp", "zero", "api-error", "no-market", "reject"],
    ),
    (
        "trailing",
        &["normal", "api-error", "no-market", "no-ticker", "reject"],
    ),
    ("cancel", &["normal", "virtual", "empty"]),
    (
        "helper",
        &[
            "normal",
            "gone",
            "partial-gone",
            "partial-error",
            "api-error",
            "reject",
        ],
    ),
    (
        "symbol",
        &["normal", "keep", "api-error", "api-error-keep", "reject"],
    ),
];
#[test]
fn spot_order_writes_native_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for (op, scenes) in GROUPS {
        for &scene in scenes {
            let h = H::new(scene);
            let r = &h.owner;
            let opts = opts(scene);
            let original = order("normal", "limit", "buy");
            let update = o(vec![
                (
                    "amount",
                    if scene == "string" {
                        s("2")
                    } else {
                        V::N(if scene == "negative" { 8. } else { 2. })
                    },
                ),
                ("price", V::N(90.)),
            ]);
            let input_orders = a(match (op, scene) {
                ("helper", _) => (0..11)
                    .map(|i| order(&format!("o{i}"), "limit", "buy"))
                    .collect(),
                ("cancel", "empty") => vec![],
                ("cancel", "virtual") => vec![
                    order("id__stop_loss", "stop_market", "sell"),
                    order("id__take_profit", "take_profit_market", "sell"),
                ],
                _ => vec![
                    original.clone(),
                    order("id__stop_loss", "stop_market", "sell"),
                ],
            });
            let payload = o(vec![
                ("qty", s(if scene == "payload" { "3.5" } else { "0" })),
                ("price", V::U),
                ("triggerPrice", s("0")),
            ]);
            let result = match op {
                "record" => api::record_recently_placed_order_from_opts(
                    &h,
                    r,
                    s("record-id"),
                    opts.clone(),
                    payload.clone(),
                )
                .map(|_| V::U),
                "local" => api::apply_local_order_update_after_rest_write(
                    &h,
                    r,
                    original.clone(),
                    update.clone(),
                )
                .map(|_| V::U),
                "guard" => api::has_full_sized_position_opposite_side(
                    &h,
                    r,
                    s("BTC"),
                    s(if scene == "sell" { "sell" } else { "buy" }),
                    V::N(if scene == "too-small" { 4. } else { 2. }),
                )
                .map(V::B),
                "batch" => run(api::place_orders(&h, r, input_orders.clone())),
                "stop" => run(api::place_stop_loss_or_take_profit(&h, r, opts.clone())),
                "trailing" => run(api::place_trailing_stop_loss(&h, r, opts.clone())),
                "cancel" => run(api::cancel_orders(&h, r, input_orders.clone())),
                "helper" => run(api::cancel_orders_helper(
                    &h,
                    r,
                    input_orders.clone(),
                    s("spot"),
                )),
                "symbol" => run(api::cancel_symbol_orders(
                    &h,
                    r,
                    s("BTC"),
                    if scene.contains("keep") {
                        V::B(false)
                    } else {
                        V::U
                    },
                )),
                _ => unreachable!(),
            };
            let failed = matches!(
                scene,
                "no-market" | "no-ticker" | "reject" | "preflight-reject" | "place-reject"
            ) || (op == "helper" && matches!(scene, "api-error" | "partial-error"));
            if failed {
                let Err(e) = &result else {
                    panic!("SPOT native expected failure {op}/{scene}")
                };
                assert_eq!(
                    eq(&e.0, &h.original_error),
                    matches!(scene, "reject" | "preflight-reject" | "place-reject"),
                    "SPOT native propagated error identity"
                );
                let message = match scene {
                    "no-market" if op == "stop" => "Market BTC not found",
                    "no-market" | "no-ticker" => "Ticker BTC not found",
                    "api-error" => "bad",
                    "partial-error" => "partial bad",
                    _ => "original",
                };
                native(
                    &get(&e.0, "message"),
                    &s(message),
                    &format!("error {op}/{scene}"),
                );
            } else {
                assert!(result.is_ok(), "SPOT native expected success {op}/{scene}");
                let value = result.as_ref().unwrap();
                let expected = match op {
                    "guard" => V::B(!matches!(scene, "sell" | "zero" | "too-small")),
                    "batch" => a(vec![s("normal"), s("id__stop_loss")]),
                    "stop" | "trailing" => a(vec![s("native-id")]),
                    _ => V::U,
                };
                native(value, &expected, &format!("result {op}/{scene}"));
            }
            if op == "record" {
                assert_eq!(h.recorded.borrow().len(), 1, "SPOT native recorded count");
                let record = &h.recorded.borrow()[0];
                let qty = if scene == "payload" {
                    3.5
                } else if scene == "zero" {
                    0.
                } else {
                    1.27
                };
                native(
                    record,
                    &o(vec![
                        ("id", s("record-id")),
                        ("status", s("open")),
                        ("symbol", s("BTC")),
                        ("type", get(&opts, "type")),
                        ("side", s("buy")),
                        ("price", V::N(90.)),
                        ("amount", V::N(qty)),
                        ("filled", V::N(0.)),
                        ("remaining", V::N(qty)),
                        ("reduceOnly", V::B(scene != "conditional")),
                        ("timeInForce", s("GoodTillCancel")),
                        ("lastUpdate", V::N(1234.)),
                    ]),
                    "recorded order",
                );
            }
            if op == "local" {
                assert_eq!(
                    h.updated.borrow().len(),
                    usize::from(scene != "nonlocal"),
                    "SPOT native local publish count"
                );
                native(
                    &get(&original, "remaining"),
                    &V::N(2.),
                    "original remaining untouched",
                );
                if scene != "nonlocal" {
                    let row = vals(&h.updated.borrow()[0])[0].clone();
                    assert!(!eq(&row, &original), "SPOT native new order identity");
                    native(
                        &get(&row, "remaining"),
                        &V::N(if scene == "string" {
                            2.
                        } else if scene == "negative" {
                            3.
                        } else {
                            0.
                        }),
                        "local remaining",
                    );
                    native(&get(&row, "lastUpdate"), &V::N(1234.), "local timestamp");
                    native(
                        &get(&row, "amount"),
                        &get(&update, "amount"),
                        "local amount",
                    );
                }
            }
            if op == "batch" {
                let events = h.events.borrow();
                let names = events.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>();
                let expected = match scene {
                    "preflight-reject" => {
                        vec!["assertCallerClientOrderIds", "getOrderExecutionParams"]
                    }
                    "place-reject" => vec![
                        "assertCallerClientOrderIds",
                        "getOrderExecutionParams",
                        "getOrderExecutionParams",
                        "placeOrder",
                    ],
                    _ => vec![
                        "assertCallerClientOrderIds",
                        "getOrderExecutionParams",
                        "getOrderExecutionParams",
                        "placeOrder",
                        "placeOrder",
                    ],
                };
                assert_eq!(names, expected, "SPOT native batch preflight before writes");
            }
            if op == "stop" || op == "trailing" {
                let requests = h.requests.borrow();
                let count = usize::from(!matches!(scene, "no-market" | "no-ticker"));
                assert_eq!(requests.len(), count, "SPOT native protective HTTP count");
                if count == 1 {
                    assert_eq!(requests[0].0, "SET_TRADING_STOP");
                    let expected = if op == "trailing" {
                        o(vec![
                            ("category", s("spot")),
                            ("symbol", s("BTC")),
                            ("positionIdx", V::N(0.)),
                            ("trailingStop", s("10")),
                        ])
                    } else {
                        let tp = scene == "tp";
                        let mut fields = vec![
                            ("category", s("spot")),
                            ("symbol", s("BTC")),
                            ("positionIdx", V::N(0.)),
                            (
                                "tpslMode",
                                s(if scene == "zero" { "Full" } else { "Partial" }),
                            ),
                            (if tp { "takeProfit" } else { "stopLoss" }, s("90")),
                            (
                                if tp { "tpTriggerBy" } else { "slTriggerBy" },
                                s(if tp { "MarkPrice" } else { "LastPrice" }),
                            ),
                        ];
                        if scene != "zero" {
                            fields.push((if tp { "tpSize" } else { "slSize" }, s("1.3")))
                        }
                        o(fields)
                    };
                    native(&requests[0].1, &expected, "protective payload");
                }
            }
            if op == "cancel" {
                let expected = if scene == "normal" { 1 } else { 0 };
                assert_eq!(
                    h.retired.borrow().len(),
                    expected,
                    "SPOT native filtered delegation count"
                );
                assert_eq!(
                    h.events
                        .borrow()
                        .iter()
                        .filter(|(k, _)| k == "markOrderSeen")
                        .count(),
                    if scene == "empty" { 0 } else { 2 },
                    "SPOT native mark virtual IDs too"
                );
                if scene == "normal" {
                    let rows = vals(&h.retired.borrow()[0]);
                    assert_eq!(rows.len(), 1, "SPOT native filtered row count");
                    assert!(
                        eq(&rows[0], &original),
                        "SPOT native cancellation row alias"
                    );
                }
            }
            if op == "helper" {
                assert_eq!(
                    h.requests.borrow().len(),
                    if failed { 1 } else { 2 },
                    "SPOT native chunk HTTP count"
                );
                assert_eq!(
                    h.retired.borrow().len(),
                    if failed { 0 } else { 2 },
                    "SPOT native retired lots count"
                );
                let first = get(&h.requests.borrow()[0].1, "request");
                assert_eq!(vals(&first).len(), 10, "SPOT native first lot count");
                native(
                    &vals(&first)[0],
                    &o(vec![("symbol", s("BTC")), ("orderId", s("o0"))]),
                    "cancel request row",
                );
                if !failed {
                    let events = h.events.borrow();
                    let sleep = events.iter().find(|(k, _)| k == "sleep").unwrap();
                    native(&sleep.1[0], &V::N(1000.), "batch sleep");
                    assert_eq!(
                        vals(&get(&h.requests.borrow()[1].1, "request")).len(),
                        1,
                        "SPOT native final lot count"
                    );
                }
            }
            if op == "symbol" {
                assert_eq!(
                    h.requests.borrow().len(),
                    1,
                    "SPOT native cancel-symbol HTTP count"
                );
                native(
                    &h.requests.borrow()[0].1,
                    &o(vec![("category", s("spot")), ("symbol", s("BTC"))]),
                    "cancel symbol payload",
                );
                let keep = scene.contains("keep");
                assert_eq!(
                    h.placed.borrow().len(),
                    usize::from(keep && !failed),
                    "SPOT native recreation count"
                );
                if keep {
                    let rows = vals(&h.placed.borrow()[0]);
                    assert_eq!(rows.len(), 1, "SPOT native protective recreation row count");
                    native(
                        &get(&rows[0], "side"),
                        &s("sell"),
                        "recreation flipped side",
                    );
                }
                assert_eq!(
                    h.retired.borrow().len(),
                    usize::from(!failed && !scene.starts_with("api-error")),
                    "SPOT native cancel-symbol retirement count"
                );
            }
            cases.push(json!({"op":op,"scene":scene,"opts":wire(&opts),"order":wire(&original),"update":wire(&update),"orders":wire(&input_orders),"payload":wire(&payload)}));
            outputs.push(json!({"result":match result{Ok(v)=>json!({"ok":wire(&v)}),Err(e)=>json!({"error":wire(&get(&e.0,"message")),"original":eq(&e.0,&h.original_error)})},"events":h.events.borrow().iter().map(|(k,args)|json!([k,args.iter().map(wire).collect::<Vec<_>>()])).collect::<Vec<_>>(),"requests":h.requests.borrow().iter().map(|(e,p)|json!([e,wire(p)])).collect::<Vec<_>>(),"recorded":h.recorded.borrow().iter().map(wire).collect::<Vec<_>>(),"updated":h.updated.borrow().iter().map(wire).collect::<Vec<_>>(),"retired":h.retired.borrow().iter().map(wire).collect::<Vec<_>>(),"placed":h.placed.borrow().iter().map(wire).collect::<Vec<_>>() }));
        }
    }
    assert_eq!(cases.len(), 40, "SPOT independent input floor");
    assert_eq!(
        json!(outputs),
        source(json!(cases)),
        "SPOT source bodies after every native assertion"
    );
}

#[test]
fn spot_native_emitter_capture_before_message_getter() {
    let mut cases = vec![];
    for op in ["stop", "trailing"] {
        let h = H::new("emit-capture");
        let opts = opts("normal");
        let result = if op == "stop" {
            run(api::place_stop_loss_or_take_profit(
                &h,
                &h.owner,
                opts.clone(),
            ))
        } else {
            run(api::place_trailing_stop_loss(&h, &h.owner, opts.clone()))
        };
        let Err(e) = result else {
            panic!("SPOT native emitter failure expected")
        };
        assert!(
            eq(&e.0, &h.original_error),
            "SPOT native emitter capture error identity"
        );
        assert_eq!(
            h.retmsg_reads.get(),
            1,
            "SPOT native emitter selected before message getter"
        );
        cases.push(json!({"op":op,"scene":"emit-capture","opts":wire(&opts),"order":wire(&V::U),"orders":wire(&V::U),"update":wire(&V::U),"payload":wire(&V::U)}));
    }
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/spot-order-writes-oracle.mjs"
        ))
        .arg("--capture-diagnostic")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(json!(cases).to_string().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "SPOT capture source {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let captured: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        captured,
        json!([{ "original":true,"retMsgReads":1},{"original":true,"retMsgReads":1}]),
        "SPOT source emitter capture order"
    );
}

#[path = "support/primary_local_order_writes.rs"]
mod primary_local_order_write_tests;
