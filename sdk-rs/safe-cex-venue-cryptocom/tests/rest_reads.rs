use safe_cex_venue_cryptocom::rest_reads::{self as b, ReadHost};
use serde_json::Value as Json;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Debug)]
enum N {
    Undefined,
    Null,
    Bool(bool),
    Num(f64),
    Text(Vec<u16>),
    Obj {
        tag: Option<String>,
        fields: RefCell<Vec<(Vec<u16>, V)>>,
    },
    Arr {
        tag: Option<String>,
        values: RefCell<Vec<V>>,
    },
    Error,
    Coercion(bool),
    Pair(Vec<V>),
    Set(Vec<V>),
}
type V = Rc<N>;
type E = V;
fn text(s: &str) -> V {
    Rc::new(N::Text(s.encode_utf16().collect()))
}
fn number(n: f64) -> V {
    Rc::new(N::Num(n))
}
fn undefined() -> V {
    Rc::new(N::Undefined)
}
fn string(v: &V) -> String {
    match &**v {
        N::Text(v) => String::from_utf16_lossy(v),
        N::Num(n) => n.to_string(),
        N::Undefined => "undefined".into(),
        N::Null => "null".into(),
        _ => "[object Object]".into(),
    }
}
fn input(v: &Json, tag: &str) -> V {
    if v["$instrument"] == true {
        return Rc::new(N::Coercion(false));
    }
    if v["$key"] == true {
        return Rc::new(N::Coercion(true));
    }
    if let Some(n) = v["$number"].as_str() {
        return number(match n {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => panic!("unknown fixture number"),
        });
    }
    Rc::new(match v {
        Json::Null => N::Null,
        Json::Bool(v) => N::Bool(*v),
        Json::Number(v) => N::Num(v.as_f64().unwrap()),
        Json::String(v) => N::Text(v.encode_utf16().collect()),
        Json::Array(v) => N::Arr {
            tag: Some(tag.into()),
            values: RefCell::new(
                v.iter()
                    .enumerate()
                    .map(|(i, v)| input(v, &format!("{tag}.{i}")))
                    .collect(),
            ),
        },
        Json::Object(v) => N::Obj {
            tag: Some(tag.into()),
            fields: RefCell::new(
                v.iter()
                    .map(|(k, v)| (k.encode_utf16().collect(), input(v, &format!("{tag}.{k}"))))
                    .collect(),
            ),
        },
    })
}
fn tag(v: &V) -> String {
    match &**v {
        N::Obj { tag, .. } | N::Arr { tag, .. } => {
            tag.clone().unwrap_or_else(|| "new-array".into())
        }
        _ => "value".into(),
    }
}
fn eq(a: &V, b: &V) -> bool {
    match (&**a, &**b) {
        (N::Text(a), N::Text(b)) => a == b,
        (N::Num(a), N::Num(b)) => a == b || a.is_nan() && b.is_nan(),
        _ => Rc::ptr_eq(a, b),
    }
}
fn units(v: &Json) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn assert_native(v: &V, e: &Json, label: &str) {
    match (&**v, e["kind"].as_str().unwrap()) {
        (N::Undefined, "undefined") | (N::Null, "null") => {}
        (N::Bool(v), "bool") => assert_eq!(
            *v,
            e["value"].as_bool().unwrap(),
            "CRYPTOCOM-READ-NATIVE-BOOL {label}"
        ),
        (N::Num(v), "number") => {
            let n = f64::from_bits(u64::from_str_radix(e["bits"].as_str().unwrap(), 16).unwrap());
            assert!(
                v.is_nan() && n.is_nan() || v.to_bits() == n.to_bits(),
                "CRYPTOCOM-READ-NATIVE-NUMBER {label}: {v:?} vs {n:?}"
            );
        }
        (N::Text(v), "string") => {
            assert_eq!(*v, units(&e["units"]), "CRYPTOCOM-READ-NATIVE-TEXT {label}")
        }
        (N::Arr { values, .. }, "array") => {
            let values = values.borrow();
            let expected = e["values"].as_array().unwrap();
            assert_eq!(
                values.len(),
                expected.len(),
                "CRYPTOCOM-READ-NATIVE-ARRAY {label}"
            );
            for (i, (v, e)) in values.iter().zip(expected).enumerate() {
                assert_native(v, e, &format!("{label}[{i}]"));
            }
        }
        (N::Obj { fields, .. }, "object") => {
            let fields = fields.borrow();
            let expected = e["fields"].as_array().unwrap();
            assert_eq!(
                fields.len(),
                expected.len(),
                "CRYPTOCOM-READ-NATIVE-FIELDS {label}"
            );
            for field in expected {
                let key = units(&field[0]);
                let v = &fields
                    .iter()
                    .find(|(k, _)| *k == key)
                    .unwrap_or_else(|| panic!("CRYPTOCOM-READ-NATIVE-KEY {label} {key:?}"))
                    .1;
                assert_native(
                    v,
                    &field[1],
                    &format!("{label}.{}", String::from_utf16_lossy(&key)),
                );
            }
        }
        _ => panic!("CRYPTOCOM-READ-NATIVE-KIND {label}: {v:?} vs {e}"),
    }
}
#[derive(Clone)]
struct NativeHost(Rc<Host>);
impl std::ops::Deref for NativeHost {
    type Target = Rc<Host>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
struct Prepared {
    receiver: V,
    key: &'static str,
}
struct Job {
    key: String,
    response: V,
    reject: bool,
    result: RefCell<Option<Result<V, E>>>,
}
struct Host {
    spec: Json,
    source: Json,
    root: V,
    xhr: V,
    store: V,
    markets: V,
    responses: [V; 2],
    events: RefCell<Vec<String>>,
    posts: RefCell<Vec<(String, V)>>,
    logs: RefCell<Vec<(V, V)>>,
    filter_returns: RefCell<Vec<V>>,
    jobs: RefCell<Vec<Rc<Job>>>,
    clock: Cell<u32>,
    bounds: Cell<u32>,
    filled: Cell<u32>,
    phase: E,
    post_error: E,
    mode_error: E,
    type_error: E,
    close_error: E,
}
impl Host {
    fn event(&self, p: impl Into<String>) -> Result<(), E> {
        let p = p.into();
        self.events.borrow_mut().push(p.clone());
        if self.spec["fail"] == p {
            Err(self.phase.clone())
        } else {
            Ok(())
        }
    }
    fn object_tag(tag: &str) -> V {
        Rc::new(N::Obj {
            tag: Some(tag.into()),
            fields: RefCell::new(vec![]),
        })
    }
    fn new(spec: Json, source: Json) -> Rc<Self> {
        let payload = |key: &str, tag: &str| {
            if spec.get(key).is_some() {
                input(&serde_json::json!({"data":spec[key]}), tag)
            } else {
                input(&serde_json::json!({}), tag)
            }
        };
        let response = |tag: &str, payload: V| {
            Rc::new(N::Obj {
                tag: Some(tag.into()),
                fields: RefCell::new(vec![("data".encode_utf16().collect(), payload)]),
            })
        };
        let responses = [
            response("response0", payload("data", "payload0")),
            response("response1", payload("advanced", "payload1")),
        ];
        let markets = input(
            spec.get("markets").unwrap_or(&serde_json::json!([])),
            "markets",
        );
        Rc::new(Self {
            spec,
            source,
            root: Self::object_tag("root"),
            xhr: Self::object_tag("xhr"),
            store: Self::object_tag("store"),
            markets,
            responses,
            events: RefCell::new(vec![]),
            posts: RefCell::new(vec![]),
            logs: RefCell::new(vec![]),
            filter_returns: RefCell::new(vec![]),
            jobs: RefCell::new(vec![]),
            clock: Cell::new(0),
            bounds: Cell::new(0),
            filled: Cell::new(0),
            phase: Rc::new(N::Error),
            post_error: Rc::new(N::Error),
            mode_error: Rc::new(N::Error),
            type_error: Rc::new(N::Error),
            close_error: Rc::new(N::Error),
        })
    }
    fn resolve(&self, job: &Job) -> Result<V, E> {
        if let Some(v) = job.result.borrow().clone() {
            return v;
        };
        self.event(format!("resolve:{}", job.key))?;
        let v = if job.reject {
            Err(self.post_error.clone())
        } else {
            Ok(job.response.clone())
        };
        *job.result.borrow_mut() = Some(v.clone());
        v
    }
    fn background(&self) {
        let jobs = self.jobs.borrow().clone();
        for job in jobs {
            let _ = self.resolve(&job);
        }
    }
    fn rows(&self, v: &V) -> Result<Vec<V>, E> {
        match &**v {
            N::Arr { values, .. } => Ok(values.borrow().clone()),
            _ => Err(self.type_error.clone()),
        }
    }
    fn raw_get(&self, v: &V, k: &str) -> V {
        match &**v {
            N::Obj { fields, .. } => fields
                .borrow()
                .iter()
                .find(|(key, _)| key.iter().copied().eq(k.encode_utf16()))
                .map(|(_, v)| v.clone())
                .unwrap_or_else(undefined),
            _ => undefined(),
        }
    }
}
struct PostFuture {
    host: NativeHost,
    jobs: Vec<Rc<Job>>,
}
impl Future for PostFuture {
    type Output = Result<V, E>;
    fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        let all = self.jobs.len() > 1;
        let results = self
            .jobs
            .iter()
            .map(|job| self.host.resolve(job))
            .collect::<Vec<_>>();
        let result = if all {
            results.into_iter().collect::<Result<Vec<_>, _>>().map(|v| {
                if self.host.spec["customAll"] == true {
                    Rc::new(N::Pair(v))
                } else {
                    self.host.array(v)
                }
            })
        } else {
            results.into_iter().next().unwrap()
        };
        Poll::Ready(result)
    }
}
// Host bindings borrow only for a single read/write; no guard crosses a callback/await.
impl ReadHost for NativeHost {
    type Value = V;
    type Error = E;
    type Prepared = Prepared;
    type PostFuture = PostFuture;
    type ModeFuture = std::future::Ready<Result<V, E>>;
    fn root(&self) -> V {
        self.root.clone()
    }
    fn undefined(&self) -> V {
        undefined()
    }
    fn number(&self, v: f64) -> V {
        number(v)
    }
    fn boolean(&self, v: bool) -> V {
        Rc::new(N::Bool(v))
    }
    fn string(&self, v: &'static str) -> V {
        text(v)
    }
    fn object(&self, fields: Vec<(&'static str, V)>) -> V {
        Rc::new(N::Obj {
            tag: None,
            fields: RefCell::new(
                fields
                    .into_iter()
                    .map(|(k, v)| (k.encode_utf16().collect(), v))
                    .collect(),
            ),
        })
    }
    fn array(&self, values: Vec<V>) -> V {
        Rc::new(N::Arr {
            tag: None,
            values: RefCell::new(values),
        })
    }
    fn get(&self, v: &V, key: &'static str) -> Result<V, E> {
        if let N::Obj { tag: Some(t), .. } = &**v {
            self.event(format!("get:{t}:{key}"))?;
            if t == "root" {
                return Ok(match key {
                    "xhr" => self.xhr.clone(),
                    "store" => self.store.clone(),
                    _ => undefined(),
                });
            }
            if t == "store" && key == "markets" {
                return Ok(self.markets.clone());
            }
            if self.spec["changeBounds"] == true && t == "opts" && (key == "from" || key == "to") {
                let n = self.bounds.get() + 1;
                self.bounds.set(n);
                return Ok(number(f64::from(n)));
            }
            if self.spec["changeFilled"] == true && t == "order" && key == "cumulative_quantity" {
                let n = self.filled.get() + 1;
                self.filled.set(n);
                return Ok(text(if n == 1 { "1" } else { "2" }));
            }
        }
        if matches!(&**v, N::Undefined | N::Null) {
            return Err(self.type_error.clone());
        }
        Ok(self.raw_get(v, key))
    }
    fn index(&self, v: &V, i: usize) -> Result<V, E> {
        match &**v {
            N::Arr { values, .. } => Ok(values.borrow().get(i).cloned().unwrap_or_else(undefined)),
            N::Obj { .. } => Ok(undefined()),
            _ => Err(self.type_error.clone()),
        }
    }
    fn truthy(&self, v: &V) -> bool {
        match &**v {
            N::Undefined | N::Null => false,
            N::Bool(v) => *v,
            N::Num(v) => *v != 0.0 && !v.is_nan(),
            N::Text(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(&**v, N::Undefined | N::Null)
    }
    fn equal_string(&self, v: &V, s: &'static str) -> bool {
        matches!(&**v,N::Text(v)if v.iter().copied().eq(s.encode_utf16()))
    }
    fn zero_number(&self, value: &V) -> bool {
        matches!(&**value,N::Num(n)if *n==0.0)
    }
    fn greater_zero(&self, value: V) -> Result<bool, E> {
        Ok(match &*value {
            N::Num(n) => *n > 0.0,
            _ => string(&value).parse::<f64>().unwrap_or(f64::NAN) > 0.0,
        })
    }
    fn divide(&self, value: V, denominator: f64) -> Result<f64, E> {
        Ok(match &*value {
            N::Num(n) => *n,
            _ => string(&value).parse::<f64>().unwrap_or(f64::NAN),
        } / denominator)
    }
    fn prepare(&self, receiver: &V, key: &'static str) -> Result<Prepared, E> {
        if key == "includes" || key == "has" || key == "push" {
        } else {
            self.event(format!("prepare:{key}"))?;
        }
        Ok(Prepared {
            receiver: receiver.clone(),
            key,
        })
    }
    fn call(&self, p: Prepared, args: Vec<V>) -> Result<V, E> {
        match p.key {
            "push" => {
                self.event("push")?;
                if let N::Arr { values, .. } = &*p.receiver {
                    values.borrow_mut().extend(args);
                    Ok(number(values.borrow().len() as f64))
                } else {
                    Err(self.type_error.clone())
                }
            }
            "includes" => {
                self.event("includes")?;
                if self.spec["includesObject"] == true {
                    return Ok(self.object(vec![("marker", number(42.0))]));
                }
                Ok(self.boolean(match &*p.receiver {
                    N::Text(v) => String::from_utf16_lossy(v).contains(&string(&args[0])),
                    N::Arr { values, .. } => values.borrow().iter().any(|v| eq(v, &args[0])),
                    N::Coercion(false) => true,
                    _ => return Err(self.type_error.clone()),
                }))
            }
            "has" => {
                if let N::Set(v) = &*p.receiver {
                    Ok(self.boolean(v.iter().any(|v| eq(v, &args[0]))))
                } else {
                    Err(self.type_error.clone())
                }
            }
            "log" => {
                assert!(
                    Rc::ptr_eq(&p.receiver, &self.root),
                    "CRYPTOCOM-READ-NATIVE-RECEIVER log"
                );
                self.event("log")?;
                self.logs
                    .borrow_mut()
                    .push((args[0].clone(), args[1].clone()));
                Ok(undefined())
            }
            "mapOrder" => {
                assert!(Rc::ptr_eq(&p.receiver, &self.root));
                self.event("call:mapOrder")?;
                b::map_order(self, args[0].clone())
            }
            "mapCryptocomOrderType" | "mapOrderStatus" | "mapCryptocomTimeInForce" => {
                assert!(
                    Rc::ptr_eq(&p.receiver, &self.root),
                    "CRYPTOCOM-READ-NATIVE-RECEIVER mapper"
                );
                self.event(format!("call:{}", p.key))?;
                let v = if let N::Text(v) = &*args[0] {
                    Some(v.as_slice())
                } else {
                    None
                };
                let out = match p.key {
                    "mapCryptocomOrderType" => serde_json::to_value(
                        safe_cex_venue_cryptocom::order_mappings::map_cryptocom_order_type(v),
                    )
                    .unwrap(),
                    "mapOrderStatus" => serde_json::to_value(
                        safe_cex_venue_cryptocom::order_mappings::map_order_status(v),
                    )
                    .unwrap(),
                    _ => serde_json::to_value(
                        safe_cex_venue_cryptocom::order_mappings::map_cryptocom_time_in_force(v),
                    )
                    .unwrap(),
                };
                Ok(text(out.as_str().unwrap()))
            }
            _ => panic!("unexpected prepared method"),
        }
    }
    fn endpoint(&self, key: &'static str) -> Result<V, E> {
        self.event(format!("endpoint:{key}"))?;
        Ok(text(key))
    }
    fn post(&self, p: Prepared, path: V, params: V) -> Result<PostFuture, E> {
        assert_eq!(p.key, "post");
        assert!(
            Rc::ptr_eq(&p.receiver, &self.xhr),
            "CRYPTOCOM-READ-NATIVE-RECEIVER post"
        );
        let key = string(&path);
        self.posts.borrow_mut().push((key.clone(), params));
        self.event("post")?;
        self.event(format!("post:{key}"))?;
        let idx = usize::from(key == "ADVANCED_OPEN_ORDERS");
        let job = Rc::new(Job {
            key,
            response: self.responses[idx].clone(),
            reject: self.spec["reject"] == true || idx == 1 && self.spec["advancedReject"] == true,
            result: RefCell::new(None),
        });
        self.jobs.borrow_mut().push(job.clone());
        Ok(PostFuture {
            host: self.clone(),
            jobs: vec![job],
        })
    }
    fn prepare_all(&self) -> Result<Prepared, E> {
        self.event("prepare-all")?;
        Ok(Prepared {
            receiver: self.root.clone(),
            key: "all",
        })
    }
    fn all(&self, prepared: Prepared, futures: Vec<PostFuture>) -> Result<PostFuture, E> {
        assert_eq!(prepared.key, "all");
        self.event("all")?;
        Ok(PostFuture {
            host: self.clone(),
            jobs: futures.into_iter().flat_map(|f| f.jobs).collect(),
        })
    }
    fn position_mode(&self, p: Prepared) -> Result<Self::ModeFuture, E> {
        assert_eq!(p.key, "getPositionMode");
        assert!(Rc::ptr_eq(&p.receiver, &self.root));
        let result = self
            .event("mode-call")
            .and_then(|()| self.event("mode-resolve"))
            .and_then(|()| {
                if self.spec["modeReject"] == true {
                    Err(self.mode_error.clone())
                } else {
                    Ok(text("hedged"))
                }
            });
        Ok(std::future::ready(result))
    }
    fn imported(&self, module: &'static str, key: &'static str) -> Result<V, E> {
        Ok(text(match (module, key) {
            ("OrderSide", "Buy") => "buy",
            ("OrderSide", "Sell") => "sell",
            ("PositionSide", "Long") => "long",
            ("PositionSide", "Short") => "short",
            ("MarginType", "Isolated") => "isolated",
            ("MarginType", "Cross") => "cross",
            _ => panic!("unknown native import {module}.{key}"),
        }))
    }
    fn prepare_parse_float(&self) -> Result<Prepared, E> {
        Ok(Prepared {
            receiver: self.root.clone(),
            key: "parseFloat",
        })
    }
    fn call_parse_float(&self, prepared: Prepared, v: V) -> Result<f64, E> {
        assert_eq!(prepared.key, "parseFloat");
        self.event("parseFloat")?;
        Ok(safe_cex_num::parse_float(&string(&v)))
    }
    fn prepare_pow(&self) -> Result<Prepared, E> {
        self.event("prepare-pow")?;
        Ok(Prepared {
            receiver: self.root.clone(),
            key: "pow",
        })
    }
    fn pow10_negative(&self, prepared: Prepared, v: V) -> Result<f64, E> {
        assert_eq!(prepared.key, "pow");
        self.event("pow")?;
        let n = match &*v {
            N::Num(n) => *n,
            _ => string(&v).parse().unwrap_or(f64::NAN),
        };
        Ok(10.0_f64.powf(-n))
    }
    fn abs(&self, v: V) -> Result<f64, E> {
        self.event("abs")?;
        Ok(match &*v {
            N::Num(n) => *n,
            _ => string(&v).parse::<f64>().unwrap_or(f64::NAN),
        }
        .abs())
    }
    fn max_zero(&self, v: f64) -> Result<f64, E> {
        self.event("max")?;
        Ok(if v.is_nan() { f64::NAN } else { 0.0_f64.max(v) })
    }
    fn now(&self) -> Result<V, E> {
        self.event("now")?;
        let n = self.clock.get();
        self.clock.set(n + 1);
        Ok(number(1000.0 + f64::from(n)))
    }
    fn template_string(&self, v: V) -> Result<V, E> {
        if matches!(&*v, N::Coercion(false)) {
            self.event("instrument-string")?;
            return Err(self.phase.clone());
        }
        Ok(text(&string(&v)))
    }
    fn property_key(&self, v: V) -> Result<V, E> {
        if matches!(&*v, N::Coercion(true)) {
            self.event("key-string")?;
            return Err(self.phase.clone());
        }
        Ok(v)
    }
    fn iterator_pair<F: FnMut(V) -> Result<V, E>>(&self, v: V, mut bind: F) -> Result<(V, V), E> {
        if let N::Pair(values) = &*v {
            self.event("pair-iterator")?;
            self.event("pair-next:0")?;
            let first = match bind(values[1].clone()) {
                Ok(v) => v,
                Err(e) => {
                    let _ = self.event("pair-close");
                    return Err(e);
                }
            };
            self.event("pair-next:1")?;
            let second = match bind(values[0].clone()) {
                Ok(v) => v,
                Err(e) => {
                    let _ = self.event("pair-close");
                    return Err(e);
                }
            };
            self.event("pair-close")?;
            if self.spec["closeThrow"] == true {
                return Err(self.close_error.clone());
            }
            return Ok((first, second));
        }
        if let N::Arr { tag: Some(t), .. } = &*v {
            self.event(format!("iterate:{t}"))?;
        }
        let values = self.rows(&v)?;
        let first = bind(values.first().cloned().unwrap_or_else(undefined))?;
        let second = bind(values.get(1).cloned().unwrap_or_else(undefined))?;
        Ok((first, second))
    }
    fn ticker_id(&self, instrument: V, time: V) -> Result<V, E> {
        Ok(text(&format!("{}-{}", string(&instrument), string(&time))))
    }
    fn interval(&self, v: V) -> Result<V, E> {
        self.event("interval")?;
        let mapped =
            safe_cex_venue_cryptocom::types::map_candlestick_interval(&string(&v)).unwrap();
        match mapped {
            safe_cex_num::prototype_realm::PrototypeValue::String(s) => Ok(text(&s)),
            _ => panic!("fixture interval"),
        }
    }
    fn create_position_id(&self, symbol: V, side: V, _mode: V) -> Result<V, E> {
        self.event("position-id")?;
        Ok(text(&format!("{}:{}", string(&symbol), string(&side))))
    }
    fn map<F: FnMut(V) -> Result<V, E>>(&self, array: V, mut callback: F) -> Result<V, E> {
        self.event(format!("map:{}", tag(&array)))?;
        let mut out = vec![];
        for v in self.rows(&array)? {
            out.push(callback(v)?);
        }
        Ok(self.array(out))
    }
    fn filter<F: FnMut(V) -> Result<V, E>>(&self, array: V, mut callback: F) -> Result<V, E> {
        self.event(format!("filter:{}", tag(&array)))?;
        let mut out = vec![];
        for v in self.rows(&array)? {
            let returned = callback(v.clone())?;
            self.filter_returns.borrow_mut().push(returned.clone());
            if self.truthy(&returned) {
                out.push(v);
            }
        }
        Ok(self.array(out))
    }
    fn spread(&self, array: V) -> Result<Vec<V>, E> {
        if let N::Arr { tag: Some(t), .. } = &*array {
            self.event(format!("iterate:{t}"))?;
        }
        self.rows(&array)
    }
    fn each<F: FnMut(V) -> Result<(), E>>(&self, array: V, mut callback: F) -> Result<(), E> {
        for v in self.spread(array)? {
            callback(v)?;
        }
        Ok(())
    }
    fn prepare_set(&self) -> Result<Prepared, E> {
        self.event("prepare-set")?;
        Ok(Prepared {
            receiver: self.root.clone(),
            key: "set",
        })
    }
    fn set(&self, prepared: Prepared, array: V) -> Result<V, E> {
        assert_eq!(prepared.key, "set");
        self.event("set")?;
        Ok(Rc::new(N::Set(self.rows(&array)?)))
    }
    fn uniq_by(&self, array: V, key: &'static str) -> Result<V, E> {
        self.event("uniqBy")?;
        self.event(format!("filter:{}", tag(&array)))?;
        let mut seen = vec![];
        let mut out = vec![];
        for value in self.rows(&array)? {
            let value_key = self.get(&value, key)?;
            let unique = !seen.iter().any(|v| eq(v, &value_key));
            self.filter_returns.borrow_mut().push(self.boolean(unique));
            if unique {
                seen.push(value_key);
                out.push(value);
            }
        }
        Ok(self.array(out))
    }

    fn assign(&self, object: &V, key: V, value: V) -> Result<(), E> {
        let key = if let N::Text(v) = &*key {
            v.clone()
        } else {
            string(&key).encode_utf16().collect()
        };
        if let N::Obj { fields, .. } = &**object {
            let mut fields = fields.borrow_mut();
            if let Some((_, v)) = fields.iter_mut().find(|(k, _)| *k == key) {
                *v = value;
            } else {
                fields.push((key, value));
            }
            Ok(())
        } else {
            Err(self.type_error.clone())
        }
    }
    fn error_value(&self, error: &E) -> V {
        error.clone()
    }
    fn error_template(&self, prefix: &'static str, error: V) -> Result<V, E> {
        let suffix = if Rc::ptr_eq(&error, &self.type_error) {
            let units = units(&self.source["logs"][0]["message"]);
            String::from_utf16_lossy(&units)
                .strip_prefix(prefix)
                .unwrap()
                .to_owned()
        } else {
            if Rc::ptr_eq(&error, &self.phase) {
                "error phase".into()
            } else if Rc::ptr_eq(&error, &self.post_error) {
                "error post".into()
            } else if Rc::ptr_eq(&error, &self.mode_error) {
                "error mode".into()
            } else if Rc::ptr_eq(&error, &self.close_error) {
                "error close".into()
            } else {
                panic!("original template error")
            }
        };
        Ok(text(&format!("{prefix}{suffix}")))
    }
    fn severity_error(&self) -> Result<V, E> {
        Ok(text(self.source["_severity"].as_str().unwrap()))
    }
}
fn drive<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let mut context = Context::from_waker(Waker::noop());
    match future.as_mut().poll(&mut context) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("bounded driver unexpectedly pending"),
    }
}
#[test]
fn rest_reads_match_actual_typescript_native() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/rest_reads_oracle.cjs"
        ))
        .env(
            "SAFE_CEX_TYPESCRIPT_PATH",
            std::env::var("SAFE_CEX_TYPESCRIPT_PATH").expect("compiler required"),
        )
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "oracle failed {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Json = serde_json::from_slice(&output.stdout).unwrap();
    let specs: Vec<Json> = serde_json::from_str(include_str!("rest_reads_fixtures.json")).unwrap();
    let rows = oracle["rows"].as_array().unwrap();
    let mut measured = 0;
    for (spec, source) in specs.iter().zip(rows) {
        let name = spec["name"].as_str().unwrap();
        let mut source = source.clone();
        source["_severity"] = oracle["severity"].clone();
        let h = NativeHost(Host::new(spec.clone(), source.clone()));
        let host = Rc::new(h.clone());
        let original_order = input(spec.get("order").unwrap_or(&serde_json::json!({})), "order");
        let op = spec["op"].as_str().unwrap();
        let mut immediate = vec![];
        macro_rules! run {
            ($future:expr) => {{
                let future = $future;
                immediate = h.events.borrow().clone();
                drive(future)
            }};
        }
        let result = match op {
            "markets" => run!(b::fetch_markets(host)),
            "tickers" => run!(b::fetch_tickers(host)),
            "ohlcv" => run!(b::fetch_ohlcv(
                host,
                input(spec.get("opts").unwrap_or(&serde_json::json!({})), "opts")
            )),
            "book" => run!(b::fetch_order_book(host, input(&spec["symbol"], "symbol"))),
            "orders" => run!(b::fetch_orders(host)),
            "positions" => run!(b::fetch_positions(host)),
            "balance" => run!(b::fetch_balance(host)),
            "mapOrder" => {
                let result = b::map_order(&h, original_order.clone());
                immediate = h.events.borrow().clone();
                result
            }
            _ => panic!("fixture operation"),
        };
        h.background();
        match result {
            Ok(v) => {
                assert!(
                    source["error"].is_null(),
                    "CRYPTOCOM-READ-NATIVE-ERROR {name} unexpected success"
                );
                assert_native(&v, &source["result"], name);
                if op == "mapOrder" {
                    assert!(
                        Rc::ptr_eq(
                            &h.raw_get(&v, "id"),
                            &h.raw_get(&original_order, "order_id")
                        ),
                        "CRYPTOCOM-READ-NATIVE-ORIGINAL-ID {name}"
                    );
                    assert!(
                        Rc::ptr_eq(
                            &h.raw_get(&v, "symbol"),
                            &h.raw_get(&original_order, "instrument_name")
                        ),
                        "CRYPTOCOM-READ-NATIVE-ORIGINAL-SYMBOL {name}"
                    );
                }
                if spec["retain"] == true {
                    let currencies = h.raw_get(&v, "currencies");
                    let old = h.raw_get(&currencies, "USD");
                    h.assign(&currencies, text("USD"), h.object(vec![]))
                        .unwrap();
                    assert!(
                        !Rc::ptr_eq(&old, &h.raw_get(&currencies, "USD")),
                        "CRYPTOCOM-READ-NATIVE-REPLACE {name}"
                    );
                    let available = h.raw_get(&old, "available");
                    assert_eq!(
                        string(&available),
                        "9",
                        "CRYPTOCOM-READ-NATIVE-RETAIN {name}"
                    );
                    h.assign(&old, text("marker"), number(99.0)).unwrap();
                    assert_eq!(string(&h.raw_get(&old, "marker")), "99");
                }
            }
            Err(error) => {
                let expected = match source["error"].as_str().unwrap_or("none") {
                    "phase-error" => &h.phase,
                    "post-error" => &h.post_error,
                    "mode-error" => &h.mode_error,
                    "type-error" => &h.type_error,
                    _ => {
                        panic!("CRYPTOCOM-READ-NATIVE-ERROR {name} unexpected Rust error {error:?}")
                    }
                };
                assert!(
                    Rc::ptr_eq(&error, expected),
                    "CRYPTOCOM-READ-NATIVE-ERROR-IDENTITY {name}"
                );
            }
        }
        let posts = h.posts.borrow();
        let expected_posts = usize::try_from(spec["expectedPosts"].as_u64().unwrap()).unwrap();
        assert_eq!(
            posts.len(),
            expected_posts,
            "CRYPTOCOM-READ-NATIVE-POST-COUNT {name}"
        );
        for ((key, params), expected) in posts.iter().zip(source["posts"].as_array().unwrap()) {
            assert_eq!(
                key,
                expected["key"].as_str().unwrap(),
                "CRYPTOCOM-READ-NATIVE-ENDPOINT {name}"
            );
            assert_native(params, &expected["params"], &format!("{name}.request"));
        }
        let logs = h.logs.borrow();
        for ((message, severity), expected) in logs.iter().zip(source["logs"].as_array().unwrap()) {
            assert_eq!(
                if let N::Text(v) = &**message {
                    v
                } else {
                    panic!("native log text")
                },
                &units(&expected["message"]),
                "CRYPTOCOM-READ-NATIVE-LOG {name}"
            );
            assert_eq!(
                string(severity),
                expected["severity"].as_str().unwrap(),
                "CRYPTOCOM-READ-NATIVE-SEVERITY {name}"
            );
        }
        assert_eq!(
            logs.len(),
            source["logs"].as_array().unwrap().len(),
            "CRYPTOCOM-READ-NATIVE-LOG-COUNT {name}"
        );
        let filter_returns = h.filter_returns.borrow();
        let expected_returns = source["filterReturns"].as_array().unwrap();
        assert_eq!(
            filter_returns.len(),
            expected_returns.len(),
            "CRYPTOCOM-READ-NATIVE-FILTER-RETURN-COUNT {name}"
        );
        for (i, (actual, expected)) in filter_returns.iter().zip(expected_returns).enumerate() {
            assert_native(actual, expected, &format!("{name}.filterReturn[{i}]"));
        }
        if name == "markets-original-filter-value" {
            let payload = h.raw_get(&h.responses[0], "data");
            let rows = h.raw_get(&payload, "data");
            let original = h.rows(&rows).unwrap();
            assert!(
                Rc::ptr_eq(&filter_returns[0], &h.raw_get(&original[0], "tradable")),
                "CRYPTOCOM-READ-NATIVE-FILTER-RETURN-IDENTITY {name}"
            );
        }
        // Native results/errors/request/log fields above precede trace and inventory assertions.
        let strings = |v: &Json| {
            v.as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            immediate,
            strings(&source["immediate"]),
            "CRYPTOCOM-READ-NATIVE-EAGER {name}"
        );
        assert_eq!(
            *h.events.borrow(),
            strings(&source["events"]),
            "CRYPTOCOM-READ-NATIVE-EFFECTS {name}"
        );
        measured += 1;
    }
    assert_eq!(specs.len(), 68, "CRYPTOCOM-READ independent fixture floor");
    assert_eq!(rows.len(), 68, "CRYPTOCOM-READ oracle floor");
    assert_eq!(measured, 68, "CRYPTOCOM-READ measured floor");
    assert_eq!(specs[67]["name"], "balance-replace-retain");
    eprintln!("READS native floor68 stops index67 balance-replace-retain");
}
