use safe_cex_venue_phemex::public_routing::{self as routing, Parameter, Request, RoutingHost};
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};
#[derive(Clone, Debug)]
enum V {
    Undefined,
    Null,
    Bool(bool),
    Num(f64),
    Text(Vec<u16>),
    Array(Rc<RefCell<Vec<V>>>),
    Object(Rc<Vec<(Vec<u16>, V)>>),
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn array(v: Vec<V>) -> V {
    V::Array(Rc::new(RefCell::new(v)))
}
fn object(v: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(v.into_iter().map(|(k, v)| (u(k), v)).collect()))
}
impl V {
    fn decode(v: &Value) -> Self {
        if let Some(a) = v.get("$units").and_then(Value::as_array) {
            return Self::Text(a.iter().map(|v| v.as_u64().unwrap() as u16).collect());
        }
        if let Some(bits) = v.get("$bits").and_then(Value::as_str) {
            return Self::Num(f64::from_bits(u64::from_str_radix(bits, 16).unwrap()));
        }
        if v.get("$undefined").is_some() {
            return Self::Undefined;
        }
        match v {
            Value::Null => Self::Null,
            Value::Bool(b) => Self::Bool(*b),
            Value::Number(n) => Self::Num(n.as_f64().unwrap()),
            Value::String(s) => Self::Text(u(s)),
            Value::Array(a) => array(a.iter().map(Self::decode).collect()),
            Value::Object(o) => Self::Object(Rc::new(
                o.iter().map(|(k, v)| (u(k), Self::decode(v))).collect(),
            )),
        }
    }
    fn get(&self, key: &str) -> V {
        match self {
            Self::Object(o) => o
                .iter()
                .find(|(k, _)| *k == u(key))
                .map(|(_, v)| v.clone())
                .unwrap_or(Self::Undefined),
            _ => Self::Undefined,
        }
    }
    fn truthy(&self) -> bool {
        match self {
            Self::Undefined | Self::Null => false,
            Self::Bool(b) => *b,
            Self::Num(n) => *n != 0.0 && !n.is_nan(),
            Self::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn text(&self) -> Option<Vec<u16>> {
        if let Self::Text(s) = self {
            Some(s.clone())
        } else {
            None
        }
    }
    fn values(&self) -> Vec<V> {
        if let Self::Array(v) = self {
            v.borrow().clone()
        } else {
            vec![]
        }
    }
    fn number(&self) -> f64 {
        match self {
            Self::Undefined => f64::NAN,
            Self::Null => 0.0,
            Self::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Num(n) => *n,
            Self::Text(s) => String::from_utf16(s)
                .map(|s| safe_cex_num::number_from_string(&s))
                .unwrap_or(f64::NAN),
            Self::Object(_) => f64::NAN,
            Self::Array(a) => {
                let a = a.borrow();
                if a.is_empty() {
                    0.0
                } else if a.len() == 1 {
                    a[0].number()
                } else {
                    f64::NAN
                }
            }
        }
    }
    fn obs(&self) -> Value {
        match self {
            Self::Undefined => json!({"undefined":true}),
            Self::Null => Value::Null,
            Self::Bool(b) => json!(b),
            Self::Num(n) => {
                json!({"bits":format!("{:016x}",if n.is_nan(){f64::NAN.to_bits()}else{n.to_bits()})})
            }
            Self::Text(s) => json!({"units":s}),
            Self::Array(a) => Value::Array(a.borrow().iter().map(Self::obs).collect()),
            Self::Object(o) => {
                let mut entries = o
                    .iter()
                    .map(|(k, v)| json!([{"units":k},v.obs()]))
                    .collect::<Vec<_>>();
                entries.sort_by_key(|a| a[0].to_string());
                json!({"object":entries})
            }
        }
    }
    fn wire(&self) -> String {
        match self {
            Self::Undefined | Self::Null => "null".into(),
            Self::Bool(b) => b.to_string(),
            Self::Num(n) => {
                if n.is_finite() {
                    safe_cex_num::js_number_string(*n)
                } else {
                    "null".into()
                }
            }
            Self::Text(s) => quote(s),
            Self::Array(a) => format!(
                "[{}]",
                a.borrow()
                    .iter()
                    .map(Self::wire)
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            Self::Object(o) => format!(
                "{{{}}}",
                o.iter()
                    .filter(|(_, v)| !matches!(v, Self::Undefined))
                    .map(|(k, v)| format!("{}:{}", quote(k), v.wire()))
                    .collect::<Vec<_>>()
                    .join(",")
            ),
        }
    }
}
fn quote(s: &[u16]) -> String {
    let mut out = String::from("\"");
    for ch in char::decode_utf16(s.iter().copied()) {
        match ch {
            Err(e) => out.push_str(&format!("\\u{:04x}", e.unpaired_surrogate())),
            Ok('"') => out.push_str("\\\""),
            Ok('\\') => out.push_str("\\\\"),
            Ok('\n') => out.push_str("\\n"),
            Ok('\r') => out.push_str("\\r"),
            Ok('\t') => out.push_str("\\t"),
            Ok('\u{8}') => out.push_str("\\b"),
            Ok('\u{c}') => out.push_str("\\f"),
            Ok(c) if (c as u32) < 32 => out.push_str(&format!("\\u{:04x}", c as u32)),
            Ok(c) => out.push(c),
        }
    }
    out.push('"');
    out
}
#[derive(Clone, Debug)]
struct E {
    original: bool,
    token: Rc<()>,
}
struct Iter {
    array: Rc<RefCell<Vec<V>>>,
    index: usize,
}
struct Host {
    input: V,
    effects: Vec<V>,
    warned: bool,
    request: f64,
    cache: Option<Vec<(f64, Vec<u16>)>>,
    frames: Vec<(Vec<u16>, V)>,
    current: Rc<RefCell<Vec<V>>>,
    replaced: bool,
    error: Rc<()>,
    payload: Option<V>,
}
impl Host {
    fn new(input: V) -> Self {
        let frames = input.get("timeframes").values();
        let frames = if matches!(input.get("timeframes"), V::Undefined) {
            vec![(u("1m"), V::Num(60.0)), (u("5m"), V::Num(300.0))]
        } else {
            frames
                .into_iter()
                .map(|p| {
                    let a = p.values();
                    (a[0].text().unwrap(), a[1].clone())
                })
                .collect()
        };
        let callbacks = Rc::new(RefCell::new(input.get("callbacks").values()));
        let request = match input.get("requestId") {
            V::Undefined | V::Null => 1200.0,
            v => v.number(),
        };
        Self {
            warned: input.get("warned").truthy(),
            input,
            effects: vec![],
            request,
            cache: None,
            frames,
            current: callbacks,
            replaced: false,
            error: Rc::new(()),
            payload: None,
        }
    }
    fn effect(&mut self, name: &str, args: Vec<V>) -> Result<(), E> {
        let mut v = vec![V::Text(u(name))];
        v.extend(args);
        self.effects.push(array(v));
        if self.input.get("failure").text() == Some(u(name)) {
            Err(E {
                original: true,
                token: self.error.clone(),
            })
        } else {
            Ok(())
        }
    }
    fn intrinsic(&self) -> E {
        E {
            original: false,
            token: Rc::new(()),
        }
    }
    fn topics(&self) -> Vec<(Vec<u16>, V)> {
        let mut out = Vec::<(Vec<u16>, V)>::new();
        for p in self.input.get("topics").values() {
            let a = p.values();
            let key = a[0].text().unwrap();
            if let Some(i) = out.iter().position(|(k, _)| *k == key) {
                out[i].1 = a[1].clone()
            } else {
                out.push((key, a[1].clone()))
            }
        }
        out.sort_by_key(|(k, _)| {
            String::from_utf16(k)
                .ok()
                .and_then(|k| safe_cex_num::js_array_index(&k))
                .map(|n| (false, n))
                .unwrap_or((true, 0))
        });
        out
    }
}
impl RoutingHost for Host {
    type Value = V;
    type Socket = ();
    type Call = bool;
    type Callbacks = Rc<RefCell<Vec<V>>>;
    type Iterator = Iter;
    type Callback = V;
    type Error = E;
    fn disposed(&mut self) -> Result<bool, E> {
        Ok(self.input.get("disposed").truthy())
    }
    fn testnet(&mut self) -> Result<bool, E> {
        Ok(self.input.get("testnet").truthy())
    }
    fn create(&mut self, url: &'static str) -> Result<(), E> {
        self.effect("create", vec![V::Text(u(url))])
    }
    fn socket(&mut self, _: ()) -> Result<(), E> {
        Ok(())
    }
    fn listener(&mut self, event: &'static str, member: &'static str) -> Result<(), E> {
        self.effect(&format!("listener:{event}"), vec![V::Text(u(member))])
    }
    fn send_target(&mut self) -> Result<bool, E> {
        Ok(self.replaced)
    }
    fn serialize(&mut self, r: Request) -> Result<Vec<u16>, E> {
        let params = array(
            r.params
                .into_iter()
                .map(|p| match p {
                    Parameter::Text(s) => V::Text(s),
                    Parameter::Number(n) => V::Num(n),
                    Parameter::Undefined => V::Undefined,
                })
                .collect(),
        );
        let request = if r.id_first {
            object(vec![
                ("id", V::Num(r.id)),
                ("method", V::Text(r.method)),
                ("params", params),
            ])
        } else {
            object(vec![
                ("method", V::Text(r.method)),
                ("params", params),
                ("id", V::Num(r.id)),
            ])
        };
        self.effect("serialize", vec![request.clone()])?;
        if self.input.get("replaceSend").truthy() {
            self.replaced = true
        }
        Ok(u(&request.wire()))
    }
    fn send(&mut self, call: bool, text: Vec<u16>) -> Result<(), E> {
        assert!(!call, "PUBLIC native retained send target");
        self.effect("send", vec![V::Text(text)])
    }
    fn reset(&mut self) -> Result<(), E> {
        self.effect("reset", vec![])
    }
    fn subscribe(&mut self) -> Result<(), E> {
        self.effect("subscribe", vec![])
    }
    fn ping(&mut self) -> Result<(), E> {
        self.effect("ping", vec![])
    }
    fn topic_keys(&mut self) -> Result<Vec<Vec<u16>>, E> {
        Ok(self.topics().into_iter().map(|(k, _)| k).collect())
    }
    fn number_text(&mut self, text: Option<&[u16]>) -> Result<f64, E> {
        Ok(text
            .map(|s| V::Text(s.to_vec()).number())
            .unwrap_or(f64::NAN))
    }
    fn topic_values(&mut self) -> Result<Vec<f64>, E> {
        Ok(self.topics().into_iter().map(|(_, v)| v.number()).collect())
    }
    fn warned(&mut self) -> Result<bool, E> {
        Ok(self.warned)
    }
    fn set_warned(&mut self, b: bool) -> Result<(), E> {
        self.warned = b;
        Ok(())
    }
    fn warning(&mut self, m: Vec<u16>) -> Result<(), E> {
        self.effect("warning", vec![V::Text(m), V::Text(u("warning"))])
    }
    fn now(&mut self) -> Result<f64, E> {
        Ok(123.25)
    }
    fn set_ping_at(&mut self, time: f64) -> Result<(), E> {
        assert_eq!(time.to_bits(), 123.25f64.to_bits(), "PUBLIC native pingAt");
        Ok(())
    }
    fn next_request_id(&mut self) -> Result<f64, E> {
        let id = self.request;
        self.request += 1.0;
        Ok(id)
    }
    fn number_string(&mut self, n: f64) -> Result<Vec<u16>, E> {
        Ok(u(&safe_cex_num::js_number_string(n)))
    }
    fn callbacks(&mut self, _: &[u16]) -> Result<Option<Self::Callbacks>, E> {
        Ok(Some(self.current.clone()))
    }
    fn nonempty(&mut self, c: &Self::Callbacks) -> Result<bool, E> {
        Ok(!c.borrow().is_empty())
    }
    fn stats(&mut self, topic: &[u16]) -> Result<(), E> {
        self.effect("stats", vec![V::Text(topic.to_vec())])
    }
    fn iterate_callbacks(&mut self, c: Self::Callbacks) -> Result<Iter, E> {
        Ok(Iter { array: c, index: 0 })
    }
    fn next_callback(&mut self, it: &mut Iter) -> Result<Option<V>, E> {
        let value = it.array.borrow().get(it.index).cloned();
        it.index += 1;
        Ok(value)
    }
    fn invoke(&mut self, callback: V, payload: &V) -> Result<(), E> {
        if let Some(original) = &self.payload {
            if let (V::Object(a), V::Object(b)) = (original, payload) {
                assert!(Rc::ptr_eq(a, b), "PUBLIC native callback object identity");
            }
            if let (V::Array(a), V::Array(b)) = (original, payload) {
                assert!(Rc::ptr_eq(a, b), "PUBLIC native callback payload identity")
            }
        }
        let id = String::from_utf16(&callback.text().unwrap()).unwrap();
        self.effect(&format!("callback:{id}"), vec![payload.clone()])?;
        if id == "a" {
            if self.input.get("append").truthy() {
                self.current.borrow_mut().push(V::Text(u("c")))
            }
            if self.input.get("remove").truthy() {
                self.current.borrow_mut().pop();
            }
            if self.input.get("replace").truthy() {
                self.current = Rc::new(RefCell::new(vec![V::Text(u("z"))]));
            }
        }
        Ok(())
    }
    fn number(&mut self, v: V) -> Result<f64, E> {
        Ok(v.number())
    }
    fn interval_cache_exists(&mut self) -> Result<bool, E> {
        Ok(self.cache.is_some())
    }
    fn create_interval_cache(&mut self) -> Result<(), E> {
        self.cache = Some(vec![]);
        if self.input.get("cacheReplacesFrames").truthy() {
            self.frames = vec![(u("other"), V::Num(60.0))];
        }
        Ok(())
    }
    fn timeframes(&mut self) -> Result<V, E> {
        Ok(V::Object(Rc::new(self.frames.clone())))
    }
    fn timeframe_keys(&mut self, frames: &V) -> Result<Vec<Vec<u16>>, E> {
        if let V::Object(frames) = frames {
            Ok(frames.iter().map(|(k, _)| k.clone()).collect())
        } else {
            panic!()
        }
    }
    fn timeframe(&mut self, frames: &V, key: &[u16]) -> Result<V, E> {
        if self.input.get("failure").text() == Some(u("timeframe")) {
            self.effect("timeframe", vec![])?;
        }
        if let V::Object(frames) = frames {
            Ok(frames.iter().find(|(k, _)| k == key).unwrap().1.clone())
        } else {
            panic!()
        }
    }
    fn cache_interval(&mut self, n: f64, s: Vec<u16>) -> Result<(), E> {
        let cache = self.cache.as_mut().unwrap();
        if let Some(i) = cache
            .iter()
            .position(|(k, _)| *k == n || (k.is_nan() && n.is_nan()))
        {
            cache[i].1 = s
        } else {
            cache.push((n, s))
        }
        Ok(())
    }
    fn cached_interval(&mut self, n: f64) -> Result<Option<Vec<u16>>, E> {
        Ok(self
            .cache
            .as_ref()
            .unwrap()
            .iter()
            .find(|(k, _)| *k == n)
            .map(|(_, s)| s.clone()))
    }
    fn received(&mut self) -> Result<(), E> {
        self.effect("received", vec![])
    }
    fn parse(&mut self, data: V) -> Result<V, E> {
        self.effect("parse", vec![])?;
        Ok(data)
    }
    fn property(&mut self, v: &V, key: &'static str) -> Result<V, E> {
        Ok(v.get(key))
    }
    fn truthy(&mut self, v: &V) -> Result<bool, E> {
        Ok(v.truthy())
    }
    fn text(&mut self, v: &V) -> Result<Option<Vec<u16>>, E> {
        Ok(v.text())
    }
    fn array(&mut self, v: &V) -> Result<bool, E> {
        Ok(matches!(v, V::Array(_)))
    }
    fn iterate_values(&mut self, v: V) -> Result<Iter, E> {
        if let V::Array(a) = v {
            Ok(Iter { array: a, index: 0 })
        } else {
            Err(self.intrinsic())
        }
    }
    fn next_value(&mut self, it: &mut Iter) -> Result<Option<V>, E> {
        self.next_callback(it)
    }
    fn index(&mut self, v: &V, i: usize) -> Result<V, E> {
        Ok(v.values().get(i).cloned().unwrap_or(V::Undefined))
    }
    fn wrap(&mut self, v: V) -> Result<V, E> {
        Ok(array(vec![v]))
    }
    fn dispatch(&mut self, t: Vec<u16>, v: V, each: Option<bool>) -> Result<(), E> {
        let mut args = vec![V::Text(t), v];
        if let Some(each) = each {
            args.push(V::Bool(each));
        }
        self.effect("dispatch", args)
    }
    fn pong(&mut self) -> Result<(), E> {
        self.effect("pong", vec![])
    }
}
fn run(name: &str, count: usize) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/public-routing-oracle.cjs"
        ))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "PUBLIC actual source oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), count, "PUBLIC source case census");
    for (index, case) in cases.into_iter().enumerate() {
        let input = V::decode(&case["input"]);
        let mut host = Host::new(input.clone());
        let mut result = V::Undefined;
        let outcome = match name {
            "connect" => routing::connect_and_subscribe(&mut host),
            "open" => routing::on_open(&mut host),
            "subscribe" => routing::subscribe(&mut host),
            "warning" => routing::update_subscription_limit_warning(&mut host).and_then(|_| {
                if input.get("repeat").truthy() {
                    routing::update_subscription_limit_warning(&mut host)
                } else {
                    Ok(())
                }
            }),
            "ping" => routing::ping(&mut host).and_then(|_| {
                if input.get("repeat").truthy() {
                    routing::ping(&mut host)
                } else {
                    Ok(())
                }
            }),
            "dispatch" => {
                let topic = input.get("topic").text().unwrap_or(u("topic"));
                let payload = if matches!(input.get("payload"), V::Undefined | V::Null) {
                    object(vec![("payload", V::Num(1.0))])
                } else {
                    input.get("payload")
                };
                host.payload = Some(payload.clone());
                routing::dispatch_topic(&mut host, &topic, payload, input.get("each").truthy())
            }
            "interval" => {
                let results = Rc::new(RefCell::new(vec![]));
                result = V::Array(results.clone());
                let mut outcome = Ok(());
                for r in input.get("resolutions").values() {
                    match routing::get_kline_interval(&mut host, r) {
                        Ok(v) => results
                            .borrow_mut()
                            .push(v.map(V::Text).unwrap_or(V::Undefined)),
                        Err(e) => {
                            outcome = Err(e);
                            break;
                        }
                    }
                    if input.get("mutateTimeframes").truthy() {
                        host.frames = vec![(u("changed"), V::Num(1.0))]
                    }
                }
                outcome
            }
            "message" => {
                let frame = if !matches!(input.get("raw"), V::Undefined) {
                    V::Null
                } else {
                    input.get("frame")
                };
                routing::on_message(&mut host, frame)
            }
            _ => panic!(),
        };
        let thrown = match outcome {
            Ok(()) => V::Undefined,
            Err(e) => {
                if e.original {
                    assert!(
                        Rc::ptr_eq(&e.token, &host.error),
                        "PUBLIC native error identity"
                    );
                    V::Text(u("original"))
                } else {
                    V::Text(u("intrinsic"))
                }
            }
        };
        let observed = object(vec![
            ("effects", array(host.effects.clone())),
            ("result", result),
            ("thrown", thrown),
            ("warned", V::Bool(host.warned)),
            ("requestId", V::Num(host.request)),
        ])
        .obs();
        let expected = &case["expected"];
        let effects = host.effects.iter().map(V::obs).collect::<Vec<_>>();
        let expected_effects = expected["object"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e[0]["units"] == json!(u("effects")))
            .unwrap()[1]
            .as_array()
            .unwrap();
        for (a, b) in effects.iter().zip(expected_effects) {
            let label = host.effects[effects.iter().position(|e| e == a).unwrap()].values()[0]
                .text()
                .unwrap();
            assert_eq!(
                a,
                b,
                "PUBLIC native effect {} family {name} case {index}",
                String::from_utf16_lossy(&label)
            );
        }
        assert_eq!(
            observed, *expected,
            "PUBLIC native complete observation family {name} case {index}"
        );
    }
}
#[test]
fn actual_typescript_connect() {
    run("connect", 5)
}
#[test]
fn actual_typescript_open() {
    run("open", 5)
}
#[test]
fn actual_typescript_subscribe() {
    run("subscribe", 7)
}
#[test]
fn actual_typescript_warning() {
    run("warning", 8)
}
#[test]
fn actual_typescript_ping() {
    run("ping", 6)
}
#[test]
fn actual_typescript_dispatch() {
    run("dispatch", 8)
}
#[test]
fn actual_typescript_interval() {
    run("interval", 8)
}
#[test]
fn actual_typescript_message() {
    run("message", 15)
}
