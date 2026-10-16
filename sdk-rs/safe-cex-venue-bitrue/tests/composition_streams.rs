use safe_cex_venue_bitrue::collection_reads::{Callback, Op, ReadHost};
use safe_cex_venue_bitrue::composition::{self as composition, CompositionHost};
use safe_cex_venue_bitrue::public_streams::{self as streams, StreamHost};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    ops::Deref,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
enum Raw {
    Error(V),
    Set(RefCell<Vec<V>>),
    Map(RefCell<Vec<(V, V)>>),
    Blob(V),
    Socket(&'static str, V),
    Named(String, Callback<Host>),
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Rc<Raw>>>),
    Object(RefCell<Vec<(Vec<u16>, Rc<Raw>)>>),
    Function(&'static str),
    Promise(Rc<RefCell<Promise>>),
    Callback(safe_cex_venue_bitrue::collection_reads::Callback<Host>),
}
type V = Rc<Raw>;
type E = V;
impl std::fmt::Debug for Raw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native class value")
    }
}
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
            } else if let Some(entries) = o.get("entries") {
                Raw::Object(RefCell::new(
                    entries
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|e| (units(e[0].as_str().unwrap()), decode(&e[1])))
                        .collect(),
                ))
            } else {
                panic!("tagged source object")
            }
        }
    })
}
fn get(v: &Raw, key: &str) -> V {
    if let Raw::Error(m) = v {
        return if key == "message" {
            m.clone()
        } else {
            Rc::new(Raw::Undefined)
        };
    }
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
        Raw::Error(m) => units(&format!(
            "Error: {}",
            String::from_utf16(&string(m)).unwrap()
        )),
        Raw::Set(_) | Raw::Map(_) | Raw::Blob(_) | Raw::Socket(_, _) => {
            panic!("outside coercion floor")
        }
        Raw::Named(_, _) => panic!("callable coercion outside floor"),
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
        Raw::Function(_) | Raw::Callback(_) | Raw::Promise(_) => {
            panic!("outside class coercion floor")
        }
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
        Raw::Error(m) => native(m, &expected["errorMessage"], path),
        Raw::Named(n, _) => assert_eq!(expected["function"], *n, "native function {path}"),
        Raw::Undefined => assert_eq!(
            expected.get("undefined"),
            Some(&Value::Bool(true)),
            "native undefined {path}"
        ),
        Raw::Null => assert!(expected.is_null(), "native null {path}"),
        Raw::Bool(b) => assert_eq!(expected.as_bool(), Some(*b), "native boolean {path}"),
        Raw::Number(n) => {
            assert!(
                expected.get("number").is_some(),
                "native number kind {path}"
            );
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
        Raw::Object(_) => {
            let a = ordered(v);
            let e = expected["entries"].as_array().expect("source entries");
            assert_eq!(a.len(), e.len(), "native object cardinality {path}");
            for ((k, v), e) in a.iter().zip(e) {
                assert_eq!(
                    *k,
                    units(e[0].as_str().unwrap()),
                    "native own key order {path}"
                );
                native(v, &e[1], &format!("{path}.{}", e[0]));
            }
        }
        Raw::Function(name) => assert_eq!(expected["function"], *name, "native function name"),
        Raw::Callback(_) => assert_eq!(expected["function"], "", "native callback {path}"),
        Raw::Set(a) => native(
            &Raw::Array(RefCell::new(a.borrow().clone())),
            &expected["set"],
            path,
        ),
        Raw::Map(a) => native(
            &Raw::Array(RefCell::new(
                a.borrow()
                    .iter()
                    .map(|(k, v)| array(vec![k.clone(), v.clone()]))
                    .collect(),
            )),
            &expected["map"],
            path,
        ),
        Raw::Blob(t) => native(t, &expected["blob"], path),
        Raw::Socket(w, _) => assert_eq!(expected["socket"], *w, "native socket {path}"),
        Raw::Promise(_) => panic!("unsettled native value"),
    }
}
fn snapshot(value: &V) -> V {
    Rc::new(match &**value {
        Raw::Set(a) => Raw::Set(RefCell::new(a.borrow().clone())),
        Raw::Map(a) => Raw::Map(RefCell::new(a.borrow().clone())),
        Raw::Blob(t) => Raw::Blob(t.clone()),
        Raw::Socket(w, r) => Raw::Socket(w, r.clone()),
        Raw::Error(m) => Raw::Error(m.clone()),
        Raw::Named(n, f) => Raw::Named(n.clone(), f.clone()),
        Raw::Undefined => Raw::Undefined,
        Raw::Null => Raw::Null,
        Raw::Bool(b) => Raw::Bool(*b),
        Raw::Number(n) => Raw::Number(*n),
        Raw::Text(s) => Raw::Text(s.clone()),
        Raw::Function(f) => Raw::Function(f),
        Raw::Callback(f) => Raw::Callback(f.clone()),
        Raw::Promise(p) => Raw::Promise(p.clone()),
        Raw::Array(a) => Raw::Array(RefCell::new(a.borrow().iter().map(snapshot).collect())),
        Raw::Object(o) => Raw::Object(RefCell::new(
            o.borrow()
                .iter()
                .map(|(k, v)| (k.clone(), snapshot(v)))
                .collect(),
        )),
    })
}
#[derive(Clone)]
struct Host(Rc<State>);
impl Deref for Host {
    type Target = State;
    fn deref(&self) -> &State {
        &self.0
    }
}
type Task = std::pin::Pin<Box<dyn Future<Output = Result<V, E>>>>;
struct Promise {
    future: Option<Task>,
    result: Option<Result<V, E>>,
}
struct State {
    cfg: V,
    r: V,
    error: V,
    events: RefCell<Vec<V>>,
    next_timer: Cell<f64>,
    timers: RefCell<Vec<(f64, f64, V)>>,
    id: Cell<usize>,
    options: V,
    store: V,
    parent: V,
    api: V,
    periodic: V,
    every: V,
    instances: RefCell<Vec<V>>,
    first_rule: RefCell<V>,
    parsed: RefCell<V>,
    book: RefCell<V>,
    group: String,
}
fn num(v: &Raw) -> f64 {
    match v {
        Raw::Number(n) => *n,
        Raw::Null => 0.0,
        Raw::Bool(b) => u8::from(*b) as f64,
        Raw::Text(s) => safe_cex_num::number_from_string(&String::from_utf16(s).unwrap()),
        _ => f64::NAN,
    }
}
fn same(a: &V, b: &V) -> bool {
    match (&**a, &**b) {
        (Raw::Undefined, Raw::Undefined) | (Raw::Null, Raw::Null) => true,
        (Raw::Bool(a), Raw::Bool(b)) => a == b,
        (Raw::Number(a), Raw::Number(b)) => a == b,
        (Raw::Text(a), Raw::Text(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
impl Host {
    fn effect(&self, kind: &str, args: Vec<V>) -> Result<(), E> {
        self.events.borrow_mut().push(object(vec![
            ("kind", text(kind)),
            ("args", array(args.iter().map(snapshot).collect())),
        ]));
        if same(&get(&self.cfg, "fail"), &text(kind)) {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn begin(&self, mut f: Task) -> V {
        let (future, result) = match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Pending => (Some(f), None),
            Poll::Ready(r) => (None, Some(r)),
        };
        Rc::new(Raw::Promise(Rc::new(RefCell::new(Promise {
            future,
            result,
        }))))
    }
}
impl ReadHost for Host {
    type Value = V;
    type Error = E;
    fn undefined(&self) -> V {
        Rc::new(Raw::Undefined)
    }
    fn number(&self, n: f64) -> V {
        Rc::new(Raw::Number(n))
    }
    fn text(&self, s: &str) -> V {
        text(s)
    }
    fn boolean(&self, b: bool) -> V {
        boolean(b)
    }
    fn get(&self, v: &V, key: &str) -> Result<V, E> {
        if self.nullish(v) {
            return Err(object(vec![(
                "message",
                text(&format!(
                    "Cannot read properties of {} (reading '{key}')",
                    if matches!(&**v, Raw::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                )),
            )]));
        }
        if let Raw::Array(a) = &**v {
            if key == "length" {
                return Ok(self.number(a.borrow().len() as f64));
            }
        }
        if key == "createXhrWithSecondaryKey"
            && self.instances.borrow().iter().any(|r| Rc::ptr_eq(r, v))
        {
            return Ok(Rc::new(Raw::Function("secondary")));
        }
        if matches!(&**v, Raw::Array(_) | Raw::Set(_))
            && matches!(key, "forEach" | "filter" | "add" | "has")
        {
            return Ok(Rc::new(Raw::Function(match key {
                "forEach" => "forEach",
                "filter" => "filter",
                "add" => "add",
                _ => "has",
            })));
        }
        Ok(get(v, key))
    }
    fn set(&self, v: &V, key: &str, next: V) -> Result<(), E> {
        set(v, key, next);
        Ok(())
    }
    fn truthy(&self, v: &V) -> bool {
        match &**v {
            Raw::Undefined | Raw::Null => false,
            Raw::Bool(b) => *b,
            Raw::Number(n) => *n != 0.0 && !n.is_nan(),
            Raw::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(&**v, Raw::Null | Raw::Undefined)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        same(a, b)
    }
    fn set_equal(&self, a: &V, b: &V) -> bool {
        same(a, b)
            || matches!((&**a,&**b),(Raw::Number(a),Raw::Number(b)) if a.is_nan()&&b.is_nan())
    }
    fn is_array(&self, v: &V) -> bool {
        matches!(&**v, Raw::Array(_))
    }
    fn is_boolean(&self, v: &V) -> bool {
        matches!(&**v, Raw::Bool(_))
    }
    fn is_finite(&self, v: &V) -> bool {
        matches!(&**v,Raw::Number(n) if n.is_finite())
    }
    fn numeric(&self, v: V) -> Result<V, E> {
        Ok(self.number(num(&v)))
    }
    fn string(&self, v: V) -> Result<V, E> {
        Ok(Rc::new(Raw::Text(string(&v))))
    }
    fn uppercase(&self, v: V) -> Result<V, E> {
        Ok(text(
            &String::from_utf16(&string(&v)).unwrap().to_uppercase(),
        ))
    }
    fn concat(&self, vs: Vec<V>) -> Result<V, E> {
        Ok(Rc::new(Raw::Text(
            vs.iter().flat_map(|v| string(v)).collect(),
        )))
    }
    fn arithmetic(&self, op: Op, a: V, b: V) -> Result<V, E> {
        if matches!(op, Op::Add) && (matches!(&*a, Raw::Text(_)) || matches!(&*b, Raw::Text(_))) {
            return self.concat(vec![a, b]);
        }
        let (a, b) = (num(&a), num(&b));
        Ok(self.number(match op {
            Op::Add => a + b,
            Op::Sub => a - b,
            Op::Mul => a * b,
            Op::Div => a / b,
            Op::Min => safe_cex_num::math_min(a, b),
            Op::Max => safe_cex_num::math_max(a, b),
        }))
    }
    fn less(&self, a: V, b: V) -> Result<bool, E> {
        Ok(match (&*a, &*b) {
            (Raw::Text(a), Raw::Text(b)) => a < b,
            _ => num(&a) < num(&b),
        })
    }
    fn abs(&self, v: V) -> Result<V, E> {
        Ok(self.number(num(&v).abs()))
    }
    fn floor(&self, v: V) -> Result<V, E> {
        Ok(self.number(num(&v).floor()))
    }
    fn date_parse(&self, v: V) -> Result<V, E> {
        let s = String::from_utf16(&string(&v)).unwrap();
        Ok(self.number(if s == "2023-11-14T22:13:20Z" {
            1_700_000_000_000.0
        } else {
            f64::NAN
        }))
    }
    fn split_at(&self, v: V, sep: &str, index: usize) -> Result<V, E> {
        Ok(String::from_utf16(&string(&v))
            .unwrap()
            .split(sep)
            .nth(index)
            .map(text)
            .unwrap_or_else(|| self.undefined()))
    }
    fn array(&self, v: Vec<V>) -> V {
        array(v)
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> V {
        object(v)
    }
    fn assign_spread(&self, target: V, value: V) -> Result<(), E> {
        for (k, v) in ordered(&value) {
            set(&target, &String::from_utf16(&k).unwrap(), v)
        }
        Ok(())
    }
    fn omit_undefined(&self, v: V) -> Result<V, E> {
        Ok(Rc::new(Raw::Object(RefCell::new(
            ordered(&v)
                .into_iter()
                .filter(|(_, v)| !matches!(&**v, Raw::Undefined))
                .collect(),
        ))))
    }
    fn items(&self, v: V) -> Result<Vec<V>, E> {
        Ok(items(&v))
    }
    fn map(&self, v: V, mut f: impl FnMut(V) -> Result<V, E>) -> Result<V, E> {
        Ok(array(
            items(&v)
                .into_iter()
                .map(&mut f)
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
    fn filter(&self, v: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        let mut out = vec![];
        for x in items(&v) {
            if f(x.clone())? {
                out.push(x)
            }
        }
        Ok(array(out))
    }
    fn some(&self, v: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<bool, E> {
        for x in items(&v) {
            if f(x.clone())? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn includes(&self, v: V, needle: V) -> Result<bool, E> {
        Ok(items(&v).iter().any(|v| self.set_equal(v, &needle)))
    }
    fn flatten(&self, v: V) -> Result<V, E> {
        let mut out = vec![];
        for v in items(&v) {
            if self.is_array(&v) {
                out.extend(items(&v))
            } else {
                out.push(v)
            }
        }
        Ok(array(out))
    }
    fn sort_time_desc(&self, v: V) -> Result<V, E> {
        let Raw::Array(a) = &*v else {
            panic!("sort array")
        };
        a.borrow_mut().sort_by(|a, b| {
            num(&get(b, "time"))
                .partial_cmp(&num(&get(a, "time")))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Ok(v)
    }
    fn slice_limit(&self, v: V, limit: V) -> Result<V, E> {
        let n = num(&limit);
        let n = if n.is_nan() { 0 } else { n.max(0.0) as usize };
        Ok(array(items(&v).into_iter().take(n).collect()))
    }
    fn table(&self, name: &str, key: V) -> Result<V, E> {
        let key = String::from_utf16(&string(&key)).unwrap();
        let s = match name {
            "ORDER_SIDE" => safe_cex_venue_bitrue::types::ORDER_SIDE
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| serde_json::to_value(v).unwrap()),
            "ORDER_TYPE" => safe_cex_venue_bitrue::types::ORDER_TYPE
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| serde_json::to_value(v).unwrap()),
            "ORDER_STATUS" => safe_cex_venue_bitrue::types::ORDER_STATUS
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| serde_json::to_value(v).unwrap()),
            _ => panic!("table"),
        };
        Ok(s.as_ref().map(decode).unwrap_or_else(|| self.undefined()))
    }
    fn call(&self, f: V, receiver: V, args: Vec<V>) -> Result<V, E> {
        if let Raw::Named(_, f) | Raw::Callback(f) = &*f {
            return f(args);
        }
        let Raw::Function(kind) = &*f else {
            return Err(self.error.clone());
        };
        let kind = *kind;
        if kind == "forEach" || kind == "filter" {
            let values = match &*receiver {
                Raw::Set(a) => a.borrow().clone(),
                _ => items(&receiver),
            };
            let mut output = vec![];
            for (i, v) in values.into_iter().enumerate() {
                let result = self.call(
                    args[0].clone(),
                    self.undefined(),
                    vec![v.clone(), self.number(i as f64), receiver.clone()],
                )?;
                if kind == "filter" && self.truthy(&result) {
                    output.push(v)
                }
            }
            return Ok(if kind == "filter" {
                array(output)
            } else {
                self.undefined()
            });
        }
        if kind == "add" || kind == "has" {
            let Raw::Set(a) = &*receiver else {
                panic!("ordinary Set")
            };
            if kind == "has" {
                return Ok(boolean(
                    a.borrow().iter().any(|v| self.set_equal(v, &args[0])),
                ));
            }
            let mut a = a.borrow_mut();
            if !a.iter().any(|v| self.set_equal(v, &args[0])) {
                a.push(args[0].clone())
            }
            return Ok(receiver.clone());
        }
        if kind == "isValid" {
            let n = string(&args[0]);
            return Ok(boolean(
                (1..=31).contains(&n.len())
                    && !n.iter().any(|c| [10, 13, 0x2028, 0x2029].contains(c)),
            ));
        }
        if kind == "secondary" {
            self.effect(
                kind,
                vec![
                    boolean(same(&args[0], &self.options)),
                    boolean(same(&args[1], &self.api)),
                    args[2].clone(),
                ],
            )?;
            let xhr = object(vec![("client", text("limited"))]);
            let value = self.call(
                get(&args[2], "rateLimit"),
                self.undefined(),
                vec![xhr.clone()],
            )?;
            assert!(
                Rc::ptr_eq(&value, &xhr),
                "native rate limiter original result"
            );
            return Ok(value);
        }
        if kind == "createAPI" {
            self.effect(
                kind,
                vec![
                    boolean(same(&args[0], &self.options)),
                    boolean(
                        self.instances
                            .borrow()
                            .iter()
                            .any(|r| Rc::ptr_eq(r, &args[1])),
                    ),
                ],
            )?;
            return Ok(object(vec![("client", text("unlimited"))]));
        }
        if kind == "on" {
            self.effect(
                kind,
                vec![
                    args[0].clone(),
                    text(match &*args[1] {
                        Raw::Named(n, _) => n,
                        _ => panic!("native fill member"),
                    }),
                ],
            )?;
            assert!(
                Rc::ptr_eq(&args[1], &get(&receiver, "handler"))
                    || self
                        .instances
                        .borrow()
                        .iter()
                        .any(|r| Rc::ptr_eq(&get(r, "handleFillForFastRefresh"), &args[1])),
                "native original fill binding"
            );
            return Ok(self.undefined());
        }
        self.effect(kind, args.clone())?;
        let out = match kind {
            "rateLimit" => args[0].clone(),
            "setTimeout" => {
                let id = self.next_timer.get();
                self.next_timer.set(id + 1.0);
                self.timers
                    .borrow_mut()
                    .push((id, num(&args[1]), args[0].clone()));
                self.number(id)
            }
            "clearTimeout" => {
                let id = num(&args[0]);
                self.timers.borrow_mut().retain(|(n, _, _)| *n != id);
                self.undefined()
            }
            "resolveMarketSymbol" => text("BTC"),
            "convertToWsSymbol" => text("e_btc"),
            "formatKlineTopic" => text("market_e_btc_kline_1min"),
            "mapKline" => array(vec![object(vec![
                ("value", self.number(1.0)),
                ("info", items(&args[0])[0].clone()),
            ])]),
            "readBlob" => {
                let Raw::Blob(t) = &*args[0] else {
                    panic!("blob")
                };
                let value = t.clone();
                let h = self.clone();
                return Ok(self.begin(Box::pin(async move {
                    let mut first = true;
                    std::future::poll_fn(move |_| {
                        if first {
                            first = false;
                            Poll::Pending
                        } else {
                            Poll::Ready(())
                        }
                    })
                    .await;
                    if flag(&h.cfg, "disposeAfterRead") {
                        set(&h.r, "isDisposed", boolean(true));
                    }
                    if flag(&h.cfg, "rejectBlob") {
                        Err(h.error.clone())
                    } else {
                        Ok(value)
                    }
                })));
            }
            "jsonParse" => {
                let value =
                    serde_json::from_str::<Value>(&String::from_utf16(&string(&args[0])).unwrap())
                        .map(|v| decode_plain(&v))
                        .unwrap_or(Rc::new(Raw::Null));
                *self.parsed.borrow_mut() = value.clone();
                value
            }
            "nanoid" => {
                let n = self.id.get() + 1;
                self.id.set(n);
                text(&format!("id{n}"))
            }
            "JSON.stringify" => {
                self.events.borrow_mut().pop();
                self.effect("stringify", args.clone())?;
                text(&json_string(&args[0]))
            }
            "calcOrderBookTotal" => {
                self.events.borrow_mut().pop();
                self.effect("totals", args.clone())?;
                for side in ["bids", "asks"] {
                    let mut total = self.number(0.0);
                    for level in items(&get(&args[0], side)) {
                        total = self.arithmetic(Op::Add, total, get(&level, "amount"))?;
                        set(&level, "total", total.clone());
                    }
                }
                self.undefined()
            }
            "console.log" => {
                self.events.borrow_mut().pop();
                self.effect("console", args.clone())?;
                self.undefined()
            }
            "resolve" => {
                assert!(
                    Rc::ptr_eq(&args[0], &self.parsed.borrow()),
                    "native original response frame"
                );
                if flag(&self.cfg, "reentry") {
                    set(
                        &get(&self.r, "resolveRequests"),
                        "topic",
                        array(vec![Rc::new(Raw::Function("inserted"))]),
                    );
                }
                self.undefined()
            }
            "bookCallback" => {
                if matches!(&**self.book.borrow(), Raw::Undefined) {
                    *self.book.borrow_mut() = args[0].clone()
                } else {
                    assert!(
                        Rc::ptr_eq(&self.book.borrow(), &args[0]),
                        "native retained book identity"
                    );
                }
                self.undefined()
            }
            "trackLastReceived"
            | "handlePingEvent"
            | "recordMessageHandlerStats"
            | "cleanupMessageHandlerStats"
            | "send"
            | "handler"
            | "foreign"
            | "candleCallback"
            | "updateTicker" => self.undefined(),
            _ => panic!("outside composition/streams binding {kind}"),
        };
        Ok(out)
    }
    fn function(&self, f: Callback<Self>) -> V {
        Rc::new(Raw::Callback(f))
    }
    fn map_series(&self, v: V, f: V) -> Result<V, E> {
        let h = self.clone();
        Ok(self.begin(Box::pin(async move {
            let mut out = vec![];
            for v in items(&v) {
                let p = h.call(f.clone(), h.undefined(), vec![v])?;
                out.push(h.await_value(p).await?);
            }
            Ok(array(out))
        })))
    }
    fn await_value(&self, v: V) -> impl Future<Output = Result<V, E>> {
        let mut first = true;
        std::future::poll_fn(move |_| {
            if first {
                first = false;
                return Poll::Pending;
            }
            let Raw::Promise(p) = &*v else {
                return Poll::Ready(Ok(v.clone()));
            };
            let mut p = p.borrow_mut();
            if let Some(r) = &p.result {
                return Poll::Ready(r.clone());
            }
            let result = p
                .future
                .as_mut()
                .unwrap()
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()));
            if let Poll::Ready(r) = &result {
                p.result = Some(r.clone());
                p.future = None;
            }
            result
        })
    }
    fn observe_detached(&self, v: V) {
        assert!(
            !matches!(&*v, Raw::Promise(_)),
            "detached promises beyond witness floor"
        );
    }
    fn sleep(&self, _ms: f64) -> Result<V, E> {
        panic!("sleep outside floor")
    }
    fn console(&self, m: &str, s: &str) -> Result<(), E> {
        self.effect(m, vec![text(s)])
    }
    fn error(&self, message: V) -> Result<E, E> {
        Ok(Rc::new(Raw::Error(
            if matches!(&*message, Raw::Undefined) {
                text("")
            } else {
                self.string(message)?
            },
        )))
    }
    fn error_value(&self, e: &E) -> V {
        e.clone()
    }
    fn position_id(&self, symbol: V, side: V, mode: V) -> Result<V, E> {
        if same(&mode, &text("oneway")) || !self.truthy(&side) {
            Ok(symbol)
        } else {
            self.concat(vec![symbol, text(":"), side])
        }
    }
}
fn decode_plain(v: &Value) -> V {
    match v {
        Value::Object(o) => Rc::new(Raw::Object(RefCell::new(
            o.iter().map(|(k, v)| (units(k), decode_plain(v))).collect(),
        ))),
        Value::Array(a) => array(a.iter().map(decode_plain).collect()),
        _ => decode(v),
    }
}
fn json_string(v: &V) -> String {
    match &**v {
        Raw::Undefined | Raw::Null => "null".into(),
        Raw::Number(n) => {
            if n.is_finite() {
                safe_cex_num::js_number_string(*n)
            } else {
                "null".into()
            }
        }
        Raw::Bool(b) => b.to_string(),
        Raw::Text(s) => serde_json::to_string(&String::from_utf16(s).unwrap()).unwrap(),
        Raw::Array(a) => format!(
            "[{}]",
            a.borrow()
                .iter()
                .map(json_string)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Raw::Object(_) => format!(
            "{{{}}}",
            ordered(v)
                .iter()
                .filter(|(_, v)| !matches!(&**v, Raw::Undefined))
                .map(|(k, v)| format!(
                    "{}:{}",
                    serde_json::to_string(&String::from_utf16(k).unwrap()).unwrap(),
                    json_string(v)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => panic!("JSON fixture floor"),
    }
}
impl StreamHost for Host {
    fn global(&self, n: &'static str) -> Result<V, E> {
        Ok(Rc::new(Raw::Function(n)))
    }
    fn dynamic(&self, v: V, k: V) -> Result<V, E> {
        if let Raw::Array(a) = &*v {
            let n = num(&k) as usize;
            return Ok(a
                .borrow()
                .get(n)
                .cloned()
                .unwrap_or_else(|| self.undefined()));
        }
        self.get(&v, &String::from_utf16(&string(&k)).unwrap())
    }
    fn put(&self, v: V, k: V, n: V) -> Result<(), E> {
        self.set(&v, &String::from_utf16(&string(&k)).unwrap(), n)
    }
    fn delete(&self, v: V, k: V) -> Result<(), E> {
        let Raw::Object(o) = &*v else { panic!("dict") };
        let k = string(&k);
        o.borrow_mut().retain(|(key, _)| *key != k);
        Ok(())
    }
    fn spread(&self, v: V) -> Result<Vec<V>, E> {
        Ok(items(&v))
    }
    fn new_set(&self) -> Result<V, E> {
        CompositionHost::new_set(self)
    }
    fn is_blob(&self, v: &V) -> Result<bool, E> {
        Ok(matches!(&**v, Raw::Blob(_)))
    }
    fn pair(&self, v: V) -> Result<(V, V), E> {
        let values = items(&v);
        Ok((
            values.first().cloned().unwrap_or_else(|| self.undefined()),
            values.get(1).cloned().unwrap_or_else(|| self.undefined()),
        ))
    }
    fn null(&self) -> V {
        Rc::new(Raw::Null)
    }
}
impl CompositionHost for Host {
    fn super_exchange(&self, a: Vec<V>) -> Result<V, E> {
        self.effect(
            "superExchange",
            vec![
                self.number(a.len() as f64),
                a[0].clone(),
                boolean(same(&a[1], &self.options)),
                boolean(same(&a[2], &self.store)),
            ],
        )?;
        let r = object(vec![
            ("name", a[0].clone()),
            ("options", a[1].clone()),
            ("store", a[2].clone()),
            (
                "emitter",
                object(vec![("on", Rc::new(Raw::Function("on")))]),
            ),
        ]);
        self.instances.borrow_mut().push(r.clone());
        Ok(r)
    }
    fn super_public(&self, a: Vec<V>) -> Result<V, E> {
        self.effect(
            "superPublic",
            vec![
                self.number(a.len() as f64),
                boolean(a.first().is_some_and(|v| same(v, &self.parent))),
            ],
        )?;
        let r = object(vec![(
            "parent",
            a.first().cloned().unwrap_or_else(|| self.undefined()),
        )]);
        self.instances.borrow_mut().push(r.clone());
        Ok(r)
    }
    fn define(&self, r: V, k: &str, v: V) -> Result<(), E> {
        self.set(&r, k, v)
    }
    fn new_set(&self) -> Result<V, E> {
        self.effect("newSet", vec![])?;
        Ok(Rc::new(Raw::Set(RefCell::new(vec![]))))
    }
    fn new_map(&self) -> Result<V, E> {
        self.effect("newMap", vec![])?;
        Ok(Rc::new(Raw::Map(RefCell::new(vec![]))))
    }
    fn rule(&self, l: &str, p: &str) -> Result<V, E> {
        self.effect("rule", vec![text(l), text(p)])?;
        let rule = object(vec![
            ("label", text(l)),
            ("requirement", text(&format!("match /{p}/"))),
            ("isValid", Rc::new(Raw::Function("isValid"))),
        ]);
        if matches!(&**self.first_rule.borrow(), Raw::Undefined) {
            *self.first_rule.borrow_mut() = rule.clone()
        }
        Ok(rule)
    }
    fn grafana(&self, which: &str) -> Result<V, E> {
        Ok(if which == "periodic" {
            self.periodic.clone()
        } else {
            self.every.clone()
        })
    }
    fn binding(&self, n: &'static str) -> Result<V, E> {
        Ok(if n == "createAPI" {
            self.api.clone()
        } else {
            Rc::new(Raw::Function(n))
        })
    }
    fn construct_socket(&self, w: &'static str, r: V) -> Result<V, E> {
        self.effect(
            if w == "public" {
                "publicSocket"
            } else {
                "privateSocket"
            },
            vec![boolean(self.instances.borrow().iter().any(|v| same(v, &r)))],
        )?;
        Ok(Rc::new(Raw::Socket(w, r)))
    }
    fn member(&self, n: &'static str, target: &'static str, r: V) -> V {
        assert_eq!(
            target,
            expected_target(n, self.group == "public_class"),
            "native independent class ABI {n}"
        );
        let owner = self.clone();
        Rc::new(Raw::Named(
            n.into(),
            Rc::new(move |_| {
                let _lexical = &r;
                if n == "ping" {
                    safe_cex_venue_bitrue::ws_public::ping();
                    Ok(owner.undefined())
                } else {
                    panic!("member call beyond witnessed floor {target}")
                }
            }),
        ))
    }
    fn named_function(&self, n: &'static str, f: Callback<Self>) -> V {
        Rc::new(Raw::Named(n.into(), f))
    }
}

fn expected_target(name: &str, public: bool) -> &'static str {
    match (public, name) {
        (false, "isSuccessCode") => "safe-cex-venue-bitrue/src/read_helpers.rs::is_success_code",
        (false, "parseMaxLeverageFromErrorMessage") => {
            "safe-cex-venue-bitrue/src/read_helpers.rs::parse_max_leverage_from_error_message"
        }
        (false, "setMarketMaxLeverage") => {
            "safe-cex-venue-bitrue/src/read_helpers.rs::set_market_max_leverage"
        }
        (false, "parseMaxLeverageFromBracketResponse") => {
            "safe-cex-venue-bitrue/src/read_helpers.rs::parse_max_leverage_from_bracket_response"
        }
        (false, "fetchAndSyncSymbolMaxLeverage") => {
            "safe-cex-venue-bitrue/src/leverage_reads.rs::fetch_and_sync_symbol_max_leverage"
        }
        (false, "normalizeTimestampMs") => {
            "safe-cex-venue-bitrue/src/read_helpers.rs::normalize_timestamp_ms"
        }
        (false, "normalizeTradeSymbol") => {
            "safe-cex-venue-bitrue/src/read_helpers.rs::normalize_trade_symbol"
        }
        (false, "pickTradeOrderId") => {
            "safe-cex-venue-bitrue/src/read_helpers.rs::pick_trade_order_id"
        }
        (false, "resolveTradeQty") => {
            "safe-cex-venue-bitrue/src/read_helpers.rs::resolve_trade_qty"
        }
        (false, "fetchBitrueTradeRows") => {
            "safe-cex-venue-bitrue/src/collection_reads.rs::trade_rows"
        }
        (false, "dispose") => "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::dispose",
        (false, "handleFillForFastRefresh") => {
            "safe-cex-venue-bitrue/src/refresh_entrypoints.rs::handle_fill_for_fast_refresh"
        }
        (false, "queueFastPrivateRefresh") => {
            "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::queue_refresh"
        }
        (false, "runFastPrivateRefresh") => {
            "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::run_refresh"
        }
        (false, "validateAccount") => "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::validate",
        (false, "start") => "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::start",
        (false, "updateOrder") => "safe-cex-venue-bitrue/src/trading_rest.rs::update",
        (false, "tick") => "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::tick",
        (false, "fetchBalanceAndPositionsRaw") => {
            "safe-cex-venue-bitrue/src/account_reads.rs::fetch_balance_and_positions_raw"
        }
        (false, "fetchBalance") => "safe-cex-venue-bitrue/src/account_reads.rs::fetch_balance",
        (false, "fetchOpenPositions") => {
            "safe-cex-venue-bitrue/src/collection_reads.rs::open_positions"
        }
        (false, "getPositionAccounts") => {
            "safe-cex-venue-bitrue/src/account_helpers.rs::get_position_accounts"
        }
        (false, "getAccountPositionSnapshots") => {
            "safe-cex-venue-bitrue/src/account_helpers.rs::get_account_position_snapshots"
        }
        (false, "fetchPositionMetadata") => {
            "safe-cex-venue-bitrue/src/position_metadata.rs::fetch_position_metadata"
        }
        (false, "fetchPositions") => "safe-cex-venue-bitrue/src/collection_reads.rs::positions",
        (false, "fetchBalanceAndPositions") => {
            "safe-cex-venue-bitrue/src/collection_reads.rs::balance_positions"
        }
        (false, "parsePositions") => {
            "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::parse_positions"
        }
        (false, "parseBalance") => "safe-cex-venue-bitrue/src/balance.rs::parse_balance",
        (false, "fetchOrders") => "safe-cex-venue-bitrue/src/collection_reads.rs::orders",
        (false, "fetchAndEmitOrders") => {
            "safe-cex-venue-bitrue/src/collection_reads.rs::emit_orders"
        }
        (false, "fetchOrdersForSymbol") => {
            "safe-cex-venue-bitrue/src/collection_reads.rs::symbol_orders"
        }
        (false, "fetchHistoricalTradesInternal") => {
            "safe-cex-venue-bitrue/src/collection_reads.rs::history"
        }
        (false, "fetchHistoricalFills") => "safe-cex-venue-bitrue/src/collection_reads.rs::fills",
        (false, "fetchMarkets") => "safe-cex-venue-bitrue/src/trading_rest.rs::markets",
        (false, "fetchOHLCV") => "safe-cex-venue-bitrue/src/trading_rest.rs::candles",
        (false, "formatCreateOrder") => "safe-cex-venue-bitrue/src/trading_rest.rs::format_order",
        (false, "placeOrder") => "safe-cex-venue-bitrue/src/trading_rest.rs::place",
        (false, "placeOrders") => "safe-cex-venue-bitrue/src/trading_rest.rs::places",
        (false, "placeOrderBatch") => "safe-cex-venue-bitrue/src/trading_rest.rs::batch",
        (false, "fetchSymbolMaxLeverage") => {
            "safe-cex-venue-bitrue/src/leverage_reads.rs::fetch_symbol_max_leverage"
        }
        (false, "updateSymbolMetadata") => {
            "safe-cex-venue-bitrue/src/leverage_reads.rs::update_symbol_metadata"
        }
        (false, "setLeverage") => "safe-cex-venue-bitrue/src/trading_rest.rs::leverage",
        (false, "changeMarginType") => {
            "safe-cex-venue-bitrue/src/refresh_entrypoints.rs::change_margin_type"
        }
        (false, "cancelOrders") => "safe-cex-venue-bitrue/src/trading_rest.rs::cancel",
        (false, "listenOHLCV") => "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::listen_candles",
        (false, "listenOrderBook") => "safe-cex-venue-bitrue/src/runtime_lifecycle.rs::listen_book",
        (true, "resolveMarketSymbol") => {
            "safe-cex-venue-bitrue/src/ws_sync.rs::resolve_market_symbol"
        }
        (true, "convertToWsSymbol") => {
            "safe-cex-venue-bitrue/src/ws_symbol_helpers.rs::convert_to_ws_symbol"
        }
        (true, "ensureTickerSubscription") => {
            "safe-cex-venue-bitrue/src/public_bodies.rs::ensure_ticker_subscription"
        }
        (true, "connectAndSubscribe") => {
            "safe-cex-venue-bitrue/src/public_bodies.rs::connect_and_subscribe"
        }
        (true, "onOpen") => "safe-cex-venue-bitrue/src/public_bodies.rs::on_open",
        (true, "onMarketsChanged") => {
            "safe-cex-venue-bitrue/src/public_bodies.rs::on_markets_changed"
        }
        (true, "handlePingEvent") => "safe-cex-venue-bitrue/src/ws_sync.rs::handle_public_ping",
        (true, "ping") => "safe-cex-venue-bitrue/src/ws_public.rs::ping",
        (true, "subscribe") => "safe-cex-venue-bitrue/src/public_bodies.rs::subscribe",
        (true, "formatKlineTopic") => {
            "safe-cex-venue-bitrue/src/ws_symbol_helpers.rs::format_kline_topic"
        }
        (true, "mapKline") => "safe-cex-venue-bitrue/src/public_bodies.rs::map_kline",
        (true, "listenOHLCV") => "safe-cex-venue-bitrue/src/public_streams.rs::listen_candles",
        (true, "onMessage") => "safe-cex-venue-bitrue/src/public_streams.rs::message",
        (true, "handleTickersEvent") => {
            "safe-cex-venue-bitrue/src/public_bodies.rs::handle_tickers_event"
        }
        (true, "makeRequest") => "safe-cex-venue-bitrue/src/public_bodies.rs::make_request",
        (true, "listenOrderBook") => "safe-cex-venue-bitrue/src/public_streams.rs::listen_book",
        _ => panic!("unexpected class method {name}"),
    }
}
fn setup(group: &str, cfg: V) -> Host {
    let undef = Rc::new(Raw::Undefined);
    let r = object(vec![
        ("isConnected", get(&cfg, "connected")),
        ("isDisposed", get(&cfg, "disposed")),
        ("topics", object(vec![])),
        ("messageHandlers", object(vec![])),
        ("resolveRequests", object(vec![])),
        (
            "parent",
            object(vec![(
                "store",
                object(vec![(
                    "updateTicker",
                    Rc::new(Raw::Function("updateTicker")),
                )]),
            )]),
        ),
    ]);
    for n in [
        "resolveMarketSymbol",
        "convertToWsSymbol",
        "formatKlineTopic",
        "mapKline",
        "trackLastReceived",
        "handlePingEvent",
        "recordMessageHandlerStats",
        "cleanupMessageHandlerStats",
        "send",
    ] {
        set(&r, n, Rc::new(Raw::Function(n)));
    }
    Host(Rc::new(State {
        cfg,
        r,
        error: object(vec![("message", text("original failure"))]),
        events: RefCell::new(vec![]),
        next_timer: Cell::new(100.0),
        timers: RefCell::new(vec![]),
        id: Cell::new(0),
        options: object(vec![("key", text("key"))]),
        store: object(vec![("tag", text("store"))]),
        parent: object(vec![("tag", text("parent"))]),
        api: Rc::new(Raw::Function("createAPI")),
        periodic: array(
            safe_cex_venue_bitrue::types::GRAFANA_ENDPOINTS_PERIODIC
                .iter()
                .map(|x| text(x))
                .collect(),
        ),
        every: array(
            safe_cex_venue_bitrue::types::GRAFANA_ENDPOINTS_EVERY
                .iter()
                .map(|x| text(x))
                .collect(),
        ),
        instances: RefCell::new(vec![]),
        first_rule: RefCell::new(undef.clone()),
        parsed: RefCell::new(undef.clone()),
        book: RefCell::new(undef),
        group: group.into(),
    }))
}
fn settle(f: impl Future<Output = Result<V, E>>) -> Result<V, E> {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return r;
        }
    }
    panic!("native finite promise floor did not settle")
}
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/composition-streams-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "source oracle startup {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn fire(h: &Host, ms: f64, returns: &mut Vec<V>) -> Result<(), E> {
    let selected = {
        let mut timers = h.timers.borrow_mut();
        timers
            .iter()
            .position(|(_, delay, _)| *delay == ms)
            .map(|i| timers.remove(i))
    };
    if let Some((_, _, fn_)) = selected {
        returns.push(h.call(fn_, h.undefined(), vec![])?);
    }
    Ok(())
}
fn frame(h: &Host, group: &str, returns: &mut Vec<V>) -> Result<(), E> {
    let handlers = get(&h.r, "messageHandlers");
    let target = ordered(&handlers)
        .into_iter()
        .find(|(_, list)| !items(list).is_empty());
    if let Some((_, list)) = target {
        let values = items(&list);
        for f in values {
            let value = if group == "book" {
                get(&h.cfg, "depth")
            } else {
                object(vec![("price", h.number(1.0))])
            };
            returns.push(h.call(f, h.undefined(), vec![value])?);
        }
    }
    Ok(())
}
fn shape(r: &V) -> V {
    array(
        ordered(r)
            .iter()
            .map(|(k, v)| {
                object(vec![
                    ("key", Rc::new(Raw::Text(k.clone()))),
                    ("writable", boolean(true)),
                    ("enumerable", boolean(true)),
                    ("configurable", boolean(true)),
                    ("value", v.clone()),
                ])
            })
            .collect(),
    )
}
fn lengths(dict: &V) -> V {
    Rc::new(Raw::Object(RefCell::new(
        ordered(dict)
            .into_iter()
            .map(|(k, v)| (k, Rc::new(Raw::Number(items(&v).len() as f64))))
            .collect(),
    )))
}
fn run(group: &str, row: &Value) {
    let h = setup(group, decode(&row["cfg"]));
    let id = String::from_utf16(&string(&get(&h.cfg, "id"))).unwrap();
    let path = format!("{group}/{id}");
    let mut identity = h.undefined();
    let mut returns = vec![];
    let mut retained_result = h.undefined();
    let outcome = (|| -> Result<V, E> {
        if matches!(group, "exchange_class" | "constructor" | "public_class") {
            let public = group == "public_class";
            let args = if public {
                if flag(&h.cfg, "noArgs") {
                    vec![]
                } else {
                    vec![h.parent.clone(), object(vec![("extra", h.number(1.0))])]
                }
            } else {
                let mut a = vec![text("Bitrue"), h.options.clone(), h.store.clone()];
                if flag(&h.cfg, "extra") {
                    a.push(h.number(99.0))
                }
                a
            };
            let first = if public {
                composition::construct_public(&h, args.clone())?
            } else {
                composition::construct_exchange(&h, args.clone())?
            };
            if group == "exchange_class" {
                native(
                    &get(&first, "inProgressOrderFetchId"),
                    &serde_json::json!({"number":"0000000000000000"}),
                    &format!("{path}.initial-id"),
                );
            }
            let result = shape(&first);
            identity = object(vec![]);
            if !public {
                set(
                    &identity,
                    "rule",
                    boolean(Rc::ptr_eq(
                        &get(&first, "clientOrderIdRule"),
                        &h.first_rule.borrow(),
                    )),
                );
                set(
                    &identity,
                    "periodic",
                    boolean(Rc::ptr_eq(
                        &get(&first, "grafanaEndpointsPeriodic"),
                        &h.periodic,
                    )),
                );
                set(
                    &identity,
                    "every",
                    boolean(Rc::ptr_eq(&get(&first, "grafanaEndpointsEvery"), &h.every)),
                );
            }
            if flag(&h.cfg, "pair") {
                if !public {
                    let Raw::Array(a) = &*h.periodic else {
                        panic!("periodic array")
                    };
                    a.borrow_mut().push(text("added"));
                }
                let second = if public {
                    composition::construct_public(&h, args)?
                } else {
                    composition::construct_exchange(&h, args)?
                };
                set(
                    &identity,
                    "fresh",
                    boolean(
                        ordered(&first)
                            .iter()
                            .filter(|(_, v)| {
                                matches!(
                                    &**v,
                                    Raw::Named(_, _) | Raw::Callback(_) | Raw::Function(_)
                                )
                            })
                            .all(|(k, v)| !same(v, &get(&second, &String::from_utf16(k).unwrap()))),
                    ),
                );
                let keys = if public {
                    vec![
                        "topics",
                        "messageHandlers",
                        "tickerSymbols",
                        "resolveRequests",
                    ]
                } else {
                    vec![
                        "prioritySymbols",
                        "pendingOrderRefreshSymbols",
                        "positionMetadataBySymbol",
                        "recentWsRequests",
                    ]
                };
                set(
                    &identity,
                    "collections",
                    boolean(
                        keys.iter()
                            .all(|k| !Rc::ptr_eq(&get(&first, k), &get(&second, k))),
                    ),
                );
                if public {
                    let out = h.call(get(&first, "ping"), second, vec![])?;
                    set(&identity, "void", boolean(matches!(&*out, Raw::Undefined)));
                } else {
                    set(
                        &identity,
                        "alias",
                        boolean(Rc::ptr_eq(
                            &get(&second, "grafanaEndpointsPeriodic"),
                            &h.periodic,
                        )),
                    );
                    let rule = get(&first, "clientOrderIdRule");
                    let mut values = vec![];
                    for v in [
                        "".into(),
                        "x".into(),
                        "x".repeat(31),
                        "x".repeat(32),
                        "x\ny".into(),
                        "💡".repeat(16),
                    ] {
                        values.push(h.call(get(&rule, "isValid"), rule.clone(), vec![text(&v)])?);
                    }
                    set(&identity, "ruleValues", array(values));
                    let Raw::Array(a) = &*h.periodic else {
                        panic!("periodic")
                    };
                    a.borrow_mut().pop();
                }
            }
            Ok(result)
        } else if group == "message" {
            set(
                &get(&h.r, "resolveRequests"),
                "topic",
                array(vec![
                    Rc::new(Raw::Function("resolve")),
                    Rc::new(Raw::Function("resolve")),
                ]),
            );
            set(
                &get(&h.r, "messageHandlers"),
                "topic",
                array(vec![
                    Rc::new(Raw::Function("handler")),
                    Rc::new(Raw::Function("handler")),
                ]),
            );
            let data = get(&h.cfg, "frameText");
            let data = if flag(&h.cfg, "blob") {
                Rc::new(Raw::Blob(data))
            } else {
                data
            };
            settle(streams::message(
                &h,
                h.r.clone(),
                object(vec![("data", data)]),
            ))
        } else {
            let topic = if group == "candles" {
                "market_e_btc_kline_1min"
            } else {
                "market_e_btc_simple_depth_step0"
            };
            if flag(&h.cfg, "foreign") {
                set(
                    &get(&h.r, "messageHandlers"),
                    topic,
                    array(vec![Rc::new(Raw::Function("foreign"))]),
                );
            }
            let count = get(&h.cfg, "count");
            if !matches!(&*count, Raw::Undefined) {
                set(&get(&h.r, "topics"), topic, count);
            }
            let callback = Rc::new(Raw::Function(if group == "book" {
                "bookCallback"
            } else {
                "candleCallback"
            }));
            let cleanup = if group == "candles" {
                streams::listen_candles(&h, h.r.clone(), get(&h.cfg, "opts"), callback)?
            } else {
                streams::listen_book(&h, h.r.clone(), get(&h.cfg, "symbol"), callback)?
            };
            retained_result = cleanup.clone();
            let clean = num(&get(&h.cfg, "cleanup"));
            if flag(&h.cfg, "connectLater")
                && matches!(id.as_str(), "cleanup-before-connect" | "cancel-wait")
            {
                for _ in 0..clean as usize {
                    returns.push(h.call(cleanup.clone(), h.undefined(), vec![])?);
                }
                set(&h.r, "isConnected", boolean(true));
                fire(&h, 100.0, &mut returns)?;
            } else if flag(&h.cfg, "connectLater") {
                set(&h.r, "isConnected", boolean(true));
                fire(&h, 100.0, &mut returns)?;
            }
            if flag(&h.cfg, "frame") {
                frame(&h, group, &mut returns)?;
            }
            if flag(&h.cfg, "fireFallback") {
                fire(&h, 3000.0, &mut returns)?;
            }
            if flag(&h.cfg, "frame") && flag(&h.cfg, "fireFallback") {
                frame(&h, group, &mut returns)?;
            }
            if !(flag(&h.cfg, "connectLater")
                && matches!(id.as_str(), "cleanup-before-connect" | "cancel-wait"))
            {
                for _ in 0..clean as usize {
                    returns.push(h.call(cleanup.clone(), h.undefined(), vec![])?);
                }
            }
            Ok(cleanup)
        }
    })();
    let (result, error) = match outcome {
        Ok(v) => (v, h.undefined()),
        Err(e) => (
            retained_result,
            object(vec![
                ("identity", boolean(Rc::ptr_eq(&e, &h.error))),
                ("message", get(&e, "message")),
            ]),
        ),
    };
    let state = if matches!(group, "message" | "candles" | "book") {
        object(vec![
            ("disposed", get(&h.r, "isDisposed")),
            ("connected", get(&h.r, "isConnected")),
            ("topics", get(&h.r, "topics")),
            ("handlers", lengths(&get(&h.r, "messageHandlers"))),
            ("requests", lengths(&get(&h.r, "resolveRequests"))),
            (
                "timers",
                array(
                    h.timers
                        .borrow()
                        .iter()
                        .map(|(id, ms, _)| {
                            object(vec![("id", h.number(*id)), ("ms", h.number(*ms))])
                        })
                        .collect(),
                ),
            ),
            ("book", h.book.borrow().clone()),
        ])
    } else {
        h.undefined()
    };
    native(&array(h.events.borrow().clone()), &row["events"], &path);
    native(&result, &row["result"], &path);
    native(&identity, &row["identity"], &path);
    native(&error, &row["error"], &path);
    native(&state, &row["state"], &path);
    native(&array(returns), &row["callbackReturns"], &path);
}
fn family(g: &str, n: usize) {
    let cases = oracle()["rows"][g].as_array().unwrap();
    assert_eq!(cases.len(), n, "independent finite floor {g}");
    for c in cases {
        run(g, c)
    }
}
#[test]
fn exchange_class_native() {
    family("exchange_class", 4)
}
#[test]
fn constructor_native() {
    family("constructor", 6)
}
#[test]
fn public_class_native() {
    family("public_class", 4)
}
#[test]
fn message_native() {
    family("message", 15)
}
#[test]
fn candles_native() {
    family("candles", 8)
}
#[test]
fn book_native() {
    family("book", 13)
}
