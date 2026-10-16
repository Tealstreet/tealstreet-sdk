use safe_cex_venue_bybit::{
    hyro_private_session::{SessionFuture, SessionHost},
    inverse_socket_bodies as api,
};
use std::{
    cell::{Cell, RefCell},
    fmt,
    rc::Rc,
};
pub type V = Rc<Node>;
pub type R = Result<V, V>;
type Function = dyn Fn(&Realm, V, Vec<V>) -> R;
#[derive(Clone)]
enum Slot {
    Data(V),
    Getter(Rc<dyn Fn(&Realm) -> R>),
}
enum Kind {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<V>>),
    Object,
    Function(Rc<Function>),
}
pub struct Node {
    kind: Kind,
    fields: RefCell<Vec<(Vec<u16>, Slot)>>,
    prototype: RefCell<Option<V>>,
}
impl fmt::Debug for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Kind::Undefined => write!(f, "undefined"),
            Kind::Null => write!(f, "null"),
            Kind::Bool(v) => v.fmt(f),
            Kind::Number(v) => v.fmt(f),
            Kind::Text(v) => v.fmt(f),
            Kind::Array(v) => v.borrow().fmt(f),
            Kind::Object => write!(f, "object@{:p}", self),
            Kind::Function(_) => write!(f, "function@{:p}", self),
        }
    }
}
fn node(kind: Kind) -> V {
    Rc::new(Node {
        kind,
        fields: RefCell::new(vec![]),
        prototype: RefCell::new(None),
    })
}
pub fn undefined() -> V {
    node(Kind::Undefined)
}
pub fn null() -> V {
    node(Kind::Null)
}
pub fn b(v: bool) -> V {
    node(Kind::Bool(v))
}
pub fn n(v: f64) -> V {
    node(Kind::Number(v))
}
pub fn t(v: &str) -> V {
    text(v.encode_utf16().collect())
}
pub fn text(v: Vec<u16>) -> V {
    node(Kind::Text(v))
}
pub fn arr(v: Vec<V>) -> V {
    node(Kind::Array(RefCell::new(v)))
}
pub fn obj(v: Vec<(&str, V)>) -> V {
    let out = node(Kind::Object);
    for (k, v) in v {
        put(&out, k, v);
    }
    out
}
pub fn fun(f: impl Fn(&Realm, V, Vec<V>) -> R + 'static) -> V {
    node(Kind::Function(Rc::new(f)))
}
pub fn put(owner: &V, key: &str, value: V) {
    put_key(owner, key.encode_utf16().collect(), value)
}
pub fn put_key(owner: &V, key: Vec<u16>, value: V) {
    let mut fields = owner.fields.borrow_mut();
    if let Some((_, slot)) = fields.iter_mut().find(|(k, _)| *k == key) {
        *slot = Slot::Data(value);
    } else {
        fields.push((key, Slot::Data(value)));
    }
}
pub fn getter(owner: &V, key: &str, f: impl Fn(&Realm) -> R + 'static) {
    owner
        .fields
        .borrow_mut()
        .push((key.encode_utf16().collect(), Slot::Getter(Rc::new(f))));
}
pub fn prototype(owner: &V, value: V) {
    *owner.prototype.borrow_mut() = Some(value);
}
pub fn push(owner: &V, value: V) {
    let Kind::Array(rows) = &owner.kind else {
        panic!("native array")
    };
    rows.borrow_mut().push(value);
}
pub fn rows(owner: &V) -> Vec<V> {
    let Kind::Array(rows) = &owner.kind else {
        panic!("native array")
    };
    rows.borrow().clone()
}
pub fn num(value: &V) -> f64 {
    let Kind::Number(v) = value.kind else {
        panic!("native Number: {value:?}")
    };
    v
}
pub fn units(value: &V) -> Vec<u16> {
    let Kind::Text(v) = &value.kind else {
        panic!("native UTF16: {value:?}")
    };
    v.clone()
}
pub fn label(value: &V) -> String {
    String::from_utf16(&units(value)).unwrap()
}
pub fn is_undefined(value: &V) -> bool {
    matches!(value.kind, Kind::Undefined)
}
fn same(a: &V, b: &V) -> bool {
    match (&a.kind, &b.kind) {
        (Kind::Undefined, Kind::Undefined) | (Kind::Null, Kind::Null) => true,
        (Kind::Bool(a), Kind::Bool(b)) => a == b,
        (Kind::Number(a), Kind::Number(b)) => a == b,
        (Kind::Text(a), Kind::Text(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
#[derive(Clone)]
pub struct Realm {
    global: V,
    pub trace: Rc<RefCell<Vec<String>>>,
    pub parsed: Rc<RefCell<V>>,
}
impl Default for Realm {
    fn default() -> Self {
        Self::new()
    }
}
impl Realm {
    pub fn new() -> Self {
        let h = Self {
            global: obj(vec![]),
            trace: Rc::new(RefCell::new(vec![])),
            parsed: Rc::new(RefCell::new(undefined())),
        };
        put(
            &h.global,
            "parseFloat",
            fun(|h, this, a| {
                assert!(is_undefined(&this), "bare parseFloat receiver");
                Ok(n(safe_cex_num::coercion::parse_float(
                    &String::from_utf16_lossy(&h.units(&a[0])?),
                )))
            }),
        );
        put(
            &h.global,
            "v",
            fun(|h, this, a| {
                assert!(is_undefined(&this), "bare v receiver");
                h.get_key(&a[0], h.units(&a[1])?)
            }),
        );
        put(
            &h.global,
            "jsonParse",
            fun(|h, this, _| {
                assert!(is_undefined(&this), "bare parser receiver");
                h.log("parse");
                Ok(h.parsed.borrow().clone())
            }),
        );
        put(
            &h.global,
            "Number",
            obj(vec![(
                "isFinite",
                fun(|_, _, a| Ok(b(matches!(a[0].kind,Kind::Number(v) if v.is_finite())))),
            )]),
        );
        put(
            &h.global,
            "OrderStatus",
            obj(vec![("Closed", t("closed")), ("Canceled", t("canceled"))]),
        );
        put(
            &h.global,
            "virtualClock",
            obj(vec![(
                "getCurrentTimeMs",
                fun(|h, _, _| {
                    h.log("clock");
                    Ok(n(1234.0))
                }),
            )]),
        );
        put(
            &h.global,
            "resolveBybitExecution",
            fun(|h, this, a| {
                assert!(is_undefined(&this));
                h.log("execution");
                h.get(&a[0], "execution")
            }),
        );
        put(&h.global, "createdFunctions", arr(vec![]));
        put(
            &h.global,
            "cleanFloat",
            fun(|_, this, a| {
                assert!(is_undefined(&this));
                Ok(n(safe_cex_store::incremental_fills::clean_float(
                    num(&a[0]),
                )))
            }),
        );
        let next = Rc::new(Cell::new(0));
        put(
            &h.global,
            "createIncrementalFills",
            fun(move |h, this, _| {
                assert!(is_undefined(&this));
                next.set(next.get() + 1);
                let factory = RefCell::new(
                    safe_cex_store::incremental_fills::create_incremental_fills(500.0),
                );
                let out = fun(move |_, _, a| {
                    let value = if is_undefined(&a[2]) {
                        None
                    } else {
                        Some(num(&a[2]))
                    };
                    let result = factory.borrow_mut()(&label(&a[0]), num(&a[1]), value);
                    Ok(obj(vec![
                        ("qty", n(result.qty)),
                        ("value", result.value.map_or_else(undefined, n)),
                    ]))
                });
                put(&out, "tag", n(f64::from(next.get())));
                push(&h.field(&h.global, "createdFunctions"), out.clone());
                Ok(out)
            }),
        );
        h
    }
    pub fn log(&self, value: &str) {
        self.trace.borrow_mut().push(value.to_owned());
    }
    pub fn global_value(&self) -> V {
        self.global.clone()
    }
    pub fn get_key(&self, owner: &V, key: Vec<u16>) -> R {
        if matches!(owner.kind, Kind::Undefined | Kind::Null) {
            return Err(t("nullish property"));
        }
        let slot = owner
            .fields
            .borrow()
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v.clone());
        if let Some(slot) = slot {
            return match slot {
                Slot::Data(v) => Ok(v),
                Slot::Getter(f) => f(self),
            };
        }
        if let Some(proto) = owner.prototype.borrow().clone() {
            let out = self.get_key(&proto, key.clone())?;
            if !is_undefined(&out) {
                return Ok(out);
            }
        }
        let ascii = String::from_utf16(&key).ok();
        if let Some(key) = ascii.as_deref() {
            match &owner.kind {
                Kind::Array(v) if key == "length" => return Ok(n(v.borrow().len() as f64)),
                Kind::Array(v) if key.parse::<usize>().is_ok() => {
                    return Ok(v
                        .borrow()
                        .get(key.parse::<usize>().unwrap())
                        .cloned()
                        .unwrap_or_else(undefined));
                }
                Kind::Text(v) if key == "length" => return Ok(n(v.len() as f64)),
                _ => {}
            }
            let array = matches!(owner.kind, Kind::Array(_))
                && ["forEach", "map", "push", "join"].contains(&key);
            let string = matches!(owner.kind, Kind::Text(_))
                && ["indexOf", "slice", "includes"].contains(&key);
            if array || string {
                let code = key.to_owned();
                return Ok(fun(move |h, this, a| h.builtin(&code, this, a)));
            }
        }
        Ok(undefined())
    }
    fn builtin(&self, code: &str, this: V, a: Vec<V>) -> R {
        match code {
            "push" => {
                for value in a {
                    push(&this, value);
                }
                Ok(n(rows(&this).len() as f64))
            }
            "forEach" | "map" => {
                let len = rows(&this).len();
                let mut out = vec![];
                for i in 0..len {
                    let value = rows(&this).get(i).cloned();
                    if let Some(value) = value {
                        out.push(self.receiver_call(
                            undefined(),
                            a[0].clone(),
                            vec![value, n(i as f64), this.clone()],
                        )?);
                    }
                }
                Ok(if code == "map" { arr(out) } else { undefined() })
            }
            "join" => {
                let separator = self.units(&a[0])?;
                let mut out = vec![];
                for (i, v) in rows(&this).iter().enumerate() {
                    if i > 0 {
                        out.extend(&separator)
                    }
                    if !matches!(v.kind, Kind::Undefined | Kind::Null) {
                        out.extend(self.units(v)?);
                    }
                }
                Ok(text(out))
            }
            "indexOf" | "includes" => {
                let hay = self.units(&this)?;
                let needle = self.units(&a[0])?;
                let start = if a.len() > 1 {
                    self.numeric(&a[1])?.max(0.0) as usize
                } else {
                    0
                };
                let found = if needle.is_empty() {
                    Some(start.min(hay.len()))
                } else if start > hay.len() {
                    None
                } else {
                    hay[start..]
                        .windows(needle.len())
                        .position(|v| v == needle)
                        .map(|i| i + start)
                };
                Ok(if code == "includes" {
                    b(found.is_some())
                } else {
                    n(found.map_or(-1.0, |i| i as f64))
                })
            }
            "slice" => {
                let v = self.units(&this)?;
                let index = |x: f64| -> usize {
                    if x < 0.0 {
                        (v.len() as f64 + x).max(0.0) as usize
                    } else {
                        x.min(v.len() as f64) as usize
                    }
                };
                let start = index(self.numeric(&a[0])?);
                let end = if a.len() > 1 {
                    index(self.numeric(&a[1])?)
                } else {
                    v.len()
                };
                Ok(text(v[start..end.max(start)].to_vec()))
            }
            _ => panic!("unimplemented builtin {code}"),
        }
    }
    pub fn field(&self, owner: &V, key: &str) -> V {
        self.get(owner, key).unwrap()
    }
}
impl SessionHost for Realm {
    type Value = V;
    type Error = V;
    fn get(&self, o: &V, k: &str) -> R {
        self.get_key(o, k.encode_utf16().collect())
    }
    fn optional(&self, o: &V, k: &str) -> R {
        if matches!(o.kind, Kind::Undefined | Kind::Null) {
            Ok(undefined())
        } else {
            self.get(o, k)
        }
    }
    fn set(&self, o: &V, k: &str, v: V) -> Result<(), V> {
        put(o, k, v);
        Ok(())
    }
    fn define(&self, o: &V, k: &str, v: V) -> Result<(), V> {
        self.set(o, k, v)
    }
    fn truthy(&self, v: &V) -> bool {
        match &v.kind {
            Kind::Undefined | Kind::Null => false,
            Kind::Bool(v) => *v,
            Kind::Number(v) => *v != 0.0 && !v.is_nan(),
            Kind::Text(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        same(a, b)
    }
    fn undefined(&self) -> V {
        undefined()
    }
    fn literal(&self, v: &str) -> V {
        t(v)
    }
    fn boolean(&self, v: bool) -> V {
        b(v)
    }
    fn number(&self, v: f64) -> V {
        n(v)
    }
    fn numeric(&self, v: &V) -> Result<f64, V> {
        Ok(match &v.kind {
            Kind::Number(v) => *v,
            Kind::Undefined => f64::NAN,
            Kind::Null => 0.0,
            Kind::Bool(v) => {
                if *v {
                    1.0
                } else {
                    0.0
                }
            }
            Kind::Text(v) => String::from_utf16(v).unwrap().parse().unwrap_or(f64::NAN),
            _ => return Err(t("numeric coercion outside witness")),
        })
    }
    fn units(&self, v: &V) -> Result<Vec<u16>, V> {
        Ok(match &v.kind {
            Kind::Text(v) => v.clone(),
            Kind::Number(v) => safe_cex_num::js_number_string(*v).encode_utf16().collect(),
            Kind::Undefined => "undefined".encode_utf16().collect(),
            Kind::Null => "null".encode_utf16().collect(),
            Kind::Bool(v) => if *v { "true" } else { "false" }.encode_utf16().collect(),
            _ => return Err(t("string coercion outside witness")),
        })
    }
    fn text(&self, v: Vec<u16>) -> V {
        text(v)
    }
    fn object(&self, v: Vec<(&'static str, V)>) -> V {
        obj(v)
    }
    fn array(&self, v: Vec<V>) -> V {
        arr(v)
    }
    fn global(&self) -> V {
        self.global.clone()
    }
    fn select(&self, o: V, k: &str) -> R {
        let f = self.get(&o, k)?;
        Ok(fun(move |h, _, a| h.receiver_call(o.clone(), f.clone(), a)))
    }
    fn receiver_call(&self, o: V, f: V, a: Vec<V>) -> R {
        let Kind::Function(f) = &f.kind else {
            return Err(t("not callable"));
        };
        f(self, o, a)
    }
    fn apply(&self, f: V, a: Vec<V>) -> R {
        self.receiver_call(undefined(), f, a)
    }
    fn wait(&self, v: V) -> SessionFuture<'_, V, V> {
        Box::pin(async move { Ok(v) })
    }
    fn intrinsic(&self, name: &str, a: Vec<V>) -> R {
        match name {
            "null" => Ok(null()),
            "increment" => Ok(n(self.numeric(&a[0])? + 1.0)),
            "require iterator result object" => {
                if matches!(a[0].kind, Kind::Array(_) | Kind::Object | Kind::Function(_)) {
                    Ok(a[0].clone())
                } else {
                    Err(t("iterator result must be object"))
                }
            }
            "computed get" => self.get_key(&a[0], self.units(&a[1])?),
            "less" | "less equal" | "greater" | "greater equal" => {
                let x = self.numeric(&a[0])?;
                let y = self.numeric(&a[1])?;
                Ok(b(match name {
                    "less" => x < y,
                    "less equal" => x <= y,
                    "greater" => x > y,
                    _ => x >= y,
                }))
            }
            "addition" => {
                if matches!(a[0].kind, Kind::Text(_)) || matches!(a[1].kind, Kind::Text(_)) {
                    let mut s = self.units(&a[0])?;
                    s.extend(self.units(&a[1])?);
                    Ok(text(s))
                } else {
                    Ok(n(self.numeric(&a[0])? + self.numeric(&a[1])?))
                }
            }
            "division" => Ok(n(self.numeric(&a[0])? / self.numeric(&a[1])?)),
            "iterator" => {
                let custom = self.get(&a[0], "$iterator")?;
                if !is_undefined(&custom) {
                    let out = self.receiver_call(a[0].clone(), custom, vec![])?;
                    return if matches!(out.kind, Kind::Array(_) | Kind::Object | Kind::Function(_))
                    {
                        Ok(out)
                    } else {
                        Err(t("iterator must be object"))
                    };
                }
                let array = a[0].clone();
                let i = Cell::new(0);
                Ok(obj(vec![(
                    "next",
                    fun(move |_, _, _| {
                        let at = i.get();
                        i.set(at + 1);
                        let value = rows(&array).get(at).cloned();
                        Ok(obj(vec![
                            ("done", b(value.is_none())),
                            ("value", value.unwrap_or_else(undefined)),
                        ]))
                    }),
                )]))
            }
            "iterator close after throw" => {
                let f = self.get(&a[0], "return")?;
                if is_undefined(&f) {
                    Ok(undefined())
                } else {
                    self.receiver_call(a[0].clone(), f, vec![])
                }
            }
            _ => panic!("unimplemented language primitive {name}"),
        }
    }
    fn allocate(&self, _: V) -> R {
        panic!("no class assembly")
    }
    fn callback(&self, o: V, code: &'static str, _: Vec<V>) -> R {
        Ok(fun(move |h, _, a| api::dispatch(h, &o, code, a)))
    }
    fn promise(&self) -> Result<(V, V, V), V> {
        panic!("no async body")
    }
    fn error(&self, v: V) -> Result<V, V> {
        Ok(v)
    }
    fn error_value(&self, e: V) -> V {
        e
    }
    fn normalize_error(&self, e: V) -> R {
        Ok(e)
    }
}
