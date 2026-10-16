use safe_cex_venue_mexc::{
    private_session::*,
    public_subscriptions::{ConnectHost, DialRequest, Runtime},
};
use serde_json::Value as J;
use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll},
};
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
#[derive(Clone, Debug)]
enum V {
    Undefined,
    Null,
    Text(Vec<u16>),
    Number(f64),
    Bool(bool),
    Symbol,
    Opaque(Rc<u32>),
    Object(Rc<Object>),
    Array(Rc<Array>),
}
#[derive(Debug)]
struct Object {
    id: u32,
    observed: bool,
    fields: RefCell<Vec<(String, V)>>,
}
#[derive(Debug)]
struct Array {
    id: u32,
    observed: bool,
    items: Vec<V>,
}
fn text(s: &str) -> V {
    V::Text(u(s))
}
fn opaque(id: u32) -> V {
    V::Opaque(Rc::new(id))
}
fn obj(id: u32, observed: bool, fields: Vec<(&str, V)>) -> V {
    V::Object(Rc::new(Object {
        id,
        observed,
        fields: RefCell::new(fields.into_iter().map(|(k, v)| (k.into(), v)).collect()),
    }))
}
fn array(id: u32, items: Vec<V>) -> V {
    V::Array(Rc::new(Array {
        id,
        observed: true,
        items,
    }))
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undefined, V::Undefined) | (V::Null, V::Null) | (V::Symbol, V::Symbol) => true,
        (V::Text(a), V::Text(b)) => a == b,
        (V::Number(a), V::Number(b)) => a.to_bits() == b.to_bits(),
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::Opaque(a), V::Opaque(b)) => Rc::ptr_eq(a, b),
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        (V::Array(a), V::Array(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
#[derive(Clone, Debug)]
enum S {
    Undefined,
    Null,
    Text(Vec<u16>),
    Number(u64),
    Nan,
    Bool(bool),
    Symbol,
    Opaque(u32),
    Object(u32, Vec<(String, S)>),
    Array(u32, Vec<S>),
}
fn snapshot(v: &V) -> S {
    match v {
        V::Undefined => S::Undefined,
        V::Null => S::Null,
        V::Text(t) => S::Text(t.clone()),
        V::Number(n) if n.is_nan() => S::Nan,
        V::Number(n) => S::Number(n.to_bits()),
        V::Bool(b) => S::Bool(*b),
        V::Symbol => S::Symbol,
        V::Opaque(id) => S::Opaque(**id),
        V::Object(o) => S::Object(
            o.id,
            o.fields
                .borrow()
                .iter()
                .map(|(k, v)| (k.clone(), snapshot(v)))
                .collect(),
        ),
        V::Array(a) => S::Array(a.id, a.items.iter().map(snapshot).collect()),
    }
}
#[derive(Debug)]
struct Failure {
    name: &'static str,
    message: Vec<u16>,
    marker: Option<u32>,
}
type E = Rc<Failure>;
fn error(name: &'static str, message: &str) -> E {
    Rc::new(Failure {
        name,
        message: u(message),
        marker: None,
    })
}
#[derive(Clone, Debug)]
enum D {
    Value(S),
    Units(Vec<u16>),
    Int(u32),
    MaybeInt(Option<u32>),
    Bool(bool),
    Word(&'static str),
    Headers(Vec<(&'static str, Vec<u16>)>),
}
#[derive(Clone, Debug)]
struct Event {
    tag: String,
    fields: Vec<(&'static str, D)>,
}
struct Operation {
    value: RefCell<Option<Result<V, E>>>,
    planned: Result<V, E>,
    then_stage: Cell<u8>,
    then_hook: Option<Rc<dyn Fn() -> Result<V, E>>>,
}
impl Completion<V, E> for Operation {
    fn poll(&self, cx: &mut Context<'_>) -> Poll<Result<V, E>> {
        if let Some(hook) = &self.then_hook {
            if self.then_stage.get() == 0 {
                self.then_stage.set(1);
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            if self.then_stage.get() == 1 {
                self.then_stage.set(2);
                *self.value.borrow_mut() = Some(hook());
            }
        }
        self.value
            .borrow()
            .clone()
            .map_or(Poll::Pending, Poll::Ready)
    }
}
type Task = Pin<Box<dyn Future<Output = ()>>>;
type TimerCallback = Rc<dyn Fn() -> MethodObserver<V, E>>;
struct Host {
    name: &'static str,
    i: usize,
    original: E,
    trace: Rc<RefCell<Vec<Event>>>,
    disposed: Cell<bool>,
    socket: Cell<Option<u32>>,
    create_id: Cell<u32>,
    send_id: Cell<u32>,
    emit_id: Cell<u32>,
    sign_reads: Cell<usize>,
    error_reads: Cell<usize>,
    flag_reads: Cell<usize>,
    flag: Cell<bool>,
    tasks: RefCell<Vec<Task>>,
    operations: RefCell<Vec<Rc<Operation>>>,
    raw: V,
    signer: V,
    sign_result: V,
    thenable: V,
    options: V,
    balance: RefCell<V>,
    timer: Cell<Option<u32>>,
    timer_callback: RefCell<Option<TimerCallback>>,
    captured: RefCell<Option<DiagnosticOptions<V, E>>>,
    ignored: RefCell<Vec<E>>,
}
impl Host {
    fn new(name: &'static str, i: usize) -> Rc<Self> {
        Rc::new(Self {
            name,
            i,
            original: Rc::new(Failure {
                name: "Error",
                message: u("boundary failure"),
                marker: Some(47),
            }),
            trace: Rc::new(RefCell::new(vec![])),
            disposed: Cell::new(i == 1),
            socket: Cell::new(if i == 2 { Some(99) } else { None }),
            create_id: Cell::new(61),
            send_id: Cell::new(31),
            emit_id: Cell::new(37),
            sign_reads: Cell::new(0),
            error_reads: Cell::new(0),
            flag_reads: Cell::new(0),
            flag: Cell::new(false),
            tasks: RefCell::new(vec![]),
            operations: RefCell::new(vec![]),
            raw: opaque(83),
            signer: opaque(41),
            sign_result: opaque(52),
            thenable: opaque(67),
            options: opaque(61),
            balance: RefCell::new(opaque(85)),
            timer: Cell::new(if i == 2 { Some(66) } else { None }),
            timer_callback: RefCell::new(None),
            captured: RefCell::new(None),
            ignored: RefCell::new(vec![]),
        })
    }
    fn failure_point(&self) -> &'static str {
        match (self.name, self.i) {
            ("connectAndSubscribe", 4) => "create",
            ("connectAndSubscribe", 5) => "add:message",
            ("connectAndSubscribe", 11) => "listener:close",
            ("onOpen", 4) => "heartbeat",
            ("onOpen", 5) => "login",
            ("onOpen", 6) => "snapshot",
            ("onOpen", 7) => "reconnected",
            ("sendLogin", 4) => "key",
            ("sendLogin", 5) => "signature:child",
            ("sendLogin", 11) => "send:login",
            ("createLoginSignature", 5) => "sign",
            ("createLoginSignature", 8) => "nanoid",
            ("sendWithDiagnostics", 4) => "send",
            ("sendWithDiagnostics", 5) => "emit:send",
            ("sendWithDiagnostics", 7) => "redact",
            ("sendWithDiagnostics", 10) => "field:redactAuth",
            ("emitReceiveDiagnostics", 5) => "emit:receive",
            ("redactAuthFrame", 4) => "parse",
            ("redactAuthFrame", 5) => "stringify",
            ("throttledFetchBalance", 4) => "clear",
            ("throttledFetchBalance", 3) => "set",
            ("throttledFetchBalance", 6) => "fetch",
            ("throttledFetchBalance", 7) => "update",
            _ => "",
        }
    }
    fn hit(&self, tag: impl Into<String>, fields: Vec<(&'static str, D)>) -> Result<(), E> {
        let tag = tag.into();
        let throws = tag == self.failure_point();
        self.trace.borrow_mut().push(Event { tag, fields });
        if throws {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn tag(&self, tag: &str) -> Result<(), E> {
        self.hit(tag, vec![])
    }
    fn key_value(&self) -> V {
        if self.name == "sendLogin" {
            if self.i == 2 {
                V::Null
            } else if self.i == 3 {
                V::Number(17.0)
            } else {
                V::Text([u(" \tAPI"), vec![0xd800], u(" \u{feff}")].concat())
            }
        } else {
            match self.i {
                3 => text(""),
                9 => V::Number(0.0),
                10 => V::Symbol,
                _ => text("api"),
            }
        }
    }
    fn get_key(&self) -> Result<V, E> {
        let v = self.key_value();
        self.hit("key", vec![("value", D::Value(snapshot(&v)))])?;
        Ok(v)
    }
    fn operation(&self, planned: Result<V, E>, ready: bool) -> Rc<Operation> {
        let op = Rc::new(Operation {
            value: RefCell::new(if ready { Some(planned.clone()) } else { None }),
            planned,
            then_stage: Cell::new(0),
            then_hook: None,
        });
        self.operations.borrow_mut().push(op.clone());
        op
    }
    fn release(&self) {
        for op in self.operations.borrow().iter() {
            if op.then_hook.is_none() {
                *op.value.borrow_mut() = Some(op.planned.clone());
            }
        }
    }
    fn drain(&self) {
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        for _ in 0..4 {
            let current = std::mem::take(&mut *self.tasks.borrow_mut());
            for mut task in current {
                if task.as_mut().poll(&mut cx).is_pending() {
                    self.tasks.borrow_mut().push(task);
                }
            }
        }
        assert!(
            self.tasks.borrow().is_empty(),
            "native tasks settle within four controlled turns"
        );
    }
    fn disposed_value(&self) -> bool {
        let v = self.disposed.get();
        self.hit("disposed", vec![("value", D::Bool(v))]).unwrap();
        v
    }
    fn encode(&self, value: &V) -> Result<Vec<u16>, E> {
        Ok(match value {
            V::Undefined | V::Symbol => u("null"),
            V::Null => u("null"),
            V::Bool(b) => u(if *b { "true" } else { "false" }),
            V::Number(n) => u(if !n.is_finite() {
                "null"
            } else {
                return Ok(u(&safe_cex_num::js_number_string(*n)));
            }),
            V::Text(t) => quote(t),
            V::Opaque(_) => u("{}"),
            V::Array(a) => {
                let mut r = vec![91];
                for (j, v) in a.items.iter().enumerate() {
                    if j > 0 {
                        r.push(44)
                    }
                    r.extend(self.encode(v)?);
                }
                r.push(93);
                r
            }
            V::Object(o) => {
                let keys = o
                    .fields
                    .borrow()
                    .iter()
                    .map(|(k, _)| k.clone())
                    .collect::<Vec<_>>();
                let mut r = vec![123];
                let mut count = 0;
                for k in keys {
                    let v = self.field(value, &k)?;
                    if matches!(v, V::Undefined | V::Symbol) {
                        continue;
                    }
                    if count > 0 {
                        r.push(44)
                    }
                    count += 1;
                    r.extend(quote(&u(&k)));
                    r.push(58);
                    r.extend(self.encode(&v)?);
                }
                r.push(125);
                r
            }
        })
    }
}
fn quote(s: &[u16]) -> Vec<u16> {
    let mut r = vec![34];
    let mut i = 0;
    while i < s.len() {
        let c = s[i];
        match c {
            34 => r.extend(u("\\\"")),
            92 => r.extend(u("\\\\")),
            8 => r.extend(u("\\b")),
            9 => r.extend(u("\\t")),
            10 => r.extend(u("\\n")),
            12 => r.extend(u("\\f")),
            13 => r.extend(u("\\r")),
            0..=31 => r.extend(u(&format!("\\u{c:04x}"))),
            0xd800..=0xdbff if i + 1 < s.len() && (0xdc00..=0xdfff).contains(&s[i + 1]) => {
                r.push(c);
                i += 1;
                r.push(s[i]);
            }
            0xd800..=0xdfff => r.extend(u(&format!("\\u{c:04x}"))),
            _ => r.push(c),
        }
        i += 1;
    }
    r.push(34);
    r
}
impl Runtime for Host {
    type Value = V;
    type Error = E;
    fn undefined(&self) -> V {
        V::Undefined
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::Undefined | V::Null => false,
            V::Text(t) => !t.is_empty(),
            V::Bool(b) => *b,
            V::Number(n) => *n != 0.0 && !n.is_nan(),
            _ => true,
        }
    }
    fn same_value(&self, a: &V, b: &V) -> bool {
        same(a, b)
    }
    fn is_text(&self, v: &V, s: &str) -> bool {
        matches!(v,V::Text(t) if *t==u(s))
    }
    fn field(&self, v: &V, k: &str) -> Result<V, E> {
        match v {
            V::Null | V::Undefined => Err(error(
                "TypeError",
                &format!(
                    "Cannot read properties of {} (reading '{k}')",
                    if matches!(v, V::Null) {
                        "null"
                    } else {
                        "undefined"
                    }
                ),
            )),
            V::Opaque(id) => match (**id, k) {
                (41, "accountName") => {
                    self.tag("field:accountName")?;
                    Ok(V::Text(
                        [
                            u(if self.i == 7 { "CHANGED" } else { "account" }),
                            vec![if self.i == 7 { 0xdfff } else { 0xd800 }],
                        ]
                        .concat(),
                    ))
                }
                (52, "error") => {
                    self.tag("field:error")?;
                    let n = self.error_reads.replace(self.error_reads.get() + 1);
                    Ok(if self.i == 11 && n > 0 {
                        self.emit_id.set(99);
                        V::Null
                    } else if self.i == 3 || self.i == 11 {
                        V::Text([u("bad"), vec![0xd800]].concat())
                    } else if self.i == 4 {
                        V::Bool(false)
                    } else {
                        V::Undefined
                    })
                }
                (52, "wsAuthSignature") => {
                    self.tag("field:signature")?;
                    let n = self.sign_reads.replace(self.sign_reads.get() + 1);
                    Ok(if self.i == 12 {
                        self.thenable.clone()
                    } else if self.i == 4 {
                        V::Undefined
                    } else if self.i == 7 && n > 0 {
                        V::Text([u("changed"), vec![0xdfff]].concat())
                    } else {
                        text("remote-signature")
                    })
                }
                (61, "redactAuth") => {
                    self.tag("field:redactAuth")?;
                    let n = self.flag_reads.replace(self.flag_reads.get() + 1);
                    Ok(V::Bool(if matches!(self.i, 6 | 7) {
                        true
                    } else if self.i == 8 {
                        n % 2 == 1
                    } else {
                        self.flag.get()
                    }))
                }
                _ => Ok(V::Undefined),
            },
            V::Object(o) => {
                if o.observed && matches!(k, "method" | "param") {
                    self.tag(&format!("field:{k}"))?;
                }
                Ok(o.fields
                    .borrow()
                    .iter()
                    .find(|(key, _)| key == k)
                    .map(|(_, v)| v.clone())
                    .unwrap_or(V::Undefined))
            }
            V::Array(a) => {
                if a.observed && matches!(k, "method" | "param") {
                    self.tag(&format!("field:{k}"))?;
                }
                Ok(V::Undefined)
            }
            _ => Ok(V::Undefined),
        }
    }
    fn abstract_string(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(match v {
            V::Undefined => u("undefined"),
            V::Null => u("null"),
            V::Text(t) => t.clone(),
            V::Bool(b) => u(if *b { "true" } else { "false" }),
            V::Number(n) => u(&safe_cex_num::js_number_string(*n)),
            V::Symbol => {
                return Err(error(
                    "TypeError",
                    "Cannot convert a Symbol value to a string",
                ));
            }
            _ => u("[object Object]"),
        })
    }
    fn global_string(&self, v: &V) -> Result<Vec<u16>, E> {
        if matches!(v, V::Symbol) {
            Ok(u("Symbol(profile)"))
        } else {
            self.abstract_string(v)
        }
    }
    fn log(&self, _: &[u16], _: &'static str) -> Result<(), E> {
        panic!("no log call in selected bodies")
    }
    fn new_error(&self, t: Vec<u16>) -> E {
        Rc::new(Failure {
            name: "Error",
            message: t,
            marker: None,
        })
    }
}
impl SessionRuntime for Host {
    fn text(&self, t: Vec<u16>) -> V {
        V::Text(t)
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::Undefined | V::Null)
    }
    fn json_parse(&self, v: &V) -> Result<V, E> {
        self.hit("parse", vec![("data", D::Value(snapshot(v)))])?;
        assert!(
            matches!(v,V::Text(t)if *t==redact_input(self.i)),
            "independent native JSON parse input"
        );
        if self.i == 3 {
            return Err(error("SyntaxError", "fixture malformed JSON"));
        }
        Ok(parsed(self.i))
    }
    fn stringify(&self, v: &V) -> Result<Vec<u16>, E> {
        self.hit("stringify", vec![("value", D::Value(snapshot(v)))])?;
        self.encode(v)
    }
    fn spread(&self, v: &V) -> Result<V, E> {
        let rows = match v {
            V::Object(o) => o.fields.borrow().clone(),
            V::Array(a) => a
                .items
                .iter()
                .enumerate()
                .map(|(i, v)| (i.to_string(), v.clone()))
                .collect(),
            V::Text(t) => t
                .iter()
                .enumerate()
                .map(|(i, c)| (i.to_string(), V::Text(vec![*c])))
                .collect(),
            _ => vec![],
        };
        Ok(V::Object(Rc::new(Object {
            id: 100,
            observed: false,
            fields: RefCell::new(rows),
        })))
    }
    fn define_field(&self, v: &V, k: &str, field: V) -> Result<(), E> {
        let V::Object(o) = v else {
            panic!("new native literal object")
        };
        let mut rows = o.fields.borrow_mut();
        if let Some((_, v)) = rows.iter_mut().find(|(key, _)| key == k) {
            *v = field
        } else {
            rows.push((k.into(), field));
        }
        Ok(())
    }
    fn set_field(&self, v: &V, k: &str, field: V) -> Result<(), E> {
        if self.i == 12
            && matches!(v,V::Object(o) if o.id==100)
            && matches!(k, "apiKey" | "signature")
        {
            self.hit(
                format!("setter:{k}"),
                vec![("value", D::Value(snapshot(&field)))],
            )?;
            return Ok(());
        }
        if k == "param" {
            self.hit("set:param", vec![("value", D::Value(snapshot(&field)))])?;
        }
        self.define_field(v, k, field)
    }
}
impl ConnectHost for Host {
    type Socket = u32;
    type Listener = u32;
    fn disposed(&self) -> bool {
        self.disposed_value()
    }
    fn testnet(&self) -> Result<V, E> {
        let v = if self.i == 6 {
            text("false")
        } else {
            V::Bool(self.i % 2 == 1)
        };
        self.hit("testnet", vec![("value", D::Value(snapshot(&v)))])?;
        Ok(v)
    }
    fn url(&self, n: &'static str) -> Result<Vec<u16>, E> {
        self.tag(&format!("url:{n}"))?;
        if self.i == 8 {
            self.create_id.set(99);
        }
        Ok(if self.i == 8 {
            [u("wss://private/"), vec![0xd800]].concat()
        } else {
            u("wss://contract.mexc.com/edge")
        })
    }
    fn create(&self, _: DialRequest<V>) -> Result<u32, E> {
        panic!("private body must use previously captured create callee")
    }
    fn assign_socket(&self, s: u32) -> Result<(), E> {
        self.hit("assign", vec![("id", D::Int(s))])?;
        self.socket.set(Some(s));
        Ok(())
    }
    fn socket(&self) -> Result<u32, E> {
        let id = self.socket.get().unwrap();
        self.hit("socket", vec![("id", D::Int(id))])?;
        Ok(id)
    }
    fn listener(&self, event: &'static str) -> Result<u32, E> {
        let id = match event {
            "open" => 11,
            "message" => 13,
            "close" => 17,
            _ => unreachable!(),
        };
        self.hit(format!("listener:{event}"), vec![("id", D::Int(id))])?;
        if self.i == 7 && event == "open" {
            self.socket.set(Some(91));
        }
        Ok(id)
    }
    fn add_listener(&self, s: u32, event: &'static str, l: u32) -> Result<(), E> {
        self.hit(
            format!("add:{event}"),
            vec![("socket", D::Int(s)), ("listener", D::Int(l))],
        )
    }
}
impl PrivateConnectHost for Host {
    type Create = u32;
    fn public_only(&self) -> Result<V, E> {
        let v = V::Bool(self.i == 2);
        self.hit("publicOnly", vec![("value", D::Value(snapshot(&v)))])?;
        Ok(v)
    }
    fn key(&self) -> Result<V, E> {
        self.get_key()
    }
    fn capture_create(&self) -> Result<u32, E> {
        let id = self.create_id.get();
        self.hit("get:create", vec![("id", D::Int(id))])?;
        Ok(id)
    }
    fn invoke_create(&self, c: u32, r: DialRequest<V>) -> Result<u32, E> {
        assert!(
            matches!(r.protocols, V::Undefined),
            "native undefined protocols"
        );
        assert_eq!(
            r.headers,
            vec![("User-Agent", u("Tealstreet"))],
            "native independent one header"
        );
        self.hit(
            "create",
            vec![
                ("callee", D::Int(c)),
                ("url", D::Units(r.url)),
                ("protocols", D::Value(snapshot(&r.protocols))),
                ("headers", D::Headers(r.headers)),
            ],
        )?;
        Ok(71)
    }
}
impl OpenHost for Host {
    fn disposed(&self) -> bool {
        self.disposed_value()
    }
    fn start_heartbeat(&self) -> Result<(), E> {
        self.tag("heartbeat")?;
        if self.i == 8 {
            self.disposed.set(true);
        }
        Ok(())
    }
    fn send_login(&self) -> Result<(), E> {
        self.tag("login")?;
        if self.i == 9 {
            self.ignored.borrow_mut().push(self.original.clone());
        }
        Ok(())
    }
    fn private_snapshot(&self, delay: u64) -> Result<(), E> {
        self.hit("snapshot", vec![("delay", D::Int(delay as u32))])?;
        if self.i == 10 {
            self.ignored.borrow_mut().push(self.original.clone());
        }
        Ok(())
    }
    fn reconnected(&self) -> Result<(), E> {
        self.tag("reconnected")
    }
}
impl TaskHost for Host {
    type Operation = Operation;
    type Resolution = Operation;
    fn resolve_return(&self, value: V) -> Result<Rc<Operation>, E> {
        if matches!(&value,V::Opaque(id) if **id==67) {
            assert!(
                same(&value, &self.thenable),
                "native original returned thenable identity"
            );
            self.tag("get:then")?;
            let trace = self.trace.clone();
            let hook = Rc::new(move || {
                trace.borrow_mut().push(Event {
                    tag: "call:then".into(),
                    fields: vec![],
                });
                Ok(V::Text([u("adopted"), vec![0xd800]].concat()))
            });
            let op = Rc::new(Operation {
                value: RefCell::new(None),
                planned: Ok(V::Undefined),
                then_stage: Cell::new(0),
                then_hook: Some(hook),
            });
            self.operations.borrow_mut().push(op.clone());
            Ok(op)
        } else {
            Ok(self.operation(Ok(value), true))
        }
    }
    fn start_task(&self, mut task: Task) {
        let waker = futures::task::noop_waker();
        let mut cx = Context::from_waker(&waker);
        if task.as_mut().poll(&mut cx).is_pending() {
            self.tasks.borrow_mut().push(task);
        }
    }
}
impl LoginHost for Host {
    type Sign = u32;
    type Send = u32;
    type Emit = u32;
    fn key(&self) -> Result<V, E> {
        self.get_key()
    }
    fn trim(&self, v: &V) -> Result<V, E> {
        match v {
            V::Text(t) => {
                let white = |c: &u16| matches!(*c, 9..=13 | 32 | 0xfeff);
                let start = t.iter().position(|c| !white(c)).unwrap_or(t.len());
                let end = t.iter().rposition(|c| !white(c)).map_or(start, |i| i + 1);
                Ok(V::Text(t[start..end].to_vec()))
            }
            V::Null => Err(error(
                "TypeError",
                "Cannot read properties of null (reading 'trim')",
            )),
            _ => Err(error(
                "TypeError",
                "this.parent.options.key.trim is not a function",
            )),
        }
    }
    fn clock(&self) -> Result<V, E> {
        let v = V::Number(match self.i {
            7 => -0.0,
            8 => f64::NAN,
            9 => f64::INFINITY,
            _ => 123.75,
        });
        self.hit("clock", vec![("value", D::Value(snapshot(&v)))])?;
        Ok(v)
    }
    fn create_signature(&self, key: V, time: V) -> Result<Rc<Operation>, E> {
        self.hit(
            "signature:child",
            vec![
                ("apiKey", D::Value(snapshot(&key))),
                ("reqTime", D::Value(snapshot(&time))),
            ],
        )?;
        Ok(self.operation(
            if self.i == 6 {
                Err(self.original.clone())
            } else {
                Ok(match self.i {
                    7 => V::Undefined,
                    8 => V::Bool(false),
                    11 => V::Symbol,
                    _ => V::Text([u("signature"), vec![0xd800]].concat()),
                })
            },
            self.i == 9,
        ))
    }
    fn capture_send(&self) -> Result<u32, E> {
        let id = self.send_id.get();
        self.hit("get:send", vec![("id", D::Int(id))])?;
        Ok(id)
    }
    fn stringify_login(&self, f: LoginFrame<V>) -> Result<Vec<u16>, E> {
        self.hit(
            "stringify:login",
            vec![
                ("method", D::Word(f.method)),
                ("apiKey", D::Value(snapshot(&f.api_key))),
                ("reqTime", D::Value(snapshot(&f.req_time))),
                ("signature", D::Value(snapshot(&f.signature))),
            ],
        )?;
        if self.i == 10 {
            self.send_id.set(91);
        }
        let value = obj(
            101,
            false,
            vec![
                ("method", text(f.method)),
                (
                    "param",
                    obj(
                        102,
                        false,
                        vec![
                            ("apiKey", f.api_key),
                            ("reqTime", f.req_time),
                            ("signature", f.signature),
                        ],
                    ),
                ),
            ],
        );
        self.encode(&value)
    }
    fn send_login_frame(&self, c: u32, data: Vec<u16>, redact: bool) -> Result<(), E> {
        self.hit(
            "send:login",
            vec![
                ("callee", D::Int(c)),
                ("data", D::Units(data)),
                ("redactAuth", D::Bool(redact)),
            ],
        )
    }
    fn remote_signer(&self) -> Result<V, E> {
        let v = match self.i {
            0 => V::Undefined,
            2 => V::Bool(false),
            _ => self.signer.clone(),
        };
        self.hit("remote", vec![("value", D::Value(snapshot(&v)))])?;
        Ok(v)
    }
    fn secret(&self) -> Result<V, E> {
        let v = if self.i == 2 {
            V::Number(0.0)
        } else {
            V::Text([u("secret"), vec![0xd800]].concat())
        };
        self.hit("secret", vec![("value", D::Value(snapshot(&v)))])?;
        Ok(v)
    }
    fn local_sign(&self, secret: V, key: V, time: V) -> Result<V, E> {
        self.hit(
            "local",
            vec![
                ("secret", D::Value(snapshot(&secret))),
                ("apiKey", D::Value(snapshot(&key))),
                ("reqTime", D::Value(snapshot(&time))),
            ],
        )?;
        Ok(text("local-signature"))
    }
    fn capture_sign(&self, _: &V) -> Result<u32, E> {
        self.hit("get:sign", vec![("id", D::Int(29))])?;
        Ok(29)
    }
    fn nanoid(&self) -> Result<V, E> {
        self.tag("nanoid")?;
        Ok(V::Text([u("req-17"), vec![0xd800]].concat()))
    }
    fn sign(&self, c: u32, r: SigningRequest<V>) -> Result<Rc<Operation>, E> {
        self.hit(
            "sign",
            vec![
                ("callee", D::Int(c)),
                ("id", D::Value(snapshot(&r.id))),
                ("type", D::Word(r.request_type)),
                ("exchangeName", D::Word(r.exchange_name)),
                ("accountName", D::Value(snapshot(&r.account_name))),
                ("toSign", D::Units(r.to_sign)),
                ("outputFormat", D::Word(r.output_format)),
                ("reqTime", D::Value(snapshot(&r.req_time))),
            ],
        )?;
        Ok(self.operation(
            if self.i == 6 {
                Err(self.original.clone())
            } else {
                Ok(if self.i == 9 {
                    V::Null
                } else {
                    self.sign_result.clone()
                })
            },
            self.i == 9,
        ))
    }
    fn capture_emit_error(&self) -> Result<u32, E> {
        let id = self.emit_id.get();
        self.hit("get:emit", vec![("id", D::Int(id))])?;
        Ok(id)
    }
    fn emit_error(&self, c: u32, t: Vec<u16>) -> Result<(), E> {
        self.hit(
            "emit:error",
            vec![("callee", D::Int(c)), ("text", D::Units(t))],
        )
    }
}
impl DiagnosticHost for Host {
    fn send(&self, data: &[u16]) -> Result<(), E> {
        self.hit("send", vec![("data", D::Units(data.to_vec()))])
    }
    fn emit_send(&self, data: &[u16], options: DiagnosticOptions<V, E>) -> Result<(), E> {
        self.hit("emit:send", vec![("data", D::Units(data.to_vec()))])?;
        *self.captured.borrow_mut() = Some(options);
        Ok(())
    }
    fn emit_receive(&self, data: V, options: DiagnosticOptions<V, E>) -> Result<(), E> {
        assert!(
            same(&data, &self.raw),
            "native original receive data identity"
        );
        self.hit("emit:receive", vec![("data", D::Value(snapshot(&data)))])?;
        *self.captured.borrow_mut() = Some(options);
        Ok(())
    }
    fn redact(&self, data: V) -> Result<V, E> {
        self.hit("redact", vec![("data", D::Value(snapshot(&data)))])?;
        Ok(V::Text([u("[redacted]"), vec![0xdfff]].concat()))
    }
}
impl BalanceHost for Host {
    type Handle = u32;
    fn timer(&self) -> Option<u32> {
        let id = self.timer.get();
        self.hit("timer", vec![("id", D::MaybeInt(id))]).unwrap();
        id
    }
    fn clear_timeout(&self, h: Option<u32>) -> Result<(), E> {
        self.hit("clear", vec![("id", D::MaybeInt(h))])?;
        if self.i == 2 {
            self.timer.set(Some(88));
        }
        Ok(())
    }
    fn set_timeout(&self, period: u64, callback: TimerCallback) -> Result<u32, E> {
        self.hit(
            "set",
            vec![("period", D::Int(period as u32)), ("id", D::Int(77))],
        )?;
        *self.timer_callback.borrow_mut() = Some(callback);
        Ok(77)
    }
    fn assign_timer(&self, h: u32) -> Result<(), E> {
        self.hit("assign:timer", vec![("id", D::Int(h))])?;
        self.timer.set(Some(h));
        Ok(())
    }
    fn fetch_balance(&self) -> Result<Rc<Operation>, E> {
        self.tag("fetch")?;
        if self.i == 8 {
            self.disposed.set(true);
        }
        Ok(self.operation(
            if self.i == 5 {
                Err(self.original.clone())
            } else {
                Ok(self.balance.borrow().clone())
            },
            true,
        ))
    }
    fn disposed(&self) -> bool {
        self.disposed_value()
    }
    fn update_balance(&self, balance: V) -> Result<(), E> {
        assert!(
            same(&balance, &self.balance.borrow()),
            "native original balance identity/bits"
        );
        self.hit("update", vec![("balance", D::Value(snapshot(&balance)))])
    }
}
fn input(i: usize) -> Vec<u16> {
    match i {
        12 => u("extra"),
        2 => [u("🦀"), vec![0xd800]].concat(),
        3 => vec![0xdfff],
        _ => u([
            "wire/0", "", "", "", "fail", "emit", "x\0y", "mutable", "symbol", "holes", "cap",
            "last",
        ][i]),
    }
}
fn redact_input(i: usize) -> Vec<u16> {
    u(match i {
        12 => "{\"method\":\"login\",\"param\":{\"reqTime\":\"12\",\"extra\":-0}}",
        1 => " {\"method\":\"ping\",\"param\":{\"apiKey\":\"visible\",\"signature\":\"visible\"}} ",
        2 => "null",
        3 => "bad{",
        6 => "{\"method\":\"login\",\"param\":0}",
        7 => "{\"method\":\"login\",\"param\":[17,\"x\"]}",
        8 => "{\"method\":\"login\",\"param\":\"\\ud83d\\ude00\\ud800\"}",
        9 => "{\"method\":\"login\",\"param\":true}",
        10 => "{\"method\":\"login\",\"param\":null}",
        11 => "[{\"method\":\"login\",\"param\":{\"apiKey\":\"nested\",\"signature\":\"still\"}}]",
        _ => {
            "{\"method\":\"login\",\"param\":{\"apiKey\":\"APIsecret\",\"signature\":\"sigRaw\",\"reqTime\":\"17\\ud800\",\"extra\":-0}}"
        }
    })
}
fn parsed(i: usize) -> V {
    if i == 2 {
        return V::Null;
    }
    if i == 11 {
        return array(
            71,
            vec![obj(
                72,
                true,
                vec![
                    ("method", text("login")),
                    (
                        "param",
                        obj(
                            73,
                            true,
                            vec![("apiKey", text("nested")), ("signature", text("still"))],
                        ),
                    ),
                ],
            )],
        );
    }
    let param = match i {
        12 => obj(
            72,
            true,
            vec![("reqTime", text("12")), ("extra", V::Number(-0.0))],
        ),
        1 => obj(
            72,
            true,
            vec![("apiKey", text("visible")), ("signature", text("visible"))],
        ),
        6 => V::Number(0.0),
        7 => array(72, vec![V::Number(17.0), text("x")]),
        8 => V::Text([u("😀"), vec![0xd800]].concat()),
        9 => V::Bool(true),
        10 => V::Null,
        _ => obj(
            72,
            true,
            vec![
                ("apiKey", text("APIsecret")),
                ("signature", text("sigRaw")),
                ("reqTime", V::Text([u("17"), vec![0xd800]].concat())),
                ("extra", V::Number(-0.0)),
            ],
        ),
    };
    obj(
        71,
        true,
        vec![
            ("method", text(if i == 1 { "ping" } else { "login" })),
            ("param", param),
        ],
    )
}
fn oracle() -> &'static J {
    static O: OnceLock<J> = OnceLock::new();
    O.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/private_session_oracle.cjs"
            ))
            .output()
            .expect("source oracle launch");
        assert!(
            output.status.success(),
            "source oracle failure {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: J = serde_json::from_slice(&output.stdout).expect("source JSON");
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            98,
            "independent 96 roots plus setter and returned-thenable witnesses"
        );
        v
    })
}
fn source_units(v: &J) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn member_count(i: usize, id: u32, is_array: bool) -> usize {
    if is_array {
        return match (id, i) {
            (71, 11) => 1,
            (72, 7) => 2,
            _ => panic!("undeclared array role"),
        };
    }
    match id {
        71 | 73 => 2,
        72 => {
            if i == 1 || i == 11 || i == 12 {
                2
            } else {
                4
            }
        }
        100 => match i {
            7 => 4,
            8 => 5,
            9 => 2,
            _ => 4,
        },
        _ => panic!("undeclared object role"),
    }
}
fn assert_value(v: &S, s: &J, i: usize, label: &str) {
    let kind = match v {
        S::Undefined => "undefined",
        S::Null => "null",
        S::Text(_) => "text",
        S::Number(_) => "number",
        S::Nan => "nan",
        S::Bool(_) => "bool",
        S::Symbol => "symbol",
        S::Opaque(_) => "opaque",
        S::Object(_, _) => "object",
        S::Array(_, _) => "array",
    };
    assert_eq!(kind, s["kind"].as_str().unwrap(), "native kind {label}");
    match v {
        S::Text(t) => assert_eq!(*t, source_units(&s["units"]), "native UTF-16 {label}"),
        S::Number(bits) => assert_eq!(
            *bits,
            u64::from_str_radix(s["bits"].as_str().unwrap(), 16).unwrap(),
            "native Number bits {label}"
        ),
        S::Bool(b) => assert_eq!(*b, s["value"].as_bool().unwrap(), "native boolean {label}"),
        S::Opaque(id) => assert_eq!(
            *id,
            s["marker"].as_u64().unwrap() as u32,
            "native opaque identity {label}"
        ),
        S::Object(id, fields) => {
            assert_eq!(
                *id,
                s["marker"].as_u64().unwrap() as u32,
                "native object identity {label}"
            );
            let count = member_count(i, *id, false);
            let source = s["fields"].as_array().unwrap();
            assert_eq!(
                fields.len(),
                count,
                "native independent object field count {label}"
            );
            assert_eq!(
                source.len(),
                count,
                "source independent object field count {label}"
            );
            for ((key, value), field) in fields.iter().zip(source) {
                assert_eq!(key, field[0].as_str().unwrap(), "native field key {label}");
                assert_value(value, &field[1], i, &format!("{label}.{key}"));
            }
        }
        S::Array(id, items) => {
            assert_eq!(
                *id,
                s["marker"].as_u64().unwrap() as u32,
                "native array identity {label}"
            );
            let count = member_count(i, *id, true);
            let source = s["items"].as_array().unwrap();
            assert_eq!(items.len(), count, "native independent array count {label}");
            assert_eq!(
                source.len(),
                count,
                "source independent array count {label}"
            );
            for (j, (value, item)) in items.iter().zip(source).enumerate() {
                assert_value(value, item, i, &format!("{label}[{j}]"));
            }
        }
        _ => {}
    }
}
fn assert_error(r: &Result<(), E>, s: &J, host: &Host, label: &str) {
    match (r, s.is_null()) {
        (Ok(()), true) => {}
        (Err(e), false) => {
            assert_eq!(
                e.name,
                s["name"].as_str().unwrap(),
                "native error name {label}"
            );
            assert_eq!(
                e.message,
                source_units(&s["message"]),
                "native error message {label}"
            );
            assert_eq!(
                e.marker,
                s["marker"].as_u64().map(|n| n as u32),
                "native error marker {label}"
            );
            assert_eq!(
                Rc::ptr_eq(e, &host.original),
                s["same"].as_bool().unwrap(),
                "native error identity {label}"
            );
        }
        _ => panic!("native error outcome {label}: {r:?} vs {s}"),
    }
}
fn prefix_count(f: usize, i: usize) -> usize {
    if i == 12 {
        return if f == 6 { 8 } else { 5 };
    }
    match f {
        0 => [17, 1, 2, 3, 7, 14, 17, 17, 17, 3, 17, 16][i],
        1 => [5, 1, 5, 5, 2, 3, 4, 5, 5, 5, 5, 5][i],
        2 => {
            if matches!(i, 2..=4) {
                1
            } else {
                3
            }
        }
        3 => {
            if matches!(i, 0 | 2 | 8) {
                3
            } else {
                5
            }
        }
        4 => {
            if i == 4 {
                1
            } else {
                2
            }
        }
        5 => 1,
        6 => [8, 5, 2, 1, 1, 6, 6, 8, 8, 8, 6, 5][i],
        7 => {
            if i == 3 {
                3
            } else if i == 4 {
                2
            } else {
                4
            }
        }
        _ => unreachable!(),
    }
}
fn trace_count(f: usize, i: usize) -> usize {
    if i == 12 {
        return if f == 6 { 8 } else { 10 };
    }
    match f {
        2 => {
            if matches!(i, 2..=4) {
                1
            } else if matches!(i, 5..=8) {
                3
            } else {
                6
            }
        }
        3 => [3, 8, 3, 9, 10, 5, 5, 8, 3, 5, 8, 9][i],
        4 => [12, 2, 2, 2, 1, 2, 22, 22, 17, 22, 12, 12][i],
        7 => {
            if i == 3 {
                3
            } else if i == 4 {
                2
            } else if i == 5 || i == 6 {
                7
            } else if i == 8 {
                10
            } else {
                12
            }
        }
        _ => prefix_count(f, i),
    }
}
fn event_fields(tag: &str) -> usize {
    match tag {
        "create" => 4,
        "sign" => 8,
        "stringify:login" => 4,
        "send:login" | "local" => 3,
        "signature:child" | "emit:error" | "set" => 2,
        t if t.starts_with("add:") => 2,
        "disposed" | "publicOnly" | "key" | "testnet" | "get:create" | "assign" | "socket"
        | "snapshot" | "clock" | "remote" | "secret" | "get:send" | "get:sign" | "get:emit"
        | "send" | "emit:send" | "emit:receive" | "redact" | "parse" | "set:param"
        | "stringify" | "timer" | "clear" | "assign:timer" | "update" => 1,
        t if t.starts_with("listener:") => 1,
        t if t.starts_with("url:") => 0,
        "heartbeat" | "login" | "reconnected" | "nanoid" | "field:accountName" | "field:error"
        | "field:signature" | "field:redactAuth" | "field:method" | "field:param" | "fetch"
        | "get:then" | "call:then" => 0,
        _ => panic!("undeclared source call-site role {tag}"),
    }
}
// Native values/records/identities/bits are checked for the whole trace FIRST.
// Stringify output argument comparisons occupy the separate second pass.
fn assert_events_native(trace: &[Event], source: &J, i: usize, label: &str) {
    let source = source.as_array().unwrap();
    for (j, (e, row)) in trace.iter().zip(source).enumerate() {
        let label = format!("{label}/event{j}");
        assert_eq!(
            e.tag,
            row["tag"].as_str().unwrap(),
            "native call order {label}"
        );
        let count = event_fields(&e.tag);
        assert_eq!(
            e.fields.len(),
            count,
            "native independent call field count {label}"
        );
        assert_eq!(
            row.as_object().unwrap().len(),
            count + 1,
            "source independent call field count {label}"
        );
        for (k, v) in &e.fields {
            let label = format!("{label}/{k}");
            match v {
                D::Value(v) => assert_value(v, &row[*k], i, &label),
                D::Int(n) => assert_eq!(
                    *n,
                    row[*k].as_u64().unwrap() as u32,
                    "native integer {label}"
                ),
                D::MaybeInt(n) => assert_eq!(
                    *n,
                    row[*k].as_u64().map(|n| n as u32),
                    "native optional handle {label}"
                ),
                D::Bool(b) => assert_eq!(*b, row[*k].as_bool().unwrap(), "native boolean {label}"),
                D::Word(s) => assert_eq!(*s, row[*k].as_str().unwrap(), "native word {label}"),
                D::Headers(h) => {
                    assert_eq!(h.len(), 1, "native independent header count");
                    assert_eq!(
                        row[*k].as_array().unwrap().len(),
                        1,
                        "source independent header count"
                    );
                    assert_eq!(
                        h[0].0,
                        row[*k][0][0].as_str().unwrap(),
                        "native header name"
                    );
                }
                D::Units(_) => {}
            }
        }
    }
}
fn assert_events_boundary(trace: &[Event], source: &J, label: &str) {
    let source = source.as_array().unwrap();
    for (j, (e, row)) in trace.iter().zip(source).enumerate() {
        for (k, v) in &e.fields {
            match v {
                D::Units(t) => assert_eq!(
                    *t,
                    source_units(&row[*k]),
                    "boundary UTF-16 {label}/event{j}/{k}"
                ),
                D::Headers(h) => assert_eq!(
                    h[0].1,
                    source_units(&row[*k][0][1]),
                    "boundary header UTF-16"
                ),
                _ => {}
            }
        }
    }
}
const NAMES: [&str; 8] = [
    "connectAndSubscribe",
    "onOpen",
    "sendLogin",
    "createLoginSignature",
    "sendWithDiagnostics",
    "emitReceiveDiagnostics",
    "redactAuthFrame",
    "throttledFetchBalance",
];
fn poll_observer(observer: &mut MethodObserver<V, E>) -> Poll<Result<V, E>> {
    let w = futures::task::noop_waker();
    let mut cx = Context::from_waker(&w);
    Pin::new(observer).poll(&mut cx)
}
fn case(f: usize, i: usize) {
    let s = &oracle()["cases"][if i == 12 {
        if f == 6 { 96 } else { 97 }
    } else {
        f * 12 + i
    }];
    let name = NAMES[f];
    assert_eq!(s["name"], name);
    assert_eq!(s["index"], i);
    assert_eq!(
        source_units(&s["data"]),
        input(i),
        "independent source root payload"
    );
    assert_eq!(
        source_units(&s["redactData"]),
        redact_input(i),
        "independent source redaction input"
    );
    let key = V::Text([u("arg-key"), vec![0xd800]].concat());
    let time = V::Text([u("123"), vec![0xdfff]].concat());
    assert_eq!(
        source_units(&s["apiKey"]),
        match &key {
            V::Text(v) => v.clone(),
            _ => unreachable!(),
        }
    );
    assert_eq!(
        source_units(&s["reqTime"]),
        match &time {
            V::Text(v) => v.clone(),
            _ => unreachable!(),
        }
    );
    let host = Host::new(name, i);
    let mut observer = None;
    let mut result = match f {
        0 => connect(&*host).map(|_| V::Undefined),
        1 => on_open(&*host).map(|_| V::Undefined),
        2 => {
            observer = Some(send_login(host.clone()));
            Ok(V::Undefined)
        }
        3 => {
            observer = Some(create_login_signature(host.clone(), key, time));
            Ok(V::Undefined)
        }
        4 => send_diagnostics(
            host.clone(),
            &input(i),
            match i {
                1 => V::Undefined,
                2 => V::Null,
                3 => V::Bool(false),
                _ => host.options.clone(),
            },
        )
        .map(|_| V::Undefined),
        5 => receive_diagnostics(host.clone(), host.raw.clone()).map(|_| V::Undefined),
        6 => Ok(redact_auth_frame(&*host, &V::Text(redact_input(i)))),
        7 => throttled_fetch_balance(host.clone()).map(|_| V::Undefined),
        _ => unreachable!(),
    };
    let prefix = host.trace.borrow().clone();
    let expected = prefix_count(f, i);
    assert_eq!(
        prefix.len(),
        expected,
        "native private prefix count {name}/{i}"
    );
    assert_eq!(
        s["prefix"].as_array().unwrap().len(),
        expected,
        "source independent prefix count {name}/{i}"
    );
    if (f == 2 || f == 3) && i == 9 {
        assert!(
            poll_observer(observer.as_mut().unwrap()).is_pending(),
            "native ready method observer pending {name}/{i}"
        );
    }
    let ignored_root = (f == 2 || f == 3) && i == 10;
    if ignored_root {
        drop(observer.take());
    }
    host.release();
    host.drain();
    if let Some(mut o) = observer {
        result = match poll_observer(&mut o) {
            Poll::Ready(v) => v,
            Poll::Pending => panic!("native method settled after controlled drain"),
        };
    }
    let mut formats = vec![];
    if let Some(options) = host.captured.borrow().as_ref() {
        if i == 9 {
            host.flag.set(true);
        }
        let messages = vec![
            V::Text([u("message"), vec![0xd800]].concat()),
            V::Undefined,
            V::Null,
            V::Bool(false),
            V::Number(0.0),
            V::Number(f64::NAN),
            V::Number(-0.0),
            V::Number(1.5),
            V::Symbol,
            opaque(92),
        ];
        for message in messages {
            formats.push((options.format)(message));
        }
    }
    let mut ticks: Vec<Option<Result<V, E>>> = vec![];
    let mut tick_prefixes = vec![];
    if let Some(callback) = host.timer_callback.borrow().as_ref() {
        for j in 0..3 {
            host.disposed.set(j == 1);
            let balance = match j {
                0 => host.balance.borrow().clone(),
                1 => text("balance-2"),
                _ => V::Number(-0.0),
            };
            *host.balance.borrow_mut() = balance;
            let before = host.trace.borrow().len();
            let mut tick = callback();
            tick_prefixes.push(host.trace.borrow().len() - before);
            if i == 9 {
                assert!(
                    poll_observer(&mut tick).is_pending(),
                    "native ready timer observer pending"
                );
            }
            if i == 10 {
                drop(tick);
                host.release();
                host.drain();
                ticks.push(None);
            } else {
                host.release();
                host.drain();
                ticks.push(Some(match poll_observer(&mut tick) {
                    Poll::Ready(v) => v,
                    Poll::Pending => panic!("native timer continuation settled"),
                }));
            }
        }
    }
    let expected_formats = if (f == 4 && !matches!(i, 4 | 5)) || (f == 5 && i != 5) {
        10
    } else {
        0
    };
    assert_eq!(
        formats.len(),
        expected_formats,
        "native independent formatter count"
    );
    let source_formats = s["formats"].as_array().unwrap();
    assert_eq!(
        source_formats.len(),
        expected_formats,
        "source independent formatter count"
    );
    let expected_ticks = if f == 7 && !matches!(i, 3 | 4) { 3 } else { 0 };
    assert_eq!(
        ticks.len(),
        expected_ticks,
        "native independent timer replay count"
    );
    assert_eq!(
        s["ticks"].as_array().unwrap().len(),
        expected_ticks,
        "source independent timer replay count"
    );
    assert_eq!(
        tick_prefixes,
        vec![1; expected_ticks],
        "native timer callback prefix floor"
    );
    assert_eq!(
        s["tickPrefixes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as usize)
            .collect::<Vec<_>>(),
        vec![1; expected_ticks],
        "source timer callback prefix floor"
    );
    for (tick, source) in ticks.iter().zip(s["ticks"].as_array().unwrap()) {
        if let Some(tick) = tick {
            assert_error(
                &tick.as_ref().map(|_| ()).map_err(|e| e.clone()),
                source,
                &host,
                "timer catches errors",
            );
            if let Ok(v) = tick {
                assert!(matches!(v, V::Undefined), "native timer undefined result");
            }
        } else {
            assert_eq!(source["unobserved"], true, "source timer promise discarded");
        }
    }
    let expected_ignored = usize::from(f == 1 && matches!(i, 9 | 10));
    assert_eq!(
        host.ignored.borrow().len(),
        expected_ignored,
        "native independent ignored child rejection count"
    );
    assert_eq!(
        s["ignored"].as_array().unwrap().len(),
        expected_ignored,
        "source independent ignored child rejection count"
    );
    for (e, row) in host
        .ignored
        .borrow()
        .iter()
        .zip(s["ignored"].as_array().unwrap())
    {
        assert_error(&Err(e.clone()), row, &host, "ignored child");
    }
    let trace = host.trace.borrow();
    let expected = trace_count(f, i);
    assert_eq!(
        trace.len(),
        expected,
        "native private total count {name}/{i}"
    );
    assert_eq!(
        s["trace"].as_array().unwrap().len(),
        expected,
        "source independent total count {name}/{i}"
    );
    assert_events_native(&prefix, &s["prefix"], i, &format!("{name}/{i}/prefix"));
    assert_events_native(&trace, &s["trace"], i, &format!("{name}/{i}/trace"));
    if !ignored_root {
        assert_error(
            &result.as_ref().map(|_| ()).map_err(|e| e.clone()),
            &s["error"],
            &host,
            "root",
        );
        if let Ok(value) = &result {
            if f == 6 {
                assert!(
                    matches!(value, V::Text(_)),
                    "native redaction return string"
                );
            }
            if f != 6 {
                assert_value(
                    &snapshot(value),
                    &s["result"],
                    i,
                    "root result after native record assertions",
                );
            }
        }
    }
    assert_eq!(
        s["rootObserved"], !ignored_root,
        "independent source root observer choice"
    );
    assert_eq!(
        host.timer.get(),
        s["timer"].as_u64().map(|n| n as u32),
        "native final timer handle"
    );
    for (j, (format, row)) in formats.iter().zip(source_formats).enumerate() {
        assert_eq!(row["index"], j);
        let expected_success = !(f == 4 && (i == 7 || i == 10 || (j == 8 && !matches!(i, 6 | 9))));
        assert_eq!(
            format.is_ok(),
            expected_success,
            "native private formatter success {name}/{i}/{j}"
        );
        assert_error(
            &format.as_ref().map(|_| ()).map_err(|e| e.clone()),
            &row["error"],
            &host,
            "formatter",
        );
    }
    assert_events_boundary(&prefix, &s["prefix"], &format!("{name}/{i}/prefix"));
    assert_events_boundary(&trace, &s["trace"], &format!("{name}/{i}/trace"));
    if f == 6 {
        if let Ok(value) = &result {
            assert_value(
                &snapshot(value),
                &s["result"],
                i,
                "redaction output after all native assertions",
            );
        }
    }
    for (j, (format, row)) in formats.iter().zip(source_formats).enumerate() {
        if let Ok(text) = format {
            assert_eq!(
                *text,
                source_units(&row["text"]),
                "formatter boundary UTF-16 {name}/{i}/{j}"
            );
        }
    }
}
fn run(f: usize) {
    for i in 0..12 {
        case(f, i)
    }
    assert_eq!(
        oracle()["cases"][f * 12 + 11]["index"],
        11,
        "fixture floor stops at index11"
    );
}
#[test]
fn connect_native() {
    run(0)
}
#[test]
fn open_native() {
    run(1)
}
#[test]
fn login_native() {
    run(2)
}
#[test]
fn signature_native() {
    run(3);
    case(3, 12)
}
#[test]
fn send_diagnostics_native() {
    run(4)
}
#[test]
fn receive_diagnostics_native() {
    run(5)
}
#[test]
fn redact_native() {
    run(6);
    case(6, 12)
}
#[test]
fn balance_native() {
    run(7)
}
#[test]
fn abandoned_methods_continue() {
    for (f, expected) in [(2, 6), (3, 8), (7, 12)] {
        case(f, 10);
        let source = &oracle()["cases"][f * 12 + 10];
        assert_eq!(
            source["trace"].as_array().unwrap().len(),
            expected,
            "independent source abandoned-effect floor"
        );
    }
}
#[test]
fn already_ready_children_still_yield() {
    for f in [2, 3, 7] {
        case(f, 9)
    }
}
