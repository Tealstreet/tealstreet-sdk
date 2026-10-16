use safe_cex_venue_bitrue::public_bodies::*;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};
#[derive(Clone)]
enum V {
    U,
    Null,
    Bool(bool),
    Num(f64),
    Str(Vec<u16>),
    Obj(Rc<O>),
    Fun(Rc<F>),
    Mark(Rc<String>),
}
struct O {
    tag: String,
    array: bool,
    fields: RefCell<Vec<(String, V)>>,
}
enum F {
    Builtin(String, String),
    Callback(NativeCallback<V, Rc<String>>),
}
fn text(s: &str) -> V {
    V::Str(s.encode_utf16().collect())
}
fn string(v: &V) -> String {
    match v {
        V::Str(s) => String::from_utf16_lossy(s),
        V::U => "undefined".into(),
        V::Null => "null".into(),
        V::Bool(v) => v.to_string(),
        V::Num(v) => v.to_string(),
        _ => "[object Object]".into(),
    }
}
fn raw(v: &V, k: &str) -> V {
    if let V::Obj(o) = v {
        o.fields
            .borrow()
            .iter()
            .find(|(key, _)| key == k)
            .map(|(_, v)| v.clone())
            .unwrap_or(V::U)
    } else {
        V::U
    }
}
fn put(v: &V, k: &str, value: V) {
    if let V::Obj(o) = v {
        let mut fields = o.fields.borrow_mut();
        if let Some((_, v)) = fields.iter_mut().find(|(key, _)| key == k) {
            *v = value;
        } else {
            fields.push((k.into(), value));
            fields.sort_by_key(|(key, _)| {
                key.parse::<u32>()
                    .ok()
                    .filter(|i| i.to_string() == *key && *i != u32::MAX)
                    .map_or((1, 0), |i| (0, i))
            });
        }
    } else {
        panic!("not object")
    }
}
fn obj(tag: &str, fields: Vec<(&str, V)>) -> V {
    V::Obj(Rc::new(O {
        tag: tag.into(),
        array: false,
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (k.into(), v)).collect()),
    }))
}
fn array(tag: &str, values: Vec<V>) -> V {
    V::Obj(Rc::new(O {
        tag: tag.into(),
        array: true,
        fields: RefCell::new(
            values
                .into_iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v))
                .collect(),
        ),
    }))
}
fn items(v: &V) -> Vec<V> {
    if let V::Obj(o) = v {
        o.fields.borrow().iter().map(|(_, v)| v.clone()).collect()
    } else {
        panic!("not array")
    }
}
fn tag(v: &V) -> String {
    if let V::Obj(o) = v {
        o.tag.clone()
    } else {
        panic!("not object receiver")
    }
}
fn builtin(tag: &str, name: &str) -> V {
    V::Fun(Rc::new(F::Builtin(tag.into(), name.into())))
}
fn marker(tag: &str) -> V {
    V::Mark(Rc::new(tag.into()))
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Obj(a), V::Obj(b)) => Rc::ptr_eq(a, b),
        (V::Mark(a), V::Mark(b)) => Rc::ptr_eq(a, b),
        (V::Fun(a), V::Fun(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn input(v: &Value) -> V {
    match v {
        Value::Null => V::Null,
        Value::Bool(v) => V::Bool(*v),
        Value::Number(v) => V::Num(v.as_f64().unwrap()),
        Value::String(v) => text(v),
        _ => panic!("fixture primitive"),
    }
}
struct Timer {
    callback: V,
    delay: V,
    value: V,
}
struct Host {
    spec: Value,
    this: V,
    json: V,
    candles: Vec<V>,
    socket: V,
    globals: Vec<(&'static str, V)>,
    error: Rc<String>,
    returned: V,
    map_return: RefCell<Option<V>>,
    promise_return: RefCell<Option<V>>,
    prepared: RefCell<Vec<(V, V)>>,
    events: RefCell<Vec<String>>,
    frames: RefCell<Vec<V>>,
    sent: RefCell<Vec<V>>,
    updates: RefCell<Vec<V>>,
    listeners: RefCell<Vec<V>>,
    emitter: RefCell<Vec<V>>,
    timers: RefCell<Vec<Timer>>,
    cleared: RefCell<Vec<V>>,
    settled: RefCell<Vec<V>>,
    callback_returns: RefCell<Vec<V>>,
    symbols: RefCell<Vec<V>>,
    wire: RefCell<Vec<V>>,
    id: RefCell<usize>,
}
impl Host {
    fn event(&self, e: String) -> Result<(), Rc<String>> {
        self.events.borrow_mut().push(e.clone());
        if self.spec["fail"].as_str() == Some(e.as_str()) {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
    fn spec_bool(&self, k: &str) -> bool {
        self.spec[k].as_bool().unwrap_or(false)
    }
    fn new(spec: Value) -> Rc<Self> {
        let json = obj(
            "json",
            vec![
                ("close", input(spec.get("close").unwrap_or(&json!("2.5")))),
                ("rose", input(spec.get("rose").unwrap_or(&json!("-0.125")))),
                ("vol", input(spec.get("vol").unwrap_or(&json!("3.25")))),
                ("amount", input(spec.get("amount").unwrap_or(&json!("7.5")))),
            ],
        );
        let defs = spec.get("candles").cloned().unwrap_or_else(
            || json!([{"ds":"date","open":1.25,"high":2.5,"low":-0.0,"close":1.5,"vol":9.0}]),
        );
        let candles = defs
            .as_array()
            .unwrap()
            .iter()
            .enumerate()
            .map(|(i, v)| {
                obj(
                    &format!("candle{i}"),
                    vec![
                        ("ds", input(&v["ds"])),
                        ("open", input(&v["open"])),
                        ("high", input(&v["high"])),
                        ("low", input(&v["low"])),
                        ("close", input(&v["close"])),
                        ("vol", input(&v["vol"])),
                    ],
                )
            })
            .collect();
        let topics = obj("topics", vec![]);
        for pair in spec
            .get("topics")
            .cloned()
            .unwrap_or_else(|| json!([["market_a", 1], ["market_b", 0]]))
            .as_array()
            .unwrap()
        {
            put(&topics, pair[0].as_str().unwrap(), input(&pair[1]));
        }
        let requests = obj("requests", vec![]);
        if spec["existingRequest"].as_bool() == Some(true) {
            put(&requests, "topic", array("existing-request", vec![]));
        }
        let symbols = obj(
            "symbols",
            vec![
                ("has", builtin("symbols", "has")),
                ("add", builtin("symbols", "add")),
            ],
        );
        let markets = array(
            "markets",
            spec.get("markets")
                .cloned()
                .unwrap_or_else(|| json!(["BTCUSDT", "ETHUSDT"]))
                .as_array()
                .unwrap()
                .iter()
                .enumerate()
                .map(|(i, s)| obj(&format!("market{i}"), vec![("symbol", input(s))]))
                .collect(),
        );
        let store = obj(
            "store",
            vec![
                ("markets", markets),
                ("updateTicker", builtin("store", "updateTicker")),
            ],
        );
        let emitter = obj(
            "emitter",
            vec![
                ("off", builtin("emitter", "off")),
                ("on", builtin("emitter", "on")),
            ],
        );
        let parent = obj("parent", vec![("store", store), ("emitter", emitter)]);
        let socket = obj(
            "socket",
            vec![("addEventListener", builtin("socket", "addEventListener"))],
        );
        let this = obj(
            "this",
            vec![
                ("topics", topics),
                ("messageHandlers", obj("handlers", vec![])),
                ("resolveRequests", requests),
                ("tickerSymbols", symbols),
                ("parent", parent),
                (
                    "isDisposed",
                    V::Bool(spec["disposed"].as_bool().unwrap_or(false)),
                ),
                (
                    "isConnected",
                    V::Bool(spec["connected"].as_bool() != Some(false)),
                ),
                (
                    "ws",
                    if spec["noSocket"].as_bool() == Some(true) {
                        V::U
                    } else {
                        marker("old-socket")
                    },
                ),
            ],
        );
        for name in ["onOpen", "onMessage", "onClose", "onMarketsChanged"] {
            put(&this, name, marker(name));
        }
        for name in [
            "convertToWsSymbol",
            "handleTickersEvent",
            "ensureTickerSubscription",
            "createWebsocket",
            "subscribe",
            "ping",
            "send",
            "recordMessageHandlerStats",
            "resolveMarketSymbol",
        ] {
            put(&this, name, builtin("this", name));
        }
        let globals = vec![
            (
                "BASE_WS_URL",
                obj(
                    "urls",
                    vec![("public", text("wss://fmarket-ws.bitrue.com/kline-api/ws"))],
                ),
            ),
            ("nanoid", builtin("global", "nanoid")),
            (
                "Object",
                obj("Object", vec![("keys", builtin("Object", "keys"))]),
            ),
            (
                "JSON",
                obj("JSON", vec![("stringify", builtin("JSON", "stringify"))]),
            ),
            ("dayjs", builtin("global", "dayjs")),
            ("Promise", builtin("global", "Promise")),
            ("setTimeout", builtin("global", "setTimeout")),
            ("clearTimeout", builtin("global", "clearTimeout")),
            ("Error", builtin("global", "Error")),
        ];
        let initial_symbols = if spec["existing"].as_bool() == Some(true) {
            vec![text("BTCUSDT")]
        } else {
            vec![]
        };
        Rc::new(Self {
            spec,
            this,
            json,
            candles,
            socket,
            globals,
            error: Rc::new("original-error".into()),
            returned: marker("ignored-return"),
            map_return: RefCell::new(None),
            promise_return: RefCell::new(None),
            prepared: RefCell::new(vec![]),
            events: RefCell::new(vec![]),
            frames: RefCell::new(vec![]),
            sent: RefCell::new(vec![]),
            updates: RefCell::new(vec![]),
            listeners: RefCell::new(vec![]),
            emitter: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
            cleared: RefCell::new(vec![]),
            settled: RefCell::new(vec![]),
            callback_returns: RefCell::new(vec![]),
            symbols: RefCell::new(initial_symbols),
            wire: RefCell::new(vec![]),
            id: RefCell::new(0),
        })
    }
    fn number_value(v: V) -> f64 {
        match v {
            V::Num(v) => v,
            V::Bool(v) => {
                if v {
                    1.0
                } else {
                    0.0
                }
            }
            V::Null => 0.0,
            V::U => f64::NAN,
            V::Str(s) => {
                let s = String::from_utf16_lossy(&s);
                if s.trim().is_empty() {
                    0.0
                } else {
                    s.trim().parse().unwrap_or(f64::NAN)
                }
            }
            _ => panic!("bounded primitive numeric fixture"),
        }
    }
}
impl PublicBodyHost for Host {
    type Value = V;
    type Error = Rc<String>;
    fn undefined(&self) -> V {
        V::U
    }
    fn text(&self, v: &str) -> V {
        text(v)
    }
    fn number(&self, v: f64) -> V {
        V::Num(v)
    }
    fn boolean(&self, v: bool) -> V {
        V::Bool(v)
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::U | V::Null => false,
            V::Bool(v) => *v,
            V::Num(v) => *v != 0.0 && !v.is_nan(),
            V::Str(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn equal_zero(&self, v: &V) -> bool {
        matches!(v,V::Num(v) if *v==0.0)
    }
    fn global(&self, name: &'static str) -> Result<V, Rc<String>> {
        Ok(self
            .globals
            .iter()
            .find(|(k, _)| *k == name)
            .unwrap()
            .1
            .clone())
    }
    fn get(&self, receiver: &V, key: V) -> Result<V, Rc<String>> {
        let k = string(&key);
        let t = tag(receiver);
        self.event(format!("get:{t}:{k}"))?;
        if let V::Obj(o) = receiver {
            if o.array {
                if k == "length" {
                    return Ok(V::Num(o.fields.borrow().len() as f64));
                }
                if ["map", "forEach", "push"].contains(&k.as_str()) {
                    let callee = builtin(&t, &k);
                    self.prepared
                        .borrow_mut()
                        .push((callee.clone(), receiver.clone()));
                    return Ok(callee);
                }
            }
        }
        let value = raw(receiver, &k);
        if matches!(value, V::Fun(_)) {
            self.prepared
                .borrow_mut()
                .push((value.clone(), receiver.clone()));
        }
        Ok(value)
    }
    fn set(&self, receiver: &V, key: V, value: V) -> Result<(), Rc<String>> {
        let k = string(&key);
        self.event(format!("set:{}:{k}", tag(receiver)))?;
        put(receiver, &k, value);
        Ok(())
    }
    fn function(&self, callback: NativeCallback<V, Rc<String>>) -> V {
        V::Fun(Rc::new(F::Callback(callback)))
    }
    fn object(&self, fields: Vec<(&'static str, V)>) -> V {
        obj("", fields)
    }
    fn array(&self, values: Vec<V>) -> V {
        array("", values)
    }
    fn template(&self, v: V) -> Result<V, Rc<String>> {
        Ok(text(&string(&v)))
    }
    fn add(&self, a: V, b: V) -> Result<V, Rc<String>> {
        Ok(if matches!(a, V::Str(_)) || matches!(b, V::Str(_)) {
            let mut a = match a {
                V::Str(s) => s,
                _ => string(&a).encode_utf16().collect(),
            };
            let b = match b {
                V::Str(s) => s,
                _ => string(&b).encode_utf16().collect(),
            };
            a.extend(b);
            V::Str(a)
        } else {
            V::Num(Self::number_value(a) + Self::number_value(b))
        })
    }
    fn divide(&self, a: V, b: V) -> Result<V, Rc<String>> {
        Ok(V::Num(Self::number_value(a) / Self::number_value(b)))
    }
    fn multiply(&self, a: V, b: V) -> Result<V, Rc<String>> {
        Ok(V::Num(Self::number_value(a) * Self::number_value(b)))
    }
    fn unary_plus(&self, v: V) -> Result<V, Rc<String>> {
        Ok(V::Num(Self::number_value(v)))
    }
    fn iterate(
        &self,
        value: V,
        body: &mut dyn FnMut(V) -> Result<(), Rc<String>>,
    ) -> Result<(), Rc<String>> {
        let tag = tag(&value);
        self.event(format!("get:{tag}:iterator"))?;
        self.event(format!("iterate:{tag}"))?;
        let mut result = Ok(());
        for v in items(&value) {
            if let Err(e) = body(v) {
                result = Err(e);
                break;
            }
        }
        let close = self.event(format!("iterator-close:{tag}"));
        result.and(close)
    }
    fn construct(&self, callee: V, args: Vec<V>) -> Result<V, Rc<String>> {
        let V::Fun(f) = callee else {
            panic!("constructor")
        };
        let F::Builtin(_, name) = f.as_ref() else {
            panic!("constructor")
        };
        self.event(format!("construct:{name}"))?;
        match name.as_str() {
            "Promise" => {
                let resolve = builtin("", "resolve");
                let reject = builtin("", "reject");
                if let Err(e) = self.call(args[0].clone(), V::U, vec![resolve, reject]) {
                    self.settled
                        .borrow_mut()
                        .push(array("", vec![text("reject"), V::Mark(e)]));
                }
                let promise = marker("promise");
                self.promise_return.replace(Some(promise.clone()));
                Ok(promise)
            }
            "Error" => {
                assert_eq!(string(&args[0]), "Request timeout");
                Ok(marker("timeout-error"))
            }
            _ => panic!("constructor {name}"),
        }
    }
    fn call(&self, callee: V, receiver: V, args: Vec<V>) -> Result<V, Rc<String>> {
        let V::Fun(f) = callee else { panic!("callee") };
        if let F::Callback(callback) = f.as_ref() {
            return callback(args);
        }
        let F::Builtin(t, name) = f.as_ref() else {
            unreachable!()
        };
        let event = if t.is_empty() && ["resolve", "reject"].contains(&name.as_str()) {
            format!("call:{name}")
        } else {
            format!("call:{t}:{name}")
        };
        self.event(event)?;
        if let Some((_, original_receiver)) = self
            .prepared
            .borrow()
            .iter()
            .rev()
            .find(|(prepared, _)| same(prepared, &V::Fun(f.clone())))
        {
            assert!(
                same(&receiver, original_receiver),
                "BITRUE-PUBLIC-NATIVE-ORIGINAL-RECEIVER {name}"
            );
        }
        if !t.is_empty() && t != "global" {
            assert_eq!(tag(&receiver), *t, "BITRUE-PUBLIC-NATIVE-RECEIVER {name}");
        }
        let a = |i: usize| args.get(i).cloned().unwrap_or(V::U);
        match (t.as_str(), name.as_str()) {
            ("symbols", "has") => Ok(V::Bool(
                self.symbols
                    .borrow()
                    .iter()
                    .any(|v| string(v) == string(&a(0))),
            )),
            ("symbols", "add") => {
                if !self
                    .symbols
                    .borrow()
                    .iter()
                    .any(|v| string(v) == string(&a(0)))
                {
                    self.symbols.borrow_mut().push(a(0));
                }
                Ok(self.returned.clone())
            }
            ("this", "convertToWsSymbol") => {
                Ok(text(&format!("e_{}", string(&a(0)).to_lowercase())))
            }
            ("this", "ensureTickerSubscription") => {
                self.updates
                    .borrow_mut()
                    .push(obj("", vec![("ensure", a(0))]));
                Ok(self.returned.clone())
            }
            ("this", "handleTickersEvent") => {
                assert!(
                    same(&a(0), &self.json),
                    "BITRUE-PUBLIC-NATIVE-CALLBACK-IDENTITY"
                );
                self.updates.borrow_mut().push(obj(
                    "",
                    vec![("callbackJson", a(0)), ("callbackSymbol", a(1))],
                ));
                Ok(self.returned.clone())
            }
            ("this", "createWebsocket") => {
                self.listeners
                    .borrow_mut()
                    .push(array("", vec![text("url"), a(0)]));
                Ok(self.socket.clone())
            }
            ("socket", "addEventListener") => {
                let label = string(&a(0));
                let target = match label.as_str() {
                    "open" => "onOpen",
                    "message" => "onMessage",
                    "close" => "onClose",
                    _ => panic!("event"),
                };
                assert!(
                    same(&a(1), &raw(&self.this, target)),
                    "BITRUE-PUBLIC-NATIVE-LISTENER-IDENTITY"
                );
                self.listeners
                    .borrow_mut()
                    .push(array("", vec![a(0), a(1)]));
                Ok(self.returned.clone())
            }
            ("emitter", "off" | "on") => {
                assert!(
                    same(&a(1), &raw(&self.this, "onMarketsChanged")),
                    "BITRUE-PUBLIC-NATIVE-EMITTER-IDENTITY"
                );
                self.emitter
                    .borrow_mut()
                    .push(array("", vec![text(name), a(0), a(1)]));
                Ok(self.returned.clone())
            }
            ("this", "subscribe" | "ping") => Ok(self.returned.clone()),
            ("this", "send") => {
                self.sent.borrow_mut().push(a(0));
                Ok(self.returned.clone())
            }
            ("this", "recordMessageHandlerStats") => {
                self.updates
                    .borrow_mut()
                    .push(obj("", vec![("stats", a(0))]));
                Ok(self.returned.clone())
            }
            ("this", "resolveMarketSymbol") => {
                self.updates
                    .borrow_mut()
                    .push(obj("", vec![("resolve", a(0))]));
                Ok(if self.spec_bool("resolvedObject") {
                    marker("resolved")
                } else {
                    input(self.spec.get("resolved").unwrap_or(&json!("BTCUSDT")))
                })
            }
            ("store", "updateTicker") => {
                assert!(
                    same(&raw(&a(1), "info"), &self.json),
                    "BITRUE-PUBLIC-NATIVE-TICKER-INFO-IDENTITY"
                );
                let resolved = raw(&a(1), "id");
                if matches!(resolved, V::Mark(_)) {
                    assert!(
                        same(&resolved, &raw(&a(1), "symbol"))
                            && same(&resolved, &raw(&a(0), "id")),
                        "BITRUE-PUBLIC-NATIVE-RESOLVED-IDENTITY"
                    );
                }
                self.updates
                    .borrow_mut()
                    .push(obj("", vec![("selector", a(0)), ("ticker", a(1))]));
                Ok(self.returned.clone())
            }
            ("global", "nanoid") => {
                let mut id = self.id.borrow_mut();
                *id += 1;
                Ok(text(&format!("id{id}")))
            }
            ("Object", "keys") => {
                let V::Obj(o) = a(0) else { panic!("keys") };
                let keys = o
                    .fields
                    .borrow()
                    .iter()
                    .map(|(k, _)| k.clone())
                    .collect::<Vec<_>>();
                let mut indices = keys
                    .iter()
                    .filter_map(|k| {
                        k.parse::<u32>()
                            .ok()
                            .filter(|i| i.to_string() == *k && *i != u32::MAX)
                    })
                    .collect::<Vec<_>>();
                indices.sort();
                let mut values = indices
                    .iter()
                    .map(|i| text(&i.to_string()))
                    .collect::<Vec<_>>();
                values.extend(
                    keys.iter()
                        .filter(|k| !indices.iter().any(|i| i.to_string() == **k))
                        .map(|k| text(k)),
                );
                Ok(array("keys", values))
            }
            ("JSON", "stringify") => {
                self.frames.borrow_mut().push(a(0));
                let value = marker(&format!("wire{}", self.wire.borrow().len()));
                self.wire.borrow_mut().push(value.clone());
                Ok(value)
            }
            ("global", "dayjs") => {
                assert_eq!(string(&a(0)), "date");
                Ok(obj("day", vec![("utcOffset", builtin("day", "utcOffset"))]))
            }
            ("day", "utcOffset") => {
                assert!(
                    matches!(a(0),V::Num(v)if v==480.0),
                    "BITRUE-PUBLIC-NATIVE-OFFSET"
                );
                assert!(
                    matches!(a(1), V::Bool(true)),
                    "BITRUE-PUBLIC-NATIVE-KEEP-LOCAL-TIME"
                );
                Ok(obj(
                    "adjusted",
                    vec![("valueOf", builtin("adjusted", "valueOf"))],
                ))
            }
            ("adjusted", "valueOf") => Ok(input(
                self.spec.get("clock").unwrap_or(&json!(1720000123456.5)),
            )),
            ("global", "setTimeout") => {
                let value = marker(&format!("timer{}", self.timers.borrow().len()));
                self.timers.borrow_mut().push(Timer {
                    callback: a(0),
                    delay: a(1),
                    value: value.clone(),
                });
                Ok(value)
            }
            ("global", "clearTimeout") => {
                let timer = self.timers.borrow()[0].value.clone();
                assert!(same(&a(0), &timer), "BITRUE-PUBLIC-NATIVE-TIMER-IDENTITY");
                self.cleared.borrow_mut().push(a(0));
                Ok(self.returned.clone())
            }
            ("", "resolve" | "reject") => {
                if name == "resolve" {
                    assert!(
                        same(&a(0), &self.json),
                        "BITRUE-PUBLIC-NATIVE-RESOLVE-IDENTITY"
                    );
                }
                self.settled
                    .borrow_mut()
                    .push(array("", vec![text(name), a(0)]));
                Ok(V::U)
            }
            (_, "forEach") => {
                for (i, v) in items(&receiver).into_iter().enumerate() {
                    let result =
                        self.call(a(0), V::U, vec![v, V::Num(i as f64), receiver.clone()])?;
                    self.callback_returns.borrow_mut().push(result);
                }
                Ok(self.returned.clone())
            }
            (_, "map") => {
                let mut values = vec![];
                for (i, v) in items(&receiver).into_iter().enumerate() {
                    values.push(self.call(
                        a(0),
                        V::U,
                        vec![v, V::Num(i as f64), receiver.clone()],
                    )?);
                }
                let value = array("", values);
                self.map_return.replace(Some(value.clone()));
                Ok(value)
            }
            (_, "push") => {
                let n = items(&receiver).len();
                put(&receiver, &n.to_string(), a(0));
                Ok(V::Num((n + 1) as f64))
            }
            _ => panic!("unsupported builtin {t}:{name}"),
        }
    }
}
fn assert_native(actual: &V, expected: &Value, label: &str) {
    let fail = || panic!("BITRUE-PUBLIC-NATIVE-VALUE {label} expected {expected}");
    match (expected["kind"].as_str().unwrap(), actual) {
        ("undefined", V::U) | ("null", V::Null) => {}
        ("boolean", V::Bool(v)) => assert_eq!(
            *v,
            expected["value"].as_bool().unwrap(),
            "BITRUE-PUBLIC-NATIVE-BOOLEAN {label}"
        ),
        ("number", V::Num(v)) => {
            let e = u64::from_str_radix(expected["bits"].as_str().unwrap(), 16).unwrap();
            assert_eq!(v.to_bits(), e, "BITRUE-PUBLIC-NATIVE-NUMBER {label}");
        }
        ("string", V::Str(v)) => assert_eq!(
            *v,
            expected["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u16)
                .collect::<Vec<_>>(),
            "BITRUE-PUBLIC-NATIVE-STRING {label}"
        ),
        ("identity", V::Mark(v)) => assert_eq!(
            v.as_str(),
            expected["tag"].as_str().unwrap(),
            "BITRUE-PUBLIC-NATIVE-MARKER {label}"
        ),
        ("function", V::Fun(f)) => {
            let tag = match f.as_ref() {
                F::Callback(_) => "callback".into(),
                F::Builtin(t, n) => format!("{t}:{n}"),
            };
            assert_eq!(
                tag,
                expected["tag"].as_str().unwrap(),
                "BITRUE-PUBLIC-NATIVE-FUNCTION {label}"
            );
        }
        ("array", V::Obj(o)) if o.array => {
            let values = items(actual);
            let want = expected["values"].as_array().unwrap();
            assert_eq!(
                values.len(),
                want.len(),
                "BITRUE-PUBLIC-NATIVE-ARRAY-COUNT {label}"
            );
            for (i, (a, e)) in values.iter().zip(want).enumerate() {
                assert_native(a, e, &format!("{label}[{i}]"));
            }
        }
        ("object", V::Obj(o)) if !o.array => {
            let fields = o
                .fields
                .borrow()
                .iter()
                .filter(|(_, v)| !matches!(v, V::Fun(_)))
                .cloned()
                .collect::<Vec<_>>();
            let want = expected["fields"].as_array().unwrap();
            assert_eq!(
                fields.len(),
                want.len(),
                "BITRUE-PUBLIC-NATIVE-FIELD-COUNT {label}"
            );
            for ((key, v), e) in fields.iter().zip(want) {
                assert_eq!(
                    key,
                    e[0].as_str().unwrap(),
                    "BITRUE-PUBLIC-NATIVE-FIELD {label}"
                );
                assert_native(v, &e[1], &format!("{label}.{key}"));
            }
        }
        _ => fail(),
    }
}
fn check_values(values: &[V], expected: &Value, label: &str) {
    let e = expected.as_array().unwrap();
    assert_eq!(values.len(), e.len(), "BITRUE-PUBLIC-NATIVE-COUNT {label}");
    for (i, (a, b)) in values.iter().zip(e).enumerate() {
        assert_native(a, b, &format!("{label}:{i}"));
    }
}
fn lengths(value: V) -> Value {
    let V::Obj(o) = value else { panic!("lengths") };
    let mut result = serde_json::Map::new();
    for (k, v) in o.fields.borrow().iter() {
        result.insert(k.clone(), json!(items(v).len()));
    }
    Value::Object(result)
}
#[test]
fn public_bodies_match_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public_bodies_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let specs: Vec<Value> =
        serde_json::from_str(include_str!("public_bodies_fixtures.json")).unwrap();
    for (spec, expected) in specs.into_iter().zip(oracle["rows"].as_array().unwrap()) {
        let name = spec["name"].as_str().unwrap().to_owned();
        let h = Host::new(spec);
        let this = h.this.clone();
        let result = match h.spec["op"].as_str().unwrap() {
            "ensureTickerSubscription" => {
                ensure_ticker_subscription(h.clone(), this, text("BTCUSDT"))
            }
            "connectAndSubscribe" => connect_and_subscribe(h.clone(), this),
            "onOpen" => on_open(h.clone(), this),
            "onMarketsChanged" => {
                let added = h
                    .spec
                    .get("added")
                    .cloned()
                    .unwrap_or_else(|| json!(["BTCUSDT", "ETHUSDT"]));
                let added = array(
                    "added",
                    added
                        .as_array()
                        .unwrap()
                        .iter()
                        .enumerate()
                        .map(|(i, s)| obj(&format!("added{i}"), vec![("symbol", input(s))]))
                        .collect(),
                );
                on_markets_changed(h.clone(), this, obj("event", vec![("added", added)]))
            }
            "subscribe" => subscribe(h.clone(), this),
            "mapKline" => map_kline(h.clone(), this, array("candles", h.candles.clone())),
            "handleTickersEvent" => {
                handle_tickers_event(h.clone(), this, h.json.clone(), text("E-BTC-USDT"))
            }
            "makeRequest" => make_request(h.clone(), this, text("topic")),
            _ => panic!("unknown op"),
        };
        let result = result.and_then(|result| {
            if h.spec_bool("fireTicker") {
                let handlers = raw(&h.this, "messageHandlers");
                let V::Obj(o) = handlers else {
                    panic!("handlers")
                };
                let parser = items(&o.fields.borrow()[0].1)[0].clone();
                let ret = h.call(parser, V::U, vec![h.json.clone()])?;
                h.callback_returns.borrow_mut().push(ret);
            }
            if h.spec_bool("fireResolve") {
                let requests = raw(&h.this, "resolveRequests");
                let parser = items(&raw(&requests, "topic"))[0].clone();
                let ret = h.call(
                    parser,
                    V::U,
                    vec![obj("response", vec![("data", h.json.clone())])],
                )?;
                h.callback_returns.borrow_mut().push(ret);
            }
            if h.spec_bool("fireTimeout") {
                let parser = h.timers.borrow()[0].callback.clone();
                let ret = h.call(parser, V::U, vec![])?;
                h.callback_returns.borrow_mut().push(ret);
            }
            Ok(result)
        });
        match result {
            Ok(v) => {
                assert!(
                    expected["thrown"].is_null(),
                    "BITRUE-PUBLIC-NATIVE-ERROR {name}"
                );
                assert_native(&v, &expected["result"], &format!("{name}:result"));
                if h.spec["op"] == "mapKline" {
                    assert!(
                        same(&v, h.map_return.borrow().as_ref().unwrap()),
                        "BITRUE-PUBLIC-NATIVE-MAP-RETURN-IDENTITY {name}"
                    );
                }
                if h.spec["op"] == "makeRequest" {
                    assert!(
                        same(&v, h.promise_return.borrow().as_ref().unwrap()),
                        "BITRUE-PUBLIC-NATIVE-PROMISE-RETURN-IDENTITY {name}"
                    );
                }
                if h.spec_bool("fireTicker") {
                    assert!(
                        same(h.callback_returns.borrow().last().unwrap(), &h.returned),
                        "BITRUE-PUBLIC-NATIVE-CALLBACK-RETURN-IDENTITY {name}"
                    );
                }
                if h.spec["op"] == "mapKline" {
                    for (c, original) in items(&v).iter().zip(&h.candles) {
                        assert!(
                            same(&raw(c, "info"), original),
                            "BITRUE-PUBLIC-NATIVE-CANDLE-INFO-IDENTITY {name}"
                        );
                    }
                }
            }
            Err(e) => {
                assert!(
                    Rc::ptr_eq(&e, &h.error),
                    "BITRUE-PUBLIC-NATIVE-ERROR-IDENTITY {name}"
                );
                assert_eq!(
                    expected["thrown"]["tag"], "original-error",
                    "BITRUE-PUBLIC-NATIVE-THROW {name}"
                );
            }
        }
        // Native values/state/callbacks BEFORE any wire forwarding or inventories.
        check_values(
            &h.updates.borrow(),
            &expected["updates"],
            &format!("{name}:updates"),
        );
        check_values(
            &h.listeners.borrow(),
            &expected["listeners"],
            &format!("{name}:listeners"),
        );
        check_values(
            &h.emitter.borrow(),
            &expected["emitter"],
            &format!("{name}:emitter"),
        );
        assert_native(
            &raw(&h.this, "topics"),
            &expected["topics"],
            &format!("{name}:topics"),
        );
        check_values(
            &h.callback_returns.borrow(),
            &expected["callbackReturns"],
            &format!("{name}:callbackReturns"),
        );
        check_values(
            &h.settled.borrow(),
            &expected["settled"],
            &format!("{name}:settled"),
        );
        for (i, timer) in h.timers.borrow().iter().enumerate() {
            assert_native(
                &timer.delay,
                &expected["timers"][i]["delay"],
                &format!("{name}:timer-delay"),
            );
            assert_native(
                &timer.value,
                &expected["timers"][i]["value"],
                &format!("{name}:timer-value"),
            );
        }
        assert_eq!(
            h.timers.borrow().len(),
            expected["timers"].as_array().unwrap().len(),
            "BITRUE-PUBLIC-NATIVE-TIMER-COUNT {name}"
        );
        check_values(
            &h.cleared.borrow(),
            &expected["cleared"],
            &format!("{name}:cleared"),
        );
        check_values(
            &h.frames.borrow(),
            &expected["frames"],
            &format!("{name}:frames"),
        );
        assert_eq!(
            *h.events.borrow(),
            expected["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap().to_owned())
                .collect::<Vec<_>>(),
            "BITRUE-PUBLIC-NATIVE-EFFECTS {name}"
        );
        assert_eq!(
            lengths(raw(&h.this, "messageHandlers")),
            expected["handlers"],
            "BITRUE-PUBLIC-NATIVE-HANDLERS {name}"
        );
        assert_eq!(
            lengths(raw(&h.this, "resolveRequests")),
            expected["requests"],
            "BITRUE-PUBLIC-NATIVE-REQUESTS {name}"
        );
        let symbols = h.symbols.borrow();
        let expected_symbols = expected["symbols"].as_array().unwrap();
        assert_eq!(
            symbols.len(),
            expected_symbols.len(),
            "BITRUE-PUBLIC-NATIVE-SYMBOL-COUNT {name}"
        );
        for (actual, e) in symbols.iter().zip(expected_symbols) {
            let V::Str(units) = actual else {
                panic!("native symbol")
            };
            assert_eq!(
                *units,
                e.as_str().unwrap().encode_utf16().collect::<Vec<_>>(),
                "BITRUE-PUBLIC-NATIVE-SYMBOL {name}"
            );
        }
        assert_eq!(
            same(&raw(&h.this, "ws"), &h.socket),
            expected["socketOriginal"].as_bool().unwrap(),
            "BITRUE-PUBLIC-NATIVE-SOCKET-IDENTITY {name}"
        );
        // Stringify is an opaque external dependency: assert the ORIGINAL return.
        for (a, b) in h.sent.borrow().iter().zip(h.wire.borrow().iter()) {
            assert!(same(a, b), "BITRUE-PUBLIC-NATIVE-WIRE-IDENTITY {name}");
        }
        check_values(&h.sent.borrow(), &expected["sent"], &format!("{name}:sent"));
        assert_eq!(expected["tickerInfo"], true);
        assert_eq!(expected["candleInfo"], true);
        assert_eq!(expected["frameIdentities"], true);
    }
    assert_eq!(
        oracle["rows"].as_array().unwrap().len(),
        FIXTURE_FLOOR,
        "source fixture floor"
    );
    assert_eq!(
        serde_json::from_str::<Vec<Value>>(include_str!("public_bodies_fixtures.json"))
            .unwrap()
            .len(),
        FIXTURE_FLOOR,
        "independent fixture floor"
    );
    println!(
        "BITRUE public bodies native floor {FIXTURE_FLOOR}, stops index60 request:timeout-store-throw; full builtin/class/runtime coverage is not claimed"
    );
}
const FIXTURE_FLOOR: usize = 61; // independently fixed after explicit fixture enumeration
