use safe_cex_venue_orderly::{
    account_order_reads::AccountReadHost,
    data_mappers::{DataHost, Op},
    runtime_actions::{self as runtime, LeverageCatch, RuntimeHost, TickCallback},
    ws_controls::WsHost,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
use std::{
    future::Future,
    pin::Pin,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
enum Raw {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Rc<Raw>>>),
    Object(RefCell<Vec<(Vec<u16>, Rc<Raw>)>>),
    Function(&'static str),
    Promise(Promise),
}
type V = Rc<Raw>;
type E = V;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    Rc::new(Raw::Text(units(s)))
}
fn array(a: Vec<V>) -> V {
    Rc::new(Raw::Array(RefCell::new(a)))
}
fn object(a: Vec<(&str, V)>) -> V {
    Rc::new(Raw::Object(RefCell::new(
        a.into_iter().map(|(k, v)| (units(k), v)).collect(),
    )))
}
fn decode(v: &Value) -> V {
    Rc::new(match v {
        Value::Null => Raw::Null,
        Value::Bool(b) => Raw::Bool(*b),
        Value::Number(n) => Raw::Number(n.as_f64().unwrap()),
        Value::String(s) => Raw::Text(units(s)),
        Value::Array(a) => Raw::Array(RefCell::new(a.iter().map(decode).collect())),
        Value::Object(o) => {
            if o.get("undefined") == Some(&Value::Bool(true)) {
                Raw::Undefined
            } else if let Some(n) = o.get("number") {
                Raw::Number(f64::from_bits(
                    u64::from_str_radix(n.as_str().unwrap(), 16).unwrap(),
                ))
            } else {
                Raw::Object(RefCell::new(
                    o.iter().map(|(k, v)| (units(k), decode(v))).collect(),
                ))
            }
        }
    })
}
fn get(v: &Raw, key: &str) -> V {
    if let Raw::Object(o) = v {
        o.borrow()
            .iter()
            .find(|(k, _)| *k == units(key))
            .map(|(_, v)| v.clone())
            .unwrap_or(Rc::new(Raw::Undefined))
    } else {
        Rc::new(Raw::Undefined)
    }
}
fn set(v: &V, key: &str, next: V) {
    let Raw::Object(o) = &**v else {
        panic!("ordinary fixture object")
    };
    let mut o = o.borrow_mut();
    if let Some((_, value)) = o.iter_mut().find(|(k, _)| *k == units(key)) {
        *value = next
    } else {
        o.push((units(key), next))
    }
}
fn boolean(b: bool) -> V {
    Rc::new(Raw::Bool(b))
}
fn flag(v: &Raw, key: &str) -> bool {
    matches!(&*get(v, key), Raw::Bool(true))
}
fn items(v: &Raw) -> Vec<V> {
    let Raw::Array(a) = v else {
        panic!("fixture array")
    };
    a.borrow().clone()
}
fn string(v: &Raw) -> Vec<u16> {
    match v {
        Raw::Undefined => units("undefined"),
        Raw::Null => units("null"),
        Raw::Bool(b) => units(if *b { "true" } else { "false" }),
        Raw::Number(n) => units(&safe_cex_num::js_number_string(*n)),
        Raw::Text(s) => s.clone(),
        Raw::Object(_) => units("[object Object]"),
        Raw::Array(a) => a
            .borrow()
            .iter()
            .map(|v| String::from_utf16(&string(v)).unwrap())
            .collect::<Vec<_>>()
            .join(",")
            .encode_utf16()
            .collect(),
        Raw::Function(_) | Raw::Promise(_) => panic!("outside fixture coercion floor"),
    }
}
fn ordered(v: &Raw) -> Vec<(Vec<u16>, V)> {
    let Raw::Object(o) = v else {
        panic!("fixture own entries")
    };
    let mut a = o.borrow().clone();
    a.sort_by_key(|(k, _)| {
        let s = String::from_utf16(k).unwrap();
        let n = s
            .parse::<u32>()
            .ok()
            .filter(|n| *n != u32::MAX && n.to_string() == s);
        (n.is_none(), n.unwrap_or(0))
    });
    a
}
fn native(v: &Raw, expected: &Value, path: &str) {
    match v {
        Raw::Undefined => assert_eq!(
            expected.get("undefined"),
            Some(&Value::Bool(true)),
            "native undefined {path}"
        ),
        Raw::Null => assert!(expected.is_null(), "native null {path}"),
        Raw::Bool(b) => assert_eq!(expected.as_bool(), Some(*b), "native boolean {path}"),
        Raw::Number(n) => {
            let bits = u64::from_str_radix(expected["number"].as_str().unwrap(), 16).unwrap();
            if n.is_nan() {
                assert!(f64::from_bits(bits).is_nan(), "native NaN {path}")
            } else {
                assert_eq!(n.to_bits(), bits, "native bits {path}")
            }
        }
        Raw::Text(s) => {
            assert!(expected.is_string(), "native text kind {path}");
            assert_eq!(
                s,
                &units(expected.as_str().unwrap()),
                "native UTF-16 {path}"
            );
        }
        Raw::Array(a) => {
            assert!(expected.is_array(), "native array kind {path}");
            let a = a.borrow();
            let e = expected.as_array().unwrap();
            assert_eq!(a.len(), e.len(), "native array cardinality {path}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                native(a, e, &format!("{path}[{i}]"))
            }
        }
        Raw::Object(o) => {
            assert!(expected.is_object(), "native object kind {path}");
            let o = o.borrow();
            let e = expected.as_object().unwrap();
            assert_eq!(o.len(), e.len(), "native object cardinality {path}");
            for (k, v) in o.iter() {
                let k = String::from_utf16(k).unwrap();
                assert!(e.contains_key(&k), "native object key {path}.{k}");
                native(v, &e[&k], &format!("{path}.{k}"))
            }
        }
        Raw::Function(_) | Raw::Promise(_) => panic!("no callback encoding assertion"),
    }
}
fn snapshot(value: &V) -> V {
    Rc::new(match &**value {
        Raw::Undefined => Raw::Undefined,
        Raw::Null => Raw::Null,
        Raw::Bool(b) => Raw::Bool(*b),
        Raw::Number(n) => Raw::Number(*n),
        Raw::Text(s) => Raw::Text(s.clone()),
        Raw::Function(f) => Raw::Function(f),
        Raw::Promise(_) => panic!("no promise trace snapshot"),
        Raw::Array(a) => Raw::Array(RefCell::new(a.borrow().iter().map(snapshot).collect())),
        Raw::Object(o) => Raw::Object(RefCell::new(
            o.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), snapshot(v)))
                .collect(),
        )),
    })
}
#[derive(Clone, Copy, Debug)]
enum Promise {
    Method(&'static str),
    Request(&'static str),
}
#[derive(Clone)]
struct Host(Rc<State>);
impl Deref for Host {
    type Target = State;
    fn deref(&self) -> &State {
        &self.0
    }
}
struct State {
    cfg: V,
    receiver: V,
    store: V,
    xhr: V,
    alternate: V,
    public: V,
    private: V,
    emitter: V,
    original: E,
    events: RefCell<Vec<V>>,
    index: Cell<usize>,
    background: RefCell<Vec<LeverageCatch<Host>>>,
    scheduled: RefCell<Vec<TickCallback<Host>>>,
    background_promise: RefCell<Option<V>>,
    detached: RefCell<Vec<E>>,
}
impl Host {
    fn effect(&self, kind: &str, fields: Vec<(&str, V)>) -> Result<(), E> {
        self.events.borrow_mut().push(object(
            vec![("kind", text(kind))]
                .into_iter()
                .chain(fields.into_iter().map(|(k, v)| (k, snapshot(&v))))
                .collect(),
        ));
        if string(&get(&self.cfg, "fail")) == units(kind) {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn num(&self, v: &Raw) -> f64 {
        match v {
            Raw::Number(n) => *n,
            Raw::Null => 0.0,
            Raw::Bool(b) => u8::from(*b) as f64,
            Raw::Text(t) => safe_cex_num::number_from_string(&String::from_utf16(t).unwrap()),
            _ => f64::NAN,
        }
    }
    fn run<'a>(&'a self, name: &str) -> Pin<Box<dyn Future<Output = Result<V, E>> + 'a>> {
        let r = self.receiver.clone();
        match name {
            "isTransientOrderlyValidationError" => {
                Box::pin(
                    async move { Ok(boolean(runtime::transient(self, self.original.clone())?)) },
                )
            }
            "dispose" => Box::pin(async move { runtime::dispose(self, r) }),
            "start" => Box::pin(runtime::start(self, r)),
            "tick" => Box::pin(runtime::tick(self, r, get(&self.cfg, "count"))),
            "updateOrder" => Box::pin(runtime::update_order(
                self,
                r,
                object(vec![
                    ("order", get(&self.cfg, "submitted")),
                    ("update", get(&self.cfg, "update")),
                ]),
            )),
            "placeOrder" => Box::pin(runtime::place_order(self, r, get(&self.cfg, "opts"))),
            "placeAlgoOrder" => Box::pin(runtime::place_algo(self, r, get(&self.cfg, "opts"))),
            "placePositionalAlgoOrder" => {
                Box::pin(runtime::place_positional(self, r, get(&self.cfg, "opts")))
            }
            _ => panic!("runtime row {name}"),
        }
    }
    fn change(&self, label: &str) {
        let c = get(&self.cfg, "afterAwait");
        if flag(&c, "xhr") {
            set(&self.receiver, "xhr", self.alternate.clone())
        }
        if self.truthy(&get(&c, "tick")) {
            set(
                &self.receiver,
                "tick",
                Rc::new(Raw::Function("alternateTick")),
            )
        }
        let side = get(&c, "optsSide");
        if self.truthy(&side) {
            set(&get(&self.cfg, "opts"), "side", side)
        }
        if self.text_eq(&get(&self.cfg, "disposeAfter"), label) {
            set(&self.receiver, "isDisposed", boolean(true))
        }
    }
}
impl WsHost for Host {
    type Value = Raw;
    type Error = E;
    type Iterator = (V, usize);
    fn get(&self, v: &V, k: &[u16]) -> Result<V, E> {
        let key = String::from_utf16(k).unwrap();
        if flag(&self.cfg, "throwAmount")
            && key == "amount"
            && Rc::ptr_eq(v, &get(&get(&self.cfg, "market"), "precision"))
        {
            return Err(self.original.clone());
        }
        if key == "length" {
            if let Raw::Array(a) = &**v {
                return Ok(self.number(a.borrow().len() as f64));
            }
        }
        Ok(get(v, &key))
    }
    fn set(&self, v: &V, k: &[u16], next: V) -> Result<(), E> {
        set(v, &String::from_utf16(k).unwrap(), next);
        Ok(())
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v {
            Raw::Undefined | Raw::Null => false,
            Raw::Bool(b) => *b,
            Raw::Number(n) => *n != 0.0 && !n.is_nan(),
            Raw::Text(t) => !t.is_empty(),
            _ => true,
        }
    }
    fn string(&self, v: &Raw) -> Result<Vec<u16>, E> {
        Ok(string(v))
    }
    fn text(&self, s: &[u16]) -> V {
        Rc::new(Raw::Text(s.to_vec()))
    }
    fn object(&self, e: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(e))
    }
    fn stringify(&self, _: V) -> Result<V, E> {
        panic!("outside runtime floor")
    }
    fn parse(&self, _: V) -> Result<V, E> {
        panic!("outside runtime floor")
    }
    fn includes(&self, _: &V, _: &[u16]) -> Result<bool, E> {
        panic!("outside runtime floor")
    }
    fn own_values(&self, _: V) -> Result<Vec<V>, E> {
        panic!("outside runtime floor")
    }
    fn flatten_one(&self, _: Vec<V>) -> Result<Vec<V>, E> {
        panic!("outside runtime floor")
    }
    fn entries(&self, v: V) -> Result<Vec<(Vec<u16>, V)>, E> {
        Ok(ordered(&v))
    }
    fn iterator(&self, v: V) -> Result<Self::Iterator, E> {
        Ok((v, 0))
    }
    fn next(&self, i: &mut Self::Iterator) -> Result<Option<V>, E> {
        let a = items(&i.0);
        let v = a.get(i.1).cloned();
        i.1 += 1;
        assert!(i.1 <= 16, "finite iterator floor");
        Ok(v)
    }
    fn call(&self, f: V, receiver: Option<V>, args: Vec<V>) -> Result<V, E> {
        let Raw::Function(name) = &*f else {
            panic!("runtime callable")
        };
        let is = |v: &V| receiver.as_ref().is_some_and(|r| Rc::ptr_eq(r, v));
        let error = || Err(self.original.clone());
        match *name {
            "get" | "put" | "post" => {
                let index = self.index.get();
                self.index.set(index + 1);
                let label = if is(&self.xhr) {
                    "original"
                } else {
                    assert!(is(&self.alternate));
                    "alternate"
                };
                self.effect(
                    "request",
                    vec![
                        ("method", text(name)),
                        ("url", args[0].clone()),
                        ("payload", args.get(1).cloned().unwrap_or(self.undefined())),
                        ("index", self.number(index as f64)),
                        ("receiver", boolean(true)),
                        ("label", text(label)),
                    ],
                )?;
                if self.text_eq(&get(&self.cfg, "syncMethod"), name) {
                    return error();
                }
                return Ok(Rc::new(Raw::Promise(Promise::Request(name))));
            }
            "isAlgoOrder" => {
                self.effect(
                    "isAlgo",
                    vec![
                        ("receiver", boolean(is(&self.receiver))),
                        ("type", args[0].clone()),
                    ],
                )?;
                return Ok(boolean(
                    ["stop_market", "take_profit_market", "trailing_stop_market"]
                        .iter()
                        .any(|s| self.text_eq(&args[0], s)),
                ));
            }
            "toOrderlyMarginMode" => {
                self.effect("marginMode", vec![("margin", args[0].clone())])?;
                return Ok(text(if self.text_eq(&args[0], "isolated") {
                    "ISOLATED"
                } else {
                    "CROSS"
                }));
            }
            "mapMarginType" => {
                self.effect("mapMargin", vec![("raw", args[0].clone())])?;
                return Ok(text(
                    if self.is_string(&args[0])
                        && String::from_utf16(&string(&args[0]))
                            .unwrap()
                            .to_uppercase()
                            == "ISOLATED"
                    {
                        "isolated"
                    } else {
                        "cross"
                    },
                ));
            }
            "assertSingleNativeClientOrderId" => {
                self.effect(
                    "single",
                    vec![
                        ("receiver", boolean(is(&self.receiver))),
                        ("client", args[0].clone()),
                        ("count", args[1].clone()),
                    ],
                )?;
            }
            "emitAndLogError" => {
                let original = args.first().is_some_and(|v| Rc::ptr_eq(v, &self.original));
                self.effect(
                    "log",
                    vec![
                        ("receiver", boolean(is(&self.receiver))),
                        ("args", array(args)),
                        ("originalError", boolean(original)),
                    ],
                )?;
            }
            "log" => {
                self.effect(
                    "info",
                    vec![
                        ("receiver", boolean(is(&self.receiver))),
                        ("message", args[0].clone()),
                    ],
                )?;
            }
            "update" => {
                self.effect(
                    "update",
                    vec![
                        ("receiver", boolean(is(&self.store))),
                        ("patch", args[0].clone()),
                    ],
                )?;
                for (k, v) in ordered(&args[0]) {
                    set(&self.store, &String::from_utf16(&k).unwrap(), v)
                }
            }
            "publicDispose" | "privateDispose" | "publicConnect" | "privateConnect" => {
                self.effect(
                    name,
                    vec![(
                        "receiver",
                        boolean(if name.starts_with("public") {
                            is(&self.public)
                        } else {
                            is(&self.private)
                        }),
                    )],
                )?;
            }
            "emit" => {
                self.effect(
                    "emit",
                    vec![
                        ("receiver", boolean(is(&self.emitter))),
                        ("event", args[0].clone()),
                    ],
                )?;
            }
            _ => {
                let opts_identity = if [
                    "resolveCallerClientOrderId",
                    "resolveMarginTypeForOrder",
                    "getPositionSideForOrder",
                    "placeAlgoOrder",
                ]
                .contains(name)
                {
                    boolean(
                        args.first()
                            .is_some_and(|a| Rc::ptr_eq(a, &get(&self.cfg, "opts"))),
                    )
                } else {
                    self.undefined()
                };
                let sibling = if *name == "cancelAlgoOrder" {
                    boolean(
                        items(&get(&self.store, "orders"))
                            .iter()
                            .any(|o| Rc::ptr_eq(o, &args[0])),
                    )
                } else {
                    self.undefined()
                };
                self.effect(
                    name,
                    vec![
                        ("receiver", boolean(is(&self.receiver))),
                        ("args", array(args)),
                        ("optsIdentity", opts_identity),
                        ("siblingIdentity", sibling),
                    ],
                )?;
                if self.text_eq(&get(&self.cfg, "syncMethod"), name) {
                    return error();
                }
                if let Some(key) = match *name {
                    "resolveCallerClientOrderId" => Some("client"),
                    "resolveMarginTypeForOrder" | "getCurrentMarginType" => Some("margin"),
                    "getTickInterval" => Some("interval"),
                    _ => None,
                } {
                    return Ok(get(&self.cfg, key));
                }
                let promise = Rc::new(Raw::Promise(Promise::Method(name)));
                if *name == "fetchAndUpdateAccountLeverage" {
                    *self.background_promise.borrow_mut() = Some(promise.clone())
                }
                return Ok(promise);
            }
        }
        Ok(self.undefined())
    }
    fn monotonic_now(&self) -> Result<V, E> {
        panic!("outside runtime floor")
    }
    fn virtual_now(&self) -> Result<V, E> {
        self.effect("clock", vec![])?;
        Ok(self.number(1723638896125.5))
    }
    fn sign(&self, _: V, _: V) -> Result<V, E> {
        panic!("outside runtime floor")
    }
    fn reconnecting(&self) -> bool {
        false
    }
}
#[allow(clippy::manual_async_fn)]
impl AccountReadHost for Host {
    fn await_value(&self, v: V) -> impl Future<Output = Result<V, E>> {
        async move {
            let Raw::Promise(kind) = &*v else {
                panic!("controlled promise")
            };
            yield_once().await;
            let label = match kind {
                Promise::Method(s) | Promise::Request(s) => *s,
            };
            self.effect("await", vec![("label", text(label))])?;
            self.change(label);
            let reject = self.text_eq(&get(&self.cfg, "rejectMethod"), label)
                || (matches!(kind, Promise::Request(_))
                    && self.is_number(&get(&self.cfg, "rejectAt"))
                    && self.num(&get(&self.cfg, "rejectAt")) == self.index.get() as f64 - 1.0);
            if reject {
                return Err(self.original.clone());
            }
            Ok(match kind {
                Promise::Request(_) => get(&self.cfg, "response"),
                Promise::Method(name) => match *name {
                    "fetchMarkets" => get(&self.cfg, "markets"),
                    "fetchTickers" => get(&self.cfg, "tickers"),
                    "fetchPositions" => get(&self.cfg, "positions"),
                    "fetchOrders" => get(&self.cfg, "orders"),
                    "getPositionSideForOrder" => get(&self.cfg, "positionSide"),
                    "placeAlgoOrder" | "updateAlgoOrder" => get(&self.cfg, "algoResult"),
                    "placePositionalAlgoOrder" => get(&self.cfg, "algoIds"),
                    _ => self.undefined(),
                },
            })
        }
    }
    fn error_value(&self, e: &E) -> V {
        e.clone()
    }
    fn nullish(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined | Raw::Null)
    }
    fn is_null(&self, v: &Raw) -> bool {
        matches!(v, Raw::Null)
    }
    fn is_string(&self, v: &Raw) -> bool {
        matches!(v, Raw::Text(_))
    }
    fn text_eq(&self, v: &Raw, s: &str) -> bool {
        matches!(v,Raw::Text(t) if *t==units(s))
    }
    fn undefined(&self) -> V {
        Rc::new(Raw::Undefined)
    }
    fn null(&self) -> V {
        Rc::new(Raw::Null)
    }
    fn number(&self, n: f64) -> V {
        Rc::new(Raw::Number(n))
    }
    fn array(&self, a: Vec<V>) -> V {
        array(a)
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v, Raw::Array(_))
    }
    fn spread(&self, v: V) -> Result<Vec<V>, E> {
        Ok(items(&v))
    }
    fn reduce(&self, rows: V, mut acc: V, mut f: impl FnMut(V, V) -> Result<V, E>) -> Result<V, E> {
        for row in items(&rows) {
            acc = f(acc, row)?
        }
        Ok(acc)
    }
    fn normalize_symbol(&self, v: V) -> Result<V, E> {
        let parts = self.split(v)?;
        let base = parts.get(1).cloned().unwrap_or(self.undefined());
        let quote = parts.get(2).cloned().unwrap_or(self.undefined());
        Ok(self.text(&[string(&base), string(&quote)].concat()))
    }
    fn get_key(&self, v: V, k: &str) -> Result<V, E> {
        if self.nullish(&v) {
            return Ok(self.undefined());
        }
        let entries = ordered(&v);
        for name in [k.to_string(), snake(k), camel(k)] {
            if let Some((_, v)) = entries.iter().find(|(key, _)| *key == units(&name)) {
                return Ok(v.clone());
            }
        }
        Ok(self.undefined())
    }
}
fn snake(key: &str) -> String {
    let mut s = String::new();
    for c in key.chars() {
        if c.is_ascii_uppercase() {
            s.push('_');
            s.push(c.to_ascii_lowercase())
        } else {
            s.push(c)
        }
    }
    s
}
fn camel(key: &str) -> String {
    key.split('_')
        .enumerate()
        .map(|(i, s)| {
            if i == 0 {
                s.to_string()
            } else {
                let mut c = s.chars();
                c.next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default()
                    + c.as_str()
            }
        })
        .collect()
}
impl DataHost for Host {
    fn numeric(&self, v: V) -> Result<f64, E> {
        Ok(self.num(&v))
    }
    fn arithmetic(&self, op: Op, a: V, b: V) -> Result<V, E> {
        let (a, b) = (self.num(&a), self.num(&b));
        Ok(self.number(match op {
            Op::Sub => a - b,
            Op::Mul => a * b,
            Op::Div => a / b,
            Op::Min => {
                if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.min(b)
                }
            }
            Op::Max => {
                if a.is_nan() || b.is_nan() {
                    f64::NAN
                } else {
                    a.max(b)
                }
            }
        }))
    }
    fn compare(&self, a: V, b: V, less: bool) -> Result<bool, E> {
        Ok(if less {
            self.num(&a) < self.num(&b)
        } else {
            self.num(&a) > self.num(&b)
        })
    }
    fn round(&self, v: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::math_round(self.num(&v))))
    }
    fn split(&self, v: V) -> Result<Vec<V>, E> {
        let Raw::Text(t) = &*v else {
            panic!("string split floor")
        };
        Ok(t.split(|c| *c == b'_' as u16)
            .map(|s| self.text(s))
            .collect())
    }
    fn starts_with(&self, v: V, p: &str) -> Result<bool, E> {
        let Raw::Text(t) = &*v else { panic!() };
        Ok(t.starts_with(&units(p)))
    }
    fn filter(&self, rows: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        let mut a = vec![];
        for row in items(&rows) {
            if f(row.clone())? {
                a.push(row)
            }
        }
        Ok(array(a))
    }
    fn map(&self, rows: V, mut f: impl FnMut(V) -> Result<V, E>) -> Result<V, E> {
        let mut a = vec![];
        for row in items(&rows) {
            a.push(f(row)?)
        }
        Ok(array(a))
    }
    fn clone_deep(&self, v: V) -> Result<V, E> {
        self.effect("clone", vec![])?;
        let cloned = snapshot(&v);
        assert!(!Rc::ptr_eq(&v, &cloned), "native clone distinct root");
        Ok(cloned)
    }
    fn merge(&self, v: V, fields: Vec<(&'static str, V)>) -> Result<V, E> {
        let out = Rc::new(Raw::Object(RefCell::new(ordered(&v))));
        for (k, v) in fields {
            set(&out, k, v)
        }
        Ok(out)
    }
    fn uniq_id(&self, v: V) -> Result<V, E> {
        let mut ids = vec![];
        let mut out = vec![];
        for row in items(&v) {
            let id = string(&get(&row, "id"));
            if !ids.contains(&id) {
                ids.push(id);
                out.push(row)
            }
        }
        Ok(array(out))
    }
    fn reverse(&self, v: V) -> Result<V, E> {
        let Raw::Array(a) = &*v else { panic!() };
        a.borrow_mut().reverse();
        Ok(v)
    }
    fn push(&self, v: V, values: Vec<V>) -> Result<(), E> {
        let Raw::Array(a) = &*v else { panic!() };
        a.borrow_mut().extend(values);
        Ok(())
    }
    fn reverse_symbol(&self, v: V) -> Result<V, E> {
        Ok(text(&format!(
            "PERP_{}_USDC",
            String::from_utf16(&string(&v))
                .unwrap()
                .replacen("USDC", "", 1)
        )))
    }
    fn order_type(&self, v: V) -> Result<V, E> {
        Ok(match String::from_utf16(&string(&v)).unwrap().as_str() {
            "LIMIT" => text("limit"),
            "MARKET" => text("market"),
            "STOP_LOSS" => text("stop_market"),
            "TAKE_PROFIT" => text("take_profit_market"),
            "TRAILING_STOP" => text("trailing_stop_market"),
            _ => self.undefined(),
        })
    }
    fn order_side(&self, v: V) -> Result<V, E> {
        Ok(if self.text_eq(&v, "BUY") {
            text("buy")
        } else if self.text_eq(&v, "SELL") {
            text("sell")
        } else {
            self.undefined()
        })
    }
    fn subtract(&self, a: V, b: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::subtract(self.num(&a), self.num(&b))))
    }
    fn is_undefined(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined)
    }
    fn boolean(&self, b: bool) -> V {
        boolean(b)
    }
}
async fn yield_once() {
    let mut first = true;
    std::future::poll_fn(move |_| {
        if first {
            first = false;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await
}
#[allow(clippy::manual_async_fn)]
impl RuntimeHost for Host {
    fn strict_eq(&self, a: &Raw, b: &Raw) -> bool {
        match (a, b) {
            (Raw::Undefined, Raw::Undefined) | (Raw::Null, Raw::Null) => true,
            (Raw::Bool(a), Raw::Bool(b)) => a == b,
            (Raw::Number(a), Raw::Number(b)) => a == b,
            (Raw::Text(a), Raw::Text(b)) => a == b,
            _ => std::ptr::eq(a, b),
        }
    }
    fn same(&self, a: &Raw, b: &Raw) -> bool {
        match (a, b) {
            (Raw::Number(a), Raw::Number(b)) => {
                (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
            }
            _ => self.strict_eq(a, b),
        }
    }
    fn is_number(&self, v: &Raw) -> bool {
        matches!(v, Raw::Number(_))
    }
    fn is_object(&self, v: &Raw) -> bool {
        matches!(v, Raw::Object(_) | Raw::Array(_) | Raw::Null)
    }
    fn own(&self, v: V, k: &str) -> Result<bool, E> {
        Ok(ordered(&v).iter().any(|(key, _)| *key == units(k)))
    }
    fn keys(&self, v: V) -> Result<Vec<Vec<u16>>, E> {
        Ok(ordered(&v).into_iter().map(|(k, _)| k).collect())
    }
    fn find(&self, v: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        for r in items(&v) {
            if f(r.clone())? {
                return Ok(r);
            }
        }
        Ok(self.undefined())
    }
    fn adjust(&self, v: V, step: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::adjust(self.num(&v), self.num(&step))))
    }
    fn inverse_type(&self, v: V) -> Result<V, E> {
        Ok(match String::from_utf16(&string(&v)).unwrap().as_str() {
            "limit" => text("LIMIT"),
            "market" => text("MARKET"),
            "stop_market" => text("STOP_LOSS"),
            "take_profit_market" => text("TAKE_PROFIT"),
            "trailing_stop_market" => text("TRAILING_STOP"),
            _ => self.undefined(),
        })
    }
    fn inverse_side(&self, v: V) -> Result<V, E> {
        Ok(if self.text_eq(&v, "buy") {
            text("BUY")
        } else if self.text_eq(&v, "sell") {
            text("SELL")
        } else {
            self.undefined()
        })
    }
    fn omit_undefined(&self, v: V) -> Result<V, E> {
        Ok(Rc::new(Raw::Object(RefCell::new(
            ordered(&v)
                .into_iter()
                .filter(|(_, v)| !self.is_undefined(v))
                .collect(),
        ))))
    }
    fn ceil(&self, v: V) -> Result<V, E> {
        Ok(self.number(self.num(&v).ceil()))
    }
    fn remainder(&self, a: V, b: V) -> Result<V, E> {
        Ok(self.number(self.num(&a) % self.num(&b)))
    }
    fn add(&self, a: V, b: V) -> Result<V, E> {
        Ok(if self.is_string(&a) || self.is_string(&b) {
            self.text(&[string(&a), string(&b)].concat())
        } else {
            self.number(self.num(&a) + self.num(&b))
        })
    }
    fn times(&self, n: V, mut f: impl FnMut() -> Result<V, E>) -> Result<V, E> {
        let n = self.num(&n);
        assert!(
            n.is_finite() && (0.0..=16.0).contains(&n),
            "finite lots floor"
        );
        Ok(array(
            (0..n as usize)
                .map(|_| f())
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
    fn map_series<F: Future<Output = Result<V, E>>>(
        &self,
        rows: V,
        mut f: impl FnMut(V) -> F,
    ) -> impl Future<Output = Result<V, E>> {
        async move {
            let mut out = vec![];
            for row in items(&rows) {
                out.push(f(row).await?)
            }
            Ok(array(out))
        }
    }
    fn sum_by(&self, rows: V, key: &str) -> Result<V, E> {
        let mut sum = self.undefined();
        for row in items(&rows) {
            let v = get(&row, key);
            if !self.is_undefined(&v) {
                sum = if self.is_undefined(&sum) {
                    v
                } else {
                    self.add(sum, v)?
                }
            }
        }
        Ok(if self.is_undefined(&sum) {
            self.number(0.0)
        } else {
            sum
        })
    }
    fn new_error(&self, message: V) -> E {
        object(vec![("message", message)])
    }
    fn error_log_options(&self, message: V) -> Result<V, E> {
        let msg = if let Raw::Text(s) = &*message {
            Some(String::from_utf16(s).unwrap())
        } else {
            None
        };
        Ok(
            if safe_cex_venue_orderly::errors::get_orderly_order_error_log_options(msg.as_deref())
                .is_some()
            {
                object(vec![("suppressConsoleLog", boolean(true))])
            } else {
                self.undefined()
            },
        )
    }
    fn attach_catch(&self, promise: V, callback: LeverageCatch<Self>) -> Result<(), E> {
        assert!(
            Rc::ptr_eq(&promise, self.background_promise.borrow().as_ref().unwrap()),
            "native original background promise"
        );
        self.effect("catch", vec![("promiseIdentity", boolean(true))])?;
        self.background.borrow_mut().push(callback);
        Ok(())
    }
    fn schedule(&self, callback: TickCallback<Self>, interval: V) {
        match self.effect("loop", vec![("interval", interval)]) {
            Ok(()) => self.scheduled.borrow_mut().push(callback),
            Err(e) => self.detached.borrow_mut().push(e),
        }
    }
}
fn setup(cfg: &Value) -> Host {
    let cfg = decode(cfg);
    let store = object(vec![
        ("orders", get(&cfg, "storeOrders")),
        (
            "marketsBySymbol",
            object(vec![("BTCUSDC", get(&cfg, "market"))]),
        ),
        ("loaded", get(&cfg, "loaded")),
        ("balance", get(&cfg, "balance")),
        ("positions", get(&cfg, "positions")),
        ("update", Rc::new(Raw::Function("update"))),
    ]);
    let xhr = || {
        object(vec![
            ("get", Rc::new(Raw::Function("get"))),
            ("post", Rc::new(Raw::Function("post"))),
            ("put", Rc::new(Raw::Function("put"))),
        ])
    };
    let xhr_original = xhr();
    let alternate = xhr();
    let public = object(vec![
        ("dispose", Rc::new(Raw::Function("publicDispose"))),
        (
            "connectAndSubscribe",
            Rc::new(Raw::Function("publicConnect")),
        ),
    ]);
    let private = object(vec![
        ("dispose", Rc::new(Raw::Function("privateDispose"))),
        (
            "connectAndSubscribe",
            Rc::new(Raw::Function("privateConnect")),
        ),
    ]);
    let emitter = object(vec![("emit", Rc::new(Raw::Function("emit")))]);
    let receiver = object(vec![
        ("store", store.clone()),
        ("xhr", xhr_original.clone()),
        ("publicWebsocket", public.clone()),
        ("privateWebsocket", private.clone()),
        ("emitter", emitter.clone()),
        ("isDisposed", get(&cfg, "disposed")),
        ("isPublicDataOnly", get(&cfg, "public")),
        ("orderlyExchangeName", get(&cfg, "exchange")),
    ]);
    for name in [
        "resolveCallerClientOrderId",
        "resolveMarginTypeForOrder",
        "getCurrentMarginType",
        "getTickInterval",
        "fetchMarkets",
        "fetchTickers",
        "fetchPositions",
        "fetchOrders",
        "fetchAndUpdateMarginTypes",
        "fetchAndUpdateAccountLeverage",
        "tick",
        "getPositionSideForOrder",
        "placeAlgoOrder",
        "updateAlgoOrder",
        "placePositionalAlgoOrder",
        "cancelAlgoOrder",
        "isAlgoOrder",
        "toOrderlyMarginMode",
        "mapMarginType",
        "assertSingleNativeClientOrderId",
        "log",
        "emitAndLogError",
    ] {
        set(&receiver, name, Rc::new(Raw::Function(name)))
    }
    let original = get(&cfg, "error");
    Host(Rc::new(State {
        cfg,
        receiver,
        store,
        xhr: xhr_original,
        alternate,
        public,
        private,
        emitter,
        original,
        events: RefCell::new(vec![]),
        index: Cell::new(0),
        background: RefCell::new(vec![]),
        scheduled: RefCell::new(vec![]),
        background_promise: RefCell::new(None),
        detached: RefCell::new(vec![]),
    }))
}
fn oracle() -> &'static Value {
    static OUT: OnceLock<Value> = OnceLock::new();
    OUT.get_or_init(|| {
        let r = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/runtime-actions-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            r.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let v: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(
            v["hashes"].as_object().unwrap().len(),
            9,
            "independent source file floor"
        );
        v
    })
}
fn drive<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    let mut cx = Context::from_waker(Waker::noop());
    for _ in 0..32 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut cx) {
            return r;
        }
    }
    panic!("finite continuation floor")
}
fn check(name: &str, floor: usize) {
    let rows = oracle()["rows"][name].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent runtime case floor");
    for (i, row) in rows.iter().enumerate() {
        let h = setup(&row["cfg"]);
        let mut f = h.run(name);
        let mut cx = Context::from_waker(Waker::noop());
        let first = f.as_mut().poll(&mut cx);
        if let Some(prefix) = row["prefix"].as_u64() {
            assert_eq!(
                h.events.borrow().len(),
                prefix as usize,
                "native synchronous prefix {name}[{i}]"
            )
        }
        let result = match first {
            Poll::Ready(v) => v,
            Poll::Pending => drive(f),
        };
        match result {
            Ok(v) => {
                assert!(row["error"].is_null(), "source error {name}[{i}] {row}");
                let identity =
                    if name == "placeAlgoOrder" && h.truthy(&get(&get(&h.cfg, "opts"), "price")) {
                        Rc::ptr_eq(&v, &get(&h.cfg, "algoIds"))
                    } else if (name == "placeOrder"
                        && ["stop_market", "take_profit_market", "trailing_stop_market"]
                            .iter()
                            .any(|s| h.text_eq(&get(&get(&h.cfg, "opts"), "type"), s)))
                        || (name == "updateOrder"
                            && ["stop_market", "take_profit_market", "trailing_stop_market"]
                                .iter()
                                .any(|s| h.text_eq(&get(&get(&h.cfg, "submitted"), "type"), s)))
                    {
                        Rc::ptr_eq(&v, &get(&h.cfg, "algoResult"))
                    } else {
                        true
                    };
                assert_eq!(
                    identity,
                    row["outputIdentity"].as_bool().unwrap(),
                    "native original delegated output"
                );
                native(&v, &row["result"], &format!("{name}[{i}] result"));
            }
            Err(e) => {
                assert!(
                    !row["error"].is_null(),
                    "unexpected native error {name}[{i}] {e:?}"
                );
                assert_eq!(
                    Rc::ptr_eq(&e, &h.original),
                    row["error"]["identity"].as_bool().unwrap(),
                    "native original error identity"
                );
                native(
                    &get(&e, "message"),
                    &row["error"]["message"],
                    "native error message",
                );
            }
        }
        let callbacks = std::mem::take(&mut *h.background.borrow_mut());
        for callback in callbacks {
            if flag(&h.cfg, "backgroundReject") {
                if let Err(e) = callback.invoke(h.original.clone()) {
                    h.effect(
                        "backgroundError",
                        vec![
                            ("identity", boolean(Rc::ptr_eq(&e, &h.original))),
                            ("message", get(&e, "message")),
                        ],
                    )
                    .unwrap();
                }
            }
        }
        for e in std::mem::take(&mut *h.detached.borrow_mut()) {
            h.effect(
                "detachedError",
                vec![
                    ("identity", boolean(Rc::ptr_eq(&e, &h.original))),
                    ("message", get(&e, "message")),
                ],
            )
            .unwrap();
        }
        let scheduled = std::mem::take(&mut *h.scheduled.borrow_mut());
        if flag(&h.cfg, "invokeLoop") {
            if let Some(cb) = scheduled.into_iter().next() {
                let p = cb.invoke().unwrap();
                drive(h.await_value(p)).unwrap();
            }
        }
        native(
            &array(h.events.borrow().clone()),
            &row["events"],
            &format!("{name}[{i}] effects"),
        );
        native(
            &object(vec![
                ("disposed", get(&h.receiver, "isDisposed")),
                ("loaded", get(&h.store, "loaded")),
                ("balance", get(&h.store, "balance")),
                ("positions", get(&h.store, "positions")),
                ("orders", get(&h.store, "orders")),
            ]),
            &row["state"],
            &format!("{name}[{i}] state"),
        );
    }
}
#[test]
fn transient_native() {
    check("isTransientOrderlyValidationError", 10)
}
#[test]
fn dispose_native() {
    check("dispose", 4)
}
#[test]
fn start_native() {
    check("start", 16)
}
#[test]
fn tick_native() {
    check("tick", 16)
}
#[test]
fn update_native() {
    check("updateOrder", 25)
}
#[test]
fn place_native() {
    check("placeOrder", 18)
}
#[test]
fn algo_native() {
    check("placeAlgoOrder", 10)
}
#[test]
fn positional_native() {
    check("placePositionalAlgoOrder", 11)
}
