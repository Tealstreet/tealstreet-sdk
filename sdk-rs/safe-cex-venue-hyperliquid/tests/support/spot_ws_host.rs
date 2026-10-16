use super::{
    cache::{self, N, arr, eq, object, s},
    native::{decode, error},
};
use safe_cex_venue_hyperliquid::{
    json_boundaries::JsPayloadHost,
    spot_json::JsString,
    spot_ws_dispatch::*,
    ws_emission::{DiagnosticScope, WsEmissionHost},
    ws_ping_lifecycle::{PingCallback, WsPingHost},
    ws_transport_leaves::{WsTransportHost, WsVenue},
};
use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};
use std::{cell::RefCell, collections::HashMap, fmt, rc::Rc};
pub fn get(v: &N, k: &str) -> N {
    JsPayloadHost::get(&mut cache::Host::default(), v, k).unwrap_or(N::U)
}
pub fn flag(v: &N, k: &str) -> bool {
    matches!(get(v, k), N::B(true))
}
pub fn str_value(v: &N) -> String {
    match v {
        N::S(s) => String::from_utf16_lossy(&s.0),
        N::U => "undefined".into(),
        N::Null => "null".into(),
        N::B(b) => b.to_string(),
        N::Num(n) => {
            if *n == 0. {
                "0".into()
            } else {
                n.to_string()
            }
        }
        N::A(a) => a
            .borrow()
            .iter()
            .map(|v| {
                if matches!(v, N::Null | N::U) {
                    String::new()
                } else {
                    str_value(v)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        _ => "[object Object]".into(),
    }
}
fn same(a: &N, b: &N) -> bool {
    eq(a, b) || matches!((a,b),(N::Builtin(a),N::Builtin(b)) if a==b)
}
fn freeze(v: &N) -> N {
    match v {
        N::A(a) => arr(a.borrow().iter().map(freeze).collect()),
        N::O(o) => {
            let r = object(vec![]);
            if let N::O(r) = &r {
                r.borrow_mut().fields = o
                    .borrow()
                    .fields
                    .iter()
                    .map(|(k, v, e)| (k.clone(), freeze(v), *e))
                    .collect();
            }
            r
        }
        _ => v.clone(),
    }
}
struct Parsed(N);
impl<'de> Deserialize<'de> for Parsed {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Parsed;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON native value")
            }
            fn visit_unit<E>(self) -> Result<Parsed, E> {
                Ok(Parsed(N::Null))
            }
            fn visit_bool<E>(self, v: bool) -> Result<Parsed, E> {
                Ok(Parsed(N::B(v)))
            }
            fn visit_f64<E>(self, v: f64) -> Result<Parsed, E> {
                Ok(Parsed(N::Num(v)))
            }
            fn visit_i64<E>(self, v: i64) -> Result<Parsed, E> {
                Ok(Parsed(N::Num(v as f64)))
            }
            fn visit_u64<E>(self, v: u64) -> Result<Parsed, E> {
                Ok(Parsed(N::Num(v as f64)))
            }
            fn visit_str<E>(self, v: &str) -> Result<Parsed, E> {
                Ok(Parsed(s(v)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Parsed, A::Error> {
                let mut v = vec![];
                while let Some(p) = a.next_element::<Parsed>()? {
                    v.push(p.0)
                }
                Ok(Parsed(arr(v)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Parsed, A::Error> {
                let v = object(vec![]);
                let N::O(o) = &v else { unreachable!() };
                while let Some((k, p)) = a.next_entry::<String, Parsed>()? {
                    o.borrow_mut().fields.push((k, p.0, true))
                }
                Ok(Parsed(v))
            }
        }
        d.deserialize_any(V)
    }
}
pub struct Host {
    pub input: N,
    pub effects: Vec<N>,
    pub parsed: N,
    pub parent: N,
    pub positions: N,
    pub tickers: N,
    pub buffer: N,
    pub symbol: N,
    pub handlers: Vec<N>,
    pub arrows: HashMap<String, (N, String, u8, PrivateMethod)>,
    pub classes: Vec<(N, N, bool)>,
    pub base_ctor: N,
    pub class_links: Vec<(N, N)>,
    pub class_metadata: Vec<(N, N)>,
    pub channel_reads: usize,
    pub poison: Option<N>,
    pub base_proto: N,
    pub tables: N,
    pub systems: N,
    pub next: usize,
}
impl Host {
    pub fn new(input: N, tables: N, systems: N) -> Self {
        let symbol = if flag(&input, "opaqueSymbol") {
            object(vec![("symbol", N::B(true))])
        } else {
            s("BTC-USDC")
        };
        let positions = arr(vec![
            object(vec![
                ("symbol", s("BTC-USDC")),
                ("contracts", N::Num(2.)),
                ("marginType", s("isolated")),
                ("leverage", N::Num(7.)),
            ]),
            object(vec![("symbol", s("OTHER")), ("contracts", N::Num(0.))]),
            object(vec![
                ("symbol", s("BTC-USDC")),
                ("contracts", N::Num(0.)),
                ("marginType", s("isolated")),
                ("leverage", N::Num(1.)),
            ]),
        ]);
        let tickers = object(vec![
            ("BTC", object(vec![("id", s("BTC"))])),
            ("ETH", object(vec![("id", s("ETH"))])),
        ]);
        let parent = object(vec![
            ("token", object(vec![])),
            (
                "options",
                object(vec![
                    ("testnet", N::B(false)),
                    ("vault", s("vault")),
                    ("applicationId", s("app")),
                ]),
            ),
            ("activeAssetData", N::Map(Rc::new(RefCell::new(vec![])))),
            (
                "store",
                object(vec![
                    ("markets", arr(vec![object(vec![("id", s("BTC"))])])),
                    (
                        "marketsById",
                        object(vec![("BTC", object(vec![("symbol", symbol.clone())]))]),
                    ),
                    ("positions", positions.clone()),
                    ("tickersById", tickers.clone()),
                    ("updateTicker", N::Builtin("ticker".into())),
                    ("updatePosition", N::Builtin("position".into())),
                ]),
            ),
            (
                "_getSymbolConfigurationFromAssetData",
                N::Builtin("config".into()),
            ),
            ("applySpotBalances", N::Builtin("balances".into())),
            ("requestSpotBalanceRefresh", N::Builtin("refresh".into())),
            ("emitter", object(vec![("emit", N::Builtin("emit".into()))])),
        ]);
        let base_proto = object(
            [
                "trackLastReceived",
                "recordMessageHandlerStats",
                "handlePongEvent",
                "createWebsocket",
                "onClose",
                "send",
            ]
            .iter()
            .map(|k| (*k, N::Builtin((*k).into())))
            .collect(),
        );
        Self {
            input,
            effects: vec![],
            parsed: N::U,
            parent,
            positions,
            tickers,
            buffer: N::U,
            symbol,
            handlers: vec![],
            arrows: HashMap::new(),
            classes: vec![],
            base_ctor: N::Class(Rc::new(())),
            class_links: vec![],
            class_metadata: vec![],
            channel_reads: 0,
            poison: None,
            base_proto,
            tables,
            systems,
            next: 0,
        }
    }
    pub fn record(&mut self, label: &str, args: &[N]) {
        let mut a = vec![s(label)];
        a.extend(args.iter().map(freeze));
        self.effects.push(arr(a));
    }
    pub fn invoke(&mut self, o: &N, k: &str, args: &[N]) -> Result<N, N> {
        let f = self.get(o, k)?;
        self.call(&f, o, args)
    }
    pub fn public_socket(&mut self) -> N {
        let parent = self.parent.clone();
        let socket = self.base_instance(&parent, &self.base_proto.clone());
        self.buffer = object(vec![("last", N::Num(0.)), ("midPrice", N::Num(0.))]);
        self.set(&socket, "_midsUpdateBuf", &self.buffer.clone())
            .unwrap();
        self.set(&socket, "handleAllMids", &N::Builtin("mids".into()))
            .unwrap();
        let handlers = object(vec![]);
        for key in ["candle.BTC.1m", "l2Book.BTC"] {
            let a = arr(vec![
                N::Builtin("handler0".into()),
                N::Builtin("handler1".into()),
            ]);
            self.handlers.push(a.clone());
            self.set(&handlers, key, &a).unwrap()
        }
        self.set(&socket, "messageHandlers", &handlers).unwrap();
        socket
    }
    fn base_instance(&mut self, parent: &N, prototype: &N) -> N {
        let v = object(vec![
            ("parent", parent.clone()),
            ("isDisposed", N::B(flag(&self.input, "disposed"))),
            ("baseToken", get(parent, "token")),
        ]);
        let N::O(o) = &v else { unreachable!() };
        o.borrow_mut().prototype = Some(prototype.clone());
        v
    }
    pub fn custom_target(&mut self) -> N {
        let c = N::Class(Rc::new(()));
        self.classes.push((c.clone(), object(vec![]), true));
        c
    }
    pub fn class_summary(&self, class: &PrivateSocketClass<N>, socket: &N, other: &N) -> N {
        let N::O(proto) = &class.prototype else {
            panic!("prototype")
        };
        let names = own_keys(socket)
            .into_iter()
            .skip(3)
            .filter(|k| matches!(get(socket,k),N::Builtin(id) if self.arrows.contains_key(&id)))
            .collect::<Vec<_>>();
        let metadata = &self
            .class_metadata
            .iter()
            .find(|(c, _)| same(c, &class.constructor))
            .unwrap()
            .1;
        object(vec![
            ("name", get(metadata, "name")),
            ("length", get(metadata, "length")),
            (
                "base",
                N::B(
                    self.class_links.iter().any(|(c, base)| {
                        same(c, &class.constructor) && same(base, &self.base_ctor)
                    }),
                ),
            ),
            (
                "prototypeConstructor",
                N::B(same(
                    &get(&class.prototype, "constructor"),
                    &class.constructor,
                )),
            ),
            (
                "prototypeKeys",
                arr(proto.borrow().fields.iter().map(|p| s(&p.0)).collect()),
            ),
            (
                "constructorEnumerable",
                N::B(
                    proto
                        .borrow()
                        .fields
                        .iter()
                        .find(|p| p.0 == "constructor")
                        .unwrap()
                        .2,
                ),
            ),
            (
                "shared",
                N::B(match (socket, other) {
                    (N::O(a), N::O(b)) => same(
                        a.borrow().prototype.as_ref().unwrap(),
                        b.borrow().prototype.as_ref().unwrap(),
                    ),
                    _ => false,
                }),
            ),
            (
                "distinct",
                N::B(names.iter().all(|k| !same(&get(socket, k), &get(other, k)))),
            ),
            (
                "arrows",
                arr(names
                    .iter()
                    .map(|k| self.arrow_summary(socket, k, false))
                    .collect()),
            ),
        ])
    }
    pub fn constructor_summary(&self, class: &PrivateSocketClass<N>, socket: &N, target: &N) -> N {
        let proto = self
            .classes
            .iter()
            .find(|(c, _, _)| same(c, target))
            .map(|(_, p, _)| p)
            .unwrap_or(&class.prototype);
        object(vec![
            ("parent", N::B(same(&get(socket, "parent"), &self.parent))),
            (
                "token",
                N::B(same(&get(socket, "baseToken"), &get(&self.parent, "token"))),
            ),
            (
                "prototype",
                N::B(match socket {
                    N::O(o) => same(o.borrow().prototype.as_ref().unwrap(), proto),
                    _ => false,
                }),
            ),
            ("keys", arr(own_keys(socket).iter().map(|k| s(k)).collect())),
            (
                "arrows",
                arr(own_keys(socket)
                    .iter()
                    .skip(3)
                    .map(|k| self.arrow_summary(socket, k, true))
                    .collect()),
            ),
        ])
    }
    fn arrow_summary(&self, socket: &N, k: &str, descriptor: bool) -> N {
        let f = get(socket, k);
        let N::Builtin(id) = f else {
            return arr(vec![s(k), s("object"), N::U, N::U]);
        };
        let (_, name, length, _) = self.arrows.get(&id).unwrap();
        let mut a = vec![s(k), s("function"), s(name), N::Num(*length as f64)];
        if descriptor {
            let N::O(o) = socket else { unreachable!() };
            a.push(N::B(o.borrow().fields.iter().find(|p| p.0 == k).unwrap().2))
        }
        arr(a)
    }
}
fn own_keys(v: &N) -> Vec<String> {
    let N::O(o) = v else { panic!("object") };
    o.borrow()
        .fields
        .iter()
        .filter(|p| p.2)
        .map(|p| p.0.clone())
        .collect()
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        N::U
    }
    fn literal(&self, t: &JsString) -> N {
        N::S(t.clone())
    }
    fn truthy(&self, v: &N) -> bool {
        !matches!(v, N::U | N::Null | N::B(false))
            && !matches!(v,N::Num(n) if *n==0.||n.is_nan())
            && !matches!(v,N::S(t) if t.0.is_empty())
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        Ok(object(vec![if k == "performance" {
            ("now", N::Builtin("now".into()))
        } else {
            ("stringify", N::Builtin("stringify".into()))
        }]))
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if self.poison.as_ref().is_some_and(|p| same(p, v)) {
            return Err(s("poison"));
        }
        let sequence = get(&self.input, "channelSequence");
        if k == "channel" && same(v, &self.parsed) && matches!(sequence, N::A(_)) {
            let value = get(&sequence, &self.channel_reads.to_string());
            self.channel_reads += 1;
            self.record("channel-read", &[value.clone()]);
            return Ok(value);
        }
        if k == "store" && matches!(v, N::O(_)) {
            let parent = get(v, "parent");
            if !matches!(parent, N::U) {
                return Ok(get(&parent, "store"));
            }
        }
        if matches!(v, N::U | N::Null) {
            return Err(error(
                "TypeError",
                &format!(
                    "Cannot read properties of {} (reading '{k}')",
                    if matches!(v, N::U) {
                        "undefined"
                    } else {
                        "null"
                    }
                ),
            ));
        }
        JsPayloadHost::get(&mut cache::Host::default(), v, k).map_err(|e| s(&e))
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        cache::Host::default()
            .put(v, k, value, true)
            .map_err(|e| s(&e))
    }
    fn define_data(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        self.set(v, k, value)
    }
    fn object(&mut self) -> Result<N, N> {
        Ok(object(vec![]))
    }
    fn callback(&mut self, _: PingCallback<N>) -> Result<N, N> {
        panic!("unused perp ping")
    }
    fn call(&mut self, f: &N, receiver: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("unexpected callee {f:?}")
        };
        if let Some((capture, _, _, body)) = self.arrows.get(id).cloned() {
            return run_private_callback(self, &capture, body, args);
        }
        let first = args.first().cloned().unwrap_or(N::U);
        match id.as_str() {
            "trackLastReceived" => {
                self.record("track", &[]);
                if flag(&self.input, "trackThrow") {
                    return Err(N::U);
                }
            }
            "recordMessageHandlerStats" => {
                self.record("stats", args);
                if flag(&self.input, "statsThrow") {
                    return Err(s("stats"));
                }
            }
            "handlePongEvent" => self.record("pong", &[]),
            "createWebsocket" => {
                self.record("create", args);
                return Ok(object(vec![
                    ("addEventListener", N::Builtin("listen".into())),
                    ("owner", receiver.clone()),
                ]));
            }
            "listen" => {
                let name = str_value(&first);
                let socket = get(receiver, "owner");
                let k = match name.as_str() {
                    "open" => "onOpen",
                    "message" => "onMessage",
                    _ => "onClose",
                };
                self.record("listen", &[first, N::B(same(&args[1], &get(&socket, k)))])
            }
            "send" => {
                let p = serde_json::from_str::<Parsed>(&str_value(&first))
                    .unwrap()
                    .0;
                let _validated = p;
                self.record("send", &[])
            }
            "now" => return Ok(N::Num(123.5)),
            "stringify" => {
                self.record("serialize", args);
                let mut base = cache::Host::default();
                return Ok(N::S(base.json_stringify(&first).unwrap().unwrap()));
            }
            "balances" => {
                let data = get(&self.input, "data");
                let mut expected = get(&data, "balances");
                if matches!(expected, N::U | N::Null) {
                    let parsed_data = get(&self.parsed, "data");
                    expected = get(&get(&parsed_data, "spotState"), "balances");
                    if matches!(expected, N::U | N::Null) {
                        expected = get(&parsed_data, "balances")
                    }
                }
                self.record("balances", &[first.clone(), N::B(same(&first, &expected))]);
                if flag(&self.input, "balancesThrow") {
                    return Err(s("balances"));
                }
            }
            "refresh" => {
                self.record("refresh", args);
                if flag(&self.input, "refreshThrow") {
                    return Err(s("refresh"));
                }
            }
            "emit" => {
                let r = &args[1];
                self.record(
                    "emit",
                    &[
                        first,
                        r.clone(),
                        N::B(same(&get(r, "symbol"), &self.symbol)),
                    ],
                );
                if flag(&self.input, "emitThrow") {
                    return Err(s("emit"));
                }
                if flag(&self.input, "appendFill") {
                    let fills = get(&get(&self.input, "data"), "fills");
                    let N::A(a) = fills else { panic!("fills") };
                    if a.borrow().len() == 1 {
                        a.borrow_mut().push(object(vec![
                            ("tid", N::Num(18.)),
                            ("side", s("B")),
                            ("coin", s("BTC")),
                            ("px", s("123.5tail")),
                            ("sz", s("2.25x")),
                        ]))
                    }
                }
            }
            "set" => {
                self.record(
                    "asset",
                    &[
                        first,
                        args[1].clone(),
                        N::B(same(&args[1], &get(&self.parsed, "data"))),
                    ],
                );
                let N::Map(m) = receiver else { panic!("map") };
                m.borrow_mut().push((args[0].clone(), args[1].clone()));
                return Ok(receiver.clone());
            }
            "config" => {
                self.record("config", args);
                return Ok(object(vec![
                    ("positionMode", s("oneway")),
                    ("marginType", s("cross")),
                    ("leverage", N::Num(3.)),
                ]));
            }
            "position" => {
                let N::A(a) = &self.positions else {
                    unreachable!()
                };
                let original = a.borrow().iter().any(|p| same(p, &first));
                self.record("position", &[first, args[1].clone(), N::B(original)])
            }
            "ticker" | "ticker-new" => {
                let original = same(&first, &get(&self.tickers, &str_value(&get(&first, "id"))));
                self.record(
                    id.as_str(),
                    &[
                        first,
                        args[1].clone(),
                        N::B(original),
                        N::B(same(&args[1], &self.buffer)),
                        N::B(same(receiver, &get(&self.parent, "store"))),
                    ],
                );
                if flag(&self.input, "mutateTicker") {
                    self.set(&args[1], "marker", &N::B(true))?;
                    let old = get(&self.parent, "store");
                    let new = freeze(&old);
                    self.set(&new, "updateTicker", &N::Builtin("ticker-new".into()))?;
                    self.set(&self.parent.clone(), "store", &new)?;
                }
            }
            "mids" => handle_all_mids(self, receiver, &first)?,
            "handler0" | "handler1" | "handler2" => {
                let n = id.trim_start_matches("handler").parse::<usize>().unwrap();
                let receiver_ok = self.handlers.iter().any(|a| same(a, receiver));
                self.record(
                    "handler",
                    &[
                        N::Num(n as f64),
                        N::B(receiver_ok),
                        N::B(same(&first, &get(&self.parsed, "data"))),
                        first,
                    ],
                );
                if n == 0 && flag(&self.input, "appendHandler") {
                    let N::A(a) = receiver else {
                        panic!("handler receiver")
                    };
                    a.borrow_mut().push(N::Builtin("handler2".into()))
                }
                if n == 0 && flag(&self.input, "handlerThrow") {
                    return Err(N::U);
                }
            }
            _ => panic!("unbound {id}"),
        };
        Ok(N::U)
    }
}
impl WsTransportHost for Host {
    fn urls_binding(&mut self, v: WsVenue) -> Result<N, N> {
        assert_eq!(v, WsVenue::Spot);
        Ok(self.tables.clone())
    }
    fn boolean(&self, b: bool) -> N {
        N::B(b)
    }
}
impl WsEmissionHost for Host {
    fn boolean(&self, b: bool) -> N {
        N::B(b)
    }
    fn system_types(&mut self, v: WsVenue) -> Result<N, N> {
        assert_eq!(v, WsVenue::Spot);
        Ok(self.systems.clone())
    }
    fn iterator(&mut self, v: &N) -> Result<N, N> {
        let N::A(a) = v else { panic!("iterable") };
        Ok(N::Iter(Rc::new(RefCell::new((a.borrow().clone(), 0)))))
    }
    fn next(&mut self, v: &N) -> Result<Option<N>, N> {
        let N::Iter(i) = v else { panic!("iterator") };
        let mut i = i.borrow_mut();
        let v = i.0.get(i.1).cloned();
        i.1 += 1;
        Ok(v)
    }
    fn close_after_throw(&mut self, _: &N) {}
    fn triple(&mut self, _: &N) -> Result<[N; 3], N> {
        panic!("unused triple")
    }
    fn string_units<'a>(&self, v: &'a N) -> Option<&'a [u16]> {
        if let N::S(s) = v { Some(&s.0) } else { None }
    }
    fn spread_into(&mut self, _: &N, _: &N) -> Result<(), N> {
        panic!("unused spread")
    }
    fn formatter(&mut self, _: DiagnosticScope) -> Result<N, N> {
        panic!("unused formatter")
    }
}
impl SpotWsHost for Host {
    fn event_data(&mut self, event: &N) -> Result<N, N> {
        if matches!(event, N::U | N::Null) {
            return Err(error(
                "TypeError",
                if matches!(event, N::U) {
                    "Cannot destructure property 'data' of 'undefined' as it is undefined."
                } else {
                    "Cannot destructure property 'data' of 'object null' as it is null."
                },
            ));
        }
        if flag(&self.input, "eventThrow") {
            return Err(s("event"));
        }
        if flag(&self.input, "eventGetter") {
            self.record("event-data", &[])
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
    fn greater(&mut self, a: &N, b: &N) -> Result<bool, N> {
        Ok(cache::num(a) > cache::num(b))
    }
    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        let k = str_value(k);
        if flag(&self.input, "unknownPoison")
            && k == "UNKNOWN"
            && same(v, &get(&self.input, "data"))
        {
            return Err(s("unknown read"));
        }
        self.get(v, &k)
    }
    fn array(&mut self) -> Result<N, N> {
        Ok(arr(vec![]))
    }
    fn parse_float(&mut self, v: &N) -> Result<f64, N> {
        let t = str_value(v);
        let t = t.trim_start();
        if t.starts_with("Infinity") {
            return Ok(f64::INFINITY);
        }
        if t.starts_with("-Infinity") {
            return Ok(f64::NEG_INFINITY);
        }
        for end in (1..=t.len()).rev().filter(|end| t.is_char_boundary(*end)) {
            if let Ok(n) = t[..end].parse() {
                return Ok(n);
            }
        }
        Ok(f64::NAN)
    }
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        Ok(JsString(str_value(v).encode_utf16().collect()))
    }
    fn json_parse(&mut self, v: &N) -> Result<N, N> {
        self.parsed = serde_json::from_str::<Parsed>(&str_value(v))
            .map(|v| v.0)
            .unwrap_or(N::Null);
        Ok(self.parsed.clone())
    }
    fn order_side(&mut self, b: bool) -> Result<N, N> {
        Ok(s(if b { "buy" } else { "sell" }))
    }
    fn for_in(&mut self, v: &N) -> Result<N, N> {
        let mut keys = vec![];
        let mut current = v.clone();
        while let N::O(o) = current {
            let o = o.borrow();
            for (k, _, e) in &o.fields {
                if *e && !keys.contains(k) {
                    keys.push(k.clone())
                }
            }
            current = o.prototype.clone().unwrap_or(N::U)
        }
        Ok(N::Iter(Rc::new(RefCell::new((
            keys.iter().map(|k| s(k)).collect(),
            0,
        )))))
    }
    fn for_in_next(&mut self, i: &N) -> Result<Option<N>, N> {
        self.next(i)
    }
    fn private_class(&mut self) -> Result<(N, N), N> {
        let c = N::Class(Rc::new(()));
        let p = object(vec![]);
        cache::Host::default()
            .put(&p, "constructor", &c, false)
            .unwrap();
        let N::O(o) = &p else { unreachable!() };
        o.borrow_mut().prototype = Some(self.base_proto.clone());
        self.classes.push((c.clone(), p.clone(), false));
        self.class_links.push((c.clone(), self.base_ctor.clone()));
        self.class_metadata.push((
            c.clone(),
            object(vec![
                ("name", s("HyperliquidSpotPrivateWebSocket")),
                ("length", N::Num(1.)),
            ]),
        ));
        Ok((c, p))
    }
    fn construct_base(&mut self, parent: &N, target: &N, prototype: &N) -> Result<N, N> {
        if flag(&self.input, "prototypeThrow") {
            return Err(s("prototype"));
        }
        let proto = self
            .classes
            .iter()
            .find(|(c, _, _)| same(c, target))
            .map(|(_, p, _)| p.clone())
            .unwrap_or_else(|| prototype.clone());
        if flag(&self.input, "baseThrow") {
            return Err(s("base"));
        }
        Ok(self.base_instance(parent, &proto))
    }
    fn arrow(&mut self, capture: &N, name: &str, length: u8, body: PrivateMethod) -> Result<N, N> {
        let id = format!("arrow{}", self.next);
        self.next += 1;
        self.arrows
            .insert(id.clone(), (capture.clone(), name.into(), length, body));
        Ok(N::Builtin(id))
    }
}
pub fn input(row: &serde_json::Value) -> N {
    decode(&row["input"])
}
