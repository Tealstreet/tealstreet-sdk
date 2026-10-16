// Per-case expectation tables. Arms that currently coincide are kept separate
// because each names an independent case; collapsing them merges two
// expectations into one, which is the homogenized-corpus false green the
// cardinality audits in this tree exist to prevent.
#![allow(clippy::if_same_then_else)]
use safe_cex_venue_bitget::{
    order_placement::{self as body, *},
    rest_actions::*,
};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
#[derive(Clone, Debug)]
enum V {
    Hole,
    U,
    Null,
    Bool(bool),
    N(f64),
    S(Vec<u16>),
    A(Rc<RefCell<Vec<V>>>),
    O(Rc<RefCell<Vec<(Vec<u16>, V)>>>),
    Promise(Rc<Result<V, E>>),
}
fn s(x: &str) -> V {
    V::S(x.encode_utf16().collect())
}
fn a(v: Vec<V>) -> V {
    V::A(Rc::new(RefCell::new(v)))
}
fn o(fields: Vec<(&str, V)>) -> V {
    V::O(Rc::new(RefCell::new(
        fields
            .into_iter()
            .map(|(k, v)| (k.encode_utf16().collect(), v))
            .collect(),
    )))
}
fn text(v: &V) -> String {
    match v {
        V::Hole | V::U => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(b) => b.to_string(),
        V::N(n) => n.to_string(),
        V::S(s) => String::from_utf16_lossy(s),
        V::A(_) => "".into(),
        V::O(_) => "[object Object]".into(),
        V::Promise(_) => "[object Promise]".into(),
    }
}
fn get(v: &V, k: &str) -> V {
    match v {
        V::O(v) => v
            .borrow()
            .iter()
            .find(|(key, _)| key == &k.encode_utf16().collect::<Vec<_>>())
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U),
        _ => V::U,
    }
}
fn truth(v: &V) -> bool {
    match v {
        V::Hole | V::U | V::Null => false,
        V::Bool(b) => *b,
        V::N(n) => *n != 0. && !n.is_nan(),
        V::S(t) => !t.is_empty(),
        _ => true,
    }
}
fn equal(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::U, V::U) | (V::Null, V::Null) => true,
        (V::S(a), V::S(b)) => a == b,
        (V::N(a), V::N(b)) => a == b,
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::A(a), V::A(b)) => Rc::ptr_eq(a, b),
        (V::O(a), V::O(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn values(v: &V) -> Vec<V> {
    match v {
        V::A(v) => v.borrow().clone(),
        _ => panic!("PLACE native array required"),
    }
}
fn native(actual: &V, expected: &V, path: &str) {
    match (actual, expected) {
        (V::N(x), V::N(y)) => assert!(
            if y.is_nan() {
                x.is_nan()
            } else {
                x.to_bits() == y.to_bits()
            },
            "PLACE native numeric {path}: {x} != {y}"
        ),
        (V::S(x), V::S(y)) => assert_eq!(x, y, "PLACE native UTF16 {path}"),
        (V::A(x), V::A(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "PLACE native independent array count {path}"
            );
            for (i, (x, y)) in x.iter().zip(y.iter()).enumerate() {
                native(x, y, &format!("{path}.{i}"))
            }
        }
        (V::O(x), V::O(y)) => {
            let x = x.borrow();
            let y = y.borrow();
            assert_eq!(
                x.len(),
                y.len(),
                "PLACE native independent own field count {path}"
            );
            for ((kx, x), (ky, y)) in x.iter().zip(y.iter()) {
                assert_eq!(kx, ky, "PLACE native own field order {path}");
                native(x, y, &format!("{path}.{}", String::from_utf16_lossy(kx)))
            }
        }
        _ => assert!(
            equal(actual, expected),
            "PLACE native scalar {path}: {actual:?} != {expected:?}"
        ),
    }
}
fn wire(v: &V) -> Json {
    match v {
        V::Hole => json!({"hole":true}),
        V::U => json!({"u":true}),
        V::Null => json!({"null":true}),
        V::Bool(b) => json!({"b":b}),
        V::N(n) => json!({"n":n.to_le_bytes()}),
        V::S(t) => json!({"s":t}),
        V::A(a) => json!({"a":a.borrow().iter().map(wire).collect::<Vec<_>>()}),
        V::Promise(_) => json!({"promise":true}),
        V::O(_) => {
            json!({"o":keys(v).into_iter().map(|k|json!([k,wire(&key(v,&k))])).collect::<Vec<_>>()})
        }
    }
}
fn run<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = Box::pin(f);
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(r) => r,
        Poll::Pending => panic!("settled fixture futures"),
    }
}

