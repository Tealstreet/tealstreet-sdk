use safe_cex_venue_bitrue::collection_reads::{Callback, Op, ReadHost};
use safe_cex_venue_bitrue::runtime_lifecycle::{self as runtime, LifecycleHost};
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
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Rc<Raw>>>),
    Set(RefCell<Vec<V>>),
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
        Raw::Set(_) => panic!("Set coercion outside floor"),
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
        Raw::Set(_) => panic!("native Set must observe values before encoding"),
        Raw::Promise(_) => panic!("unsettled native value"),
    }
}
fn snapshot(value: &V) -> V {
    Rc::new(match &**value {
        Raw::Error(m) => Raw::Error(m.clone()),
        Raw::Undefined => Raw::Undefined,
        Raw::Null => Raw::Null,
        Raw::Bool(b) => Raw::Bool(*b),
        Raw::Number(n) => Raw::Number(*n),
        Raw::Text(s) => Raw::Text(s.clone()),
        Raw::Function(f) => Raw::Function(f),
        Raw::Callback(f) => Raw::Callback(f.clone()),
        Raw::Promise(p) => Raw::Promise(p.clone()),
        Raw::Set(a) => Raw::Set(RefCell::new(a.borrow().clone())),
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
    final_error: V,
    events: RefCell<Vec<V>>,
    next_timer: Cell<f64>,
    timers: RefCell<Vec<(f64, V)>>,
    loop_callback: RefCell<V>,
    detached: RefCell<Vec<V>>,
    cleanup: V,
    callback: V,
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
            Err(
                if kind == "queueFastPrivateRefresh"
                    && same(&get(&self.cfg, "id"), &text("finally-throws"))
                {
                    self.final_error.clone()
                } else {
                    self.error.clone()
                },
            )
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
    fn hook(&self, kind: &str) -> Result<(), E> {
        let hook = get(&self.cfg, "hook");
        if (kind == "fetchBalanceAndPositions" && same(&hook, &text("balance-dispose")))
            || (kind == "fetchMarkets" && same(&hook, &text("markets-dispose")))
            || (kind == "fetchOrders" && same(&hook, &text("fetch-orders-dispose")))
            || (kind == "fetchAndSetOrders" && same(&hook, &text("orders-dispose")))
        {
            set(&self.r, "isDisposed", boolean(true));
        }
        if kind == "fetchMarkets" && same(&hook, &text("markets-public")) {
            set(&self.r, "isPublicDataOnly", boolean(true));
        }
        if kind == "fetchAndSetOrders" && same(&hook, &text("orders-late")) {
            self.set_add(get(&self.r, "pendingOrderRefreshSymbols"), text("LATE"))?;
        }
        if kind == "fetchBalanceAndPositions" && same(&hook, &text("replace-tick")) {
            set(&self.r, "tick", Rc::new(Raw::Function("tickChanged")));
        }
        if kind == "fetchPositionMetadata" && same(&hook, &text("position-mutate")) {
            let account = items(&get(&self.cfg, "accounts"))[0].clone();
            let vo = items(&get(&account, "positionVos"))[0].clone();
            let pos = items(&get(&vo, "positions"))[0].clone();
            set(&pos, "openPrice", self.number(105.0));
        }
        Ok(())
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
            h.hook(kind)?;
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
        if matches!(&**v, Raw::Promise(_)) && matches!(key, "then" | "catch") {
            return Ok(Rc::new(Raw::Function(if key == "then" {
                "then"
            } else {
                "catch"
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
        if let Raw::Callback(f) = &*f {
            return f(args);
        }
        let Raw::Function(kind) = &*f else {
            return Err(self.error.clone());
        };
        let kind = *kind;
        if kind == "then" || kind == "catch" {
            let h = self.clone();
            let callback = args[0].clone();
            return Ok(self.begin(Box::pin(async move {
                let value = h.await_value(receiver).await;
                match value {
                    Ok(v) if kind == "then" => h.call(callback, h.undefined(), vec![v]),
                    Err(e) if kind == "catch" => h.call(callback, h.undefined(), vec![e]),
                    v => v,
                }
            })));
        }
        self.effect(kind, args.clone())?;
        if kind == "fetchPositionMetadata" {
            assert!(
                Rc::ptr_eq(&args[1], &get(&self.cfg, "snapshots")),
                "native original position snapshots"
            );
        }
        if matches!(kind, "getPositionAccounts" | "getAccountPositionSnapshots") {
            assert!(
                Rc::ptr_eq(&args[0], &get(&self.cfg, "data")),
                "native original account data"
            );
        }
        if kind == "setTimeout" {
            let id = self.next_timer.get();
            self.next_timer.set(id + 1.0);
            self.timers.borrow_mut().push((id, args[0].clone()));
            if flag(&self.cfg, "immediate") {
                self.call(args[0].clone(), self.undefined(), vec![])?;
            }
            return Ok(self.number(id));
        }
        if kind == "clearTimeout" {
            let id = num(&args[0]);
            self.timers.borrow_mut().retain(|(n, _)| *n != id);
            return Ok(self.undefined());
        }
        if kind == "loop" {
            *self.loop_callback.borrow_mut() = args[0].clone();
            return Ok(self.undefined());
        }
        if kind == "runFastPrivateRefresh" && flag(&self.cfg, "runBody") {
            let h = self.clone();
            return Ok(self.begin(Box::pin(async move {
                runtime::run_refresh(&h, h.r.clone()).await
            })));
        }
        let out = match kind {
            "get" => object(vec![("data", get(&self.cfg, "account"))]),
            "fetchBalanceAndPositions" => get(&self.cfg, "result"),
            "fetchMarkets" => get(&self.cfg, "markets"),
            "fetchOrders" => get(&self.cfg, "orders"),
            "initExternalPublicData" => get(&self.cfg, "external"),
            "fetchPositionMetadata" => get(&self.cfg, "metadata"),
            "fetchAndSetOrders" => self.undefined(),
            "getPositionAccounts" => get(&self.cfg, "accounts"),
            "getAccountPositionSnapshots" => get(&self.cfg, "snapshots"),
            "isSuccessCode" => boolean(
                [self.number(0.0), text("0"), self.number(200.0), text("200")]
                    .iter()
                    .any(|v| same(v, &args[0])),
            ),
            "addOrUpdatePositionsFull" => {
                assert!(
                    Rc::ptr_eq(&args[0], &get(&get(&self.cfg, "result"), "positions")),
                    "native original refreshed positions"
                );
                set(&get(&self.r, "store"), "positions", args[0].clone());
                self.undefined()
            }
            "update" => {
                assert!(
                    Rc::ptr_eq(&receiver, &get(&self.r, "store")),
                    "native original Store receiver"
                );
                for (k, v) in ordered(&args[0]) {
                    set(&receiver, &String::from_utf16(&k).unwrap(), v);
                }
                self.undefined()
            }
            "off" => {
                assert!(
                    Rc::ptr_eq(&args[1], &get(&self.r, "handleFillForFastRefresh")),
                    "native original fill listener"
                );
                self.undefined()
            }
            "publicDispose" => {
                if same(&get(&self.cfg, "hook"), &text("dispose")) {
                    assert!(
                        matches!(
                            &*get(&get(&self.r, "privateWebsocket"), "dispose"),
                            Raw::Function("changedPrivateDispose")
                        ),
                        "native post-super private member"
                    );
                }
                self.undefined()
            }
            "listenOHLCV" | "listenOrderBook" => {
                assert!(
                    Rc::ptr_eq(&receiver, &get(&self.r, "publicWebsocket")),
                    "native original socket delegate receiver"
                );
                assert!(
                    Rc::ptr_eq(&args[1], &self.callback),
                    "native original callback"
                );
                if kind == "listenOHLCV" {
                    assert!(
                        Rc::ptr_eq(&args[0], &get(&self.cfg, "opts")),
                        "native original candle options"
                    );
                }
                self.cleanup.clone()
            }
            "getTickInterval" => self.number(400.0),
            "tick" | "tickChanged" => get(&self.cfg, "callbackResult"),
            "privateDispose"
            | "changedPrivateDispose"
            | "publicConnect"
            | "privateConnect"
            | "emitAndLogError"
            | "queueFastPrivateRefresh"
            | "runFastPrivateRefresh" => self.undefined(),
            _ => panic!("outside lifecycle binding floor {kind}"),
        };
        if matches!(
            kind,
            "get"
                | "fetchBalanceAndPositions"
                | "fetchMarkets"
                | "fetchOrders"
                | "initExternalPublicData"
                | "fetchPositionMetadata"
                | "fetchAndSetOrders"
        ) {
            let out = if same(&get(&self.cfg, "reject"), &text(kind)) {
                Err(self.error.clone())
            } else {
                Ok(out)
            };
            Ok(self.returned(kind, out))
        } else {
            Ok(out)
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
        if matches!(&*v, Raw::Promise(_)) {
            self.detached.borrow_mut().push(v);
        }
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
impl LifecycleHost for Host {
    fn builtin(&self, n: &'static str) -> Result<V, E> {
        Ok(Rc::new(Raw::Function(n)))
    }
    fn dynamic(&self, v: V, k: V) -> Result<V, E> {
        self.get(&v, &String::from_utf16(&string(&k)).unwrap())
    }
    fn set_add(&self, s: V, v: V) -> Result<(), E> {
        let priority = Rc::ptr_eq(&s, &get(&self.r, "prioritySymbols"));
        self.effect(
            if priority { "priority-add" } else { "set-add" },
            vec![v.clone()],
        )?;
        let Raw::Set(a) = &*s else {
            panic!("source Set")
        };
        let mut a = a.borrow_mut();
        if !a.iter().any(|x| self.set_equal(x, &v)) {
            a.push(v)
        }
        Ok(())
    }
    fn set_clear(&self, s: V) -> Result<(), E> {
        self.effect("set-clear", vec![])?;
        let Raw::Set(a) = &*s else {
            panic!("source Set")
        };
        a.borrow_mut().clear();
        Ok(())
    }
    fn set_values(&self, s: V) -> Result<V, E> {
        self.effect("set-values", vec![])?;
        Ok(self.set_array(&s))
    }
    fn set_size(&self, s: V) -> Result<V, E> {
        let Raw::Set(a) = &*s else {
            panic!("source Set")
        };
        Ok(self.number(a.borrow().len() as f64))
    }
    fn find(&self, a: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        for v in items(&a) {
            if f(v.clone())? {
                return Ok(v);
            }
        }
        Ok(self.undefined())
    }
    fn super_dispose(&self, r: V) -> Result<V, E> {
        assert!(Rc::ptr_eq(&r, &self.r), "native base disposal receiver");
        self.effect("baseDispose", vec![])?;
        if same(&get(&self.cfg, "hook"), &text("dispose")) {
            set(
                &get(&self.r, "privateWebsocket"),
                "dispose",
                Rc::new(Raw::Function("changedPrivateDispose")),
            );
        }
        Ok(self.undefined())
    }
}
impl Host {
    fn set_array(&self, s: &V) -> V {
        let Raw::Set(a) = &**s else {
            panic!("native Set")
        };
        array(a.borrow().clone())
    }
    fn pump(&self) -> Result<(), E> {
        loop {
            let next = self.detached.borrow_mut().pop();
            let Some(p) = next else { return Ok(()) };
            settle(self.await_value(p))?;
        }
    }
}
fn set_value(a: Vec<V>) -> V {
    let mut out = vec![];
    for v in a {
        if !out.iter().any(|x| same(x, &v)) {
            out.push(v)
        }
    }
    Rc::new(Raw::Set(RefCell::new(out)))
}
fn setup(cfg: V) -> Host {
    let undef = Rc::new(Raw::Undefined);
    let error = object(vec![
        ("message", text("original failure")),
        (
            "response",
            object(vec![("data", object(vec![("msg", text("server cause"))]))]),
        ),
    ]);
    let store = object(vec![
        ("positions", get(&cfg, "oldPositions")),
        ("loaded", object(vec![("sentinel", boolean(true))])),
        ("tickersBySymbol", get(&cfg, "tickers")),
        (
            "addOrUpdatePositionsFull",
            Rc::new(Raw::Function("addOrUpdatePositionsFull")),
        ),
        ("update", Rc::new(Raw::Function("update"))),
    ]);
    let r = object(vec![
        ("isDisposed", get(&cfg, "disposed")),
        ("isPublicDataOnly", get(&cfg, "publicOnly")),
        ("isFastPrivateRefreshRunning", get(&cfg, "running")),
        ("fastPrivateRefreshTimeoutId", get(&cfg, "timer")),
        (
            "pendingOrderRefreshSymbols",
            set_value(items(&get(&cfg, "pending"))),
        ),
        ("prioritySymbols", set_value(vec![])),
        ("store", store),
        ("xhr", object(vec![("get", Rc::new(Raw::Function("get")))])),
        (
            "emitter",
            object(vec![("off", Rc::new(Raw::Function("off")))]),
        ),
        (
            "publicWebsocket",
            object(vec![
                ("dispose", Rc::new(Raw::Function("publicDispose"))),
                (
                    "connectAndSubscribe",
                    Rc::new(Raw::Function("publicConnect")),
                ),
                ("listenOHLCV", Rc::new(Raw::Function("listenOHLCV"))),
                ("listenOrderBook", Rc::new(Raw::Function("listenOrderBook"))),
            ]),
        ),
        (
            "privateWebsocket",
            object(vec![
                ("dispose", Rc::new(Raw::Function("privateDispose"))),
                (
                    "connectAndSubscribe",
                    Rc::new(Raw::Function("privateConnect")),
                ),
            ]),
        ),
        (
            "handleFillForFastRefresh",
            Rc::new(Raw::Function("handleFillForFastRefresh")),
        ),
    ]);
    for n in [
        "fetchBalanceAndPositions",
        "fetchAndSetOrders",
        "fetchMarkets",
        "fetchOrders",
        "initExternalPublicData",
        "fetchPositionMetadata",
        "emitAndLogError",
        "queueFastPrivateRefresh",
        "runFastPrivateRefresh",
        "tick",
        "getTickInterval",
        "getPositionAccounts",
        "getAccountPositionSnapshots",
        "isSuccessCode",
    ] {
        set(&r, n, Rc::new(Raw::Function(n)));
    }
    Host(Rc::new(State {
        cfg,
        r,
        error,
        final_error: object(vec![("message", text("finally failure"))]),
        events: RefCell::new(vec![]),
        next_timer: Cell::new(100.0),
        timers: RefCell::new(vec![]),
        loop_callback: RefCell::new(undef),
        detached: RefCell::new(vec![]),
        cleanup: Rc::new(Raw::Function("cleanup")),
        callback: Rc::new(Raw::Function("callback")),
    }))
}
fn settle(f: impl Future<Output = Result<V, E>>) -> Result<V, E> {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return r;
        }
    }
    panic!("native finite witness did not settle")
}
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/runtime-lifecycle-oracle.cjs"
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
fn run(group: &str, row: &Value) {
    let h = setup(decode(&row["cfg"]));
    let id = String::from_utf16(&string(&get(&h.cfg, "id"))).unwrap();
    let p = format!("{group}/{id}");
    let mut identity = h.undefined();
    let mut callback_return = h.undefined();
    let outcome = (|| -> Result<V, E> {
        let result = match group {
            "dispose" => runtime::dispose(&h, h.r.clone()),
            "queue" => runtime::queue_refresh(&h, h.r.clone(), get(&h.cfg, "symbol")),
            "refresh" => settle(runtime::run_refresh(&h, h.r.clone())),
            "validate" => settle(runtime::validate(&h, h.r.clone())),
            "start" => settle(runtime::start(&h, h.r.clone())),
            "tick" => settle(runtime::tick(&h, h.r.clone())),
            "positions" => settle(runtime::parse_positions(
                &h,
                h.r.clone(),
                get(&h.cfg, "data"),
            )),
            "candles" => {
                runtime::listen_candles(&h, h.r.clone(), get(&h.cfg, "opts"), h.callback.clone())
            }
            "book" => {
                runtime::listen_book(&h, h.r.clone(), get(&h.cfg, "symbol"), h.callback.clone())
            }
            _ => panic!("family"),
        }?;
        if flag(&h.cfg, "fire") {
            let id = num(&get(&h.r, "fastPrivateRefreshTimeoutId"));
            let cb = h
                .timers
                .borrow()
                .iter()
                .find(|(n, _)| *n == id)
                .unwrap()
                .1
                .clone();
            callback_return = h.call(cb, h.undefined(), vec![])?;
        }
        if flag(&h.cfg, "fireLoop") {
            callback_return = h.call(h.loop_callback.borrow().clone(), h.undefined(), vec![])?;
            assert!(
                Rc::ptr_eq(&callback_return, &get(&h.cfg, "callbackResult")),
                "native current loop callback return {p}"
            );
        }
        h.pump()?;
        if group == "positions" {
            let values = items(&result);
            let fallback = Rc::ptr_eq(&result, &get(&get(&h.r, "store"), "positions"));
            let source_info = if let Raw::Array(a) = &*get(&h.cfg, "accounts") {
                let a = a.borrow();
                if a.is_empty() {
                    h.undefined()
                } else {
                    let vo = items(&get(&a[0], "positionVos"))[0].clone();
                    items(&get(&vo, "positions"))[0].clone()
                }
            } else {
                h.undefined()
            };
            let info = values.is_empty() || same(&get(&values[0], "info"), &source_info);
            identity = object(vec![
                ("fallback", boolean(fallback)),
                ("info", boolean(info)),
            ]);
        }
        if group == "candles" || group == "book" {
            identity = boolean(Rc::ptr_eq(&result, &h.cleanup));
        }
        Ok(result)
    })();
    let (result, error) = match outcome {
        Ok(v) => (v, h.undefined()),
        Err(e) => (
            h.undefined(),
            object(vec![
                ("identity", boolean(Rc::ptr_eq(&e, &h.error))),
                ("message", get(&e, "message")),
            ]),
        ),
    };
    let store = Rc::new(Raw::Object(RefCell::new(
        ordered(&get(&h.r, "store"))
            .into_iter()
            .filter(|(_, v)| !matches!(&**v, Raw::Function(_) | Raw::Callback(_)))
            .collect(),
    )));
    let state = object(vec![
        ("disposed", get(&h.r, "isDisposed")),
        ("publicOnly", get(&h.r, "isPublicDataOnly")),
        ("running", get(&h.r, "isFastPrivateRefreshRunning")),
        ("timer", get(&h.r, "fastPrivateRefreshTimeoutId")),
        (
            "pending",
            h.set_array(&get(&h.r, "pendingOrderRefreshSymbols")),
        ),
        ("priority", h.set_array(&get(&h.r, "prioritySymbols"))),
        ("store", store),
    ]);
    native(&array(h.events.borrow().clone()), &row["events"], &p);
    native(&result, &row["result"], &p);
    native(&identity, &row["identity"], &p);
    native(&error, &row["error"], &p);
    native(&state, &row["state"], &p);
    native(&callback_return, &row["callbackReturn"], &p);
}
fn family(g: &str, n: usize) {
    let rows = oracle()["rows"][g].as_array().unwrap();
    assert_eq!(rows.len(), n, "independent case floor {g}");
    for r in rows {
        run(g, r)
    }
}
#[test]
fn dispose_native() {
    family("dispose", 7)
}
#[test]
fn queue_native() {
    family("queue", 8)
}
#[test]
fn refresh_native() {
    family("refresh", 10)
}
#[test]
fn validate_native() {
    family("validate", 7)
}
#[test]
fn start_native() {
    family("start", 10)
}
#[test]
fn tick_native() {
    family("tick", 9)
}
#[test]
fn positions_native() {
    family("positions", 9)
}
#[test]
fn candles_native() {
    family("candles", 3)
}
#[test]
fn book_native() {
    family("book", 2)
}
