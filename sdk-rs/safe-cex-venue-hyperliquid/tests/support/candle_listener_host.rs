use super::cache::{self, N, arr, eq, object, s};
use safe_cex_venue_hyperliquid::{
    candle_listeners::*,
    json_boundaries::JsPayloadHost,
    spot_json::JsString,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use serde_json::Value;
use std::collections::BTreeMap;

pub fn input(v: &Value) -> N {
    match v {
        Value::Null => N::Null,
        Value::Bool(v) => N::B(*v),
        Value::Number(v) => N::Num(v.as_f64().unwrap()),
        Value::String(v) => match v.as_str() {
            "$undefined" => N::U,
            "$nan" => N::Num(f64::NAN),
            "$inf" => N::Num(f64::INFINITY),
            "$-0" => N::Num(-0.),
            _ => s(v),
        },
        Value::Object(v) => N::S(JsString(
            v["units"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_u64().unwrap() as u16)
                .collect(),
        )),
        _ => panic!("candle input"),
    }
}
pub fn same(a: &N, b: &N) -> bool {
    eq(a, b) || matches!((a,b),(N::Builtin(a),N::Builtin(b))if a==b)
}
pub fn text(v: &N) -> JsString {
    match v {
        N::S(v) => v.clone(),
        N::Num(v) => JsString(safe_cex_num::js_number_string(*v).encode_utf16().collect()),
        N::U => JsString("undefined".encode_utf16().collect()),
        N::Null => JsString("null".encode_utf16().collect()),
        N::B(v) => JsString(v.to_string().encode_utf16().collect()),
        _ => JsString("[object Object]".encode_utf16().collect()),
    }
}
pub fn own(v: &N, k: &str) -> N {
    let N::O(o) = v else {
        panic!("native own object")
    };
    o.borrow()
        .fields
        .iter()
        .find(|p| p.0 == k)
        .map_or(N::U, |p| p.1.clone())
}
fn quote(units: &[u16]) -> String {
    let mut out = String::from("\"");
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        match u {
            34 => out.push_str("\\\""),
            92 => out.push_str("\\\\"),
            8 => out.push_str("\\b"),
            9 => out.push_str("\\t"),
            10 => out.push_str("\\n"),
            12 => out.push_str("\\f"),
            13 => out.push_str("\\r"),
            0..=31 => out.push_str(&format!("\\u{u:04x}")),
            0xd800..=0xdbff
                if units
                    .get(i + 1)
                    .is_some_and(|n| (0xdc00..=0xdfff).contains(n)) =>
            {
                let n = 0x10000 + ((u as u32 - 0xd800) << 10) + (units[i + 1] as u32 - 0xdc00);
                out.push(char::from_u32(n).unwrap());
                i += 1;
            }
            0xd800..=0xdfff => out.push_str(&format!("\\u{u:04x}")),
            _ => out.push(char::from_u32(u as u32).unwrap()),
        }
        i += 1;
    }
    out.push('"');
    out
}
pub fn stringify(v: &N) -> String {
    match v {
        N::Null | N::U => "null".into(),
        N::B(v) => v.to_string(),
        N::Num(v) => {
            if v.is_finite() {
                safe_cex_num::js_number_string(*v)
            } else {
                "null".into()
            }
        }
        N::S(v) => quote(&v.0),
        N::A(v) => format!(
            "[{}]",
            v.borrow()
                .iter()
                .map(stringify)
                .collect::<Vec<_>>()
                .join(",")
        ),
        N::O(v) => format!(
            "{{{}}}",
            v.borrow()
                .fields
                .iter()
                .filter(|p| !matches!(p.1, N::U))
                .map(|(k, v, _)| format!(
                    "{}:{}",
                    quote(&k.encode_utf16().collect::<Vec<_>>()),
                    stringify(v)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => panic!("fixture stringify"),
    }
}
pub struct Dict {
    pub value: N,
    pub entries: Vec<(JsString, N)>,
}
pub struct Timer {
    pub id: i32,
    pub callback: N,
}
pub struct Host {
    pub row: Value,
    pub socket: N,
    pub opts: N,
    pub error: N,
    pub symbol: N,
    pub frames: N,
    pub first: N,
    pub second: N,
    pub foreign: N,
    pub handlers: N,
    pub topics: N,
    pub other_handlers: N,
    pub other_topics: N,
    pub dicts: Vec<Dict>,
    pub callbacks: BTreeMap<String, CandleCallback<N>>,
    pub timers: Vec<Timer>,
    pub cleared: Vec<i32>,
    pub effects: Vec<N>,
    pub candles: Vec<N>,
    pub sent: Vec<N>,
    pub send_calls: usize,
    pub stat_keys: Vec<JsString>,
    pub cleanup_keys: Vec<JsString>,
    pub logs: Vec<JsString>,
    pub filter_results: Vec<N>,
    next: usize,
    pub interval_reads: usize,
    pub bad_interval_reads: usize,
    handler_reads: usize,
    topic_reads: usize,
    parse_changed: bool,
    clear_reentered: bool,
}
impl Host {
    pub fn scene(&self, n: &str) -> bool {
        self.row["name"] == n
    }
    pub fn set_connected(&mut self, v: bool) {
        cache::Host::default()
            .put(&self.socket, "isConnected", &N::B(v), true)
            .unwrap()
    }
    pub fn coin(&self) -> N {
        if self.row["venue"] == "spot"
            && (self.scene("missing-market") || !self.truthy(&self.symbol))
        {
            s("")
        } else {
            self.symbol.clone()
        }
    }
    pub fn expected_key(&self, handler: bool) -> JsString {
        let mut t = "candle.".encode_utf16().collect::<Vec<_>>();
        t.extend(text(&self.coin()).0);
        t.push(46);
        let i = if handler && self.scene("live-interval") {
            1
        } else {
            0
        };
        t.extend(text(&input(&self.row["expectedIntervals"][i])).0);
        JsString(t)
    }
    pub fn slot(&self, table: &N, key: &JsString) -> N {
        self.dicts
            .iter()
            .find(|d| same(&d.value, table))
            .unwrap()
            .entries
            .iter()
            .find(|p| p.0 == *key)
            .map_or(N::U, |p| p.1.clone())
    }
    fn put_slot(&mut self, table: &N, key: JsString, value: N) {
        let d = self
            .dicts
            .iter_mut()
            .find(|d| same(&d.value, table))
            .unwrap();
        if let Some(p) = d.entries.iter_mut().find(|p| p.0 == key) {
            p.1 = value
        } else {
            d.entries.push((key, value))
        }
    }
    pub fn record(&mut self, n: &str, args: &[N]) {
        let mut v = vec![s(n)];
        v.extend_from_slice(args);
        self.effects.push(arr(v))
    }
    fn frame(symbol: &N, interval: &N, second: bool) -> N {
        object(if second {
            vec![
                ("s", symbol.clone()),
                ("i", interval.clone()),
                ("t", N::Num(2000.)),
                ("o", s("-0")),
                ("h", s("Infinity")),
                ("l", s("nonsense")),
                ("c", s("0x10")),
                ("v", s("  .5tail")),
            ]
        } else {
            vec![
                ("s", symbol.clone()),
                ("i", interval.clone()),
                ("t", N::Num(1234500.)),
                ("o", s("1.25x")),
                ("h", s("8.5")),
                ("l", s("-2.75")),
                ("c", s("3e2junk")),
                ("v", s("4.125")),
            ]
        })
    }
    pub fn new(row: &Value) -> Self {
        let symbol = input(&row["id"]);
        let interval = input(&row["expectedIntervals"][0]);
        let coin = if row["venue"] == "spot"
            && (row["name"] == "falsy-id" || row["name"] == "missing-market")
        {
            s("")
        } else {
            symbol.clone()
        };
        let frame_interval = if row["name"] == "live-interval" {
            s("30m")
        } else if row["name"] == "send-callee-capture" && row["venue"] == "core" {
            s("5m")
        } else {
            interval.clone()
        };
        let first = Self::frame(&coin, &frame_interval, false);
        let second = Self::frame(
            &coin,
            &if row["name"] == "mutate-interval" {
                s("5m")
            } else {
                frame_interval.clone()
            },
            true,
        );
        let frames = match row["name"].as_str().unwrap() {
            "single-frame" => first.clone(),
            "empty-frame" | "stats-mutate" => arr(vec![]),
            "mismatch-frame" => arr(vec![
                Self::frame(&s("OTHER"), &interval, false),
                Self::frame(&coin, &s("wrong"), false),
            ]),
            "connected" => arr(vec![
                first.clone(),
                Self::frame(&s("OTHER"), &interval, false),
                second.clone(),
                Self::frame(&coin, &s("wrong"), false),
            ]),
            "replace-frame" => arr(vec![
                first.clone(),
                Self::frame(&s("OLD"), &s("wrong"), false),
            ]),
            "mutate-interval" | "truncate-frame" | "callback-error" => {
                arr(vec![first.clone(), second.clone()])
            }
            _ => arr(vec![first.clone()]),
        };
        let handlers = object(vec![]);
        let topics = object(vec![]);
        let other_handlers = object(vec![]);
        let other_topics = object(vec![]);
        let market = object(vec![("symbol", s("BTC/USDC")), ("id", symbol.clone())]);
        let markets = arr(if row["name"] == "missing-market" {
            vec![]
        } else {
            vec![object(vec![("symbol", s("OTHER"))]), market]
        });
        let parent = object(vec![("store", object(vec![("markets", markets)]))]);
        let disconnected = matches!(
            row["name"].as_str().unwrap(),
            "disconnected-poll"
                | "unsubscribe-pending"
                | "zero-handle"
                | "queued-poll"
                | "timer-reentry"
                | "clear-getter-reentry"
                | "clear-call-reentry"
        );
        let socket = object(vec![
            ("parent", parent),
            ("isConnected", N::B(!disconnected)),
            ("sendSubscription", N::Builtin("sendSubscription".into())),
            ("send", N::Builtin("send".into())),
            ("recordMessageHandlerStats", N::Builtin("stats".into())),
            ("cleanupMessageHandlerStats", N::Builtin("cleanup".into())),
        ]);
        let mut h = Self {
            row: row.clone(),
            socket,
            opts: object(vec![("symbol", s("BTC/USDC")), ("interval", interval)]),
            error: object(vec![("original", s("candle failure"))]),
            symbol,
            frames,
            first,
            second,
            foreign: N::Builtin("foreign".into()),
            handlers: handlers.clone(),
            topics: topics.clone(),
            other_handlers: other_handlers.clone(),
            other_topics: other_topics.clone(),
            dicts: [handlers, topics, other_handlers, other_topics]
                .into_iter()
                .map(|value| Dict {
                    value,
                    entries: vec![],
                })
                .collect(),
            callbacks: BTreeMap::new(),
            timers: vec![],
            cleared: vec![],
            effects: vec![],
            candles: vec![],
            sent: vec![],
            send_calls: 0,
            stat_keys: vec![],
            cleanup_keys: vec![],
            logs: vec![],
            filter_results: vec![],
            next: 0,
            interval_reads: 0,
            bad_interval_reads: 0,
            handler_reads: 0,
            topic_reads: 0,
            parse_changed: false,
            clear_reentered: false,
        };
        if h.scene("retain-foreign") {
            h.put_slot(
                &h.handlers.clone(),
                h.expected_key(true),
                arr(vec![h.foreign.clone()]),
            )
        }
        if h.scene("string-count") {
            h.put_slot(&h.topics.clone(), h.expected_key(false), s("2"))
        }
        if h.scene("table-swap") {
            h.put_slot(
                &h.other_handlers.clone(),
                h.expected_key(true),
                arr(vec![h.foreign.clone()]),
            );
            h.put_slot(&h.other_topics.clone(), h.expected_key(false), s("2"))
        }
        h
    }
    pub fn parser(&self) -> Option<N> {
        self.callbacks
            .iter()
            .find(|(_, v)| matches!(v, CandleCallback::Parser { .. }))
            .map(|(k, _)| N::Builtin(k.clone()))
    }
    pub fn invoke(&mut self, f: &N, args: &[N]) -> Result<N, N> {
        self.call(f, &object(vec![("foreignThis", N::B(true))]), args)
    }
    pub fn fire(&mut self, index: usize) -> Result<N, N> {
        let callback = self.timers[index].callback.clone();
        self.call(&callback, &N::U, &[])
    }
    fn reenter_clear(&mut self) -> Result<(), N> {
        if !self.clear_reentered {
            self.clear_reentered = true;
            self.set_connected(false);
            self.fire(0)?;
        }
        Ok(())
    }
    pub fn assert_payload(&self, v: &N) {
        let N::O(o) = v else {
            panic!("CANDLE native subscription object")
        };
        assert_eq!(
            o.borrow()
                .fields
                .iter()
                .map(|p| p.0.clone())
                .collect::<Vec<_>>(),
            ["type", "coin", "interval"],
            "CANDLE native exact subscription keys"
        );
        assert!(same(&own(v, "type"), &s("candle")));
        let actual = own(v, "coin");
        let expected = self.coin();
        if let (N::Num(a), N::Num(b)) = (&actual, &expected) {
            assert_eq!(a.to_bits(), b.to_bits(), "CANDLE native raw coin bits")
        } else {
            assert!(same(&actual, &expected), "CANDLE native raw coin");
        }
        assert!(
            same(
                &own(v, "interval"),
                &input(&self.row["expectedSubscriptionInterval"])
            ),
            "CANDLE native subscription fresh interval"
        );
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
            && !matches!(v,N::S(s)if s.0.is_empty())
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        if k == "clearTimeout" && self.scene("clear-getter-reentry") {
            self.reenter_clear()?
        }
        if k == "JSON" && self.scene("send-callee-capture") {
            cache::Host::default()
                .put(&self.socket, "send", &N::Builtin("wrong-send".into()), true)
                .unwrap()
        }
        Ok(match k {
            "Array" => object(vec![("isArray", N::Builtin("isArray".into()))]),
            "JSON" => object(vec![("stringify", N::Builtin("stringify".into()))]),
            "console" => object(vec![("error", N::Builtin("console".into()))]),
            "parseFloat" if self.parse_changed => N::Builtin("parseFloatDouble".into()),
            _ => N::Builtin(k.into()),
        })
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if same(v, &self.socket) {
            match k {
                "messageHandlers" => {
                    let n = self.handler_reads;
                    self.handler_reads += 1;
                    return Ok(if self.scene("table-swap") && n == 1 {
                        self.other_handlers.clone()
                    } else {
                        self.handlers.clone()
                    });
                }
                "topics" => {
                    let n = self.topic_reads;
                    self.topic_reads += 1;
                    return Ok(if self.scene("table-swap") && n == 1 {
                        self.other_topics.clone()
                    } else {
                        self.topics.clone()
                    });
                }
                "sendSubscription" if self.scene("send-callee-capture") => {
                    cache::Host::default()
                        .put(&self.opts, "interval", &s("5m"), true)
                        .unwrap();
                }
                _ => {}
            }
        }
        if same(v, &self.opts) && k == "interval" {
            let n = self.interval_reads;
            self.interval_reads += 1;
            if self.scene("live-interval") {
                let a = self.row["expectedIntervals"].as_array().unwrap();
                return Ok(input(&a[n.min(a.len() - 1)]));
            }
        }
        if self.scene("mismatch-frame")
            && k == "i"
            && matches!(v, N::O(_))
            && same(&own(v, "s"), &s("OTHER"))
        {
            self.bad_interval_reads += 1;
            return Err(self.error.clone());
        }
        if same(v, &self.first) && k == "o" && self.scene("parse-callee-capture") {
            self.parse_changed = true;
        }
        JsPayloadHost::get(&mut cache::Host::default(), v, k).map_err(|_| self.error.clone())
    }
    fn set(&mut self, v: &N, k: &str, x: &N) -> Result<(), N> {
        cache::Host::default()
            .put(v, k, x, true)
            .map_err(|_| self.error.clone())
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn define_data(&mut self, v: &N, k: &str, x: &N) -> Result<(), N> {
        self.set(v, k, x)
    }
    fn callback(&mut self, _: PingCallback<N>) -> Result<N, N> {
        panic!("not ping")
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("native candle callee")
        };
        let capture = self.callbacks.get(id).cloned();
        if let Some(c) = capture {
            return run_candle_callback(self, &c, args);
        }
        match id.as_str() {
            "find" => {
                let N::A(a) = r else { panic!("find receiver") };
                let length = a.borrow().len();
                for i in 0..length {
                    let value = a.borrow().get(i).cloned().unwrap_or(N::U);
                    let found = self.call(
                        &args[0],
                        &N::U,
                        &[value.clone(), N::Num(i as f64), r.clone()],
                    )?;
                    if self.truthy(&found) {
                        return Ok(value);
                    }
                }
                Ok(N::U)
            }
            "filter" => {
                let N::A(a) = r else {
                    panic!("filter receiver")
                };
                let length = a.borrow().len();
                let mut values = vec![];
                for i in 0..length {
                    let value = a.borrow().get(i).cloned().unwrap_or(N::U);
                    let keep = self.call(
                        &args[0],
                        &N::U,
                        &[value.clone(), N::Num(i as f64), r.clone()],
                    )?;
                    if self.truthy(&keep) {
                        values.push(value)
                    }
                }
                let result = arr(values);
                self.filter_results.push(result.clone());
                Ok(result)
            }
            "isArray" => Ok(N::B(matches!(args[0], N::A(_)))),
            "parseFloat" | "parseFloatDouble" => {
                assert!(matches!(r, N::U), "CANDLE native parseFloat receiver");
                let value =
                    safe_cex_num::parse_float(&String::from_utf16(&text(&args[0]).0).unwrap());
                Ok(N::Num(if id == "parseFloatDouble" {
                    value * 2.
                } else {
                    value
                }))
            }
            "stats" => {
                assert!(same(r, &self.socket));
                let N::S(key) = &args[0] else {
                    panic!("stats key")
                };
                self.stat_keys.push(key.clone());
                self.record("stats", args);
                if self.scene("stats-mutate") {
                    let N::A(a) = &self.frames else {
                        unreachable!()
                    };
                    a.borrow_mut().push(self.first.clone());
                }
                Ok(object(vec![("ignored", N::B(true))]))
            }
            "cleanup" => {
                assert!(same(r, &self.socket));
                let N::S(key) = &args[0] else {
                    panic!("cleanup key")
                };
                self.cleanup_keys.push(key.clone());
                self.record("cleanup", args);
                Ok(object(vec![("ignored", N::B(true))]))
            }
            "sendSubscription" => {
                self.send_calls += 1;
                assert!(same(r, &self.socket));
                assert_eq!(args.len(), 1);
                self.assert_payload(&args[0]);
                self.sent.push(args[0].clone());
                self.record("send-subscription", args);
                if self.scene("send-error") {
                    Err(self.error.clone())
                } else {
                    Ok(object(vec![("ignored", N::B(true))]))
                }
            }
            "stringify" => {
                let N::O(o) = &args[0] else { panic!("payload") };
                assert_eq!(
                    o.borrow()
                        .fields
                        .iter()
                        .map(|p| p.0.clone())
                        .collect::<Vec<_>>(),
                    ["method", "subscription"],
                    "CANDLE native exact spot payload keys"
                );
                assert!(same(&own(&args[0], "method"), &s("subscribe")));
                self.assert_payload(&own(&args[0], "subscription"));
                self.sent.push(args[0].clone());
                Ok(s(&stringify(&args[0])))
            }
            "send" => {
                self.send_calls += 1;
                assert_eq!(args.len(), 1, "CANDLE native exact spot send arity");
                assert!(
                    matches!(&args[0], N::S(_)),
                    "CANDLE native spot send string variant"
                );
                assert!(same(r, &self.socket));
                self.record("send", args);
                if self.scene("send-error") {
                    Err(self.error.clone())
                } else {
                    Ok(object(vec![("ignored", N::B(true))]))
                }
            }
            "wrong-send" => panic!("CANDLE native send callee captured before JSON"),
            "setTimeout" => {
                assert!(matches!(r, N::U));
                assert_eq!(args.len(), 2);
                assert!(
                    matches!(args[1],N::Num(n)if n.to_bits()==100f64.to_bits()),
                    "CANDLE native poll delay"
                );
                let index = self.timers.len();
                let id = if self.scene("zero-handle") {
                    0
                } else {
                    42 * (index as i32 + 1)
                };
                self.timers.push(Timer {
                    id,
                    callback: args[0].clone(),
                });
                self.record("timer", &[N::Num(id as f64), args[1].clone()]);
                if self.scene("timer-reentry") && index == 0 {
                    self.fire(0)?;
                }
                Ok(N::Num(id as f64))
            }
            "clearTimeout" => {
                assert!(matches!(r, N::U));
                let id = cache::num(&args[0]) as i32;
                self.cleared.push(id);
                self.record("clear", args);
                if self.scene("clear-call-reentry") {
                    self.reenter_clear()?
                }
                Ok(N::U)
            }
            "output" => {
                assert!(matches!(r, N::U), "CANDLE native callback bare receiver");
                assert_eq!(args.len(), 1, "CANDLE native callback arity");
                self.candles.push(args[0].clone());
                self.record("candle", args);
                let n = self.candles.len();
                if n == 1 {
                    match self.row["name"].as_str().unwrap() {
                        "mutate-interval" => {
                            cache::Host::default()
                                .put(&self.opts, "interval", &s("5m"), true)
                                .unwrap();
                        }
                        "append-frame" => {
                            let N::A(a) = &self.frames else {
                                unreachable!()
                            };
                            a.borrow_mut().push(self.second.clone());
                        }
                        "truncate-frame" => {
                            let N::A(a) = &self.frames else {
                                unreachable!()
                            };
                            a.borrow_mut().clear();
                        }
                        "replace-frame" => {
                            let N::A(a) = &self.frames else {
                                unreachable!()
                            };
                            a.borrow_mut()[1] = self.second.clone();
                        }
                        _ => {}
                    }
                }
                if self.scene("callback-error") {
                    Err(self.error.clone())
                } else {
                    Ok(object(vec![("ignoredThenable", N::B(true))]))
                }
            }
            "console" => {
                let N::S(v) = &args[0] else {
                    panic!("console text")
                };
                self.logs.push(v.clone());
                self.record("console", args);
                Ok(N::U)
            }
            "foreign" => Ok(N::U),
            _ => panic!("unexpected candle callee {id}"),
        }
    }
}
impl CandleListenerHost for Host {
    fn number(&self, v: f64) -> N {
        N::Num(v)
    }
    fn boolean(&self, v: bool) -> N {
        N::B(v)
    }
    fn nullish(&self, v: &N) -> bool {
        matches!(v, N::U | N::Null)
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        same(a, b)
    }
    fn binary(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        Ok(match op {
            "+" if matches!(a, N::S(_)) || matches!(b, N::S(_)) => {
                let mut u = text(a).0;
                u.extend(text(b).0);
                N::S(JsString(u))
            }
            "+" => N::Num(cache::num(a) + cache::num(b)),
            "/" => N::Num(cache::num(a) / cache::num(b)),
            "<" => N::B(cache::num(a) < cache::num(b)),
            _ => panic!("candle binary"),
        })
    }
    fn decrement(&mut self, v: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(v) - 1.))
    }
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        Ok(text(v))
    }
    fn index(&mut self, v: &N, key: &N) -> Result<N, N> {
        if self.dicts.iter().any(|d| same(&d.value, v)) {
            return Ok(self.slot(v, &text(key)));
        }
        self.get(v, &String::from_utf16(&text(key).0).unwrap())
    }
    fn assign_index(&mut self, v: &N, key: &N, x: &N) -> Result<(), N> {
        self.put_slot(v, text(key), x.clone());
        Ok(())
    }
    fn array(&mut self, values: &[N]) -> Result<N, N> {
        Ok(arr(values.to_vec()))
    }
    fn spread_append(&mut self, v: &N, last: &N) -> Result<N, N> {
        let N::A(a) = v else {
            panic!("native iterable")
        };
        let mut values = a.borrow().clone();
        values.push(last.clone());
        Ok(arr(values))
    }
    fn spread_array(&mut self, v: &N) -> Result<N, N> {
        let N::A(a) = v else {
            panic!("native iterable")
        };
        Ok(arr(a.borrow().clone()))
    }
    fn candle_callback(&mut self, c: CandleCallback<N>) -> Result<N, N> {
        let id = format!("candle-{}", self.next);
        self.next += 1;
        self.callbacks.insert(id.clone(), c);
        Ok(N::Builtin(id))
    }
}
