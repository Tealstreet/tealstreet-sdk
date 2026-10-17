use safe_cex_venue_orderly::{
    account_order_reads::AccountReadHost,
    classes::{self, BoundMember, ClassHost, Initializer, Kind, NativeMember, Return},
    data_mappers::{self as data, DataHost, Op},
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
    Map(RefCell<Vec<(V, V)>>),
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
        Raw::Function(_) | Raw::Bound(_) | Raw::Map(_) => panic!("outside class coercion floor"),
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
        Raw::Function(name) => assert_eq!(expected["function"], *name, "native function name"),
        Raw::Bound(f) => assert_eq!(expected["function"], f.name, "native bound function name"),
        Raw::Map(a) => assert_eq!(
            expected["map"].as_array().unwrap().len(),
            a.borrow().len(),
            "native map size"
        ),
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
        Raw::Map(a) => Raw::Map(RefCell::new(a.borrow().clone())),
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
    count: Cell<usize>,
    current: RefCell<Option<V>>,
    normalized: RefCell<Option<V>>,
    every: V,
    periodic: V,
    prototypes: RefCell<Vec<(&'static str, NativeMember<Host>)>>,
    flags: RefCell<Vec<(usize, Vec<u16>, (bool, bool, bool))>>,
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
    fn initialized(&self, r: &V) -> bool {
        classes::definitions()
            .iter()
            .filter(|d| !matches!(d.initializer, Initializer::Prototype(_)))
            .all(|d| ordered(r).iter().any(|(k, _)| *k == units(d.name)))
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
        Ok(get(v, &String::from_utf16(k).unwrap()))
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
    fn text(&self, v: &[u16]) -> V {
        Rc::new(Raw::Text(v.to_vec()))
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> Result<V, E> {
        Ok(object(v))
    }
    fn stringify(&self, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
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
            Raw::Function("listenOHLCV") => {
                let receiver = receiver.expect("original socket receiver");
                self.effect(
                    "delegate",
                    vec![
                        ("baseId", get(&receiver, "baseId")),
                        (
                            "optionsIdentity",
                            boolean(self.same_value(&args[0], &get(&self.cfg, "argument"))),
                        ),
                        (
                            "callbackIdentity",
                            boolean(self.same_value(&args[1], &get(&self.cfg, "callback"))),
                        ),
                    ],
                )?;
                if flag(&self.cfg, "invokeFailure") {
                    return Err(self.original.clone());
                }
                Ok(get(&receiver, "cleanup"))
            }
            _ => panic!("outside finite native member binding"),
        }
    }
    fn monotonic_now(&self) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
    fn virtual_now(&self) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
    fn sign(&self, _: V, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
    }
    fn reconnecting(&self) -> bool {
        false
    }
}
#[allow(clippy::manual_async_fn)]
impl AccountReadHost for Host {
    fn await_value(&self, _: V) -> impl Future<Output = Result<V, E>> {
        async { panic!("cached prototype floor has no request") }
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
    fn reverse_symbol(&self, _: V) -> Result<V, E> {
        panic!("outside class invocation floor")
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
impl ClassHost for Host {
    fn spread_object(&self, v: V) -> Result<V, E> {
        Ok(Rc::new(Raw::Object(RefCell::new(if self.nullish(&v) {
            vec![]
        } else {
            ordered(&v)
        }))))
    }
    fn starts_with(&self, v: V, p: &str) -> Result<bool, E> {
        DataHost::starts_with(self, v, p)
    }
    fn super_exchange(&self, kind: Kind, name: V, options: V, store: V) -> Result<V, E> {
        let extra = get(&options, "extra");
        self.effect(
            "super",
            vec![
                ("newTarget", text(kind.name())),
                (
                    "nameIdentity",
                    boolean(self.same_value(&name, &get(&self.cfg, "name"))),
                ),
                (
                    "optionsIdentity",
                    boolean(Rc::ptr_eq(&options, &get(&self.cfg, "options"))),
                ),
                (
                    "storeIdentity",
                    boolean(self.same_value(&store, &get(&self.cfg, "store"))),
                ),
                (
                    "nestedIdentity",
                    boolean(self.same_value(&extra, &get(&get(&self.cfg, "options"), "extra"))),
                ),
                ("options", options.clone()),
            ],
        )?;
        let id = self.count.get() + 1;
        self.count.set(id);
        let r = object(vec![
            ("baseId", self.number(id as f64)),
            ("exchangeName", name),
            ("options", options.clone()),
            ("store", store),
            ("isDisposed", boolean(false)),
            ("newTarget", text(kind.name())),
        ]);
        for (k, v) in ordered(&r) {
            self.own_define(&r, &String::from_utf16(&k).unwrap(), v)
        }
        *self.current.borrow_mut() = Some(r.clone());
        *self.normalized.borrow_mut() = Some(options);
        Ok(r)
    }
    fn define(&self, r: V, name: &str, v: V) -> Result<(), E> {
        self.own_define(&r, name, v);
        Ok(())
    }
    fn fresh_map(&self) -> Result<V, E> {
        Ok(Rc::new(Raw::Map(RefCell::new(vec![]))))
    }
    fn client_rule(&self, name: &str, pattern: &str) -> Result<V, E> {
        Ok(object(vec![
            ("label", text(name)),
            ("requirement", text(&format!("match /{pattern}/"))),
            ("isValid", Rc::new(Raw::Function("isValid"))),
        ]))
    }
    fn imported(&self, name: &'static str) -> V {
        match name {
            "GRAFANA_ENDPOINTS_EVERY" => self.every.clone(),
            "GRAFANA_ENDPOINTS_PERIODIC" => self.periodic.clone(),
            _ => panic!("unknown source import"),
        }
    }
    fn member(&self, name: &'static str, target: &'static str) -> NativeMember<Self> {
        let h = self.clone();
        Rc::new(move |r, args| {
            assert!(
                target.starts_with("safe-cex-venue-orderly/src/"),
                "bound native target"
            );
            match name {
                "listenOHLCV" => Ok(Return::Value(data::listen_candles(
                    &h,
                    r,
                    args.first().cloned().unwrap_or_else(|| h.undefined()),
                    args.get(1).cloned().unwrap_or_else(|| h.undefined()),
                )?)),
                "getPositionMode" => {
                    let h = h.clone();
                    Ok(Return::Future(Box::pin(async move {
                        safe_cex_venue_orderly::account_order_reads::position_mode(&h, r).await
                    })))
                }
                _ => panic!("member body outside finite composition invocation floor: {name}"),
            }
        })
    }
    fn arrow(&self, name: &'static str, body: BoundMember<Self>) -> V {
        Rc::new(Raw::Bound(Rc::new(Callable {
            name: name.into(),
            body,
        })))
    }
    fn prototype(&self, name: &'static str, body: NativeMember<Self>) {
        let mut p = self.prototypes.borrow_mut();
        if !p.iter().any(|(n, _)| *n == name) {
            p.push((name, body))
        }
    }
    fn create_api(&self, options: V, r: V) -> Result<V, E> {
        self.effect(
            "api",
            vec![
                (
                    "optionsIdentity",
                    boolean(Rc::ptr_eq(
                        &options,
                        self.normalized.borrow().as_ref().unwrap(),
                    )),
                ),
                (
                    "receiverIdentity",
                    boolean(Rc::ptr_eq(&r, self.current.borrow().as_ref().unwrap())),
                ),
                ("initialized", boolean(self.initialized(&r))),
                (
                    "xhrUndefined",
                    boolean(matches!(&*get(&r, "xhr"), Raw::Undefined)),
                ),
                ("tag", get(&r, "orderlyExchangeName")),
            ],
        )?;
        Ok(object(vec![("client", get(&r, "baseId"))]))
    }
    fn rate_limit(&self, client: V, max_rps: f64) -> Result<V, E> {
        self.effect(
            "rate",
            vec![("client", client.clone()), ("maxRPS", self.number(max_rps))],
        )?;
        Ok(object(vec![("wrapped", boolean(true)), ("client", client)]))
    }
    fn construct_socket(&self, private: bool, parent: V) -> Result<V, E> {
        let label = if private { "private" } else { "public" };
        self.effect(
            label,
            vec![
                (
                    "parentIdentity",
                    boolean(Rc::ptr_eq(&parent, self.current.borrow().as_ref().unwrap())),
                ),
                ("tag", get(&parent, "orderlyExchangeName")),
                ("xhrPresent", get(&get(&parent, "xhr"), "wrapped")),
                (
                    "publicPresent",
                    boolean(self.truthy(&get(&parent, "publicWebsocket"))),
                ),
                ("initialized", boolean(self.initialized(&parent))),
            ],
        )?;
        let id = get(&parent, "baseId");
        Ok(object(vec![
            ("label", text(label)),
            ("baseId", id.clone()),
            (
                "cleanup",
                object(vec![("cleanup", text(label)), ("baseId", id)]),
            ),
            ("listenOHLCV", Rc::new(Raw::Function("listenOHLCV"))),
        ]))
    }
}
fn setup(cfg: &Value) -> Host {
    let cfg = decode(cfg);
    let every = array(
        safe_cex_venue_orderly::types::GRAFANA_ENDPOINTS_EVERY
            .iter()
            .map(|s| text(s))
            .collect(),
    );
    let periodic = array(
        safe_cex_venue_orderly::types::GRAFANA_ENDPOINTS_PERIODIC
            .iter()
            .map(|s| text(s))
            .collect(),
    );
    Host(Rc::new(State {
        cfg,
        events: RefCell::new(vec![]),
        original: object(vec![
            ("name", text("Error")),
            ("message", text("original constructor error")),
        ]),
        count: Cell::new(0),
        current: RefCell::new(None),
        normalized: RefCell::new(None),
        every,
        periodic,
        prototypes: RefCell::new(vec![]),
        flags: RefCell::new(vec![]),
    }))
}
fn settle(ret: Return<Host>) -> Result<V, E> {
    match ret {
        Return::Value(v) => Ok(v),
        Return::Future(mut f) => {
            for _ in 0..20 {
                if let Poll::Ready(r) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    return r;
                }
            }
            panic!("finite composition continuation floor")
        }
    }
}
fn construct(h: &Host, kind: Kind, ctor: bool) -> Result<V, E> {
    let args = if !ctor && flag(&h.cfg, "noArgs") {
        vec![]
    } else {
        vec![
            get(&h.cfg, "name"),
            get(&h.cfg, "options"),
            get(&h.cfg, "store"),
            object(vec![("ignored", boolean(true))]),
        ]
    };
    classes::construct(h, kind, args)
}
fn shape(h: &Host, r: &V, e: &Value) {
    let entries = ordered(r);
    let expected = e.as_array().unwrap();
    assert_eq!(entries.len(), 54, "independent native own field floor");
    assert_eq!(entries.len(), expected.len(), "native own count");
    for ((key, v), e) in entries.iter().zip(expected) {
        assert_eq!(*key, units(e["key"].as_str().unwrap()), "native own order");
        let (enumerable, configurable, writable) = h.flags(r, key);
        assert_eq!(e["enumerable"], enumerable, "native descriptor enumerable");
        assert_eq!(
            e["configurable"], configurable,
            "native descriptor configurable"
        );
        assert_eq!(e["writable"], writable, "native descriptor writable");
        match &**v {
            Raw::Bound(f) => {
                assert_eq!(e["kind"], "function");
                assert_eq!(e["value"], f.name, "native arrow name")
            }
            Raw::Map(rows) => {
                assert_eq!(e["kind"], "map");
                assert_eq!(rows.borrow().len(), 0, "native fresh map");
                assert_eq!(e["value"], 0)
            }
            v => {
                assert_eq!(e["kind"], "value");
                native(v, &e["value"], "native class field")
            }
        }
    }
}
fn oracle() -> &'static Value {
    static O: OnceLock<Value> = OnceLock::new();
    O.get_or_init(|| {
        let r = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/classes-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            r.status.success(),
            "source classes {}",
            String::from_utf8_lossy(&r.stderr)
        );
        let v: Value = serde_json::from_slice(&r.stdout).unwrap();
        assert_eq!(
            v["hashes"].as_object().unwrap().len(),
            9,
            "independent class source file floor"
        );
        assert_eq!(
            v["fields"].as_array().unwrap().len(),
            48,
            "source own field floor"
        );
        assert_eq!(
            v["arrows"].as_array().unwrap().len(),
            38,
            "source arrow floor"
        );
        v
    })
}
fn check(kind: Kind, ctor: bool, floor: usize) {
    assert_eq!(
        classes::definitions().len(),
        49,
        "independent native definition floor"
    );
    assert_eq!(
        classes::definitions()
            .iter()
            .filter(|d| matches!(d.initializer, Initializer::Arrow(_)))
            .count(),
        38,
        "independent native arrow floor"
    );
    let name = if ctor { "constructor" } else { kind.name() };
    let rows = oracle()["rows"][name].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent class case floor");
    for row in rows {
        let h = setup(&row["cfg"]);
        let result = construct(&h, kind, ctor);
        let mut error = None;
        match result {
            Err(e) => error = Some(e),
            Ok(first) => {
                shape(&h, &first, &row["snapshot"]);
                if flag(&h.cfg, "pair") {
                    let proto = h.prototypes.borrow()[0].1.clone();
                    let second = construct(&h, kind, ctor).unwrap();
                    assert_eq!(
                        h.prototypes.borrow().len(),
                        1,
                        "native prototype installation once"
                    );
                    assert!(
                        Rc::ptr_eq(&proto, &h.prototypes.borrow()[0].1),
                        "native persistent prototype body"
                    );
                    assert_eq!(row["pair"]["prototypePersistent"], true);
                    for e in row["pair"]["functions"].as_array().unwrap() {
                        let k = e["key"].as_str().unwrap();
                        assert!(
                            !h.same_value(&get(&first, k), &get(&second, k)),
                            "native fresh arrow"
                        );
                        assert_eq!(e["fresh"], true)
                    }
                    for e in row["pair"]["containers"].as_array().unwrap() {
                        let k = e["key"].as_str().unwrap();
                        assert_eq!(
                            h.same_value(&get(&first, k), &get(&second, k)),
                            e["shared"].as_bool().unwrap(),
                            "native graph sharing {k}"
                        )
                    }
                    assert!(
                        !ordered(&first)
                            .iter()
                            .any(|(k, _)| *k == units("getPositionMode")),
                        "native prototype member not own"
                    );
                    assert_eq!(row["pair"]["prototypeOwn"], false);
                    if flag(&h.cfg, "invoke") {
                        let f = get(&first, "listenOHLCV");
                        match h.call(
                            f,
                            Some(second),
                            vec![get(&h.cfg, "argument"), get(&h.cfg, "callback")],
                        ) {
                            Err(e) => error = Some(e),
                            Ok(v) => {
                                let foreign = object(vec![(
                                    "_accountPositionMode",
                                    object(vec![("opaque", text("foreign mode"))]),
                                )]);
                                let mode = settle(
                                    (h.prototypes.borrow()[0].1)(foreign.clone(), vec![]).unwrap(),
                                )
                                .unwrap();
                                native(
                                    &object(vec![
                                        (
                                            "cleanupIdentity",
                                            boolean(Rc::ptr_eq(
                                                &v,
                                                &get(&get(&first, "publicWebsocket"), "cleanup"),
                                            )),
                                        ),
                                        (
                                            "foreignModeIdentity",
                                            boolean(Rc::ptr_eq(
                                                &mode,
                                                &get(&foreign, "_accountPositionMode"),
                                            )),
                                        ),
                                        ("ownGetMode", boolean(false)),
                                    ]),
                                    &row["invocation"],
                                    "native original member return identity",
                                );
                            }
                        }
                    }
                }
            }
        }
        if let Some(e) = error {
            assert_eq!(
                Rc::ptr_eq(&e, &h.original),
                row["error"]["identity"].as_bool().unwrap(),
                "native original constructor error"
            );
            native(&get(&e, "name"), &row["error"]["name"], "native error name");
            native(
                &get(&e, "message"),
                &row["error"]["message"],
                "native error message",
            );
        } else {
            assert!(row["error"].is_null(), "source constructor error")
        }
        native(
            &array(h.events.borrow().clone()),
            &row["events"],
            "native construction effects",
        );
        native(
            &get(&h.cfg, "options"),
            &row["optionsAfter"],
            "native original caller options unchanged",
        );
    }
}
#[test]
fn constructor_native() {
    check(Kind::Orderly, true, 11)
}
#[test]
fn orderly_native() {
    check(Kind::Orderly, false, 7)
}
#[test]
fn kodiakfi_native() {
    check(Kind::KodiakFI, false, 7)
}
#[test]
fn mode_native() {
    check(Kind::Mode, false, 7)
}
#[test]
fn salsadex_native() {
    check(Kind::SalsaDEX, false, 7)
}
#[test]
fn tealstreet_dex_native() {
    check(Kind::TealstreetDEX, false, 7)
}
#[test]
fn what_native() {
    check(Kind::What, false, 7)
}
#[test]
fn woofi_native() {
    check(Kind::Woofi, false, 7)
}
