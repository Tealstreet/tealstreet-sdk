// Per-case expectation tables. Arms that currently coincide are kept separate
// because each names an independent case; collapsing them merges two
// expectations into one, which is the homogenized-corpus false green the
// cardinality audits in this tree exist to prevent.
#![allow(clippy::if_same_then_else)]
use safe_cex_venue_woo::{legacy_metadata::Task, legacy_order_control::*};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type JsString = Vec<u16>;
#[derive(Clone, Debug)]
enum Js {
    U,
    Null,
    B(bool),
    N(f64),
    S(JsString),
    A(Vec<Option<V>>),
    O(Vec<(String, V)>),
}
type V = Rc<RefCell<Js>>;
fn val(j: Js) -> V {
    Rc::new(RefCell::new(j))
}
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn strv(s: &str) -> V {
    val(Js::S(u(s)))
}
fn num(n: f64) -> V {
    val(Js::N(n))
}
fn obj(fields: Vec<(&str, V)>) -> V {
    val(Js::O(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ))
}
fn arr(a: Vec<V>) -> V {
    val(Js::A(a.into_iter().map(Some).collect()))
}
fn field(v: &V, k: &str) -> V {
    match &*v.borrow() {
        Js::O(o) => o
            .iter()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| val(Js::U)),
        _ => val(Js::U),
    }
}
fn set(v: &V, k: &str, new: V) {
    let mut v = v.borrow_mut();
    let Js::O(o) = &mut *v else {
        panic!("native object write")
    };
    if let Some((_, v)) = o.iter_mut().find(|(n, _)| n == k) {
        *v = new;
    } else {
        o.push((k.into(), new));
    }
}
fn truthy(v: &V) -> bool {
    match &*v.borrow() {
        Js::U | Js::Null => false,
        Js::B(b) => *b,
        Js::N(n) => *n != 0.0 && !n.is_nan(),
        Js::S(s) => !s.is_empty(),
        _ => true,
    }
}
fn txt(v: &V) -> String {
    match &*v.borrow() {
        Js::U => "undefined".into(),
        Js::Null => "null".into(),
        Js::B(b) => b.to_string(),
        Js::N(n) => safe_cex_num::js_number_string(*n),
        Js::S(s) => String::from_utf16(s).unwrap(),
        Js::O(_) => "[object Object]".into(),
        Js::A(a) => a
            .iter()
            .map(|v| {
                v.as_ref()
                    .filter(|v| !matches!(*v.borrow(), Js::U | Js::Null))
                    .map(txt)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(","),
    }
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
fn enc(v: &V) -> Value {
    match &*v.borrow() {
        Js::U => json!({"u":true}),
        Js::Null => Value::Null,
        Js::B(b) => json!(b),
        Js::N(n) => json!({"n":bits(*n)}),
        Js::S(s) => json!({"s":s}),
        Js::A(a) => Value::Array(
            a.iter()
                .map(|v| v.as_ref().map(enc).unwrap_or(json!({"hole":true})))
                .collect(),
        ),
        Js::O(o) => Value::Object(o.iter().map(|(k, v)| (k.clone(), enc(v))).collect()),
    }
}
fn error(s: &str) -> V {
    obj(vec![("message", strv(s))])
}
fn ready<T: 'static>(r: Result<T, V>) -> Task<T, V> {
    Box::pin(std::future::ready(r))
}
fn run<T: 'static>(mut f: Task<T, V>) -> Result<T, V> {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected pending"),
    }
}
fn same(a: &V, b: &V) -> bool {
    if Rc::ptr_eq(a, b) {
        return true;
    }
    match (&*a.borrow(), &*b.borrow()) {
        (Js::U, Js::U) | (Js::Null, Js::Null) => true,
        (Js::N(a), Js::N(b)) => a == b || (a.is_nan() && b.is_nan()),
        (Js::S(a), Js::S(b)) => a == b,
        (Js::B(a), Js::B(b)) => a == b,
        _ => false,
    }
}
fn base(id: &str, ty: &str, parent: V) -> V {
    obj(vec![
        ("id", strv(id)),
        ("type", strv(ty)),
        ("parentId", parent),
        ("symbol", strv("BTCUSDT")),
    ])
}
fn sibling() -> V {
    obj(vec![
        ("id", strv("s")),
        ("parentId", strv("p")),
        ("type", strv("stop_market")),
        ("symbol", strv("BTCUSDT")),
        ("side", strv("sell")),
        ("price", num(20.)),
        ("slTriggerType", strv("mark")),
        ("tpTriggerType", strv("last")),
    ])
}
#[derive(Clone, Copy)]
struct Captured {
    op: Operation,
    generation: usize,
}
struct H {
    capture: usize,
    generation: usize,
    captured: Vec<usize>,
    awaits: usize,
    target: Option<V>,
    family: &'static str,
    i: usize,
    trace: Vec<String>,
    requests: Vec<(V, Options, V)>,
    logs: Vec<(bool, V)>,
    updated: Vec<V>,
    placed: Vec<V>,
    mapped: Vec<V>,
    err: V,
    rows: V,
    limits: V,
    algos: V,
    orders: V,
    mode: V,
}
impl Host for H {
    type Value = V;
    type Error = V;
    type Prepared = Captured;
    fn prepare(&mut self, op: Operation) -> Result<Captured, V> {
        if self.capture == 1 && op == Operation::Place {
            self.generation += 1;
            if let Some(target) = &self.target {
                set(target, "type", strv("take_profit_market"));
            }
        }
        if self.capture == 2 && op == Operation::UpdateCanceled {
            if let Some(target) = &self.target {
                set(target, "id", strv("changed"));
            }
        }
        Ok(Captured {
            op,
            generation: self.generation,
        })
    }
    fn await_element(&mut self, v: V) -> Task<V, V> {
        self.awaits += 1;
        if self.capture == 3 {
            self.trace.push("then:get".into());
        }
        ready(Ok(v))
    }
    fn call(&mut self, c: Call) -> Result<Task<V, V>, V> {
        let (label, value, reject) = match c {
            Call::LimitOrders => ("limit", self.limits.clone(), self.i == 1),
            Call::AlgoOrders => ("algo", self.algos.clone(), self.i == 2),
            Call::PositionMode => ("mode", self.mode.clone(), self.i == 5),
        };
        self.trace.push(label.into());
        Ok(ready(if reject {
            Err(self.err.clone())
        } else {
            Ok(value)
        }))
    }
    fn recursive_orders(&mut self, url: &str) -> Result<Task<V, V>, V> {
        self.trace.push("recursive".into());
        self.requests.push((strv(url), Options::None, val(Js::U)));
        if self.i == 4 {
            return Err(self.err.clone());
        }
        Ok(ready(if self.i == 3 {
            Err(self.err.clone())
        } else {
            Ok(self.rows.clone())
        }))
    }
    fn delete(
        &mut self,
        call: Captured,
        url: V,
        options: Options,
        body: V,
    ) -> Result<Task<(), V>, V> {
        assert_eq!(
            call.op,
            Operation::Delete,
            "ORDER native captured delete operation"
        );
        self.trace.push("delete".into());
        self.requests.push((url, options, body));
        if (self.family == "all" || self.family == "symbol") && self.i == 2 {
            return Err(self.err.clone());
        }
        Ok(ready(
            if ((self.family == "all" || self.family == "symbol") && self.i == 1)
                || (self.family == "batch" && self.i == 4)
                || (self.family == "single" && (self.i == 3 || self.i == 4))
            {
                Err(self.err.clone())
            } else {
                Ok(())
            },
        ))
    }
    fn field(&mut self, v: &V, k: &str) -> Result<V, V> {
        if self.capture == 1
            && k == "type"
            && self.target.as_ref().is_some_and(|t| Rc::ptr_eq(t, v))
        {
            self.generation += 1;
        }
        if matches!(*v.borrow(), Js::U | Js::Null) {
            Err(error("field of nullish"))
        } else {
            Ok(field(v, k))
        }
    }
    fn index(&mut self, v: &V, i: usize) -> Result<V, V> {
        match &*v.borrow() {
            Js::A(a) => Ok(a
                .get(i)
                .and_then(Clone::clone)
                .unwrap_or_else(|| val(Js::U))),
            _ => Err(error("index")),
        }
    }
    fn length(&mut self, v: &V) -> Result<usize, V> {
        match &*v.borrow() {
            Js::A(a) => Ok(a.len()),
            _ => Err(error("length")),
        }
    }
    fn has_index(&mut self, v: &V, i: usize) -> Result<bool, V> {
        Ok(match &*v.borrow() {
            Js::A(a) => a.get(i).is_some_and(Option::is_some),
            _ => false,
        })
    }
    fn truthy(&self, v: &V) -> bool {
        truthy(v)
    }
    fn is_null(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::Null)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::Null | Js::U)
    }
    fn omit_undefined(&self, v: V) -> V {
        match &*v.borrow() {
            Js::O(o) => val(Js::O(
                o.iter()
                    .filter(|(_, v)| !matches!(*v.borrow(), Js::U))
                    .cloned()
                    .collect(),
            )),
            _ => v.clone(),
        }
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(*v.borrow(), Js::A(_))
    }
    fn same(&self, a: &V, b: &V) -> bool {
        if matches!(*a.borrow(),Js::N(n) if n.is_nan())
            || matches!(*b.borrow(),Js::N(n) if n.is_nan())
        {
            false
        } else {
            same(a, b)
        }
    }
    fn same_zero(&self, a: &V, b: &V) -> bool {
        same(a, b)
    }
    fn equals(&self, v: &V, s: &str) -> bool {
        matches!(&*v.borrow(),Js::S(v) if *v==u(s))
    }
    fn array(&self, v: Vec<V>) -> V {
        arr(v)
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> V {
        obj(v)
    }
    fn undefined(&self) -> V {
        val(Js::U)
    }
    fn string(&self, s: &str) -> V {
        strv(s)
    }
    fn utf16(&self, s: Vec<u16>) -> V {
        val(Js::S(s))
    }
    fn stringify(&mut self, v: &V) -> Result<Vec<u16>, V> {
        Ok(match &*v.borrow() {
            Js::S(s) => s.clone(),
            _ => u(&txt(v)),
        })
    }
    fn spread(&mut self, v: &V) -> Result<Vec<V>, V> {
        match &*v.borrow() {
            Js::A(a) => Ok(a
                .iter()
                .map(|v| v.clone().unwrap_or_else(|| val(Js::U)))
                .collect()),
            _ => Err(error("not iterable")),
        }
    }
    fn parse_int(&mut self, call: Captured, v: &V) -> Result<V, V> {
        assert_eq!(
            call.op,
            Operation::ParseInt,
            "ORDER native captured parse_int operation"
        );
        Ok(num(safe_cex_num::parse_int(&txt(v), Some(10))))
    }
    fn reverse_symbol(&mut self, call: Captured, v: V) -> Result<V, V> {
        assert_eq!(
            call.op,
            Operation::ReverseSymbol,
            "ORDER native captured reverse_symbol operation"
        );
        Ok(strv(&format!(
            "PERP_{}_USDT",
            txt(&v).strip_suffix("USDT").unwrap_or(&txt(&v))
        )))
    }
    fn map_limit(&mut self, call: Captured, v: V) -> Result<V, V> {
        assert_eq!(
            call.op,
            Operation::MapLimit,
            "ORDER native captured map_limit operation"
        );
        self.trace.push("map".into());
        self.mapped.push(v.clone());
        Ok(if txt(&field(&v, "id")) == "b" {
            val(Js::Null)
        } else {
            v
        })
    }
    fn map_algo(&mut self, call: Captured, v: V) -> Result<V, V> {
        assert_eq!(
            call.op,
            Operation::MapAlgo,
            "ORDER native captured map_algo operation"
        );
        self.trace.push("map".into());
        self.mapped.push(v.clone());
        Ok(if truthy(&v) {
            arr(vec![v, obj(vec![("id", strv("child"))])])
        } else {
            arr(vec![val(Js::U)])
        })
    }
    fn is_algo(&mut self, call: Captured, v: V) -> Result<bool, V> {
        assert_eq!(
            call.op,
            Operation::IsAlgo,
            "ORDER native captured is_algo operation"
        );
        self.trace.push(format!("isAlgo:{}", txt(&v)));
        Ok(txt(&v) != "limit")
    }
    fn stored_orders(&mut self) -> Result<V, V> {
        Ok(self.orders.clone())
    }
    fn place_positional(&mut self, call: Captured, v: V) -> Result<Task<(), V>, V> {
        assert_eq!(
            call.op,
            Operation::Place,
            "ORDER native captured place_positional operation"
        );
        self.trace.push("place".into());
        self.captured.push(call.generation);
        self.placed.push(v);
        Ok(ready(if self.i == 6 {
            Err(self.err.clone())
        } else {
            Ok(())
        }))
    }
    fn update_canceled(&mut self, call: Captured, id: V) -> Result<(), V> {
        assert_eq!(
            call.op,
            Operation::UpdateCanceled,
            "ORDER native captured update_canceled operation"
        );
        self.trace.push("update".into());
        self.captured.push(call.generation);
        self.updated.push(id);
        Ok(())
    }
    fn type_error(&mut self, m: &str) -> V {
        error(m)
    }
    fn error_field(&mut self, e: &V, p: &[&str]) -> Result<V, V> {
        let mut v = e.clone();
        for k in p {
            v = field(&v, k);
        }
        Ok(v)
    }
    fn includes(&mut self, v: &V, s: &str) -> Result<bool, V> {
        if matches!(*v.borrow(), Js::S(_)) {
            Ok(txt(v).contains(s))
        } else {
            Err(error("includes is not a function"))
        }
    }
    fn emit(&mut self, call: Captured, e: Option<&V>, m: V) -> Result<(), V> {
        assert_eq!(
            call.op,
            Operation::Emit,
            "ORDER native captured emit operation"
        );
        self.trace.push("emit".into());
        self.logs
            .push((e.is_some_and(|v| Rc::ptr_eq(v, &self.err)), m));
        Ok(())
    }
}
fn setup(family: &'static str, i: usize) -> (Rc<RefCell<H>>, V, V) {
    let err = error("top failure");
    if i == 2 || i == 4 {
        set(
            &err,
            "response",
            obj(vec![(
                "data",
                obj(vec![(
                    "message",
                    strv(if i == 4 {
                        "already canceled"
                    } else {
                        "nested failure"
                    }),
                )]),
            )]),
        );
    }
    let limits = arr(vec![obj(vec![("id", strv("L"))])]);
    let algos = arr(vec![obj(vec![("id", strv("A"))])]);
    let mut rows = arr(vec![
        obj(vec![("id", strv("a"))]),
        obj(vec![("id", strv("b"))]),
    ]);
    if i == 1 {
        if let Js::A(a) = &mut *rows.borrow_mut() {
            a[0] = None;
        }
    }
    if i == 2 {
        rows = val(Js::Null);
    }
    let mut batch = arr(vec![
        base("12x", "limit", val(Js::U)),
        base("a", "stop_market", strv("p")),
        base("b", "take_profit_market", strv("p")),
        base("13", "limit", val(Js::U)),
    ]);
    if i == 1 {
        batch = arr(vec![
            base("a", "stop_market", val(Js::U)),
            base("b", "stop_market", val(Js::U)),
        ]);
    }
    if i == 2 {
        batch = arr(vec![
            base("1", "stop_market", num(0.)),
            base("2", "stop_market", num(0.)),
        ]);
    }
    if i == 3 {
        batch = val(Js::A(vec![Some(base("NaN", "limit", val(Js::U))), None]));
    }
    if i == 5 {
        batch = arr(vec![]);
    }
    if i == 7 {
        batch = arr(vec![base("utf16", "stop_market", val(Js::S(vec![0xd800])))])
    };
    if i == 6 {
        batch = arr(vec![
            base("16", "limit", strv("q")),
            base("17", "limit", strv("q")),
        ]);
    }
    let order = base("o", "stop_market", strv("p"));
    let sibling = sibling();
    if i == 2 {
        set(&sibling, "type", strv("take_profit_market"));
    }
    let orders = if i == 1 {
        arr(vec![order.clone()])
    } else {
        arr(vec![order.clone(), sibling])
    };
    let opts = obj(vec![
        ("side", strv(if i == 1 || i == 3 { "sell" } else { "buy" })),
        ("reduceOnly", val(Js::B(i == 2 || i == 3 || i == 5))),
    ]);
    let mode = strv(if i == 4 { "oneway" } else { "hedged" });
    let (input, arg) = match family {
        "combined" => (
            obj(vec![("limits", limits.clone()), ("algos", algos.clone())]),
            val(Js::U),
        ),
        "limit" | "algo" => (obj(vec![("rows", rows.clone())]), val(Js::U)),
        "batch" => (obj(vec![("orders", batch.clone())]), batch),
        "single" => (
            obj(vec![
                ("order", order.clone()),
                ("orders", orders.clone()),
                ("recreate", val(Js::B(i != 5))),
            ]),
            order,
        ),
        "side" => (
            obj(vec![("opts", opts.clone()), ("mode", mode.clone())]),
            opts,
        ),
        "all" => (obj(vec![]), val(Js::U)),
        "symbol" => (obj(vec![("symbol", strv("BTCUSDT"))]), strv("BTCUSDT")),
        _ => unreachable!(),
    };
    (
        Rc::new(RefCell::new(H {
            capture: 0,
            generation: 0,
            captured: vec![],
            awaits: 0,
            target: None,
            family,
            i,
            trace: vec![],
            requests: vec![],
            logs: vec![],
            updated: vec![],
            placed: vec![],
            mapped: vec![],
            err,
            rows,
            limits,
            algos,
            orders,
            mode,
        })),
        input,
        arg,
    )
}
fn native_string(v: &V, s: &str, label: &str) {
    assert!(
        matches!(&*v.borrow(),Js::S(v) if *v==u(s)),
        "ORDER native {label}: expected {s}, got {v:?}"
    );
}
fn native_len(v: &V, n: usize, label: &str) {
    assert!(
        matches!(&*v.borrow(),Js::A(v) if v.len()==n),
        "ORDER native {label} size {n}: {v:?}"
    );
}
fn get_index(v: &V, i: usize) -> V {
    match &*v.borrow() {
        Js::A(a) => a[i].clone().unwrap_or_else(|| val(Js::U)),
        _ => panic!("native array"),
    }
}
fn native(family: &str, i: usize, h: &H, arg: &V, result: &Result<V, V>, before: &[String]) {
    let failed = family == "combined" && (i == 1 || i == 2)
        || family == "single" && (i == 3 || i == 6)
        || family == "side" && i == 5
        || family == "batch" && i == 4;
    assert_eq!(
        result.is_err(),
        failed,
        "ORDER native {family} outcome iteration {i}"
    );
    if let Err(e) = result {
        assert!(
            Rc::ptr_eq(e, &h.err),
            "ORDER native original error {family} iteration {i}"
        );
    }
    let expected_before: Vec<&str> = match family {
        "combined" => vec!["limit"],
        "limit" | "algo" => {
            if i == 4 {
                vec!["recursive", "emit"]
            } else {
                vec!["recursive"]
            }
        }
        "batch" => vec![],
        "single" => vec!["delete"],
        "side" => vec!["mode"],
        "all" | "symbol" => {
            if i == 2 {
                vec!["delete", "emit"]
            } else {
                vec!["delete"]
            }
        }
        _ => unreachable!(),
    };
    assert_eq!(
        before, expected_before,
        "ORDER native {family} eager sequence iteration {i}"
    );
    let expected_trace: Vec<&str> = match family {
        "combined" => {
            if i == 1 {
                vec!["limit"]
            } else {
                vec!["limit", "algo"]
            }
        }
        "limit" | "algo" => match i {
            0 | 1 => vec!["recursive", "map", "map"],
            _ => vec!["recursive", "emit"],
        },
        "batch" => match i {
            0 => vec![
                "isAlgo:limit",
                "delete",
                "isAlgo:stop_market",
                "delete",
                "isAlgo:limit",
                "delete",
            ],
            1 | 2 => vec![
                "isAlgo:stop_market",
                "delete",
                "isAlgo:stop_market",
                "delete",
            ],
            3 | 6 => vec!["isAlgo:limit", "delete"],
            4 => vec!["isAlgo:limit", "delete", "emit"],
            5 => vec![],
            7 => vec!["isAlgo:stop_market", "delete"],
            _ => unreachable!(),
        },
        "single" => match i {
            0 | 2 => vec!["delete", "place"],
            1 | 5 => vec!["delete"],
            3 => vec!["delete", "emit"],
            4 => vec!["delete", "update"],
            6 => vec!["delete", "place", "emit"],
            _ => unreachable!(),
        },
        "side" => vec!["mode"],
        "all" | "symbol" => {
            if i == 0 {
                vec!["delete"]
            } else {
                vec!["delete", "emit"]
            }
        }
        _ => unreachable!(),
    };
    assert_eq!(
        h.trace, expected_trace,
        "ORDER native {family} sequence iteration {i}"
    );
    let log = matches!(family, "limit" | "algo") && i >= 2
        || family == "batch" && i == 4
        || family == "single" && (i == 3 || i == 6)
        || (family == "all" || family == "symbol") && i > 0;
    assert_eq!(
        h.logs.len(),
        usize::from(log),
        "ORDER native {family} logs iteration {i}"
    );
    if log {
        assert_eq!(
            h.logs[0].0,
            family == "batch",
            "ORDER native error argument {family}"
        );
        native_string(
            &h.logs[0].1,
            if matches!(family, "limit" | "algo") && i == 2 {
                "rows.reduce is not a function"
            } else if i == 2 {
                "nested failure"
            } else if i == 4 {
                "already canceled"
            } else {
                "top failure"
            },
            "error message",
        );
    }
    if family == "combined" && i == 0 {
        let v = result.as_ref().unwrap();
        native_len(v, 2, "combined orders");
        assert!(
            Rc::ptr_eq(&get_index(v, 0), &get_index(&h.limits, 0))
                && Rc::ptr_eq(&get_index(v, 1), &get_index(&h.algos, 0)),
            "ORDER native combined aliases"
        );
    }
    if matches!(family, "limit" | "algo") {
        let v = result.as_ref().unwrap();
        native_len(
            v,
            if i >= 2 {
                0
            } else if family == "limit" {
                if i == 0 { 1 } else { 1 }
            } else {
                if i == 0 { 4 } else { 3 }
            },
            "mapped orders",
        );
        if i == 0 {
            assert!(
                Rc::ptr_eq(&get_index(v, 0), &get_index(&h.rows, 0)),
                "ORDER native mapper row alias"
            );
        }
        if family == "limit" && i == 1 {
            assert!(
                matches!(*get_index(v, 0).borrow(), Js::U),
                "ORDER native null-only filter"
            );
        }
        if family == "algo" && i == 0 {
            assert!(
                Rc::ptr_eq(&get_index(v, 2), &get_index(&h.rows, 1)),
                "ORDER native child row alias"
            );
            native_string(&field(&get_index(v, 1), "id"), "child", "child value");
        }
    }
    if family == "side" && i < 5 {
        let v = result.as_ref().unwrap();
        if i == 4 {
            assert!(
                matches!(*v.borrow(), Js::U),
                "ORDER native one-way undefined"
            );
        } else {
            native_string(
                v,
                if i == 0 || i == 3 { "LONG" } else { "SHORT" },
                "position side",
            );
        }
    }
    let expected_n = match family {
        "combined" | "side" => 0,
        "limit" | "algo" => 1,
        "batch" => match i {
            0 => 3,
            1 | 2 => 2,
            5 => 0,
            _ => 1,
        },
        _ => 1,
    };
    assert_eq!(
        h.requests.len(),
        expected_n,
        "ORDER native {family} request count iteration {i}"
    );
    if family == "limit" || family == "algo" {
        native_string(
            &h.requests[0].0,
            if family == "limit" {
                "/v1/orders"
            } else {
                "/v3/algo/orders"
            },
            "read route",
        );
    }
    if family == "all" {
        native_string(&h.requests[0].0, "/v3/orders/pending", "cancel all route");
        assert_eq!(h.requests[0].1, Options::None, "ORDER native all options");
    }
    if family == "symbol" {
        native_string(&h.requests[0].0, "/v1/orders", "cancel symbol route");
        assert_eq!(
            h.requests[0].1,
            Options::Params,
            "ORDER native symbol options"
        );
        native_string(
            &field(&h.requests[0].2, "symbol"),
            "PERP_BTC_USDT",
            "symbol",
        );
    }
    if family == "batch" && i != 5 {
        for (j, (url, options, body)) in h.requests.iter().enumerate() {
            let algo = match i {
                0 => j == 1,
                1 | 2 | 7 => true,
                _ => false,
            };
            if algo {
                if i == 7 {
                    let mut expected = u("/v3/algo/order/");
                    expected.push(0xd800);
                    assert!(
                        matches!(&*url.borrow(),Js::S(s) if *s==expected),
                        "ORDER native surrogate URL"
                    );
                } else {
                    native_string(
                        url,
                        if i == 0 {
                            "/v3/algo/order/p"
                        } else if i == 1 {
                            "/v3/algo/order/undefined"
                        } else {
                            "/v3/algo/order/0"
                        },
                        "algo cancel URL",
                    );
                }
                assert_eq!(*options, Options::None, "ORDER native algo options");
            } else {
                native_string(url, "/v1/order", "limit cancel URL");
                assert_eq!(*options, Options::Data, "ORDER native limit options");
                native_string(&field(body, "symbol"), "PERP_BTC_USDT", "limit symbol");
                let id = field(body, "order_id");
                let expected = match i {
                    0 => {
                        if j == 0 {
                            12.
                        } else {
                            13.
                        }
                    }
                    3 => f64::NAN,
                    6 => 16.,
                    _ => 12.,
                };
                assert!(
                    matches!(*id.borrow(),Js::N(n) if n.to_bits()==expected.to_bits()||n.is_nan()&&expected.is_nan()),
                    "ORDER native parsed id iteration {i}"
                );
            }
        }
    }
    if family == "single" {
        native_string(&h.requests[0].0, "/v3/algo/order/p", "single URL");
        let placed = matches!(i, 0 | 2 | 6);
        assert_eq!(
            h.placed.len(),
            usize::from(placed),
            "ORDER native recreate count iteration {i}"
        );
        if placed {
            let opts = &h.placed[0];
            let (price, trigger) = if i == 2 {
                ("takeProfit", "tpTriggerType")
            } else {
                ("stopLoss", "slTriggerType")
            };
            let p = field(opts, price);
            assert!(
                matches!(*p.borrow(),Js::N(n) if n==20.),
                "ORDER native sibling price"
            );
            native_string(
                &field(opts, trigger),
                if i == 2 { "last" } else { "mark" },
                "sibling trigger",
            );
            native_string(&field(opts, "symbol"), "BTCUSDT", "sibling symbol");
            native_string(&field(opts, "side"), "sell", "sibling side");
        }
        assert_eq!(
            h.updated.len(),
            usize::from(i == 4),
            "ORDER native already-canceled status iteration {i}"
        );
        if i == 4 {
            let id = field(arg, "id");
            assert!(
                Rc::ptr_eq(&h.updated[0], &id),
                "ORDER native update id alias"
            );
        }
    }
}
fn corpus() -> &'static Vec<Value> {
    static CORPUS: std::sync::OnceLock<Vec<Value>> = std::sync::OnceLock::new();
    CORPUS.get_or_init(|| {
        let out = std::process::Command::new("node")
            .arg(format!(
                "{}/tests/legacy_order_control_oracle.cjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .output()
            .expect("source oracle");
        assert!(
            out.status.success(),
            "source oracle failure: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(rows.len(), 40, "ORDER corpus SIZE");
        let expectations = [
            ("combined", 3),
            ("limit", 5),
            ("algo", 5),
            ("batch", 8),
            ("single", 7),
            ("side", 6),
            ("all", 3),
            ("symbol", 3),
        ];
        let mut n = 0;
        for (family, count) in expectations {
            for i in 0..count {
                assert_eq!(
                    rows[n]["family"], family,
                    "ORDER corpus CONTENT iteration {n}"
                );
                assert_eq!(rows[n]["i"], i, "ORDER corpus CONTENT index iteration {n}");
                n += 1;
            }
        }
        rows
    })
}
fn family(family: &'static str, count: usize) {
    let source = corpus();
    let mut actual = vec![];
    for i in 0..count {
        let (h, input, arg) = setup(family, i);
        let input = enc(&input);
        let task: Task<V, V> = match family {
            "combined" => fetch_orders(h.clone()),
            "limit" => fetch_limit_orders(h.clone()),
            "algo" => fetch_algo_orders(h.clone()),
            "side" => get_position_side_for_order(h.clone(), arg.clone()),
            _ => {
                let task = match family {
                    "batch" => cancel_orders(h.clone(), arg.clone()),
                    "single" => cancel_algo_order(h.clone(), arg.clone(), i != 5),
                    "all" => cancel_all_orders(h.clone()),
                    "symbol" => cancel_symbol_orders(h.clone(), arg.clone()),
                    _ => unreachable!(),
                };
                Box::pin(async move { task.await.map(|()| val(Js::U)) })
            }
        };
        let before = h.borrow().trace.clone();
        let result = run(task);
        let h = h.borrow();
        native(family, i, &h, &arg, &result, &before);
        let identity = if family == "combined" && i == 0 {
            true
        } else {
            (family == "limit" || family == "algo") && i == 0
        };
        let result = match result {
            Ok(v) => json!({"ok":enc(&v)}),
            Err(e) => json!({"error":enc(&field(&e,"message")),"errorAlias":Rc::ptr_eq(&e,&h.err)}),
        };
        let requests = arr(h
            .requests
            .iter()
            .map(|(url, opts, body)| {
                arr(vec![
                    url.clone(),
                    strv(if family == "limit" || family == "algo" {
                        "recursive"
                    } else {
                        match opts {
                            Options::None => "none",
                            Options::Params => "params",
                            Options::Data => "data",
                        }
                    }),
                    body.clone(),
                ])
            })
            .collect());
        let logs = arr(h
            .logs
            .iter()
            .map(|(full, message)| {
                obj(vec![
                    ("full", val(Js::B(*full))),
                    ("message", message.clone()),
                ])
            })
            .collect());
        let updated = arr(h
            .updated
            .iter()
            .map(|id| arr(vec![id.clone(), strv("canceled")]))
            .collect());
        actual.push(json!({"family":family,"i":i,"input":input,"result":result,"before":before,"trace":h.trace,"requests":enc(&requests),"logs":enc(&logs),"updated":enc(&updated),"placed":enc(&arr(h.placed.clone())),"mapped":enc(&arr(h.mapped.clone())),"identity":identity}));
    }
    for (i, actual) in actual.iter().enumerate() {
        let expected = source
            .iter()
            .find(|r| r["family"] == family && r["i"] == i)
            .unwrap();
        assert_eq!(actual, expected, "ORDER source wire {family} iteration {i}");
    }
}
#[test]
fn combined_native() {
    family("combined", 3)
}
#[test]
fn limit_native() {
    family("limit", 5)
}
#[test]
fn algo_native() {
    family("algo", 5)
}
#[test]
fn batch_native() {
    family("batch", 8)
}
#[test]
fn single_native() {
    family("single", 7)
}
#[test]
fn side_native() {
    family("side", 6)
}
#[test]
fn all_native() {
    family("all", 3)
}
#[test]
fn symbol_native() {
    family("symbol", 3)
}
#[test]
fn captured_callbacks_native() {
    let mut wire = vec![];
    let (h, _input, arg) = setup("single", 0);
    {
        let mut h = h.borrow_mut();
        h.capture = 1;
        h.target = Some(get_index(&h.orders, 1));
    }
    run(cancel_algo_order(h.clone(), arg, true)).unwrap();
    {
        let h = h.borrow();
        assert_eq!(h.captured, vec![3], "ORDER native place capture generation");
        let opts = &h.placed[0];
        assert!(
            matches!(*field(opts,"stopLoss").borrow(),Js::N(n) if n==20.),
            "ORDER native keys before place capture"
        );
        assert!(
            matches!(*field(opts, "takeProfit").borrow(), Js::U),
            "ORDER native no replaced price key"
        );
        wire.push(enc(&obj(vec![
            ("family", strv("place")),
            ("generation", num(3.)),
            ("opts", opts.clone()),
        ])));
    }
    let (h, _input, arg) = setup("single", 4);
    {
        let mut h = h.borrow_mut();
        h.capture = 2;
        h.target = Some(arg.clone());
    }
    run(cancel_algo_order(h.clone(), arg, true)).unwrap();
    {
        let h = h.borrow();
        native_string(&h.updated[0], "changed", "captured status id");
        wire.push(enc(&obj(vec![
            ("family", strv("update")),
            ("id", h.updated[0].clone()),
        ])));
    }
    let (h, _input, _arg) = setup("batch", 0);
    let arg = arr(vec![base("o", "stop_market", strv("p"))]);
    h.borrow_mut().capture = 3;
    let task = cancel_orders(h.clone(), arg);
    let before = h.borrow().trace.clone();
    assert_eq!(before, vec!["then:get"], "ORDER native eager await element");
    run(task).unwrap();
    {
        let h = h.borrow();
        assert_eq!(
            h.trace,
            vec!["then:get", "isAlgo:stop_market", "delete"],
            "ORDER native await before callback"
        );
        wire.push(enc(&obj(vec![
            ("family", strv("await")),
            ("before", arr(before.iter().map(|s| strv(s)).collect())),
            ("trace", arr(h.trace.iter().map(|s| strv(s)).collect())),
        ])));
    }
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/tests/legacy_order_control_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .arg("--captures")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "capture source failed {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(rows.len(), 3, "ORDER capture corpus SIZE");
    for (i, family) in ["place", "update", "await"].iter().enumerate() {
        assert_eq!(
            rows[i]["family"],
            enc(&strv(family)),
            "ORDER capture corpus CONTENT {i}"
        );
    }
    assert_eq!(wire, rows, "ORDER capture source wire");
}
