use safe_cex_venue_bitget::private_topics::{self as body, *};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
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
    Set(Rc<RefCell<Vec<V>>>),
    Map(Rc<RefCell<Vec<(V, f64)>>>),
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
        V::O(_) | V::Set(_) | V::Map(_) => "[object Object]".into(),
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
fn num(v: &V) -> f64 {
    match v {
        V::N(n) => *n,
        _ => panic!("TOPIC native numeric field required: {v:?}"),
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
        (V::Set(a), V::Set(b)) => Rc::ptr_eq(a, b),
        (V::Map(a), V::Map(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn values(v: &V) -> Vec<V> {
    match v {
        V::A(v) => v.borrow().clone(),
        _ => panic!("TOPIC native array required"),
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
            "TOPIC native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "TOPIC native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "TOPIC native independent array count {path}"
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
                "TOPIC native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "TOPIC native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        (V::Set(x), V::Set(y)) => native(
            &a(x.borrow().clone()),
            &a(y.borrow().clone()),
            &format!("{path}.set"),
        ),
        (V::Map(x), V::Map(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "TOPIC native independent map count {path}"
            );
            for ((kx, vx), (ky, vy)) in x.iter().zip(y.iter()) {
                native(kx, ky, path);
                native(&V::N(*vx), &V::N(*vy), path);
            }
        }
        _ => assert!(
            equal(actual, expected),
            "TOPIC native scalar {path}: {actual:?} != {expected:?}"
        ),
    }
}
fn wire(v: &V) -> Json {
    match v {
        V::Set(v) => json!({"set":v.borrow().iter().map(wire).collect::<Vec<_>>()}),
        V::Map(v) => {
            json!({"map":v.borrow().iter().map(|(k,n)|json!([wire(k),wire(&V::N(*n))])).collect::<Vec<_>>()})
        }
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
    data: V,
    orders: V,
    original: Rc<()>,
    fail: Option<usize>,
    events: RefCell<Vec<Event>>,
    pending_orders: Cell<bool>,
    pending_positions: Cell<bool>,
    pending_balance: Cell<bool>,
    snapshot_at: Cell<f64>,
    snapshot_ids: RefCell<V>,
    updated: V,
    property_reads: Cell<usize>,
    probe: &'static str,
    alternate: V,
    live: Cell<bool>,
    live_kind: &'static str,
}
fn event(name: &str, args: Vec<V>) -> Event {
    Event {
        name: name.into(),
        args,
    }
}
fn parent() -> V {
    o(vec![
        ("id", s("a")),
        ("side", s("buy")),
        ("symbol", s("BTC")),
        ("price", V::N(91.)),
    ])
}
fn child() -> V {
    o(vec![
        ("id", s("b")),
        ("side", s("sell")),
        ("symbol", s("BTC")),
        ("price", V::N(92.)),
    ])
}
fn raw_order(status: &str, scene: &str) -> V {
    o(vec![
        ("status", s(status)),
        ("orderStatus", s(status)),
        ("posMode", s("hedge_mode")),
        ("reduceOnly", s(if scene == "ack" { "no" } else { "yes" })),
        ("side", s("sell")),
        (
            "priceAvg",
            if scene == "partial" {
                V::N(0.)
            } else {
                s("125.5")
            },
        ),
        (
            "baseVolume",
            if scene == "partial" {
                V::N(0.)
            } else {
                s("2.25")
            },
        ),
        ("accBaseVolume", s("3.5")),
        (
            "actualSize",
            if scene == "partial" {
                s("0")
            } else {
                s("2.25")
            },
        ),
        ("triggerPrice", s("125.5")),
        ("avgPrice", s("101.5")),
        ("cumExecQty", s("8.5")),
        ("info", o(vec![("opaque", s("payload"))])),
    ])
}
fn raw_position(id: &str) -> V {
    o(vec![("id", s(id)), ("symbol", s("BTC"))])
}
const OPS: [&str; 8] = [
    "mark",
    "order",
    "uta-order",
    "algo",
    "positions",
    "uta-positions",
    "account",
    "uta-account",
];
const SCENES: [&str; 6] = ["normal", "empty", "invalid", "partial", "ack", "cancel"];
fn data(op: &str, sc: &str) -> V {
    let status = match sc {
        "partial" => "partial_fill",
        "ack" => "success",
        "cancel" => "canceled",
        _ => "filled",
    };
    let rows = match op {
        "order" | "algo" | "uta-order" => {
            if sc == "empty" {
                a(vec![])
            } else if sc == "invalid" {
                a(vec![raw_order("unrecognized", sc)])
            } else {
                a(vec![raw_order(status, sc)])
            }
        }
        "mark" => {
            if sc == "empty" {
                a(vec![])
            } else {
                a(vec![
                    raw_position("p1"),
                    raw_position("p2"),
                    raw_position("p1"),
                ])
            }
        }
        "positions" | "uta-positions" => match sc {
            "empty" => a(vec![]),
            "invalid" => a(vec![V::Null, V::N(0.), V::Bool(false), s("bad")]),
            "cancel" => o(vec![]),
            "partial" => a(vec![V::Null, raw_position("p1"), V::Bool(false), a(vec![])]),
            _ => a(vec![raw_position("p1"), raw_position("p2")]),
        },
        "account" => {
            if sc == "empty" {
                a(vec![])
            } else {
                a(vec![
                    o(vec![("marginCoin", s("BTC"))]),
                    o(vec![
                        ("marginCoin", s("USDT")),
                        (
                            "unionTotalMargin",
                            if sc == "invalid" { s("bad") } else { s("100") },
                        ),
                        ("unionAvailable", s("60")),
                        ("unrealizedPL", if sc == "partial" { s("") } else { s("3") }),
                        ("crossedUnrealizedPL", s("1")),
                        ("isolatedUnrealizedPL", s("2")),
                    ]),
                ])
            }
        }
        "uta-account" => {
            let raw = o(vec![
                (
                    "accountEquity",
                    if sc == "invalid" { s("bad") } else { s("200") },
                ),
                ("effEquity", s("80")),
                (
                    "unrealisedPnl",
                    if sc == "partial" { s("") } else { s("4") },
                ),
                ("usdtUnrealisedPnl", s("9")),
            ]);
            match sc {
                "empty" => a(vec![]),
                "ack" => raw,
                _ => a(vec![raw]),
            }
        }
        _ => panic!(),
    };
    o(vec![
        ("data", rows),
        (
            "action",
            match sc {
                "normal" | "empty" | "invalid" | "cancel" => s("snapshot"),
                "partial" => s("update"),
                _ => V::N(7.),
            },
        ),
        ("tag", s("channel")),
    ])
}
impl H {
    fn new(op: &str, sc: &str, fail: Option<usize>) -> Self {
        Self {
            data: data(op, sc),
            orders: a(vec![parent(), child()]),
            original: Rc::new(()),
            fail,
            events: RefCell::new(vec![]),
            pending_orders: Cell::new(false),
            pending_positions: Cell::new(false),
            pending_balance: Cell::new(false),
            snapshot_at: Cell::new(0.),
            snapshot_ids: RefCell::new(V::Set(Rc::new(RefCell::new(vec![s("old")])))),
            updated: V::Map(Rc::new(RefCell::new(vec![(s("old"), 99.)]))),
            property_reads: Cell::new(0),
            probe: "",
            alternate: a(vec![raw_position("next")]),
            live: Cell::new(false),
            live_kind: "",
        }
    }
    fn error(&self) -> E {
        E {
            identity: self.original.clone(),
            message: s("original"),
        }
    }
    fn record(&self, name: &str, args: Vec<V>) -> Result<(), E> {
        let mut events = self.events.borrow_mut();
        let idx = events.len();
        events.push(event(name, args));
        if self.fail == Some(idx) {
            return Err(self.error());
        }
        Ok(())
    }
    fn mapped_position(&self, raw: V, uta: bool) -> V {
        o(vec![
            (
                "id",
                if matches!(raw, V::A(_)) {
                    s("array")
                } else {
                    get(&raw, "id")
                },
            ),
            ("uta", V::Bool(uta)),
            ("info", raw),
        ])
    }
}
impl PrivateTopicsHost for H {
    type Value = V;
    type Error = E;
    type Iterator = Iter;
    fn property(&self, v: &V, name: &str) -> Result<V, E> {
        if equal(v, &self.data) && name == "data" && !self.probe.is_empty() {
            let n = self.property_reads.get();
            self.property_reads.set(n + 1);
            self.record("data-get", vec![])?;
            if n == 1 {
                if self.probe == "throw" {
                    return Err(self.error());
                }
                return Ok(self.alternate.clone());
            }
        }
        if matches!(v, V::U | V::Null) {
            return Err(E {
                identity: Rc::new(()),
                message: s(&format!(
                    "Cannot read properties of {} (reading '{name}')",
                    text(v)
                )),
            });
        }
        Ok(get(v, name))
    }
    fn receiver(&self, n: &str) -> Result<V, E> {
        Ok(match n {
            "parent.apiMarginCoin" => s("USDT"),
            "wsPositionUpdatedAtById" => self.updated.clone(),
            _ => panic!("receiver {n}"),
        })
    }
    fn set_receiver(&self, n: &str, v: V) -> Result<(), E> {
        self.record(&format!("set:{n}"), vec![v.clone()])?;
        match n {
            "hasPendingOrderEvents" => self.pending_orders.set(truth(&v)),
            "hasPendingPositionEvents" => self.pending_positions.set(truth(&v)),
            "hasPendingBalanceEvents" => self.pending_balance.set(truth(&v)),
            "lastWsPositionSnapshotAt" => self.snapshot_at.set(num(&v)),
            "lastWsPositionSnapshotIds" => *self.snapshot_ids.borrow_mut() = v,
            _ => panic!(),
        }
        Ok(())
    }
    fn call(&self, n: &str, args: Vec<V>) -> Result<V, E> {
        self.record(n, args.clone())?;
        if self.live_kind == "forEach" && n == "store.updateOrderStatus" && !self.live.replace(true)
        {
            if let V::A(orders) = &self.orders {
                let mut orders = orders.borrow_mut();
                orders[1] = o(vec![
                    ("id", s("late")),
                    ("side", s("sell")),
                    ("symbol", s("BTC")),
                    ("price", V::N(88.)),
                ]);
                orders.push(o(vec![("id", s("extra"))]));
            }
        }
        if self.live_kind == "iterator" && n == "parent.mapOrder" && !self.live.replace(true) {
            if let V::A(rows) = get(&self.data, "data") {
                rows.borrow_mut().push(raw_order("filled", "normal"));
            }
        }

        Ok(match n {
            "parent.mapOrder" => self.orders.clone(),
            "parent.mapPosition" => self.mapped_position(args[0].clone(), false),
            "parent.mapUtaPosition" => self.mapped_position(args[0].clone(), true),
            "getRawOrderStatus" => {
                let mut v = V::U;
                for name in ["status", "state", "planStatus", "orderStatus"] {
                    v = self.property(&args[0], name)?;
                    if !self.nullish(&v) {
                        break;
                    }
                }
                s(&if self.nullish(&v) {
                    "".into()
                } else {
                    text(&v)
                }
                .to_lowercase()
                .replace('-', "_"))
            }
            "isTerminalFillStatus" => V::Bool(matches!(
                text(&args[0]).as_str(),
                "triggered" | "filled" | "full_fill"
            )),
            _ => V::U,
        })
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U | V::Null)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        equal(a, b)
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(v, V::A(_))
    }
    fn is_object(&self, v: &V) -> bool {
        matches!(v, V::A(_) | V::O(_) | V::Null | V::Set(_) | V::Map(_))
    }
    fn is_string(&self, v: &V) -> bool {
        matches!(v, V::S(_))
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn string(&self, u: &[u16]) -> V {
        V::S(u.to_vec())
    }
    fn number(&self, n: f64) -> V {
        V::N(n)
    }
    fn boolean(&self, b: bool) -> V {
        V::Bool(b)
    }
    fn array(&self, v: Vec<Option<V>>) -> V {
        a(v.into_iter().map(|v| v.unwrap_or(V::Hole)).collect())
    }
    fn length(&self, v: &V) -> Result<usize, E> {
        if let V::A(a) = v {
            Ok(a.borrow().len())
        } else {
            Err(self.error())
        }
    }
    fn has_index(&self, v: &V, i: usize) -> Result<bool, E> {
        if let V::A(a) = v {
            Ok(a.borrow().get(i).is_some_and(|v| !matches!(v, V::Hole)))
        } else {
            Err(self.error())
        }
    }
    fn index(&self, v: &V, i: usize) -> Result<V, E> {
        if let V::A(a) = v {
            Ok(a.borrow()
                .get(i)
                .filter(|v| !matches!(v, V::Hole))
                .cloned()
                .unwrap_or(V::U))
        } else {
            Err(self.error())
        }
    }
    fn iterator(&self, v: &V) -> Result<Iter, E> {
        self.length(v)?;
        Ok(Iter {
            array: v.clone(),
            index: 0,
        })
    }
    fn next(&self, i: &mut Iter) -> Result<Option<V>, E> {
        if i.index >= self.length(&i.array)? {
            return Ok(None);
        }
        let v = self.index(&i.array, i.index)?;
        i.index += 1;
        Ok(Some(v))
    }
    fn object(&self, v: Vec<(&str, V)>) -> V {
        o(v)
    }
    fn spread(&self, v: &V) -> Result<V, E> {
        match v {
            V::O(fields) => {
                let keys: Vec<_> = fields.borrow().iter().map(|(k, _)| k.clone()).collect();
                let mut out = vec![];
                for key in keys {
                    let name = String::from_utf16(&key).unwrap();
                    out.push((key, self.property(v, &name)?));
                }
                Ok(V::O(Rc::new(RefCell::new(out))))
            }
            V::Null | V::U => Ok(o(vec![])),
            _ => Err(self.error()),
        }
    }
    fn define(&self, v: &V, name: &str, value: V) -> Result<(), E> {
        let V::O(v) = v else { panic!() };
        let mut v = v.borrow_mut();
        let key: Vec<u16> = name.encode_utf16().collect();
        if let Some((_, old)) = v.iter_mut().find(|(k, _)| *k == key) {
            *old = value
        } else {
            v.push((key, value))
        }
        Ok(())
    }
    fn finite(&self, v: &V, f: f64) -> Result<f64, E> {
        use safe_cex_venue_bitget::numeric::{FiniteNumberInput, to_finite_number};
        Ok(match v {
            V::N(n) => to_finite_number(FiniteNumberInput::Number(*n), f),
            V::S(t) => to_finite_number(FiniteNumberInput::String(&String::from_utf16_lossy(t)), f),
            _ => f,
        })
    }
    fn parse_float(&self, v: &V) -> Result<f64, E> {
        Ok(safe_cex_num::parse_float(&text(v)))
    }
    fn status_lookup(&self, v: &V) -> Result<V, E> {
        Ok(match text(v).as_str() {
            "init" | "new" | "not_trigger" | "partial_fill" | "partially_fill"
            | "partially_filled" | "live" | "pending" | "submitting" => s("open"),
            "triggered" | "full_fill" | "filled" | "success" | "executed" => s("closed"),
            "fail_trigger" | "cancel" | "cancelled" | "canceled" | "failed" | "fail_execute" => {
                s("canceled")
            }
            _ => V::U,
        })
    }
    fn account_upnl(&self, n: f64) -> Result<f64, E> {
        self.record("parent.calculateAccountUpnl", vec![V::N(n)])?;
        Ok(n + 2.)
    }
    fn clock_now(&self) -> Result<f64, E> {
        self.record("clock", vec![])?;
        Ok(1234.)
    }
    fn set_of(&self, v: V) -> Result<V, E> {
        let mut out = vec![];
        for v in values(&v) {
            let v = if matches!(v, V::Hole) { V::U } else { v };
            if !out.iter().any(|x| equal(x, &v)) {
                out.push(v)
            }
        }
        Ok(V::Set(Rc::new(RefCell::new(out))))
    }
    fn map_set(&self, map: V, key: V, value: f64) -> Result<(), E> {
        self.record("map.set", vec![key.clone(), V::N(value)])?;
        let V::Map(m) = map else { panic!() };
        let mut m = m.borrow_mut();
        if let Some((_, old)) = m.iter_mut().find(|(k, _)| equal(k, &key)) {
            *old = value
        } else {
            m.push((key, value))
        }
        Ok(())
    }
}
fn execute(h: &H, op: &str) -> Result<(), E> {
    match op {
        "mark" => body::mark_ws_positions(h, get(&h.data, "data"), get(&h.data, "action")),
        "order" => body::handle_order_topic(h, h.data.clone()),
        "uta-order" => body::handle_uta_order_topic(h, h.data.clone()),
        "algo" => body::handle_algo_orders_topic(h, h.data.clone()),
        "positions" => body::handle_positions_topic(h, h.data.clone()),
        "uta-positions" => body::handle_uta_positions_topic(h, h.data.clone()),
        "account" => body::handle_account_topic(h, h.data.clone()),
        "uta-account" => body::handle_uta_account_topic(h, h.data.clone()),
        _ => panic!(),
    }
}
fn expected(h: &H, op: &str, sc: &str) -> Vec<Event> {
    let mut out = vec![];
    let mut push = |n: &str, args: Vec<V>| out.push(event(n, args));
    let rows = get(&h.data, "data");
    match op {
        "mark" => {
            push("clock", vec![]);
            if matches!(sc, "normal" | "empty" | "invalid" | "cancel") {
                push("set:lastWsPositionSnapshotAt", vec![V::N(1234.)]);
                push(
                    "set:lastWsPositionSnapshotIds",
                    vec![V::Set(Rc::new(RefCell::new(if sc == "empty" {
                        vec![]
                    } else {
                        vec![s("p1"), s("p2")]
                    })))],
                );
            }
            if sc != "empty" {
                for id in ["p1", "p2", "p1"] {
                    push("map.set", vec![s(id), V::N(1234.)]);
                }
            }
        }
        "uta-order" => {
            let payload = o(vec![
                ("data", a(vec![])),
                ("action", get(&h.data, "action")),
                ("tag", s("channel")),
            ]);
            let mut orders = vec![];
            if sc != "empty" {
                let original = values(&rows)[0].clone();
                let V::O(fields) = original else { panic!() };
                let clone = V::O(Rc::new(RefCell::new(fields.borrow().clone())));
                h.define(&clone, "status", get(&clone, "orderStatus"))
                    .unwrap();
                h.define(&clone, "priceAvg", s("101.5")).unwrap();
                orders.push(clone);
            }
            h.define(&payload, "data", a(orders)).unwrap();
            push("handleOrderTopic", vec![payload]);
        }
        "order" | "algo" => {
            push("parent.markPrivateStreamMessageReceived", vec![]);
            if sc != "empty" {
                let raw = values(&rows)[0].clone();
                let status = match sc {
                    "partial" => "partial_fill",
                    "ack" => "success",
                    "cancel" => "canceled",
                    "invalid" => "unrecognized",
                    _ => "filled",
                };
                push("getRawOrderStatus", vec![raw.clone()]);
                if op == "order" {
                    push("isTerminalFillStatus", vec![s(status)])
                }
                let V::O(fields) = raw else { panic!() };
                let payload = V::O(Rc::new(RefCell::new(fields.borrow().clone())));
                h.define(
                    &payload,
                    "side",
                    s(if op == "order" && sc == "ack" {
                        "sell"
                    } else {
                        "buy"
                    }),
                )
                .unwrap();
                push("parent.mapOrder", vec![payload]);
                if op == "algo" {
                    push("isTerminalFillStatus", vec![s(status)])
                }
                if op == "algo" && sc != "invalid" {
                    if sc == "partial" {
                        push("store.addOrUpdateOrder", vec![parent()]);
                        push("markOrderAsOpenFromWs", vec![s("a")]);
                    } else {
                        push(
                            "store.updateOrderStatus",
                            vec![
                                parent(),
                                s(if sc == "cancel" { "canceled" } else { "closed" }),
                            ],
                        );
                        push("markOrderAsTerminal", vec![s("a")]);
                    }
                }
                if sc == "normal" || (op == "order" && sc == "partial") {
                    push(
                        "emitter.emit",
                        vec![
                            s("fill"),
                            o(vec![
                                ("id", s("a")),
                                ("side", s("buy")),
                                ("symbol", s("BTC")),
                                (
                                    "price",
                                    if op == "algo" {
                                        V::N(125.5)
                                    } else if sc == "partial" {
                                        V::N(91.)
                                    } else {
                                        s("125.5")
                                    },
                                ),
                                ("amount", V::N(if sc == "partial" { 3.5 } else { 2.25 })),
                            ]),
                        ],
                    );
                    push("throttledFetchBalance", vec![]);
                }
                if sc == "normal" {
                    push("throttledFetchPositions", vec![]);
                    push("throttledFetchBalance", vec![]);
                }
                if op == "order" && sc != "invalid" {
                    for (order, id) in [(parent(), "a"), (child(), "b")] {
                        if sc == "partial" {
                            push("store.addOrUpdateOrder", vec![order]);
                            push("markOrderAsOpenFromWs", vec![s(id)]);
                        } else {
                            push(
                                "store.updateOrderStatus",
                                vec![order, s(if sc == "cancel" { "canceled" } else { "closed" })],
                            );
                            push("markOrderAsTerminal", vec![s(id)]);
                        }
                    }
                }
            }
            push("set:hasPendingOrderEvents", vec![V::Bool(true)]);
            push("throttledFetchOrders", vec![]);
        }
        "positions" | "uta-positions" => {
            push("parent.markPrivateStreamMessageReceived", vec![]);
            let action = get(&h.data, "action");
            let action = if matches!(action, V::S(_)) {
                action
            } else {
                V::U
            };
            let snapshot = matches!(sc, "normal" | "empty" | "invalid" | "cancel");
            if matches!(sc, "empty" | "invalid" | "cancel") {
                push("markWsPositions", vec![a(vec![]), s("snapshot")]);
                push("store.addOrUpdatePositionsFull", vec![a(vec![])]);
            } else {
                let raws = values(&rows);
                let raw = if sc == "partial" {
                    vec![raws[1].clone(), raws[3].clone()]
                } else {
                    raws
                };
                let mut mapped = vec![];
                for raw in raw {
                    push(
                        if op == "uta-positions" {
                            "parent.mapUtaPosition"
                        } else {
                            "parent.mapPosition"
                        },
                        vec![raw.clone()],
                    );
                    mapped.push(o(vec![
                        (
                            "id",
                            if matches!(raw, V::A(_)) {
                                s("array")
                            } else {
                                get(&raw, "id")
                            },
                        ),
                        ("uta", V::Bool(op == "uta-positions")),
                        ("info", raw),
                    ]));
                }
                let positions = a(mapped.clone());
                push("markWsPositions", vec![positions.clone(), action]);
                if snapshot {
                    push("store.addOrUpdatePositionsFull", vec![positions])
                } else {
                    for p in mapped {
                        push("store.addOrUpdatePosition", vec![p]);
                    }
                }
            }
            push("set:hasPendingPositionEvents", vec![V::Bool(true)]);
            push("throttledFetchPositions", vec![]);
            push("throttledFetchBalance", vec![]);
        }
        "account" | "uta-account" => {
            push("parent.markPrivateStreamMessageReceived", vec![]);
            if sc != "empty" && sc != "invalid" {
                let uta = op == "uta-account";
                let info = if uta {
                    if sc == "ack" {
                        rows
                    } else {
                        values(&rows)[0].clone()
                    }
                } else {
                    values(&rows)[1].clone()
                };
                let api = if uta {
                    if sc == "partial" { 9. } else { 4. }
                } else {
                    3.
                };
                push("parent.calculateAccountUpnl", vec![V::N(api)]);
                push(
                    "store.update",
                    vec![o(vec![(
                        "balance",
                        o(vec![
                            ("used", V::N(if uta { 120. } else { 40. })),
                            ("free", V::N(if uta { 80. } else { 60. })),
                            (
                                "total",
                                V::N(if uta {
                                    if sc == "partial" { 189. } else { 194. }
                                } else {
                                    95.
                                }),
                            ),
                            ("upnl", V::N(api + 2.)),
                            ("info", info),
                        ]),
                    )])],
                );
                if uta {
                    push("set:hasPendingBalanceEvents", vec![V::Bool(true)]);
                }
            }
            if op == "uta-account" {
                push("throttledFetchBalance", vec![]);
            }
        }
        _ => panic!(),
    };
    out
}
fn assert_events(actual: &[Event], expected: &[Event], path: &str) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "TOPIC native independent event count {path}"
    );
    for (i, (actual, e)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual.name, e.name,
            "TOPIC native operation order {path}/{i}"
        );
        native(
            &a(actual.args.clone()),
            &a(e.args.clone()),
            &format!("{path}/{i}:{}", actual.name),
        );
    }
}
fn pack_events(events: &[Event]) -> Json {
    json!(
        events
            .iter()
            .map(|e| json!({"name":e.name,"args":e.args.iter().map(wire).collect::<Vec<_>>()}))
            .collect::<Vec<_>>()
    )
}
fn output(h: &H, result: &Result<(), E>) -> Json {
    json!({"events":pack_events(&h.events.borrow()),"orders":h.pending_orders.get(),"positions":h.pending_positions.get(),"balance":h.pending_balance.get(),"snapshotAt":wire(&V::N(h.snapshot_at.get())),"snapshotIds":wire(&h.snapshot_ids.borrow()),"updated":wire(&h.updated),"error":result.as_ref().err().map(|e|json!({"original":Rc::ptr_eq(&e.identity,&h.original),"message":wire(&e.message)}))})
}
fn input(h: &H, op: &str, scene: &str) -> Json {
    json!({"op":op,"scene":scene,"data":wire(&h.data),"orders":wire(&h.orders),"fail":h.fail,"probe":h.probe,"alternate":wire(&h.alternate),"live":h.live_kind})
}
fn source(cases: &[Json], mode: &str) -> Vec<Json> {
    let mut c = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/private-topics-capture.mjs"),
        )
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let write = c
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes());
    let out = c.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "TOPIC actual source: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    write.unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn native_topics_then_actual_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for op in OPS {
        for sc in SCENES {
            let h = H::new(op, sc, None);
            let result = execute(&h, op);
            assert!(result.is_ok(), "TOPIC native success {op}/{sc}: {result:?}");
            assert_events(
                &h.events.borrow(),
                &expected(&h, op, sc),
                &format!("{op}/{sc}"),
            );
            assert_eq!(
                h.pending_orders.get(),
                matches!(op, "order" | "algo"),
                "TOPIC native pending order flag"
            );
            assert_eq!(
                h.pending_positions.get(),
                matches!(op, "positions" | "uta-positions"),
                "TOPIC native pending position flag"
            );
            assert_eq!(
                h.pending_balance.get(),
                op == "uta-account" && !matches!(sc, "empty" | "invalid"),
                "TOPIC native pending balance flag"
            );
            if op == "mark" {
                native(
                    &V::N(h.snapshot_at.get()),
                    &V::N(if matches!(sc, "normal" | "empty" | "invalid" | "cancel") {
                        1234.
                    } else {
                        0.
                    }),
                    "snapshot time",
                );
                native(
                    &h.snapshot_ids.borrow(),
                    &V::Set(Rc::new(RefCell::new(if sc == "empty" {
                        vec![]
                    } else if matches!(sc, "normal" | "invalid" | "cancel") {
                        vec![s("p1"), s("p2")]
                    } else {
                        vec![s("old")]
                    }))),
                    "snapshot IDs",
                );
                native(
                    &h.updated,
                    &V::Map(Rc::new(RefCell::new(if sc == "empty" {
                        vec![(s("old"), 99.)]
                    } else {
                        vec![(s("old"), 99.), (s("p1"), 1234.), (s("p2"), 1234.)]
                    }))),
                    "position update map",
                );
            }
            if matches!(op, "account" | "uta-account") && !matches!(sc, "empty" | "invalid") {
                let events = h.events.borrow();
                let patch = &events
                    .iter()
                    .find(|e| e.name == "store.update")
                    .unwrap()
                    .args[0];
                let rows = get(&h.data, "data");
                let original = if op == "account" {
                    values(&rows)[1].clone()
                } else if sc == "ack" {
                    rows
                } else {
                    values(&rows)[0].clone()
                };
                assert!(
                    equal(&get(&get(patch, "balance"), "info"), &original),
                    "TOPIC native balance info alias"
                );
            }
            if matches!(op, "positions" | "uta-positions") && sc == "normal" {
                let events = h.events.borrow();
                let mark = events.iter().find(|e| e.name == "markWsPositions").unwrap();
                let full = events
                    .iter()
                    .find(|e| e.name == "store.addOrUpdatePositionsFull")
                    .unwrap();
                assert!(
                    equal(&mark.args[0], &full.args[0]),
                    "TOPIC native snapshot array alias"
                );
                for (p, raw) in values(&full.args[0])
                    .iter()
                    .zip(values(&get(&h.data, "data")))
                {
                    assert!(
                        equal(&get(p, "info"), &raw),
                        "TOPIC native mapped position info alias"
                    );
                }
            }
            if matches!(op, "order" | "algo") && sc != "empty" {
                let events = h.events.borrow();
                let payload = &events
                    .iter()
                    .find(|e| e.name == "parent.mapOrder")
                    .unwrap()
                    .args[0];
                let original = values(&get(&h.data, "data"))[0].clone();
                assert!(
                    equal(&get(payload, "info"), &get(&original, "info")),
                    "TOPIC native mapped raw info alias"
                );
                for e in events.iter().filter(|e| {
                    e.name == "store.addOrUpdateOrder" || e.name == "store.updateOrderStatus"
                }) {
                    assert!(
                        values(&h.orders).iter().any(|o| equal(&e.args[0], o)),
                        "TOPIC native stored order alias"
                    );
                }
            }
            cases.push(input(&h, op, sc));
            outputs.push(output(&h, &result));
        }
    }
    assert_eq!(cases.len(), 48, "TOPIC independent primary floor");
    let source = source(&cases, "primary");
    assert_eq!(source.len(), 48, "TOPIC independent source floor");
    for (i, (a, b)) in outputs.iter().zip(source.iter()).enumerate() {
        assert_eq!(a, b, "TOPIC source primary case {i}");
    }
}
#[test]
fn native_original_errors_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for op in OPS {
        let h = H::new(op, "normal", Some(0));
        let result = execute(&h, op);
        let e = result.as_ref().unwrap_err();
        assert!(
            Rc::ptr_eq(&e.identity, &h.original),
            "TOPIC native original error identity"
        );
        native(&e.message, &s("original"), "original error payload");
        assert_eq!(
            h.events.borrow().len(),
            1,
            "TOPIC native first-failure event count"
        );
        assert!(
            !h.pending_orders.get() && !h.pending_positions.get() && !h.pending_balance.get(),
            "TOPIC native first failure flags"
        );
        cases.push(input(&h, op, "normal"));
        outputs.push(output(&h, &result));
    }
    let src = source(&cases, "errors");
    assert_eq!(src.len(), 8, "TOPIC independent error source floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        assert_eq!(a, b, "TOPIC source error case {i}");
    }
}
#[test]
fn native_guarded_data_getters_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for (op, probe) in [
        ("positions", "change"),
        ("uta-positions", "throw"),
        ("uta-order", "change"),
        ("uta-account", "change"),
    ] {
        let mut h = H::new(op, if op == "uta-account" { "ack" } else { "normal" }, None);
        h.probe = probe;
        if op == "positions" {
            let raw = raw_position("next");
            h.define(&raw, "id", V::S(vec![0xd800, 65])).unwrap();
            h.alternate = a(vec![raw]);
        }
        if op == "uta-order" {
            let raw = raw_order("canceled", "normal");
            h.define(&raw, "baseVolume", V::Null).unwrap();
            h.alternate = a(vec![raw]);
        }
        if op == "uta-account" {
            h.alternate = o(vec![
                ("accountEquity", s("500")),
                ("effEquity", s("40")),
                ("unrealisedPnl", s("2")),
            ]);
        }
        let result = execute(&h, op);
        assert_eq!(
            h.property_reads.get(),
            if op == "uta-order" { 3 } else { 2 },
            "TOPIC native data getter count"
        );
        if probe == "throw" {
            let e = result.as_ref().unwrap_err();
            assert!(
                Rc::ptr_eq(&e.identity, &h.original),
                "TOPIC native guarded getter original error"
            );
            assert_eq!(
                h.events.borrow().len(),
                3,
                "TOPIC native getter failure effect count"
            );
            assert!(
                !h.pending_positions.get(),
                "TOPIC native getter failure no pending flag"
            );
        } else {
            assert!(result.is_ok());
            let events = h.events.borrow();
            match op {
                "positions" => {
                    let p = &events
                        .iter()
                        .find(|e| e.name == "store.addOrUpdatePositionsFull")
                        .unwrap()
                        .args[0];
                    native(
                        &get(&values(p)[0], "id"),
                        &V::S(vec![0xd800, 65]),
                        "second data UTF16 position",
                    );
                    assert_eq!(
                        values(p).len(),
                        1,
                        "TOPIC native second data position count"
                    );
                    assert!(
                        equal(&get(&values(p)[0], "info"), &values(&h.alternate)[0]),
                        "TOPIC native second data position alias"
                    );
                }
                "uta-order" => {
                    let data = &events
                        .iter()
                        .find(|e| e.name == "handleOrderTopic")
                        .unwrap()
                        .args[0];
                    let raw = values(&get(data, "data"))[0].clone();
                    native(
                        &get(&raw, "status"),
                        &s("canceled"),
                        "second data UTA order status",
                    );
                    native(
                        &get(&raw, "baseVolume"),
                        &s("8.5"),
                        "UTA nullish volume fallback",
                    );
                    native(&get(&raw, "priceAvg"), &s("101.5"), "UTA price replacement");
                }
                "uta-account" => {
                    let balance = get(
                        &events
                            .iter()
                            .find(|e| e.name == "store.update")
                            .unwrap()
                            .args[0],
                        "balance",
                    );
                    native(
                        &balance,
                        &o(vec![
                            ("used", V::N(460.)),
                            ("free", V::N(40.)),
                            ("total", V::N(496.)),
                            ("upnl", V::N(4.)),
                            ("info", h.alternate.clone()),
                        ]),
                        "second non-array data account",
                    );
                    assert!(
                        equal(&get(&balance, "info"), &h.alternate),
                        "TOPIC native second data account alias"
                    );
                }
                _ => panic!(),
            }
        }
        cases.push(input(&h, op, "probe"));
        outputs.push(output(&h, &result));
    }
    let src = source(&cases, "probes");
    assert_eq!(src.len(), 4, "TOPIC independent getter source floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        assert_eq!(a, b, "TOPIC source guarded getter case {i}");
    }
}
#[test]
fn native_explicit_and_ambiguous_statuses_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for op in ["order", "algo"] {
        for (status, normal_fills, algo_fills, full, mapped) in [
            ("partially_fill", 0, 0, false, "open"),
            ("partially_filled", 1, 0, false, "open"),
            ("full_fill", 1, 1, true, "closed"),
            ("executed", 0, 0, false, "closed"),
            ("not_trigger", 0, 0, false, "open"),
        ] {
            let h = H::new(op, "normal", None);
            let raw = values(&get(&h.data, "data"))[0].clone();
            h.define(&raw, "status", s(status)).unwrap();
            let result = execute(&h, op);
            assert!(result.is_ok());
            let events = h.events.borrow();
            let fills: Vec<_> = events.iter().filter(|e| e.name == "emitter.emit").collect();
            assert_eq!(
                fills.len(),
                if op == "order" {
                    normal_fills
                } else {
                    algo_fills
                },
                "TOPIC native explicit status fill count {op}/{status}"
            );
            for event in fills {
                native(&event.args[0], &s("fill"), "fill event name");
                native(
                    &event.args[1],
                    &o(vec![
                        ("id", s("a")),
                        ("side", s("buy")),
                        ("symbol", s("BTC")),
                        (
                            "price",
                            if op == "order" {
                                s("125.5")
                            } else {
                                V::N(125.5)
                            },
                        ),
                        ("amount", V::N(2.25)),
                    ]),
                    "explicit status fill fields",
                );
            }
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.name == "throttledFetchPositions")
                    .count(),
                usize::from(full),
                "TOPIC native explicit status position reconcile count"
            );
            let writes: Vec<_> = events
                .iter()
                .filter(|e| {
                    e.name
                        == if mapped == "open" {
                            "store.addOrUpdateOrder"
                        } else {
                            "store.updateOrderStatus"
                        }
                })
                .collect();
            assert_eq!(
                writes.len(),
                if op == "order" { 2 } else { 1 },
                "TOPIC native explicit status store count"
            );
            if mapped == "closed" {
                for e in writes {
                    native(&e.args[1], &s("closed"), "ambiguous close transition");
                }
            }
            cases.push(input(&h, op, status));
            outputs.push(output(&h, &result));
        }
    }
    let src = source(&cases, "statuses");
    assert_eq!(src.len(), 10, "TOPIC independent status source floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        assert_eq!(a, b, "TOPIC source status case {i}");
    }
}
#[test]
fn native_live_arrays_and_utf16_then_source() {
    let mut cases = vec![];
    let mut outputs = vec![];
    for kind in ["forEach", "iterator", "utf16"] {
        let mut h = H::new(
            if kind == "utf16" { "mark" } else { "order" },
            "normal",
            None,
        );
        h.live_kind = kind;
        if kind == "utf16" {
            h.data = o(vec![
                (
                    "data",
                    a(vec![o(vec![("id", V::S(vec![0xd800, 0, 0xdc00]))])]),
                ),
                ("action", s("snapshot")),
            ]);
        }
        let op = if kind == "utf16" { "mark" } else { "order" };
        let case = input(&h, op, kind);
        let result = execute(&h, op);
        assert!(result.is_ok());
        let events = h.events.borrow();
        if kind == "forEach" {
            let ids = events
                .iter()
                .filter(|e| e.name == "markOrderAsTerminal")
                .map(|e| e.args[0].clone())
                .collect();
            native(
                &a(ids),
                &a(vec![s("a"), s("late")]),
                "forEach initial length and live replacement",
            );
            assert_eq!(
                values(&h.orders).len(),
                3,
                "TOPIC native appended mapper array length"
            );
        } else if kind == "iterator" {
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.name == "parent.mapOrder")
                    .count(),
                2,
                "TOPIC native live iterator map count"
            );
            assert_eq!(
                events.iter().filter(|e| e.name == "emitter.emit").count(),
                2,
                "TOPIC native live iterator fill count"
            );
            assert_eq!(
                events
                    .iter()
                    .filter(|e| e.name == "store.updateOrderStatus")
                    .count(),
                4,
                "TOPIC native live iterator terminal count"
            );
        } else {
            native(
                &h.snapshot_ids.borrow(),
                &V::Set(Rc::new(RefCell::new(vec![V::S(vec![0xd800, 0, 0xdc00])]))),
                "UTF16 native snapshot Set",
            );
            native(
                &h.updated,
                &V::Map(Rc::new(RefCell::new(vec![
                    (s("old"), 99.),
                    (V::S(vec![0xd800, 0, 0xdc00]), 1234.),
                ]))),
                "UTF16 native update Map",
            );
        }
        cases.push(case);
        outputs.push(output(&h, &result));
    }
    let src = source(&cases, "live");
    assert_eq!(src.len(), 3, "TOPIC independent live source floor");
    for (i, (a, b)) in outputs.iter().zip(src.iter()).enumerate() {
        assert_eq!(a, b, "TOPIC source live case {i}");
    }
}
