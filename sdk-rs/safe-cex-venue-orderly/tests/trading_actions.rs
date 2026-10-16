use safe_cex_venue_orderly::{
    account_order_reads::AccountReadHost,
    trading_actions::{self as actions, ActionHost},
    ws_controls::WsHost,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
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
#[derive(Debug)]
struct Failure {
    value: V,
    original: bool,
}
type E = Rc<Failure>;
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
#[derive(Clone, Debug)]
enum Promise {
    Request(usize),
    Empty,
}
struct Host {
    cfg: V,
    receiver: V,
    store: V,
    map: V,
    original: E,
    events: RefCell<Vec<V>>,
    cache: RefCell<Vec<V>>,
    count: Cell<usize>,
    appended: Cell<bool>,
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
    fn xhr(label: &'static str) -> V {
        object(vec![
            ("label", text(label)),
            ("put", Rc::new(Raw::Function("put"))),
            ("delete", Rc::new(Raw::Function("delete"))),
            ("post", Rc::new(Raw::Function("post"))),
        ])
    }
    fn run<'a>(&'a self, name: &str) -> Pin<Box<dyn Future<Output = Result<V, E>> + 'a>> {
        let r = self.receiver.clone();
        match name {
            "updateAlgoOrder" => Box::pin(actions::update_algo(
                self,
                r,
                object(vec![
                    ("order", get(&self.cfg, "order")),
                    ("update", get(&self.cfg, "update")),
                ]),
            )),
            "cancelOrders" => Box::pin(actions::cancel_orders(self, r, get(&self.cfg, "orders"))),
            "cancelAlgoOrder" => Box::pin(actions::cancel_algo(self, r, get(&self.cfg, "order"))),
            "cancelAllOrders" => Box::pin(actions::cancel_all(self, r)),
            "cancelSymbolOrders" => {
                Box::pin(actions::cancel_symbol(self, r, get(&self.cfg, "symbol")))
            }
            "setLeverage" => Box::pin(actions::set_leverage(
                self,
                r,
                get(&self.cfg, "symbol"),
                get(&self.cfg, "input"),
            )),
            "setAllLeverage" => {
                Box::pin(actions::set_all_leverage(self, r, get(&self.cfg, "input")))
            }
            "changeMarginType" => Box::pin(actions::change_margin(
                self,
                r,
                get(&self.cfg, "isolated"),
                get(&self.cfg, "symbol"),
            )),
            _ => panic!(),
        }
    }
    fn num(&self, v: &Raw) -> f64 {
        match v {
            Raw::Number(n) => *n,
            Raw::Undefined => f64::NAN,
            Raw::Null => 0.0,
            Raw::Bool(b) => u8::from(*b) as f64,
            Raw::Text(s) => safe_cex_num::number_from_string(&String::from_utf16(s).unwrap()),
            _ => f64::NAN,
        }
    }
}
impl WsHost for Host {
    type Value = Raw;
    type Error = E;
    type Iterator = (V, usize);
    fn get(&self, v: &V, key: &[u16]) -> Result<V, E> {
        Ok(get(v, &String::from_utf16(key).unwrap()))
    }
    fn set(&self, v: &V, key: &[u16], next: V) -> Result<(), E> {
        set(v, &String::from_utf16(key).unwrap(), next);
        Ok(())
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v {
            Raw::Undefined | Raw::Null => false,
            Raw::Bool(b) => *b,
            Raw::Number(n) => *n != 0.0 && !n.is_nan(),
            Raw::Text(s) => !s.is_empty(),
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
        panic!("outside action floor")
    }
    fn parse(&self, _: V) -> Result<V, E> {
        panic!("outside action floor")
    }
    fn includes(&self, v: &V, n: &[u16]) -> Result<bool, E> {
        let Raw::Text(s) = &**v else {
            panic!("string includes floor")
        };
        Ok(n.is_empty() || s.windows(n.len()).any(|w| w == n))
    }
    fn own_values(&self, _: V) -> Result<Vec<V>, E> {
        panic!("outside action floor")
    }
    fn flatten_one(&self, _: Vec<V>) -> Result<Vec<V>, E> {
        panic!("outside action floor")
    }
    fn entries(&self, v: V) -> Result<Vec<(Vec<u16>, V)>, E> {
        Ok(ordered(&v))
    }
    fn iterator(&self, v: V) -> Result<Self::Iterator, E> {
        Ok((v, 0))
    }
    fn next(&self, i: &mut Self::Iterator) -> Result<Option<V>, E> {
        let Raw::Array(a) = &*i.0 else { panic!() };
        let v = a.borrow().get(i.1).cloned();
        i.1 += 1;
        assert!(i.1 <= 16, "finite iteration floor");
        Ok(v)
    }
    fn call(&self, f: V, receiver: Option<V>, args: Vec<V>) -> Result<V, E> {
        let Raw::Function(name) = &*f else {
            panic!("original callable")
        };
        let is = |v: &V| receiver.as_ref().is_some_and(|r| Rc::ptr_eq(r, v));
        match *name {
            "put" | "delete" | "post" => {
                let index = self.count.get();
                self.count.set(index + 1);
                let xhr = receiver.as_ref().unwrap();
                self.effect(
                    "request",
                    vec![
                        ("method", text(name)),
                        ("url", args[0].clone()),
                        ("payload", args.get(1).cloned().unwrap_or(self.undefined())),
                        ("index", self.number(index as f64)),
                        ("receiver", boolean(Rc::ptr_eq(&f, &get(xhr, name)))),
                        ("label", get(xhr, "label")),
                        ("argc", self.number(args.len() as f64)),
                    ],
                )?;
                let sync = get(&self.cfg, "syncAt");
                if matches!(&*sync,Raw::Number(n) if *n==index as f64) {
                    return Err(self.original.clone());
                }
                return Ok(Rc::new(Raw::Promise(Promise::Request(index))));
            }
            "isAlgoOrder" => {
                if flag(&self.cfg, "append") && !self.appended.replace(true) {
                    let orders = get(&self.cfg, "orders");
                    let Raw::Array(a) = &*orders else { panic!() };
                    let first = a.borrow()[0].clone();
                    a.borrow_mut().push(first);
                }
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
                        .any(|v| self.text_eq(&args[0], v)),
                ));
            }
            "cancelAlgoOrder" => {
                let identity = items(&get(&self.cfg, "orders"))
                    .iter()
                    .any(|o| Rc::ptr_eq(o, &args[0]));
                self.effect(
                    "algo",
                    vec![("identity", boolean(identity)), ("order", args[0].clone())],
                )?;
                return Ok(Rc::new(Raw::Promise(Promise::Empty)));
            }
            "mapMarginType" => {
                self.effect("mapMargin", vec![("raw", args[0].clone())])?;
                return Ok(text(
                    if String::from_utf16(&string(&args[0]))
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
            "toOrderlyMarginMode" => {
                self.effect("marginMode", vec![("margin", args[0].clone())])?;
                return Ok(text(if self.text_eq(&args[0], "isolated") {
                    "ISOLATED"
                } else {
                    "CROSS"
                }));
            }
            "getCurrentMarginType" => {
                self.effect("currentMargin", vec![("symbol", args[0].clone())])?;
                return Ok(get(&self.cfg, "margin"));
            }
            "placePositionalAlgoOrder" => {
                self.effect("place", vec![("payload", args[0].clone())])?;
                return Ok(Rc::new(Raw::Promise(Promise::Empty)));
            }
            "emitAndLogError" => self.effect(
                "log",
                vec![
                    ("receiver", boolean(is(&self.receiver))),
                    ("message", args[0].clone()),
                ],
            )?,
            "updateOrderStatus" => self.effect(
                "status",
                vec![
                    ("receiver", boolean(is(&self.store))),
                    ("selector", args[0].clone()),
                    ("status", args[1].clone()),
                ],
            )?,
            "updatePositions" => {
                let positions = items(&get(&self.store, "positions"));
                let patches = items(&args[0]);
                let identities = array(
                    patches
                        .iter()
                        .map(|pair| {
                            boolean(positions.iter().any(|p| Rc::ptr_eq(p, &items(pair)[0])))
                        })
                        .collect(),
                );
                self.effect(
                    "updatePositions",
                    vec![
                        ("receiver", boolean(is(&self.store))),
                        ("patches", args[0].clone()),
                        ("identities", identities),
                    ],
                )?;
                for pair in patches {
                    let pair = items(&pair);
                    for (k, v) in ordered(&pair[1]) {
                        self.set(&pair[0], &k, v)?;
                    }
                }
            }
            "set" => {
                self.effect(
                    "mapSet",
                    vec![
                        ("receiver", boolean(is(&self.map))),
                        ("symbol", args[0].clone()),
                        ("margin", args[1].clone()),
                    ],
                )?;
                self.cache.borrow_mut().push(array(args));
            }
            _ => panic!("action callable {name}"),
        };
        Ok(self.undefined())
    }
    fn monotonic_now(&self) -> Result<V, E> {
        panic!("outside action floor")
    }
    fn virtual_now(&self) -> Result<V, E> {
        panic!("outside action floor")
    }
    fn sign(&self, _: V, _: V) -> Result<V, E> {
        panic!("outside action floor")
    }
    fn reconnecting(&self) -> bool {
        false
    }
}
#[allow(clippy::manual_async_fn)]
impl AccountReadHost for Host {
    fn await_value(&self, v: V) -> impl Future<Output = Result<V, E>> {
        async move {
            let Raw::Promise(p) = &*v else { panic!() };
            match p {
                Promise::Empty => Ok(self.undefined()),
                Promise::Request(index) => {
                    yield_once().await;
                    self.effect("await", vec![("index", self.number(*index as f64))])?;
                    let change = get(&self.cfg, "afterAwait");
                    if self.truthy(&get(&change, "xhr")) {
                        set(&self.receiver, "xhr", Self::xhr("alternate"));
                    }
                    let price = get(&change, "siblingPrice");
                    if !matches!(&*price, Raw::Undefined) {
                        let orders = items(&get(&self.store, "orders"));
                        if let Some(sibling) = orders.get(1) {
                            set(sibling, "price", price);
                        }
                    }
                    let positions = get(&change, "positions");
                    if self.truthy(&positions) {
                        set(&self.store, "positions", positions);
                    }
                    let reject = get(&self.cfg, "rejectAt");
                    if matches!(&*reject,Raw::Number(n) if *n==*index as f64) {
                        return Err(self.original.clone());
                    }
                    Ok(get(&self.cfg, "response"))
                }
            }
        }
    }
    fn error_value(&self, e: &E) -> V {
        e.value.clone()
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
    fn array(&self, v: Vec<V>) -> V {
        array(v)
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
    fn normalize_symbol(&self, _: V) -> Result<V, E> {
        panic!("outside action floor")
    }
    fn get_key(&self, v: V, key: &str) -> Result<V, E> {
        Ok(get(&v, key))
    }
}
#[allow(clippy::manual_async_fn)]
impl ActionHost for Host {
    fn has(&self, v: &V, key: &str) -> Result<bool, E> {
        Ok(ordered(v).iter().any(|(k, _)| *k == units(key)))
    }
    fn strict_eq(&self, l: &Raw, r: &Raw) -> bool {
        match (l, r) {
            (Raw::Undefined, Raw::Undefined) | (Raw::Null, Raw::Null) => true,
            (Raw::Number(a), Raw::Number(b)) => a == b,
            (Raw::Text(a), Raw::Text(b)) => a == b,
            (Raw::Bool(a), Raw::Bool(b)) => a == b,
            _ => std::ptr::eq(l, r),
        }
    }
    fn finite_number(&self, v: &Raw) -> bool {
        matches!(v,Raw::Number(n) if n.is_finite())
    }
    fn true_value(&self) -> V {
        boolean(true)
    }
    fn numeric(&self, v: V) -> Result<V, E> {
        Ok(self.number(self.num(&v)))
    }
    fn parse_int(&self, v: V, radix: u32) -> Result<V, E> {
        Ok(self.number(safe_cex_num::parse_int(
            &String::from_utf16(&string(&v)).unwrap(),
            Some(radix as i32),
        )))
    }
    fn math_min(&self, l: V, r: V) -> Result<V, E> {
        let (a, b) = (self.num(&l), self.num(&r));
        Ok(self.number(if a.is_nan() || b.is_nan() {
            f64::NAN
        } else {
            a.min(b)
        }))
    }
    fn math_max(&self, l: V, r: V) -> Result<V, E> {
        let (a, b) = (self.num(&l), self.num(&r));
        Ok(self.number(if a.is_nan() || b.is_nan() {
            f64::NAN
        } else {
            a.max(b)
        }))
    }
    fn math_round(&self, v: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::math_round(self.num(&v))))
    }
    fn reverse_symbol(&self, v: V) -> Result<V, E> {
        Ok(text(&format!(
            "PERP_{}_USDC",
            String::from_utf16(&string(&v))
                .unwrap()
                .replacen("USDC", "", 1)
        )))
    }
    fn new_error(&self, message: V) -> E {
        Rc::new(Failure {
            value: object(vec![(
                "message",
                text(&String::from_utf16(&string(&message)).unwrap()),
            )]),
            original: false,
        })
    }
    fn filter(&self, rows: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        let mut out = vec![];
        for row in items(&rows) {
            if f(row.clone())? {
                out.push(row)
            }
        }
        Ok(array(out))
    }
    fn map(&self, rows: V, mut f: impl FnMut(V) -> Result<V, E>) -> Result<V, E> {
        let mut out = vec![];
        for row in items(&rows) {
            out.push(f(row)?)
        }
        Ok(array(out))
    }
    fn find(&self, rows: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        for row in items(&rows) {
            if f(row.clone())? {
                return Ok(row);
            }
        }
        Ok(self.undefined())
    }
    fn series<F: Future<Output = Result<(), E>>>(
        &self,
        rows: V,
        mut f: impl FnMut(V) -> F,
    ) -> impl Future<Output = Result<(), E>> {
        async move {
            let mut index = 0;
            loop {
                let values = items(&rows);
                let Some(value) = values.get(index).cloned() else {
                    break;
                };
                index += 1;
                assert!(index <= 16, "finite series floor");
                yield_once().await;
                f(value).await?;
            }
            Ok(())
        }
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
fn setup(cfg: &Value) -> Host {
    let cfg = decode(cfg);
    let symbol = String::from_utf16(&string(&get(&cfg, "symbol"))).unwrap();
    let store = object(vec![
        ("orders", get(&cfg, "storeOrders")),
        ("positions", get(&cfg, "positions")),
        (
            "marketsBySymbol",
            object(vec![(&symbol, get(&cfg, "market"))]),
        ),
        (
            "updateOrderStatus",
            Rc::new(Raw::Function("updateOrderStatus")),
        ),
        ("updatePositions", Rc::new(Raw::Function("updatePositions"))),
    ]);
    let map = object(vec![("set", Rc::new(Raw::Function("set")))]);
    let receiver = object(vec![
        ("store", store.clone()),
        ("marginTypeBySymbol", map.clone()),
        ("xhr", Host::xhr("original")),
        ("isDisposed", boolean(flag(&cfg, "disposed"))),
        ("accountLeverage", get(&cfg, "accountLeverage")),
    ]);
    for name in [
        "isAlgoOrder",
        "cancelAlgoOrder",
        "mapMarginType",
        "toOrderlyMarginMode",
        "getCurrentMarginType",
        "placePositionalAlgoOrder",
        "emitAndLogError",
    ] {
        set(&receiver, name, Rc::new(Raw::Function(name)))
    }
    let original = Rc::new(Failure {
        value: get(&cfg, "error"),
        original: true,
    });
    Host {
        cfg,
        receiver,
        store,
        map,
        original,
        events: RefCell::new(vec![]),
        cache: RefCell::new(vec![]),
        count: Cell::new(0),
        appended: Cell::new(false),
    }
}
fn oracle() -> &'static Value {
    static OUT: OnceLock<Value> = OnceLock::new();
    OUT.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/trading-actions-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            v["hashes"].as_object().unwrap().len(),
            6,
            "independent source file floor"
        );
        v
    })
}
fn check(name: &str, floor: usize) {
    let rows = oracle()["rows"][name].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent action floor");
    for (i, row) in rows.iter().enumerate() {
        let h = setup(&row["cfg"]);
        let mut future = h.run(name);
        let mut cx = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut cx);
        assert_eq!(
            h.events.borrow().len(),
            row["prefix"].as_u64().unwrap() as usize,
            "native synchronous prefix {name}[{i}]"
        );
        let result = match first {
            Poll::Ready(v) => v,
            Poll::Pending => {
                let mut out = None;
                for _ in 0..24 {
                    if let Poll::Ready(v) = future.as_mut().poll(&mut cx) {
                        out = Some(v);
                        break;
                    }
                }
                out.expect("finite continuation floor")
            }
        };
        match result {
            Ok(v) => {
                assert!(row["error"].is_null(), "source error {row}");
                native(&v, &row["result"], "native action result")
            }
            Err(e) => {
                assert_eq!(e.original, row["error"]["identity"].as_bool().unwrap());
                if e.original {
                    assert!(
                        Rc::ptr_eq(&e, &h.original),
                        "native original error allocation"
                    )
                }
                native(
                    &get(&e.value, "message"),
                    &row["error"]["message"],
                    "native error metadata",
                )
            }
        };
        native(
            &Raw::Array(RefCell::new(h.events.borrow().clone())),
            &row["events"],
            &format!("{name}[{i}] effects"),
        );
        let state = object(vec![
            ("leverage", get(&h.receiver, "accountLeverage")),
            ("positions", get(&h.store, "positions")),
            ("cache", array(h.cache.borrow().clone())),
        ]);
        native(&state, &row["state"], "native final state");
    }
}
#[test]
fn update_algo_native() {
    check("updateAlgoOrder", 8)
}
#[test]
fn cancel_orders_native() {
    check("cancelOrders", 8)
}
#[test]
fn cancel_algo_native() {
    check("cancelAlgoOrder", 9)
}
#[test]
fn cancel_all_native() {
    check("cancelAllOrders", 7)
}
#[test]
fn cancel_symbol_native() {
    check("cancelSymbolOrders", 7)
}
#[test]
fn leverage_native() {
    check("setLeverage", 14)
}
#[test]
fn all_leverage_native() {
    check("setAllLeverage", 13)
}
#[test]
fn margin_native() {
    check("changeMarginType", 10)
}
