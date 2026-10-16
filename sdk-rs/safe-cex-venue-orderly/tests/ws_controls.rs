use safe_cex_venue_orderly::ws_controls::{self as ws, WsHost};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
};
#[derive(Debug)]
enum Raw {
    Undefined,
    Null,
    Bool(bool),
    Number(f64),
    Text(Vec<u16>),
    Array(RefCell<Vec<Rc<Raw>>>),
    Object(RefCell<Vec<(Vec<u16>, Rc<Raw>)>>),
    Function(&'static str),
}
type V = Rc<Raw>;
type E = Rc<&'static str>;
fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}
fn text(s: &str) -> V {
    Rc::new(Raw::Text(units(s)))
}
fn array(a: Vec<V>) -> V {
    Rc::new(Raw::Array(RefCell::new(a)))
}
fn object(a: Vec<(&str, V)>) -> V {
    Rc::new(Raw::Object(RefCell::new(
        a.into_iter().map(|(k, v)| (units(k), v)).collect(),
    )))
}
fn decode(v: &Value) -> V {
    Rc::new(match v {
        Value::Null => Raw::Null,
        Value::Bool(b) => Raw::Bool(*b),
        Value::Number(n) => Raw::Number(n.as_f64().unwrap()),
        Value::String(s) => Raw::Text(units(s)),
        Value::Array(a) => Raw::Array(RefCell::new(a.iter().map(decode).collect())),
        Value::Object(o) => {
            if o.get("undefined") == Some(&Value::Bool(true)) {
                Raw::Undefined
            } else if let Some(n) = o.get("number") {
                Raw::Number(f64::from_bits(
                    u64::from_str_radix(n.as_str().unwrap(), 16).unwrap(),
                ))
            } else {
                Raw::Object(RefCell::new(
                    o.iter().map(|(k, v)| (units(k), decode(v))).collect(),
                ))
            }
        }
    })
}
fn get(v: &Raw, key: &str) -> V {
    if let Raw::Object(o) = v {
        o.borrow()
            .iter()
            .find(|(k, _)| *k == units(key))
            .map(|(_, v)| v.clone())
            .unwrap_or(Rc::new(Raw::Undefined))
    } else {
        Rc::new(Raw::Undefined)
    }
}
fn set(v: &V, key: &str, next: V) {
    let Raw::Object(o) = &**v else {
        panic!("ordinary fixture object")
    };
    let mut o = o.borrow_mut();
    if let Some((_, value)) = o.iter_mut().find(|(k, _)| *k == units(key)) {
        *value = next
    } else {
        o.push((units(key), next))
    }
}
fn boolean(b: bool) -> V {
    Rc::new(Raw::Bool(b))
}
fn flag(v: &Raw, key: &str) -> bool {
    matches!(&*get(v, key), Raw::Bool(true))
}
fn items(v: &Raw) -> Vec<V> {
    let Raw::Array(a) = v else {
        panic!("fixture array")
    };
    a.borrow().clone()
}
fn string(v: &Raw) -> Vec<u16> {
    match v {
        Raw::Undefined => units("undefined"),
        Raw::Null => units("null"),
        Raw::Bool(b) => units(if *b { "true" } else { "false" }),
        Raw::Number(n) => units(&safe_cex_num::js_number_string(*n)),
        Raw::Text(s) => s.clone(),
        Raw::Object(_) => units("[object Object]"),
        Raw::Array(a) => a
            .borrow()
            .iter()
            .map(|v| String::from_utf16(&string(v)).unwrap())
            .collect::<Vec<_>>()
            .join(",")
            .encode_utf16()
            .collect(),
        Raw::Function(_) => panic!("outside fixture coercion floor"),
    }
}
fn ordered(v: &Raw) -> Vec<(Vec<u16>, V)> {
    let Raw::Object(o) = v else {
        panic!("fixture own entries")
    };
    let mut a = o.borrow().clone();
    a.sort_by_key(|(k, _)| {
        let s = String::from_utf16(k).unwrap();
        let n = s
            .parse::<u32>()
            .ok()
            .filter(|n| *n != u32::MAX && n.to_string() == s);
        (n.is_none(), n.unwrap_or(0))
    });
    a
}
fn native(v: &Raw, expected: &Value, path: &str) {
    match v {
        Raw::Undefined => assert_eq!(
            expected.get("undefined"),
            Some(&Value::Bool(true)),
            "native undefined {path}"
        ),
        Raw::Null => assert!(expected.is_null(), "native null {path}"),
        Raw::Bool(b) => assert_eq!(expected.as_bool(), Some(*b), "native boolean {path}"),
        Raw::Number(n) => {
            let bits = u64::from_str_radix(expected["number"].as_str().unwrap(), 16).unwrap();
            if n.is_nan() {
                assert!(f64::from_bits(bits).is_nan(), "native NaN {path}")
            } else {
                assert_eq!(n.to_bits(), bits, "native bits {path}")
            }
        }
        Raw::Text(s) => {
            assert!(expected.is_string(), "native text kind {path}");
            assert_eq!(
                s,
                &units(expected.as_str().unwrap()),
                "native UTF-16 {path}"
            );
        }
        Raw::Array(a) => {
            assert!(expected.is_array(), "native array kind {path}");
            let a = a.borrow();
            let e = expected.as_array().unwrap();
            assert_eq!(a.len(), e.len(), "native array cardinality {path}");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                native(a, e, &format!("{path}[{i}]"))
            }
        }
        Raw::Object(o) => {
            assert!(expected.is_object(), "native object kind {path}");
            let o = o.borrow();
            let e = expected.as_object().unwrap();
            assert_eq!(o.len(), e.len(), "native object cardinality {path}");
            for (k, v) in o.iter() {
                let k = String::from_utf16(k).unwrap();
                assert!(e.contains_key(&k), "native object key {path}.{k}");
                native(v, &e[&k], &format!("{path}.{k}"))
            }
        }
        Raw::Function(_) => panic!("no callback encoding assertion"),
    }
}
struct Host {
    cfg: V,
    receiver: V,
    parent: V,
    socket: V,
    parsed: V,
    original: E,
    events: RefCell<Vec<V>>,
    encoded: RefCell<Option<V>>,
    count: Cell<usize>,
    append: Cell<bool>,
    private: bool,
}
impl Host {
    fn effect(&self, kind: &str, details: Vec<(&str, V)>) -> Result<(), E> {
        let mut fields = vec![("kind", text(kind))];
        fields.extend(details);
        self.events.borrow_mut().push(object(fields));
        if string(&get(&self.cfg, "fail")) == units(kind) {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn run(&self, name: &str) -> Result<(), E> {
        let r = self.receiver.clone();
        match (self.private, name) {
            (false, "connectAndSubscribe") => ws::public_connect(self, r),
            (true, "connectAndSubscribe") => ws::private_connect(self, r),
            (false, "onOpen") => ws::public_open(self, r),
            (true, "onOpen") => ws::private_open(self, r),
            (false, "subscribe") => ws::public_subscribe(self, r),
            (true, "subscribe") => ws::private_subscribe(self, r),
            (false, "ping") => ws::public_ping(self, r),
            (true, "ping") => ws::private_ping(self, r),
            (false, "handlePingEvent") => ws::public_handle_ping(self, r),
            (false, "onMessage") => {
                ws::public_message(self, r, object(vec![("data", get(&self.cfg, "data"))]))
            }
            _ => panic!("outside new body floor"),
        }
    }
}
impl WsHost for Host {
    type Value = Raw;
    type Error = E;
    type Iterator = (V, usize);
    fn get(&self, v: &V, key: &[u16]) -> Result<V, E> {
        Ok(get(v, &String::from_utf16(key).unwrap()))
    }
    fn set(&self, v: &V, key: &[u16], next: V) -> Result<(), E> {
        set(v, &String::from_utf16(key).unwrap(), next);
        Ok(())
    }
    fn truthy(&self, v: &Raw) -> bool {
        match v {
            Raw::Undefined | Raw::Null => false,
            Raw::Bool(b) => *b,
            Raw::Number(n) => *n != 0.0 && !n.is_nan(),
            Raw::Text(s) => !s.is_empty(),
            _ => true,
        }
    }
    fn string(&self, v: &Raw) -> Result<Vec<u16>, E> {
        Ok(string(v))
    }
    fn text(&self, s: &[u16]) -> V {
        Rc::new(Raw::Text(s.to_vec()))
    }
    fn object(&self, entries: Vec<(&'static str, V)>) -> Result<V, E> {
        for (key, value) in &entries {
            if *key == "orderly_key" {
                assert!(
                    Rc::ptr_eq(value, &get(&get(&self.parent, "options"), "key")),
                    "native original key allocation"
                );
            }
        }
        Ok(object(entries))
    }
    fn stringify(&self, v: V) -> Result<V, E> {
        *self.encoded.borrow_mut() = Some(v.clone());
        let keys = array(ordered(&v).iter().map(|(k, _)| self.text(k)).collect());
        let params = get(&v, "params");
        let params_keys = if matches!(&*params, Raw::Object(_)) {
            array(ordered(&params).iter().map(|(k, _)| self.text(k)).collect())
        } else {
            array(vec![])
        };
        self.effect(
            "encode",
            vec![
                ("value", v.clone()),
                ("keys", keys),
                ("paramsKeys", params_keys),
            ],
        )?;
        Ok(v)
    }
    fn parse(&self, v: V) -> Result<V, E> {
        assert!(
            Rc::ptr_eq(&v, &get(&self.cfg, "data")),
            "original data forwarded"
        );
        self.effect("parse", vec![("data", v)])?;
        Ok(self.parsed.clone())
    }
    fn includes(&self, v: &V, needle: &[u16]) -> Result<bool, E> {
        let Raw::Text(s) = &**v else {
            panic!("fixture string includes")
        };
        Ok(needle.is_empty() || s.windows(needle.len()).any(|w| w == needle))
    }
    fn own_values(&self, v: V) -> Result<Vec<V>, E> {
        Ok(ordered(&v).into_iter().map(|(_, v)| v).collect())
    }
    fn flatten_one(&self, values: Vec<V>) -> Result<Vec<V>, E> {
        let mut out = vec![];
        for v in values {
            if let Raw::Array(a) = &*v {
                out.extend(a.borrow().iter().cloned())
            } else {
                out.push(v)
            }
        }
        Ok(out)
    }
    fn entries(&self, v: V) -> Result<Vec<(Vec<u16>, V)>, E> {
        Ok(ordered(&v))
    }
    fn iterator(&self, v: V) -> Result<Self::Iterator, E> {
        assert!(matches!(&*v, Raw::Array(_)));
        Ok((v, 0))
    }
    fn next(&self, i: &mut Self::Iterator) -> Result<Option<V>, E> {
        let Raw::Array(a) = &*i.0 else { unreachable!() };
        let next = a.borrow().get(i.1).cloned();
        i.1 += 1;
        Ok(next)
    }
    fn call(&self, f: V, receiver: Option<V>, args: Vec<V>) -> Result<V, E> {
        let Raw::Function(name) = &*f else {
            panic!("native original callable")
        };
        let is_receiver = receiver
            .as_ref()
            .is_some_and(|r| Rc::ptr_eq(r, &self.receiver));
        match *name {
            "createWebsocket" => {
                self.effect(
                    "create",
                    vec![("receiver", boolean(is_receiver)), ("url", args[0].clone())],
                )?;
                return Ok(self.socket.clone());
            }
            "addEventListener" => {
                let event = String::from_utf16(&string(&args[0])).unwrap();
                let field = match event.as_str() {
                    "open" => "onOpen",
                    "message" => "onMessage",
                    "close" => "onClose",
                    _ => panic!(),
                };
                self.effect(
                    &format!("listener:{event}"),
                    vec![
                        (
                            "socket",
                            boolean(
                                receiver
                                    .as_ref()
                                    .is_some_and(|r| Rc::ptr_eq(r, &self.socket)),
                            ),
                        ),
                        (
                            "identity",
                            boolean(Rc::ptr_eq(&args[1], &get(&self.receiver, field))),
                        ),
                    ],
                )?
            }
            "send" | "sendWithLog" => {
                let n = self.count.get() + 1;
                self.count.set(n);
                self.effect(
                    &format!("send:{n}"),
                    vec![
                        ("log", boolean(*name == "sendWithLog")),
                        ("receiver", boolean(is_receiver)),
                        (
                            "encoded",
                            boolean(
                                self.encoded
                                    .borrow()
                                    .as_ref()
                                    .is_some_and(|v| Rc::ptr_eq(v, &args[0])),
                            ),
                        ),
                        ("value", args[0].clone()),
                    ],
                )?;
                if flag(&self.cfg, "mutateDisposed") {
                    set(&self.receiver, "isDisposed", boolean(true))
                }
            }
            "ping" | "subscribe" => {
                assert!(is_receiver, "native current receiver forwarding");
                self.run(name)?
            }
            "emitWsReconnected" => {
                self.effect("reconnected", vec![("receiver", boolean(is_receiver))])?
            }
            "trackLastReceived" => {
                self.effect("received", vec![("receiver", boolean(is_receiver))])?
            }
            "fetchAndSetPrivateSnapshot" => self.effect(
                "snapshot",
                vec![(
                    "receiver",
                    boolean(
                        receiver
                            .as_ref()
                            .is_some_and(|r| Rc::ptr_eq(r, &self.parent)),
                    ),
                )],
            )?,
            "a" | "b" | "c" | "x" | "appended" | "new" => {
                self.effect(
                    &format!("callback:{name}"),
                    vec![
                        ("receiver", boolean(receiver.is_none())),
                        ("identity", boolean(Rc::ptr_eq(&args[0], &self.parsed))),
                        ("json", args[0].clone()),
                    ],
                )?;
                let handlers = get(&self.receiver, "messageHandlers");
                if flag(&self.cfg, "append") && !self.append.replace(true) {
                    let list = get(&handlers, "tickers");
                    let Raw::Array(a) = &*list else { panic!() };
                    a.borrow_mut().push(Rc::new(Raw::Function("appended")))
                }
                if flag(&self.cfg, "mutateHandlers") {
                    set(
                        &handlers,
                        "tickersSuffix",
                        array(vec![Rc::new(Raw::Function("new"))]),
                    )
                }
            }
            _ => panic!("outside callable floor {name}"),
        };
        Ok(Rc::new(Raw::Undefined))
    }
    fn monotonic_now(&self) -> Result<V, E> {
        self.effect("clock", vec![])?;
        let n = get(&self.cfg, "clock");
        Ok(if matches!(&*n, Raw::Null | Raw::Undefined) {
            Rc::new(Raw::Number(19.25))
        } else {
            n
        })
    }
    fn virtual_now(&self) -> Result<V, E> {
        self.effect("virtual", vec![])?;
        Ok(Rc::new(Raw::Number(1723638896789.5)))
    }
    fn sign(&self, message: V, secret: V) -> Result<V, E> {
        let original = get(&get(&self.parent, "options"), "secret");
        self.effect(
            "sign",
            vec![
                ("message", message),
                ("identity", boolean(Rc::ptr_eq(&secret, &original))),
            ],
        )?;
        Ok(text("signed"))
    }
    fn reconnecting(&self) -> bool {
        flag(&self.cfg, "reconnect")
    }
}
fn setup(cfg: &Value, private: bool) -> Host {
    let cfg = decode(cfg);
    let application = if flag(&cfg, "applicationIdMissing") {
        Rc::new(Raw::Undefined)
    } else {
        get(&cfg, "applicationId")
    };
    let options = object(vec![
        ("testnet", get(&cfg, "testnet")),
        ("applicationId", application),
        ("key", get(&cfg, "key")),
        ("secret", get(&cfg, "secret")),
    ]);
    let parent = object(vec![
        ("options", options),
        (
            "fetchAndSetPrivateSnapshot",
            Rc::new(Raw::Function("fetchAndSetPrivateSnapshot")),
        ),
    ]);
    let handlers = object(vec![]);
    for entry in items(&get(&cfg, "handlers")) {
        let pair = items(&entry);
        let topic = String::from_utf16(&string(&pair[0])).unwrap();
        let callbacks = items(&pair[1])
            .iter()
            .map(|id| {
                let id = String::from_utf16(&string(id)).unwrap();
                Rc::new(Raw::Function(match id.as_str() {
                    "a" => "a",
                    "b" => "b",
                    "c" => "c",
                    "x" => "x",
                    _ => panic!(),
                }))
            })
            .collect();
        set(&handlers, &topic, array(callbacks));
    }
    let receiver = object(vec![
        ("parent", parent.clone()),
        ("isDisposed", boolean(flag(&cfg, "disposed"))),
        ("topics", get(&cfg, "topics")),
        ("messageHandlers", handlers),
        ("pingAt", text("old")),
        ("ws", text("old")),
    ]);
    for name in [
        "createWebsocket",
        "onOpen",
        "onMessage",
        "onClose",
        "ping",
        "subscribe",
        "send",
        "sendWithLog",
        "emitWsReconnected",
        "trackLastReceived",
    ] {
        set(&receiver, name, Rc::new(Raw::Function(name)))
    }
    let socket = object(vec![(
        "addEventListener",
        Rc::new(Raw::Function("addEventListener")),
    )]);
    let parsed = get(&cfg, "parsed");
    Host {
        cfg,
        receiver,
        parent,
        socket,
        parsed,
        original: Rc::new("controlled failure"),
        events: RefCell::new(vec![]),
        encoded: RefCell::new(None),
        count: Cell::new(0),
        append: Cell::new(false),
        private,
    }
}
fn oracle() -> &'static Value {
    static OUT: OnceLock<Value> = OnceLock::new();
    OUT.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/ws-controls-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["hashes"].as_object().unwrap().len(),
            3,
            "independent source file floor"
        );
        value
    })
}
fn check(family: &str, floor: usize) {
    let rows = oracle()["rows"][family].as_array().unwrap();
    assert_eq!(rows.len(), floor, "independent {family} fixture floor");
    for (i, row) in rows.iter().enumerate() {
        let h = setup(&row["cfg"], family.starts_with("private"));
        let result = h.run(family.split('.').nth(1).unwrap());
        if let Err(e) = result {
            assert!(
                Rc::ptr_eq(&e, &h.original),
                "native original error allocation"
            );
            assert_eq!(row["error"]["identity"], true);
            assert_eq!(row["error"]["message"], *e)
        } else {
            assert!(row["error"].is_null(), "source error {row}")
        };
        native(
            &Raw::Array(RefCell::new(h.events.borrow().clone())),
            &row["events"],
            &format!("{family}[{i}] effects"),
        );
        native(&get(&h.receiver, "pingAt"), &row["pingAt"], "pingAt");
        native(
            &get(&h.receiver, "isDisposed"),
            &row["disposed"],
            "disposed",
        );
        assert_eq!(
            Rc::ptr_eq(&get(&h.receiver, "ws"), &h.socket),
            row["ws"].as_bool().unwrap(),
            "native socket allocation"
        );
    }
}
#[test]
fn public_connect_native() {
    check("public.connectAndSubscribe", 5)
}
#[test]
fn private_connect_native() {
    check("private.connectAndSubscribe", 6)
}
#[test]
fn public_open_native() {
    check("public.onOpen", 6)
}
#[test]
fn private_open_native() {
    check("private.onOpen", 6)
}
#[test]
fn public_subscribe_native() {
    check("public.subscribe", 6)
}
#[test]
fn private_subscribe_native() {
    check("private.subscribe", 5)
}
#[test]
fn public_ping_native() {
    check("public.ping", 6)
}
#[test]
fn private_ping_native() {
    check("private.ping", 6)
}
#[test]
fn public_pong_native() {
    check("public.handlePingEvent", 4)
}
#[test]
fn public_message_native() {
    check("public.onMessage", 12)
}
