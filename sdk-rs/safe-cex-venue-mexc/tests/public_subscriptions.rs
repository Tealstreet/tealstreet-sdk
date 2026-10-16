use safe_cex_venue_mexc::public_subscriptions::*;
use serde_json::Value as J;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    sync::OnceLock,
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
    Function,
    Object(Rc<Object>),
}
#[derive(Debug)]
struct Object {
    id: u32,
    marker: Cell<u32>,
    fields: Vec<(&'static str, V)>,
    expected_kline: Option<bool>,
}
fn object(id: u32, fields: Vec<(&'static str, V)>) -> V {
    V::Object(Rc::new(Object {
        id,
        marker: Cell::new(id),
        fields,
        expected_kline: None,
    }))
}
fn text(s: &str) -> V {
    V::Text(u(s))
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
    Object(u32),
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
        V::Function => S::Object(91),
        V::Object(o) => S::Object(o.marker.get()),
    }
}
fn same(a: &V, b: &V) -> bool {
    match (a, b) {
        (V::Undefined, V::Undefined)
        | (V::Null, V::Null)
        | (V::Symbol, V::Symbol)
        | (V::Function, V::Function) => true,
        (V::Text(a), V::Text(b)) => a == b,
        (V::Number(a), V::Number(b)) => a == b || (a.is_nan() && b.is_nan()),
        (V::Bool(a), V::Bool(b)) => a == b,
        (V::Object(a), V::Object(b)) => Rc::ptr_eq(a, b),
        _ => false,
    }
}
#[derive(Clone, Debug)]
struct Sub {
    expected_kline: bool,
    marker: u32,
    method: S,
    params: Vec<(&'static str, S)>,
}
fn sub(v: &V) -> Sub {
    let V::Object(o) = v else {
        panic!("native subscription must be an object")
    };
    assert_eq!(
        o.fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        ["method", "param"],
        "native subscription keys before stringify"
    );
    let V::Object(p) = &o.fields[1].1 else {
        panic!("native param object")
    };
    let V::Text(method) = &o.fields[0].1 else {
        panic!("native subscription method")
    };
    let expected_kline = o
        .expected_kline
        .expect("fixture declares subscription role");
    assert_eq!(
        *method,
        u(if expected_kline {
            "sub.kline"
        } else {
            "sub.ticker"
        }),
        "native subscription method from fixed fixture role"
    );
    let expected: Vec<_> = if expected_kline {
        vec!["symbol", "interval"]
    } else {
        vec!["symbol"]
    };
    assert_eq!(
        p.fields.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        expected,
        "native param keys before stringify"
    );
    Sub {
        expected_kline,
        marker: o.id,
        method: snapshot(&o.fields[0].1),
        params: p.fields.iter().map(|(k, v)| (*k, snapshot(v))).collect(),
    }
}
// This is the fixture's JSON.stringify intrinsic, not a wire assertion. The native
// subscription's independent shape is asserted above before this boundary.
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
fn key(v: &V) -> Vec<u16> {
    let s = sub(v);
    let S::Text(method) = s.method else {
        unreachable!()
    };
    let mut r = u("{\"method\":");
    r.extend(quote(&method));
    r.extend(u(",\"param\":{"));
    for (i, (k, v)) in s.params.iter().enumerate() {
        if i > 0 {
            r.push(44);
        }
        r.extend(quote(&u(k)));
        r.push(58);
        match v {
            S::Text(t) => r.extend(quote(t)),
            _ => panic!("fixture stringify domain"),
        }
    }
    r.extend(u("}}"));
    r
}
#[derive(Debug)]
struct Failure {
    name: &'static str,
    message: Vec<u16>,
    marker: Option<u32>,
}
type E = Rc<Failure>;
fn fail(name: &'static str, message: &str) -> E {
    Rc::new(Failure {
        name,
        message: u(message),
        marker: None,
    })
}
#[derive(Debug)]
enum D {
    Value(S),
    Units(Vec<u16>),
    Int(u32),
    Bool(bool),
    Sub(Sub),
    Headers(Vec<(&'static str, Vec<u16>)>),
    Word(&'static str),
}
#[derive(Debug)]
struct Event {
    tag: String,
    fields: Vec<(&'static str, D)>,
}
type Map = Rc<RefCell<Vec<Option<(Vec<u16>, V)>>>>;
struct Cursor {
    map: Map,
    index: usize,
    values: bool,
}
#[derive(Clone)]
enum Handlers {
    Array(Rc<RefCell<Vec<Option<u32>>>>),
    Constructor,
    ToString,
}
struct Iter {
    handlers: Rc<RefCell<Vec<Option<u32>>>>,
    index: usize,
}
type Handler = Rc<dyn Fn(V) -> Result<(), E>>;
struct Host {
    name: &'static str,
    i: usize,
    original: E,
    trace: RefCell<Vec<Event>>,
    socket: Cell<Option<u32>>,
    opts: V,
    json: V,
    raw: V,
    normalized: V,
    interval_reads: Cell<usize>,
    symbol_reads: Cell<usize>,
    next_sub: Cell<u32>,
    registered: RefCell<Option<Handler>>,
    candle_created: RefCell<Option<V>>,
    send_options: RefCell<Option<SendOptions<V, E>>>,
    receive_options: RefCell<Option<ReceiveOptions<V, E>>>,
    ticker: RefCell<Map>,
    regular: RefCell<Map>,
    extra: V,
    replacement: V,
    child_count: Cell<usize>,
    send_count: Cell<usize>,
    channel: RefCell<V>,
    channel_reads: Cell<usize>,
    handlers: RefCell<Handlers>,
}
fn make_sub(
    id: u32,
    method: &'static str,
    params: Vec<(&'static str, V)>,
    expected_kline: bool,
) -> V {
    V::Object(Rc::new(Object {
        id,
        marker: Cell::new(id),
        expected_kline: Some(expected_kline),
        fields: vec![
            ("method", text(method)),
            ("param", object(id + 10000, params)),
        ],
    }))
}
fn old(id: u32, method: &'static str, symbol: &str) -> V {
    let expected_kline = matches!(id, 12 | 13);
    let mut params = vec![("symbol", text(symbol))];
    if expected_kline {
        params.push(("interval", text("Min1")));
    }
    make_sub(id, method, params, expected_kline)
}
fn map(values: Vec<V>) -> Map {
    Rc::new(RefCell::new(
        values.into_iter().map(|v| Some((key(&v), v))).collect(),
    ))
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
            trace: RefCell::new(vec![]),
            socket: Cell::new(if i == 2 { Some(99) } else { None }),
            opts: object(1, vec![]),
            json: object(71, vec![]),
            raw: object(83, vec![]),
            normalized: if i == 6 {
                V::Null
            } else if i == 7 {
                V::Number(0.0)
            } else {
                object(
                    89,
                    vec![("timestamp", V::Number(-0.0)), ("open", V::Number(0.125))],
                )
            },
            interval_reads: Cell::new(0),
            symbol_reads: Cell::new(0),
            next_sub: Cell::new(100),
            registered: RefCell::new(None),
            candle_created: RefCell::new(None),
            send_options: RefCell::new(None),
            receive_options: RefCell::new(None),
            ticker: RefCell::new(map(if i == 2 {
                vec![]
            } else {
                vec![
                    old(10, "sub.ticker", "OLD_USDT"),
                    old(11, "sub.ticker", "ID0_USDT"),
                ]
            })),
            regular: RefCell::new(map(vec![
                old(12, "sub.kline", "REG1_USDT"),
                old(13, "sub.kline", "REG2_USDT"),
            ])),
            extra: old(14, "sub.ticker", "EXTRA_USDT"),
            replacement: old(15, "sub.ticker", "REPLACEMENT_USDT"),
            child_count: Cell::new(0),
            send_count: Cell::new(0),
            channel: RefCell::new(text(match i {
                2 => "push.ticker",
                3 => "push.depth",
                4 => "push.depth.full",
                _ => "other",
            })),
            channel_reads: Cell::new(0),
            handlers: RefCell::new(Handlers::Array(Rc::new(RefCell::new(if i == 1 {
                vec![]
            } else {
                vec![Some(11), Some(13)]
            })))),
        })
    }
    fn failure_point(&self) -> &'static str {
        match (self.name, self.i) {
            ("connectAndSubscribe", 4) => "create",
            ("connectAndSubscribe", 5) => "add:message",
            ("connectAndSubscribe", 11) => "listener:close",
            ("listenOHLCV", 4) => "add",
            ("listenOHLCV", 5) => "subscribe",
            ("listenOHLCV", 8) => "callback",
            ("handleActiveTickerSymbolsChanged", 5) => "reconcile",
            ("reconcileTickerSubscriptions", 4) => "unsubscribe",
            ("reconcileTickerSubscriptions", 5) => "subscribe",
            ("resubscribe", 5) => "child:send",
            ("sendWithDiagnostics", 4) => "send",
            ("sendWithDiagnostics", 5) => "emit:send",
            ("emitReceiveDiagnostics", 5) => "emit:receive",
            ("handleRoutedMessage", 6) => "handler:11",
            _ => "",
        }
    }
    fn hit(&self, tag: impl Into<String>, fields: Vec<(&'static str, D)>) -> Result<(), E> {
        let tag = tag.into();
        let throw = tag == self.failure_point();
        self.trace.borrow_mut().push(Event { tag, fields });
        if throw {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn tag(&self, tag: &str) -> Result<(), E> {
        self.hit(tag, vec![])
    }
    fn subscription(&self, method: &'static str, param: Vec<(&'static str, V)>) -> V {
        let id = self.next_sub.get();
        self.next_sub.set(id + 1);
        let value = make_sub(id, method, param, self.name == "listenOHLCV");
        if self.name == "listenOHLCV" {
            *self.candle_created.borrow_mut() = Some(value.clone());
        }
        value
    }
    fn child(&self, name: &str, s: V, track: V) -> Result<(), E> {
        self.hit(
            name,
            vec![
                ("subscription", D::Sub(sub(&s))),
                ("track", D::Value(snapshot(&track))),
            ],
        )?;
        if name == "unsubscribe" && self.name == "reconcileTickerSubscriptions" && self.i == 7 {
            let c = self.child_count.replace(self.child_count.get() + 1);
            if c == 0 {
                *self.ticker.borrow_mut() = map(vec![self.replacement.clone()]);
            }
        }
        Ok(())
    }
    fn stringify(&self, v: &V) -> Result<Vec<u16>, E> {
        let native = sub(v);
        self.hit("stringify", vec![("subscription", D::Sub(native))])?;
        Ok(key(v))
    }
    fn map_set(m: &Map, k: Vec<u16>, v: V) {
        let mut rows = m.borrow_mut();
        if let Some(row) = rows.iter_mut().flatten().find(|(key, _)| *key == k) {
            row.1 = v;
        } else {
            rows.push(Some((k, v)));
        }
    }
    fn map_delete(m: &Map, k: &[u16]) {
        let mut rows = m.borrow_mut();
        for row in rows.iter_mut() {
            if row.as_ref().is_some_and(|(key, _)| key == k) {
                *row = None;
                break;
            }
        }
    }
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
            V::Bool(b) => *b,
            V::Number(n) => *n != 0.0 && !n.is_nan(),
            V::Text(t) => !t.is_empty(),
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
        let V::Object(o) = v else {
            panic!("native property receiver")
        };
        match (o.id, k) {
            (1, "interval") => {
                self.tag("field:interval")?;
                let c = self.interval_reads.replace(self.interval_reads.get() + 1);
                Ok(if self.i == 2 {
                    if c == 0 {
                        text("unsupported")
                    } else {
                        V::Text([u("changed"), vec![0xdfff]].concat())
                    }
                } else {
                    text(if self.i == 3 {
                        "constructor"
                    } else if self.i == 11 {
                        "5m"
                    } else {
                        "1m"
                    })
                })
            }
            (1, "symbol") => {
                self.tag("field:symbol")?;
                let c = self.symbol_reads.replace(self.symbol_reads.get() + 1);
                Ok(if self.i == 8 {
                    V::Text([vec![0xd800], u("_USDT")].concat())
                } else {
                    text(if self.i == 9 && c > 0 {
                        "CHANGED_USDT"
                    } else {
                        "BTC_USDT"
                    })
                })
            }
            (71, "data") => {
                self.tag("field:data")?;
                Ok(if self.i == 6 || self.i == 7 {
                    V::Number(0.0)
                } else {
                    object(73, vec![])
                })
            }
            (71, "channel") => {
                self.tag("field:channel")?;
                if self.i == 11 {
                    return Err(self.original.clone());
                }
                if self.i == 10 {
                    let c = self.channel_reads.replace(self.channel_reads.get() + 1);
                    Ok(text(["other", "push.depth", "unused"][c % 3]))
                } else {
                    Ok(self.channel.borrow().clone())
                }
            }
            (id, "id") if (2000..2200).contains(&id) => {
                self.tag("field:id")?;
                Ok(text(&format!(
                    "ID{}_USDT",
                    if self.i == 9 && id == 2001 {
                        0
                    } else {
                        id - 2000
                    }
                )))
            }
            _ => Ok(o
                .fields
                .iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.clone())
                .unwrap_or(V::Undefined)),
        }
    }
    fn abstract_string(&self, v: &V) -> Result<Vec<u16>, E> {
        Ok(match v {
            V::Undefined => u("undefined"),
            V::Null => u("null"),
            V::Text(t) => t.clone(),
            V::Bool(b) => u(if *b { "true" } else { "false" }),
            V::Number(n) if n.is_nan() => u("NaN"),
            V::Number(n) if *n == 0.0 => u("0"),
            V::Number(n) => u(&n.to_string()),
            V::Symbol => {
                return Err(fail(
                    "TypeError",
                    "Cannot convert a Symbol value to a string",
                ));
            }
            V::Function => u("function Object() { [native code] }"),
            V::Object(_) => u("[object Object]"),
        })
    }
    fn global_string(&self, v: &V) -> Result<Vec<u16>, E> {
        if matches!(v, V::Symbol) {
            Ok(u("Symbol(profile)"))
        } else {
            self.abstract_string(v)
        }
    }
    fn log(&self, text: &[u16], severity: &'static str) -> Result<(), E> {
        self.hit(
            "log",
            vec![
                ("text", D::Units(text.to_vec())),
                ("severity", D::Word(severity)),
            ],
        )
    }
    fn new_error(&self, text: Vec<u16>) -> E {
        Rc::new(Failure {
            name: "Error",
            message: text,
            marker: None,
        })
    }
}
impl ConnectHost for Host {
    type Socket = u32;
    type Listener = u32;
    fn disposed(&self) -> bool {
        self.hit("disposed", vec![("value", D::Bool(self.i == 1))])
            .unwrap();
        self.i == 1
    }
    fn testnet(&self) -> Result<V, E> {
        let v = if self.i == 3 {
            text("false")
        } else {
            V::Bool(self.i % 2 == 1)
        };
        self.hit("testnet", vec![("value", D::Value(snapshot(&v)))])?;
        Ok(v)
    }
    fn url(&self, network: &'static str) -> Result<Vec<u16>, E> {
        self.tag(&format!("url:{network}"))?;
        Ok(if self.i == 8 {
            [u("wss://changed/"), vec![0xd800]].concat()
        } else {
            u("wss://contract.mexc.com/edge")
        })
    }
    fn create(&self, r: DialRequest<V>) -> Result<u32, E> {
        assert!(
            matches!(r.protocols, V::Undefined),
            "native dial undefined protocols"
        );
        assert_eq!(
            r.headers,
            vec![("User-Agent", u("Tealstreet"))],
            "native dial headers"
        );
        self.hit(
            "create",
            vec![
                ("url", D::Units(r.url)),
                ("protocols", D::Value(snapshot(&r.protocols))),
                ("headers", D::Headers(r.headers)),
            ],
        )?;
        Ok(71)
    }
    fn assign_socket(&self, s: u32) -> Result<(), E> {
        self.hit("assign", vec![("id", D::Int(s))])?;
        self.socket.set(Some(s));
        Ok(())
    }
    fn socket(&self) -> Result<u32, E> {
        let s = self.socket.get().expect("native current socket");
        self.hit("socket", vec![("id", D::Int(s))])?;
        Ok(s)
    }
    fn listener(&self, event: &'static str) -> Result<u32, E> {
        let id = match event {
            "open" => 11,
            "message" => 13,
            "close" => 17,
            _ => panic!("native event"),
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
impl CandleHost for Host {
    type Callback = u32;
    fn interval(&self, k: &V) -> Result<V, E> {
        self.hit("interval", vec![("key", D::Value(snapshot(k)))])?;
        Ok(if self.is_text(k, "constructor") {
            V::Function
        } else if self.is_text(k, "1m") {
            text("Min1")
        } else if self.is_text(k, "5m") {
            text("Min5")
        } else {
            V::Undefined
        })
    }
    fn to_symbol(&self, s: &V) -> Result<V, E> {
        let V::Text(units) = s else {
            panic!("native symbol string")
        };
        assert!(
            units.contains(&95),
            "fixture executes original underscore-path symbol utility"
        );
        Ok(s.clone())
    }
    fn subscription(&self, m: &'static str, p: Vec<(&'static str, V)>) -> V {
        Host::subscription(self, m, p)
    }
    fn normalize(&self, data: &V) -> Result<V, E> {
        self.hit("normalize", vec![("data", D::Value(snapshot(data)))])?;
        Ok(self.normalized.clone())
    }
    fn callback(&self, cb: &u32, candle: V) -> Result<(), E> {
        assert_eq!(*cb, 37, "native original callback identity");
        assert!(
            same(&candle, &self.normalized),
            "native original normalized candle identity"
        );
        if let V::Object(o) = &candle {
            let V::Number(timestamp) = o.fields[0].1 else {
                unreachable!()
            };
            let V::Number(open) = o.fields[1].1 else {
                unreachable!()
            };
            assert_eq!(
                timestamp.to_bits(),
                (-0.0f64).to_bits(),
                "native candle timestamp bits"
            );
            assert_eq!(
                open.to_bits(),
                0.125f64.to_bits(),
                "native candle open bits"
            );
        }
        self.hit("callback", vec![("candle", D::Value(snapshot(&candle)))])
    }
    fn add_handler(&self, key: &[u16], handler: Handler) -> Result<(), E> {
        self.hit("add", vec![("key", D::Units(key.to_vec()))])?;
        *self.registered.borrow_mut() = Some(handler);
        Ok(())
    }
    fn remove_handler(&self, key: &[u16], handler: &Handler) -> Result<(), E> {
        assert!(
            Rc::ptr_eq(handler, self.registered.borrow().as_ref().unwrap()),
            "native original handler identity"
        );
        self.hit("remove", vec![("key", D::Units(key.to_vec()))])
    }
    fn subscribe(&self, s: V) -> Result<(), E> {
        self.child("subscribe", s, V::Undefined)
    }
    fn unsubscribe(&self, s: V) -> Result<(), E> {
        assert!(
            same(&s, self.candle_created.borrow().as_ref().unwrap()),
            "native original cleanup subscription identity"
        );
        self.child("unsubscribe", s, V::Undefined)
    }
}
impl TickerHost for Host {
    type Map = Map;
    type Cursor = Cursor;
    fn disposed(&self) -> bool {
        ConnectHost::disposed(self)
    }
    fn reconcile(&self) -> Result<(), E> {
        self.tag("reconcile")
    }
    fn available_symbols(&self) -> Result<Vec<V>, E> {
        self.tag("available")?;
        Ok((0..if self.i == 10 {
            101
        } else if self.i == 11 {
            102
        } else {
            3
        })
            .map(|j| text(&format!("S{j}_USDT")))
            .collect())
    }
    fn active_symbols(&self) -> Result<Vec<V>, E> {
        self.tag("active")?;
        Ok(match self.i {
            2 => vec![text("S1_USDT"), text("ABSENT")],
            3 => vec![text("ABSENT")],
            8 => vec![text("S2_USDT"), text("S0_USDT"), text("S2_USDT")],
            _ => vec![],
        })
    }
    fn market(&self, s: &V) -> Result<V, E> {
        self.hit("market", vec![("symbol", D::Value(snapshot(s)))])?;
        let V::Text(t) = s else { unreachable!() };
        let name = String::from_utf16(t).unwrap();
        let j: u32 = name
            .trim_start_matches('S')
            .trim_end_matches("_USDT")
            .parse()
            .unwrap();
        Ok(if self.i == 11 && j == 0 {
            V::Undefined
        } else {
            object(2000 + j, vec![])
        })
    }
    fn subscription(&self, m: &'static str, p: Vec<(&'static str, V)>) -> V {
        Host::subscription(self, m, p)
    }
    fn stringify(&self, v: &V) -> Result<Vec<u16>, E> {
        Host::stringify(self, v)
    }
    fn map(&self, name: &'static str) -> Result<Map, E> {
        self.tag(if name == "subscriptions" {
            "map:regular"
        } else {
            "map:ticker"
        })?;
        Ok(if name == "subscriptions" {
            self.regular.borrow().clone()
        } else {
            self.ticker.borrow().clone()
        })
    }
    fn cursor(&self, map: Map, values: bool) -> Result<Cursor, E> {
        Ok(Cursor {
            map,
            index: 0,
            values,
        })
    }
    fn next(&self, c: &mut Cursor) -> Result<Option<(Vec<u16>, V)>, E> {
        loop {
            let rows = c.map.borrow();
            if c.index >= rows.len() {
                return Ok(None);
            }
            let row = rows[c.index].clone();
            drop(rows);
            c.index += 1;
            if let Some((k, v)) = row {
                let mut fields = vec![("subscription", D::Sub(sub(&v)))];
                if !c.values {
                    fields.push(("key", D::Units(k.clone())));
                }
                self.hit(if c.values { "value" } else { "entry" }, fields)?;
                return Ok(Some((k, v)));
            }
        }
    }
    fn has(&self, m: &Map, k: &[u16]) -> Result<bool, E> {
        self.hit("has", vec![("key", D::Units(k.to_vec()))])?;
        Ok(m.borrow().iter().flatten().any(|(key, _)| key == k))
    }
    fn set(&self, m: &Map, k: Vec<u16>, v: V) -> Result<(), E> {
        self.hit(
            "set",
            vec![
                ("key", D::Units(k.clone())),
                ("subscription", D::Sub(sub(&v))),
            ],
        )?;
        Self::map_set(m, k, v);
        Ok(())
    }
    fn delete(&self, m: &Map, k: &[u16]) -> Result<(), E> {
        self.hit("delete", vec![("key", D::Units(k.to_vec()))])?;
        Self::map_delete(m, k);
        Ok(())
    }
    fn subscribe(&self, s: V, track: bool) -> Result<(), E> {
        self.child("subscribe", s, V::Bool(track))
    }
    fn unsubscribe(&self, s: V, track: bool) -> Result<(), E> {
        self.child("unsubscribe", s, V::Bool(track))
    }
    fn send(&self, data: Vec<u16>) -> Result<(), E> {
        self.hit("child:send", vec![("data", D::Units(data))])?;
        let n = self.send_count.replace(self.send_count.get() + 1);
        if self.name == "resubscribe" && n == 0 {
            if self.i == 7 {
                let regular = self.regular.borrow().clone();
                Self::map_delete(&regular, &key(&old(13, "sub.kline", "REG2_USDT")));
                Self::map_set(&regular, key(&self.extra), self.extra.clone());
            }
            if self.i == 8 {
                *self.regular.borrow_mut() = map(vec![self.replacement.clone()]);
                *self.ticker.borrow_mut() = map(vec![self.replacement.clone()]);
            }
        }
        Ok(())
    }
}
impl DiagnosticHost for Host {
    fn send(&self, data: &[u16]) -> Result<(), E> {
        self.hit("send", vec![("data", D::Units(data.to_vec()))])
    }
    fn emit_send(&self, data: &[u16], options: SendOptions<V, E>) -> Result<(), E> {
        self.hit("emit:send", vec![("data", D::Units(data.to_vec()))])?;
        *self.send_options.borrow_mut() = Some(options);
        Ok(())
    }
    fn emit_receive(&self, data: V, options: ReceiveOptions<V, E>) -> Result<(), E> {
        assert!(
            same(&data, &self.raw),
            "native original receive data identity"
        );
        self.hit("emit:receive", vec![("data", D::Value(snapshot(&data)))])?;
        *self.receive_options.borrow_mut() = Some(options);
        Ok(())
    }
}
impl RouteHost for Host {
    type Handlers = Handlers;
    type Iterator = Iter;
    type Handler = Option<u32>;
    fn handlers(&self, key: &[u16]) -> Result<Option<Handlers>, E> {
        Ok(if key == u("constructor") {
            Some(Handlers::Constructor)
        } else if key == u("toString") {
            Some(Handlers::ToString)
        } else if key == u("missing") {
            None
        } else {
            Some(self.handlers.borrow().clone())
        })
    }
    fn length(&self, h: &Handlers) -> Result<V, E> {
        let n = match h {
            Handlers::Array(a) => {
                let n = a.borrow().len();
                self.hit("length", vec![("length", D::Int(n as u32))])?;
                n
            }
            Handlers::Constructor => 1,
            Handlers::ToString => 0,
        };
        Ok(V::Number(n as f64))
    }
    fn iterator(&self, h: Handlers) -> Result<Iter, E> {
        match h {
            Handlers::Array(a) => Ok(Iter {
                handlers: a,
                index: 0,
            }),
            _ => Err(fail("TypeError", "handlers is not iterable")),
        }
    }
    fn next_handler(&self, it: &mut Iter) -> Result<Option<Option<u32>>, E> {
        let n = it.handlers.borrow().len();
        self.hit("length", vec![("length", D::Int(n as u32))])?;
        if it.index >= n {
            return Ok(None);
        }
        let index = it.index;
        it.index += 1;
        self.hit("at", vec![("index", D::Int(index as u32))])?;
        Ok(Some(it.handlers.borrow()[index]))
    }
    fn invoke(&self, h: Option<u32>, json: &V) -> Result<(), E> {
        assert!(
            same(json, &self.json),
            "native original routed JSON identity"
        );
        let Some(id) = h else {
            return Err(fail("TypeError", "handler is not a function"));
        };
        self.hit(
            format!("handler:{id}"),
            vec![("data", D::Value(snapshot(json)))],
        )?;
        if id == 11 {
            if self.i == 0 {
                let V::Object(o) = json else { unreachable!() };
                o.marker.set(91);
            }
            if self.i == 8 {
                let Handlers::Array(a) = &*self.handlers.borrow() else {
                    unreachable!()
                };
                a.borrow_mut().push(Some(17));
            }
            if self.i == 9 {
                let Handlers::Array(a) = &*self.handlers.borrow() else {
                    unreachable!()
                };
                a.borrow_mut()[1] = None;
            }
            if self.i == 7 {
                *self.handlers.borrow_mut() =
                    Handlers::Array(Rc::new(RefCell::new(vec![Some(99)])));
            }
        }
        Ok(())
    }
}
fn oracle() -> &'static J {
    static O: OnceLock<J> = OnceLock::new();
    O.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/public_subscriptions_oracle.cjs"
            ))
            .output()
            .expect("source oracle launch");
        assert!(
            output.status.success(),
            "source oracle failure: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let v: J = serde_json::from_slice(&output.stdout).expect("source oracle JSON");
        assert_eq!(
            v["cases"].as_array().unwrap().len(),
            96,
            "independent eight methods times twelve fixtures"
        );
        v
    })
}
fn source_units(v: &J) -> Vec<u16> {
    v.as_array()
        .expect("source UTF-16 units")
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn assert_value(v: &S, s: &J, label: &str) {
    let kind = match v {
        S::Undefined => "undefined",
        S::Null => "null",
        S::Text(_) => "text",
        S::Number(_) => "number",
        S::Nan => "nan",
        S::Bool(_) => "bool",
        S::Symbol => "symbol",
        S::Object(_) => "object",
    };
    assert_eq!(
        kind,
        s["kind"].as_str().unwrap(),
        "native value kind: {label}"
    );
    match v {
        S::Text(t) => assert_eq!(*t, source_units(&s["units"]), "native UTF-16: {label}"),
        S::Number(bits) => assert_eq!(
            *bits,
            u64::from_str_radix(s["bits"].as_str().unwrap(), 16).unwrap(),
            "native number bits: {label}"
        ),
        S::Bool(b) => assert_eq!(*b, s["value"].as_bool().unwrap(), "native boolean: {label}"),
        S::Object(marker) => assert_eq!(
            *marker,
            s["marker"].as_u64().unwrap() as u32,
            "native object identity/marker: {label}"
        ),
        _ => {}
    }
}
fn assert_sub(v: &Sub, s: &J, label: &str) {
    assert_eq!(
        v.marker,
        s["marker"].as_u64().unwrap() as u32,
        "native subscription identity: {label}"
    );
    assert_value(&v.method, &s["method"], label);
    assert_eq!(
        s["method"]["units"]
            .as_array()
            .map(|_| source_units(&s["method"]["units"])),
        Some(u(if v.expected_kline {
            "sub.kline"
        } else {
            "sub.ticker"
        })),
        "source method from fixed fixture role"
    );
    let params = s["params"].as_array().unwrap();
    let expected = if v.expected_kline { 2 } else { 1 };
    assert_eq!(
        params.len(),
        expected,
        "independent source param count: {label}"
    );
    assert_eq!(
        v.params.len(),
        expected,
        "independent native param count: {label}"
    );
    for ((k, v), p) in v.params.iter().zip(params) {
        assert_eq!(*k, p[0].as_str().unwrap(), "native param key: {label}");
        assert_value(v, &p[1], label);
    }
}
fn assert_error(result: &Result<(), E>, s: &J, host: &Host, label: &str) {
    match (result, s.is_null()) {
        (Ok(()), true) => {}
        (Err(e), false) => {
            assert_eq!(
                e.name,
                s["name"].as_str().unwrap(),
                "native error name: {label}"
            );
            assert_eq!(
                e.message,
                source_units(&s["message"]),
                "native error message: {label}"
            );
            assert_eq!(
                e.marker,
                s["marker"].as_u64().map(|n| n as u32),
                "native error marker: {label}"
            );
            assert_eq!(
                Rc::ptr_eq(e, &host.original),
                s["same"].as_bool().unwrap(),
                "native original error identity: {label}"
            );
        }
        _ => panic!("native error outcome: {label}: {result:?} vs {s}"),
    }
}
fn expected_count(name: &str, i: usize) -> usize {
    match name {
        "connectAndSubscribe" => [14, 1, 14, 14, 4, 11, 14, 14, 14, 14, 14, 13][i],
        "listenOHLCV" => [13, 13, 4, 13, 5, 9, 12, 12, 13, 13, 13, 13][i],
        "handleActiveTickerSymbolsChanged" => {
            if i == 1 {
                1
            } else {
                2
            }
        }
        "reconcileTickerSubscriptions" => [29, 29, 11, 11, 14, 24, 29, 32, 21, 24, 805, 804][i],
        "resubscribe" => {
            if i == 2 {
                8
            } else if i == 5 {
                4
            } else if i == 8 {
                11
            } else {
                14
            }
        }
        "sendWithDiagnostics" => {
            if i == 4 {
                1
            } else {
                2
            }
        }
        "emitReceiveDiagnostics" => [10, 10, 8, 9, 10, 1, 10, 10, 10, 10, 9, 4][i],
        "handleRoutedMessage" => [8, 1, 0, 0, 0, 8, 4, 8, 11, 6, 8, 8][i],
        _ => unreachable!(),
    }
}
fn assert_trace(host: &Host, s: &J) {
    let source = s["trace"].as_array().unwrap();
    let trace = host.trace.borrow();
    let label = format!("{}/{}", host.name, host.i);
    let expected = expected_count(host.name, host.i);
    assert_eq!(
        trace.len(),
        expected,
        "native independent trace count {label}"
    );
    assert_eq!(
        source.len(),
        expected,
        "source independent trace count {label}"
    );
    for (n, (e, row)) in trace.iter().zip(source).enumerate() {
        let label = format!("{label}/event{n}");
        assert_eq!(
            e.tag,
            row["tag"].as_str().unwrap(),
            "native event order: {label}"
        );
        assert_eq!(
            e.fields.len() + 1,
            row.as_object().unwrap().len(),
            "native event field count: {label}"
        );
        for (k, d) in &e.fields {
            match d {
                D::Value(v) => assert_value(v, &row[*k], &label),
                D::Units(t) => assert_eq!(
                    *t,
                    source_units(&row[*k]),
                    "native event UTF-16: {label}/{k}"
                ),
                D::Int(n) => assert_eq!(
                    *n,
                    row[*k].as_u64().unwrap() as u32,
                    "native event integer: {label}/{k}"
                ),
                D::Bool(b) => assert_eq!(
                    *b,
                    row[*k].as_bool().unwrap(),
                    "native event boolean: {label}/{k}"
                ),
                D::Word(t) => assert_eq!(
                    *t,
                    row[*k].as_str().unwrap(),
                    "native event word: {label}/{k}"
                ),
                D::Sub(v) => assert_sub(v, &row[*k], &label),
                D::Headers(headers) => {
                    assert_eq!(headers.len(), 1, "native independent headers count");
                    let values = row[*k].as_array().unwrap();
                    assert_eq!(values.len(), 1, "source independent headers count");
                    assert_eq!(
                        headers[0].0,
                        values[0][0].as_str().unwrap(),
                        "native header name"
                    );
                    assert_eq!(
                        headers[0].1,
                        source_units(&values[0][1]),
                        "native header value"
                    );
                }
            }
        }
    }
}
fn map_count(name: &str, i: usize, ticker: bool) -> usize {
    if name == "resubscribe" && i == 8 {
        return 1;
    }
    if !ticker {
        return 2;
    }
    if name != "reconcileTickerSubscriptions" {
        return if i == 2 { 0 } else { 2 };
    }
    match i {
        0 | 1 | 6 => 3,
        2 | 5 => 1 + usize::from(i == 5),
        3 => 0,
        4 => 2,
        7 => 4,
        8 | 9 => 2,
        10 => 100,
        11 => 99,
        _ => unreachable!(),
    }
}
fn assert_map(host: &Host, name: &str, s: &J) {
    let m = if name == "ticker" {
        host.ticker.borrow().clone()
    } else {
        host.regular.borrow().clone()
    };
    let rows = m.borrow();
    let native = rows.iter().flatten().collect::<Vec<_>>();
    let source = s[name].as_array().unwrap();
    let count = map_count(host.name, host.i, name == "ticker");
    assert_eq!(
        native.len(),
        count,
        "native independent map count {}/{}/{}",
        host.name,
        host.i,
        name
    );
    assert_eq!(source.len(), count, "source independent map count");
    for ((k, v), row) in native.iter().zip(source) {
        assert_sub(&sub(v), &row[1], "native final subscription fields");
        assert_eq!(*k, source_units(&row[0]), "native final map key");
    }
}
fn input(i: usize) -> Vec<u16> {
    match i {
        2 => [u("🦀"), vec![0xd800]].concat(),
        3 => vec![0xdfff],
        _ => u([
            "wire/0", "", "", "", "fail", "emit", "x\0y", "mutable", "symbol", "holes", "cap",
            "last",
        ][i]),
    }
}
fn route_key(i: usize) -> Vec<u16> {
    u(match i {
        2 => "constructor",
        3 => "toString",
        4 => "missing",
        _ => "route",
    })
}
const NAMES: [&str; 8] = [
    "connectAndSubscribe",
    "listenOHLCV",
    "handleActiveTickerSymbolsChanged",
    "reconcileTickerSubscriptions",
    "resubscribe",
    "sendWithDiagnostics",
    "emitReceiveDiagnostics",
    "handleRoutedMessage",
];
fn run(family: usize) {
    let source = oracle();
    for i in 0..12 {
        let s = &source["cases"][family * 12 + i];
        let name = NAMES[family];
        assert_eq!(s["name"], name);
        assert_eq!(s["index"], i);
        assert_eq!(
            source_units(&s["data"]),
            input(i),
            "independent source root data"
        );
        assert_eq!(
            source_units(&s["key"]),
            route_key(i),
            "independent source route key"
        );
        let host = Host::new(name, i);
        let mut cleanup = None;
        let result = match family {
            0 => connect(&*host),
            1 => listen_ohlcv(host.clone(), &host.opts, 37).map(|fn_| cleanup = Some(fn_)),
            2 => active_tickers_changed(&*host),
            3 => reconcile_tickers(&*host),
            4 => resubscribe(&*host),
            5 => send_diagnostics(host.clone(), &input(i)),
            6 => receive_diagnostics(host.clone(), host.raw.clone(), host.json.clone()),
            7 => route_message(&*host, &route_key(i), &host.json),
            _ => unreachable!(),
        };
        assert_error(&result, &s["error"], &host, "root");
        let mut ticks = vec![];
        if let Some(handler) = host.registered.borrow().as_ref() {
            ticks.push(handler(host.json.clone()));
        }
        let cleanups = usize::from(cleanup.is_some()) * 2;
        if let Some(cleanup) = cleanup {
            for _ in 0..2 {
                ticks.push(cleanup());
            }
        }
        let expected_registered = usize::from(family == 1 && !matches!(i, 2 | 4));
        let expected_cleanup = if family == 1 && !matches!(i, 2 | 4 | 5) {
            2
        } else {
            0
        };
        assert_eq!(
            ticks.len(),
            expected_registered + expected_cleanup,
            "native independent callback replay count"
        );
        assert_eq!(
            cleanups, expected_cleanup,
            "native independent cleanup replay count"
        );
        assert_eq!(s["cleanupTicks"], expected_cleanup);
        let st = s["callbackTicks"].as_array().unwrap();
        assert_eq!(
            st.len(),
            expected_registered + expected_cleanup,
            "source independent callback replay count"
        );
        for (r, v) in ticks.iter().zip(st) {
            assert_error(r, v, &host, "retained callback/cleanup");
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
            object(92, vec![]),
        ];
        let send = host.send_options.borrow();
        let receive = host.receive_options.borrow();
        let format = send
            .as_ref()
            .map(|o| o.format.clone())
            .or_else(|| receive.as_ref().map(|o| o.format.clone()));
        let expected_formats = if (family == 5 && !matches!(i, 4 | 5)) || (family == 6 && i != 5) {
            10
        } else {
            0
        };
        let formats = s["formats"].as_array().unwrap();
        assert_eq!(
            formats.len(),
            expected_formats,
            "source independent formatter replay count"
        );
        assert_eq!(
            usize::from(format.is_some()) * 10,
            expected_formats,
            "native independent formatter replay count"
        );
        if let Some(format) = format {
            for (j, v) in messages.into_iter().enumerate() {
                let native = format(v);
                let row = &formats[j];
                assert_eq!(row["index"], j);
                match native {
                    Ok(text) => {
                        assert!(row["error"].is_null(), "native formatter expected success");
                        assert_eq!(
                            text,
                            source_units(&row["text"]),
                            "native formatter UTF-16 {name}/{i}/{j}"
                        );
                    }
                    Err(e) => {
                        assert!(row["text"].is_null(), "native formatter expected failure");
                        assert_error(&Err(e), &row["error"], &host, "formatter");
                    }
                }
            }
        }
        let allowed = s["allowed"].as_array().unwrap();
        let expected_allowed = if family == 6 && i != 5 { 3 } else { 0 };
        assert_eq!(
            allowed.len(),
            expected_allowed,
            "source independent predicate replay count"
        );
        assert_eq!(
            usize::from(receive.is_some()) * 3,
            expected_allowed,
            "native independent predicate replay count"
        );
        if let Some(receive) = receive.as_ref() {
            let initial = host.channel.borrow().clone();
            for (j, next) in [initial, text("other"), text("push.depth.full")]
                .into_iter()
                .enumerate()
            {
                *host.channel.borrow_mut() = next;
                let r = (receive.allow_channel)();
                match r {
                    Ok(v) => {
                        assert!(allowed[j]["error"].is_null(), "native predicate success");
                        assert_eq!(
                            v,
                            allowed[j]["value"].as_bool().unwrap(),
                            "native predicate value"
                        );
                    }
                    Err(e) => {
                        assert!(allowed[j]["value"].is_null());
                        assert_error(&Err(e), &allowed[j]["error"], &host, "predicate");
                    }
                }
            }
        }
        assert_trace(&host, s);
        assert_map(&host, "regular", s);
        assert_map(&host, "ticker", s);
    }
    assert_eq!(
        source["cases"][family * 12 + 11]["index"],
        11,
        "enumeration stopped at fixture 11 for each family"
    );
}
#[test]
fn connect_native() {
    run(0)
}
#[test]
fn candle_native() {
    run(1)
}
#[test]
fn active_native() {
    run(2)
}
#[test]
fn ticker_native() {
    run(3)
}
#[test]
fn resubscribe_native() {
    run(4)
}
#[test]
fn send_diagnostics_native() {
    run(5)
}
#[test]
fn receive_diagnostics_native() {
    run(6)
}
#[test]
fn route_native() {
    run(7)
}
