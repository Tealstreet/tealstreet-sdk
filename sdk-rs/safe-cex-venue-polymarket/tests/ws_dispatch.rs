use safe_cex_venue_polymarket::ws_dispatch::*;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
#[derive(Clone, Debug)]
enum V {
    U,
    N,
    B(bool),
    Num(f64),
    Text(Vec<u16>),
    Symbol,
    Obj(Rc<RefCell<O>>),
}
#[derive(Debug)]
struct O {
    marker: Option<u32>,
    fields: Vec<(&'static str, V)>,
}
#[derive(Debug)]
struct Failure {
    marker: u32,
    name: Option<&'static str>,
    message: &'static str,
}
type E = Rc<Failure>;
type Callback = Rc<u32>;
type Set = Rc<RefCell<Vec<Callback>>>;
type Handlers = Rc<RefCell<Vec<Option<u32>>>>;
#[derive(Debug, Clone)]
enum Snapshot {
    U,
    N,
    B(bool),
    Num(f64),
    Text(Vec<u16>),
    Symbol,
    Obj(Option<u32>),
}
fn snap(v: &V) -> Snapshot {
    match v {
        V::U => Snapshot::U,
        V::N => Snapshot::N,
        V::B(v) => Snapshot::B(*v),
        V::Num(v) => Snapshot::Num(*v),
        V::Text(v) => Snapshot::Text(v.clone()),
        V::Symbol => Snapshot::Symbol,
        V::Obj(v) => Snapshot::Obj(v.borrow().marker),
    }
}
#[derive(Debug)]
enum Event {
    Tag(String),
    Flag(String, bool),
    Socket(String, Option<u32>),
    Id(String, u32),
    Text(String, Vec<u16>),
    Value(String, Snapshot),
    Log(Vec<u16>, &'static str),
    Add(&'static str, u32, Option<u32>),
    Send(AssetOperation),
    Refresh(&'static str),
    Update(Snapshot, Snapshot, Snapshot, f64),
}
impl Event {
    fn tag(&self) -> &str {
        match self {
            Self::Tag(t)
            | Self::Flag(t, _)
            | Self::Socket(t, _)
            | Self::Id(t, _)
            | Self::Text(t, _)
            | Self::Value(t, _) => t,
            Self::Log(..) => "log",
            Self::Add(event, ..) => match *event {
                "open" => "add:open",
                "message" => "add:message",
                "close" => "add:close",
                "error" => "add:error",
                _ => unreachable!(),
            },
            Self::Send(_) => "send",
            Self::Refresh(_) => "refresh",
            Self::Update(..) => "update",
        }
    }
}
fn u(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn token(i: usize) -> Vec<u16> {
    match i {
        3 => vec![0xd800],
        4 => vec![0xdfff],
        _ => u([
            "token/0",
            "",
            "🦀,x",
            "",
            "",
            "quote\"\\x",
            "a\0b",
            "__proto__",
            "object-token",
            "9",
            "missing",
            "final",
        ][i]),
    }
}
fn obj(marker: Option<u32>, fields: Vec<(&'static str, V)>) -> V {
    V::Obj(Rc::new(RefCell::new(O { marker, fields })))
}
fn equal(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::U, V::U) | (V::N, V::N) | (V::Symbol, V::Symbol) => true,
        (V::B(a), V::B(b)) => a == b,
        (V::Num(a), V::Num(b)) => a.to_bits() == b.to_bits(),
        (V::Text(a), V::Text(b)) => a == b,
        (V::Obj(a), V::Obj(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
fn parsed(i: usize) -> V {
    let fields = match i {
        2 | 3 => return V::N,
        4 => vec![("event_type", V::Text(u("pong")))],
        5 => vec![
            ("event_type", V::Text(u("authenticated"))),
            ("error", V::Text(u("x"))),
        ],
        6 => vec![
            ("event_type", V::Text(u("topic"))),
            ("authenticated", V::Num(1.0)),
            ("error", V::Text(u("x"))),
        ],
        7 => vec![
            ("event_type", V::Text(u("error"))),
            ("error", V::Text(vec![])),
            ("message", V::Text(u("fallback"))),
        ],
        8 => vec![
            ("event_type", V::Text(u("topic"))),
            ("error", V::Text(u("boom"))),
            ("message", V::Text(u("unused"))),
        ],
        10 => vec![
            ("event_type", V::Text(vec![])),
            ("type", V::Text(u("topic"))),
        ],
        _ => vec![("event_type", V::Text(u("topic")))],
    };
    obj([0, 9, 10, 11].contains(&i).then_some(71), fields)
}
struct Host {
    name: &'static str,
    i: usize,
    events: RefCell<Vec<Event>>,
    failure: E,
    type_error: E,
    handler_error: E,
    invalid_handlers: Cell<bool>,
    fail: &'static str,
    disposed: Cell<bool>,
    socket: Cell<Option<u32>>,
    url: RefCell<Vec<u16>>,
    message_listener: Cell<u32>,
    errors: RefCell<Vec<Rc<dyn Fn(V) -> Result<(), E>>>>,
    read: Cell<usize>,
    error_reads: Cell<usize>,
    raw: Vec<u16>,
    parsed: V,
    handlers: RefCell<Handlers>,
    assets: RefCell<Vec<Vec<u16>>>,
    registry: RefCell<Vec<(Vec<u16>, Set)>>,
    callback: Callback,
    trade_token: V,
    trade_data: V,
    patch: RefCell<Option<(V, LastTradePatch<V>)>>,
}
impl Host {
    fn new(_venue: &'static str, name: &'static str, i: usize, raw: Vec<u16>) -> Self {
        let token = token(i);
        let tok = if i == 8 {
            obj(Some(83), vec![])
        } else {
            V::Text(token.clone())
        };
        let price = match i {
            2 => V::Symbol,
            8 => V::Num(0.5),
            9 => V::Num(0.1),
            6 => V::B(false),
            7 => V::Text(vec![49, 50, 0xd800, 57, 57]),
            _ => V::Text(u([
                "12.5tail",
                "bad",
                "",
                "Infinity",
                "-0",
                "\u{feff}.25tail",
                "",
                "",
                "",
                "",
                "2",
                "3",
            ][i])),
        };
        let data = obj(None, vec![("asset_id", tok.clone()), ("price", price)]);
        let registry = if i == 3 {
            vec![(token.clone(), Rc::new(RefCell::new(vec![Rc::new(99)])))]
        } else if i % 4 == 1 {
            vec![(token.clone(), Rc::new(RefCell::new(vec![])))]
        } else {
            vec![]
        };
        Self {
            name,
            i,
            events: RefCell::new(vec![]),
            failure: Rc::new(Failure {
                marker: 47,
                name: None,
                message: "native boundary failure",
            }),
            type_error: Rc::new(Failure {
                marker: 83,
                name: Some("TypeError"),
                message: "Cannot convert a Symbol value to a string",
            }),
            handler_error: Rc::new(Failure {
                marker: 97,
                name: Some("TypeError"),
                message: "handlers.forEach is not a function",
            }),
            invalid_handlers: Cell::new(false),
            fail: match name {
                "connectAndSubscribe" => match i {
                    4 => "log",
                    5 => "create",
                    6 => "add:message",
                    _ => "",
                },
                "subscribeToOrderBook" => match i {
                    4 => "send",
                    5 => "callback:add",
                    6 => "child:subscribe",
                    _ => "",
                },
                "subscribeToAsset" | "unsubscribeFromAsset" => {
                    if i == 4 {
                        "send"
                    } else {
                        ""
                    }
                }
                "handleLastTradePrice" => {
                    if i == 11 {
                        "update"
                    } else {
                        ""
                    }
                }
                _ => "",
            },
            disposed: Cell::new(i == 1),
            socket: Cell::new((i == 2).then_some(71)),
            url: RefCell::new(
                u("ws://fixture/")
                    .into_iter()
                    .chain(token.iter().copied())
                    .collect(),
            ),
            message_listener: Cell::new(13),
            errors: RefCell::new(vec![]),
            read: Cell::new(0),
            error_reads: Cell::new(0),
            raw,
            parsed: parsed(i),
            handlers: RefCell::new(Rc::new(RefCell::new(vec![Some(11), Some(13)]))),
            assets: RefCell::new(if i % 3 == 1 { vec![token] } else { vec![] }),
            registry: RefCell::new(registry),
            callback: Rc::new(37),
            trade_token: tok,
            trade_data: data,
            patch: RefCell::new(None),
        }
    }
    fn hit(&self, e: Event) -> Result<(), E> {
        let bad = e.tag() == self.fail;
        self.events.borrow_mut().push(e);
        if bad {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
    fn tag(&self, t: &str) -> Result<(), E> {
        self.hit(Event::Tag(t.into()))
    }
    fn tok(&self, t: &str, v: &[u16]) -> Result<(), E> {
        self.hit(Event::Text(t.into(), v.to_vec()))
    }
}
impl Runtime for Host {
    type Value = V;
    type Error = E;
    fn undefined(&self) -> V {
        V::U
    }
    fn nullish(&self, v: &V) -> bool {
        matches!(v, V::U | V::N)
    }
    fn truthy(&self, v: &V) -> bool {
        match v {
            V::U | V::N => false,
            V::B(v) => *v,
            V::Num(v) => *v != 0.0 && !v.is_nan(),
            V::Text(v) => !v.is_empty(),
            _ => true,
        }
    }
    fn is_text(&self, v: &V, t: &str) -> bool {
        matches!(v,V::Text(v) if *v==u(t))
    }
    fn field(&self, v: &V, key: &str) -> Result<V, E> {
        self.tag(&format!("field:{key}"))?;
        let V::Obj(v) = v else { return Ok(V::U) };
        if self.name == "onMessage" && self.i == 8 && key == "error" {
            let n = self.error_reads.get();
            self.error_reads.set(n + 1);
            if n > 0 {
                return Ok(V::U);
            }
        }
        Ok(v.borrow()
            .fields
            .iter()
            .find(|(k, _)| *k == key)
            .map_or(V::U, |(_, v)| v.clone()))
    }
    fn string(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(match v {
            V::U => u("undefined"),
            V::N => u("null"),
            V::B(v) => u(if *v { "true" } else { "false" }),
            V::Num(v) => u(&safe_cex_num::js_number_string(*v)),
            V::Text(v) => v.clone(),
            V::Symbol => return Err(self.type_error.clone()),
            V::Obj(_) => u("[object Object]"),
        })
    }
    fn log(&self, text: &[u16], severity: &'static str) -> Result<(), E> {
        self.hit(Event::Log(text.to_vec(), severity))?;
        if self.name == "connectAndSubscribe" && self.i == 7 {
            self.disposed.set(true);
            *self.url.borrow_mut() = u("changed://").into_iter().chain([0xd800]).collect();
        }
        Ok(())
    }
}
impl ConnectHost for Host {
    type Socket = u32;
    type Listener = u32;
    fn credentials(&self) -> Result<V, E> {
        self.tag("credentials")?;
        Ok(if self.i == 3 { V::N } else { obj(None, vec![]) })
    }
    fn disposed(&self) -> bool {
        self.hit(Event::Flag("disposed".into(), self.disposed.get()))
            .unwrap();
        self.disposed.get()
    }
    fn socket(&self) -> Result<Option<u32>, E> {
        self.hit(Event::Socket("socket".into(), self.socket.get()))?;
        Ok(self.socket.get())
    }
    fn url(&self) -> Result<Vec<u16>, E> {
        self.hit(Event::Text("url".into(), self.url.borrow().clone()))?;
        Ok(self.url.borrow().clone())
    }
    fn create_socket(&self, url: &[u16]) -> Result<u32, E> {
        self.tok("create", url)?;
        Ok(71)
    }
    fn assign_socket(&self, s: u32) -> Result<(), E> {
        self.hit(Event::Socket("assign".into(), Some(s)))?;
        self.socket.set(Some(s));
        Ok(())
    }
    fn listener(&self, event: &'static str) -> Result<u32, E> {
        let id = match event {
            "open" => 11,
            "message" => self.message_listener.get(),
            "close" => 17,
            _ => unreachable!(),
        };
        self.hit(Event::Id(format!("listener:{event}"), id))?;
        if self.i == 9 && event == "open" {
            self.socket.set(Some(91))
        }
        Ok(id)
    }
    fn add_listener(&self, socket: u32, event: &'static str, listener: u32) -> Result<(), E> {
        self.hit(Event::Add(event, socket, Some(listener)))?;
        if self.i == 8 && event == "open" {
            self.socket.set(Some(91));
            self.message_listener.set(32)
        }
        Ok(())
    }
    fn add_error_listener(&self, socket: u32, cb: Rc<dyn Fn(V) -> Result<(), E>>) -> Result<(), E> {
        self.hit(Event::Add("error", socket, None))?;
        self.errors.borrow_mut().push(cb);
        Ok(())
    }
    fn missing_socket_error(&self) -> E {
        self.failure.clone()
    }
}
impl MessageHost for Host {
    type Event = ();
    type Handlers = Handlers;
    type Handler = u32;
    fn track_received(&self) -> Result<(), E> {
        self.tag("track")
    }
    fn event_data(&self, _: &()) -> Result<V, E> {
        self.tag("event:data")?;
        let count = self.read.get();
        self.read.set(count + 1);
        Ok(V::Text(if self.i == 10 && count == 0 {
            u("first-read")
        } else {
            self.raw.clone()
        }))
    }
    fn json_parse(&self, raw: &V) -> Result<V, E> {
        self.hit(Event::Value("parse".into(), snap(raw)))?;
        assert!(
            matches!(raw,V::Text(v) if *v==self.raw),
            "native parser receives current second event read"
        );
        Ok(self.parsed.clone())
    }
    fn pong(&self) -> Result<(), E> {
        self.tag("pong")
    }
    fn handlers(&self, topic: &V) -> Result<Option<Handlers>, E> {
        Ok(
            (self.invalid_handlers.get() || self.is_text(topic, "topic"))
                .then(|| self.handlers.borrow().clone()),
        )
    }
    fn stats(&self, topic: &V) -> Result<(), E> {
        self.hit(Event::Value("stats".into(), snap(topic)))?;
        if self.i == 10 {
            self.handlers.borrow().borrow_mut().push(Some(40));
            *self.handlers.borrow_mut() = Rc::new(RefCell::new(vec![Some(99)]));
        }
        Ok(())
    }
    fn handler_length(&self, handlers: &Handlers) -> Result<usize, E> {
        if self.invalid_handlers.get() {
            Err(self.handler_error.clone())
        } else {
            Ok(handlers.borrow().len())
        }
    }
    fn handler_at(&self, handlers: &Handlers, i: usize) -> Result<Option<u32>, E> {
        Ok(handlers.borrow().get(i).copied().flatten())
    }
    fn invoke(&self, handler: u32, data: &V) -> Result<(), E> {
        assert!(equal(data, &self.parsed), "native parsed data identity");
        self.hit(Event::Value(format!("handler:{handler}"), snap(data)))?;
        if handler == 11 {
            if self.i == 0 {
                if let V::Obj(data) = data {
                    data.borrow_mut().marker = Some(91)
                }
            }
            if self.i == 9 {
                let array = self.handlers.borrow().clone();
                array.borrow_mut()[1] = None;
                array.borrow_mut().push(Some(40));
            }
            if self.i == 11 {
                let array = self.handlers.borrow().clone();
                array.borrow_mut()[1] = Some(40);
                array.borrow_mut().push(Some(99));
            }
        }
        Ok(())
    }
    fn refresh_positions(&self, trigger: &'static str) -> Result<(), E> {
        self.hit(Event::Refresh(trigger))
    }
}
impl SubscriptionHost for Host {
    type Callback = Callback;
    type Callbacks = Set;
    fn has_asset(&self, t: &[u16]) -> Result<bool, E> {
        self.tok("asset:has", t)?;
        Ok(self.assets.borrow().iter().any(|v| v == t))
    }
    fn insert_asset(&self, t: &[u16]) -> Result<(), E> {
        self.tok("asset:add", t)?;
        if !self.assets.borrow().iter().any(|v| v == t) {
            self.assets.borrow_mut().push(t.to_vec())
        }
        Ok(())
    }
    fn delete_asset(&self, t: &[u16]) -> Result<(), E> {
        self.tok("asset:delete", t)?;
        self.assets.borrow_mut().retain(|v| v != t);
        Ok(())
    }
    fn connected(&self) -> bool {
        self.hit(Event::Flag("connected".into(), self.i != 2))
            .unwrap();
        self.i != 2
    }
    fn send_operation(&self, frame: AssetOperation) -> Result<(), E> {
        assert_eq!(
            frame.assets_ids.len(),
            1,
            "independent native operation asset count"
        );
        assert_eq!(frame.assets_ids[0], token(self.i), "native operation token");
        self.hit(Event::Send(frame))
    }
    fn has_callbacks(&self, t: &[u16]) -> Result<bool, E> {
        self.tok("registry:has", t)?;
        Ok(self.registry.borrow().iter().any(|(k, _)| k == t))
    }
    fn insert_callback_set(&self, t: &[u16]) -> Result<(), E> {
        self.tok("registry:set", t)?;
        self.registry
            .borrow_mut()
            .push((t.to_vec(), Rc::new(RefCell::new(vec![]))));
        Ok(())
    }
    fn callbacks(&self, t: &[u16]) -> Result<Option<Set>, E> {
        self.tok("registry:get", t)?;
        Ok(self
            .registry
            .borrow()
            .iter()
            .find(|(k, _)| k == t)
            .map(|(_, s)| s.clone()))
    }
    fn add_callback(&self, s: &Set, c: Callback) -> Result<(), E> {
        assert!(Rc::ptr_eq(&c, &self.callback), "native callback identity");
        self.hit(Event::Id("callback:add".into(), *c))?;
        if !s.borrow().iter().any(|v| Rc::ptr_eq(v, &c)) {
            s.borrow_mut().push(c)
        }
        Ok(())
    }
    fn delete_callback(&self, s: &Set, c: &Callback) -> Result<(), E> {
        assert!(Rc::ptr_eq(c, &self.callback));
        self.hit(Event::Id("callback:delete".into(), **c))?;
        s.borrow_mut().retain(|v| !Rc::ptr_eq(v, c));
        Ok(())
    }
    fn callback_size(&self, s: &Set) -> usize {
        s.borrow().len()
    }
    fn delete_callback_set(&self, t: &[u16]) -> Result<(), E> {
        self.tok("registry:delete", t)?;
        self.registry.borrow_mut().retain(|(k, _)| k != t);
        Ok(())
    }
    fn subscribe_asset(&self, t: &[u16]) -> Result<(), E> {
        self.tok("child:subscribe", t)?;
        safe_cex_venue_polymarket::ws_dispatch::subscribe_asset(self, t)
    }
    fn unsubscribe_asset(&self, t: &[u16]) -> Result<(), E> {
        self.tok("child:unsubscribe", t)?;
        safe_cex_venue_polymarket::ws_dispatch::unsubscribe_asset(self, t)
    }
    fn missing_callbacks_error(&self) -> E {
        self.failure.clone()
    }
}
impl TradeHost for Host {
    fn token_symbol(&self, t: &V) -> Result<V, E> {
        self.tag("token:map")?;
        if self.i == 10 {
            return Ok(V::U);
        }
        assert!(equal(t, &self.trade_token), "native token lookup identity");
        self.hit(Event::Value("token:lookup".into(), snap(t)))?;
        Ok(match self.i {
            1 => V::U,
            2 => V::Text(vec![]),
            8 => V::Num(1.0),
            _ => V::Text(u("symbol")),
        })
    }
    fn update_ticker(&self, id: &V, update: LastTradePatch<V>) -> Result<(), E> {
        assert!(equal(id, &self.trade_token));
        assert!(equal(&update.id, &self.trade_token));
        *self.patch.borrow_mut() = Some((id.clone(), update.clone()));
        self.hit(Event::Update(
            snap(id),
            snap(&update.id),
            snap(&update.symbol),
            update.last,
        ))
    }
}
fn oracle() -> &'static Value {
    static CACHE: OnceLock<Result<Value, String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let r = Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/ws_dispatch_oracle.cjs"
                ))
                .output()
                .map_err(|e| e.to_string())?;
            if !r.status.success() {
                return Err(String::from_utf8_lossy(&r.stderr).into_owned());
            }
            serde_json::from_slice(&r.stdout).map_err(|e| e.to_string())
        })
        .as_ref()
        .unwrap_or_else(|e| panic!("source oracle:{e}"))
}
fn decoded(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|v| u16::try_from(v.as_u64().unwrap()).unwrap())
        .collect()
}
fn value_assert(v: &Snapshot, s: &Value) {
    match v {
        Snapshot::U => assert_eq!(s["kind"], "undefined"),
        Snapshot::N => assert_eq!(s["kind"], "null"),
        Snapshot::B(v) => {
            assert_eq!(s["kind"], "bool");
            assert_eq!(s["value"], *v)
        }
        Snapshot::Symbol => assert_eq!(s["kind"], "symbol"),
        Snapshot::Num(v) => {
            if v.is_nan() {
                assert_eq!(s["kind"], "nan")
            } else {
                assert_eq!(s["kind"], "number");
                assert_eq!(
                    v.to_bits(),
                    u64::from_str_radix(s["bits"].as_str().unwrap(), 16).unwrap(),
                    "native Number bits"
                )
            }
        }
        Snapshot::Text(v) => {
            assert_eq!(s["kind"], "text");
            assert_eq!(*v, decoded(&s["units"]))
        }
        Snapshot::Obj(marker) => {
            assert_eq!(s["kind"], "object");
            if let Some(marker) = marker {
                assert_eq!(s["marker"], *marker)
            } else {
                assert!(s["marker"].is_null())
            }
        }
    }
}
fn event_assert(e: &Event, s: &Value) {
    match e {
        Event::Tag(_) => {}
        Event::Flag(_, v) => assert_eq!(s["value"], *v),
        Event::Socket(_, id) => {
            if let Some(id) = id {
                assert_eq!(s["id"], *id)
            } else {
                assert!(s["id"].is_null())
            }
        }
        Event::Id(_, id) => assert_eq!(s["id"], *id),
        Event::Text(tag, text) => assert_eq!(
            *text,
            decoded(
                &s[if ["url", "create"].contains(&tag.as_str()) {
                    "text"
                } else {
                    "token"
                }]
            )
        ),
        Event::Value(tag, v) => value_assert(
            v,
            &s[if tag == "parse" {
                "raw"
            } else if tag == "stats" {
                "topic"
            } else if tag == "token:lookup" {
                "token"
            } else {
                "data"
            }],
        ),
        Event::Log(text, severity) => {
            assert_eq!(*text, decoded(&s["text"]));
            assert_eq!(s["severity"], *severity)
        }
        Event::Add(_, socket, listener) => {
            assert_eq!(s["socket"], *socket);
            if let Some(id) = listener {
                assert_eq!(s["listener"], *id)
            } else {
                assert!(s["listener"].is_null())
            }
        }
        Event::Send(frame) => {
            assert_eq!(frame.assets_ids.len(), 1);
            assert_eq!(s["assets"].as_array().unwrap().len(), 1);
            assert_eq!(frame.assets_ids[0], decoded(&s["assets"][0]));
            assert_eq!(s["operation"], frame.operation)
        }
        Event::Refresh(trigger) => assert_eq!(s["trigger"], *trigger, "native refresh trigger"),
        Event::Update(selector, id, symbol, last) => {
            value_assert(selector, &s["selector"]);
            value_assert(id, &s["id"]);
            value_assert(symbol, &s["symbol"]);
            value_assert(&Snapshot::Num(*last), &s["last"])
        }
    }
}
fn error_assert(h: &Host, result: Result<(), E>, source: &Value) {
    match result {
        Ok(()) => assert!(source.is_null(), "native unexpected success"),
        Err(e) => {
            assert!(!source.is_null(), "native unexpected failure");
            if Rc::ptr_eq(&e, &h.failure) {
                assert_eq!(source["same"], true);
                assert_eq!(e.marker, 47);
                assert_eq!(source["marker"], 47);
                assert!(e.name.is_none())
            } else {
                assert!(
                    Rc::ptr_eq(&e, &h.type_error),
                    "native coercion error identity"
                );
                assert_eq!(e.marker, 83);
                assert_eq!(source["same"], false);
                assert_eq!(source["name"], "TypeError");
                assert_eq!(e.name, Some("TypeError"))
            }
            assert_eq!(source["message"], e.message)
        }
    }
}
fn expected_tags(venue: &str, name: &str, i: usize) -> Vec<String> {
    let mut tags: Vec<&str> = vec![];
    match name {
        "connectAndSubscribe" => {
            if venue == "private" {
                tags.push("credentials");
                if i == 3 {
                    tags.push("log");
                    return tags.into_iter().map(str::to_owned).collect();
                }
            }
            tags.push("disposed");
            if i == 1 {
                return tags.into_iter().map(str::to_owned).collect();
            }
            tags.push("socket");
            if i == 2 {
                return tags.into_iter().map(str::to_owned).collect();
            }
            tags.push("log");
            if i == 4 {
                return tags.into_iter().map(str::to_owned).collect();
            }
            tags.extend(["url", "create"]);
            if i == 5 {
                return tags.into_iter().map(str::to_owned).collect();
            }
            tags.push("assign");
            for event in ["open", "message", "close"] {
                tags.push("socket");
                tags.push(match event {
                    "open" => "listener:open",
                    "message" => "listener:message",
                    _ => "listener:close",
                });
                tags.push(match event {
                    "open" => "add:open",
                    "message" => "add:message",
                    _ => "add:close",
                });
                if i == 6 && event == "message" {
                    return tags.into_iter().map(str::to_owned).collect();
                }
            }
            tags.extend(["socket", "add:error"]);
            if i != 10 {
                tags.push("field:message")
            }
            if i != 11 {
                tags.push("log")
            }
        }
        "onMessage" => {
            tags.extend(["track", "event:data"]);
            if i == 1 {
                tags.push("pong")
            } else {
                tags.extend(["event:data", "parse"]);
                if i != 2 && i != 3 {
                    tags.push("field:event_type");
                    if i == 10 {
                        tags.push("field:type")
                    }
                    if i == 4 {
                        tags.push("pong")
                    } else if venue == "private" && (i == 5 || i == 6) {
                        if i == 6 {
                            tags.push("field:authenticated")
                        }
                        tags.extend(["log", "refresh"])
                    } else if venue == "private" && (i == 7 || i == 8) {
                        tags.push("field:authenticated");
                        if i == 8 {
                            tags.push("field:error")
                        }
                        tags.extend(["field:error", "field:message", "log"])
                    } else {
                        if venue == "private" {
                            tags.extend(["field:authenticated", "field:error"])
                        }
                        if i != 5 && i != 7 {
                            tags.extend(["stats", "handler:11"]);
                            if i != 9 {
                                tags.push(if i == 11 { "handler:40" } else { "handler:13" });
                            }
                            if i == 10 {
                                tags.push("handler:40");
                            }
                        }
                    }
                }
            }
        }
        "subscribeToAsset" => {
            tags.push("asset:has");
            if i % 3 != 1 {
                tags.extend(["asset:add", "connected"]);
                if i != 2 {
                    tags.push("send")
                }
            }
        }
        "unsubscribeFromAsset" => {
            tags.push("asset:has");
            if i % 3 == 1 {
                tags.extend(["asset:delete", "connected"]);
                if i != 2 {
                    tags.push("send")
                }
            }
        }
        "subscribeToOrderBook" => {
            tags.push("registry:has");
            if i % 4 != 1 && i != 3 {
                tags.push("registry:set")
            }
            tags.extend(["registry:get", "callback:add"]);
            if i == 5 {
                return tags.into_iter().map(str::to_owned).collect();
            }
            tags.push("child:subscribe");
            if i == 6 {
                return tags.into_iter().map(str::to_owned).collect();
            }
            tags.push("asset:has");
            if i % 3 != 1 {
                tags.extend(["asset:add", "connected"]);
                if i != 2 {
                    tags.push("send")
                }
            }
            for tick in 0..2 {
                tags.push("registry:get");
                if tick == 0 || i == 3 || i == 10 {
                    tags.push("callback:delete");
                    if i != 3 {
                        tags.extend(["registry:delete", "child:unsubscribe", "asset:has"]);
                        if tick == 0 {
                            tags.extend(["asset:delete", "connected"]);
                            if i != 2 {
                                tags.push("send")
                            }
                        }
                    }
                }
            }
        }
        "handleLastTradePrice" => {
            tags.extend(["field:asset_id", "token:map"]);
            if i != 10 {
                tags.push("token:lookup")
            }
            tags.push("field:price");
            if [0, 4, 5, 7, 8, 9, 11].contains(&i) {
                tags.push("update")
            }
        }
        _ => unreachable!(),
    };
    tags.into_iter().map(str::to_owned).collect()
}
fn check(venue: &'static str, name: &'static str) {
    let all = oracle()["cases"].as_array().unwrap();
    assert_eq!(all.len(), 96, "independent eight by twelve fixture floor");
    assert_eq!(all[95]["name"], "handleLastTradePrice");
    assert_eq!(all[95]["index"], 11);
    let cases: Vec<_> = all
        .iter()
        .filter(|c| c["venue"] == venue && c["name"] == name)
        .collect();
    assert_eq!(cases.len(), 12, "independent method fixture floor");
    for (i, c) in cases.iter().enumerate() {
        assert_eq!(c["index"], i);
        assert_eq!(decoded(&c["token"]), token(i));
        let h = Rc::new(Host::new(venue, name, i, decoded(&c["raw"])));
        let mut returned = None;
        let root = match (venue, name) {
            ("public", "connectAndSubscribe") => connect_public(h.clone()),
            ("private", "connectAndSubscribe") => connect_private(h.clone()),
            ("public", "onMessage") => message_public(&*h, &()),
            ("private", "onMessage") => message_private(&*h, &()),
            (_, "subscribeToAsset") => subscribe_asset(&*h, &token(i)),
            (_, "unsubscribeFromAsset") => unsubscribe_asset(&*h, &token(i)),
            (_, "subscribeToOrderBook") => subscribe_book(h.clone(), &token(i), h.callback.clone())
                .map(|cb| returned = Some(cb)),
            (_, "handleLastTradePrice") => last_trade(&*h, &h.trade_data),
            _ => unreachable!(),
        };
        error_assert(&h, root, &c["error"]);
        assert_eq!(c["hasReturn"], returned.is_some());
        let mut ticks = vec![];
        if let Some(cb) = h.errors.borrow().first().cloned() {
            let event = if i == 10 {
                V::N
            } else {
                obj(
                    None,
                    vec![(
                        "message",
                        if i == 11 {
                            V::Symbol
                        } else if i == 3 {
                            V::Num(0.0)
                        } else {
                            V::Text(u("broken").into_iter().chain([0xd800]).collect())
                        },
                    )],
                )
            };
            ticks.push(cb(event))
        }
        if let Some(cb) = returned {
            for tick in 0..2 {
                if i == 10 && tick == 1 {
                    h.registry
                        .borrow_mut()
                        .push((token(i), Rc::new(RefCell::new(vec![h.callback.clone()]))));
                }
                ticks.push(cb())
            }
        }
        assert_eq!(
            ticks.len(),
            if name == "connectAndSubscribe"
                && ![1, 2, 4, 5, 6].contains(&i)
                && !(venue == "private" && i == 3)
            {
                1
            } else if name == "subscribeToOrderBook" && i != 5 && i != 6 {
                2
            } else {
                0
            },
            "independent replay count:{venue}.{name} index{i}"
        );
        assert_eq!(c["ticks"].as_array().unwrap().len(), ticks.len());
        for (tick, source) in ticks.into_iter().zip(c["ticks"].as_array().unwrap()) {
            error_assert(&h, tick, source)
        }
        let tags = expected_tags(venue, name, i);
        let events = h.events.borrow();
        assert_eq!(
            events.len(),
            tags.len(),
            "native dispatch trace:{venue}.{name} index{i}"
        );
        assert_eq!(
            c["trace"].as_array().unwrap().len(),
            tags.len(),
            "independent source dispatch trace floor"
        );
        for (j, (event, tag)) in events.iter().zip(tags).enumerate() {
            assert_eq!(
                event.tag(),
                tag,
                "native dispatch trace:{venue}.{name} index{i}"
            );
            assert_eq!(c["trace"][j]["tag"], tag);
            event_assert(event, &c["trace"][j]);
        }
        if let Some((selector, patch)) = &*h.patch.borrow() {
            assert!(equal(selector, &h.trade_token));
            assert!(equal(&patch.id, &h.trade_token));
            let expected: f64 = match i {
                0 => 12.5,
                4 => -0.0,
                5 => 0.25,
                7 => 12.0,
                8 => 0.5,
                9 => 0.1,
                11 => 3.0,
                _ => panic!("native unexpected ticker patch"),
            };
            assert_eq!(
                patch.last.to_bits(),
                expected.to_bits(),
                "native last-trade numeric result"
            );
            assert!(equal(
                &patch.symbol,
                &if i == 8 {
                    V::Num(1.0)
                } else {
                    V::Text(u("symbol"))
                }
            ));
        }
        let initial_asset = i % 3 == 1;
        let initial_callbacks = || {
            if i == 3 {
                Some(vec![99])
            } else if i % 4 == 1 {
                Some(vec![])
            } else {
                None
            }
        };
        let (has_asset, callback_ids): (bool, Option<Vec<u32>>) = match name {
            "subscribeToAsset" => (true, initial_callbacks()),
            "unsubscribeFromAsset" => (false, initial_callbacks()),
            "subscribeToOrderBook" => match i {
                3 => (true, Some(vec![99])),
                5 => (initial_asset, Some(vec![])),
                6 => (initial_asset, Some(vec![37])),
                _ => (false, None),
            },
            _ => (initial_asset, initial_callbacks()),
        };
        assert_eq!(
            h.assets.borrow().len(),
            usize::from(has_asset),
            "independent native asset count"
        );
        assert_eq!(
            c["assets"].as_array().unwrap().len(),
            usize::from(has_asset),
            "independent source asset count"
        );
        if has_asset {
            assert_eq!(h.assets.borrow()[0], token(i));
            assert_eq!(decoded(&c["assets"][0]), token(i));
        }
        assert_eq!(
            h.registry.borrow().len(),
            usize::from(callback_ids.is_some()),
            "independent native registry count"
        );
        assert_eq!(
            c["registry"].as_array().unwrap().len(),
            usize::from(callback_ids.is_some()),
            "independent source registry count"
        );
        if let Some(ids) = callback_ids {
            let registry = h.registry.borrow();
            let (key, set) = &registry[0];
            assert_eq!(*key, token(i));
            assert_eq!(decoded(&c["registry"][0][0]), token(i));
            assert_eq!(
                set.borrow().len(),
                ids.len(),
                "independent native callback count"
            );
            assert_eq!(
                c["registry"][0][1].as_array().unwrap().len(),
                ids.len(),
                "independent source callback count"
            );
            for (j, id) in ids.into_iter().enumerate() {
                assert_eq!(*set.borrow()[j], id);
                assert_eq!(c["registry"][0][1][j], id);
            }
        }
        drop(events);
        h.errors.borrow_mut().clear();
    }
    assert_eq!(cases[11]["index"], 11, "last method fixture");
}
macro_rules! row {
    ($test:ident,$venue:literal,$name:literal) => {
        #[test]
        fn $test() {
            check($venue, $name)
        }
    };
}
row!(public_connect, "public", "connectAndSubscribe");
row!(private_connect, "private", "connectAndSubscribe");
row!(public_message, "public", "onMessage");
row!(private_message, "private", "onMessage");
row!(asset_subscribe, "public", "subscribeToAsset");
row!(asset_unsubscribe, "public", "unsubscribeFromAsset");
row!(book_subscribe, "public", "subscribeToOrderBook");
row!(last_trade_price, "public", "handleLastTradePrice");

#[test]
fn inherited_truthy_handlers_record_stats_then_fail() {
    let raw = u("{\"event_type\":\"constructor\"}");
    let mut host = Host::new("public", "onMessage", 0, raw);
    host.parsed = obj(None, vec![("event_type", V::Text(u("constructor")))]);
    host.invalid_handlers.set(true);
    let result = message_public(&host, &()).unwrap_err();
    assert!(
        Rc::ptr_eq(&result, &host.handler_error),
        "native inherited handler failure identity"
    );
    assert_eq!(result.marker, 97);
    assert_eq!(result.name, Some("TypeError"));
    let expected = [
        "track",
        "event:data",
        "event:data",
        "parse",
        "field:event_type",
        "stats",
    ];
    assert_eq!(
        host.events.borrow().len(),
        6,
        "independent invalid-handler trace count"
    );
    for (event, tag) in host.events.borrow().iter().zip(expected) {
        assert_eq!(event.tag(), tag);
    }
    assert_eq!(
        oracle()["invalidHandler"]["tags"].as_array().unwrap().len(),
        6
    );
    for (j, tag) in expected.iter().enumerate() {
        assert_eq!(oracle()["invalidHandler"]["tags"][j], *tag);
    }
    assert_eq!(oracle()["invalidHandler"]["name"], "TypeError");
    assert_eq!(oracle()["invalidHandler"]["message"], result.message);
}
