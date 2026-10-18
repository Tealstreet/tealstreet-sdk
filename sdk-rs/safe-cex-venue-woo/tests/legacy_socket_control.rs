use safe_cex_venue_woo::{
    legacy_metadata::{JsString, Task},
    legacy_socket_control::*,
};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn u(s: &str) -> JsString {
    s.encode_utf16().collect()
}
fn text(v: &Value) -> String {
    safe_cex_num::js_string(Some(v)).unwrap()
}
fn truthy(v: &Value) -> bool {
    safe_cex_num::js_truthy(Some(v))
}
fn ready<T: 'static>(r: Result<T, String>) -> Task<T, String> {
    Box::pin(std::future::ready(r))
}
fn run<T>(mut f: Task<T, String>) -> Result<T, String> {
    let w = Waker::noop().clone();
    let mut c = Context::from_waker(&w);
    match f.as_mut().poll(&mut c) {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("SOCKET ready fixture unresolved"),
    }
}
fn wire_value(v: &Value) -> Value {
    match v {
        Value::Number(n) => {
            let f = n.as_f64().unwrap();
            if f.fract() == 0.0 && f.abs() < 9007199254740992.0 {
                json!(f as i64)
            } else {
                v.clone()
            }
        }
        Value::Array(a) => Value::Array(a.iter().map(wire_value).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), wire_value(v))).collect())
        }
        _ => v.clone(),
    }
}
fn js_json(v: &Value) -> String {
    match v {
        Value::Number(_) => text(v),
        Value::Array(a) => format!("[{}]", a.iter().map(js_json).collect::<Vec<_>>().join(",")),
        Value::Object(o) => format!(
            "{{{}}}",
            o.iter()
                .map(|(k, v)| format!("{}:{}", serde_json::to_string(k).unwrap(), js_json(v)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => serde_json::to_string(v).unwrap(),
    }
}
#[derive(Clone)]
struct Frame {
    logged: bool,
    units: JsString,
    value: Value,
}
struct Fixture {
    disposed: bool,
    testnet: bool,
    enabled: bool,
    remote: bool,
    outcome: usize,
    trace: Vec<String>,
    frames: Vec<Frame>,
    url: Option<JsString>,
    listeners: Vec<(String, Listener)>,
    requests: Vec<Value>,
    errors: Vec<Value>,
    topics: Value,
    handlers: Vec<(String, Vec<usize>)>,
    socket: Option<usize>,
    stats_error: bool,
    callback_error: bool,
    swap_key: bool,
    send_generation: usize,
    send_calls: Vec<usize>,
    pending_value: Option<Value>,
}
impl Fixture {
    fn new(_channel: Channel) -> Self {
        Self {
            disposed: false,
            testnet: false,
            enabled: false,
            remote: false,
            outcome: 0,
            trace: vec![],
            frames: vec![],
            url: None,
            listeners: vec![],
            requests: vec![],
            errors: vec![],
            topics: json!({"2":"second","1":["first","extra"],"alpha":"last"}),
            handlers: vec![
                ("ping".into(), vec![0]),
                ("pong".into(), vec![1]),
                ("ticks".into(), vec![2]),
                ("live".into(), vec![3]),
            ],
            socket: None,
            stats_error: false,
            callback_error: false,
            swap_key: false,
            send_generation: 0,
            send_calls: vec![],
            pending_value: None,
        }
    }
    fn wire(
        &self,
        family: &str,
        i: usize,
        input: Value,
        error: Option<String>,
        before: Option<Vec<String>>,
    ) -> Value {
        json!({"family":family,"i":i,"input":input,"error":error,"before":before,"trace":self.trace,"frames":self.frames.iter().map(|f|json!({"logged":f.logged,"text":f.units,"value":wire_value(&f.value)})).collect::<Vec<_>>(),"url":self.url,"listeners":self.listeners.iter().map(|(s,l)|json!([s,match l{Listener::Open=>"open",Listener::Message=>"message",Listener::Close=>"close"}])).collect::<Vec<_>>(),"requests":self.requests.iter().map(wire_value).collect::<Vec<_>>(),"errors":self.errors,"keySwaps":self.send_generation})
    }
}
impl Host for Fixture {
    type Value = Value;
    type Error = String;
    type Socket = usize;
    type Call = (Callee, usize);
    type Signer = usize;
    type Iterator = (String, usize);
    type Callback = (String, usize);
    fn prepare(&mut self, c: Callee) -> Result<Self::Call, String> {
        Ok((c, self.send_generation))
    }
    fn disposed(&mut self) -> Result<bool, String> {
        Ok(self.disposed)
    }
    fn option(&mut self, k: &str) -> Result<Value, String> {
        Ok(match k {
            "testnet" => json!(self.testnet),
            "applicationId" => json!("app"),
            "key" => {
                if self.swap_key {
                    self.send_generation += 1;
                }
                json!("key")
            }
            "secret" => json!("secret"),
            "remoteSigner" => {
                if self.remote {
                    json!({"accountName":"account"})
                } else {
                    Value::Null
                }
            }
            _ => panic!("unknown option"),
        })
    }
    fn truthy(&mut self, v: &Value) -> bool {
        truthy(v)
    }
    fn nullish(&mut self, v: &Value) -> bool {
        v.is_null()
    }
    fn is_string(&mut self, v: &Value) -> bool {
        v.is_string()
    }
    fn equals(&mut self, v: &Value, s: &str) -> bool {
        v.as_str() == Some(s)
    }
    fn text(&mut self, v: &Value) -> Result<JsString, String> {
        Ok(u(&text(v)))
    }
    fn field(&mut self, v: &Value, k: &str) -> Result<Value, String> {
        if k == "length" {
            if let Some(key) = v.get("$handlers").and_then(Value::as_str) {
                return Ok(json!(
                    self.handlers
                        .iter()
                        .find(|(s, _)| s == key)
                        .unwrap()
                        .1
                        .len()
                ));
            }
            if v.get("$function").is_some() {
                return Ok(json!(1));
            }
        }
        Ok(v.get(k).cloned().unwrap_or(Value::Null))
    }
    fn string(&self, v: JsString) -> Value {
        json!(String::from_utf16(&v).unwrap())
    }
    fn number(&self, n: f64) -> Value {
        json!(n)
    }
    fn object(&self, fields: Vec<(&'static str, Value)>) -> Value {
        Value::Object(fields.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }
    fn stringify(&mut self, v: &Value) -> Result<JsString, String> {
        self.pending_value = Some(v.clone());
        Ok(u(&js_json(v)))
    }
    fn parse(&mut self, data: Value) -> Result<Value, String> {
        Ok(serde_json::from_str(data.as_str().unwrap()).unwrap_or(Value::Null))
    }
    fn create_socket(&mut self, c: Self::Call, url: JsString) -> Result<usize, String> {
        assert_eq!(
            c.0,
            Callee::CreateSocket,
            "SOCKET native captured creation callee"
        );
        self.trace.push("create".into());
        self.url = Some(url);
        Ok(7)
    }
    fn set_socket(&mut self, s: usize) {
        self.socket = Some(s);
    }
    fn socket(&mut self) -> Result<usize, String> {
        Ok(self.socket.unwrap())
    }
    fn add_listener(&mut self, s: usize, event: &str, l: Listener) -> Result<(), String> {
        assert_eq!(s, 7, "SOCKET native assigned socket");
        self.trace.push(format!("listen:{event}"));
        self.listeners.push((event.into(), l));
        Ok(())
    }
    fn send(&mut self, c: Self::Call, units: JsString, logged: bool) -> Result<(), String> {
        assert_eq!(c.0, Callee::Send(logged), "SOCKET native send callee");
        self.send_calls.push(c.1);
        self.trace
            .push(if logged { "logged" } else { "send" }.into());
        self.frames.push(Frame {
            logged,
            value: self.pending_value.take().unwrap(),
            units,
        });
        Ok(())
    }
    fn ping(&mut self, c: Channel) -> Result<(), String> {
        self.trace.push(format!(
            "ping:{}",
            if c == Channel::Public {
                "public"
            } else {
                "private"
            }
        ));
        Ok(())
    }
    fn pong(&mut self) -> Result<(), String> {
        self.trace.push("pong".into());
        Ok(())
    }
    fn reset_orderbook(&mut self) -> Result<(), String> {
        self.trace.push("reset".into());
        Ok(())
    }
    fn subscribe(&mut self, c: Channel) -> Result<(), String> {
        self.trace.push(format!(
            "subscribe:{}",
            if c == Channel::Public {
                "public"
            } else {
                "private"
            }
        ));
        Ok(())
    }
    fn reconnecting_enabled(&mut self) -> bool {
        self.enabled
    }
    fn emit_reconnected(&mut self) -> Result<(), String> {
        self.trace.push("reconnected".into());
        Ok(())
    }
    fn topics(&mut self) -> Result<Value, String> {
        Ok(self.topics.clone())
    }
    fn object_values(&mut self, v: Value) -> Result<Vec<Value>, String> {
        let m = v.as_object().unwrap();
        let mut keys = m.keys().cloned().collect::<Vec<_>>();
        keys.sort_by_key(|k| k.parse::<u32>().unwrap_or(u32::MAX));
        Ok(keys.iter().map(|k| m[k].clone()).collect())
    }
    fn flattenable_values(&mut self, v: &Value) -> Result<Option<Vec<Value>>, String> {
        Ok(v.as_array().cloned())
    }
    fn track_last_received(&mut self) -> Result<(), String> {
        self.trace.push("track".into());
        Ok(())
    }
    fn diagnostic_receive(&mut self, _: &Value) -> Result<(), String> {
        self.trace.push("diagnostic".into());
        Ok(())
    }
    fn handlers(&mut self, k: &[u16]) -> Result<Value, String> {
        let k = String::from_utf16(k).unwrap();
        Ok(if self.handlers.iter().any(|(s, _)| s == &k) {
            json!({"$handlers":k})
        } else if k == "hasOwnProperty" {
            json!({"$function":k})
        } else {
            Value::Null
        })
    }
    fn stats(&mut self, key: JsString) -> Result<(), String> {
        self.trace
            .push(format!("stats:{}", String::from_utf16(&key).unwrap()));
        if self.stats_error {
            return Err("stats failed".into());
        }
        Ok(())
    }
    fn iterator(&mut self, h: Value) -> Result<Self::Iterator, String> {
        h.get("$handlers")
            .and_then(Value::as_str)
            .map(|k| (k.into(), 0))
            .ok_or("handlers is not iterable".into())
    }
    fn next(&mut self, it: &mut Self::Iterator) -> Result<Option<Self::Callback>, String> {
        let list = &self.handlers.iter().find(|(s, _)| s == &it.0).unwrap().1;
        let callback = list.get(it.1).copied().map(|id| (it.0.clone(), id));
        it.1 += 1;
        Ok(callback)
    }
    fn callback(&mut self, c: Self::Callback, _: &Value) -> Result<(), String> {
        let name = match c.1 {
            0 => "ping",
            1 => "pong",
            2 => "ticks",
            3 => "live0",
            4 => "live1",
            _ => panic!("callback id"),
        };
        self.trace.push(format!("callback:{name}"));
        if self.callback_error {
            return Err("callback failed".into());
        }
        if c.1 == 3 {
            self.handlers
                .iter_mut()
                .find(|(s, _)| s == "live")
                .unwrap()
                .1
                .push(4);
        }
        Ok(())
    }
    fn execution_report(&mut self, c: Self::Call, _: Value, algo: bool) -> Result<(), String> {
        assert_eq!(c.0, Callee::Execution(algo));
        self.trace
            .push(if algo { "algo" } else { "execution" }.into());
        Ok(())
    }
    fn emit_error(&mut self, c: Self::Call, error: Value) -> Result<(), String> {
        assert_eq!(c.0, Callee::EmitError);
        self.trace.push("error".into());
        self.errors.push(error);
        Ok(())
    }
    fn timestamp(&mut self) -> Result<f64, String> {
        self.trace.push("clock".into());
        Ok(1700000000123.0)
    }
    fn nanoid(&mut self) -> Result<JsString, String> {
        self.trace.push("nanoid".into());
        Ok(u("request-id"))
    }
    fn prepare_signer(&mut self, _: Value) -> Result<usize, String> {
        Ok(9)
    }
    fn sign(&mut self, s: usize, request: Value) -> Result<Task<Value, String>, String> {
        assert_eq!(s, 9);
        self.trace.push("sign".into());
        self.requests.push(request);
        Ok(ready(if self.outcome == 5 {
            Err("sign rejected".into())
        } else if self.outcome == 4 {
            Ok(json!({"error":"offline"}))
        } else {
            Ok(json!({"wsAuthSignature":"remote-signature"}))
        }))
    }
    fn hmac_sha256_hex(&mut self, secret: Value, to_sign: JsString) -> Result<JsString, String> {
        self.trace.push("hmac".into());
        assert_eq!(secret, "secret", "SOCKET native HMAC secret");
        assert_eq!(to_sign, u("|1700000000123"), "SOCKET native HMAC input");
        Ok(u(&safe_cex_crypto::hmac_sha256(
            b"secret",
            String::from_utf16(&to_sign).unwrap().as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        )))
    }
    fn private_snapshot(&mut self) -> Result<(), String> {
        self.trace.push("snapshot".into());
        Ok(())
    }
}
const PUB: [&str; 13] = [
    "bad",
    "null",
    "false",
    r#"{"event":"subscribe","topic":"ticks"}"#,
    r#"{"event":"ping","topic":"ticks"}"#,
    r#"{"event":"pong"}"#,
    r#"{"topic":"ticks"}"#,
    r#"{"event":2,"topic":"ticks"}"#,
    r#"{"topic":"missing"}"#,
    r#"{"topic":"live"}"#,
    r#"{"event":"ping"}"#,
    r#"{"topic":"hasOwnProperty"}"#,
    r#"{"topic":"toString"}"#,
];
const PRIV: [&str; 11] = [
    "bad",
    "null",
    r#"{"event":"ping","topic":"executionreport"}"#,
    r#"{"event":"pong"}"#,
    r#"{"event":"auth","success":1}"#,
    r#"{"event":"auth","success":false,"message":" "}"#,
    r#"{"event":"auth","error":{"code":7}}"#,
    r#"{"event":"auth"}"#,
    r#"{"topic":"executionreport","data":{"n":1}}"#,
    r#"{"topic":"algoexecutionreportv2","data":[{"n":2}]}"#,
    r#"{"event":"ping"}"#,
];
fn source() -> Vec<Value> {
    let o = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/legacy_socket_control_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        o.status.success(),
        "SOCKET source oracle {}",
        String::from_utf8_lossy(&o.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&o.stdout).unwrap();
    assert_eq!(rows.len(), 48, "SOCKET corpus SIZE");
    let mut n = 0;
    for (family, count) in [
        ("connect", 8),
        ("publicOpen", 4),
        ("privateOpen", 8),
        ("subscribe", 4),
        ("publicMessage", 13),
        ("privateMessage", 11),
    ] {
        for i in 0..count {
            assert_eq!(
                rows[n]["family"], family,
                "SOCKET corpus CONTENT family {n}"
            );
            assert_eq!(rows[n]["i"], i, "SOCKET corpus CONTENT iteration {n}");
            let expected = match family {
                "connect" => {
                    json!({"channel":if i<4{"public"}else{"private"},"disposed":i%4>=2,"testnet":i%2==1,"app":"app"})
                }
                "publicOpen" => json!({"disposed":i>=2,"enabled":i%2==1}),
                "privateOpen" => {
                    json!({"disposed":i==6,"enabled":i%2==1,"remote":(2..=5).contains(&i)||i==7,"keySwap":i==7,"outcome":if i==4{"offline"}else if i==5{"rejected"}else{"ok"}})
                }
                "subscribe" => {
                    json!({"channel":if i<2{"public"}else{"private"},"disposed":i%2==1,"topics":{"2":"second","1":["first","extra"],"alpha":"last"}})
                }
                "publicMessage" => json!({"data":PUB[i],"disposed":i==10}),
                _ => json!({"data":PRIV[i],"disposed":i==10}),
            };
            assert_eq!(
                rows[n]["input"], expected,
                "SOCKET corpus CONTENT input {n}"
            );
            n += 1;
        }
    }
    rows
}
fn frames(h: &Fixture, events: &[(&str, Option<&str>, bool)]) {
    assert_eq!(h.frames.len(), events.len(), "SOCKET native frame count");
    for (i, (event, topic, logged)) in events.iter().enumerate() {
        let f = &h.frames[i];
        assert_eq!(f.logged, *logged, "SOCKET native logged flag {i}");
        assert_eq!(f.value["event"], *event, "SOCKET native event {i}");
        if let Some(t) = topic {
            assert_eq!(f.value["topic"], *t, "SOCKET native topic {i}");
        }
    }
}
#[test]
fn native_connection_urls_assignment_and_listener_order() {
    let s = source();
    for i in 0..8 {
        let c = if i < 4 {
            Channel::Public
        } else {
            Channel::Private
        };
        let mut h = Fixture::new(c);
        h.disposed = i % 4 >= 2;
        h.testnet = i % 2 == 1;
        let result = if c == Channel::Public {
            public_connect_and_subscribe(&mut h)
        } else {
            private_connect_and_subscribe(&mut h)
        };
        result.unwrap();
        if h.disposed {
            assert!(h.url.is_none(), "SOCKET native disposed connection");
            assert!(h.listeners.is_empty());
        } else {
            let base = match (c, h.testnet) {
                (Channel::Public, false) => "wss://wss.woox.io/ws/stream/",
                (Channel::Public, true) => "wss://wss.staging.woox.io/ws/stream/",
                (Channel::Private, false) => "wss://wss.woox.io/v2/ws/private/stream/",
                (Channel::Private, true) => "wss://wss.staging.woox.io/v2/ws/private/stream/",
            };
            assert_eq!(
                h.url,
                Some(u(&(base.to_owned() + "app"))),
                "SOCKET native connection URL {i}"
            );
            assert_eq!(h.socket, Some(7));
            assert_eq!(
                h.listeners,
                vec![
                    ("open".into(), Listener::Open),
                    ("message".into(), Listener::Message),
                    ("close".into(), Listener::Close)
                ],
                "SOCKET native listener identity/order {i}"
            );
        }
        assert_eq!(
            s[i],
            h.wire("connect", i, s[i]["input"].clone(), None, None)
        );
    }
}
#[test]
fn native_public_open_frames() {
    let s = source();
    for i in 0..4 {
        let mut h = Fixture::new(Channel::Public);
        h.disposed = i >= 2;
        h.enabled = i % 2 == 1;
        public_on_open(&mut h).unwrap();
        if h.disposed {
            assert!(h.trace.is_empty(), "SOCKET native disposed public open");
            frames(&h, &[]);
        } else {
            frames(
                &h,
                &[
                    ("subscribe", Some("tickers"), false),
                    ("subscribe", Some("bbos"), false),
                    ("subscribe", Some("markprices"), false),
                ],
            );
            let mut expected = vec![
                "ping:public",
                "send",
                "send",
                "send",
                "reset",
                "subscribe:public",
            ];
            if h.enabled {
                expected.push("reconnected");
            }
            assert_eq!(h.trace, expected, "SOCKET native public open sequence");
        }
        assert_eq!(
            s[8 + i],
            h.wire("publicOpen", i, s[8 + i]["input"].clone(), None, None)
        );
    }
}
fn subscription_case(s: &[Value], i: usize) {
    let mut h = Fixture::new(if i < 2 {
        Channel::Public
    } else {
        Channel::Private
    });
    h.disposed = i % 2 == 1;
    if i < 2 {
        public_subscribe(&mut h).unwrap();
        frames(
            &h,
            &[
                ("subscribe", Some("first"), false),
                ("subscribe", Some("extra"), false),
                ("subscribe", Some("second"), false),
                ("subscribe", Some("last"), false),
            ],
        );
    } else {
        private_subscribe(&mut h).unwrap();
        if h.disposed {
            frames(&h, &[]);
            assert!(h.trace.is_empty());
        } else {
            frames(
                &h,
                &[
                    ("subscribe", Some("executionreport"), true),
                    ("subscribe", Some("algoexecutionreportv2"), true),
                ],
            );
            assert_eq!(
                h.trace,
                vec!["logged", "logged", "ping:private"],
                "SOCKET native private subscribe sequence"
            );
        }
    }
    assert_eq!(
        s[20 + i],
        h.wire("subscribe", i, s[20 + i]["input"].clone(), None, None)
    );
}
#[test]
fn native_public_subscription_frames() {
    let s = source();
    for i in 0..2 {
        subscription_case(&s, i);
    }
}
#[test]
fn native_private_subscription_frames() {
    let s = source();
    for i in 2..4 {
        subscription_case(&s, i);
    }
}

#[test]
fn native_private_auth_local_remote_failures_and_await() {
    let s = source();
    for i in 0..8 {
        let h = Rc::new(RefCell::new(Fixture::new(Channel::Private)));
        {
            let mut h = h.borrow_mut();
            h.disposed = i == 6;
            h.enabled = i % 2 == 1;
            h.remote = (2..=5).contains(&i) || i == 7;
            h.swap_key = i == 7;
            h.outcome = i;
        }
        let f = private_on_open(h.clone());
        let before = h.borrow().trace.clone();
        let result = run(f);
        let error = result.err();
        let h = h.borrow();
        if i == 5 {
            assert_eq!(
                error.as_deref(),
                Some("sign rejected"),
                "SOCKET native signing rejection"
            );
        } else {
            assert_eq!(error, None);
        }
        if i < 4 || i == 7 {
            frames(&h, &[("auth", None, true)]);
            let params = &h.frames[0].value["params"];
            assert_eq!(params["apikey"], "key", "SOCKET native auth key");
            assert_eq!(
                params["timestamp"].as_f64().unwrap().to_bits(),
                1700000000123f64.to_bits(),
                "SOCKET native auth timestamp"
            );
            assert_eq!(
                params["sign"],
                if i < 2 {
                    "6422eeee89532d3a370e89c44df54c01129b1ae916d553f90274ad609de62a04"
                } else {
                    "remote-signature"
                },
                "SOCKET native auth signature"
            );
            if i < 2 {
                assert_eq!(before, h.trace, "SOCKET native local auth eager effects");
            } else {
                assert_eq!(
                    before,
                    vec!["clock", "nanoid", "sign"],
                    "SOCKET native remote auth prefix"
                );
                assert_eq!(
                    h.requests[0],
                    json!({"id":"request-id","type":"ws-auth","exchangeName":"woo","accountName":"account","wsAuth":{"toSign":"|1700000000123","outputFormat":"hex","extraData":{"timestamp":1700000000123.0}}}),
                    "SOCKET native signer request"
                );
            }
        } else {
            frames(&h, &[]);
        }
        if i == 4 {
            assert_eq!(
                h.errors,
                vec![json!("Remote WS signing unavailable: offline")],
                "SOCKET native remote signer availability message"
            );
        }
        assert_eq!(
            h.trace.iter().filter(|v| *v == "snapshot").count(),
            usize::from((i < 4 || i == 7) && i % 2 == 1),
            "SOCKET native reconnect snapshot condition"
        );
        if i == 7 {
            assert_eq!(h.send_generation, 1);
            assert_eq!(
                h.send_calls,
                vec![0],
                "SOCKET native captured send survives key getter replacement"
            );
        }
        assert_eq!(
            s[12 + i],
            h.wire(
                "privateOpen",
                i,
                s[12 + i]["input"].clone(),
                error,
                Some(before)
            )
        );
    }
}
#[test]
fn native_public_control_precedence_and_live_iteration() {
    let s = source();
    let expected: [&[&str]; 13] = [
        &["track"],
        &["track"],
        &["track"],
        &["track"],
        &["track", "stats:ping", "callback:ping"],
        &["track", "stats:pong", "callback:pong"],
        &["track", "stats:ticks", "callback:ticks"],
        &["track", "stats:ticks", "callback:ticks"],
        &["track"],
        &["track", "stats:live", "callback:live0", "callback:live1"],
        &[],
        &["track", "stats:hasOwnProperty"],
        &["track"],
    ];
    for (i, data) in PUB.into_iter().enumerate() {
        let mut h = Fixture::new(Channel::Public);
        h.disposed = i == 10;
        let error = public_on_message(&mut h, json!(data)).err();
        assert_eq!(
            error.as_deref(),
            if i == 11 {
                Some("handlers is not iterable")
            } else {
                None
            },
            "SOCKET native inherited iteration error {i}"
        );
        assert_eq!(
            h.trace, expected[i],
            "SOCKET native public message effects {i}"
        );
        assert_eq!(
            s[24 + i],
            h.wire("publicMessage", i, s[24 + i]["input"].clone(), error, None)
        );
    }
    let mut h = Fixture::new(Channel::Public);
    h.stats_error = true;
    let e = public_on_message(&mut h, json!(PUB[6])).unwrap_err();
    assert_eq!(e, "stats failed");
    assert_eq!(
        h.trace,
        vec!["track", "stats:ticks"],
        "SOCKET native stats aborts callbacks"
    );
}
#[test]
fn native_private_message_routes_error_values_and_diagnostics() {
    let s = source();
    let expected: [&[&str]; 11] = [
        &["track", "diagnostic"],
        &["track", "diagnostic"],
        &["track", "diagnostic", "logged"],
        &["track", "diagnostic", "pong"],
        &["track", "diagnostic", "subscribe:private"],
        &["track", "diagnostic", "error"],
        &["track", "diagnostic", "error"],
        &["track", "diagnostic", "error"],
        &["track", "diagnostic", "stats:order", "execution"],
        &["track", "diagnostic", "stats:orderAlgo", "algo"],
        &[],
    ];
    for (i, data) in PRIV.into_iter().enumerate() {
        let mut h = Fixture::new(Channel::Private);
        h.disposed = i == 10;
        private_on_message(&mut h, json!(data)).unwrap();
        assert_eq!(
            h.trace, expected[i],
            "SOCKET native private message effects {i}"
        );
        if i == 2 {
            frames(&h, &[("pong", None, true)]);
        }
        if (5..=7).contains(&i) {
            assert_eq!(
                h.errors[0],
                match i {
                    5 => json!(" "),
                    6 => json!({"code":7}),
                    _ => json!("WOO X private websocket authentication failed"),
                },
                "SOCKET native auth failure value {i}"
            );
        }
        assert_eq!(
            s[37 + i],
            h.wire("privateMessage", i, s[37 + i]["input"].clone(), None, None)
        );
    }
}
