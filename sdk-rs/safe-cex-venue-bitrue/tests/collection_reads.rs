use safe_cex_venue_bitrue::collection_reads::{self as reads, Callback, Op, ReadHost};
use serde_json::Value;
use std::{
    cell::RefCell,
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
        Raw::Callback(_) => panic!("callback outside observation floor"),
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
    parsed: RefCell<V>,
    fetches: RefCell<usize>,
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
fn dep_value(v: &Raw) -> Value {
    match v {
        Raw::Undefined | Raw::Null => Value::Null,
        Raw::Bool(b) => Value::Bool(*b),
        Raw::Number(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(safe_cex_num::js_number_string(*n))),
        Raw::Text(s) => Value::String(String::from_utf16(s).unwrap()),
        Raw::Array(a) => Value::Array(a.borrow().iter().map(|v| dep_value(v)).collect()),
        Raw::Object(_) => Value::Object(
            ordered(v)
                .into_iter()
                .filter(|(_, v)| !matches!(&**v, Raw::Undefined))
                .map(|(k, v)| (String::from_utf16(&k).unwrap(), dep_value(&v)))
                .collect(),
        ),
        _ => panic!("dependency binding floor"),
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
        if kind == "fetchBalanceAndPositionsRaw" && same(&hook, &text("raw")) {
            *self.parsed.borrow_mut() = array(vec![object(vec![("id", text("changed"))])]);
            set(
                &self.r,
                "parsePositions",
                Rc::new(Raw::Function("parsePositionsChanged")),
            );
            set(
                &self.r,
                "parseBalance",
                Rc::new(Raw::Function("parseBalanceChanged")),
            );
        }
        if kind == "fetchOrdersForSymbol" {
            *self.fetches.borrow_mut() += 1;
            let first = *self.fetches.borrow() == 1;
            if same(&hook, &text("priority")) && first {
                set(&self.r, "prioritySymbols", array(vec![text("B")]));
            }
            if same(&hook, &text("disposedFetch")) {
                set(&self.r, "isDisposed", boolean(true));
            }
            if same(&hook, &text("cancel")) && first {
                set(&self.r, "inProgressOrderFetchId", self.number(99.0));
            }
        }
        if kind == "sleep" && same(&hook, &text("disposedSleep")) {
            set(&self.r, "isDisposed", boolean(true));
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
        let Raw::Function(kind) = &*f else {
            return Err(self.error.clone());
        };
        let kind = *kind;
        assert!(
            same(
                &receiver,
                &if kind == "get" {
                    get(&self.r, "xhr")
                } else if kind == "addOrUpdateOrders" {
                    get(&self.r, "store")
                } else {
                    self.r.clone()
                }
            ),
            "native original method receiver"
        );
        if kind == "fetchBitrueTradeRows" {
            assert!(
                Rc::ptr_eq(&args[0], &get(&self.cfg, "opts")),
                "native original history options"
            )
        }
        if kind == "fetchHistoricalTradesInternal" {
            assert!(
                !Rc::ptr_eq(&args[0], &get(&self.cfg, "opts")),
                "native fresh fill options"
            );
            assert_eq!(
                ordered(&args[0]).len(),
                5,
                "native independent fill option floor"
            )
        }

        let effect = self.effect(kind, args.clone());
        let sync = matches!(
            kind,
            "normalizeTimestampMs"
                | "normalizeTradeSymbol"
                | "pickTradeOrderId"
                | "resolveTradeQty"
                | "isSuccessCode"
                | "parseBalance"
                | "parseBalanceChanged"
                | "emitAndLogError"
                | "addOrUpdateOrders"
        );
        if sync {
            effect?;
            return Ok(match kind {
                "normalizeTimestampMs" => {
                    let v = &args[0];
                    safe_cex_venue_bitrue::read_helpers::normalize_timestamp_ms(
                        if self.is_finite(v) {
                            Some(num(v))
                        } else {
                            None
                        },
                    )
                    .map(|v| self.number(v))
                    .unwrap_or_else(|| self.undefined())
                }
                "normalizeTradeSymbol" => text(
                    &safe_cex_venue_bitrue::read_helpers::normalize_trade_symbol(Some(&dep_value(
                        &args[0],
                    )))
                    .unwrap(),
                ),
                "pickTradeOrderId" => {
                    let e = dep_value(&args[0]);
                    safe_cex_venue_bitrue::read_helpers::pick_trade_order_id(e.as_object().unwrap())
                        .unwrap()
                        .map(|s| text(&s))
                        .unwrap_or_else(|| self.undefined())
                }
                "resolveTradeQty" => {
                    let e = dep_value(&args[0]);
                    self.number(
                        safe_cex_venue_bitrue::read_helpers::resolve_trade_qty(
                            e.as_object().unwrap(),
                            &safe_cex_exchange::native_store::DefaultStore::default(),
                            &String::from_utf16(&string(&args[1])).unwrap(),
                            num(&args[2]),
                        )
                        .unwrap(),
                    )
                }
                "isSuccessCode" => {
                    let value = dep_value(&args[0]);
                    boolean(safe_cex_venue_bitrue::read_helpers::is_success_code(
                        if matches!(&*args[0], Raw::Undefined) {
                            None
                        } else {
                            Some(&value)
                        },
                    ))
                }
                "parseBalance" | "parseBalanceChanged" => get(&self.cfg, "balance"),
                "addOrUpdateOrders" => {
                    assert!(
                        Rc::ptr_eq(&args[0], &get(&self.cfg, "symbolOrders")),
                        "native store original orders"
                    );
                    set(&get(&self.r, "store"), "orders", args[0].clone());
                    self.undefined()
                }
                "emitAndLogError" => self.undefined(),
                _ => unreachable!(),
            });
        }
        let out = effect.and_then(|()| {
            if kind == "fetchBitrueTradeRows" && flag(&self.cfg, "rowReject")
                || kind == "fetchHistoricalTradesInternal" && flag(&self.cfg, "historyReject")
            {
                return Err(self.error.clone());
            }
            Ok(match kind {
                "fetchBalanceAndPositionsRaw" => get(&self.cfg, "raw"),
                "parsePositions" | "parsePositionsChanged" => self.parsed.borrow().clone(),
                "fetchOpenPositions" => get(&self.cfg, "open"),
                "fetchPositionMetadata" => get(&self.cfg, "metadata"),
                "fetchOrdersForSymbol" => get(&self.cfg, "symbolOrders"),
                "fetchAndEmitOrders" => self.undefined(),
                "get" => object(vec![("data", get(&self.cfg, "data"))]),
                "fetchBitrueTradeRows" => get(&self.cfg, "tradeRows"),
                "fetchHistoricalTradesInternal" => get(&self.cfg, "trades"),
                _ => panic!("async binding {kind}"),
            })
        });
        Ok(self.returned(kind, out))
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
        Ok(object(vec![("message", message)]))
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
fn setup(cfg: &Value) -> Host {
    let cfg = decode(cfg);
    let mut fields = vec![
        ("isPublicDataOnly", boolean(flag(&cfg, "publicOnly"))),
        ("isDisposed", boolean(false)),
        ("inProgressOrderFetchId", Rc::new(Raw::Number(0.0))),
        ("prioritySymbols", get(&cfg, "priority")),
        (
            "store",
            object(vec![
                ("markets", get(&cfg, "markets")),
                ("orders", get(&cfg, "orders")),
                ("positions", get(&cfg, "open")),
                (
                    "addOrUpdateOrders",
                    Rc::new(Raw::Function("addOrUpdateOrders")),
                ),
            ]),
        ),
        ("xhr", object(vec![("get", Rc::new(Raw::Function("get")))])),
    ];
    for name in [
        "normalizeTimestampMs",
        "normalizeTradeSymbol",
        "pickTradeOrderId",
        "resolveTradeQty",
        "isSuccessCode",
        "fetchBalanceAndPositionsRaw",
        "parsePositions",
        "parseBalance",
        "fetchOpenPositions",
        "fetchPositionMetadata",
        "fetchOrdersForSymbol",
        "fetchAndEmitOrders",
        "fetchBitrueTradeRows",
        "fetchHistoricalTradesInternal",
        "emitAndLogError",
    ] {
        fields.push((name, Rc::new(Raw::Function(name))))
    }
    let parsed = get(&cfg, "parsedPositions");
    Host(Rc::new(State {
        cfg,
        r: object(fields),
        error: object(vec![
            ("message", text("original failure")),
            (
                "response",
                object(vec![("data", object(vec![("msg", text("api failure"))]))]),
            ),
        ]),
        events: RefCell::new(vec![]),
        detached: RefCell::new(vec![]),
        parsed: RefCell::new(parsed),
        fetches: RefCell::new(0),
    }))
}
fn drive<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..100 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return r;
        }
    }
    panic!("finite async read floor")
}
fn run(h: &Host, g: &str) -> Result<V, E> {
    let r = h.r.clone();
    let arg = if ["positions", "orders"].contains(&g) {
        if flag(&h.cfg, "noSymbols") {
            h.undefined()
        } else {
            get(&h.cfg, "symbols")
        }
    } else {
        get(&h.cfg, "opts")
    };
    match g {
        "open" => drive(reads::open_positions(h, r)),
        "positions" => drive(reads::positions(h, r, arg)),
        "balance" => drive(reads::balance_positions(h, r)),
        "orders" => drive(reads::orders(h, r, arg)),
        "emit" => drive(reads::emit_orders(h, r)),
        "symbol" => drive(reads::symbol_orders(h, r, get(&arg, "symbol"))),
        "rows" => drive(reads::trade_rows(h, r, arg)),
        "history" => drive(reads::history(h, r, arg)),
        "fills" => drive(reads::fills(h, r, arg)),
        _ => panic!("family"),
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
                "/tests/collection-reads-oracle.cjs"
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
    assert_eq!(rows.len(), floor, "native independent named case floor {g}");
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
                let info = if g == "symbol" {
                    let original = get(&get(&h.cfg, "data"), "data");
                    items(&v).iter().all(|o| {
                        h.is_array(&original)
                            && items(&original)
                                .iter()
                                .any(|e| Rc::ptr_eq(&get(o, "info"), e))
                    })
                } else {
                    true
                };
                let identity = object(vec![
                    ("raw", boolean(Rc::ptr_eq(&v, &get(&h.cfg, "raw")))),
                    ("open", boolean(Rc::ptr_eq(&v, &get(&h.cfg, "open")))),
                    ("parsed", boolean(Rc::ptr_eq(&v, &h.parsed.borrow()))),
                    (
                        "balance",
                        boolean(Rc::ptr_eq(&get(&v, "balance"), &get(&h.cfg, "balance"))),
                    ),
                    (
                        "positions",
                        boolean(Rc::ptr_eq(&get(&v, "positions"), &h.parsed.borrow())),
                    ),
                    (
                        "tradeRows",
                        boolean(Rc::ptr_eq(&v, &get(&get(&h.cfg, "data"), "data"))),
                    ),
                    ("info", boolean(info)),
                    (
                        "fallback",
                        boolean(
                            g == "symbol"
                                && items(&v).iter().any(|o| {
                                    items(&get(&h.cfg, "orders"))
                                        .iter()
                                        .any(|old| Rc::ptr_eq(o, old))
                                }),
                        ),
                    ),
                ]);
                native(&identity, &row["identity"], &format!("{path} identity"));
                if ["open", "positions", "balance"].contains(&g) {
                    set(
                        &get(&h.r, "store"),
                        "positions",
                        array(vec![object(vec![("id", text("replacement"))])]),
                    );
                    set(
                        &get(&h.r, "store"),
                        "balance",
                        object(vec![("total", h.number(999.0))]),
                    );
                    set(
                        &get(&h.r, "store"),
                        "tickersBySymbol",
                        object(vec![("A", object(vec![("last", h.number(1.0))]))]),
                    );
                    native(
                        &v,
                        &row["result"],
                        "native retained result after store replacement",
                    );
                }
            }
            Err(e) => {
                assert_eq!(
                    field(&row["error"], "identity"),
                    &Value::Bool(Rc::ptr_eq(&e, &h.error)),
                    "native original error {path}"
                );
                native(
                    &get(&e, "message"),
                    field(&row["error"], "message"),
                    &format!("{path} error message"),
                );
            }
        }
        for p in std::mem::take(&mut *h.detached.borrow_mut()) {
            if let Err(e) = drive(h.await_value(p)) {
                h.effect(
                    "detached",
                    vec![boolean(Rc::ptr_eq(&e, &h.error)), get(&e, "message")],
                )
                .unwrap();
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
            &get(&h.r, "isDisposed"),
            &Value::Bool(row["disposed"].as_bool().unwrap()),
            &format!("{path} disposal"),
        );
        native(
            &get(&h.r, "inProgressOrderFetchId"),
            &row["fetchId"],
            &format!("{path} fetch ID"),
        );
        native(
            &get(&get(&h.r, "store"), "orders"),
            &row["storeOrders"],
            &format!("{path} store orders"),
        );
    }
}
#[test]
fn open_native() {
    check("open", 3)
}
#[test]
fn positions_native() {
    check("positions", 6)
}
#[test]
fn balance_native() {
    check("balance", 3)
}
#[test]
fn orders_native() {
    check("orders", 5)
}
#[test]
fn emit_native() {
    check("emit", 7)
}
#[test]
fn symbol_native() {
    check("symbol", 7)
}
#[test]
fn rows_native() {
    check("rows", 8)
}
#[test]
fn history_native() {
    check("history", 8)
}
#[test]
fn fills_native() {
    check("fills", 6)
}
