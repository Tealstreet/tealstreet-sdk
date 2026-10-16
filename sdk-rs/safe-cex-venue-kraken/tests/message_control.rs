use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_venue_kraken::{
    message_dispatch::{self, MessageHost},
    private_control::{self, AuthHost, RefreshCallback, RefreshHost, SignRequest, Subscription},
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    future::{Future, poll_fn},
    pin::pin,
    process::Command,
    rc::Rc,
    sync::OnceLock,
    task::{Context, Poll, Waker},
};

#[derive(Clone, Debug)]
enum Raw {
    U,
    Null,
    B(bool),
    N(f64),
    S(Vec<u16>),
    A(Vec<Rc<Raw>>),
    O(Vec<(String, Rc<Raw>)>),
}
fn text(s: &str) -> Rc<Raw> {
    Rc::new(Raw::S(s.encode_utf16().collect()))
}
fn raw(v: &Value) -> Rc<Raw> {
    Rc::new(match v {
        Value::Null => Raw::Null,
        Value::Bool(v) => Raw::B(*v),
        Value::Number(v) => Raw::N(v.as_f64().unwrap()),
        Value::String(v) => Raw::S(v.encode_utf16().collect()),
        Value::Array(v) => Raw::A(v.iter().map(raw).collect()),
        Value::Object(v) => Raw::O(v.iter().map(|(k, v)| (k.clone(), raw(v))).collect()),
    })
}
fn obj(fields: Vec<(&str, Rc<Raw>)>) -> Rc<Raw> {
    Rc::new(Raw::O(
        fields.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    ))
}
fn undefined() -> Rc<Raw> {
    Rc::new(Raw::U)
}
fn boolraw(v: bool) -> Rc<Raw> {
    Rc::new(Raw::B(v))
}
fn truth(v: &Raw) -> bool {
    match v {
        Raw::U | Raw::Null => false,
        Raw::B(v) => *v,
        Raw::N(v) => *v != 0.0 && !v.is_nan(),
        Raw::S(v) => !v.is_empty(),
        _ => true,
    }
}
fn string(v: &Raw) -> Vec<u16> {
    match v {
        Raw::U => "undefined".encode_utf16().collect(),
        Raw::Null => "null".encode_utf16().collect(),
        Raw::S(v) => v.clone(),
        Raw::B(v) => v.to_string().encode_utf16().collect(),
        Raw::N(v) => safe_cex_num::js_number_string(*v).encode_utf16().collect(),
        Raw::O(_) => "[object Object]".encode_utf16().collect(),
        Raw::A(v) => v
            .iter()
            .map(|v| {
                if matches!(**v, Raw::U | Raw::Null) {
                    String::new()
                } else {
                    String::from_utf16_lossy(&string(v))
                }
            })
            .collect::<Vec<_>>()
            .join(",")
            .encode_utf16()
            .collect(),
    }
}
fn json(v: &Raw) -> Value {
    match v {
        Raw::U | Raw::Null => Value::Null,
        Raw::B(v) => Value::Bool(*v),
        Raw::N(v) => serde_json::json!(v),
        Raw::S(v) => Value::String(String::from_utf16(v).unwrap()),
        Raw::A(v) => Value::Array(v.iter().map(|v| json(v)).collect()),
        Raw::O(v) => Value::Object(
            v.iter()
                .filter(|(_, v)| !matches!(**v, Raw::U))
                .map(|(k, v)| (k.clone(), json(v)))
                .collect(),
        ),
    }
}
fn assert_native(actual: &Raw, expected: &Value, label: &str) {
    match actual {
        Raw::U => assert_eq!(
            expected,
            &serde_json::json!({"u":true}),
            "{label} native undefined"
        ),
        Raw::Null => assert!(expected.is_null(), "{label} native null"),
        Raw::B(v) => assert_eq!(Some(*v), expected.as_bool(), "{label} native bool"),
        Raw::N(v) => {
            let bits = expected["n"]
                .as_str()
                .map(|s| u64::from_str_radix(s, 16).unwrap())
                .unwrap_or_else(|| panic!("{label} native number tag {expected}"));
            if f64::from_bits(bits).is_nan() {
                assert!(v.is_nan(), "{label} native NaN")
            } else {
                assert_eq!(v.to_bits(), bits, "{label} native number")
            }
        }
        Raw::S(v) => assert_eq!(
            v,
            &expected["s"]
                .as_array()
                .unwrap_or_else(|| panic!("{label} native UTF16 tag {expected}"))
                .iter()
                .map(|x| x.as_u64().unwrap() as u16)
                .collect::<Vec<_>>(),
            "{label} native UTF16"
        ),
        Raw::A(v) => {
            let expected = expected.as_array().unwrap();
            assert_eq!(v.len(), expected.len(), "{label} native array length");
            for (a, e) in v.iter().zip(expected) {
                assert_native(a, e, label)
            }
        }
        Raw::O(v) => {
            let expected = expected.as_object().unwrap();
            assert_eq!(v.len(), expected.len(), "{label} native object presence");
            for (k, a) in v {
                assert_native(a, &expected[k], label)
            }
        }
    }
}
struct Event {
    name: String,
    args: Vec<Rc<Raw>>,
}
struct Host {
    config: Value,
    family: String,
    events: RefCell<Vec<Event>>,
    state: RefCell<HashMap<&'static str, Rc<Raw>>>,
    original: RefCell<Option<ExchangeError>>,
    error_pointer: usize,
    faulted: Cell<bool>,
    parsed: RefCell<Option<Rc<Raw>>>,
    input: RefCell<Option<Rc<Raw>>>,
    released: Cell<bool>,
    loaded: RefCell<Rc<Raw>>,
    timer: RefCell<Rc<Raw>>,
    callback: Cell<Option<RefreshCallback>>,
}
impl Host {
    fn new(config: &Value, family: &str) -> Self {
        let pending = config
            .get("pending")
            .map(raw)
            .unwrap_or_else(|| text("challenge-雪"));
        let state = HashMap::from([
            ("pending", pending),
            ("original", text("old-original")),
            ("signed", text("old-signed")),
            ("authenticated", boolraw(true)),
            ("token", text("old")),
            ("waiting", boolraw(true)),
            ("key", text("K-雪")),
        ]);
        let mut original = ExchangeError::new("control failure");
        original.name = "ControlError".into();
        Self {
            config: config.clone(),
            family: family.into(),
            events: RefCell::new(vec![]),
            state: RefCell::new(state),
            error_pointer: (&*original.metadata as *const _) as usize,
            original: RefCell::new(Some(original)),
            faulted: Cell::new(false),
            parsed: RefCell::new(None),
            input: RefCell::new(None),
            released: Cell::new(false),
            loaded: RefCell::new(raw(
                &serde_json::json!({"markets":true,"orders":false,"custom":"old"}),
            )),
            timer: RefCell::new(raw(config.get("timer").unwrap_or(&serde_json::json!(0)))),
            callback: Cell::new(None),
        }
    }
    fn error(&self) -> ExchangeError {
        self.original
            .borrow_mut()
            .take()
            .expect("single original injected error")
    }
    fn original_identity(&self, error: &ExchangeError) -> bool {
        (&*error.metadata as *const _) as usize == self.error_pointer
    }
    fn flag(&self, k: &str) -> bool {
        self.config[k].as_bool().unwrap_or(false)
    }
    fn event(&self, name: &str, args: Vec<Rc<Raw>>) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event {
            name: name.into(),
            args,
        });
        if !self.faulted.get() && self.config["fail"].as_str() == Some(name) {
            self.faulted.set(true);
            Err(self.error())
        } else {
            Ok(())
        }
    }
    fn fresh(&self, name: &str) -> ExchangeError {
        let mut e = ExchangeError::new("bound intrinsic failure");
        e.name = name.into();
        e
    }
    fn get(&self, k: &'static str) -> Rc<Raw> {
        self.state
            .borrow()
            .get(k)
            .cloned()
            .unwrap_or_else(undefined)
    }
    fn compare(&self, row: &Value, label: &str) {
        let events = self.events.borrow();
        let expected = row["trace"].as_array().unwrap();
        assert_eq!(
            events.len(),
            expected.len(),
            "{label} native event count config {}",
            self.config
        );
        for (i, (a, e)) in events.iter().zip(expected).enumerate() {
            let e = e.as_array().unwrap();
            assert_eq!(
                a.name,
                e[0].as_str().unwrap(),
                "{label} native event {i} config {}",
                self.config
            );
            assert_eq!(a.args.len(), e.len() - 1, "{label} native arity");
            for (a, e) in a.args.iter().zip(&e[1..]) {
                assert_native(a, e, label)
            }
        }
    }
    fn result(&self, result: Result<(), ExchangeError>, row: &Value) {
        match result {
            Ok(()) => assert!(
                row["failure"].is_null(),
                "CONTROL native unexpected fulfillment {}",
                self.config
            ),
            Err(e) => {
                assert_eq!(
                    e.name,
                    row["failure"]["name"].as_str().unwrap(),
                    "CONTROL native error name"
                );
                assert_eq!(
                    self.original_identity(&e),
                    row["failure"]["identity"].as_bool().unwrap(),
                    "CONTROL native error identity"
                );
                if self.original_identity(&e) {
                    assert_eq!(e.message, row["failure"]["message"].as_str().unwrap())
                }
            }
        }
    }
}
impl MessageHost for Host {
    type Value = Raw;
    fn disposed(&self) -> Result<bool, ExchangeError> {
        self.event("disposed", vec![])?;
        Ok(self.flag("disposed"))
    }
    fn track_received(&self) -> Result<(), ExchangeError> {
        self.event("track", vec![])
    }
    fn parse_data(&self, data: &Raw, tolerant: bool) -> Result<Rc<Raw>, ExchangeError> {
        let text = match self.config["dataKind"].as_str() {
            Some("null") => return Err(self.fresh("TypeError")),
            Some("object") => {
                self.event("toString", vec![])?;
                let v = self.config["raw"].as_str().unwrap_or("");
                if v.is_empty() {
                    if tolerant { "" } else { "{}" }
                } else {
                    v
                }
            }
            Some("missing") => {
                if tolerant {
                    ""
                } else {
                    "{}"
                }
            }
            _ => {
                let Raw::S(v) = data else {
                    panic!("native raw data")
                };
                return self.parse_text(String::from_utf16(v).unwrap(), tolerant);
            }
        };
        self.parse_text(text.into(), tolerant)
    }
    fn field(
        &self,
        value: &Raw,
        key: &'static str,
        optional: bool,
    ) -> Result<Rc<Raw>, ExchangeError> {
        match value {
            Raw::Null | Raw::U => {
                if optional {
                    Ok(undefined())
                } else {
                    Err(self.fresh("TypeError"))
                }
            }
            Raw::O(fields) => {
                self.event("field", vec![text(key)])?;
                if self.config["getFail"].as_str() == Some(key) {
                    return Err(self.error());
                }
                Ok(fields
                    .iter()
                    .find(|(k, _)| k == key)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(undefined))
            }
            _ => Ok(undefined()),
        }
    }
    fn truthy(&self, value: &Raw) -> bool {
        truth(value)
    }
    fn equals(&self, value: &Raw, key: &'static str) -> bool {
        matches!(value,Raw::S(v) if *v==key.encode_utf16().collect::<Vec<_>>())
    }
    fn string(&self, value: &Raw) -> Result<Vec<u16>, ExchangeError> {
        Ok(string(value))
    }
    fn stringify(&self, value: &Raw) -> Result<Vec<u16>, ExchangeError> {
        self.event("stringify", vec![Rc::new(value.clone())])?;
        Ok(serde_json::to_string(&json(value))
            .unwrap()
            .encode_utf16()
            .collect())
    }
    fn includes(&self, value: &Raw, key: &'static str) -> Result<bool, ExchangeError> {
        match value {
            Raw::Null | Raw::U => Ok(false),
            Raw::S(v) => Ok(String::from_utf16_lossy(v).contains(key)),
            Raw::A(v) => Ok(v.iter().any(|v| self.equals(v, key))),
            _ => Err(self.fresh("TypeError")),
        }
    }
    fn stats(&self, feed: Rc<Raw>) -> Result<(), ExchangeError> {
        self.event("stats", vec![feed])
    }
    fn invoke(&self, method: &'static str, message: Rc<Raw>) -> Result<(), ExchangeError> {
        assert!(
            Rc::ptr_eq(self.parsed.borrow().as_ref().unwrap(), &message),
            "DISPATCH native original message identity"
        );
        self.event("invoke", vec![text(method)])
    }
    fn perform_auth(&self) -> Result<(), ExchangeError> {
        self.event("invoke", vec![text("performAuth")])
    }
    fn pending_challenge(&self, value: Vec<u16>) -> Result<(), ExchangeError> {
        let value = Rc::new(Raw::S(value));
        self.event("state", vec![text("pendingChallenge"), value.clone()])?;
        self.state.borrow_mut().insert("pending", value);
        Ok(())
    }
    fn waiting_challenge(&self, value: bool) -> Result<(), ExchangeError> {
        self.event("state", vec![text("waitingChallenge"), boolraw(value)])?;
        self.state.borrow_mut().insert("waiting", boolraw(value));
        Ok(())
    }
    fn authenticated(&self, value: bool) -> Result<(), ExchangeError> {
        let tag = if self.family == "auth" {
            "write"
        } else {
            "state"
        };
        self.event(tag, vec![text("authenticated"), boolraw(value)])?;
        self.state
            .borrow_mut()
            .insert("authenticated", boolraw(value));
        Ok(())
    }
    fn clear_token(&self) -> Result<(), ExchangeError> {
        self.event("state", vec![text("wsToken"), undefined()])?;
        self.state.borrow_mut().insert("token", undefined());
        Ok(())
    }
    fn log(&self, message: Vec<u16>, severity: LogSeverity) -> Result<(), ExchangeError> {
        assert_eq!(severity, LogSeverity::Error, "CONTROL native log severity");
        self.event("log", vec![Rc::new(Raw::S(message))])
    }
    fn console_subscription(
        &self,
        prefix: &'static str,
        value: Rc<Raw>,
    ) -> Result<(), ExchangeError> {
        self.event("consoleSubscription", vec![text(prefix), value])
    }
    fn console_parse(
        &self,
        prefix: &'static str,
        error: ExchangeError,
        data: Rc<Raw>,
    ) -> Result<(), ExchangeError> {
        assert!(
            Rc::ptr_eq(self.input.borrow().as_ref().unwrap(), &data),
            "DISPATCH native original raw data identity"
        );
        let own = self.original_identity(&error);
        self.event(
            "consoleParse",
            vec![
                text(prefix),
                boolraw(own),
                text(&error.name),
                if own {
                    text(&error.message)
                } else {
                    undefined()
                },
                data,
            ],
        )
    }
}
impl Host {
    fn parse_text(&self, input: String, tolerant: bool) -> Result<Rc<Raw>, ExchangeError> {
        self.event("parse", vec![text(&input), boolraw(tolerant)])?;
        let parsed = match serde_json::from_str::<Value>(&input) {
            Ok(v) => raw(&v),
            Err(_) => {
                if tolerant {
                    Rc::new(Raw::Null)
                } else {
                    return Err(self.fresh("SyntaxError"));
                }
            }
        };
        self.parsed.replace(Some(parsed.clone()));
        Ok(parsed)
    }
}
impl AuthHost for Host {
    type Signer = Raw;
    fn read(&self, slot: &'static str) -> Result<Rc<Raw>, ExchangeError> {
        self.event("read", vec![text(slot)])?;
        Ok(match slot {
            "remote" => {
                if self.flag("remote") {
                    obj(vec![("accountName", text("account-雪"))])
                } else {
                    undefined()
                }
            }
            "secret" => text("secret-base64"),
            _ => self.get(slot),
        })
    }
    fn write(&self, slot: &'static str, value: Option<Rc<Raw>>) -> Result<(), ExchangeError> {
        let value = value.unwrap_or_else(undefined);
        self.event("write", vec![text(slot), value.clone()])?;
        self.state.borrow_mut().insert(slot, value);
        Ok(())
    }
    fn signer(&self, value: Rc<Raw>) -> Result<Rc<Raw>, ExchangeError> {
        self.event("signer", vec![])?;
        Ok(value)
    }
    fn nanoid(&self) -> Result<Rc<Raw>, ExchangeError> {
        self.event("nanoid", vec![])?;
        Ok(text("request-id"))
    }
    fn sign(
        &self,
        _signer: Rc<Raw>,
        r: SignRequest<Raw>,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, ExchangeError>>, ExchangeError> {
        assert!(
            Rc::ptr_eq(&r.to_sign, &r.challenge),
            "AUTH native original challenge identity"
        );
        let request = obj(vec![
            ("id", r.id),
            ("type", text(r.kind)),
            ("exchangeName", text(r.exchange)),
            ("accountName", r.account_name),
            (
                "wsAuth",
                obj(vec![
                    ("toSign", r.to_sign),
                    ("outputFormat", text(r.output_format)),
                    ("extraData", obj(vec![("challenge", r.challenge)])),
                ]),
            ),
        ]);
        self.event("sign", vec![request])?;
        if self.flag("signSync") {
            return Err(self.error());
        }
        Ok(poll_fn(move |_| {
            if !self.released.get() {
                Poll::Pending
            } else {
                Poll::Ready(if self.flag("signReject") {
                    Err(self.error())
                } else {
                    Ok(self.config.get("result").map(raw).unwrap_or_else(|| {
                        obj(vec![("wsAuthSignature", text("remote-signature"))])
                    }))
                })
            }
        }))
    }
    fn hash_challenge(
        &self,
        challenge: impl FnOnce() -> Result<Rc<Raw>, ExchangeError>,
    ) -> Result<Rc<Raw>, ExchangeError> {
        self.event("hashFactory", vec![text("sha256")])?;
        let challenge = challenge()?;
        self.event("hash", vec![challenge, text("sha256"), text("utf8")])?;
        Ok(text("challenge-hash"))
    }
    fn decode_secret(&self, secret: Rc<Raw>) -> Result<Rc<Raw>, ExchangeError> {
        self.event("decode", vec![secret, text("base64")])?;
        Ok(text("decoded-secret"))
    }
    fn hmac_signature(&self, secret: Rc<Raw>, hash: Rc<Raw>) -> Result<Rc<Raw>, ExchangeError> {
        self.event("hmac", vec![secret, hash, text("sha512")])?;
        Ok(text("local-signature"))
    }
    fn emit_error(&self, message: Vec<u16>) -> Result<(), ExchangeError> {
        self.event("emitError", vec![Rc::new(Raw::S(message))])
    }
    fn error_text(&self, error: &ExchangeError) -> Result<Vec<u16>, ExchangeError> {
        Ok(if error.message.is_empty() {
            error.js_error_string()
        } else {
            error.message.clone()
        }
        .encode_utf16()
        .collect())
    }
    fn subscribe(&self) -> Result<(), ExchangeError> {
        self.event("subscribe", vec![])
    }
    fn send_subscription(&self, r: Subscription<Raw>) -> Result<(), ExchangeError> {
        let payload = obj(vec![
            ("event", text(r.event)),
            ("api_key", r.key),
            ("original_challenge", r.original),
            ("signed_challenge", r.signed),
            ("feed", text(r.feed)),
        ]);
        self.event("payload", vec![payload.clone()])?;
        self.event("send", vec![raw(&json(&payload))])?;
        if self.flag("mutateSend") {
            self.state.borrow_mut().insert("key", text("changed"));
        }
        let count = self
            .events
            .borrow()
            .iter()
            .filter(|e| e.name == "send")
            .count();
        if Some(count as u64) == self.config["sendFail"].as_u64() {
            Err(self.error())
        } else {
            Ok(())
        }
    }
}
impl RefreshHost for Host {
    type Value = Raw;
    type Timer = Raw;
    fn timeout(&self) -> Result<Rc<Raw>, ExchangeError> {
        self.event("timeout", vec![])?;
        Ok(self.timer.borrow().clone())
    }
    fn timer_truthy(&self, timer: &Raw) -> bool {
        truth(timer)
    }
    fn clear_timeout(&self, timer: Rc<Raw>) -> Result<(), ExchangeError> {
        self.event("clear", vec![timer])
    }
    fn schedule(&self, callback: RefreshCallback, delay: u32) -> Result<Rc<Raw>, ExchangeError> {
        assert_eq!(
            callback,
            if self.family == "futures" {
                RefreshCallback::Futures
            } else {
                RefreshCallback::Spot
            },
            "REFRESH native callback identity"
        );
        self.event(
            "schedule",
            vec![text(&self.family), Rc::new(Raw::N(delay as f64))],
        )?;
        self.callback.set(Some(callback));
        Ok(Rc::new(Raw::N(73.0)))
    }
    fn set_timeout(&self, timer: Option<Rc<Raw>>) -> Result<(), ExchangeError> {
        let timer = timer.unwrap_or_else(undefined);
        self.event("setTimeoutState", vec![timer.clone()])?;
        self.timer.replace(timer);
        Ok(())
    }
    fn disposed(&self) -> Result<bool, ExchangeError> {
        MessageHost::disposed(self)
    }
    fn fetch_orders(
        &self,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, ExchangeError>>, ExchangeError> {
        self.event("fetch", vec![])?;
        if self.flag("fetchSync") {
            return Err(self.error());
        }
        Ok(poll_fn(move |_| {
            if !self.released.get() {
                Poll::Pending
            } else {
                Poll::Ready(if self.flag("reject") {
                    Err(self.error())
                } else {
                    Ok(Rc::new(Raw::A(vec![text("order-marker")])))
                })
            }
        }))
    }
    fn current_loaded(&self) -> Result<Rc<Raw>, ExchangeError> {
        self.event("loaded", vec![])?;
        Ok(self.loaded.borrow().clone())
    }
    fn update_orders(
        &self,
        orders: Rc<Raw>,
        loaded: Rc<Raw>,
        orders_loaded: bool,
    ) -> Result<(), ExchangeError> {
        assert!(
            Rc::ptr_eq(&loaded, &self.loaded.borrow()),
            "REFRESH native current loaded identity"
        );
        let Raw::O(fields) = &*loaded else {
            panic!("loaded")
        };
        let mut fields = fields.clone();
        if let Some((_, v)) = fields.iter_mut().find(|(k, _)| k == "orders") {
            *v = boolraw(orders_loaded)
        } else {
            fields.push(("orders".into(), boolraw(orders_loaded)))
        }
        self.event(
            "update",
            vec![obj(vec![
                ("orders", orders),
                ("loaded", Rc::new(Raw::O(fields))),
            ])],
        )
    }
    fn fetch_and_set_orders(
        &self,
    ) -> Result<impl Future<Output = Result<(), ExchangeError>>, ExchangeError> {
        let result = self.event("fetchSet", vec![]).and_then(|()| {
            if self.flag("reject") {
                Err(self.error())
            } else {
                Ok(())
            }
        });
        Ok(async move { result })
    }
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let out = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/message-control-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["header"]["hashes"].as_object().unwrap().len(), 5);
        v
    })
}
fn run<F: Future>(f: F) -> F::Output {
    let mut f = pin!(f);
    let mut cx = Context::from_waker(Waker::noop());
    match f.as_mut().poll(&mut cx) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("native finite continuation remained pending"),
    }
}
#[test]
fn message_dispatch_live_source() {
    let rows = oracle()["dispatches"].as_array().unwrap();
    assert_eq!(rows.len(), 168, "DISPATCH independent 42 cases per body");
    let mut floors: HashMap<String, (usize, usize, usize)> = HashMap::new();
    for row in rows {
        let family = row["family"].as_str().unwrap();
        let config = &row["config"];
        let host = Host::new(config, family);
        host.state.borrow_mut().insert("pending", undefined());
        let data = match config["dataKind"].as_str() {
            Some("null") => Rc::new(Raw::Null),
            Some("object") => obj(vec![]),
            Some("missing") => obj(vec![("toString", undefined())]),
            _ => text(
                config["raw"]
                    .as_str()
                    .unwrap_or(&serde_json::to_string(&config["message"]).unwrap()),
            ),
        };
        host.input.replace(Some(data.clone()));
        let result = match family {
            "kraken.ws-public" => message_dispatch::futures_public(&host, data),
            "kraken.ws-private" => message_dispatch::futures_private(&host, data),
            "krakenspot.ws-public" => message_dispatch::spot_public(&host, data),
            _ => message_dispatch::spot_private(&host, data),
        };
        let floor = floors.entry(family.into()).or_default();
        floor.0 += 1;
        floor.1 += host.events.borrow().len();
        floor.2 += usize::from(result.is_err());
        host.result(result, row);
        host.compare(row, "DISPATCH");
        let state = obj(vec![
            ("authenticated", host.get("authenticated")),
            ("token", host.get("token")),
            ("waiting", host.get("waiting")),
            ("pending", host.get("pending")),
        ]);
        assert_native(&state, &row["state"], "DISPATCH state");
    }
    for (family, expected) in [
        ("kraken.ws-public", (42, 262, 1)),
        ("kraken.ws-private", (42, 222, 3)),
        ("krakenspot.ws-public", (42, 269, 1)),
        ("krakenspot.ws-private", (42, 271, 1)),
    ] {
        assert_eq!(
            floors[family], expected,
            "DISPATCH independent finite floor"
        );
    }
}
#[test]
fn perform_auth_live_source() {
    let rows = oracle()["auths"].as_array().unwrap();
    assert_eq!(rows.len(), 20, "AUTH independent cases");
    let mut events = 0;
    for row in rows {
        let host = Host::new(&row["config"], "auth");
        let mut future = pin!(private_control::perform_auth(&host));
        let mut cx = Context::from_waker(Waker::noop());
        let first = future.as_mut().poll(&mut cx);
        assert_eq!(
            host.events.borrow().len(),
            row["prefix"].as_u64().unwrap() as usize,
            "AUTH native await prefix"
        );
        if host.flag("changeAwait") {
            host.state
                .borrow_mut()
                .insert("pending", text("changed-while-awaiting"));
        }
        host.released.set(true);
        let result = match first {
            Poll::Ready(v) => v,
            Poll::Pending => run(future),
        };
        host.result(result, row);
        host.compare(row, "AUTH");
        events += host.events.borrow().len();
        let state = obj(vec![
            ("pending", host.get("pending")),
            ("original", host.get("original")),
            ("signed", host.get("signed")),
            ("authenticated", host.get("authenticated")),
        ]);
        assert_native(&state, &row["state"], "AUTH state");
    }
    assert_eq!(events, 224, "AUTH independent finite events");
}
#[test]
fn subscriptions_live_source() {
    let rows = oracle()["subs"].as_array().unwrap();
    assert_eq!(rows.len(), 8, "SUBSCRIBE independent cases");
    let mut events = 0;
    for row in rows {
        let host = Host::new(&row["config"], "subscriptions");
        host.state
            .borrow_mut()
            .insert("original", text("original-雪"));
        host.state.borrow_mut().insert("signed", undefined());
        let result = private_control::subscribe_private_feeds(&host);
        host.result(result, row);
        host.compare(row, "SUBSCRIBE");
        events += host.events.borrow().len();
    }
    assert_eq!(events, 59, "SUBSCRIBE independent finite events");
}
fn refresh_family(selected: &str) {
    let rows = oracle()["refreshes"].as_array().unwrap();
    assert_eq!(rows.len(), 24, "REFRESH independent 8 cases per body");
    let rows: Vec<_> = rows
        .iter()
        .filter(|row| row["family"] == selected)
        .collect();
    assert_eq!(rows.len(), 8, "REFRESH independent selected body cases");
    let mut floors: HashMap<String, (usize, usize, usize)> = HashMap::new();
    for row in rows {
        let family = row["family"].as_str().unwrap();
        let host = Host::new(&row["config"], family);
        let result = if family == "fetch" {
            let mut future = pin!(private_control::futures_fetch_throttled_orders(&host));
            let mut cx = Context::from_waker(Waker::noop());
            let first = future.as_mut().poll(&mut cx);
            assert_eq!(
                host.events.borrow().len(),
                row["prefix"].as_u64().unwrap() as usize,
                "REFRESH native await prefix"
            );
            host.loaded.replace(raw(
                &serde_json::json!({"markets":false,"orders":false,"custom":"current"}),
            ));
            host.released.set(true);
            match first {
                Poll::Ready(v) => v,
                Poll::Pending => run(future),
            }
        } else {
            let result = if family == "futures" {
                private_control::futures_throttled_fetch_orders(&host)
            } else {
                private_control::spot_throttled_fetch_orders(&host)
            };
            if result.is_ok()
                && family == "spot"
                && host.flag("invoke")
                && host.callback.get().is_some()
            {
                run(private_control::spot_refresh_callback(&host))
            } else {
                result
            }
        };
        let floor = floors.entry(family.into()).or_default();
        floor.0 += 1;
        floor.1 += host.events.borrow().len();
        floor.2 += usize::from(result.is_err());
        host.result(result, row);
        host.compare(row, "REFRESH");
    }
    let expected = match selected {
        "futures" => (8, 27, 3),
        "spot" => (8, 29, 3),
        "fetch" => (8, 25, 2),
        _ => panic!("selected family"),
    };
    assert_eq!(
        floors[selected], expected,
        "REFRESH independent finite floor"
    );
}
#[test]
fn futures_throttle_live_source() {
    refresh_family("futures");
}
#[test]
fn spot_throttle_live_source() {
    refresh_family("spot");
}
#[test]
fn futures_refresh_live_source() {
    refresh_family("fetch");
}
