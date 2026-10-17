use safe_cex_venue_bybit::hyro_private_session::{self as api, SessionFuture, SessionHost as Host};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::Future,
    pin::pin,
    process::Command,
    rc::Rc,
    task::{Context, Poll, Waker},
};
type Raw = Rc<Node>;
#[derive(Debug)]
enum Node {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Raw>>),
    Map(RefCell<Vec<(Raw, Raw)>>),
    Iterator {
        map: Raw,
        index: RefCell<usize>,
    },
    Promise(RefCell<Option<Result<Raw, Raw>>>),
    Object {
        fields: RefCell<Vec<(Vec<u16>, Raw)>>,
        prototype: Option<Raw>,
        tag: Option<String>,
        error: bool,
    },
    Function {
        owner: Raw,
        code: String,
        captures: Vec<Raw>,
    },
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: Vec<u16>) -> Raw {
    Rc::new(Node::Text(s))
}
fn raw(s: &str) -> Raw {
    text(u(s))
}
fn undef() -> Raw {
    Rc::new(Node::Undefined)
}
fn null() -> Raw {
    Rc::new(Node::Null)
}
fn number(n: f64) -> Raw {
    Rc::new(Node::Number(n))
}
fn boolean(b: bool) -> Raw {
    Rc::new(Node::Bool(b))
}
fn object(fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: None,
        tag: None,
        error: false,
    })
}
fn tagged(tag: &str, fields: Vec<(&str, Raw)>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (u(k), v)).collect()),
        prototype: None,
        tag: Some(tag.into()),
        error: false,
    })
}
fn array(v: Vec<Raw>) -> Raw {
    Rc::new(Node::Array(RefCell::new(v)))
}
fn func(owner: Raw, code: &str, captures: Vec<Raw>) -> Raw {
    Rc::new(Node::Function {
        owner,
        code: code.into(),
        captures,
    })
}
fn tag(v: &Raw) -> String {
    match v.as_ref() {
        Node::Object { tag: Some(s), .. } => s.clone(),
        _ => "this".into(),
    }
}
fn elements(v: &Raw) -> Vec<Raw> {
    let Node::Array(a) = v.as_ref() else {
        panic!("not array")
    };
    a.borrow().clone()
}
fn eq(a: &Raw, b: &Raw) -> bool {
    match (a.as_ref(), b.as_ref()) {
        (Node::Undefined, Node::Undefined) | (Node::Null, Node::Null) => true,
        (Node::Bool(a), Node::Bool(b)) => a == b,
        (Node::Number(a), Node::Number(b)) => a == b,
        (Node::Text(a), Node::Text(b)) => a == b,
        _ => Rc::ptr_eq(a, b),
    }
}
fn get_units(v: &Raw, key: &[u16]) -> Raw {
    match v.as_ref() {
        Node::Object {
            fields, prototype, ..
        } => fields
            .borrow()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(|| {
                prototype
                    .as_ref()
                    .map(|p| get_units(p, key))
                    .unwrap_or_else(undef)
            }),
        Node::Array(a) if key == u("length") => number(a.borrow().len() as f64),
        Node::Array(a) => String::from_utf16(key)
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|i| a.borrow().get(i).cloned())
            .unwrap_or_else(undef),
        Node::Map(m) if key == u("size") => number(m.borrow().len() as f64),
        _ => undef(),
    }
}
fn get(v: &Raw, key: &str) -> Raw {
    get_units(v, &u(key))
}
fn set(v: &Raw, key: &str, value: Raw) {
    let Node::Object { fields, .. } = v.as_ref() else {
        panic!("write nonobject")
    };
    let mut fields = fields.borrow_mut();
    if let Some((_, v)) = fields.iter_mut().find(|(k, _)| *k == u(key)) {
        *v = value
    } else {
        fields.push((u(key), value));
    }
}
fn numeric(v: &Raw) -> f64 {
    match v.as_ref() {
        Node::Number(n) => *n,
        Node::Null => 0.0,
        Node::Bool(b) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Node::Text(t) => String::from_utf16(t)
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(f64::NAN),
        _ => f64::NAN,
    }
}
fn js(v: &Raw) -> Vec<u16> {
    match v.as_ref() {
        Node::Text(t) => t.clone(),
        Node::Undefined => u("undefined"),
        Node::Null => u("null"),
        Node::Bool(b) => u(if *b { "true" } else { "false" }),
        Node::Number(n) => u(&safe_cex_num::js_number_string(*n)),
        Node::Object { error: true, .. } => u("Error: ")
            .into_iter()
            .chain(js(&get(v, "message")))
            .collect(),
        _ => u("[object Object]"),
    }
}
fn atom(v: &Raw) -> Value {
    match v.as_ref() {
        Node::Iterator { .. } | Node::Promise(_) => panic!("observer internal value"),
        Node::Undefined => json!({"u":true}),
        Node::Null => Value::Null,
        Node::Bool(b) => json!(b),
        Node::Number(n) => {
            json!({"n":format!("{:016x}",if n.is_nan(){0x7ff8000000000000}else{n.to_bits()})})
        }
        Node::Text(t) => json!({"s":t}),
        Node::Array(a) => json!(a.borrow().iter().map(atom).collect::<Vec<_>>()),
        Node::Map(m) => {
            json!({"map":m.borrow().iter().map(|(k,v)|json!([atom(k),atom(v)])).collect::<Vec<_>>()})
        }
        Node::Function {
            owner,
            code,
            captures,
        } => {
            json!({"fn":code,"owner":tag(owner),"captures":captures.iter().map(atom).collect::<Vec<_>>()})
        }
        Node::Object { tag: Some(tag), .. } => json!({"ref":tag}),
        Node::Object { error: true, .. } => json!({"error":true,"message":js(&get(v,"message"))}),
        Node::Object { fields, .. } => {
            json!({"o":fields.borrow().iter().map(|(k,v)|json!([k,atom(v)])).collect::<Vec<_>>()})
        }
    }
}
fn decode(v: &Value) -> Raw {
    if v.is_null() {
        return null();
    }
    if let Some(b) = v.as_bool() {
        return boolean(b);
    }
    if v["u"] == true {
        return undef();
    }
    if let Some(t) = v["s"].as_array() {
        return text(t.iter().map(|n| n.as_u64().unwrap() as u16).collect());
    }
    if let Some(n) = v["n"].as_str() {
        return number(f64::from_bits(u64::from_str_radix(n, 16).unwrap()));
    }
    if let Some(tag) = v["ref"].as_str() {
        return tagged(tag, vec![]);
    }
    if let Some(a) = v.as_array() {
        return array(a.iter().map(decode).collect());
    }
    if let Some(code) = v["fn"].as_str() {
        let owner = tagged(v["owner"].as_str().unwrap(), vec![]);
        let captures = v["captures"]
            .as_array()
            .unwrap()
            .iter()
            .map(decode)
            .collect();
        return func(owner, code, captures);
    }
    let fields = v["o"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (serde_json::from_value(p[0].clone()).unwrap(), decode(&p[1])))
        .collect();
    Rc::new(Node::Object {
        fields: RefCell::new(fields),
        prototype: None,
        tag: None,
        error: false,
    })
}
fn block_on<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = pin!(f);
    for _ in 0..64 {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(r) => return r,
            Poll::Pending => std::thread::yield_now(),
        }
    }
    panic!("native future did not settle within 64 polls")
}
struct Harness {
    profile: String,
    failure: String,
    counts: RefCell<Vec<(String, usize)>>,
    root: RefCell<Raw>,
    parent: Raw,
    socket: Raw,
    old_socket: Raw,
    global: Raw,
    primary: Raw,
    writes_enabled: RefCell<bool>,
    writes: RefCell<Vec<Value>>,
    defs: RefCell<Vec<Value>>,
    ops: RefCell<Vec<Value>>,
    timers: RefCell<Vec<(Raw, f64, bool)>>,
    ids: RefCell<usize>,
    parsed: Vec<Value>,
    parse_index: RefCell<usize>,
    payloads: RefCell<Vec<Raw>>,
    settlements: RefCell<Vec<Value>>,
    attempt: RefCell<usize>,
    prototype: Raw,
}
fn error(message: Vec<u16>) -> Raw {
    Rc::new(Node::Object {
        fields: RefCell::new(vec![(u("message"), text(message))]),
        prototype: None,
        tag: None,
        error: true,
    })
}
impl Harness {
    fn new(profile: &str, failure: &str, parsed: Vec<Value>) -> Self {
        let remote = if profile.starts_with("remote")
            || profile.contains("sign")
            || profile == "raw-error"
        {
            tagged(
                "remote",
                vec![(
                    "accountName",
                    text(u("acct-").into_iter().chain([0xd800]).collect()),
                )],
            )
        } else {
            undef()
        };
        Self {
            profile: profile.into(),
            failure: failure.into(),
            counts: RefCell::new(vec![]),
            root: RefCell::new(undef()),
            parent: tagged(
                "parent",
                vec![(
                    "options",
                    object(vec![
                        ("key", text(u("key-").into_iter().chain([0xd800]).collect())),
                        ("secret", raw("secret")),
                        ("remoteSigner", remote),
                    ]),
                )],
            ),
            socket: tagged("socket", vec![]),
            old_socket: tagged("old-socket", vec![]),
            global: tagged("global", vec![]),
            primary: error(u("primary-").into_iter().chain([0xd800]).collect()),
            writes_enabled: RefCell::new(false),
            writes: RefCell::new(vec![]),
            defs: RefCell::new(vec![]),
            ops: RefCell::new(vec![]),
            timers: RefCell::new(vec![]),
            ids: RefCell::new(0),
            parsed,
            parse_index: RefCell::new(0),
            payloads: RefCell::new(vec![]),
            settlements: RefCell::new(vec![]),
            attempt: RefCell::new(0),
            prototype: tagged("hyro-prototype", vec![]),
        }
    }
    fn touch(&self, name: &str) -> Result<(), Raw> {
        let mut counts = self.counts.borrow_mut();
        let n = if let Some((_, n)) = counts.iter_mut().find(|(k, _)| k == name) {
            let v = *n;
            *n += 1;
            v
        } else {
            counts.push((name.into(), 1));
            0
        };
        if self.failure == format!("{name}#{n}") {
            Err(self.primary.clone())
        } else {
            Ok(())
        }
    }
    fn rec(&self, name: &str, args: &[Raw]) -> Result<(), Raw> {
        self.touch(name)?;
        self.ops
            .borrow_mut()
            .push(json!({"name":name,"args":args.iter().map(atom).collect::<Vec<_>>()}));
        Ok(())
    }
    fn owner(&self) -> Raw {
        self.root.borrow().clone()
    }
    fn mutate(&self, phase: &str) -> Result<(), Raw> {
        let owner = self.owner();
        if self.profile == format!("dispose-{phase}") {
            self.set(&owner, "isDisposed", boolean(true))?;
        }
        if self.profile == format!("generation-{phase}") {
            self.set(
                &owner,
                "generation",
                number(numeric(&get(&owner, "generation")) + 1.0),
            )?;
        }
        if self.profile == format!("socket-{phase}") {
            self.set(&owner, "ws", self.old_socket.clone())?;
        }
        Ok(())
    }
    fn pending(&self, k: &str, op: &str) -> Raw {
        object(vec![
            ("op", raw(op)),
            ("timer", number(if k == "a" { 91.0 } else { 92.0 })),
            (
                "resolve",
                func(self.owner(), &format!("resolve:{k}"), vec![]),
            ),
            ("reject", func(self.owner(), &format!("reject:{k}"), vec![])),
        ])
    }
    fn map_set(&self, map: &Raw, key: Raw, value: Raw) {
        let Node::Map(entries) = map.as_ref() else {
            panic!("native map")
        };
        let mut entries = entries.borrow_mut();
        if let Some((_, v)) = entries.iter_mut().find(|(k, _)| eq(k, &key)) {
            *v = value;
        } else {
            entries.push((key, value));
        }
    }
    fn snapshot(&self) -> Value {
        let owner = self.owner();
        let map = get(&owner, "pendingAcks");
        let Node::Map(entries) = map.as_ref() else {
            panic!("map")
        };
        json!({"ready":atom(&get(&owner,"ready")),"started":atom(&get(&owner,"started")),"revision":atom(&get(&owner,"stateRevision")),"generation":atom(&get(&owner,"generation")),"last":atom(&get(&owner,"lastReceived")),"disposed":atom(&get(&owner,"isDisposed")),"map":entries.borrow().iter().map(|(k,v)|json!([js(k),{"op":atom(&get(v,"op")),"timer":atom(&get(v,"timer"))}])).collect::<Vec<_>>()})
    }
}
impl Host for Harness {
    type Value = Raw;
    type Error = Raw;
    fn get(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        if tag(v) == "parent" && k == "syncSocketHealthToStore" {
            if self.profile == "no-health" {
                return Ok(undef());
            }
            if self.profile == "null-health" {
                return Ok(null());
            }
        }
        let value = get(v, k);
        if !matches!(value.as_ref(), Node::Undefined) {
            return Ok(value);
        }
        let code = match (tag(v).as_str(), k) {
            ("this", "resetSession") => "reset-owned",
            ("this", "request") => "request-owned",
            ("parent", "syncSocketHealthToStore") => "health",
            ("parent", "observePrivateOrders") => "observe",
            ("parent", "reconcilePrivateState") => "reconcile",
            ("remote", "sign") => "sign",
            _ => return Ok(value),
        };
        Ok(func(v.clone(), code, vec![]))
    }
    fn optional(&self, v: &Raw, k: &str) -> Result<Raw, Raw> {
        self.get(v, k)
    }
    fn set(&self, v: &Raw, k: &str, value: Raw) -> Result<(), Raw> {
        if *self.writes_enabled.borrow() {
            self.touch(&format!("write:{k}"))?;
            self.writes
                .borrow_mut()
                .push(json!({"key":k,"value":atom(&value)}));
        }
        set(v, k, value);
        Ok(())
    }
    fn define(&self, v: &Raw, k: &str, value: Raw) -> Result<(), Raw> {
        self.touch(&format!("define:{k}"))?;
        self.defs
            .borrow_mut()
            .push(json!({"key":k,"value":atom(&value)}));
        set(v, k, value);
        Ok(())
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v.as_ref() {
            Node::Undefined | Node::Null => false,
            Node::Bool(b) => *b,
            Node::Number(n) => *n != 0.0 && !n.is_nan(),
            Node::Text(t) => !t.is_empty(),
            _ => true,
        }
    }
    fn equal(&self, a: &Raw, b: &Raw) -> bool {
        eq(a, b)
    }
    fn undefined(&self) -> Raw {
        undef()
    }
    fn literal(&self, s: &str) -> Raw {
        raw(s)
    }
    fn boolean(&self, b: bool) -> Raw {
        boolean(b)
    }
    fn number(&self, n: f64) -> Raw {
        number(n)
    }
    fn numeric(&self, v: &Raw) -> Result<f64, Raw> {
        Ok(numeric(v))
    }
    fn units(&self, v: &Raw) -> Result<Vec<u16>, Raw> {
        Ok(js(v))
    }
    fn text(&self, v: Vec<u16>) -> Raw {
        text(v)
    }
    fn object(&self, fields: Vec<(&'static str, Raw)>) -> Raw {
        object(fields)
    }
    fn array(&self, values: Vec<Raw>) -> Raw {
        array(values)
    }
    fn global(&self) -> Raw {
        self.global.clone()
    }
    fn select(&self, v: Raw, k: &str) -> Result<Raw, Raw> {
        let value = self.get(&v, k)?;
        if !matches!(value.as_ref(), Node::Undefined) {
            Ok(value)
        } else {
            Ok(func(v, k, vec![]))
        }
    }
    fn receiver_call(&self, _owner: Raw, callable: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        self.apply(callable, args)
    }
    fn apply(&self, selected: Raw, args: Vec<Raw>) -> Result<Raw, Raw> {
        let Node::Function {
            owner,
            code,
            captures,
        } = selected.as_ref()
        else {
            panic!("native noncallable {}", atom(&selected))
        };
        if let Node::Map(entries) = owner.as_ref() {
            return Ok(match code.as_str() {
                "values" => Rc::new(Node::Iterator {
                    map: owner.clone(),
                    index: RefCell::new(0),
                }),
                "get" => entries
                    .borrow()
                    .iter()
                    .find(|(k, _)| eq(k, &args[0]))
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(undef),
                "set" => {
                    self.map_set(owner, args[0].clone(), args[1].clone());
                    owner.clone()
                }
                "clear" => {
                    entries.borrow_mut().clear();
                    undef()
                }
                "delete" => {
                    let mut e = entries.borrow_mut();
                    let n = e.len();
                    e.retain(|(k, _)| !eq(k, &args[0]));
                    boolean(e.len() != n)
                }
                _ => panic!("map method {code}"),
            });
        }
        if let Node::Iterator { map, index } = owner.as_ref() {
            assert_eq!(code, "next");
            let Node::Map(entries) = map.as_ref() else {
                panic!()
            };
            let i = *index.borrow();
            *index.borrow_mut() += 1;
            return Ok(entries
                .borrow()
                .get(i)
                .map(|(_, v)| object(vec![("done", boolean(false)), ("value", v.clone())]))
                .unwrap_or_else(|| object(vec![("done", boolean(true))])));
        }
        if let Some(k) = code.strip_prefix("resolve:") {
            self.rec(code, &args)?;
            self.settlements
                .borrow_mut()
                .push(json!({"id":k,"ok":true}));
            return Ok(undef());
        }
        if let Some(k) = code.strip_prefix("reject:") {
            self.rec(code, &args)?;
            self.settlements.borrow_mut().push(json!({"id":k,"ok":false,"error":atom(&args[0]),"primary":Rc::ptr_eq(&args[0],&self.primary)}));
            if k == "a" && self.profile == "live-add" {
                self.map_set(
                    &get(owner, "pendingAcks"),
                    raw("late"),
                    self.pending("late", "auth"),
                );
            }
            return Ok(undef());
        }
        match code.as_str() {
            "reconnect" => {
                let result = block_on(api::send_reconnect_sequence(self, owner)).map(|()| undef());
                Ok(Rc::new(Node::Promise(RefCell::new(Some(result)))))
            }
            "reset-owned" => {
                api::reset_session(self, owner)?;
                Ok(undef())
            }
            "request-owned" => api::request(self, owner, args[0].clone(), args[1].clone()),
            "current" => api::is_current(self, owner, captures),
            "order" | "connect" | "close" | "ping" | "message" | "timeout" => api::dispatch(
                self,
                owner,
                api::Callback::from_name(code).expect("source callback selector"),
                captures,
                args,
            ),
            "promise-resolve" | "promise-reject" => {
                let Node::Promise(state) = captures[0].as_ref() else {
                    panic!()
                };
                if state.borrow().is_none() {
                    *state.borrow_mut() = Some(if code == "promise-resolve" {
                        Ok(undef())
                    } else {
                        Err(args[0].clone())
                    });
                }
                Ok(undef())
            }
            "nanoid" => {
                self.rec("id", &[])?;
                *self.ids.borrow_mut() += 1;
                Ok(text(
                    u(&format!("id-{}-", *self.ids.borrow()))
                        .into_iter()
                        .chain([0xd800])
                        .collect(),
                ))
            }
            "setTimeout" => {
                self.rec("timer", &[args[1].clone()])?;
                self.timers
                    .borrow_mut()
                    .push((args[0].clone(), numeric(&args[1]), true));
                Ok(number(self.timers.borrow().len() as f64))
            }
            "clearTimeout" => {
                self.rec("clear", &args)?;
                let index = numeric(&args[0]) as usize;
                if index > 0 {
                    if let Some((_, _, active)) = self.timers.borrow_mut().get_mut(index - 1) {
                        *active = false;
                    }
                }
                Ok(undef())
            }
            "jsonParse" => {
                self.rec("parse", &args)?;
                let i = *self.parse_index.borrow();
                *self.parse_index.borrow_mut() += 1;
                Ok(decode(&self.parsed[i]))
            }
            "stringify" => {
                self.payloads.borrow_mut().push(args[0].clone());
                Ok(raw(&wire(&args[0])))
            }
            "getCurrentTimeMs" => {
                self.rec("clock", &[])?;
                Ok(number(123456.0))
            }
            "createHmac" => {
                self.rec("hmac", &args)?;
                Ok(tagged("hmac", vec![]))
            }
            "update" => {
                self.rec("hmac-update", &args)?;
                Ok(tagged("hmac", vec![]))
            }
            "digest" => {
                self.rec("hmac-digest", &args)?;
                Ok(raw("signed"))
            }
            "request-dependency" => {
                let operation = js(&args[0]);
                let native_args = elements(&args[1]);
                if operation == u("auth") {
                    assert_eq!(native_args.len(), 3, "reconnect_native_auth_count");
                    assert_eq!(
                        js(&native_args[0]),
                        u("key-").into_iter().chain([0xd800]).collect::<Vec<_>>(),
                        "reconnect_native_auth_key"
                    );
                    assert_eq!(
                        numeric(&native_args[1]).to_bits(),
                        133456.0f64.to_bits(),
                        "reconnect_native_expiry"
                    );
                    assert_eq!(
                        js(&native_args[2]),
                        if matches!(
                            get(&get(&self.parent, "options"), "remoteSigner").as_ref(),
                            Node::Undefined
                        ) {
                            u("signed")
                        } else {
                            u("signed-").into_iter().chain([0xd800]).collect()
                        },
                        "reconnect_native_signature"
                    );
                } else {
                    assert_eq!(
                        operation,
                        u("subscribe"),
                        "reconnect_native_subscription_operation"
                    );
                    assert_eq!(
                        native_args.iter().map(js).collect::<Vec<_>>(),
                        vec![u("order"), u("position")],
                        "reconnect_native_subscription_topics"
                    );
                }
                self.rec("request", &args)?;
                let phase = String::from_utf16(&js(&args[0])).unwrap();
                self.mutate(&phase)?;
                let result = if self.profile == format!("{phase}-error") {
                    Err(self.primary.clone())
                } else {
                    Ok(undef())
                };
                Ok(Rc::new(Node::Promise(RefCell::new(Some(result)))))
            }
            "sign" => {
                self.rec("sign", &args)?;
                self.mutate("sign")?;
                let result = match self.profile.as_str() {
                    "raw-error" => Err(text(u("raw-").into_iter().chain([0xd800]).collect())),
                    "sign-error" => Err(self.primary.clone()),
                    "missing-signature" => Ok(object(vec![])),
                    "remote-error" => Ok(object(vec![(
                        "error",
                        text(u("remote-").into_iter().chain([0xd800]).collect()),
                    )])),
                    _ => Ok(object(vec![(
                        "wsAuthSignature",
                        text(u("signed-").into_iter().chain([0xd800]).collect()),
                    )])),
                };
                Ok(Rc::new(Node::Promise(RefCell::new(Some(result)))))
            }
            "reconcile" => {
                let guard = self.apply(args[0].clone(), vec![])?;
                self.rec("reconcile", &[guard])?;
                let n = *self.attempt.borrow();
                *self.attempt.borrow_mut() += 1;
                self.mutate("reconcile")?;
                let result = if self.profile == "reconcile-error" {
                    Err(self.primary.clone())
                } else {
                    Ok(boolean(match self.profile.as_str() {
                        "unstable" => false,
                        "third" => n == 2,
                        "second" => n == 1,
                        _ => true,
                    }))
                };
                Ok(Rc::new(Node::Promise(RefCell::new(Some(result)))))
            }
            "track" => {
                self.rec("track", &args)?;
                self.set(owner, "lastReceived", number(88.0))?;
                Ok(undef())
            }
            "base-reconnect" => {
                self.rec("reconnect", &args)?;
                Ok(undef())
            }
            "observe" | "health" | "send" | "log" => {
                self.rec(code, &args)?;
                Ok(undef())
            }
            code if code.starts_with("base:") => {
                self.rec(code, &args)?;
                Ok(undef())
            }
            _ => panic!("native missing dependency {code}"),
        }
    }
    fn wait(&self, v: Raw) -> SessionFuture<'_, Raw, Raw> {
        Box::pin(async move {
            let mut first = true;
            std::future::poll_fn(|cx| {
                if first {
                    first = false;
                    cx.waker().wake_by_ref();
                    Poll::Pending
                } else {
                    Poll::Ready(())
                }
            })
            .await;
            if let Node::Promise(state) = v.as_ref() {
                state
                    .borrow()
                    .clone()
                    .expect("native dependency promise ready")
            } else {
                Ok(v)
            }
        })
    }
    fn intrinsic(&self, name: &str, _args: Vec<Raw>) -> Result<Raw, Raw> {
        Ok(match name {
            "new Map" => Rc::new(Node::Map(RefCell::new(vec![]))),
            "null" => null(),
            "JSON" => tagged("JSON", vec![]),
            "virtualClock" => tagged("clock", vec![]),
            _ => panic!("intrinsic {name}"),
        })
    }
    fn allocate(&self, parent: Raw) -> Result<Raw, Raw> {
        let owner = Rc::new(Node::Object {
            fields: RefCell::new(vec![]),
            prototype: Some(self.prototype.clone()),
            tag: Some("this".into()),
            error: false,
        });
        *self.root.borrow_mut() = owner.clone();
        self.rec("base", std::slice::from_ref(&parent))?;
        set(&owner, "parent", parent);
        set(&owner, "ws", self.socket.clone());
        set(&owner, "isDisposed", boolean(false));
        set(&owner, "lastReceived", number(77.0));
        for name in [
            "handleOrderTopic",
            "onMessage",
            "onClose",
            "connectAndSubscribe",
            "ping",
        ] {
            set(
                &owner,
                name,
                func(owner.clone(), &format!("base:{name}"), vec![]),
            );
        }
        for (key, code) in [
            ("trackLastReceived", "track"),
            ("sendWithLog", "send"),
            ("emitAndLogError", "log"),
            ("reconnect", "base-reconnect"),
        ] {
            set(&owner, key, func(owner.clone(), code, vec![]));
        }
        Ok(owner)
    }
    fn callback(&self, owner: Raw, code: &'static str, captures: Vec<Raw>) -> Result<Raw, Raw> {
        Ok(func(owner, code, captures))
    }
    fn promise(&self) -> Result<(Raw, Raw, Raw), Raw> {
        let promise = Rc::new(Node::Promise(RefCell::new(None)));
        Ok((
            promise.clone(),
            func(self.owner(), "promise-resolve", vec![promise.clone()]),
            func(self.owner(), "promise-reject", vec![promise]),
        ))
    }
    fn error(&self, message: Raw) -> Result<Raw, Raw> {
        Ok(error(js(&message)))
    }
    fn error_value(&self, error: Raw) -> Raw {
        error
    }
    fn normalize_error(&self, e: Raw) -> Result<Raw, Raw> {
        Ok(if matches!(e.as_ref(), Node::Object { error: true, .. }) {
            e
        } else {
            error(js(&e))
        })
    }
}
fn wire(v: &Raw) -> String {
    match v.as_ref() {
        Node::Undefined | Node::Null => "null".into(),
        Node::Bool(b) => b.to_string(),
        Node::Number(n) => {
            if n.is_finite() {
                safe_cex_num::js_number_string(*n)
            } else {
                "null".into()
            }
        }
        Node::Text(t) => quote(t),
        Node::Array(a) => format!(
            "[{}]",
            a.borrow().iter().map(wire).collect::<Vec<_>>().join(",")
        ),
        Node::Object { fields, .. } => format!(
            "{{{}}}",
            fields
                .borrow()
                .iter()
                .filter(|(_, v)| !matches!(v.as_ref(), Node::Undefined))
                .map(|(k, v)| format!("{}:{}", quote(k), wire(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => panic!("wire value"),
    }
}
fn quote(t: &[u16]) -> String {
    let mut s = String::from("\"");
    for ch in char::decode_utf16(t.iter().copied()) {
        match ch {
            Ok(c) => {
                let encoded = serde_json::to_string(&c.to_string()).unwrap();
                s.push_str(&encoded[1..encoded.len() - 1]);
            }
            Err(e) => s.push_str(&format!("\\u{:04x}", e.unpaired_surrogate())),
        }
    }
    s.push('"');
    s
}
fn outcome(result: Result<(), Raw>, h: &Harness) -> Value {
    match result {
        Ok(()) => json!({"ok":true}),
        Err(e) => json!({"ok":false,"error":atom(&e),"primary":Rc::ptr_eq(&e,&h.primary)}),
    }
}
fn invoke(h: &Harness, name: &str, args: Vec<Raw>) -> Result<(), Raw> {
    h.apply(h.get(&h.owner(), name)?, args)?;
    Ok(())
}
#[test]
fn hyro_private_session_source_parity() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/hyro-private-session-fixtures.cjs"
        ))
        .output()
        .expect("source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Value = serde_json::from_slice(&output.stdout).expect("UTF16 observer receipts");
    assert_eq!(
        cases.as_array().unwrap().len(),
        439,
        "independent source case floor"
    );
    let mut supplemental = Vec::new();
    for row in cases.as_array().unwrap() {
        let kind = row["kind"].as_str().unwrap();
        let profile = row["profile"].as_str().unwrap();
        let failure = row["failure"].as_str().unwrap();
        let h = Harness::new(
            profile,
            failure,
            row["parsed"].as_array().cloned().unwrap_or_default(),
        );
        let allocation = api::allocate_session(&h, h.parent.clone());
        let construction = allocation.and_then(|owner| {
            assert_eq!(
                atom(&get(&owner, "ready")),
                json!(false),
                "class_native_initial_ready"
            );
            api::constructor(&h, &owner)?;
            Ok(owner)
        });
        let owner = match construction {
            Ok(owner) => owner,
            Err(e) => {
                assert_eq!(
                    json!({"ok":false,"primary":Rc::ptr_eq(&e,&h.primary),"error":atom(&e)}),
                    row["result"],
                    "native allocation failure"
                );
                supplemental.push((
                    json!(*h.defs.borrow()),
                    row["defs"].clone(),
                    String::from("allocation definitions"),
                ));
                supplemental.push((
                    json!(*h.ops.borrow()),
                    row["ops"].clone(),
                    String::from("allocation trace"),
                ));
                continue;
            }
        };
        assert_eq!(
            json!({"ready":atom(&get(&owner,"ready")),"started":atom(&get(&owner,"started")),"revision":atom(&get(&owner,"stateRevision")),"generation":atom(&get(&owner,"generation")),"offline":atom(&get(&owner,"recoversAfterOffline")),"map":matches!(get(&owner,"pendingAcks").as_ref(),Node::Map(_))}),
            row["initial"],
            "native initial fields"
        );
        *h.writes_enabled.borrow_mut() = true;
        let mut snapshots = vec![];
        let mut request_states = vec![];
        let actual = (|| -> Result<Value, Raw> {
            match kind {
                "class" => {
                    *h.writes_enabled.borrow_mut() = false;
                    let second = api::allocate_session(&h, h.parent.clone())?;
                    api::constructor(&h, &second)?;
                    *h.root.borrow_mut() = owner.clone();
                    assert!(
                        !Rc::ptr_eq(&get(&second, "pendingAcks"), &get(&owner, "pendingAcks")),
                        "class_native_fresh_map"
                    );
                    for name in [
                        "sendReconnectSequence",
                        "handleOrderTopic",
                        "connectAndSubscribe",
                        "onClose",
                        "ping",
                        "onMessage",
                    ] {
                        let first = get(&owner, name);
                        assert!(
                            !Rc::ptr_eq(&first, &get(&second, name)),
                            "class_native_fresh_callback"
                        );
                        let Node::Function { owner: lexical, .. } = first.as_ref() else {
                            panic!("bound callback")
                        };
                        assert!(
                            Rc::ptr_eq(lexical, &owner),
                            "class_native_callback_receiver"
                        );
                    }
                    let Node::Object {
                        prototype: Some(p), ..
                    } = owner.as_ref()
                    else {
                        panic!("nominal prototype")
                    };
                    assert!(Rc::ptr_eq(p, &h.prototype), "class_native_prototype");
                    Ok(json!({"ok":true,"fresh":true,"prototype":true}))
                }
                "accessors" => {
                    h.set(&owner, "started", boolean(profile != "not-started"))?;
                    h.set(&owner, "ready", boolean(profile == "ready"))?;
                    h.set(
                        &owner,
                        "generation",
                        number(match profile {
                            "nan" => f64::NAN,
                            "negative-zero" => -0.0,
                            _ => 19.0,
                        }),
                    )?;
                    let version = api::session_version(&h, &owner)?;
                    assert_eq!(
                        numeric(&version).to_bits(),
                        numeric(&get(&owner, "generation")).to_bits(),
                        "version_native_generation"
                    );
                    let complete = api::is_recovery_complete(&h, &owner)?;
                    assert_eq!(
                        atom(&complete),
                        json!(profile == "not-started" || profile == "ready"),
                        "recovery_native_complete"
                    );
                    let resync = api::resyncs_private_state_on_reopen(&h);
                    assert_eq!(atom(&resync), json!(false), "resync_native_false");
                    Ok(
                        json!({"ok":true,"version":atom(&version),"complete":atom(&complete),"resync":atom(&resync)}),
                    )
                }
                "reset" => {
                    h.set(&owner, "ready", boolean(true))?;
                    h.set(
                        &owner,
                        "generation",
                        number(if profile == "nan" { f64::NAN } else { 4.0 }),
                    )?;
                    h.map_set(
                        &get(&owner, "pendingAcks"),
                        raw("a"),
                        h.pending("a", "auth"),
                    );
                    h.map_set(
                        &get(&owner, "pendingAcks"),
                        raw("b"),
                        h.pending("b", "subscribe"),
                    );
                    let result = api::reset_session(&h, &owner);
                    if failure == "none" {
                        assert_eq!(
                            atom(&get(&owner, "ready")),
                            json!(false),
                            "reset_native_ready_false"
                        );
                        assert_eq!(
                            h.settlements.borrow().len(),
                            if profile == "live-add" { 3 } else { 2 },
                            "reset_native_live_rejections"
                        );
                    }
                    Ok(outcome(result, &h))
                }
                "wrapper" => {
                    let (method, state) = profile.split_once(':').unwrap();
                    h.set(&owner, "ready", boolean(state == "ready"))?;
                    if state == "disposed" {
                        h.set(&owner, "isDisposed", boolean(true))?;
                    }
                    let args = if method == "handleOrderTopic" {
                        vec![array(vec![object(vec![(
                            "orderLinkId",
                            text(u("x-").into_iter().chain([0xd800]).collect()),
                        )])])]
                    } else if method == "connectAndSubscribe" || method == "ping" {
                        vec![raw("ignored")]
                    } else {
                        let target = if state == "stale" {
                            h.old_socket.clone()
                        } else if state == "no-target" {
                            undef()
                        } else {
                            h.socket.clone()
                        };
                        let message = match state {
                            "mismatch" => {
                                object(vec![("op", raw("subscribe")), ("req_id", raw("a"))])
                            }
                            "topic-only" => object(vec![("topic", raw("order"))]),
                            "unknown" => {
                                object(vec![("op", raw("auth")), ("req_id", raw("unknown"))])
                            }
                            _ => object(vec![
                                ("topic", raw("order")),
                                ("op", raw("auth")),
                                ("req_id", raw("a")),
                                (
                                    "success",
                                    if state == "truthy" {
                                        number(1.0)
                                    } else {
                                        boolean(state != "reject")
                                    },
                                ),
                                ("ret_msg", raw("")),
                            ]),
                        };
                        h.map_set(
                            &get(&owner, "pendingAcks"),
                            raw("a"),
                            h.pending("a", "auth"),
                        );
                        vec![object(vec![
                            ("currentTarget", target),
                            (
                                "data",
                                raw(&if state == "invalid" {
                                    String::from("invalid")
                                } else {
                                    wire(&message)
                                }),
                            ),
                        ])]
                    };
                    let result = invoke(&h, method, args);
                    if profile == "handleOrderTopic:basic" && failure == "none" {
                        assert!(
                            h.ops.borrow().iter().any(|v| v["name"] == "observe"),
                            "constructor_native_order_observer"
                        );
                    }
                    Ok(outcome(result, &h))
                }
                "request" => {
                    let promise = api::request(
                        &h,
                        &owner,
                        raw("auth"),
                        array(vec![
                            text(u("key-").into_iter().chain([0xd800]).collect()),
                            number(-0.0),
                            null(),
                        ]),
                    )?;
                    for payload in h.payloads.borrow().iter() {
                        assert_eq!(
                            js(&get(payload, "op")),
                            u("auth"),
                            "request_native_operation"
                        );
                        assert_eq!(
                            js(&get(payload, "req_id")),
                            u("id-1-").into_iter().chain([0xd800]).collect::<Vec<_>>(),
                            "request_native_id"
                        );
                        let args = elements(&get(payload, "args"));
                        assert_eq!(args.len(), 3, "request_native_argument_count");
                        assert_eq!(
                            js(&args[0]),
                            u("key-").into_iter().chain([0xd800]).collect::<Vec<_>>(),
                            "request_native_key"
                        );
                        assert_eq!(
                            numeric(&args[1]).to_bits(),
                            (-0.0f64).to_bits(),
                            "request_native_argument_bits"
                        );
                        assert!(
                            matches!(args[2].as_ref(), Node::Null),
                            "request_native_null_argument"
                        );
                    }
                    let read = || {
                        let Node::Promise(s) = promise.as_ref() else {
                            panic!("promise")
                        };
                        s.borrow().clone()
                    };
                    snapshots.push(h.snapshot());
                    let before = read();
                    if profile == "timeout" {
                        let timers = h.timers.borrow().clone();
                        for (callback, _, active) in timers {
                            if active {
                                h.apply(callback, vec![])?;
                            }
                        }
                    }
                    if profile == "ack" {
                        invoke(
                            &h,
                            "onMessage",
                            vec![object(vec![
                                ("currentTarget", h.socket.clone()),
                                (
                                    "data",
                                    raw(&wire(&object(vec![
                                        ("op", raw("auth")),
                                        (
                                            "req_id",
                                            text(u("id-1-").into_iter().chain([0xd800]).collect()),
                                        ),
                                        ("success", boolean(true)),
                                    ]))),
                                ),
                            ])],
                        )?;
                    }
                    if profile == "reset" {
                        api::reset_session(&h, &owner)?;
                    }
                    if failure == "none" {
                        assert_eq!(
                            h.timers.borrow()[0].1.to_bits(),
                            10_000.0f64.to_bits(),
                            "request_native_timeout_delay"
                        );
                    }
                    let state = before.or_else(read);
                    if let Some(state) = state {
                        request_states.push(outcome(state.map(|_| ()), &h));
                    }
                    Ok(json!({"ok":true,"settled":!request_states.is_empty()}))
                }
                "reconnect" => {
                    set(
                        &owner,
                        "request",
                        func(owner.clone(), "request-dependency", vec![]),
                    );
                    let result = h
                        .apply(h.get(&owner, "sendReconnectSequence")?, vec![])
                        .and_then(|promise| block_on(h.wait(promise)))
                        .map(|_| ());
                    if failure == "none" && profile == "basic" {
                        assert_eq!(
                            atom(&get(&owner, "ready")),
                            json!(true),
                            "reconnect_native_ready"
                        );
                        assert_eq!(*h.attempt.borrow(), 1, "reconnect_native_attempt_count");
                    }
                    if failure == "none" && profile == "unstable" {
                        assert_eq!(*h.attempt.borrow(), 3, "reconnect_native_attempt_limit");
                    }
                    Ok(outcome(result, &h))
                }
                _ => panic!("unknown source kind"),
            }
        })();
        let actual = match actual {
            Ok(value) => value,
            Err(e) => outcome(Err(e), &h),
        };
        let context = format!("{kind}/{profile}/{failure}");
        assert_eq!(h.snapshot(), row["final"], "native session state {context}");
        assert_eq!(
            *h.writes.borrow(),
            row["writes"].as_array().unwrap().clone(),
            "native writes {context}"
        );
        assert_eq!(
            *h.settlements.borrow(),
            row["settlements"].as_array().unwrap().clone(),
            "native pending resolutions {context}"
        );
        assert_eq!(
            request_states,
            row["requestStates"].as_array().unwrap().clone(),
            "native request outcomes {context}"
        );
        assert_eq!(
            snapshots,
            row["snapshots"].as_array().unwrap().clone(),
            "native request publication {context}"
        );
        assert_eq!(h.timers.borrow().iter().enumerate().map(|(i,(_,delay,active))|json!({"id":i+1,"delay":format!("{:016x}",delay.to_bits()),"active":active})).collect::<Vec<_>>(),row["timers"].as_array().unwrap().clone(),"native timer state {context}");
        assert_eq!(actual, row["result"], "native result {context}");
        supplemental.push((
            json!(*h.defs.borrow()),
            row["defs"].clone(),
            format!("definitions {context}"),
        ));
        supplemental.push((
            json!(*h.ops.borrow()),
            row["ops"].clone(),
            format!("dependency trace {context}"),
        ));
    }
    for (actual, expected, label) in supplemental {
        assert_eq!(actual, expected, "supplemental {label}");
    }
}