#[derive(Clone, Debug)]
struct E {
    identity: Rc<()>,
    value: V,
}
fn put(v: &V, key: Vec<u16>, field: V) {
    let V::O(v) = v else { panic!("native object") };
    let mut v = v.borrow_mut();
    if let Some((_, f)) = v.iter_mut().find(|(k, _)| *k == key) {
        *f = field
    } else {
        v.push((key, field))
    }
}
fn keys(v: &V) -> Vec<Vec<u16>> {
    let V::O(v) = v else {
        panic!("native keys object")
    };
    let mut keys = v
        .borrow()
        .iter()
        .map(|(k, _)| k.clone())
        .collect::<Vec<_>>();
    keys.sort_by_key(|k| {
        let t = String::from_utf16_lossy(k);
        match t.parse::<u32>() {
            Ok(n) if n != u32::MAX && n.to_string() == t => (0, n),
            _ => (1, 0),
        }
    });
    keys
}
fn key(v: &V, k: &[u16]) -> V {
    let V::O(v) = v else { return V::U };
    v.borrow()
        .iter()
        .find(|(key, _)| key == k)
        .map(|(_, v)| v.clone())
        .unwrap_or(V::U)
}
fn compare(a: &Json, b: &Json, path: &str) {
    if a == b {
        return;
    }
    match (a, b) {
        (Json::Array(a), Json::Array(b)) => {
            assert_eq!(a.len(), b.len(), "PLACE source array size {path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}.{i}"))
            }
        }
        (Json::Object(a), Json::Object(b)) => {
            assert_eq!(a.len(), b.len(), "PLACE source own field size {path}");
            for (k, a) in a {
                compare(
                    a,
                    b.get(k).expect("PLACE source field missing"),
                    &format!("{path}.{k}"),
                )
            }
        }
        _ => assert_eq!(a, b, "PLACE source value {path}"),
    }
}
#[derive(Clone, Debug)]
struct Event {
    name: String,
    args: Vec<V>,
}
struct H {
    op: &'static str,
    scene: &'static str,
    data: V,
    opts: V,
    events: RefCell<Vec<Event>>,
    original: E,
    at: RefCell<usize>,
    predicate: V,
    formatted: RefCell<Vec<V>>,
}
fn order(id: &str, kind: &str) -> V {
    o(vec![
        ("id", s(id)),
        ("type", s(kind)),
        ("symbol", s("BTCUSD")),
        ("clientOid", s(id)),
    ])
}
fn payload(id: &str, symbol: &str, amount: f64) -> V {
    o(vec![
        ("clientOid", s(id)),
        ("symbol", s(symbol)),
        ("size", V::N(amount)),
        ("qty", V::N(amount)),
        (
            "planType",
            s(if id == "p0" {
                "track_plan"
            } else {
                "normal_plan"
            }),
        ),
    ])
}
fn make(op: &'static str, scene: &'static str) -> H {
    let data = if op == "single" {
        order(
            "one",
            if scene == "uta-trailing" {
                "trailing_stop_market"
            } else if scene == "algo" || scene == "uta-algo" {
                "stop_market"
            } else {
                "limit"
            },
        )
    } else if op == "bulk" {
        if scene == "empty" {
            a(vec![])
        } else {
            a(vec![
                order("n0", "limit"),
                order(
                    "a0",
                    if scene == "trailing" {
                        "trailing_stop_market"
                    } else {
                        "stop_market"
                    },
                ),
                order("n1", "limit"),
            ])
        }
    } else if op.contains("batch") {
        if scene == "empty" {
            a(vec![])
        } else {
            let mut v = (0..51)
                .map(|i| payload(&format!("b{i}"), "BTCUSD", i as f64 + 1.))
                .collect::<Vec<_>>();
            v.push(payload("eth", "ETHUSD", 4.));
            a(v)
        }
    } else if op == "cancel" {
        if scene == "empty" {
            a(vec![])
        } else {
            let one = o(vec![
                ("id", s("one")),
                (
                    "info",
                    o(vec![
                        ("orderId", s("remote-one")),
                        ("clientOid", s("client-one")),
                    ]),
                ),
            ]);
            a(vec![
                one.clone(),
                one,
                o(vec![
                    ("id", s("client-two")),
                    ("info", o(vec![("clientOid", s("client-two"))])),
                ]),
                o(vec![("id", s("third")), ("info", V::Null)]),
            ])
        }
    } else if scene == "empty" {
        a(vec![])
    } else {
        a(vec![
            payload("p0", "BTCUSD", 2.),
            payload("p1", "BTCUSD", 3.),
        ])
    };
    if scene == "no-id" && (op == "uta" || op == "strategy") {
        for p in values(&data) {
            put(&p, "clientOid".encode_utf16().collect(), V::U);
        }
    }
    let absent = scene == "throw-absent" || scene == "strict-throw";
    let original = E {
        identity: Rc::new(()),
        value: o(vec![
            ("message", s("original")),
            (
                "response",
                o(vec![(
                    "data",
                    o(vec![(
                        "msg",
                        s(if absent {
                            "corresponding strategy id could not be found"
                        } else {
                            "api original"
                        }),
                    )]),
                )]),
            ),
        ]),
    };
    H {
        op,
        scene,
        data,
        opts: if scene == "null-opts" {
            V::Null
        } else if scene.starts_with("strict-") {
            o(vec![("treatNotFoundAsSuccess", V::Bool(false))])
        } else {
            V::U
        },
        events: RefCell::new(vec![]),
        original,
        at: RefCell::new(0),
        predicate: method("isAlgoOrder", V::U),
        formatted: RefCell::new(vec![]),
    }
}
fn method(path: &str, owner: V) -> V {
    o(vec![("path", s(path)), ("owner", owner)])
}
fn num(v: &V) -> f64 {
    match v {
        V::N(n) => *n,
        V::Null => 0.,
        V::U => f64::NAN,
        V::Bool(b) => {
            if *b {
                1.
            } else {
                0.
            }
        }
        _ => text(v).parse().unwrap_or(f64::NAN),
    }
}
fn promise(r: Result<V, E>) -> V {
    V::Promise(Rc::new(r))
}
fn unwrap_promise(v: V) -> Result<V, E> {
    match v {
        V::Promise(p) => p.as_ref().clone(),
        v => Ok(v),
    }
}
fn units(v: &V) -> Vec<u16> {
    match v {
        V::S(s) => s.clone(),
        _ => text(v).encode_utf16().collect(),
    }
}
fn omit_symbol(v: &V) -> V {
    o(keys(v)
        .iter()
        .filter(|k| **k != units(&s("symbol")))
        .map(|k| (String::from_utf16_lossy(k), key(v, k)))
        .collect::<Vec<_>>()
        .iter()
        .map(|(k, v)| (k.as_str(), v.clone()))
        .collect())
}
impl H {
    fn record(&self, name: &str, args: Vec<V>) {
        self.events.borrow_mut().push(Event {
            name: name.into(),
            args,
        })
    }
    fn uta(&self) -> bool {
        ["uta", "uta-algo", "uta-trailing", "trailing"].contains(&self.scene)
    }
    fn response(&self, index: usize) -> V {
        if self.op.contains("batch") {
            let ids = if self.scene == "no-ids" {
                a(vec![])
            } else if self.scene == "missing-methods" {
                o(vec![("map", V::U)])
            } else if self.scene == "holes" {
                a(vec![
                    V::Hole,
                    o(vec![("orderId", s(&format!("batch-{index}")))]),
                ])
            } else {
                a(vec![o(vec![("orderId", s(&format!("batch-{index}")))])])
            };
            return o(vec![(
                "data",
                o(vec![(
                    "data",
                    o(vec![
                        ("successList", ids),
                        (
                            "failureList",
                            if self.scene == "failure" {
                                a(vec![o(vec![("errorMsg", s("lot failure"))])])
                            } else if self.scene == "missing-methods" {
                                o(vec![("forEach", V::U)])
                            } else {
                                a(vec![])
                            },
                        ),
                    ]),
                )]),
            )]);
        }
        let absent = self.op == "cancel" && ["absent", "strict-absent"].contains(&self.scene);
        let code = if self.scene == "api-error" || absent {
            "400"
        } else {
            "00000"
        };
        let msg = if absent {
            "corresponding strategy id could not be found"
        } else if self.scene == "api-error" && (self.op == "uta" || self.op == "strategy") {
            ""
        } else {
            "api failure"
        };
        let inner = if self.scene == "client-fallback" {
            o(vec![
                ("orderId", s("")),
                ("clientOid", s(&format!("server-client-{index}"))),
            ])
        } else if self.scene == "payload-fallback" {
            V::U
        } else if self.scene == "no-id" {
            o(vec![("orderId", V::N(0.)), ("clientOid", s(""))])
        } else {
            o(vec![("orderId", s(&format!("server-{index}")))])
        };
        o(vec![(
            "data",
            o(vec![("code", s(code)), ("msg", s(msg)), ("data", inner)]),
        )])
    }
}
impl RestActionsHost for H {
    type Value = V;
    type Error = E;
    fn receiver(&self, path: &str) -> Result<V, E> {
        Ok(match path {
            "usesUtaRouting" => V::Bool(self.uta()),
            "isAlgoOrder" => self.predicate.clone(),
            "apiProductType" => s("umcbl"),
            "apiMarginCoin" => s("USDT"),
            "OrderType.TrailingStopLoss" => s("trailing_stop_market"),
            "LogSeverity.Debug" => s("debug"),
            _ if path.starts_with("ENDPOINTS.") => s(&path[10..]),
            _ => panic!("PLACE unknown receiver {path}"),
        })
    }
    fn set_receiver(&self, _p: &str, _v: V) -> Result<(), E> {
        panic!("PLACE unexpected receiver write")
    }
    fn capture(&self, path: &str) -> Result<V, E> {
        if ![
            "groupBy",
            "chunk",
            "Object.entries",
            "omit",
            "omitUndefined",
            "uniqBy",
            "partition",
            "parseInt",
            "isBitgetSuccessCode",
            "getBitgetApiErrorMessage",
            "isBitgetUtaStrategyOrderNotFound",
            "Error",
        ]
        .contains(&path)
        {
            self.record(&format!("capture:{path}"), vec![])
        }
        Ok(method(path, V::U))
    }
    fn capture_member(&self, v: &V, name: &str) -> Result<V, E> {
        Ok(method(name, v.clone()))
    }
    fn invoke(&self, f: V, args: Vec<V>) -> Result<V, E> {
        let path = text(&get(&f, "path"));
        let owner = get(&f, "owner");
        let scalar = [
            "groupBy",
            "chunk",
            "Object.entries",
            "omit",
            "omitUndefined",
            "uniqBy",
            "partition",
            "parseInt",
            "isBitgetSuccessCode",
            "getBitgetApiErrorMessage",
            "isBitgetUtaStrategyOrderNotFound",
            "Error",
            "push",
            "slice",
            "flat",
        ];
        if path == "Promise.all" {
            self.record("promise:all", vec![])
        } else if !scalar.contains(&path.as_str()) {
            self.record(&format!("call:{path}"), args.clone())
        }
        if path == "resolveCallerClientOrderId" && self.scene == "resolver-error"
            || path == "assertCallerClientOrderIds" && self.scene == "assert-error"
        {
            return Err(self.original.clone());
        }
        Ok(match path.as_str() {
            "Promise.all" => {
                let mut out = vec![];
                for v in values(&args[0]) {
                    match unwrap_promise(if matches!(v, V::Hole) { V::U } else { v }) {
                        Ok(v) => out.push(v),
                        Err(e) => return Ok(promise(Err(e))),
                    }
                }
                promise(Ok(a(out)))
            }
            "partition" => {
                assert!(
                    equal(&args[1], &self.predicate),
                    "PLACE native original partition predicate"
                );
                let mut algo = vec![];
                let mut normal = vec![];
                for v in values(&args[0]) {
                    if truth(&self.invoke(self.predicate.clone(), vec![v.clone()])?) {
                        algo.push(v)
                    } else {
                        normal.push(v)
                    }
                }
                a(vec![a(algo), a(normal)])
            }
            "groupBy" => {
                let out = o(vec![]);
                for v in values(&args[0]) {
                    let k = get(&v, "symbol");
                    let mut list = get(&out, &text(&k));
                    if !truth(&list) {
                        list = a(vec![]);
                        put(&out, units(&k), list.clone());
                    }
                    let V::A(list) = list else {
                        panic!("native group array")
                    };
                    list.borrow_mut().push(v);
                }
                out
            }
            "Object.entries" => a(keys(&args[0])
                .into_iter()
                .map(|k| a(vec![V::S(k.clone()), key(&args[0], &k)]))
                .collect()),
            "chunk" => a(values(&args[0])
                .chunks(num(&args[1]) as usize)
                .map(|c| a(c.to_vec()))
                .collect()),
            "uniqBy" => {
                let mut out = vec![];
                let mut seen = vec![];
                for v in values(&args[0]) {
                    let id = get(&v, "id");
                    if !seen.iter().any(|k| equal(k, &id)) {
                        seen.push(id);
                        out.push(v)
                    }
                }
                a(out)
            }
            "omit" => omit_symbol(&args[0]),
            "omitUndefined" => o(keys(&args[0])
                .iter()
                .map(|k| (String::from_utf16_lossy(k), key(&args[0], k)))
                .filter(|(_, v)| !matches!(v, V::U))
                .collect::<Vec<_>>()
                .iter()
                .map(|(k, v)| (k.as_str(), v.clone()))
                .collect()),
            "parseInt" => V::N(safe_cex_num::parse_int(&text(&args[0]), None)),
            "isBitgetSuccessCode" => {
                V::Bool(safe_cex_num::parse_int(&text(&args[0]), Some(10)) == 0.)
            }
            "getBitgetApiErrorMessage" => {
                let msg = get(&args[0], "msg");
                if truth(&msg) { msg } else { args[1].clone() }
            }
            "isBitgetUtaStrategyOrderNotFound" => {
                V::Bool(text(&args[0]) == "corresponding strategy id could not be found")
            }
            "push" => {
                let V::A(v) = owner else {
                    return Err(self.original.clone());
                };
                v.borrow_mut().extend(args);
                let len = v.borrow().len();
                V::N(len as f64)
            }
            "slice" => a(
                values(&owner)[(num(&args[0]) as usize).min(values(&owner).len())
                    ..(num(&args[1]) as usize).min(values(&owner).len())]
                    .to_vec(),
            ),
            "flat" => {
                let mut out = vec![];
                for v in values(&owner) {
                    if let V::A(c) = v {
                        for v in c.borrow().iter() {
                            if !matches!(v, V::Hole) {
                                out.push(v.clone())
                            }
                        }
                    } else if !matches!(v, V::Hole) {
                        out.push(v)
                    }
                }
                a(out)
            }
            "ensureAccountTypeDetected" => promise(if self.scene == "ensure-error" {
                Err(self.original.clone())
            } else {
                Ok(V::U)
            }),
            "isAlgoOrder" => V::Bool(text(&get(&args[0], "type")) != "limit"),
            "toFiniteNumber" => V::N(num(&args[0])),
            "formatCreateOrder" | "formatUtaCreateOrder" => {
                if self.scene == "format-reject" {
                    promise(Err(self.original.clone()))
                } else {
                    let id = text(&get(&args[0], "id"));
                    let p = payload(&id, "BTCUSD", if id == "n1" { 0. } else { 2. });
                    self.formatted.borrow_mut().push(p.clone());
                    promise(Ok(a(vec![p])))
                }
            }
            "formatAlgoOrder" | "formatUtaStrategyOrder" => {
                let p = payload(&text(&get(&args[0], "id")), "BTCUSD", 1.);
                self.formatted.borrow_mut().push(p.clone());
                promise(Ok(p))
            }
            "placeOrderBatch" | "placeUtaOrders" => promise(if self.scene == "place-reject" {
                Err(self.original.clone())
            } else if self.scene == "first-iterator-error" {
                Ok(V::N(12.))
            } else {
                Ok(a(if values(&args[0]).is_empty() {
                    vec![]
                } else {
                    vec![s(if path == "placeOrderBatch" {
                        "normal-id"
                    } else {
                        "uta-id"
                    })]
                }))
            }),
            "placeAlgoOrders" | "placeUtaStrategyOrders" => {
                promise(Ok(a(if values(&args[0]).is_empty() {
                    vec![]
                } else {
                    vec![s(if path == "placeAlgoOrders" {
                        "algo-id"
                    } else {
                        "strategy-id"
                    })]
                })))
            }
            "getMarginCoinForSymbol" => s(if text(&args[0]) == "BTCUSD" {
                "BTC"
            } else {
                "ETH"
            }),
            "getOrFetchMarginType" => promise(if self.scene == "margin-reject" {
                Err(self.original.clone())
            } else {
                Ok(s("crossed"))
            }),
            "unlimitedXHR.post" => {
                let index = *self.at.borrow();
                *self.at.borrow_mut() += 1;
                promise(
                    if self.scene == "reject"
                        || self.scene == "throw-absent"
                        || self.scene == "strict-throw"
                    {
                        Err(self.original.clone())
                    } else {
                        Ok(self.response(index))
                    },
                )
            }
            _ => V::U,
        })
    }
    fn start(&self, f: V, args: Vec<V>) -> Result<RestActionFuture<'_, V, E>, E> {
        let r = self.invoke(f, args).and_then(unwrap_promise);
        Ok(Box::pin(async move { r }))
    }
    fn property(&self, v: &V, k: &str) -> Result<V, E> {
        if matches!(v, V::Null | V::U) {
            return Err(E {
                identity: Rc::new(()),
                value: o(vec![(
                    "message",
                    s(&format!(
                        "Cannot read properties of {} (reading '{k}')",
                        text(v)
                    )),
                )]),
            });
        }
        if let V::A(arr) = v {
            if k == "length" {
                return Ok(V::N(arr.borrow().len() as f64));
            }
            if k == "map" || k == "forEach" {
                return Ok(method(k, V::U));
            }
        }
        Ok(get(v, k))
    }
    fn key_get(&self, v: &V, k: &V) -> Result<V, E> {
        Ok(match v {
            V::A(v) => v.borrow().get(num(k) as usize).cloned().unwrap_or(V::U),
            _ => get(v, &text(k)),
        })
    }
    fn has(&self, v: &V, k: &str) -> Result<bool, E> {
        Ok(keys(v).contains(&units(&s(k))))
    }
    fn assign(&self, v: &V, k: &str, value: V) -> Result<(), E> {
        put(v, units(&s(k)), value);
        Ok(())
    }
    fn define(&self, v: &V, k: &str, value: V) -> Result<(), E> {
        put(v, units(&s(k)), value);
        Ok(())
    }
    fn delete(&self, _v: &V, _k: &V) -> Result<(), E> {
        panic!("PLACE unexpected delete")
    }
    fn truthy(&self, v: &V) -> bool {
        truth(v)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::Null | V::U)
    }
    fn equal(&self, a: &V, b: &V) -> bool {
        equal(a, b)
    }
    fn number(&self, n: f64) -> V {
        V::N(n)
    }
    fn boolean(&self, b: bool) -> V {
        V::Bool(b)
    }
    fn string(&self, s: &[u16]) -> V {
        V::S(s.to_vec())
    }
    fn undefined(&self) -> V {
        V::U
    }
    fn null(&self) -> V {
        V::Null
    }
    fn array(&self, v: Vec<V>) -> V {
        a(v)
    }
    fn object(&self, f: Vec<(&str, V)>) -> V {
        o(f)
    }
    fn filter<F>(&self, m: V, array: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        binding(&m, &array, "filter");
        let mut out = vec![];
        for v in values(&array) {
            if !matches!(v, V::Hole) && f(v.clone())? {
                out.push(v)
            }
        }
        Ok(a(out))
    }
    fn find<F>(&self, m: V, array: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        binding(&m, &array, "find");
        for v in values(&array) {
            if f(v.clone())? {
                return Ok(v);
            }
        }
        Ok(V::U)
    }
    fn some<F>(&self, m: V, array: V, mut f: F) -> Result<bool, E>
    where
        F: FnMut(V) -> Result<bool, E>,
    {
        binding(&m, &array, "some");
        for v in values(&array) {
            if !matches!(v, V::Hole) && f(v)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
    fn operation(&self, name: &str, args: Vec<V>) -> Result<V, E> {
        Ok(match name {
            "greater" => V::Bool(num(&args[0]) > num(&args[1])),
            "less" => V::Bool(num(&args[0]) < num(&args[1])),
            "add" => V::N(num(&args[0]) + num(&args[1])),
            "restOmitSymbol" => omit_symbol(&args[0]),
            "absentMessage" => s(&format!(
                "Bitget UTA strategy order {} was already absent during cancel",
                text(&args[0])
            )),
            _ => panic!("PLACE unknown op {name}"),
        })
    }
    fn new_error(&self, _ctor: V, msg: V) -> Result<E, E> {
        Ok(E {
            identity: Rc::new(()),
            value: o(vec![("message", msg)]),
        })
    }
    fn error_value(&self, e: &E) -> V {
        e.value.clone()
    }
}
struct Iter {
    array: V,
    index: usize,
}
impl OrderPlacementHost for H {
    type Iterator = Iter;
    fn spread_into(&self, array: &V, elements: V) -> Result<(), E> {
        let mut i = self.iterator(&elements)?;
        let V::A(out) = array else {
            panic!("native array literal")
        };
        while let Some(v) = self.next(&mut i)? {
            out.borrow_mut().push(v)
        }
        Ok(())
    }
    fn iterator(&self, value: &V) -> Result<Iter, E> {
        if !matches!(value, V::A(_)) {
            return Err(E {
                identity: Rc::new(()),
                value: o(vec![("message", s("(intermediate value) is not iterable"))]),
            });
        }
        Ok(Iter {
            array: value.clone(),
            index: 0,
        })
    }
    fn next(&self, i: &mut Iter) -> Result<Option<V>, E> {
        let V::A(v) = &i.array else {
            panic!("native iterator")
        };
        let v = v.borrow();
        let out = v.get(i.index).cloned();
        i.index += 1;
        Ok(out.map(|v| if matches!(v, V::Hole) { V::U } else { v }))
    }
    fn pair(&self, v: V) -> Result<(V, V), E> {
        let values = values(&v);
        Ok((
            values.first().cloned().unwrap_or(V::U),
            values.get(1).cloned().unwrap_or(V::U),
        ))
    }
    fn map<F>(&self, m: V, array: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V) -> Result<V, E>,
    {
        binding(&m, &array, "map");
        let mut out = vec![];
        for v in values(&array) {
            out.push(if matches!(v, V::Hole) { V::Hole } else { f(v)? })
        }
        Ok(a(out))
    }
    fn reduce<F>(&self, m: V, array: V, mut initial: V, mut f: F) -> Result<V, E>
    where
        F: FnMut(V, V) -> Result<V, E>,
    {
        binding(&m, &array, "reduce");
        for v in values(&array) {
            if !matches!(v, V::Hole) {
                initial = f(initial, v)?
            }
        }
        Ok(initial)
    }
    fn for_each<F>(&self, m: V, array: V, mut f: F) -> Result<(), E>
    where
        F: FnMut(V) -> Result<(), E>,
    {
        binding(&m, &array, "forEach");
        for v in values(&array) {
            if !matches!(v, V::Hole) {
                f(v)?
            }
        }
        Ok(())
    }
    fn assign_key(&self, v: &V, k: &V, value: V) -> Result<(), E> {
        put(v, units(k), value);
        Ok(())
    }
}
fn execute(h: &H) -> Result<V, E> {
    run(async {
        match h.op {
            "single" => body::place_order(h, h.data.clone()).await,
            "bulk" => body::place_orders(h, h.data.clone()).await,
            "batch" => body::place_order_batch(h, h.data.clone()).await,
            "inverse-batch" => body::inverse_place_order_batch(h, h.data.clone()).await,
            "uta" => body::place_uta_orders(h, h.data.clone()).await,
            "strategy" => body::place_uta_strategy_orders(h, h.data.clone()).await,
            "algo" => body::place_algo_orders(h, h.data.clone()).await,
            "cancel" => body::cancel_uta_strategy_orders(h, h.data.clone(), h.opts.clone()).await,
            _ => unreachable!(),
        }
    })
}
fn events(h: &H, name: &str) -> Vec<Event> {
    h.events
        .borrow()
        .iter()
        .filter(|e| e.name == name)
        .cloned()
        .collect()
}
fn count(h: &H, name: &str, n: usize) {
    assert_eq!(
        events(h, name).len(),
        n,
        "PLACE native independent event count {}:{}:{name}",
        h.op,
        h.scene
    )
}
fn success(r: &Result<V, E>, v: V) {
    native(
        r.as_ref().expect("PLACE expected native success"),
        &v,
        "returned value",
    )
}
fn failure(h: &H, r: &Result<V, E>, original: bool, message: &str) {
    let e = r.as_ref().expect_err("PLACE expected native error");
    assert_eq!(
        Rc::ptr_eq(&e.identity, &h.original.identity),
        original,
        "PLACE native original error identity"
    );
    native(&get(&e.value, "message"), &s(message), "Error message")
}
fn args(h: &H, path: &str) -> Vec<Vec<V>> {
    events(h, &format!("call:{path}"))
        .into_iter()
        .map(|e| e.args)
        .collect()
}
fn alias(a: &V, b: &V, why: &str) {
    assert!(equal(a, b), "PLACE native original reference {why}")
}
fn check(h: &H, r: &Result<V, E>) {
    match h.op {
        "single" => {
            let err = [
                "resolver-error",
                "ensure-error",
                "format-reject",
                "uta-trailing",
            ]
            .contains(&h.scene);
            if err {
                failure(
                    h,
                    r,
                    h.scene != "uta-trailing",
                    if h.scene == "uta-trailing" {
                        "Bitget UTA trailing-stop orders are not supported yet."
                    } else {
                        "original"
                    },
                )
            } else {
                success(
                    r,
                    a(vec![s(match h.scene {
                        "algo" => "algo-id",
                        "uta" => "uta-id",
                        "uta-algo" => "strategy-id",
                        _ => "normal-id",
                    })]),
                )
            }
            count(h, "call:resolveCallerClientOrderId", 1);
            alias(
                &args(h, "resolveCallerClientOrderId")[0][0],
                &h.data,
                "single resolver opts",
            );
            count(
                h,
                "call:ensureAccountTypeDetected",
                usize::from(h.scene != "resolver-error"),
            );
            count(
                h,
                "call:emitAndLogError",
                usize::from(h.scene == "uta-trailing"),
            );
            let format = match h.scene {
                "algo" => "formatAlgoOrder",
                "uta" => "formatUtaCreateOrder",
                "uta-algo" => "formatUtaStrategyOrder",
                _ => "formatCreateOrder",
            };
            let place = match h.scene {
                "algo" => "placeAlgoOrders",
                "uta" => "placeUtaOrders",
                "uta-algo" => "placeUtaStrategyOrders",
                _ => "placeOrderBatch",
            };
            let formatted = !matches!(h.scene, "resolver-error" | "ensure-error" | "uta-trailing");
            count(h, &format!("call:{format}"), usize::from(formatted));
            count(h, &format!("call:{place}"), usize::from(!err));
            if formatted {
                alias(&args(h, format)[0][0], &h.data, "single formatter opts")
            }
            if !err {
                let ps = values(&args(h, place)[0][0]);
                assert_eq!(ps.len(), 1, "PLACE native single placement payload count");
                native(
                    &ps[0],
                    &payload(
                        "one",
                        "BTCUSD",
                        if h.scene.contains("algo") { 1. } else { 2. },
                    ),
                    "single payload fields",
                );
                alias(&ps[0], &h.formatted.borrow()[0], "single formatted payload")
            }
        }
        "bulk" => {
            let early = matches!(h.scene, "assert-error" | "trailing" | "format-reject");
            let error = early || matches!(h.scene, "first-iterator-error" | "place-reject");
            if error {
                failure(
                    h,
                    r,
                    !matches!(h.scene, "trailing" | "first-iterator-error"),
                    if h.scene == "trailing" {
                        "Bitget UTA trailing-stop orders are not supported yet."
                    } else if h.scene == "first-iterator-error" {
                        "(intermediate value) is not iterable"
                    } else {
                        "original"
                    },
                )
            } else {
                success(
                    r,
                    if h.scene == "empty" {
                        a(vec![])
                    } else {
                        a(vec![
                            s(if h.uta() { "uta-id" } else { "normal-id" }),
                            s(if h.uta() { "strategy-id" } else { "algo-id" }),
                        ])
                    },
                )
            }
            count(h, "call:assertCallerClientOrderIds", 1);
            alias(
                &args(h, "assertCallerClientOrderIds")[0][0],
                &h.data,
                "bulk caller array",
            );
            count(
                h,
                "call:ensureAccountTypeDetected",
                usize::from(h.scene != "assert-error"),
            );
            count(
                h,
                "call:emitAndLogError",
                usize::from(h.scene == "trailing"),
            );
            count(
                h,
                "promise:all",
                if h.scene == "assert-error" || h.scene == "trailing" {
                    0
                } else if h.scene == "format-reject" {
                    1
                } else {
                    2
                },
            );
            let normal = if h.uta() {
                "placeUtaOrders"
            } else {
                "placeOrderBatch"
            };
            let algo = if h.uta() {
                "placeUtaStrategyOrders"
            } else {
                "placeAlgoOrders"
            };
            count(h, &format!("call:{normal}"), usize::from(!early));
            count(h, &format!("call:{algo}"), usize::from(!error));
            if !early {
                let list = values(&args(h, normal)[0][0]);
                assert_eq!(
                    list.len(),
                    usize::from(h.scene != "empty"),
                    "PLACE native independent filtered normal payload count"
                );
                if h.scene != "empty" {
                    native(
                        &list[0],
                        &payload("n0", "BTCUSD", 2.),
                        "filtered normal fields",
                    );
                    alias(
                        &list[0],
                        &h.formatted.borrow()[0],
                        "normal filter retains formatter object",
                    )
                }
            }
            if !error {
                let list = values(&args(h, algo)[0][0]);
                assert_eq!(
                    list.len(),
                    usize::from(h.scene != "empty"),
                    "PLACE native independent algo payload count"
                );
                if h.scene != "empty" {
                    native(&list[0], &payload("a0", "BTCUSD", 1.), "bulk algo fields");
                    alias(
                        &list[0],
                        &h.formatted.borrow()[2],
                        "algo retains formatted object",
                    )
                }
            }
        }
        "batch" | "inverse-batch" => {
            let inverse = h.op == "inverse-batch";
            let empty = h.scene == "empty";
            let error = matches!(h.scene, "failure" | "reject" | "margin-reject");
            let requests = if empty || h.scene == "margin-reject" {
                0
            } else if error {
                1
            } else {
                3
            };
            count(h, "call:unlimitedXHR.post", requests);
            count(
                h,
                "call:getOrFetchMarginType",
                if empty {
                    0
                } else if error {
                    1
                } else {
                    2
                },
            );
            count(
                h,
                "call:getMarginCoinForSymbol",
                if inverse {
                    if empty {
                        0
                    } else if error {
                        1
                    } else {
                        2
                    }
                } else {
                    0
                },
            );
            if error {
                failure(
                    h,
                    r,
                    h.scene != "failure",
                    if h.scene == "failure" {
                        "lot failure"
                    } else {
                        "original"
                    },
                )
            } else {
                let mut ids = vec![];
                if !empty && !matches!(h.scene, "no-ids" | "missing-methods") {
                    for i in 0..3 {
                        if h.scene == "holes" {
                            ids.push(V::U)
                        }
                        ids.push(s(&format!("batch-{i}")))
                    }
                }
                success(r, a(ids))
            }
            let scheduled =
                !inverse && !empty && !error && !matches!(h.scene, "no-ids" | "missing-methods");
            count(
                h,
                "call:schedulePrivateStreamWriteCheck",
                usize::from(scheduled),
            );
            count(
                h,
                "call:emitAndLogError",
                if h.scene == "failure" {
                    2
                } else if h.scene == "reject" {
                    1
                } else {
                    0
                },
            );
            if h.scene == "failure" {
                for e in args(h, "emitAndLogError") {
                    native(
                        &a(e),
                        &a(vec![s("lot failure")]),
                        "double emitted lot failure",
                    )
                }
            }
            for (i, call) in args(h, "unlimitedXHR.post").iter().enumerate() {
                assert_eq!(call.len(), 2, "PLACE native batch HTTP argument count");
                native(&call[0], &s("BATCH_ORDERS"), "batch endpoint selection");
                let (symbol, coin, n, start) = if i == 0 {
                    ("BTCUSD", if inverse { "BTC" } else { "USDT" }, 50, 0)
                } else if i == 1 {
                    ("BTCUSD", if inverse { "BTC" } else { "USDT" }, 1, 50)
                } else {
                    ("ETHUSD", if inverse { "ETH" } else { "USDT" }, 1, 51)
                };
                let mut rows = vec![];
                for at in start..start + n {
                    let id = if at == 51 {
                        "eth".into()
                    } else {
                        format!("b{at}")
                    };
                    let amount = if at == 51 { 4. } else { at as f64 + 1. };
                    rows.push(o(vec![
                        ("clientOid", s(&id)),
                        ("size", V::N(amount)),
                        ("qty", V::N(amount)),
                        ("planType", s("normal_plan")),
                    ]));
                }
                native(
                    &call[1],
                    &o(vec![
                        ("symbol", s(symbol)),
                        ("productType", s("umcbl")),
                        ("marginCoin", s(coin)),
                        ("marginMode", s("crossed")),
                        ("orderList", a(rows)),
                    ]),
                    "50-row lot fields and boundaries",
                );
            }
        }
        "uta" | "strategy" => {
            let error = matches!(h.scene, "api-error" | "reject");
            let empty = h.scene == "empty";
            let n = if empty {
                0
            } else if error {
                1
            } else {
                2
            };
            count(h, "call:unlimitedXHR.post", n);
            count(
                h,
                "call:schedulePrivateStreamWriteCheck",
                usize::from(!empty && !error && h.scene != "no-id"),
            );
            count(h, "call:emitAndLogError", usize::from(error));
            if error {
                failure(
                    h,
                    r,
                    h.scene == "reject",
                    if h.scene == "reject" {
                        "original"
                    } else if h.op == "strategy" {
                        "Bitget UTA place strategy order request failed"
                    } else {
                        "Bitget UTA place order request failed"
                    },
                )
            } else {
                success(
                    r,
                    if empty || h.scene == "no-id" {
                        a(vec![])
                    } else {
                        a((0..2)
                            .map(|i| {
                                s(&match h.scene {
                                    "client-fallback" => format!("server-client-{i}"),
                                    "payload-fallback" => format!("p{i}"),
                                    _ => format!("server-{i}"),
                                })
                            })
                            .collect())
                    },
                )
            }
            for (i, call) in args(h, "unlimitedXHR.post").iter().enumerate() {
                native(
                    &call[0],
                    &s(if h.op == "strategy" {
                        "UTA_PLACE_STRATEGY_ORDER"
                    } else {
                        "UTA_PLACE_ORDER"
                    }),
                    "UTA endpoint selection",
                );
                native(
                    &call[1],
                    &{
                        let p = payload(&format!("p{i}"), "BTCUSD", i as f64 + 2.);
                        if h.scene == "no-id" {
                            put(&p, "clientOid".encode_utf16().collect(), V::U);
                        }
                        p
                    },
                    "UTA original payload fields",
                );
                alias(&call[1], &values(&h.data)[i], "UTA passed original payload")
            }
            if error {
                assert_eq!(
                    args(h, "emitAndLogError")[0].len(),
                    1,
                    "PLACE native UTA emit argument count"
                )
            }
        }
        "algo" => {
            let empty = h.scene == "empty";
            let error = matches!(h.scene, "api-error" | "reject");
            let n = if empty {
                0
            } else if error {
                1
            } else {
                2
            };
            count(h, "call:unlimitedXHR.post", n);
            count(
                h,
                "call:schedulePrivateStreamWriteCheck",
                usize::from(!empty && !error && h.scene != "no-id"),
            );
            count(h, "call:emitAndLogError", usize::from(error));
            if error {
                failure(
                    h,
                    r,
                    h.scene == "reject",
                    if h.scene == "reject" {
                        "original"
                    } else {
                        "api failure"
                    },
                );
                let emit = args(h, "emitAndLogError");
                assert_eq!(emit[0].len(), 2, "PLACE native algo emit argument count");
                alias(
                    &emit[0][0],
                    &r.as_ref().unwrap_err().value,
                    "algo emitted Error object",
                );
                native(
                    &emit[0][1],
                    &s(if h.scene == "reject" {
                        "api original"
                    } else {
                        "api failure"
                    }),
                    "algo Error message extraction",
                )
            } else {
                success(
                    r,
                    if empty || h.scene == "no-id" {
                        a(vec![])
                    } else {
                        a(vec![s("server-0"), s("server-1")])
                    },
                )
            }
            for (i, call) in args(h, "unlimitedXHR.post").iter().enumerate() {
                native(
                    &call[0],
                    &s(if i == 0 {
                        "PLACE_TRAILING_STOP_ORDER"
                    } else {
                        "PLACE_ALGO_ORDER"
                    }),
                    "algo plan routing endpoint",
                );
                alias(&call[1], &values(&h.data)[i], "algo original payload")
            }
        }
        "cancel" => {
            let error = matches!(
                h.scene,
                "strict-absent" | "strict-throw" | "reject" | "null-opts"
            );
            let n = if h.scene == "empty" || h.scene == "null-opts" {
                0
            } else if error {
                1
            } else {
                3
            };
            count(h, "call:unlimitedXHR.post", n);
            count(
                h,
                "call:schedulePrivateStreamWriteCheck",
                usize::from(!error && h.scene != "empty"),
            );
            count(
                h,
                "call:log",
                if h.scene == "absent" || h.scene == "throw-absent" {
                    3
                } else {
                    0
                },
            );
            count(
                h,
                "call:emitAndLogError",
                usize::from(error && h.scene != "null-opts"),
            );
            if error {
                failure(
                    h,
                    r,
                    h.scene == "strict-throw" || h.scene == "reject",
                    if h.scene == "null-opts" {
                        "Cannot read properties of null (reading 'treatNotFoundAsSuccess')"
                    } else if h.scene == "reject" {
                        "original"
                    } else if h.scene == "strict-throw" {
                        "original"
                    } else {
                        "corresponding strategy id could not be found"
                    },
                )
            } else {
                success(r, V::U)
            }
            let payloads = [
                o(vec![
                    ("orderId", s("remote-one")),
                    ("clientOid", s("client-one")),
                ]),
                o(vec![("clientOid", s("client-two"))]),
                o(vec![("orderId", s("third"))]),
            ];
            for (i, call) in args(h, "unlimitedXHR.post").iter().enumerate() {
                native(
                    &call[0],
                    &s("UTA_CANCEL_STRATEGY_ORDER"),
                    "cancel endpoint selection",
                );
                native(&call[1], &payloads[i], "cancel dedup and IDs");
            }
            for (i, call) in args(h, "log").iter().enumerate() {
                native(
                    &a(call.clone()),
                    &a(vec![
                        s(&format!(
                            "Bitget UTA strategy order {} was already absent during cancel",
                            ["one", "client-two", "third"][i]
                        )),
                        s("debug"),
                    ]),
                    "absent log fields",
                )
            }
        }
        _ => unreachable!(),
    }
}
fn inputs(h: &H) -> Json {
    json!({"op":h.op,"scene":h.scene,"data":wire(&h.data),"opts":wire(&h.opts)})
}
fn output(h: &H, r: &Result<V, E>) -> Json {
    json!({"result":match r{Ok(v)=>json!({"ok":wire(v)}),Err(e)=>json!({"original":Rc::ptr_eq(&e.identity,&h.original.identity),"message":wire(&get(&e.value,"message"))})},"events":h.events.borrow().iter().map(|e|json!({"name":e.name,"args":e.args.iter().map(wire).collect::<Vec<_>>()})).collect::<Vec<_>>()})
}
fn source(cases: &[Json], mode: &str) -> Vec<Json> {
    let mut cmd = Command::new("node")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("scripts/order-placement-capture.mjs"),
        )
        .arg(mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    cmd.stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let r = cmd.wait_with_output().unwrap();
    assert!(
        r.status.success(),
        "PLACE source oracle {}",
        String::from_utf8_lossy(&r.stderr)
    );
    serde_json::from_slice(&r.stdout).unwrap()
}
fn suite(ops: &[&'static str], scenes: &[&'static str], mode: &str, floor: usize) {
    assert_eq!(
        ops.len() * scenes.len(),
        floor,
        "PLACE independent fixture recipe count"
    );
    let mut all = vec![];
    for &op in ops {
        for &scene in scenes {
            let h = make(op, scene);
            let input = inputs(&h);
            let r = execute(&h);
            check(&h, &r);
            all.push((h, r, input))
        }
    }
    let inputs = all.iter().map(|(_, _, v)| v.clone()).collect::<Vec<_>>();
    let outputs = source(&inputs, mode);
    assert_eq!(
        outputs.len(),
        floor,
        "PLACE independent source output count"
    );
    for (i, ((h, r, _), s)) in all.iter().zip(outputs).enumerate() {
        compare(
            &output(h, r),
            &s,
            &format!("{mode}:{i}:{}:{}", h.op, h.scene),
        )
    }
}
#[test]
fn native_single_then_source() {
    suite(
        &["single"],
        &[
            "normal",
            "algo",
            "uta",
            "uta-algo",
            "uta-trailing",
            "resolver-error",
            "ensure-error",
            "format-reject",
        ],
        "single",
        8,
    )
}
#[test]
fn native_bulk_then_source() {
    suite(
        &["bulk"],
        &[
            "normal",
            "uta",
            "trailing",
            "empty",
            "format-reject",
            "first-iterator-error",
            "place-reject",
            "assert-error",
        ],
        "bulk",
        8,
    )
}
#[test]
fn native_batches_then_source() {
    suite(
        &["batch", "inverse-batch"],
        &[
            "normal",
            "empty",
            "failure",
            "reject",
            "margin-reject",
            "no-ids",
            "missing-methods",
            "holes",
        ],
        "batch",
        16,
    )
}
#[test]
fn native_uta_then_source() {
    suite(
        &["uta", "strategy"],
        &[
            "normal",
            "client-fallback",
            "payload-fallback",
            "no-id",
            "api-error",
            "reject",
            "empty",
        ],
        "uta",
        14,
    )
}
#[test]
fn native_algo_then_source() {
    suite(
        &["algo"],
        &["normal", "no-id", "api-error", "reject", "empty"],
        "algo",
        5,
    )
}
#[test]
fn native_cancel_then_source() {
    suite(
        &["cancel"],
        &[
            "normal",
            "absent",
            "throw-absent",
            "strict-absent",
            "strict-throw",
            "reject",
            "empty",
            "null-opts",
        ],
        "cancel",
        8,
    )
}

fn binding(member: &V, array: &V, name: &str) {
    native(&get(member, "path"), &s(name), "array method selector");
    let owner = get(member, "owner");
    if !matches!(owner, V::U) {
        alias(&owner, array, "captured array method owner");
    }
}
