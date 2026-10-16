use safe_cex_venue_orderly::{
    account_order_reads::AccountReadHost,
    api::{self, ApiHost},
    classes::{BodyFuture, BoundMember, NativeMember, Return},
    data_mappers::{DataHost, Op},
    public_feeds::{self as feeds, FeedHost},
    socket_classes::{self as sockets, SocketHost},
    ws_controls::WsHost,
};
use serde_json::Value;
use std::{cell::RefCell, ops::Deref, process::Command, rc::Rc, sync::OnceLock};
use std::{
    future::Future,
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
    Bound(Rc<Callable>),
    Method(Rc<dyn Fn(Option<V>, Vec<V>) -> Result<V, E>>),
    Promise(usize),
}
type V = Rc<Raw>;
type E = V;
impl std::fmt::Debug for Raw {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native class value")
    }
}
struct Callable {
    name: String,
    body: BoundMember<Host>,
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
        Raw::Function(_) | Raw::Bound(_) | Raw::Method(_) | Raw::Promise(_) => {
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
        Raw::Method(_) => panic!("method not captured"),
        Raw::Bound(f) => assert_eq!(expected["function"], f.name, "native bound function name"),
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
        Raw::Bound(f) => Raw::Bound(f.clone()),
        Raw::Method(f) => Raw::Method(f.clone()),
        Raw::Promise(i) => Raw::Promise(*i),
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
struct State {
    cfg: V,
    events: RefCell<Vec<V>>,
    original: V,
    flags: RefCell<Vec<(usize, Vec<u16>, (bool, bool, bool))>>,
    timers: RefCell<Vec<(V, bool)>>,
    pending: RefCell<Vec<Option<Result<V, E>>>>,
    tasks: RefCell<Vec<BodyFuture<Host>>>,
    detached: RefCell<Vec<E>>,
    client: RefCell<Option<V>>,
    api_config: RefCell<Option<V>>,
    interceptor: RefCell<Option<V>>,
    last_book: RefCell<Option<V>>,
    group: String,
}
impl Host {
    fn same_value(&self, a: &V, b: &V) -> bool {
        match (&**a, &**b) {
            (Raw::Undefined, Raw::Undefined) | (Raw::Null, Raw::Null) => true,
            (Raw::Text(a), Raw::Text(b)) => a == b,
            (Raw::Number(a), Raw::Number(b)) => a == b,
            (Raw::Bool(a), Raw::Bool(b)) => a == b,
            _ => Rc::ptr_eq(a, b),
        }
    }
    fn effect(&self, kind: &str, fields: Vec<(&str, V)>) -> Result<(), E> {
        self.events.borrow_mut().push(object(
            vec![("kind", text(kind))]
                .into_iter()
                .chain(fields.into_iter().map(|(k, v)| (k, snapshot(&v))))
                .collect(),
        ));
        if self.same_value(&get(&self.cfg, "fail"), &text(kind)) {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn own_define(&self, r: &V, key: &str, v: V) {
        set(r, key, v);
        let id = Rc::as_ptr(r) as usize;
        let mut flags = self.flags.borrow_mut();
        if let Some(e) = flags.iter_mut().find(|e| e.0 == id && e.1 == units(key)) {
            e.2 = (true, true, true)
        } else {
            flags.push((id, units(key), (true, true, true)))
        }
    }
    fn flags(&self, r: &V, key: &[u16]) -> (bool, bool, bool) {
        self.flags
            .borrow()
            .iter()
            .find(|e| e.0 == Rc::as_ptr(r) as usize && e.1 == key)
            .unwrap()
            .2
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
}
impl WsHost for Host {
    type Value = Raw;
    type Error = E;
    type Iterator = (V, usize);
    fn get(&self, v: &V, k: &[u16]) -> Result<V, E> {
        let k = String::from_utf16(k).unwrap();
        if let Raw::Array(a) = &**v {
            if k == "length" {
                return Ok(self.number(a.borrow().len() as f64));
            }
        }
        if self.nullish(v) {
            return Err(self.original.clone());
        }
        Ok(get(v, &k))
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
        if flag(v, "throwString") {
            self.effect("url-string", vec![])?;
            return Err(self.original.clone());
        }
        Ok(string(v))
    }
    fn text(&self, v: &[u16]) -> V {
        Rc::new(Raw::Text(v.to_vec()))
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(v))
    }
    fn stringify(&self, v: V) -> Result<V, E> {
        Ok(text(&json(&v)))
    }
    fn parse(&self, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
    fn includes(&self, _: &V, _: &[u16]) -> Result<bool, E> {
        panic!("outside class invocation floor")
    }
    fn own_values(&self, _: V) -> Result<Vec<V>, E> {
        panic!("outside class invocation floor")
    }
    fn flatten_one(&self, _: Vec<V>) -> Result<Vec<V>, E> {
        panic!("outside class invocation floor")
    }
    fn entries(&self, v: V) -> Result<Vec<(Vec<u16>, V)>, E> {
        Ok(ordered(&v))
    }
    fn iterator(&self, v: V) -> Result<Self::Iterator, E> {
        Ok((v, 0))
    }
    fn next(&self, i: &mut Self::Iterator) -> Result<Option<V>, E> {
        let out = items(&i.0).get(i.1).cloned();
        i.1 += 1;
        Ok(out)
    }
    fn call(&self, f: V, receiver: Option<V>, args: Vec<V>) -> Result<V, E> {
        match &*f {
            Raw::Bound(f) => settle((f.body)(args)?),
            Raw::Method(f) => f(receiver, args),
            Raw::Function("use") => {
                let client = self.client.borrow().as_ref().unwrap().clone();
                self.effect(
                    "use",
                    vec![(
                        "receiverIdentity",
                        boolean(Rc::ptr_eq(
                            receiver.as_ref().unwrap(),
                            &get(&get(&client, "interceptors"), "request"),
                        )),
                    )],
                )?;
                *self.interceptor.borrow_mut() = Some(args[0].clone());
                Ok(self.undefined())
            }
            Raw::Function("send") => {
                self.effect("send", vec![("frame", args[0].clone())])?;
                Ok(self.undefined())
            }
            Raw::Function("cleanup") => {
                self.effect("cleanup", vec![("topic", args[0].clone())])?;
                Ok(self.undefined())
            }
            Raw::Function("error") => {
                self.effect("error", vec![("message", args[0].clone())])?;
                Ok(self.undefined())
            }
            Raw::Function("request") => {
                self.effect("request", vec![("url", args[0].clone())])?;
                let mut p = self.pending.borrow_mut();
                let i = p.len();
                p.push(None);
                Ok(Rc::new(Raw::Promise(i)))
            }
            Raw::Function("process") => feeds::process_book(self, args[0].clone(), args[1].clone()),
            Raw::Function("callback") => {
                let same = self.group == "book"
                    && self
                        .last_book
                        .borrow()
                        .as_ref()
                        .is_none_or(|v| Rc::ptr_eq(v, &args[0]));
                self.effect(
                    "callback",
                    vec![("value", args[0].clone()), ("sameBook", boolean(same))],
                )?;
                *self.last_book.borrow_mut() = Some(args[0].clone());
                Ok(self.undefined())
            }
            _ => panic!("outside finite body binding"),
        }
    }
    fn monotonic_now(&self) -> Result<V, E> {
        panic!("outside floor")
    }
    fn virtual_now(&self) -> Result<V, E> {
        self.effect("clock", vec![])?;
        Ok(self.number(1234.0))
    }
    fn sign(&self, message: V, secret: V) -> Result<V, E> {
        self.effect(
            "sign",
            vec![
                ("message", message),
                ("secret", secret.clone()),
                (
                    "secretIdentity",
                    boolean(self.same_value(&secret, &get(&get(&self.cfg, "options"), "secret"))),
                ),
            ],
        )?;
        Ok(text("signature"))
    }
    fn reconnecting(&self) -> bool {
        false
    }
}
impl AccountReadHost for Host {
    fn await_value(&self, v: V) -> impl Future<Output = Result<V, E>> {
        std::future::poll_fn(move |_| {
            let Raw::Promise(i) = &*v else {
                return Poll::Ready(Ok(v.clone()));
            };
            match self.pending.borrow_mut()[*i].take() {
                Some(r) => Poll::Ready(r),
                None => Poll::Pending,
            }
        })
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
    fn array(&self, v: Vec<V>) -> V {
        array(v)
    }
    fn is_array(&self, v: &Raw) -> bool {
        matches!(v, Raw::Array(_))
    }
    fn spread(&self, v: V) -> Result<Vec<V>, E> {
        Ok(items(&v))
    }
    fn reduce(
        &self,
        v: V,
        mut initial: V,
        mut f: impl FnMut(V, V) -> Result<V, E>,
    ) -> Result<V, E> {
        for row in items(&v) {
            initial = f(initial, row)?
        }
        Ok(initial)
    }
    fn normalize_symbol(&self, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
    fn get_key(&self, _: V, _: &str) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
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
            Op::Min => a.min(b),
            Op::Max => a.max(b),
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
    fn split(&self, _: V) -> Result<Vec<V>, E> {
        panic!("outside class invocation floor")
    }
    fn starts_with(&self, v: V, p: &str) -> Result<bool, E> {
        let Raw::Text(t) = &*v else {
            panic!("string class key floor")
        };
        Ok(t.starts_with(&units(p)))
    }
    fn filter(&self, v: V, mut f: impl FnMut(V) -> Result<bool, E>) -> Result<V, E> {
        let mut out = vec![];
        for r in items(&v) {
            if f(r.clone())? {
                out.push(r)
            }
        }
        Ok(array(out))
    }
    fn map(&self, v: V, mut f: impl FnMut(V) -> Result<V, E>) -> Result<V, E> {
        Ok(array(
            items(&v)
                .into_iter()
                .map(&mut f)
                .collect::<Result<Vec<_>, _>>()?,
        ))
    }
    fn clone_deep(&self, v: V) -> Result<V, E> {
        Ok(snapshot(&v))
    }
    fn merge(&self, v: V, fields: Vec<(&'static str, V)>) -> Result<V, E> {
        let out = Rc::new(Raw::Object(RefCell::new(ordered(&v))));
        for (k, v) in fields {
            set(&out, k, v)
        }
        Ok(out)
    }
    fn uniq_id(&self, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
    fn reverse(&self, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
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
    fn order_type(&self, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
    fn order_side(&self, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
    fn subtract(&self, a: V, b: V) -> Result<V, E> {
        Ok(self.number(safe_cex_num::subtract(self.num(&a), self.num(&b))))
    }
    fn is_undefined(&self, v: &Raw) -> bool {
        matches!(v, Raw::Undefined)
    }
    fn boolean(&self, v: bool) -> V {
        boolean(v)
    }
}
fn json(v: &Raw) -> String {
    match v {
        Raw::Undefined | Raw::Function(_) | Raw::Bound(_) | Raw::Method(_) => "null".into(),
        Raw::Null => "null".into(),
        Raw::Bool(b) => b.to_string(),
        Raw::Number(n) => {
            if n.is_finite() {
                safe_cex_num::js_number_string(*n)
            } else {
                "null".into()
            }
        }
        Raw::Text(s) => serde_json::to_string(&String::from_utf16(s).unwrap()).unwrap(),
        Raw::Array(a) => format!(
            "[{}]",
            a.borrow()
                .iter()
                .map(|v| json(v))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Raw::Object(_) => format!(
            "{{{}}}",
            ordered(v)
                .into_iter()
                .filter(|(_, v)| !matches!(
                    &**v,
                    Raw::Undefined | Raw::Function(_) | Raw::Bound(_) | Raw::Method(_)
                ))
                .map(|(k, v)| format!(
                    "{}:{}",
                    serde_json::to_string(&String::from_utf16(&k).unwrap()).unwrap(),
                    json(&v)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        Raw::Promise(_) => panic!("outside serialization floor"),
    }
}
fn percent(s: &str) -> String {
    s.as_bytes()
        .iter()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(b) {
                (*b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn bound(name: &str, body: BoundMember<Host>) -> V {
    Rc::new(Raw::Bound(Rc::new(Callable {
        name: name.into(),
        body,
    })))
}
impl ApiHost for Host {
    fn spread_object(&self, v: V) -> Result<V, E> {
        Ok(Rc::new(Raw::Object(RefCell::new(if self.nullish(&v) {
            vec![]
        } else {
            ordered(&v)
        }))))
    }
    fn delete(&self, v: V, k: &str) -> Result<(), E> {
        FeedHost::delete(self, v, &units(k))
    }
    fn uppercase_optional(&self, v: V) -> Result<V, E> {
        Ok(if let Raw::Text(s) = &*v {
            text(&String::from_utf16(s).unwrap().to_uppercase())
        } else {
            self.undefined()
        })
    }
    fn includes_optional(&self, v: V, n: &str) -> Result<bool, E> {
        Ok(matches!(&*v,Raw::Text(s) if String::from_utf16(s).unwrap().contains(n)))
    }
    fn keys_len(&self, v: V) -> Result<usize, E> {
        Ok(match &*v {
            Raw::Object(o) => o.borrow().len(),
            Raw::Array(a) => a.borrow().len(),
            Raw::Text(s) => s.len(),
            _ => 0,
        })
    }
    fn query(&self, v: V, sort: bool) -> Result<V, E> {
        self.effect(
            "query",
            vec![("params", v.clone()), ("sorted", boolean(sort))],
        )?;
        let mut pairs = ordered(&v);
        if sort {
            pairs.sort_by(|a, b| a.0.cmp(&b.0))
        }
        Ok(text(
            &pairs
                .into_iter()
                .filter(|(_, v)| !matches!(&**v, Raw::Undefined))
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        percent(&String::from_utf16(&k).unwrap()),
                        percent(&if self.nullish(&v) {
                            String::new()
                        } else {
                            String::from_utf16(&string(&v)).unwrap()
                        })
                    )
                })
                .collect::<Vec<_>>()
                .join("&"),
        ))
    }
    fn join(&self, v: Vec<V>) -> Result<V, E> {
        Ok(text(
            &v.into_iter()
                .map(|v| {
                    if self.nullish(&v) {
                        String::new()
                    } else {
                        String::from_utf16(&string(&v)).unwrap()
                    }
                })
                .collect::<String>(),
        ))
    }
    fn axios_headers(&self, v: V) -> Result<V, E> {
        self.effect("headers", vec![("fields", v.clone())])?;
        Ok(v)
    }
    fn function(&self, n: &str, b: BoundMember<Self>) -> V {
        bound(n, b)
    }
    fn axios_create(&self, c: V, o: V, e: V) -> Result<V, E> {
        self.effect(
            "create",
            vec![
                ("config", c.clone()),
                (
                    "optionsIdentity",
                    boolean(Rc::ptr_eq(&o, &get(&self.cfg, "options"))),
                ),
                ("name", get(&e, "name")),
            ],
        )?;
        *self.api_config.borrow_mut() = Some(c);
        let client = object(vec![(
            "interceptors",
            object(vec![(
                "request",
                object(vec![("use", Rc::new(Raw::Function("use")))]),
            )]),
        )]);
        *self.client.borrow_mut() = Some(client.clone());
        Ok(client)
    }
    fn network_retry_condition(&self) -> Result<V, E> {
        self.effect("condition", vec![])?;
        Ok(text("condition"))
    }
    fn retry(&self, c: V, o: V) -> Result<(), E> {
        self.effect(
            "retry",
            vec![
                (
                    "clientIdentity",
                    boolean(Rc::ptr_eq(&c, self.client.borrow().as_ref().unwrap())),
                ),
                ("options", o),
            ],
        )
    }
    fn transport(&self, c: V, o: V, n: V) -> Result<(), E> {
        self.effect(
            "transport",
            vec![
                (
                    "clientIdentity",
                    boolean(Rc::ptr_eq(&c, self.client.borrow().as_ref().unwrap())),
                ),
                (
                    "optionsIdentity",
                    boolean(Rc::ptr_eq(&o, &get(&self.cfg, "options"))),
                ),
                ("name", n),
            ],
        )
    }
}
impl FeedHost for Host {
    fn function(&self, n: &str, b: BoundMember<Self>) -> V {
        bound(n, b)
    }
    fn timeout(&self, cb: V, delay: f64) -> Result<V, E> {
        let id = self.timers.borrow().len() + 1;
        self.timers.borrow_mut().push((cb, false));
        self.effect(
            "timer",
            vec![
                ("id", self.number(id as f64)),
                ("delay", self.number(delay)),
            ],
        )?;
        Ok(self.number(id as f64))
    }
    fn clear_timeout(&self, id: V) -> Result<(), E> {
        self.timers.borrow_mut()[self.num(&id) as usize - 1].1 = true;
        self.effect("clear", vec![("id", id)])
    }
    fn spawn(&self, mut f: BodyFuture<Self>) {
        match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Pending => self.tasks.borrow_mut().push(f),
            Poll::Ready(Err(e)) => self.detached.borrow_mut().push(e),
            Poll::Ready(Ok(_)) => {}
        }
    }
    fn strict_eq(&self, a: &Raw, b: &Raw) -> bool {
        match (a, b) {
            (Raw::Number(a), Raw::Number(b)) => a == b,
            (Raw::Null, Raw::Null) | (Raw::Undefined, Raw::Undefined) => true,
            (Raw::Text(a), Raw::Text(b)) => a == b,
            (Raw::Bool(a), Raw::Bool(b)) => a == b,
            _ => std::ptr::eq(a, b),
        }
    }
    fn delete(&self, v: V, k: &[u16]) -> Result<(), E> {
        let Raw::Object(o) = &*v else {
            panic!("fixture delete")
        };
        o.borrow_mut().retain(|(key, _)| key != k);
        Ok(())
    }
    fn for_each(&self, v: V, mut f: impl FnMut(V) -> Result<(), E>) -> Result<(), E> {
        let len = items(&v).len();
        for i in 0..len {
            if let Some(v) = items(&v).get(i) {
                f(v.clone())?
            }
        }
        Ok(())
    }
    fn pair(&self, v: V) -> Result<(V, V), E> {
        let a = items(&v);
        Ok((
            a.first().cloned().unwrap_or_else(|| self.undefined()),
            a.get(1).cloned().unwrap_or_else(|| self.undefined()),
        ))
    }
    fn update_level(&self, b: V, side: &str, update: V) -> Result<(), E> {
        let rows = get(&b, side);
        let Raw::Array(rows) = &*rows else {
            panic!("book levels")
        };
        let mut rows = rows.borrow_mut();
        let price = get(&update, "price");
        let amount = get(&update, "amount");
        let mut left = 0usize;
        let mut right = rows.len();
        while left < right {
            let mid = (left + right) / 2;
            let mp = get(&rows[mid], "price");
            if FeedHost::strict_eq(self, &mp, &price) {
                if FeedHost::strict_eq(self, &amount, &self.number(0.0)) {
                    rows.remove(mid);
                } else {
                    set(&rows[mid], "amount", amount);
                }
                return Ok(());
            }
            if if side == "bids" {
                self.num(&mp) > self.num(&price)
            } else {
                self.num(&mp) < self.num(&price)
            } {
                left = mid + 1
            } else {
                right = mid
            }
        }
        if !FeedHost::strict_eq(self, &amount, &self.number(0.0)) {
            rows.insert(left, update);
        }
        Ok(())
    }
    fn calc_total(&self, b: V) -> Result<(), E> {
        for side in ["bids", "asks"] {
            let mut total = self.number(0.0);
            for row in items(&get(&b, side)) {
                let amount = get(&row, "amount");
                total = if matches!(&*total, Raw::Text(_)) || matches!(&*amount, Raw::Text(_)) {
                    text(&format!(
                        "{}{}",
                        String::from_utf16(&string(&total)).unwrap(),
                        String::from_utf16(&string(&amount)).unwrap()
                    ))
                } else {
                    self.number(self.num(&total) + self.num(&amount))
                };
                set(&row, "total", total.clone());
            }
        }
        Ok(())
    }
}
impl SocketHost for Host {
    fn super_socket(&self, private: bool, args: Vec<V>) -> Result<V, E> {
        self.effect(
            "super",
            vec![
                ("private", boolean(private)),
                ("argCount", self.number(args.len() as f64)),
                (
                    "parentIdentity",
                    boolean(
                        args.first()
                            .is_some_and(|v| Rc::ptr_eq(v, &get(&self.cfg, "options"))),
                    ),
                ),
            ],
        )?;
        let r = object(vec![]);
        self.own_define(
            &r,
            "parent",
            args.first().cloned().unwrap_or_else(|| self.undefined()),
        );
        Ok(r)
    }
    fn define(&self, r: V, k: &str, v: V) -> Result<(), E> {
        self.own_define(&r, k, v);
        Ok(())
    }
    fn member(
        &self,
        private: bool,
        name: &'static str,
        target: &'static str,
    ) -> NativeMember<Self> {
        assert!(target.starts_with("safe-cex-venue-orderly/src/"));
        assert!(sockets::definitions(private).iter().any(|d| d.name == name));
        let h = self.clone();
        Rc::new(move |r, args| match name {
            "listenOHLCV" => Ok(Return::Value(feeds::listen_candles(
                &h,
                r,
                args[0].clone(),
                args[1].clone(),
            )?)),
            "listenOrderBook" => Ok(Return::Value(feeds::listen_book(
                &h,
                r,
                args[0].clone(),
                args[1].clone(),
            )?)),
            "processOrderBookUpdate" => Ok(Return::Value(feeds::process_book(
                &h,
                args[0].clone(),
                args[1].clone(),
            )?)),
            _ => panic!("member ABI outside selected composition invocation floor: {name}"),
        })
    }
    fn function(&self, n: &str, b: BoundMember<Self>) -> V {
        bound(n, b)
    }
}
fn setup(cfg: &Value, group: &str) -> Host {
    Host(Rc::new(State {
        cfg: decode(cfg),
        events: RefCell::new(vec![]),
        original: object(vec![("message", text("original failure"))]),
        flags: RefCell::new(vec![]),
        timers: RefCell::new(vec![]),
        pending: RefCell::new(vec![]),
        tasks: RefCell::new(vec![]),
        detached: RefCell::new(vec![]),
        client: RefCell::new(None),
        api_config: RefCell::new(None),
        interceptor: RefCell::new(None),
        last_book: RefCell::new(None),
        group: group.into(),
    }))
}
fn settle(ret: Return<Host>) -> Result<V, E> {
    match ret {
        Return::Value(v) => Ok(v),
        Return::Future(mut f) => match f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            Poll::Ready(r) => r,
            Poll::Pending => panic!("outside finite synchronous floor"),
        },
    }
}
fn drain(h: &Host) {
    let fs = std::mem::take(&mut *h.tasks.borrow_mut());
    for f in fs {
        FeedHost::spawn(h, f)
    }
}
fn receiver(h: &Host) -> V {
    object(vec![
        ("isConnected", get(&h.cfg, "connected")),
        ("isDisposed", boolean(flag(&h.cfg, "disposed"))),
        ("messageHandlers", object(vec![])),
        ("topics", object(vec![])),
        ("send", Rc::new(Raw::Function("send"))),
        (
            "cleanupMessageHandlerStats",
            Rc::new(Raw::Function("cleanup")),
        ),
        ("emitAndLogError", Rc::new(Raw::Function("error"))),
        ("processOrderBookUpdate", Rc::new(Raw::Function("process"))),
        (
            "parent",
            object(vec![(
                "xhr",
                object(vec![("get", Rc::new(Raw::Function("request")))]),
            )]),
        ),
    ])
}
fn shape(h: &Host, r: &V) -> V {
    array(
        ordered(r)
            .into_iter()
            .map(|(k, v)| {
                let (w, e, c) = h.flags(r, &k);
                object(vec![
                    ("key", h.text(&k)),
                    ("writable", boolean(w)),
                    ("enumerable", boolean(e)),
                    ("configurable", boolean(c)),
                    ("value", v),
                ])
            })
            .collect(),
    )
}
fn run(h: &Host, g: &str) -> Result<(V, V), E> {
    let c = &h.cfg;
    let config = get(c, "config");
    let options = get(c, "options");
    if g == "sign" {
        let out = api::sign_request(h, config.clone(), options)?;
        let identity = object(vec![
            ("fresh", boolean(!Rc::ptr_eq(&out, &config))),
            (
                "nestedData",
                boolean(h.same_value(&get(&out, "data"), &get(&config, "data"))),
            ),
            (
                "paramsOriginal",
                boolean(ordered(&config).iter().any(|(k, _)| *k == units("params"))),
            ),
        ]);
        return Ok((out, identity));
    }
    if g == "api" {
        let out = api::create_api(h, options.clone(), object(vec![("name", text("Orderly"))]))?;
        let identity = object(vec![(
            "client",
            boolean(Rc::ptr_eq(&out, h.client.borrow().as_ref().unwrap())),
        )]);
        let config0 = h.api_config.borrow().as_ref().unwrap().clone();
        let f = get(&get(&config0, "paramsSerializer"), "serialize");
        let v = h.call(
            f,
            None,
            vec![object(vec![("z", text("last")), ("a", text("with space"))])],
        )?;
        h.effect("serialize", vec![("value", v)])?;
        if flag(c, "mutate") {
            set(&options, "testnet", boolean(true));
            set(&options, "secret", text("changed"));
        }
        let f = h.interceptor.borrow().as_ref().unwrap().clone();
        let out = h.call(f, None, vec![config.clone()])?;
        set(
            &identity,
            "publicOriginal",
            boolean(Rc::ptr_eq(&out, &config)),
        );
        return Ok((out, identity));
    }
    if g == "process" {
        let book = object(vec![("bids", array(vec![])), ("asks", array(vec![]))]);
        for u in items(&get(c, "updates")) {
            feeds::process_book(h, book.clone(), u)?;
        }
        return Ok((book, h.undefined()));
    }
    if g == "public" || g == "private" {
        let private = g == "private";
        let args = if flag(c, "noArgs") {
            vec![]
        } else {
            vec![options.clone(), object(vec![("extra", h.number(1.0))])]
        };
        let first = sockets::construct(h, private, args.clone())?;
        let out = shape(h, &first);
        let identity = object(vec![]);
        assert_eq!(
            ordered(&first).len(),
            if private { 9 } else { 16 },
            "native independent socket own field floor"
        );
        if flag(c, "pair") {
            let second = sockets::construct(h, private, args)?;
            let fresh = ordered(&first)
                .iter()
                .filter(|(k, v)| {
                    matches!(&**v, Raw::Bound(_))
                        || *k == units("topics")
                        || *k == units("messageHandlers")
                })
                .all(|(k, v)| !Rc::ptr_eq(v, &get(&second, &String::from_utf16(k).unwrap())));
            set(&identity, "fresh", boolean(fresh));
            if !private {
                for (topic, method, data) in [
                    ("ping", "handlePingEvent", false),
                    ("pong", "handlePongEvent", false),
                    ("tickers", "handleTickersStreamEvents", true),
                    ("bbos", "handleBBOStreamEvents", true),
                    ("markprices", "handleMarkPricesStreamEvents", true),
                    ("estfundingrate", "handleFundingRateStreamEvents", true),
                ] {
                    let clone = h.clone();
                    let original = options.clone();
                    let captured = first.clone();
                    let v = Rc::new(Raw::Method(Rc::new(move |receiver, args| {
                        clone.effect(
                            "dispatch",
                            vec![
                                ("method", text(method)),
                                (
                                    "receiverIdentity",
                                    boolean(
                                        receiver.as_ref().is_some_and(|v| Rc::ptr_eq(v, &captured)),
                                    ),
                                ),
                                ("argCount", clone.number(args.len() as f64)),
                                (
                                    "dataIdentity",
                                    boolean(!data || Rc::ptr_eq(&args[0], &original)),
                                ),
                            ],
                        )?;
                        Ok(clone.undefined())
                    })));
                    set(&first, method, v);
                    let f = items(&get(&get(&first, "messageHandlers"), topic))[0].clone();
                    h.call(
                        f,
                        Some(second.clone()),
                        vec![object(vec![("data", options.clone())])],
                    )?;
                }
            }
        }
        return Ok((out, identity));
    }
    let r = receiver(h);
    let book = g == "book";
    let topic = format!(
        "{}{}",
        String::from_utf16(&string(&*h.reverse_symbol(get(c, "symbol"))?)).unwrap(),
        if book {
            "@orderbookupdate".to_string()
        } else {
            format!(
                "@kline_{}",
                String::from_utf16(&string(&get(c, "interval"))).unwrap()
            )
        }
    );
    let listen = || {
        if book {
            feeds::listen_book(
                h,
                r.clone(),
                get(c, "symbol"),
                Rc::new(Raw::Function("callback")),
            )
        } else {
            feeds::listen_candles(
                h,
                r.clone(),
                object(vec![
                    ("symbol", get(c, "symbol")),
                    ("interval", get(c, "interval")),
                ]),
                Rc::new(Raw::Function("callback")),
            )
        }
    };
    let cleanup = listen()?;
    let second = if flag(c, "second") {
        Some(listen()?)
    } else {
        None
    };
    if !flag(c, "connected") {
        h.call(cleanup, None, vec![])?;
        set(&r, "isConnected", boolean(true));
        let ts = h.timers.borrow().clone();
        for (f, clear) in ts {
            if !clear {
                h.call(f, None, vec![])?;
            }
        }
    } else if !book {
        let candle = get(c, "candle");
        let candle = if h.nullish(&candle) {
            object(vec![
                ("startTime", h.number(2500.0)),
                ("open", h.number(1.0)),
                ("high", h.number(3.0)),
                ("low", h.number(0.0)),
                ("close", h.number(2.0)),
                ("volume", h.number(4.0)),
            ])
        } else {
            candle
        };
        for f in items(&get(&get(&r, "messageHandlers"), &topic)) {
            h.call(f, None, vec![object(vec![("data", candle.clone())])])?;
        }
        h.call(cleanup.clone(), None, vec![])?;
        if let Some(second) = second {
            h.call(second, None, vec![])?;
        }
        h.call(cleanup, None, vec![])?;
    } else {
        let f = items(&get(&get(&r, "messageHandlers"), &topic))[0].clone();
        let updates = items(&get(c, "updates"));
        for u in updates.iter().take(3) {
            h.call(f.clone(), None, vec![u.clone()])?;
        }
        if flag(c, "cleanupPending") {
            h.call(cleanup.clone(), None, vec![])?;
        }
        if flag(c, "disposePending") {
            set(&r, "isDisposed", boolean(true));
        }
        let response = || object(vec![("data", object(vec![("data", get(c, "snapshot"))]))]);
        if !h.pending.borrow().is_empty() {
            if flag(c, "reject") {
                h.pending.borrow_mut()[0] = Some(Err(h.original.clone()));
                drain(h);
                h.call(f.clone(), None, vec![updates[2].clone()])?;
                if h.pending.borrow().len() > 1 {
                    h.pending.borrow_mut()[1] = Some(Ok(response()));
                }
            } else {
                h.pending.borrow_mut()[0] = Some(Ok(response()));
            }
            drain(h);
        }
        if !flag(c, "cleanupPending") && !flag(c, "disposePending") {
            h.call(f, None, vec![updates[3].clone()])?;
            h.call(cleanup, None, vec![])?;
            if let Some(second) = second {
                h.call(second, None, vec![])?;
            }
        }
    }
    drain(h);
    Ok((
        object(vec![
            ("handlers", get(&r, "messageHandlers")),
            ("topics", get(&r, "topics")),
            (
                "book",
                h.last_book
                    .borrow()
                    .clone()
                    .unwrap_or_else(|| h.undefined()),
            ),
        ]),
        h.undefined(),
    ))
}
fn expected() -> &'static Value {
    static O: OnceLock<Value> = OnceLock::new();
    O.get_or_init(|| {
        let out = Command::new("node")
            .arg(format!(
                "{}/tests/api-book-sockets-oracle.cjs",
                env!("CARGO_MANIFEST_DIR")
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "source oracle {}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    })
}
fn field<'a>(o: &'a Value, key: &str) -> &'a Value {
    o["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e[0] == key)
        .map(|e| &e[1])
        .unwrap()
}
fn check(g: &str, floor: usize) {
    let rows = expected()["rows"][g].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent named case floor {g}");
    for row in rows {
        let h = setup(&row["cfg"], g);
        let id = field(&row["cfg"], "id").as_str().unwrap();
        match run(&h, g) {
            Ok((v, identity)) => {
                assert_eq!(
                    row["error"]["undefined"], true,
                    "unexpected source error {g}/{id}"
                );
                native(&v, &row["result"], &format!("{g}/{id} result"));
                native(&identity, &row["identity"], &format!("{g}/{id} identity"));
            }
            Err(e) => {
                let err = &row["error"];
                assert_eq!(
                    field(err, "identity"),
                    &Value::Bool(Rc::ptr_eq(&e, &h.original)),
                    "native original error {g}/{id}"
                );
                native(
                    &get(&e, "message"),
                    field(err, "message"),
                    "native error message",
                );
            }
        }
        for e in std::mem::take(&mut *h.detached.borrow_mut()) {
            h.effect(
                "detached",
                vec![
                    ("identity", boolean(Rc::ptr_eq(&e, &h.original))),
                    ("message", get(&e, "message")),
                ],
            )
            .unwrap();
        }
        native(
            &array(h.events.borrow().clone()),
            &row["events"],
            &format!("{g}/{id} effects"),
        );
        native(
            &get(&h.cfg, "config"),
            &row["configAfter"],
            "native original config",
        );
        native(
            &get(&h.cfg, "options"),
            &row["optionsAfter"],
            "native live options",
        );
    }
}
#[test]
fn sign_native() {
    check("sign", 9)
}
#[test]
fn api_native() {
    check("api", 8)
}
#[test]
fn candles_native() {
    check("candles", 6)
}
#[test]
fn book_native() {
    check("book", 9)
}
#[test]
fn process_native() {
    check("process", 3)
}
#[test]
fn public_native() {
    check("public", 3)
}
#[test]
fn private_native() {
    check("private", 3)
}
