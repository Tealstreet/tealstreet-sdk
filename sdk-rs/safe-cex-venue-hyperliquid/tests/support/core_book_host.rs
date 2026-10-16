use super::cache::{self, N, arr, object, s};
use safe_cex_venue_hyperliquid::{
    candle_listeners::{CandleCallback, CandleListenerHost},
    core_book_listener::*,
    json_boundaries::JsPayloadHost,
    spot_json::JsString,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use serde_json::Value;
use std::collections::BTreeMap;
pub fn same(a: &N, b: &N) -> bool {
    cache::eq(a, b) || matches!((a,b),(N::Builtin(a),N::Builtin(b))if a==b)
}
pub fn own(v: &N, k: &str) -> N {
    let N::O(v) = v else { panic!("own object") };
    v.borrow()
        .fields
        .iter()
        .find(|p| p.0 == k)
        .map_or(N::U, |p| p.1.clone())
}
pub fn input(v: &Value) -> N {
    match v {
        Value::String(v) => match v.as_str() {
            "$undefined" => N::U,
            "$nan" => N::Num(f64::NAN),
            "$inf" => N::Num(f64::INFINITY),
            "$-0" => N::Num(-0.),
            _ => s(v),
        },
        Value::Object(v) if v.contains_key("units") => N::S(JsString(
            v["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_u64().unwrap() as u16)
                .collect(),
        )),
        _ => cache::Host::decode(v),
    }
}
pub fn text(v: &N) -> JsString {
    match v {
        N::S(s) => s.clone(),
        N::Num(n) => JsString(safe_cex_num::js_number_string(*n).encode_utf16().collect()),
        N::U => JsString("undefined".encode_utf16().collect()),
        _ => panic!("fixture text"),
    }
}
pub struct Table {
    pub value: N,
    pub entries: Vec<(JsString, N)>,
}
pub struct Host {
    pub name: String,
    pub socket: N,
    pub parent: N,
    pub id: N,
    pub handlers: N,
    pub topics: N,
    pub inner: N,
    pub tables: Vec<Table>,
    pub callbacks: BTreeMap<String, CoreBookCallback<N>>,
    pub timers: Vec<(N, N)>,
    pub clear: Vec<N>,
    pub books: Vec<N>,
    pub pre_total: Vec<N>,
    pub total_inputs: Vec<N>,
    pub filter_results: Vec<N>,
    pub sends: Vec<(bool, N)>,
    pub logs: Vec<N>,
    pub error: N,
    pub first_level: N,
    pub levels: N,
    pub last_bbo: N,
    pub number_changed: bool,
    pub float_changed: bool,
    pub px_reads: usize,
    pub calls: Vec<String>,
    next: usize,
}
impl Host {
    pub fn new(row: &Value) -> Self {
        let handlers = object(vec![]);
        let topics = object(vec![]);
        let inner = object(vec![]);
        let id = input(&row["id"]);
        let market = object(vec![
            ("symbol", s("BTC/USDC")),
            ("id", id.clone()),
            (
                "precision",
                object(vec![(
                    "price",
                    N::Num(if row["name"] == "zero-precision" {
                        0.
                    } else {
                        0.1
                    }),
                )]),
            ),
        ]);
        let markets = if row["name"] == "missing-market" {
            arr(vec![object(vec![("symbol", s("OTHER"))])])
        } else {
            arr(vec![object(vec![("symbol", s("OTHER"))]), market])
        };
        let parent = object(vec![
            ("store", object(vec![("markets", markets)])),
            ("log", N::Builtin("log".into())),
            ("emitAndLogError", N::Builtin("emit-error".into())),
        ]);
        let socket = object(vec![
            ("parent", parent.clone()),
            ("isConnected", N::B(row["connected"].as_bool().unwrap())),
            ("messageHandlers", handlers.clone()),
            ("topics", topics.clone()),
            ("orderBookInnerState", inner.clone()),
            ("sendSubscription", N::Builtin("subscribe".into())),
            ("sendUnsubscription", N::Builtin("unsubscribe".into())),
        ]);
        let tables = [&handlers, &topics, &inner]
            .into_iter()
            .map(|v| Table {
                value: v.clone(),
                entries: vec![],
            })
            .collect();
        let mut h = Self {
            name: row["name"].as_str().unwrap().into(),
            socket,
            parent,
            id,
            handlers,
            topics,
            inner,
            tables,
            callbacks: BTreeMap::new(),
            timers: vec![],
            clear: vec![],
            books: vec![],
            pre_total: vec![],
            total_inputs: vec![],
            filter_results: vec![],
            sends: vec![],
            logs: vec![],
            error: object(vec![("original", s("book failure"))]),
            first_level: N::U,
            levels: N::U,
            last_bbo: N::U,
            number_changed: false,
            float_changed: false,
            px_reads: 0,
            calls: vec![],
            next: 0,
        };
        if h.name == "foreign-handlers" {
            for prefix in ["l2Book.", "bbo."] {
                let k = h.key(prefix);
                h.slot_set(
                    &h.handlers.clone(),
                    k,
                    arr(vec![N::Builtin("foreign".into())]),
                );
            }
        }
        h
    }
    pub fn key(&self, prefix: &str) -> JsString {
        let mut k = prefix.encode_utf16().collect::<Vec<_>>();
        k.extend(text(&self.id).0);
        JsString(k)
    }
    pub fn slot(&self, t: &N, k: &JsString) -> N {
        self.tables
            .iter()
            .find(|p| same(&p.value, t))
            .unwrap()
            .entries
            .iter()
            .find(|p| p.0 == *k)
            .map_or(N::U, |p| p.1.clone())
    }
    fn slot_set(&mut self, t: &N, k: JsString, v: N) {
        let entries = &mut self
            .tables
            .iter_mut()
            .find(|p| same(&p.value, t))
            .unwrap()
            .entries;
        if let Some(p) = entries.iter_mut().find(|p| p.0 == k) {
            p.1 = v
        } else {
            entries.push((k, v))
        }
    }
    pub fn connected(&mut self, v: bool) {
        self.set(&self.socket.clone(), "isConnected", &N::B(v))
            .unwrap()
    }
    pub fn invoke(&mut self, f: &N, args: &[N]) -> Result<N, N> {
        self.call(f, &N::U, args)
    }
    pub fn parser(&self, bbo: bool) -> N {
        self.callbacks
            .iter()
            .find(|(_, c)| {
                if bbo {
                    matches!(c, CoreBookCallback::BboParser { .. })
                } else {
                    matches!(c, CoreBookCallback::Parser { .. })
                }
            })
            .map(|(id, _)| N::Builtin(id.clone()))
            .unwrap()
    }
    pub fn frame(&mut self, action: &Value) -> N {
        if action["kind"] == "depth" {
            let sides = action["levels"]
                .as_array()
                .unwrap()
                .iter()
                .map(|side| {
                    arr(side
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|p| object(vec![("px", input(&p[0])), ("sz", input(&p[1]))]))
                        .collect())
                })
                .collect::<Vec<_>>();
            self.levels = sides[0].clone();
            if let N::A(a) = &self.levels {
                self.first_level = a.borrow().first().cloned().unwrap_or(N::U)
            }
            object(vec![
                ("levels", arr(sides)),
                ("time", input(&action["time"])),
            ])
        } else {
            let best = if let Some(a) = action["bbo"].as_array() {
                arr(a
                    .iter()
                    .map(|p| {
                        if p.is_null() {
                            N::Null
                        } else {
                            object(vec![("px", input(&p[0])), ("sz", input(&p[1]))])
                        }
                    })
                    .collect())
            } else {
                N::Null
            };
            let event = object(vec![("bbo", best), ("time", input(&action["time"]))]);
            self.last_bbo = event.clone();
            event
        }
    }
    pub fn snapshot(&self) -> N {
        let table = |t: &N| {
            arr(self
                .tables
                .iter()
                .find(|p| same(&p.value, t))
                .unwrap()
                .entries
                .iter()
                .map(|(k, v)| arr(vec![N::S(k.clone()), v.clone()]))
                .collect())
        };
        object(vec![
            ("books", arr(self.books.clone())),
            ("handlers", table(&self.handlers)),
            ("topics", table(&self.topics)),
            ("logs", arr(self.logs.clone())),
            ("clear", arr(self.clear.clone())),
            (
                "subscriptions",
                arr(self
                    .sends
                    .iter()
                    .map(|(u, v)| arr(vec![N::B(*u), v.clone()]))
                    .collect()),
            ),
        ])
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, s: &JsString) -> N {
        N::S(s.clone())
    }
    fn truthy(&self, v: &N) -> bool {
        !matches!(v, N::U | N::Null | N::B(false))
            && !matches!(v,N::Num(n)if *n==0.||n.is_nan())
            && !matches!(v,N::S(s)if s.0.is_empty())
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        Ok(N::Builtin(
            if k == "Number" && self.number_changed {
                "NumberZero"
            } else if k == "parseFloat" && self.float_changed {
                "parseFloatDouble"
            } else {
                k
            }
            .into(),
        ))
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if same(v, &self.first_level) && k == "px" {
            self.px_reads += 1;
            if self.px_reads == 2 {
                if self.name == "live-depth-loop" {
                    let N::A(a) = &self.levels else {
                        unreachable!()
                    };
                    a.borrow_mut()
                        .push(object(vec![("px", s("98")), ("sz", s("8"))]));
                }
                if self.name == "parse-callee-capture" {
                    self.float_changed = true
                }
            }
        }
        if self.name == "number-callee-capture" && k == "time" {
            self.number_changed = true;
        }
        if let N::Builtin(n) = v {
            return Ok(N::Builtin(format!("{n}.{k}")));
        }
        cache::Host::default()
            .get(v, k)
            .map_err(|_| self.error.clone())
    }
    fn set(&mut self, o: &N, k: &str, v: &N) -> Result<(), N> {
        cache::Host::default()
            .put(o, k, v, true)
            .map_err(|_| self.error.clone())
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn define_data(&mut self, o: &N, k: &str, v: &N) -> Result<(), N> {
        self.set(o, k, v)
    }
    fn callback(&mut self, _: PingCallback<N>) -> Result<N, N> {
        panic!("unrelated ping")
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("fixture callee")
        };
        if let Some(c) = self.callbacks.get(id).cloned() {
            return run_core_book_callback(self, &c, args);
        }
        let a = args.first().cloned().unwrap_or(N::U);
        self.calls.push(id.clone());
        match id.as_str() {
            "find" | "map" | "filter" => {
                let N::A(values) = r else {
                    panic!("array receiver")
                };
                let len = values.borrow().len();
                let mut out = vec![];
                for i in 0..len {
                    let v = values.borrow().get(i).cloned().unwrap_or(N::U);
                    let result = self.call(&a, &N::U, &[v.clone(), N::Num(i as f64), r.clone()])?;
                    if id == "find" && self.truthy(&result) {
                        return Ok(v);
                    }
                    if id == "map" {
                        out.push(result)
                    } else if id == "filter" && self.truthy(&result) {
                        out.push(v)
                    }
                }
                if id == "find" {
                    Ok(N::U)
                } else {
                    let result = arr(out);
                    if id == "filter" {
                        self.filter_results.push(result.clone());
                    }
                    Ok(result)
                }
            }
            "push" => {
                let N::A(values) = r else {
                    panic!("push receiver")
                };
                values.borrow_mut().extend_from_slice(args);
                Ok(N::Num(values.borrow().len() as f64))
            }
            "Array.isArray" => {
                assert!(same(r, &N::Builtin("Array".into())));
                assert_eq!(args.len(), 1);
                Ok(N::B(matches!(a, N::A(_))))
            }
            "Number" | "NumberZero" | "parseFloat" | "parseFloatDouble" => {
                assert!(matches!(r, N::U), "BOOK bare numeric callee receiver");
                assert_eq!(args.len(), 1, "BOOK numeric callee arity");
                let n = if id.starts_with("parseFloat") {
                    safe_cex_num::parse_float(&String::from_utf16(&text(&a).0).unwrap())
                } else if id == "NumberZero" {
                    0.
                } else {
                    safe_cex_num::number_from_string(&String::from_utf16(&text(&a).0).unwrap())
                };
                Ok(N::Num(if id == "parseFloatDouble" { n * 2. } else { n }))
            }
            "Number.isFinite" | "NumberZero.isFinite" => {
                let expected = id.split('.').next().unwrap();
                assert!(
                    same(r, &N::Builtin(expected.into())),
                    "BOOK finite callee receiver"
                );
                assert_eq!(args.len(), 1);
                Ok(N::B(matches!(a,N::Num(n)if n.is_finite())))
            }
            "nanoid" => {
                assert!(matches!(r, N::U));
                assert!(args.is_empty());
                Ok(s("inner"))
            }
            "subscribe" | "unsubscribe" => {
                assert!(same(r, &self.socket));
                assert_eq!(args.len(), 1);
                let N::O(o) = &a else {
                    panic!("BOOK native payload")
                };
                assert_eq!(
                    o.borrow()
                        .fields
                        .iter()
                        .map(|p| p.0.as_str())
                        .collect::<Vec<_>>(),
                    ["type", "coin"],
                    "BOOK exact subscription keys"
                );
                assert!(
                    same(&own(&a, "coin"), &self.id),
                    "BOOK raw subscription coin"
                );
                self.sends.push((id == "unsubscribe", a));
                if self.name == "send-error" {
                    return Err(self.error.clone());
                }
                Ok(object(vec![("ignored", N::B(true))]))
            }
            "log" | "emit-error" => {
                assert!(same(r, &self.parent));
                assert_eq!(args.len(), if id == "log" { 1 } else { 2 });
                if id == "emit-error" {
                    assert!(same(&args[1], &s("listenOrderBook")))
                }
                self.logs.push(a);
                Ok(N::U)
            }
            "calcOrderBookTotal" => {
                assert!(matches!(r, N::U));
                assert_eq!(args.len(), 1);
                self.total_inputs.push(a.clone());
                self.pre_total.push(deep(&a));
                for side in ["bids", "asks"] {
                    let N::A(levels) = own(&a, side) else {
                        panic!("BOOK total side")
                    };
                    let levels = levels.borrow().clone();
                    let mut total = 0.;
                    for level in levels {
                        let N::Num(amount) = own(&level, "amount") else {
                            panic!("BOOK total amount")
                        };
                        total += amount;
                        self.set(&level, "total", &N::Num(total))?;
                    }
                }
                Ok(N::U)
            }
            "output" => {
                assert!(matches!(r, N::U));
                assert_eq!(args.len(), 1);
                self.books.push(a);
                if self.name == "callback-error" {
                    Err(self.error.clone())
                } else {
                    Ok(object(vec![("ignoredThenable", N::B(true))]))
                }
            }
            "setTimeout" => {
                assert!(matches!(r, N::U));
                assert_eq!(args.len(), 2);
                assert!(same(&args[1], &N::Num(100.)), "BOOK independent poll delay");
                let id = N::Num(if self.name == "zero-handle" {
                    0.
                } else {
                    42. * (self.timers.len() + 1) as f64
                });
                let first = self.timers.is_empty();
                self.timers.push((id.clone(), a.clone()));
                if self.name == "timer-reentry" && first {
                    self.invoke(&a, &[])?;
                }
                Ok(id)
            }
            "clearTimeout" => {
                assert!(matches!(r, N::U));
                assert_eq!(args.len(), 1);
                self.clear.push(a);
                Ok(N::U)
            }
            _ => panic!("BOOK unsupported callee {id}"),
        }
    }
}
pub fn deep(v: &N) -> N {
    match v {
        N::A(a) => arr(a.borrow().iter().map(deep).collect()),
        N::O(o) => {
            let out = object(vec![]);
            let N::O(n) = &out else { unreachable!() };
            n.borrow_mut().fields = o
                .borrow()
                .fields
                .iter()
                .map(|(k, v, e)| (k.clone(), deep(v), *e))
                .collect();
            out
        }
        _ => v.clone(),
    }
}
impl CandleListenerHost for Host {
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        same(a, b)
    }
    fn binary(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        if op == "+" && matches!(a, N::S(_)) {
            let mut t = text(a).0;
            t.extend(text(b).0);
            return Ok(N::S(JsString(t)));
        }
        if let (N::S(a), N::S(b)) = (a, b) {
            return Ok(N::B(match op {
                "<" => a.0 < b.0,
                ">" => a.0 > b.0,
                ">=" => a.0 >= b.0,
                _ => panic!("string op"),
            }));
        }
        let a = cache::num(a);
        let b = cache::num(b);
        Ok(match op {
            "+" => N::Num(a + b),
            "<" => N::B(a < b),
            ">" => N::B(a > b),
            ">=" => N::B(a >= b),
            _ => panic!("binary"),
        })
    }
    fn decrement(&mut self, v: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(v) - 1.))
    }
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        Ok(text(v))
    }
    fn index(&mut self, o: &N, k: &N) -> Result<N, N> {
        let key = text(k);
        if self.tables.iter().any(|t| same(&t.value, o)) {
            Ok(self.slot(o, &key))
        } else {
            self.get(o, &String::from_utf16(&key.0).unwrap())
        }
    }
    fn assign_index(&mut self, o: &N, k: &N, v: &N) -> Result<(), N> {
        if self.tables.iter().any(|t| same(&t.value, o)) {
            self.slot_set(o, text(k), v.clone());
            Ok(())
        } else {
            self.set(o, &String::from_utf16(&text(k).0).unwrap(), v)
        }
    }
    fn array(&mut self, v: &[N]) -> Result<N, N> {
        Ok(arr(v.to_vec()))
    }
    fn spread_append(&mut self, v: &N, last: &N) -> Result<N, N> {
        let N::A(a) = v else { panic!("spread") };
        let mut a = a.borrow().clone();
        a.push(last.clone());
        Ok(arr(a))
    }
    fn spread_array(&mut self, v: &N) -> Result<N, N> {
        let N::A(a) = v else { panic!("spread") };
        Ok(arr(a.borrow().clone()))
    }
    fn candle_callback(&mut self, _: CandleCallback<N>) -> Result<N, N> {
        panic!("unrelated candle")
    }
}
impl CoreBookHost for Host {
    fn imported(&mut self, n: &str) -> Result<N, N> {
        Ok(N::Builtin(n.into()))
    }
    fn spread_object(&mut self, v: &N) -> Result<N, N> {
        let N::O(o) = v else { panic!("spread object") };
        let n = object(vec![]);
        for (k, v, e) in o.borrow().fields.clone() {
            if e {
                self.set(&n, &k, &v)?
            }
        }
        Ok(n)
    }
    fn delete_index(&mut self, o: &N, k: &N) -> Result<(), N> {
        let key = text(k);
        self.tables
            .iter_mut()
            .find(|t| same(&t.value, o))
            .unwrap()
            .entries
            .retain(|p| p.0 != key);
        Ok(())
    }
    fn construct(&mut self, f: &N, args: &[N]) -> Result<N, N> {
        assert!(same(f, &N::Builtin("Error".into())));
        assert_eq!(args.len(), 1);
        Ok(object(vec![("message", args[0].clone())]))
    }
    fn throw_value(&self, v: N) -> N {
        v
    }
    fn book_callback(&mut self, c: CoreBookCallback<N>) -> Result<N, N> {
        let id = format!("book:{}", self.next);
        self.next += 1;
        self.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
}
