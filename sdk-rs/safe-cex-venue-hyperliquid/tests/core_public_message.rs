#[allow(dead_code)]
#[path = "support/cache_host.rs"]
mod cache;
use cache::{N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::{
    core_public_message::*,
    json_boundaries::JsPayloadHost,
    json_values::JsonValue,
    spot_json::JsString,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use serde_json::{Value, json};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    process::Command,
};
fn get(v: &N, k: &str) -> N {
    JsPayloadHost::get(&mut cache::Host::default(), v, k).unwrap()
}
fn put(v: &N, k: &str, value: &N) {
    cache::Host::default().put(v, k, value, true).unwrap()
}
fn same(a: &N, b: &N) -> bool {
    eq(a, b) || matches!((a,b),(N::Builtin(a),N::Builtin(b))if a==b)
}
fn from_json(v: JsonValue) -> N {
    match v {
        JsonValue::Null => N::Null,
        JsonValue::Bool(v) => N::B(v),
        JsonValue::Number(v) => N::Num(v),
        JsonValue::String(v) => N::S(v),
        JsonValue::Array(v) => arr(v.into_iter().map(from_json).collect()),
        JsonValue::Object(v) => {
            let out = object(vec![]);
            for (k, v) in v.0 {
                let key = String::from_utf16(&k.0)
                    .expect("FRAME fixture object keys scalar; values remain UTF16");
                put(&out, &key, &from_json(v))
            }
            out
        }
    }
}
fn own_keys(v: &N) -> Vec<String> {
    let N::O(o) = v else {
        panic!("FRAME native own object");
    };
    o.borrow().fields.iter().map(|p| p.0.clone()).collect()
}
fn encode(v: &N) -> Value {
    match v {
        N::U => json!(["undefined"]),
        N::Null => json!(["null"]),
        N::B(b) => json!(["bool", b]),
        N::Num(n) => json!(["number", format!("{:016x}", n.to_bits())]),
        N::S(s) => json!(["string", s.0]),
        N::A(a) => json!(["array", a.borrow().iter().map(encode).collect::<Vec<_>>()]),
        N::O(o) => json!([
            "object",
            o.borrow()
                .fields
                .iter()
                .filter(|p| p.2)
                .map(|(k, v, _)| json!([k.encode_utf16().collect::<Vec<_>>(), encode(v)]))
                .collect::<Vec<_>>()
        ]),
        _ => panic!("FRAME projection outside native value domain"),
    }
}
fn quote(v: &[u16]) -> String {
    let mut out = String::from("\"");
    for c in char::decode_utf16(v.iter().copied()) {
        match c {
            Ok(c) => {
                let text = serde_json::to_string(&c.to_string()).unwrap();
                out.push_str(&text[1..text.len() - 1])
            }
            Err(e) => out.push_str(&format!("\\u{:04x}", e.unpaired_surrogate())),
        }
    }
    out.push('"');
    out
}
struct Host {
    name: String,
    raw: N,
    parsed: N,
    socket: N,
    event: N,
    parent: N,
    store: N,
    registry: N,
    handlers: N,
    topic: JsString,
    marker: N,
    trace: Vec<String>,
    payloads: Vec<N>,
    logs: Vec<(N, N)>,
    latency: Option<N>,
    channel_reads: usize,
}
impl Host {
    fn new(name: &str, wire: &str) -> Self {
        let parsed = serde_json::from_str::<JsonValue>(wire)
            .map(from_json)
            .unwrap_or(N::Null);
        let raw = s(wire);
        let parent = object(vec![("log", N::Builtin("log".into()))]);
        let store = object(vec![("update", N::Builtin("update".into()))]);
        let handlers = arr(vec![
            N::Builtin("first".into()),
            N::Builtin("second".into()),
        ]);
        let registry = object(vec![]);
        let prefix = if name.starts_with("candle") {
            "candle."
        } else if name.starts_with("book") {
            "l2Book."
        } else {
            "bbo."
        };
        let mut topic = prefix.encode_utf16().collect::<Vec<_>>();
        topic.extend(if name.ends_with("utf16") {
            vec![0xd800]
        } else {
            "BTC".encode_utf16().collect()
        });
        if name.starts_with("candle") {
            topic.extend(".1m".encode_utf16())
        }
        let ping = match name {
            "pong-no-ping" => N::U,
            "pong-negative-zero" => N::Num(-0.),
            "pong-nan" => N::Num(f64::NAN),
            "pong-infinity" => N::Num(f64::INFINITY),
            "pong-round-negative-zero" => N::Num(2.),
            _ => N::Num(1.),
        };
        let socket = object(vec![
            ("isDisposed", N::B(name == "disposed")),
            ("parent", parent.clone()),
            ("store", store.clone()),
            ("messageHandlers", registry.clone()),
            ("pingAt", ping),
        ]);
        for method in [
            "trackLastReceived",
            "emitReceiveDiagnostics",
            "recordMessageHandlerStats",
            "handleAllMids",
            "handleFastAssetCtxs",
        ] {
            put(&socket, method, &N::Builtin(method.into()))
        }
        let event = if name == "event-undefined" {
            N::U
        } else if name == "event-null" {
            N::Null
        } else {
            object(vec![("data", raw.clone())])
        };
        Self {
            name: name.into(),
            raw,
            parsed,
            socket,
            event,
            parent,
            store,
            registry,
            handlers,
            topic: JsString(topic),
            marker: object(vec![("original", N::B(true))]),
            trace: vec![],
            payloads: vec![],
            logs: vec![],
            latency: None,
            channel_reads: 0,
        }
    }
    fn stringify(&mut self, v: &N) -> Result<String, N> {
        Ok(match v {
            N::Null | N::U => "null".into(),
            N::B(b) => b.to_string(),
            N::Num(n) => {
                if n.is_finite() {
                    safe_cex_num::js_number_string(*n)
                } else {
                    "null".into()
                }
            }
            N::S(v) => quote(&v.0),
            N::A(a) => {
                let members = a.borrow().clone();
                let mut text = vec![];
                for v in members {
                    text.push(self.stringify(&v)?)
                }
                format!("[{}]", text.join(","))
            }
            N::O(o) => {
                let keys = o
                    .borrow()
                    .fields
                    .iter()
                    .filter(|p| p.2)
                    .map(|p| p.0.clone())
                    .collect::<Vec<_>>();
                let mut text = vec![];
                for k in keys {
                    let v = self.get(v, &k)?;
                    if !matches!(v, N::U) {
                        text.push(format!(
                            "{}:{}",
                            quote(&k.encode_utf16().collect::<Vec<_>>()),
                            self.stringify(&v)?
                        ))
                    }
                }
                format!("{{{}}}", text.join(","))
            }
            _ => panic!("FRAME JSON fixture intrinsic"),
        })
    }
    fn assert_payload(&self, value: &N) {
        let data = get(&self.parsed, "data");
        assert!(eq(value, &data), "FRAME native original payload allocation");
        if self.name.starts_with("candle") {
            let sample = if let N::A(a) = value {
                a.borrow()[0].clone()
            } else {
                value.clone()
            };
            let expected = if self.name.ends_with("utf16") {
                N::S(JsString(vec![0xd800]))
            } else {
                s("BTC")
            };
            assert!(
                same(&get(&sample, "s"), &expected),
                "FRAME native candle symbol UTF16"
            );
            assert!(same(&get(&sample, "i"), &s("1m")));
            assert!(same(&get(&sample, "o"), &s("2")));
        } else {
            let expected = if self.name.ends_with("utf16") {
                N::S(JsString(vec![0xd800]))
            } else {
                s("BTC")
            };
            assert!(
                same(&get(value, "coin"), &expected),
                "FRAME native book coin UTF16"
            );
            assert!(
                matches!(get(value,"sentinel"),N::Num(n)if n.to_bits()==(-0f64).to_bits()),
                "FRAME native admitted signed zero"
            );
            assert!(
                matches!(get(value,"overflow"),N::Num(n)if n.to_bits()==f64::INFINITY.to_bits()),
                "FRAME native admitted overflow"
            );
            let N::A(levels) = get(value, "levels") else {
                panic!("FRAME native levels")
            };
            let N::A(bids) = &levels.borrow()[0] else {
                panic!("FRAME native bids")
            };
            let N::A(first) = &bids.borrow()[0] else {
                panic!("FRAME native level")
            };
            assert!(
                matches!(&first.borrow()[0],N::Num(n)if n.to_bits()==1f64.to_bits()),
                "FRAME native first price"
            );
            assert!(
                matches!(&first.borrow()[1],N::Num(n)if n.to_bits()==2f64.to_bits()),
                "FRAME native first size"
            );
        }
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, v: &JsString) -> N {
        N::S(v.clone())
    }
    fn truthy(&self, v: &N) -> bool {
        !matches!(v, N::U | N::Null | N::B(false))
            && !matches!(v,N::Num(n)if *n==0.||n.is_nan())
            && !matches!(v,N::S(v)if v.0.is_empty())
    }
    fn global(&mut self, name: &str) -> Result<N, N> {
        Ok(object(match name {
            "performance" => vec![("now", N::Builtin("now".into()))],
            "Math" => vec![("round", N::Builtin("round".into()))],
            "JSON" => vec![("stringify", N::Builtin("stringify".into()))],
            "LogSeverity" => vec![("Debug", s("debug")), ("Error", s("error"))],
            _ => panic!("FRAME global {name}"),
        }))
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if eq(v, &self.socket) && k == "isDisposed" && self.name == "disposed-getter-error" {
            self.trace.push("disposed-throw".into());
            return Err(self.marker.clone());
        }
        if eq(v, &self.handlers) && k == "length" && self.name == "book-length-error" {
            self.trace.push("length-throw".into());
            return Err(self.marker.clone());
        }
        if eq(v, &self.parsed) && k == "channel" && self.name == "channel-reentry" {
            self.channel_reads += 1;
            return Ok(s(if self.channel_reads == 1 {
                "subscriptionResponse"
            } else {
                "allMids"
            }));
        }
        // These JSON arrays have no custom named properties. The shared cache
        // fixture supplies virtual array methods for arbitrary names; frame
        // routing instead observes an ordinary missing named property.
        if matches!(v, N::A(_)) && k != "length" && k.parse::<usize>().is_err() {
            return Ok(N::U);
        }
        Ok(get(v, k))
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        put(v, k, value);
        Ok(())
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn define_data(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        put(v, k, value);
        Ok(())
    }
    fn callback(&mut self, _: PingCallback<N>) -> Result<N, N> {
        panic!("FRAME no timer")
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("FRAME native callee {f:?}")
        };
        match id.as_str() {
            "trackLastReceived" => {
                assert!(eq(r, &self.socket));
                assert!(args.is_empty());
                self.trace.push("track".into());
                if self.name == "track-error" {
                    return Err(self.marker.clone());
                }
            }
            "emitReceiveDiagnostics" => {
                assert!(eq(r, &self.socket));
                assert_eq!(args.len(), 2);
                assert!(same(&args[0], &self.raw), "FRAME native original raw input");
                assert!(
                    same(&args[1], &self.parsed),
                    "FRAME native original parsed input"
                );
                self.trace.push("diagnostic".into());
                if self.name == "diagnostic-error" {
                    return Err(self.marker.clone());
                }
            }
            "recordMessageHandlerStats" => {
                assert!(eq(r, &self.socket));
                assert_eq!(args.len(), 1);
                if self.name.starts_with("bbo") && !same(&args[0], &s("pong")) {
                    assert!(
                        matches!(&args[0], N::S(key) if key == &self.topic),
                        "FRAME native exact BBO statistic key"
                    );
                }
                let key = match &args[0] {
                    N::U => "undefined".into(),
                    N::S(v) => String::from_utf16(&v.0).unwrap_or_else(|_| "bbo.<D800>".into()),
                    _ => panic!("FRAME stats key"),
                };
                self.trace.push(format!("stats:{key}"))
            }
            "handleAllMids" => {
                assert!(eq(r, &self.socket));
                assert_eq!(args.len(), 1);
                let mids = get(&get(&self.parsed, "data"), "mids");
                if self.name == "mids" || self.name == "channel-reentry" {
                    assert!(eq(&args[0], &mids), "FRAME native original mids allocation");
                    assert!(same(&get(&args[0], "BTC"), &s("1")));
                    assert!(same(&get(&args[0], "ETH"), &s("2")));
                } else {
                    let N::O(o) = &args[0] else {
                        panic!("FRAME fresh mids object")
                    };
                    assert!(o.borrow().fields.is_empty());
                    assert!(!same(&args[0], &mids), "FRAME native fresh fallback object")
                }
                self.trace.push("mids".into());
                self.payloads.push(args[0].clone())
            }
            "handleFastAssetCtxs" => {
                assert!(eq(r, &self.socket));
                assert_eq!(args.len(), 1);
                let data = get(&self.parsed, "data");
                assert!(same(&args[0], &data), "FRAME native original fast payload");
                if self.name == "fast" {
                    assert!(
                        matches!(get(&args[0],"time"),N::Num(n)if n.to_bits()==(-0f64).to_bits()),
                        "FRAME native fast signed zero"
                    );
                    let N::A(values) = get(&args[0], "ctxs") else {
                        panic!("FRAME native fast ctxs")
                    };
                    assert!(matches!(&values.borrow()[0],N::Num(n)if n.to_bits()==1f64.to_bits()));
                    assert!(matches!(&values.borrow()[1],N::Num(n)if n.to_bits()==2f64.to_bits()));
                }
                self.trace.push("fast".into());
                self.payloads.push(args[0].clone())
            }
            "first" | "second" | "third" => {
                assert!(eq(r, &self.handlers), "FRAME native indexed-call receiver");
                assert_eq!(args.len(), 1);
                self.assert_payload(&args[0]);
                self.trace.push(id.clone());
                self.payloads.push(args[0].clone());
                if id == "first" {
                    if self.name.ends_with("extend") {
                        let N::A(a) = &self.handlers else {
                            unreachable!()
                        };
                        a.borrow_mut().push(N::Builtin("third".into()));
                    }
                    if self.name == "candle-mutate" {
                        assert!(matches!(get(&args[0], "marker"), N::U));
                        put(&args[0], "marker", &s("changed"));
                    }
                    if self.name.ends_with("first-error") {
                        return Err(self.marker.clone());
                    }
                } else if self.name == "candle-mutate" {
                    assert!(
                        same(&get(&args[0], "marker"), &s("changed")),
                        "FRAME native second sees original payload mutation"
                    )
                }
            }
            "now" => {
                assert!(args.is_empty());
                self.trace.push("clock".into());
                if self.name == "pong-clock-error" {
                    return Err(self.marker.clone());
                }
                return Ok(N::Num(if self.name == "pong-round-negative-zero" {
                    1.
                } else {
                    4.
                }));
            }
            "round" => {
                assert_eq!(args.len(), 1);
                let N::Num(n) = args[0] else {
                    panic!("FRAME native round argument")
                };
                return Ok(N::Num(safe_cex_num::math_round(n)));
            }
            "update" => {
                assert!(eq(r, &self.store));
                assert_eq!(args.len(), 1);
                assert_eq!(
                    own_keys(&args[0]),
                    ["latency"],
                    "FRAME native exact Store update keys"
                );
                let value = get(&args[0], "latency");
                let N::Num(n) = value else {
                    panic!("FRAME native latency")
                };
                let expected = if self.name == "pong-round-negative-zero" {
                    -0.
                } else if self.name == "pong-infinity" {
                    f64::NEG_INFINITY
                } else {
                    2.
                };
                assert_eq!(
                    n.to_bits(),
                    expected.to_bits(),
                    "FRAME native latency bits before wire"
                );
                self.latency = Some(N::Num(n));
                self.trace.push("latency".into())
            }
            "stringify" => {
                assert_eq!(args.len(), 1);
                return Ok(s(&self.stringify(&args[0])?));
            }
            "log" => {
                assert!(eq(r, &self.parent));
                assert_eq!(args.len(), 2);
                let level = if self.name.starts_with("error") {
                    "error"
                } else {
                    "debug"
                };
                assert!(same(&args[1], &s(level)), "FRAME native exact log severity");
                let N::S(message) = &args[0] else {
                    panic!("FRAME native log text")
                };
                let prefix = if level == "error" {
                    "WebSocket error: "
                } else {
                    "Subscription response: "
                };
                assert!(
                    message
                        .0
                        .starts_with(&prefix.encode_utf16().collect::<Vec<_>>()),
                    "FRAME native log prefix"
                );
                let expected_wire = if self.name == "channel-reentry" {
                    r#"{"channel":"allMids","data":{"mids":{"BTC":"1","ETH":"2"}}}"#.to_string()
                } else {
                    let N::S(raw) = &self.raw else { unreachable!() };
                    String::from_utf16(&raw.0).expect("fixture diagnostic wire is scalar")
                };
                assert_eq!(
                    message.0,
                    format!("{prefix}{expected_wire}")
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                    "FRAME native exact log text"
                );
                self.logs.push((args[0].clone(), args[1].clone()));
                self.trace.push(format!("log:{level}"));
                if self.name.ends_with("log-error") {
                    return Err(self.marker.clone());
                }
            }
            _ => panic!("FRAME unsupported callee {id}"),
        };
        Ok(N::U)
    }
}
impl CorePublicMessageHost for Host {
    fn event_data(&mut self, event: &N) -> Result<N, N> {
        if matches!(event, N::U | N::Null) {
            return Err(object(vec![
                ("name", s("TypeError")),
                (
                    "message",
                    s(if matches!(event, N::U) {
                        "Cannot destructure property 'data' of 'undefined' as it is undefined."
                    } else {
                        "Cannot destructure property 'data' of 'object null' as it is null."
                    }),
                ),
            ]));
        }
        self.trace.push("event-data".into());
        if self.name == "event-getter-error" {
            return Err(self.marker.clone());
        }
        self.get(event, "data")
    }
    fn is_nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn is_array(&mut self, v: &N) -> Result<bool, N> {
        Ok(matches!(v, N::A(_)))
    }
    fn equal(&self, a: &N, b: &N) -> bool {
        same(a, b)
    }
    fn number(&self, n: f64) -> N {
        N::Num(n)
    }
    fn less(&mut self, a: &N, b: &N) -> Result<bool, N> {
        Ok(cache::num(a) < cache::num(b))
    }
    fn subtract(&mut self, a: &N, b: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(a) - cache::num(b)))
    }
    fn divide(&mut self, a: &N, b: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(a) / cache::num(b)))
    }
    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        if eq(v, &self.registry) {
            let N::S(key) = k else {
                panic!("FRAME native UTF16 key")
            };
            assert_eq!(key, &self.topic, "FRAME native exact handler key");
            return Ok(if self.name.ends_with("absent") {
                N::U
            } else {
                self.handlers.clone()
            });
        }
        if eq(v, &self.handlers)
            && matches!(k,N::Num(n)if *n==0.)
            && self.name == "book-index-error"
        {
            self.trace.push("index-throw".into());
            return Err(self.marker.clone());
        }
        self.get(v, &safe_cex_num::js_number_string(cache::num(k)))
    }
    fn array(&mut self) -> Result<N, N> {
        Ok(arr(vec![]))
    }
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        Ok(match v {
            N::S(v) => v.clone(),
            N::U => JsString("undefined".encode_utf16().collect()),
            N::Num(n) => JsString(safe_cex_num::js_number_string(*n).encode_utf16().collect()),
            _ => panic!("FRAME fixture template value"),
        })
    }
    fn json_parse(&mut self, raw: &N) -> Result<N, N> {
        assert!(same(raw, &self.raw));
        self.trace.push("parse".into());
        Ok(self.parsed.clone())
    }
}
fn expected(name: &str) -> Vec<String> {
    let mut out = match name {
        "event-undefined" | "event-null" => vec![],
        "event-getter-error" | "disposed" => vec!["event-data"],
        "disposed-getter-error" => vec!["event-data", "disposed-throw"],
        "track-error" => vec!["event-data", "track"],
        _ => vec!["event-data", "track", "parse", "diagnostic"],
    }
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    if [
        "event-undefined",
        "event-null",
        "event-getter-error",
        "disposed",
        "disposed-getter-error",
        "track-error",
        "diagnostic-error",
    ]
    .contains(&name)
    {
        return out;
    }
    if name.starts_with("subscription") {
        out.push("log:debug".into());
        if name.ends_with("log-error") {
            return out;
        }
    }
    if name == "channel-reentry" {
        out.push("log:debug".into())
    }
    let category = if name.starts_with("pong") {
        Some("pong")
    } else if name.starts_with("mids") || name == "channel-reentry" {
        Some("allMids")
    } else if name.starts_with("fast") {
        Some("fastAssetCtxs")
    } else if ["invalid-json", "json-null", "json-array"].contains(&name) {
        Some("undefined")
    } else if name == "unknown-channel" {
        Some("unknown")
    } else {
        None
    };
    if let Some(category) = category {
        out.push(format!("stats:{category}"))
    }
    out.push("stats:pong".into());
    if name.starts_with("error") {
        out.push("log:error".into())
    } else if [
        "pong",
        "pong-infinity",
        "pong-round-negative-zero",
        "pong-clock-error",
    ]
    .contains(&name)
    {
        out.push("clock".into());
        if name != "pong-clock-error" {
            out.push("latency".into())
        }
    } else if name.starts_with("mids") || name == "channel-reentry" {
        out.push("mids".into())
    } else if name.starts_with("fast") {
        out.push("fast".into())
    } else if name.starts_with("candle") || name.starts_with("book") || name.starts_with("bbo") {
        if name.ends_with("absent") || name.contains("missing-") || name == "candle-empty" {
            return out;
        }
        if name.starts_with("bbo") {
            out.push(if name.ends_with("utf16") {
                "stats:bbo.<D800>".into()
            } else {
                "stats:bbo.BTC".into()
            })
        }
        if name == "book-length-error" {
            out.push("length-throw".into());
            return out;
        }
        if name == "book-index-error" {
            out.push("index-throw".into())
        } else {
            out.push("first".into())
        }
        if name != "candle-first-error" {
            out.push("second".into())
        }
        if name.ends_with("extend") {
            out.push("third".into())
        }
    }
    out
}
fn run(spec: &Value) -> Value {
    let name = spec["name"].as_str().unwrap();
    let mut h = Host::new(name, spec["wire"].as_str().unwrap());
    let socket = h.socket.clone();
    let event = h.event.clone();
    let result = core_public_on_message(&mut h, &socket, &event);
    let fails = [
        "event-undefined",
        "event-null",
        "event-getter-error",
        "disposed-getter-error",
        "track-error",
    ]
    .contains(&name);
    assert_eq!(
        result.is_err(),
        fails,
        "FRAME native independent result kind {name}"
    );
    let error = if let Err(error) = result {
        if name == "event-undefined" || name == "event-null" {
            assert!(same(&get(&error, "name"), &s("TypeError")));
            let message = if name == "event-undefined" {
                "Cannot destructure property 'data' of 'undefined' as it is undefined."
            } else {
                "Cannot destructure property 'data' of 'object null' as it is null."
            };
            assert!(
                same(&get(&error, "message"), &s(message)),
                "FRAME native exact binding error"
            );
            Some(json!(["TypeError", message]))
        } else {
            assert!(
                eq(&error, &h.marker),
                "FRAME native original outside-catch error"
            );
            Some(json!(["original"]))
        }
    } else {
        None
    };
    assert_eq!(
        h.trace,
        expected(name),
        "FRAME native independent effect order {name}"
    );
    let expected_payloads =
        if name.starts_with("mids") || name.starts_with("fast") || name == "channel-reentry" {
            1
        } else {
            expected(name)
                .iter()
                .filter(|s| ["first", "second", "third"].contains(&s.as_str()))
                .count()
        };
    assert_eq!(
        h.payloads.len(),
        expected_payloads,
        "FRAME native independent payload floor {name}"
    );
    if name.ends_with("extend") {
        let N::A(a) = &h.handlers else { unreachable!() };
        assert_eq!(a.borrow().len(), 3, "FRAME native live handler extension")
    };
    if name == "channel-reentry" {
        assert_eq!(h.channel_reads, 10, "FRAME native fresh channel read floor")
    };
    json!({"name":name,"error":error,"trace":h.trace,"payloads":h.payloads.iter().map(encode).collect::<Vec<_>>(),"logs":h.logs.iter().map(|(m,l)|json!([encode(m),encode(l)])).collect::<Vec<_>>(),"latency":h.latency.as_ref().map(encode),"channel_reads":h.channel_reads})
}
#[test]
fn core_public_frames_native_before_source() {
    let inputs: Value =
        serde_json::from_str(include_str!("fixtures/core-public-message-inputs.json")).unwrap();
    assert_eq!(
        inputs.as_array().unwrap().len(),
        52,
        "FRAME independent input floor"
    );
    let mut outputs = vec![];
    let mut failures = vec![];
    for spec in inputs.as_array().unwrap() {
        match catch_unwind(AssertUnwindSafe(|| run(spec))) {
            Ok(out) => outputs.push(out),
            Err(_) => failures.push(spec["name"].clone()),
        }
    }
    assert!(failures.is_empty(), "FRAME native failures {failures:?}");
    assert_eq!(
        outputs.len(),
        52,
        "FRAME independently completed native floor"
    );
    let source = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/core-public-message-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "FRAME source {}",
        String::from_utf8_lossy(&source.stderr)
    );
    let value: Value = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(value["last_index"], 51, "FRAME source final iteration");
    assert_eq!(
        value["cases"],
        json!(outputs),
        "FRAME source after all native assertions"
    );
}
