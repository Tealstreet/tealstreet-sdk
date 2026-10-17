use safe_cex_venue_bitrue::collection_reads::{Callback, Op, ReadHost};
use safe_cex_venue_bitrue::private_auth::{self as private, PrivateHost};
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
        Raw::Callback(_) => panic!("callback outside observation floor"),
        Raw::Promise(_) => panic!("unsettled native value"),
    }
}
fn snapshot(value: &V) -> V {
    Rc::new(match &**value {
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
    post: Cell<usize>,
    delays: V,
    client: RefCell<V>,
    api_config: RefCell<V>,
    interceptor: RefCell<V>,
    response_interceptor: RefCell<V>,
    initial_incremental: RefCell<V>,
    expected_update: RefCell<V>,
    prototype_dispose: V,
    instances: RefCell<Vec<V>>,
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
        if (kind == "fetchListenKey" && same(&hook, &text("connect")))
            || (kind == "put" && same(&hook, &text("refresh")))
        {
            set(&self.r, "isDisposed", boolean(true));
        }
        if kind == "post" && same(&hook, &text("delays")) {
            let Raw::Array(a) = &*self.delays else {
                panic!("delays")
            };
            *a.borrow_mut() = vec![self.number(42.0)];
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
        if key == "dispose" && self.instances.borrow().iter().any(|r| Rc::ptr_eq(r, v)) {
            return Ok(self.prototype_dispose.clone());
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
        if kind == "use" {
            assert!(
                Rc::ptr_eq(
                    &receiver,
                    &get(&get(&self.client.borrow(), "interceptors"), "request")
                ),
                "native interceptor receiver"
            );
            self.effect("use", vec![boolean(true)])?;
            *self.interceptor.borrow_mut() = args[0].clone();
            return Ok(self.undefined());
        }
        if kind == "use-response" {
            assert!(
                Rc::ptr_eq(
                    &receiver,
                    &get(&get(&self.client.borrow(), "interceptors"), "response")
                ),
                "native response interceptor receiver"
            );
            self.effect("use-response", vec![boolean(true)])?;
            *self.response_interceptor.borrow_mut() = args[0].clone();
            return Ok(self.undefined());
        }
        if !matches!(kind, "update" | "digest") {
            self.effect(kind, args.clone())?;
        }
        let out = match kind {
            "post" | "put" => {
                assert!(
                    Rc::ptr_eq(&receiver, &get(&get(&self.r, "parent"), "xhr")),
                    "native listen-key receiver"
                );
                let n = self.post.get();
                self.post.set(n + 1);
                let responses = items(&get(&self.cfg, "responses"));
                let out = if flag(&self.cfg, "rejectAll") {
                    Err(self.error.clone())
                } else {
                    Ok(object(vec![(
                        "data",
                        responses[n.min(responses.len() - 1)].clone(),
                    )]))
                };
                return Ok(self.returned(kind, out));
            }
            "fetchListenKey" => {
                let out = if flag(&self.cfg, "fetchReject") {
                    Err(self.error.clone())
                } else {
                    Ok(get(&self.cfg, "key"))
                };
                return Ok(self.returned(kind, out));
            }
            "createWebsocket" => object(vec![(
                "addEventListener",
                Rc::new(Raw::Function("addEventListener")),
            )]),
            "isSuccessfulCode" => {
                use safe_cex_venue_bitrue::ws_private::{SuccessCode, is_successful_code};
                boolean(is_successful_code(match &*args[0] {
                    Raw::Number(n) => SuccessCode::Number(*n),
                    Raw::Text(t) => SuccessCode::String(t),
                    _ => SuccessCode::Other,
                }))
            }
            "parseOrderSide" => {
                safe_cex_venue_bitrue::ws_sync::parse_order_side(self, args[0].clone())?
            }
            "parseOrderType" => {
                safe_cex_venue_bitrue::ws_sync::parse_order_type(self, args[0].clone())?
            }
            "parseOrderStatus" => safe_cex_venue_bitrue::ws_sync::parse_order_status(
                self,
                args[0].clone(),
                args[1].clone(),
            )?,
            "addOrUpdateOrder" => {
                assert!(
                    Rc::ptr_eq(&get(&args[0], "info"), &self.expected_update.borrow()),
                    "native original order info"
                );
                self.undefined()
            }
            "trackLastReceived"
            | "emitWsDiagnosticReceive"
            | "recordMessageHandlerStats"
            | "handleAccountUpdate"
            | "handleOrderTradeUpdate"
            | "handlePingEvent"
            | "scheduleListenKeyRefresh"
            | "retryFailedConnect"
            | "emitAndLogError"
            | "addEventListener"
            | "log"
            | "emit" => self.undefined(),
            "update" => {
                self.effect("hmac-update", args.clone())?;
                set(&receiver, "message", args[0].clone());
                receiver
            }
            "digest" => {
                self.effect("hmac-digest", args.clone())?;
                text(&safe_cex_crypto::hmac_sha256(
                    String::from_utf16(&string(&get(&receiver, "secret")))
                        .unwrap()
                        .as_bytes(),
                    String::from_utf16(&string(&get(&receiver, "message")))
                        .unwrap()
                        .as_bytes(),
                    safe_cex_crypto::Encoding::Hex,
                ))
            }
            _ => panic!("outside private binding floor {kind}"),
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
        panic!("unexpected detached promise: {v:?}")
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
fn js_json(v: &V) -> Value {
    match &**v {
        Raw::Undefined => Value::Null,
        Raw::Null => Value::Null,
        Raw::Bool(b) => Value::Bool(*b),
        Raw::Number(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Raw::Text(s) => Value::String(String::from_utf16(s).unwrap()),
        Raw::Array(a) => Value::Array(a.borrow().iter().map(js_json).collect()),
        Raw::Object(_) => {
            let mut out = serde_json::Map::new();
            for (k, v) in ordered(v) {
                if !matches!(&*v, Raw::Undefined) {
                    out.insert(String::from_utf16(&k).unwrap(), js_json(&v));
                }
            }
            Value::Object(out)
        }
        _ => panic!("JSON outside floor"),
    }
}
fn stringify_ordered(v: &V) -> String {
    match &**v {
        Raw::Number(n) => {
            if n.is_finite() {
                safe_cex_num::js_number_string(*n)
            } else {
                "null".to_owned()
            }
        }
        Raw::Object(_) => format!(
            "{{{}}}",
            ordered(v)
                .into_iter()
                .filter(|(_, v)| !matches!(&**v, Raw::Undefined))
                .map(|(k, v)| format!(
                    "{}:{}",
                    serde_json::to_string(&String::from_utf16(&k).unwrap()).unwrap(),
                    stringify_ordered(&v)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Raw::Array(a) => format!(
            "[{}]",
            a.borrow()
                .iter()
                .map(stringify_ordered)
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(&js_json(v)).unwrap(),
    }
}
fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
impl PrivateHost for Host {
    fn contains(&self, v: V, n: &str) -> Result<bool, E> {
        Ok(String::from_utf16(&string(&v)).unwrap().contains(n))
    }
    fn less_equal(&self, a: V, b: V) -> Result<bool, E> {
        Ok(num(&a) <= num(&b))
    }
    fn null_is(&self, v: &V) -> bool {
        matches!(&**v, Raw::Null)
    }
    fn add(&self, a: V, b: V) -> Result<V, E> {
        self.arithmetic(Op::Add, a, b)
    }
    fn copy(&self, v: V) -> Result<V, E> {
        Ok(Rc::new(Raw::Object(RefCell::new(if self.nullish(&v) {
            vec![]
        } else {
            ordered(&v)
        }))))
    }
    fn find(&self, v: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        for x in items(&v) {
            if f(x.clone())? {
                return Ok(x);
            }
        }
        Ok(self.undefined())
    }
    fn is_string(&self, v: &V) -> bool {
        matches!(&**v, Raw::Text(_))
    }
    fn starts_with(&self, v: V, p: &str) -> Result<bool, E> {
        Ok(String::from_utf16(&string(&v)).unwrap().starts_with(p))
    }
    fn split(&self, v: V, s: &str) -> Result<V, E> {
        Ok(array(
            String::from_utf16(&string(&v))
                .unwrap()
                .split(s)
                .map(text)
                .collect(),
        ))
    }
    fn index(&self, v: V, n: usize) -> Result<V, E> {
        Ok(items(&v)
            .get(n)
            .cloned()
            .unwrap_or_else(|| self.undefined()))
    }
    fn stringify(&self, v: V) -> Result<V, E> {
        Ok(text(&stringify_ordered(&v)))
    }
    fn join(&self, v: Vec<V>) -> Result<V, E> {
        self.concat(
            v.into_iter()
                .map(|v| if self.nullish(&v) { text("") } else { v })
                .collect(),
        )
    }
    fn query(&self, v: V) -> Result<V, E> {
        self.effect("query", vec![v.clone()])?;
        Ok(text(
            &ordered(&v)
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        percent(&String::from_utf16(k).unwrap()),
                        percent(&String::from_utf16(&string(v)).unwrap())
                    )
                })
                .collect::<Vec<_>>()
                .join("&"),
        ))
    }
    fn now(&self) -> Result<V, E> {
        self.effect("now", vec![])?;
        Ok(self.number(2000.0))
    }
    fn virtual_now(&self) -> Result<V, E> {
        self.effect("clock", vec![])?;
        Ok(self.number(2000.0))
    }
    fn parse_message(&self, v: V) -> Result<V, E> {
        self.effect("parse", vec![v.clone()])?;
        if let Raw::Text(s) = &*v {
            Ok(
                serde_json::from_str::<Value>(&String::from_utf16(s).unwrap())
                    .map(|x| decode(&x))
                    .unwrap_or(Rc::new(Raw::Null)),
            )
        } else {
            Ok(v)
        }
    }
    fn error_instance(&self, e: &V) -> bool {
        matches!(&**e, Raw::Error(_))
    }
    fn console_args(&self, args: Vec<V>) -> Result<(), E> {
        self.effect("console", args)
    }
    fn delays(&self) -> Result<V, E> {
        Ok(self.delays.clone())
    }
    fn delay_promise(&self, d: V) -> Result<V, E> {
        let out = self.effect("delay", vec![d]).map(|()| self.undefined());
        Ok(self.returned("delay", out))
    }
    fn reverse_symbol(&self, v: V) -> Result<V, E> {
        Ok(text(&safe_cex_venue_bitrue::utils::reverse_ws_symbol(
            &String::from_utf16(&string(&v)).unwrap(),
        )))
    }
    fn create_incremental(&self) -> Result<V, E> {
        self.effect("incremental", vec![])?;
        let history = Rc::new(RefCell::new(Vec::<(Vec<u16>, f64)>::new()));
        let h = self.clone();
        let f = self.named_function(
            "",
            Rc::new(move |a| {
                let id = string(&a[0]);
                let q = num(&a[1]);
                let mut history = history.borrow_mut();
                let last = history
                    .iter()
                    .find(|(k, _)| *k == id)
                    .map(|(_, q)| *q)
                    .unwrap_or(0.0);
                let delta = q - last;
                if delta <= 0.0 || delta.is_nan() {
                    return Ok(object(vec![("qty", h.number(0.0))]));
                }
                history.retain(|(k, _)| *k != id);
                history.push((id, q));
                if history.len() > 500 {
                    history.remove(0);
                }
                Ok(object(vec![("qty", h.number(delta))]))
            }),
        );
        if matches!(&**self.initial_incremental.borrow(), Raw::Undefined) {
            *self.initial_incremental.borrow_mut() = f.clone()
        }
        Ok(f)
    }
    fn axios_headers(&self, v: V) -> Result<V, E> {
        self.effect("headers", vec![v.clone()])?;
        Ok(v)
    }
    fn axios_create(&self, config: V, opts: V, exchange: V) -> Result<V, E> {
        self.effect(
            "create",
            vec![
                config.clone(),
                boolean(Rc::ptr_eq(&opts, &get(&self.cfg, "options"))),
                get(&exchange, "name"),
            ],
        )?;
        *self.api_config.borrow_mut() = config;
        let client = object(vec![(
            "interceptors",
            object(vec![
                (
                    "request",
                    object(vec![("use", Rc::new(Raw::Function("use")))]),
                ),
                (
                    "response",
                    object(vec![("use", Rc::new(Raw::Function("use-response")))]),
                ),
            ]),
        )]);
        *self.client.borrow_mut() = client.clone();
        Ok(client)
    }
    fn transport(&self, c: V, o: V, n: V, p: &str) -> Result<(), E> {
        self.effect(
            "transport",
            vec![
                boolean(Rc::ptr_eq(&c, &self.client.borrow())),
                boolean(Rc::ptr_eq(&o, &get(&self.cfg, "options"))),
                n,
                text(p),
            ],
        )
    }
    fn clean_float(&self, v: V) -> Result<V, E> {
        Ok(self.number(safe_cex_store::incremental_fills::clean_float(num(&v))))
    }
    fn retry_condition(&self) -> Result<V, E> {
        Ok(Rc::new(Raw::Function("isRetryable")))
    }
    fn signature_rejection(&self) -> Result<V, E> {
        Ok(Rc::new(Raw::Function("isBitrueSignatureRejection")))
    }
    fn resend_on_signature_rejection(&self, client: V, _response: V, predicate: V) -> Result<V, E> {
        self.effect(
            "resend",
            vec![
                boolean(Rc::ptr_eq(&client, &self.client.borrow())),
                predicate,
            ],
        )?;
        Ok(self.undefined())
    }
    fn retry(&self, c: V, o: V) -> Result<(), E> {
        self.effect(
            "retry",
            vec![boolean(Rc::ptr_eq(&c, &self.client.borrow())), o],
        )
    }
    fn create_hmac(&self, a: &str, s: V) -> Result<V, E> {
        self.effect("hmac", vec![text(a), s.clone()])?;
        Ok(object(vec![
            ("secret", s),
            ("update", Rc::new(Raw::Function("update"))),
            ("digest", Rc::new(Raw::Function("digest"))),
        ]))
    }
    fn method_uppercase_optional(&self, v: V) -> Result<V, E> {
        if self.nullish(&v) {
            Ok(self.undefined())
        } else {
            self.uppercase(v)
        }
    }
    fn super_socket(&self, args: Vec<V>) -> Result<V, E> {
        self.effect(
            "super",
            vec![
                self.number(args.len() as f64),
                boolean(
                    args.first()
                        .is_some_and(|a| Rc::ptr_eq(a, &get(&self.cfg, "options"))),
                ),
            ],
        )?;
        let r = object(vec![(
            "parent",
            args.first().cloned().unwrap_or_else(|| self.undefined()),
        )]);
        self.instances.borrow_mut().push(r.clone());
        Ok(r)
    }
    fn define(&self, r: V, k: &str, v: V) -> Result<(), E> {
        self.set(&r, k, v)
    }
    fn member(&self, n: &'static str, t: &'static str, r: V) -> V {
        let expected = match n {
            "connectAndSubscribe" => "safe-cex-venue-bitrue/src/private_auth.rs::connect",
            "onOpen" => "safe-cex-venue-bitrue/src/ws_private.rs::on_open",
            "onMessage" => "safe-cex-venue-bitrue/src/private_auth.rs::message",
            "parseOrderType" => "safe-cex-venue-bitrue/src/ws_sync.rs::parse_order_type",
            "parseOrderSide" => "safe-cex-venue-bitrue/src/ws_sync.rs::parse_order_side",
            "parseOrderStatus" => "safe-cex-venue-bitrue/src/ws_sync.rs::parse_order_status",
            "handleOrderTradeUpdate" => "safe-cex-venue-bitrue/src/private_auth.rs::order_update",
            "handleAccountUpdate" => {
                "safe-cex-venue-bitrue/src/refresh_entrypoints.rs::handle_account_update"
            }
            "handlePingEvent" => "safe-cex-venue-bitrue/src/ws_sync.rs::handle_private_ping",
            "ping" => "safe-cex-venue-bitrue/src/ws_private.rs::ping",
            "isSuccessfulCode" => "safe-cex-venue-bitrue/src/ws_private.rs::is_successful_code",
            "scheduleListenKeyRefresh" => {
                "safe-cex-venue-bitrue/src/ws_sync.rs::schedule_listen_key_refresh"
            }
            "fetchListenKey" => "safe-cex-venue-bitrue/src/private_auth.rs::fetch_key",
            "updateListenKey" => "safe-cex-venue-bitrue/src/private_auth.rs::update_key",
            "subscribe" => "safe-cex-venue-bitrue/src/ws_sync.rs::subscribe_private",
            _ => panic!("native unexpected class member {n}"),
        };
        assert_eq!(t, expected, "native exact class ABI");
        let h = self.clone();
        self.named_function(
            n,
            Rc::new(move |_| {
                let _lexical = &r;
                if n == "ping" {
                    safe_cex_venue_bitrue::ws_private::ping();
                    Ok(h.undefined())
                } else {
                    panic!("mandatory class ABI outside call floor {t}")
                }
            }),
        )
    }
    fn class_environment(&self, initial_delays: &[f64], disposal_target: &'static str) {
        assert_eq!(
            initial_delays,
            &[250.0, 500.0, 1000.0],
            "native static retry initializer"
        );
        assert_eq!(
            disposal_target, "safe-cex-venue-bitrue/src/ws_sync.rs::dispose_private",
            "native prototype disposal ABI"
        );
    }
    fn named_function(&self, n: &str, f: Callback<Self>) -> V {
        Rc::new(Raw::Named(n.to_owned(), f))
    }
}
impl safe_cex_venue_bitrue::ws_sync::ParseHost for Host {
    type Value = V;
    type Error = E;
    fn string(&self, v: &V) -> Option<Vec<u16>> {
        if let Raw::Text(t) = &**v {
            Some(t.clone())
        } else {
            None
        }
    }
    fn number(&self, v: &V) -> Option<f64> {
        if let Raw::Number(n) = &**v {
            Some(*n)
        } else {
            None
        }
    }
    fn truthy(&self, v: &V) -> bool {
        ReadHost::truthy(self, v)
    }
    fn empty_string(&self) -> V {
        text("")
    }
    fn template(&self, v: V) -> Result<Vec<u16>, E> {
        Ok(string(&v))
    }
    fn uppercase(&self, v: Vec<u16>) -> Result<Vec<u16>, E> {
        Ok(units(&String::from_utf16(&v).unwrap().to_uppercase()))
    }
    fn lookup(&self, t: &'static str, k: Vec<u16>) -> Result<V, E> {
        self.table(t, Rc::new(Raw::Text(k)))
    }
    fn order_type(&self, v: safe_cex_types::OrderType) -> V {
        decode(&serde_json::to_value(v).unwrap())
    }
    fn order_side(&self, v: safe_cex_types::OrderSide) -> V {
        decode(&serde_json::to_value(v).unwrap())
    }
    fn order_status(&self, v: safe_cex_types::OrderStatus) -> V {
        decode(&serde_json::to_value(v).unwrap())
    }
}
fn setup(group: &str, cfg: V) -> Host {
    let undef = Rc::new(Raw::Undefined);
    let error = if flag(&cfg, "plainError") {
        object(vec![("message", text("plain cause"))])
    } else {
        Rc::new(Raw::Error(text("original failure")))
    };
    let xhr = object(vec![
        ("post", Rc::new(Raw::Function("post"))),
        ("put", Rc::new(Raw::Function("put"))),
    ]);
    let r = object(vec![
        ("isDisposed", get(&cfg, "disposed")),
        (
            "parent",
            object(vec![
                ("store", object(vec![("markets", get(&cfg, "markets"))])),
                ("xhr", xhr),
                ("log", Rc::new(Raw::Function("log"))),
            ]),
        ),
        (
            "emitter",
            object(vec![("emit", Rc::new(Raw::Function("emit")))]),
        ),
        (
            "store",
            object(vec![(
                "addOrUpdateOrder",
                Rc::new(Raw::Function("addOrUpdateOrder")),
            )]),
        ),
    ]);
    for n in [
        "parseOrderType",
        "parseOrderSide",
        "parseOrderStatus",
        "isSuccessfulCode",
        "trackLastReceived",
        "emitWsDiagnosticReceive",
        "recordMessageHandlerStats",
        "handleAccountUpdate",
        "handleOrderTradeUpdate",
        "handlePingEvent",
        "scheduleListenKeyRefresh",
        "retryFailedConnect",
        "emitAndLogError",
        "fetchListenKey",
        "createWebsocket",
    ] {
        set(&r, n, Rc::new(Raw::Function(n)));
    }
    for n in ["onOpen", "onMessage", "onClose"] {
        set(&r, n, Rc::new(Raw::Function("handler")));
    }
    let h = Host(Rc::new(State {
        cfg,
        r,
        error,
        events: RefCell::new(vec![]),
        post: Cell::new(0),
        delays: array(vec![
            Rc::new(Raw::Number(250.0)),
            Rc::new(Raw::Number(500.0)),
            Rc::new(Raw::Number(1000.0)),
        ]),
        client: RefCell::new(undef.clone()),
        api_config: RefCell::new(undef.clone()),
        interceptor: RefCell::new(undef.clone()),
        response_interceptor: RefCell::new(undef.clone()),
        initial_incremental: RefCell::new(undef.clone()),
        expected_update: RefCell::new(undef),
        prototype_dispose: Rc::new(Raw::Function("dispose")),
        instances: RefCell::new(vec![]),
    }));
    if group == "order" {
        let history = Rc::new(RefCell::new(Vec::<(Vec<u16>, f64)>::new()));
        let owner = h.clone();
        set(
            &h.r,
            "incrementalFills",
            h.named_function(
                "",
                Rc::new(move |a| {
                    owner.effect("incrementalFills", a.clone())?;
                    let id = string(&a[0]);
                    let q = num(&a[1]);
                    let mut hist = history.borrow_mut();
                    let last = hist
                        .iter()
                        .find(|(k, _)| *k == id)
                        .map(|(_, q)| *q)
                        .unwrap_or(0.0);
                    let d = q - last;
                    // JS `!(d > 0)`: a NaN duration must take this branch,
                    // which clippy's `d <= 0.0` rewrite would skip.
                    #[allow(clippy::neg_cmp_op_on_partial_ord)]
                    if !(d > 0.0) {
                        return Ok(object(vec![("qty", owner.number(0.0))]));
                    }
                    hist.retain(|(k, _)| *k != id);
                    hist.push((id, q));
                    Ok(object(vec![("qty", owner.number(d))]))
                }),
            ),
        );
    }
    h
}
fn settle(f: impl Future<Output = Result<V, E>>) -> Result<V, E> {
    let mut f = std::pin::pin!(f);
    for _ in 0..1000 {
        if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return r;
        }
    }
    panic!("native bounded witness did not settle")
}
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private-auth-oracle.cjs"
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
    let cfg = decode(&row["cfg"]);
    let id = String::from_utf16(&string(&get(&cfg, "id"))).unwrap();
    let p = format!("{group}/{id}");
    let h = setup(group, cfg);
    let mut identity = h.undefined();
    let outcome = (|| -> Result<V, E> {
        match group {
            "api" => {
                let out = private::create_api(
                    &h,
                    get(&h.cfg, "options"),
                    object(vec![("name", text("Bitrue"))]),
                )?;
                let serial = get(
                    &get(&h.api_config.borrow(), "paramsSerializer"),
                    "serialize",
                );
                let q = h.call(
                    serial,
                    h.undefined(),
                    vec![object(vec![("z", text("last")), ("a", text("with space"))])],
                )?;
                h.effect("serialize", vec![q])?;
                if flag(&h.cfg, "mutate") {
                    let o = get(&h.cfg, "options");
                    set(&o, "key", text("changed"));
                    set(&o, "secret", text("new-secret"));
                    set(&get(&o, "extra"), "recvWindow", h.number(9.0));
                }
                let c = get(&h.cfg, "config");
                let result = h.call(
                    h.interceptor.borrow().clone(),
                    h.undefined(),
                    vec![c.clone()],
                )?;
                let response = object(vec![("data", object(vec![("code", h.number(0.0))]))]);
                let kept = h.call(
                    h.response_interceptor.borrow().clone(),
                    h.undefined(),
                    vec![response.clone()],
                )?;
                identity = object(vec![
                    ("client", boolean(Rc::ptr_eq(&out, &h.client.borrow()))),
                    ("fresh", boolean(!Rc::ptr_eq(&result, &c))),
                    (
                        "data",
                        boolean(same(&get(&result, "data"), &get(&c, "data"))),
                    ),
                    (
                        "params",
                        boolean(same(&get(&result, "params"), &get(&c, "params"))),
                    ),
                    (
                        "headers",
                        boolean(same(&get(&result, "headers"), &get(&c, "headers"))),
                    ),
                    ("responseKept", boolean(Rc::ptr_eq(&kept, &response))),
                ]);
                Ok(result)
            }
            "class" => {
                let args = if flag(&h.cfg, "noArgs") {
                    vec![]
                } else {
                    vec![
                        get(&h.cfg, "options"),
                        object(vec![("extra", h.number(1.0))]),
                    ]
                };
                let first = private::construct(&h, args.clone())?;
                native(
                    &get(&first, "listenKeyRefreshTimeoutId"),
                    &serde_json::json!({"undefined":true}),
                    &format!("{p}.timeout"),
                );
                let result = array(
                    ordered(&first)
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
                );
                identity = object(vec![(
                    "incremental",
                    boolean(Rc::ptr_eq(
                        &get(&first, "incrementalFills"),
                        &h.initial_incremental.borrow(),
                    )),
                )]);
                if flag(&h.cfg, "pair") {
                    let delays = h.delays.clone();
                    let Raw::Array(a) = &*delays else {
                        panic!("class static")
                    };
                    a.borrow_mut()[0] = h.number(125.0);
                    let second = private::construct(&h, args)?;
                    set(
                        &identity,
                        "fresh",
                        boolean(
                            ordered(&first)
                                .iter()
                                .filter(|(_, v)| {
                                    matches!(&**v, Raw::Named(_, _) | Raw::Function(_))
                                })
                                .all(|(k, v)| {
                                    !Rc::ptr_eq(v, &get(&second, &String::from_utf16(k).unwrap()))
                                }),
                        ),
                    );
                    set(
                        &identity,
                        "static",
                        boolean(
                            Rc::ptr_eq(&delays, &h.delays) && num(&items(&h.delays)[0]) == 125.0,
                        ),
                    );
                    let returned = h.call(get(&first, "ping"), second.clone(), vec![])?;
                    set(
                        &identity,
                        "void",
                        boolean(matches!(&*returned, Raw::Undefined)),
                    );
                    set(
                        &identity,
                        "prototype",
                        boolean(
                            !ordered(&first).iter().any(|(k, _)| *k == units("dispose"))
                                && Rc::ptr_eq(
                                    &h.get(&first, "dispose")?,
                                    &h.get(&second, "dispose")?,
                                ),
                        ),
                    );
                }
                Ok(result)
            }
            "incremental" => {
                let v = private::incremental_fills(&h)?;
                let eq = Rc::ptr_eq(&v, &h.initial_incremental.borrow());
                identity = boolean(eq);
                Ok(object(vec![("factory", boolean(eq))]))
            }
            "connect" => settle(private::connect(&h, h.r.clone())),
            "message" => private::message(
                &h,
                h.r.clone(),
                object(vec![(
                    "data",
                    if matches!(&*get(&h.cfg, "raw"), Raw::Undefined) {
                        get(&h.cfg, "json")
                    } else {
                        get(&h.cfg, "raw")
                    },
                )]),
            ),
            "order" => {
                for json in items(&get(&h.cfg, "updates")) {
                    let o = get(&json, "o");
                    *h.expected_update.borrow_mut() = if h.truthy(&o) { o } else { json.clone() };
                    private::order_update(&h, h.r.clone(), json)?;
                }
                Ok(h.undefined())
            }
            "fetch" => settle(private::fetch_key(&h, h.r.clone())),
            "refresh" => settle(private::update_key(&h, h.r.clone(), get(&h.cfg, "key"))),
            _ => panic!("family"),
        }
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
    native(&array(h.events.borrow().clone()), &row["events"], &p);
    native(&result, &row["result"], &p);
    native(&identity, &row["identity"], &p);
    native(&error, &row["error"], &p);
    native(&get(&h.cfg, "options"), &row["optionsAfter"], &p);
    native(&get(&h.cfg, "config"), &row["configAfter"], &p);
    native(&get(&h.r, "isDisposed"), &row["disposed"], &p);
}
fn family(group: &str, n: usize) {
    let rows = oracle()["rows"][group].as_array().unwrap();
    assert_eq!(rows.len(), n, "independent case floor {group}");
    for row in rows {
        run(group, row)
    }
}
#[test]
fn api_native() {
    family("api", 10)
}
#[test]
fn class_native() {
    family("class", 4)
}
#[test]
fn incremental_native() {
    family("incremental", 2)
}
#[test]
fn connect_native() {
    family("connect", 6)
}
#[test]
fn message_native() {
    family("message", 13)
}
#[test]
fn order_native() {
    family("order", 10)
}
#[test]
fn fetch_native() {
    family("fetch", 10)
}
#[test]
fn refresh_native() {
    family("refresh", 7)
}
