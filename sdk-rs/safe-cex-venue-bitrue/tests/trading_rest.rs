use safe_cex_venue_bitrue::collection_reads::{Callback, Op, ReadHost};
use safe_cex_venue_bitrue::trading_rest::{self as trading, AsyncCallback, TradingHost};
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
    Async(AsyncCallback<Host>),
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
        Raw::Function(_) | Raw::Callback(_) | Raw::Async(_) | Raw::Promise(_) => {
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
        Raw::Callback(_) | Raw::Async(_) => panic!("callback outside observation floor"),
        Raw::Promise(_) => panic!("unsettled native value"),
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
        Raw::Callback(f) => Raw::Callback(f.clone()),
        Raw::Async(f) => Raw::Async(f.clone()),
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
    detached: RefCell<Vec<V>>,
    pending: RefCell<Vec<(usize, Rc<RefCell<Option<Result<V, E>>>>)>>,
    background: RefCell<Vec<Task>>,
    handled: RefCell<Vec<E>>,
    uuid: Cell<usize>,
    post: Cell<usize>,
    formats: Cell<usize>,
    sleeps: Cell<usize>,
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
    fn hook(&self, kind: &str) {
        let hook = get(&self.cfg, "hook");
        if kind == "placeOrderBatch" && same(&hook, &text("place")) {
            set(&get(&self.cfg, "opts"), "type", text("stop_market"));
        }
        if kind == "cancelOrders" && same(&hook, &text("update")) {
            set(
                &self.r,
                "placeOrder",
                Rc::new(Raw::Function("placeOrderChanged")),
            );
        }
        if kind == "makeRequest" && same(&hook, &text("candles")) {
            set(
                &get(&self.r, "publicWebsocket"),
                "mapKline",
                Rc::new(Raw::Function("mapKlineChanged")),
            );
        }
        if kind == "sleep" {
            self.sleeps.set(self.sleeps.get() + 1);
            if same(
                &self.number(self.sleeps.get() as f64),
                &get(&self.cfg, "connectAfter"),
            ) {
                set(
                    &get(&get(&self.r, "publicWebsocket"), "ws"),
                    "readyState",
                    self.number(1.0),
                );
            }
        }
    }
    fn returned(&self, kind: &'static str, out: Result<V, E>) -> V {
        let h = self.clone();
        self.begin(Box::pin(async move {
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
            if out.is_ok() {
                h.hook(kind)
            }
            out
        }))
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
        if let Raw::Callback(f) = &*f {
            return f(args);
        }
        if let Raw::Async(f) = &*f {
            return Ok(self.begin(f(args)));
        }
        let Raw::Function(kind) = &*f else {
            return Err(self.error.clone());
        };
        let kind = *kind;
        if kind == "post" {
            let index = self.post.get();
            self.post.set(index + 1);
            let label = get(&receiver, "label");
            assert!(
                same(
                    &receiver,
                    &get(&self.r, &String::from_utf16(&string(&label)).unwrap())
                ),
                "native original post receiver"
            );
            self.effect(
                "post",
                vec![
                    label,
                    self.number(index as f64),
                    args[0].clone(),
                    args[1].clone(),
                ],
            )?;
            let ready = Rc::new(RefCell::new(None));
            self.pending.borrow_mut().push((index, ready.clone()));
            return Ok(self.begin(Box::pin(std::future::poll_fn(move |_| {
                ready
                    .borrow_mut()
                    .take()
                    .map(Poll::Ready)
                    .unwrap_or(Poll::Pending)
            }))));
        }
        if kind == "get" {
            assert!(
                Rc::ptr_eq(&receiver, &get(&self.r, "unlimitedXHR")),
                "native market unlimited client"
            );
            let out = self
                .effect("get", vec![text("unlimitedXHR"), args[0].clone()])
                .map(|()| object(vec![("data", array(vec![get(&self.cfg, "market")]))]));
            return Ok(self.returned("get", out));
        }
        let effect = self.effect(kind, args.clone());
        let asynchronous = matches!(
            kind,
            "makeRequest"
                | "placeOrderBatch"
                | "cancelOrders"
                | "placeOrder"
                | "placeOrderChanged"
                | "callMethodInWorker"
        );
        let out = effect.and_then(|()| {
            Ok(match kind {
                "formatKlineTopic" => text("topic"),
                "mapKline" | "mapKlineChanged" => {
                    assert!(
                        Rc::ptr_eq(&args[0], &get(&self.cfg, "result")),
                        "native original raw candle argument"
                    );
                    get(&self.cfg, "result")
                }
                "resolveCallerClientOrderId" => get(&self.cfg, "callerId"),
                "assertSingleNativeClientOrderId" => self.undefined(),
                "assertCallerClientOrderIds" => {
                    assert!(
                        Rc::ptr_eq(&args[0], &get(&self.cfg, "orders")),
                        "native original orders assertion"
                    );
                    self.undefined()
                }
                "formatCreateOrder" => {
                    self.formats.set(self.formats.get() + 1);
                    let a = get(&self.cfg, "formatted");
                    if flag(&self.cfg, "reentrant") && self.formats.get() == 2 {
                        self.push(a.clone(), object(vec![("late", boolean(true))]))?;
                    }
                    a
                }
                "placeOrderBatch" => get(&self.cfg, "batchResult"),
                "cancelOrders" => {
                    assert!(
                        Rc::ptr_eq(&items(&args[0])[0], &get(&self.cfg, "order")),
                        "native original cancelled order"
                    );
                    self.undefined()
                }
                "placeOrder" | "placeOrderChanged" => get(&self.cfg, "batchResult"),
                "makeRequest" | "callMethodInWorker" => get(&self.cfg, "result"),
                "isSuccessCode" => boolean(
                    [self.number(0.0), text("0"), self.number(200.0), text("200")]
                        .iter()
                        .any(|v| same(v, &args[0])),
                ),
                "delete" | "setMarketMaxLeverage" | "emitAndLogError" => self.undefined(),
                "parseMaxLeverageFromErrorMessage" => {
                    safe_cex_venue_bitrue::read_helpers::parse_max_leverage_from_error_message(
                        Some(&String::from_utf16(&string(&args[0])).unwrap()),
                    )
                    .map(|n| self.number(n))
                    .unwrap_or(Rc::new(Raw::Null))
                }
                _ => panic!("outside binding floor {kind}"),
            })
        });
        if asynchronous {
            Ok(self.returned(kind, out))
        } else {
            out
        }
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
        self.detached.borrow_mut().push(v)
    }
    fn sleep(&self, ms: f64) -> Result<V, E> {
        let out = self
            .effect("sleep", vec![self.number(ms)])
            .map(|()| self.undefined());
        Ok(self.returned("sleep", out))
    }
    fn console(&self, m: &str, s: &str) -> Result<(), E> {
        self.effect(m, vec![text(s)])
    }
    fn error(&self, message: V) -> Result<E, E> {
        Ok(object(vec![(
            "message",
            if matches!(&*message, Raw::Undefined) {
                text("")
            } else {
                self.string(message)?
            },
        )]))
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
impl TradingHost for Host {
    fn key(&self, v: &V, k: V) -> Result<V, E> {
        let key = String::from_utf16(&string(&k)).unwrap();
        if let Raw::Array(a) = &**v {
            return Ok(key
                .parse::<usize>()
                .ok()
                .and_then(|i| a.borrow().get(i).cloned())
                .unwrap_or_else(|| self.undefined()));
        }
        self.get(v, &key)
    }
    fn set_dynamic(&self, v: &V, k: V, n: V) -> Result<(), E> {
        self.set(v, &String::from_utf16(&string(&k)).unwrap(), n)
    }
    fn null_is(&self, v: &V) -> bool {
        matches!(&**v, Raw::Null)
    }
    fn has(&self, v: V, k: &str) -> Result<bool, E> {
        Ok(ordered(&v).iter().any(|(key, _)| *key == units(k)))
    }
    fn flat_map(&self, v: V, mut f: impl FnMut(V) -> Result<V, E>) -> Result<V, E> {
        let mut out = vec![];
        for v in items(&v) {
            let mapped = f(v)?;
            if self.is_array(&mapped) {
                out.extend(items(&mapped))
            } else {
                out.push(mapped)
            }
        }
        Ok(array(out))
    }
    fn find(&self, v: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        for x in items(&v) {
            if f(x.clone())? {
                return Ok(x);
            }
        }
        Ok(self.undefined())
    }
    fn copy(&self, v: V) -> Result<V, E> {
        Ok(Rc::new(Raw::Object(RefCell::new(if self.nullish(&v) {
            vec![]
        } else {
            ordered(&v)
        }))))
    }
    fn remove_key(&self, v: V, key: &str) -> Result<(), E> {
        let Raw::Object(o) = &*v else {
            panic!("copy object")
        };
        o.borrow_mut().retain(|(k, _)| *k != units(key));
        Ok(())
    }
    fn chunk(&self, v: V, n: usize) -> Result<V, E> {
        Ok(array(
            items(&v).chunks(n).map(|v| array(v.to_vec())).collect(),
        ))
    }
    fn times(&self, n: V, mut f: impl FnMut() -> Result<V, E>) -> Result<V, E> {
        let n = num(&n).trunc();
        assert!(n.is_finite() && n <= 100.0, "ordinary bounded times floor");
        Ok(array(
            (0..n.max(0.0) as usize)
                .map(|_| f())
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
    fn push(&self, v: V, item: V) -> Result<(), E> {
        let Raw::Array(a) = &*v else {
            panic!("source array push")
        };
        a.borrow_mut().push(item);
        Ok(())
    }
    fn ceil(&self, v: V) -> Result<V, E> {
        Ok(self.number(num(&v).ceil()))
    }
    fn remainder(&self, a: V, b: V) -> Result<V, E> {
        Ok(self.number(num(&a) % num(&b)))
    }
    fn power(&self, a: V, b: V) -> Result<V, E> {
        Ok(self.number(num(&a).powf(num(&b))))
    }
    fn adjust(&self, v: V, step: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::adjust(num(&v), num(&step))))
    }
    fn parse_float(&self, v: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::parse_float(
            &String::from_utf16(&string(&v)).unwrap(),
        )))
    }
    fn uuid(&self) -> Result<V, E> {
        let n = self.uuid.get() + 1;
        self.uuid.set(n);
        let value = text(&format!("uuid-{n}"));
        self.effect("uuid", vec![value.clone()])?;
        Ok(value)
    }
    fn inverse_order_type(&self, k: V) -> Result<V, E> {
        let key = String::from_utf16(&string(&k)).unwrap();
        Ok(safe_cex_venue_bitrue::types::ORDER_TYPE
            .iter()
            .find(|(_, v)| serde_json::to_value(v).unwrap() == key)
            .map(|(k, _)| text(k))
            .unwrap_or_else(|| self.undefined()))
    }
    fn now(&self) -> Result<V, E> {
        self.effect("now", vec![])?;
        Ok(get(&self.cfg, "now"))
    }
    fn async_function(&self, f: AsyncCallback<Self>) -> V {
        Rc::new(Raw::Async(f))
    }
    fn promise_all(&self, v: V) -> Result<V, E> {
        let h = self.clone();
        let mut tasks: Vec<Option<Task>> = items(&v)
            .into_iter()
            .map(|p| {
                let h = h.clone();
                Some(Box::pin(async move { h.await_value(p).await }) as Task)
            })
            .collect();
        let mut results: Vec<Option<V>> = vec![None; tasks.len()];
        Ok(self.begin(Box::pin(std::future::poll_fn(move |_| {
            for i in 0..tasks.len() {
                if let Some(f) = &mut tasks[i] {
                    match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                        Poll::Ready(Ok(v)) => {
                            results[i] = Some(v);
                            tasks[i] = None;
                        }
                        Poll::Ready(Err(e)) => {
                            tasks[i] = None;
                            h.background
                                .borrow_mut()
                                .extend(tasks.iter_mut().filter_map(Option::take));
                            return Poll::Ready(Err(e));
                        }
                        Poll::Pending => {}
                    }
                }
            }
            if tasks.iter().all(Option::is_none) {
                Poll::Ready(Ok(array(
                    results.iter().map(|v| v.clone().unwrap()).collect(),
                )))
            } else {
                Poll::Pending
            }
        }))))
    }
}
fn setup(cfg: &Value) -> Host {
    let cfg = decode(cfg);
    let market = get(&cfg, "market");
    let store = object(vec![
        (
            "marketsBySymbol",
            if flag(&cfg, "noMarket") {
                object(vec![])
            } else {
                object(vec![("E-BTC-USDT", market)])
            },
        ),
        ("positions", get(&cfg, "positions")),
        (
            "tickersBySymbol",
            object(vec![("E-BTC-USDT", get(&cfg, "ticker"))]),
        ),
    ]);
    let client = |label: &str| {
        object(vec![
            ("label", text(label)),
            ("get", Rc::new(Raw::Function("get"))),
            ("post", Rc::new(Raw::Function("post"))),
        ])
    };
    let ws = object(vec![
        (
            "ws",
            if flag(&cfg, "socket") {
                object(vec![("readyState", get(&cfg, "ready"))])
            } else {
                Rc::new(Raw::Undefined)
            },
        ),
        (
            "formatKlineTopic",
            Rc::new(Raw::Function("formatKlineTopic")),
        ),
        ("makeRequest", Rc::new(Raw::Function("makeRequest"))),
        ("mapKline", Rc::new(Raw::Function("mapKline"))),
    ]);
    let mut fields = vec![
        ("store", store),
        ("xhr", client("xhr")),
        ("unlimitedXHR", client("unlimitedXHR")),
        ("publicWebsocket", ws),
        (
            "recentWsRequests",
            object(vec![(
                "topic",
                object(vec![
                    ("time", get(&cfg, "cacheTime")),
                    ("data", get(&cfg, "cached")),
                ]),
            )]),
        ),
        (
            "positionMetadataBySymbol",
            object(vec![("delete", Rc::new(Raw::Function("delete")))]),
        ),
    ];
    for name in [
        "resolveCallerClientOrderId",
        "assertSingleNativeClientOrderId",
        "assertCallerClientOrderIds",
        "formatCreateOrder",
        "placeOrderBatch",
        "cancelOrders",
        "placeOrder",
        "callMethodInWorker",
        "isSuccessCode",
        "emitAndLogError",
        "parseMaxLeverageFromErrorMessage",
        "setMarketMaxLeverage",
    ] {
        fields.push((name, Rc::new(Raw::Function(name))))
    }
    let empty_error = flag(&cfg, "emptyError");
    Host(Rc::new(State {
        r: object(fields),
        cfg,
        error: if empty_error {
            object(vec![])
        } else {
            object(vec![
                ("message", text("original failure")),
                (
                    "response",
                    object(vec![("data", object(vec![("msg", text("api failure"))]))]),
                ),
            ])
        },
        events: RefCell::new(vec![]),
        detached: RefCell::new(vec![]),
        pending: RefCell::new(vec![]),
        background: RefCell::new(vec![]),
        handled: RefCell::new(vec![]),
        uuid: Cell::new(0),
        post: Cell::new(0),
        formats: Cell::new(0),
        sleeps: Cell::new(0),
    }))
}
fn settle_pending(h: &Host) {
    let mut pending = h.pending.borrow_mut();
    if pending.is_empty() {
        return;
    }
    let (i, cell) = if flag(&h.cfg, "reverse") {
        pending.pop().unwrap()
    } else {
        pending.remove(0)
    };
    drop(pending);
    h.effect("complete", vec![h.number(i as f64)]).unwrap();
    let result = if same(&h.number(i as f64), &get(&h.cfg, "rejectIndex")) {
        Err(h.error.clone())
    } else {
        let msg = if h.truthy(&get(&h.cfg, "message")) {
            get(&h.cfg, "message")
        } else {
            text("bad response")
        };
        Ok(object(vec![(
            "data",
            object(vec![
                (
                    "code",
                    h.number(if same(&h.number(i as f64), &get(&h.cfg, "badIndex")) {
                        500.0
                    } else {
                        0.0
                    }),
                ),
                ("msg", msg),
                (
                    "data",
                    object(vec![(
                        "orderId",
                        if flag(&h.cfg, "rawIds") && i == 0 {
                            h.undefined()
                        } else {
                            h.number(i as f64)
                        },
                    )]),
                ),
            ]),
        )]))
    };
    *cell.borrow_mut() = Some(result)
}
fn poll_background(h: &Host) {
    let tasks = std::mem::take(&mut *h.background.borrow_mut());
    for mut f in tasks {
        match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Pending => h.background.borrow_mut().push(f),
            Poll::Ready(Err(e)) => h.handled.borrow_mut().push(e),
            Poll::Ready(Ok(_)) => {}
        }
    }
}
fn drive<F: Future>(h: &Host, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    let mut result = None;
    for _ in 0..2000 {
        for _ in 0..12 {
            if result.is_none() {
                if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    result = Some(r)
                }
            }
            poll_background(h);
        }
        if !h.pending.borrow().is_empty() {
            settle_pending(h);
        } else if result.is_some() && h.background.borrow().is_empty() {
            return result.unwrap();
        }
    }
    panic!("finite native scheduler floor")
}
fn run(h: &Host, g: &str) -> Result<V, E> {
    let r = h.r.clone();
    let opts = get(&h.cfg, "opts");
    match g {
        "markets" => drive(h, trading::markets(h, r)),
        "candles" => drive(h, trading::candles(h, r, opts)),
        "format" => trading::format_order(h, r, opts),
        "place" => drive(h, trading::place(h, r, opts)),
        "places" => drive(h, trading::places(h, r, get(&h.cfg, "orders"))),
        "batch" => drive(h, trading::batch(h, r, get(&h.cfg, "payloads"))),
        "update" => drive(
            h,
            trading::update(
                h,
                r,
                object(vec![
                    ("order", get(&h.cfg, "order")),
                    ("update", get(&h.cfg, "update")),
                ]),
            ),
        ),
        "leverage" => drive(
            h,
            trading::leverage(
                h,
                r,
                get(&opts, "symbol"),
                h.number(17.0),
                get(&h.cfg, "silent"),
            ),
        ),
        "cancel" => drive(h, trading::cancel(h, r, get(&h.cfg, "orders"))),
        _ => panic!("row"),
    }
}
fn field<'a>(v: &'a Value, k: &str) -> &'a Value {
    v["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e[0] == k)
        .map(|e| &e[1])
        .unwrap()
}
fn oracle() -> &'static Value {
    static O: OnceLock<Value> = OnceLock::new();
    O.get_or_init(|| {
        let p = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/trading-rest-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            p.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&p.stderr)
        );
        serde_json::from_slice(&p.stdout).unwrap()
    })
}
fn check(g: &str, floor: usize) {
    let rows = oracle()["rows"][g].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent named case floor {g}");
    for row in rows {
        let h = setup(&row["cfg"]);
        let id = field(&row["cfg"], "id").as_str().unwrap();
        let path = format!("{g}/{id}");
        match run(&h, g) {
            Ok(v) => {
                assert_eq!(
                    row["error"]["undefined"], true,
                    "native unexpected source error {path}"
                );
                native(&v, &row["result"], &format!("{path} result"));
                let identity = object(vec![
                    ("result", boolean(Rc::ptr_eq(&v, &get(&h.cfg, "result")))),
                    (
                        "batch",
                        boolean(Rc::ptr_eq(&v, &get(&h.cfg, "batchResult"))),
                    ),
                    ("cached", boolean(Rc::ptr_eq(&v, &get(&h.cfg, "cached")))),
                    (
                        "marketInfo",
                        boolean(
                            g == "markets"
                                && items(&v)
                                    .iter()
                                    .all(|m| Rc::ptr_eq(&get(m, "info"), &get(&h.cfg, "market"))),
                        ),
                    ),
                ]);
                native(&identity, &row["identity"], &format!("{path} identity"));
            }
            Err(e) => {
                native(
                    &get(&e, "message"),
                    field(&row["error"], "message"),
                    &format!("{path} error message"),
                );
                assert_eq!(
                    field(&row["error"], "identity"),
                    &Value::Bool(Rc::ptr_eq(&e, &h.error)),
                    "native error identity {path}"
                );
            }
        }
        native(
            &array(h.events.borrow().clone()),
            &Value::Array(
                row["events"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|e| serde_json::json!({"entries":[["kind",e["kind"]],["args",e["args"]]]}))
                    .collect(),
            ),
            &format!("{path} effects"),
        );
        native(
            &get(&h.r, "recentWsRequests"),
            &row["cache"],
            &format!("{path} cache"),
        );
        native(
            &get(&h.cfg, "opts"),
            &row["optsAfter"],
            &format!("{path} options"),
        );
        native(
            &get(&h.cfg, "formatted"),
            &row["formattedAfter"],
            &format!("{path} formatted"),
        );
        assert!(
            h.pending.borrow().is_empty() && h.background.borrow().is_empty(),
            "native all sibling requests settled {path}"
        );
    }
}
#[test]
fn markets_native() {
    check("markets", 3)
}
#[test]
fn candles_native() {
    check("candles", 7)
}
#[test]
fn format_native() {
    check("format", 14)
}
#[test]
fn place_native() {
    check("place", 6)
}
#[test]
fn places_native() {
    check("places", 5)
}
#[test]
fn batch_native() {
    check("batch", 7)
}
#[test]
fn update_native() {
    check("update", 5)
}
#[test]
fn leverage_native() {
    check("leverage", 6)
}
#[test]
fn cancel_native() {
    check("cancel", 7)
}
