use safe_cex_venue_hyperliquid::{
    cache_orders::CacheOrdersHost, cache_runtime::*, cross_cache::CrossCacheHost,
    json_boundaries::JsPayloadHost, spot_json::JsString,
};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};
#[derive(Clone, Debug)]
pub enum N {
    U,
    Null,
    B(bool),
    Num(f64),
    S(JsString),
    O(Rc<RefCell<Obj>>),
    A(Rc<RefCell<Vec<N>>>),
    Map(Rc<RefCell<Vec<(N, N)>>>),
    Set(Rc<RefCell<Vec<N>>>),
    Fn(Rc<(N, Callback)>),
    Builtin(String),
    Iter(Rc<RefCell<(Vec<N>, usize)>>),
    Class(Rc<()>),
}
#[derive(Debug, Default)]
pub struct Obj {
    pub fields: Vec<(String, N, bool)>,
    pub prototype: Option<N>,
}
pub fn s(v: &str) -> N {
    N::S(JsString(v.encode_utf16().collect()))
}
pub fn object(fields: Vec<(&str, N)>) -> N {
    N::O(Rc::new(RefCell::new(Obj {
        fields: fields
            .into_iter()
            .map(|(k, v)| (k.into(), v, true))
            .collect(),
        prototype: None,
    })))
}
pub fn arr(v: Vec<N>) -> N {
    N::A(Rc::new(RefCell::new(v)))
}
pub fn num(v: &N) -> f64 {
    match v {
        N::Num(n) => *n,
        N::Null => 0.,
        N::B(b) => {
            if *b {
                1.
            } else {
                0.
            }
        }
        N::S(s) => String::from_utf16_lossy(&s.0).parse().unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
pub fn eq(a: &N, b: &N) -> bool {
    match (a, b) {
        (N::U, N::U) | (N::Null, N::Null) => true,
        (N::Class(a), N::Class(b)) => Rc::ptr_eq(a, b),
        (N::B(a), N::B(b)) => a == b,
        (N::Num(a), N::Num(b)) => a == b,
        (N::S(a), N::S(b)) => a == b,
        (N::O(a), N::O(b)) => Rc::ptr_eq(a, b),
        (N::A(a), N::A(b)) => Rc::ptr_eq(a, b),
        (N::Map(a), N::Map(b)) => Rc::ptr_eq(a, b),
        (N::Set(a), N::Set(b)) => Rc::ptr_eq(a, b),
        (N::Fn(a), N::Fn(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn same(a: &N, b: &N) -> bool {
    eq(a, b) || matches!((a,b),(N::Num(a),N::Num(b)) if a.is_nan()&&b.is_nan())
}
fn string(v: &N) -> String {
    match v {
        N::S(s) => String::from_utf16_lossy(&s.0),
        N::U => "undefined".into(),
        N::Null => "null".into(),
        N::Num(n) => n.to_string(),
        N::B(b) => b.to_string(),
        _ => "[object Object]".into(),
    }
}
pub struct Host {
    pub time: f64,
    pub ticks: Vec<f64>,
    pub broadcast: bool,
    pub fail_broadcast: bool,
    pub fail_handler: bool,
    pub fail_map: bool,
    pub storage: N,
    pub messages: Vec<N>,
    pub events: Vec<String>,
    pub reads: usize,
}
impl Default for Host {
    fn default() -> Self {
        Self {
            time: 40000.,
            ticks: vec![],
            broadcast: true,
            fail_broadcast: false,
            fail_handler: false,
            fail_map: false,
            storage: N::U,
            messages: vec![],
            events: vec![],
            reads: 0,
        }
    }
}
impl Host {
    pub fn invoke(&mut self, o: &N, k: &str, args: &[N]) -> Result<N, String> {
        let f = self.get(o, k)?;
        self.call(&f, o, args)
    }
    pub fn put(&mut self, o: &N, k: &str, v: &N, e: bool) -> Result<(), String> {
        if self.fail_handler && k == "onmessage" {
            return Err("handler".into());
        }
        let N::O(o) = o else {
            return Err("assignment".into());
        };
        let mut o = o.borrow_mut();
        if let Some(p) = o.fields.iter_mut().find(|p| p.0 == k) {
            p.1 = v.clone();
            p.2 = e;
        } else {
            o.fields.push((k.into(), v.clone(), e));
        }
        Ok(())
    }
    pub fn decode(v: &Value) -> N {
        match v {
            Value::Null => N::Null,
            Value::Bool(b) => N::B(*b),
            Value::Number(n) => N::Num(n.as_f64().unwrap()),
            Value::String(t) => s(t),
            Value::Array(a) => arr(a.iter().map(Self::decode).collect()),
            Value::Object(o) => {
                let n = object(vec![]);
                if let N::O(p) = &n {
                    p.borrow_mut().fields = o
                        .iter()
                        .map(|(k, v)| (k.clone(), Self::decode(v), true))
                        .collect();
                }
                n
            }
        }
    }
}
impl JsPayloadHost for Host {
    type Value = N;
    type Error = String;
    fn string_units<'a>(&self, v: &'a N) -> Option<&'a [u16]> {
        if let N::S(s) = v { Some(&s.0) } else { None }
    }
    fn is_object(&self, v: &N) -> bool {
        matches!(
            v,
            N::O(_) | N::A(_) | N::Map(_) | N::Set(_) | N::Fn(_) | N::Class(_)
        )
    }
    fn is_array(&mut self, v: &N) -> Result<bool, String> {
        Ok(matches!(v, N::A(_)))
    }
    fn is_nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn undefined(&self) -> N {
        N::U
    }
    fn parse_json(&mut self, v: &[u16]) -> Result<N, String> {
        let j = serde_json::from_str::<Value>(&String::from_utf16_lossy(v))
            .map_err(|e| e.to_string())?;
        Ok(Self::decode(&j))
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, String> {
        match v {
            N::O(o) => {
                let o = o.borrow();
                if let Some(p) = o.fields.iter().find(|p| p.0 == k) {
                    return Ok(p.1.clone());
                }
                if let Some(p) = &o.prototype {
                    return self.get(p, k);
                }
                Ok(N::U)
            }
            N::A(a) => {
                let a = a.borrow();
                if k == "length" {
                    Ok(N::Num(a.len() as f64))
                } else if let Ok(i) = k.parse::<usize>() {
                    Ok(a.get(i).cloned().unwrap_or(N::U))
                } else {
                    Ok(N::Builtin(k.into()))
                }
            }
            N::Map(m) => {
                if k == "size" {
                    Ok(N::Num(m.borrow().len() as f64))
                } else {
                    Ok(N::Builtin(k.into()))
                }
            }
            N::Set(m) => {
                if k == "size" {
                    Ok(N::Num(m.borrow().len() as f64))
                } else {
                    Ok(N::Builtin(k.into()))
                }
            }
            N::Iter(_) => Ok(N::Builtin(k.into())),
            N::U | N::Null => Err("nullish".into()),
            _ => Ok(N::U),
        }
    }
    fn json_stringify(&mut self, v: &N) -> Result<Option<JsString>, String> {
        fn j(v: &N) -> Value {
            match v {
                N::Null | N::U => Value::Null,
                N::Num(n) => serde_json::json!(n),
                N::B(b) => Value::Bool(*b),
                N::S(s) => Value::String(String::from_utf16_lossy(&s.0)),
                N::A(a) => Value::Array(a.borrow().iter().map(j).collect()),
                N::O(o) => Value::Object(
                    o.borrow()
                        .fields
                        .iter()
                        .filter(|p| p.2 && !matches!(p.1, N::U))
                        .map(|p| (p.0.clone(), j(&p.1)))
                        .collect(),
                ),
                _ => panic!("not JSON fixture"),
            }
        }
        if matches!(v, N::U) {
            return Ok(None);
        }
        Ok(Some(JsString(j(v).to_string().encode_utf16().collect())))
    }
    fn to_string(&mut self, v: &N) -> Result<JsString, String> {
        Ok(JsString(string(v).encode_utf16().collect()))
    }
}
impl CrossCacheHost for Host {
    fn null(&self) -> N {
        N::Null
    }
    fn is_undefined(&self, v: &N) -> bool {
        matches!(v, N::U)
    }
    fn truthy(&self, v: &N) -> bool {
        match v {
            N::U | N::Null => false,
            N::B(b) => *b,
            N::Num(n) => *n != 0. && !n.is_nan(),
            N::S(s) => !s.0.is_empty(),
            _ => true,
        }
    }
    fn session_storage(&mut self) -> Result<N, String> {
        self.events.push("storage".into());
        Ok(self.storage.clone())
    }
    fn literal(&self, t: &JsString) -> N {
        N::S(t.clone())
    }
    fn object(&mut self) -> Result<N, String> {
        Ok(object(vec![]))
    }
    fn set(&mut self, o: &N, k: &N, v: &N) -> Result<(), String> {
        self.put(o, &string(k), v, true)
    }
    fn define_data(&mut self, o: &N, k: &JsString, v: &N) -> Result<(), String> {
        self.put(o, &String::from_utf16_lossy(&k.0), v, true)
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, String> {
        let a = args.first().cloned().unwrap_or(N::U);
        let b = args.get(1).cloned().unwrap_or(N::U);
        if let N::Fn(f) = f {
            return run_callback(self, f.1, &f.0, r, args);
        }
        let N::Builtin(k) = f else {
            return Err("not callable".into());
        };
        match (r, k.as_str()) {
            (N::Map(m), "get") => Ok(m
                .borrow()
                .iter()
                .find(|p| same(&p.0, &a))
                .map(|p| p.1.clone())
                .unwrap_or(N::U)),
            (N::Map(m), "set") => {
                let mut m = m.borrow_mut();
                if let Some(p) = m.iter_mut().find(|p| same(&p.0, &a)) {
                    p.1 = b;
                } else {
                    m.push((a, b));
                }
                Ok(r.clone())
            }
            (N::Map(m), "delete") => {
                let mut m = m.borrow_mut();
                let old = m.len();
                m.retain(|p| !same(&p.0, &a));
                Ok(N::B(old != m.len()))
            }
            (N::Set(m), "add") => {
                if !m.borrow().iter().any(|v| same(v, &a)) {
                    m.borrow_mut().push(a);
                }
                Ok(r.clone())
            }
            (N::Set(m), "has") => Ok(N::B(m.borrow().iter().any(|v| same(v, &a)))),
            (N::Set(m), "delete") => {
                let mut m = m.borrow_mut();
                let old = m.len();
                m.retain(|v| !same(v, &a));
                Ok(N::B(old != m.len()))
            }
            (N::Set(m), "values") => Ok(N::Iter(Rc::new(RefCell::new((m.borrow().clone(), 0))))),
            (N::Iter(i), "next") => {
                let mut i = i.borrow_mut();
                let v = i.0.get(i.1).cloned();
                i.1 += 1;
                Ok(object(vec![
                    ("value", v.clone().unwrap_or(N::U)),
                    ("done", N::B(v.is_none())),
                ]))
            }
            (N::A(v), "push") => {
                v.borrow_mut().extend_from_slice(args);
                Ok(N::Num(v.borrow().len() as f64))
            }
            (N::A(v), "filter") | (N::A(v), "findIndex") => {
                let input = v.borrow().clone();
                let mut out = vec![];
                for (i, e) in input.into_iter().enumerate() {
                    let yes = self.call(&a, &N::U, &[e.clone(), N::Num(i as f64), r.clone()])?;
                    if self.truthy(&yes) {
                        if k == "findIndex" {
                            return Ok(N::Num(i as f64));
                        }
                        out.push(e)
                    }
                }
                if k == "findIndex" {
                    Ok(N::Num(-1.))
                } else {
                    Ok(arr(out))
                }
            }
            (N::A(v), "some") => {
                let input = v.borrow().clone();
                for (i, e) in input.into_iter().enumerate() {
                    let yes = self.call(&a, &N::U, &[e, N::Num(i as f64), r.clone()])?;
                    if self.truthy(&yes) {
                        return Ok(N::B(true));
                    }
                }
                Ok(N::B(false))
            }
            (N::A(v), "indexOf") => Ok(N::Num(
                v.borrow()
                    .iter()
                    .position(|v| eq(v, &a))
                    .map(|i| i as f64)
                    .unwrap_or(-1.),
            )),
            (N::A(v), "splice") => {
                let mut v = v.borrow_mut();
                let start = num(&a) as usize;
                let end = (start + num(&b) as usize).min(v.len());
                Ok(arr(v.drain(start..end).collect()))
            }
            (N::O(_), "postMessage") => {
                self.messages.push(a);
                Ok(N::U)
            }
            (N::O(_), "close") => {
                self.events.push("close".into());
                Ok(N::U)
            }
            (N::O(_), "removeItem") => {
                self.events.push(format!("remove:{}", string(&a)));
                self.delete(r, &a)?;
                Ok(N::U)
            }
            (N::O(_), "getItem") => self.get(r, &string(&a)),
            (N::O(_), "setItem") => {
                self.set(r, &a, &b)?;
                Ok(N::U)
            }
            _ => Err(format!("unsupported {k}")),
        }
    }
    fn parse_json_value(&mut self, v: &N) -> Result<N, String> {
        let t = self.to_string(v)?;
        self.parse_json(&t.0)
    }
    fn property_key(&mut self, v: &N) -> Result<N, String> {
        let t = self.to_string(v)?;
        Ok(N::S(t))
    }
    fn entries(&mut self, v: &N) -> Result<N, String> {
        let v = match v {
            N::Map(m) => m
                .borrow()
                .iter()
                .map(|p| arr(vec![p.0.clone(), p.1.clone()]))
                .collect(),
            N::A(a) => a.borrow().clone(),
            _ => return Err("iterator".into()),
        };
        Ok(N::Iter(Rc::new(RefCell::new((v, 0)))))
    }
    fn iterator_step(&mut self, i: &N) -> Result<Option<N>, String> {
        let N::Iter(i) = i else {
            return Err("iterator".into());
        };
        let mut i = i.borrow_mut();
        let v = i.0.get(i.1).cloned();
        i.1 += 1;
        Ok(v)
    }
    fn entry_pair(&mut self, v: &N) -> Result<[N; 2], String> {
        let N::A(a) = v else {
            return Err("pair".into());
        };
        let a = a.borrow();
        Ok([a[0].clone(), a[1].clone()])
    }
    fn close_after_throw(&mut self, _: &N) {
        self.events.push("iterator-close".into())
    }
    fn spread(&mut self, v: &N) -> Result<N, String> {
        let N::A(a) = v else {
            return Err("spread".into());
        };
        Ok(arr(a.borrow().clone()))
    }
    fn greater_than_500(&mut self, v: &N) -> Result<bool, String> {
        Ok(num(v) > 500.)
    }
}
impl CacheOrdersHost for Host {
    fn now(&mut self) -> Result<N, String> {
        Ok(N::Num(if self.ticks.is_empty() {
            self.time
        } else {
            self.ticks.remove(0)
        }))
    }
    fn object_keys(&mut self, v: &N) -> Result<N, String> {
        let N::O(o) = v else {
            return Err("keys".into());
        };
        let mut keys = o
            .borrow()
            .fields
            .iter()
            .filter(|p| p.2)
            .map(|p| p.0.clone())
            .collect::<Vec<_>>();
        keys.sort_by_key(|k| {
            k.parse::<u32>()
                .ok()
                .filter(|n| n.to_string() == *k && *n < u32::MAX)
                .map(|n| (0, n))
                .unwrap_or((1, 0))
        });
        Ok(arr(keys.iter().map(|k| s(k)).collect()))
    }
    fn get_index(&mut self, v: &N, k: &N) -> Result<N, String> {
        self.get(v, &string(k))
    }
    fn delete(&mut self, v: &N, k: &N) -> Result<(), String> {
        let N::O(o) = v else {
            return Err("delete".into());
        };
        o.borrow_mut().fields.retain(|p| p.0 != string(k));
        Ok(())
    }
    fn older_than(&mut self, n: &N, t: &N, m: f64) -> Result<bool, String> {
        Ok(num(n) - num(t) > m)
    }
    fn singleton(&mut self) -> Result<N, String> {
        Err("perp delegate outside fixture".into())
    }
    fn imported(&mut self, name: &str) -> Result<N, String> {
        Err(format!("import {name} outside fixture"))
    }
}
impl CacheRuntimeHost for Host {
    fn number(&self, n: f64) -> N {
        N::Num(n)
    }
    fn boolean(&self, b: bool) -> N {
        N::B(b)
    }
    fn equal(&self, a: &N, b: &N) -> bool {
        eq(a, b)
    }
    fn subtract(&mut self, a: &N, b: &N) -> Result<N, String> {
        Ok(N::Num(num(a) - num(b)))
    }
    fn less(&mut self, a: &N, b: &N) -> Result<bool, String> {
        Ok(num(a) < num(b))
    }
    fn less_equal(&mut self, a: &N, b: &N) -> Result<bool, String> {
        Ok(num(a) <= num(b))
    }
    fn abstract_string(&mut self, v: &N) -> Result<JsString, String> {
        self.to_string(v)
    }
    fn array(&mut self, v: &[N]) -> Result<N, String> {
        Ok(arr(v.to_vec()))
    }
    fn class_definition(&mut self) -> Result<(N, N), String> {
        Ok((N::Class(Rc::new(())), object(vec![])))
    }
    fn define_method(&mut self, p: &N, k: &JsString, f: &N) -> Result<(), String> {
        self.put(p, &String::from_utf16_lossy(&k.0), f, false)
    }
    fn instance(&mut self, p: &N) -> Result<N, String> {
        Ok(N::O(Rc::new(RefCell::new(Obj {
            fields: vec![],
            prototype: Some(p.clone()),
        }))))
    }
    fn new_map(&mut self) -> Result<N, String> {
        self.events.push("map".into());
        if self.fail_map {
            Err("map".into())
        } else {
            Ok(N::Map(Rc::new(RefCell::new(vec![]))))
        }
    }
    fn new_set(&mut self) -> Result<N, String> {
        self.events.push("set".into());
        Ok(N::Set(Rc::new(RefCell::new(vec![]))))
    }
    fn function(&mut self, c: N, b: Callback) -> Result<N, String> {
        Ok(N::Fn(Rc::new((c, b))))
    }
    fn broadcast_constructor(&mut self) -> Result<N, String> {
        self.reads += 1;
        self.events.push("broadcast-read".into());
        Ok(if self.broadcast {
            N::Class(Rc::new(()))
        } else {
            N::U
        })
    }
    fn construct_broadcast(&mut self, _: &N, name: &N) -> Result<N, String> {
        self.events.push(format!("channel:{}", string(name)));
        if self.fail_broadcast {
            return Err("broadcast".into());
        }
        Ok(object(vec![
            ("postMessage", N::Builtin("postMessage".into())),
            ("close", N::Builtin("close".into())),
        ]))
    }
    fn destructure(&mut self, v: &N, ks: &[&str]) -> Result<Vec<N>, String> {
        ks.iter().map(|k| self.get(v, k)).collect()
    }
}
// Assertions inspect native variants directly, before any wire checks.
pub fn assert_native(actual: &N, expected: &Value, label: &str) {
    match expected.as_array().unwrap()[0].as_str().unwrap() {
        "u" => assert!(matches!(actual, N::U), "{label}: undefined {actual:?}"),
        "null" => assert!(matches!(actual, N::Null), "{label}: null"),
        "bool" => {
            let N::B(b) = actual else {
                panic!("{label}: bool")
            };
            assert_eq!(*b, expected[1].as_bool().unwrap(), "{label}")
        }
        "number" => {
            let N::Num(n) = actual else {
                panic!("{label}: number")
            };
            if expected[1] == "nan" {
                assert!(n.is_nan(), "{label}")
            } else {
                assert_eq!(*n, expected[1].as_f64().unwrap(), "{label}");
            }
        }
        "string" => {
            let N::S(t) = actual else {
                panic!("{label}: string")
            };
            let units = expected[1]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u16)
                .collect::<Vec<_>>();
            assert_eq!(t.0, units, "{label}")
        }
        "array" => {
            let N::A(a) = actual else {
                panic!("{label}: array")
            };
            let a = a.borrow();
            let e = expected[1].as_array().unwrap();
            assert_eq!(a.len(), e.len(), "{label}");
            for (a, e) in a.iter().zip(e) {
                assert_native(a, e, label)
            }
        }
        "object" => {
            let N::O(a) = actual else {
                panic!("{label}: object")
            };
            let a = a.borrow();
            let fields = a.fields.iter().filter(|p| p.2).collect::<Vec<_>>();
            let e = expected[1].as_array().unwrap();
            assert_eq!(fields.len(), e.len(), "{label}");
            for (k, v) in e.iter().map(|e| (e[0].as_str().unwrap(), &e[1])) {
                let p = fields
                    .iter()
                    .find(|p| p.0 == k)
                    .unwrap_or_else(|| panic!("{label}: missing {k}"));
                assert_native(&p.1, v, label)
            }
        }
        "map" => {
            let N::Map(m) = actual else {
                panic!("{label}: map")
            };
            let m = m.borrow();
            let e = expected[1].as_array().unwrap();
            assert_eq!(m.len(), e.len(), "{label}");
            for (p, e) in m.iter().zip(e) {
                assert_native(&p.0, &e[0], label);
                assert_native(&p.1, &e[1], label)
            }
        }
        "set" => {
            let N::Set(m) = actual else {
                panic!("{label}: set")
            };
            let m = m.borrow();
            let e = expected[1].as_array().unwrap();
            assert_eq!(m.len(), e.len(), "{label}");
            for (a, e) in m.iter().zip(e) {
                assert_native(a, e, label)
            }
        }
        _ => panic!("unknown tag"),
    }
}
