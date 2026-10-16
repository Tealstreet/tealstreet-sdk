#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::*;
use safe_cex_venue_kucoin::spot_public_class::define_spot_public_class;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
use values::*;
const ARROWS: [(&str, usize, bool); 11] = [
    ("connectAndSubscribe", 0, true),
    ("onOpen", 0, false),
    ("ping", 0, false),
    ("subscribe", 0, false),
    ("subscribeTopic", 1, false),
    ("unsubscribeTopic", 1, false),
    ("onMessage", 1, false),
    ("handleTickerSnapshot", 1, false),
    ("listenOHLCV", 2, false),
    ("listenOrderBook", 2, false),
    ("getCandleHandlerTopic", 1, false),
];
const FIELDS: [&str; 14] = [
    "token",
    "socketEndpoint",
    "pingInterval",
    "connectAndSubscribe",
    "onOpen",
    "ping",
    "subscribe",
    "subscribeTopic",
    "unsubscribeTopic",
    "onMessage",
    "handleTickerSnapshot",
    "listenOHLCV",
    "listenOrderBook",
    "getCandleHandlerTopic",
];
fn keys(v: &V) -> Vec<String> {
    let Js::O(fields) = v.as_ref() else {
        panic!("native object")
    };
    fields.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn rows(v: &V) -> Vec<V> {
    let Js::A(values) = v.as_ref() else {
        panic!("native array")
    };
    values.borrow().clone()
}
fn at(v: &V, key: &str) -> V {
    get(v, key).unwrap()
}
fn assert_number(v: &V, want: f64) {
    assert!(
        matches!(v.as_ref(), Js::N(_)),
        "KUCOIN-PUBLIC native number kind"
    );
    assert_eq!(
        number(v).to_bits(),
        want.to_bits(),
        "KUCOIN-PUBLIC native number bits"
    )
}
fn assert_text(v: &V, want: &str) {
    assert!(
        matches!(v.as_ref(), Js::S(_)),
        "KUCOIN-PUBLIC native string kind"
    );
    assert_eq!(text(v), want, "KUCOIN-PUBLIC native string")
}
struct Fixture {
    rt: Rc<Runtime>,
    class: V,
    r: V,
    parent: V,
    fields: Vec<String>,
    post: V,
    frames: Rc<RefCell<Vec<V>>>,
    listeners: Rc<RefCell<Vec<(String, V)>>>,
    parsed: Rc<RefCell<Vec<V>>>,
    updates: Rc<RefCell<Vec<V>>>,
}
impl Fixture {
    fn new() -> Self {
        let rt = Rc::new(Runtime::default());
        *rt.expected_class.borrow_mut() = "KucoinSpotPublicWebsocket".into();
        rt.globals.borrow_mut().insert(
            "BaseWebSocket".into(),
            obj(vec![("prototype", obj(vec![]))]),
        );
        rt.globals
            .borrow_mut()
            .insert("ENABLE_RECONNECTING_WEBSOCKET".into(), b(true));
        rt.globals.borrow_mut().insert(
            "Date".into(),
            obj(vec![("now", function(|_, _| Ok(n(41.))))]),
        );
        rt.globals.borrow_mut().insert(
            "performance".into(),
            obj(vec![("now", function(|_, _| Ok(n(79.))))]),
        );
        let stringify_rt = rt.clone();
        rt.globals.borrow_mut().insert(
            "JSON".into(),
            obj(vec![(
                "stringify",
                function(move |_, a| {
                    stringify_rt.stringified.borrow_mut().push(a[0].clone());
                    Ok(s(&stringify(&a[0])))
                }),
            )]),
        );
        rt.globals.borrow_mut().insert(
            "Object".into(),
            obj(vec![(
                "keys",
                function(|_, a| Ok(arr(keys(&a[0]).iter().map(|k| s(k)).collect()))),
            )]),
        );
        let parsed = Rc::new(RefCell::new(Vec::new()));
        let capture = parsed.clone();
        rt.globals.borrow_mut().insert(
            "jsonParse".into(),
            function(move |_, a| {
                let v = serde_json::from_str::<Value>(&text(&a[0]))
                    .map(|v| decode(&v))
                    .unwrap_or_else(|_| Rc::new(Js::Null));
                capture.borrow_mut().push(v.clone());
                Ok(v)
            }),
        );
        for (name, table) in [
            (
                "KUCOIN_SPOT_ENDPOINTS",
                safe_cex_venue_kucoin::spot_types::KUCOIN_SPOT_ENDPOINTS,
            ),
            (
                "KUCOIN_SPOT_INTERVAL",
                safe_cex_venue_kucoin::spot_types::KUCOIN_SPOT_INTERVAL,
            ),
        ] {
            rt.globals.borrow_mut().insert(
                name.into(),
                obj(table.iter().map(|(k, v)| (*k, s(v))).collect()),
            );
        }
        rt.globals.borrow_mut().insert(
            "parseFloat".into(),
            function(|_, a| Ok(n(safe_cex_num::coercion::parse_float(&text(&a[0]))))),
        );
        rt.globals.borrow_mut().insert(
            "parseNumber".into(),
            function(|_, a| {
                Ok(
                    safe_cex_venue_kucoin::ws_helpers::parse_number(Some(&literal(&a[0])))
                        .unwrap()
                        .map(n)
                        .unwrap_or_else(u),
                )
            }),
        );
        rt.globals.borrow_mut().insert(
            "normalizeTimestamp".into(),
            function(|_, a| {
                Ok(n(safe_cex_venue_kucoin::ws_helpers::normalize_timestamp(
                    Some(&literal(&a[0])),
                    41.,
                )
                .unwrap()))
            }),
        );
        rt.globals.borrow_mut().insert(
            "sortOrderBook".into(),
            function(|_, a| {
                for (side, reverse) in [("bids", true), ("asks", false)] {
                    let value = at(&a[0], side);
                    let Js::A(rows) = value.as_ref() else {
                        panic!("book rows")
                    };
                    rows.borrow_mut().sort_by(|a, b| {
                        let order = number(&at(a, "price"))
                            .partial_cmp(&number(&at(b, "price")))
                            .unwrap();
                        if reverse { order.reverse() } else { order }
                    });
                }
                Ok(u())
            }),
        );
        rt.globals.borrow_mut().insert(
            "calcOrderBookTotal".into(),
            function(|_, a| {
                for side in ["bids", "asks"] {
                    let mut total = 0.;
                    for row in rows(&at(&a[0], side)) {
                        total += number(&at(&row, "amount"));
                        set(&row, "total", n(total));
                    }
                }
                Ok(u())
            }),
        );
        let post = pending();
        let promise = post.clone();
        let post_rt = rt.clone();
        let ticker = obj(vec![("id", s("native-ticker"))]);
        let expected = ticker.clone();
        let updates = Rc::new(RefCell::new(Vec::new()));
        let changed = updates.clone();
        let error_rt = rt.clone();
        let parent = obj(vec![
            (
                "xhr",
                obj(vec![(
                    "post",
                    function(move |_, a| {
                        post_rt
                            .trace
                            .borrow_mut()
                            .push(format!("post:{}", text(&a[0])));
                        Ok(promise.clone())
                    }),
                )]),
            ),
            (
                "store",
                obj(vec![
                    ("tickersBySymbol", obj(vec![("BTC-USDT", ticker)])),
                    (
                        "updateTicker",
                        function(move |_, a| {
                            assert!(
                                Rc::ptr_eq(&a[0], &expected),
                                "KUCOIN-PUBLIC original ticker identity"
                            );
                            changed.borrow_mut().push(a[1].clone());
                            Ok(u())
                        }),
                    ),
                ]),
            ),
            (
                "emitAndLogError",
                function(move |_, a| {
                    error_rt
                        .trace
                        .borrow_mut()
                        .push(format!("error:{}", text(&a[0])));
                    Ok(u())
                }),
            ),
        ]);
        let class = define_spot_public_class(rt.operations()).unwrap();
        let r = rt.construct(&class, parent.clone());
        let fields = keys(&r)[2..].to_vec();
        set(&r, "isConnected", b(true));
        set(&r, "messageHandlers", obj(vec![]));
        let frames = Rc::new(RefCell::new(Vec::new()));
        let sent = frames.clone();
        set(
            &r,
            "send",
            function(move |_, a| {
                sent.borrow_mut().extend(a);
                Ok(u())
            }),
        );
        let listeners = Rc::new(RefCell::new(Vec::new()));
        let installed = listeners.clone();
        let socket_rt = rt.clone();
        set(
            &r,
            "createWebsocket",
            function(move |_, a| {
                socket_rt
                    .trace
                    .borrow_mut()
                    .push(format!("socket:{}", text(&a[0])));
                let installed = installed.clone();
                Ok(obj(vec![(
                    "addEventListener",
                    function(move |_, a| {
                        installed.borrow_mut().push((text(&a[0]), a[1].clone()));
                        Ok(u())
                    }),
                )]))
            }),
        );
        set(&r, "onClose", function(|_, _| Ok(u())));
        for key in [
            "trackLastReceived",
            "handlePongEvent",
            "recordMessageHandlerStats",
            "cleanupMessageHandlerStats",
            "retryFailedConnect",
            "emitWsReconnected",
        ] {
            let rt = rt.clone();
            set(
                &r,
                key,
                function(move |_, a| {
                    rt.trace.borrow_mut().push(if a.is_empty() {
                        key.into()
                    } else {
                        format!("{key}:{}", text(&a[0]))
                    });
                    Ok(u())
                }),
            );
        }
        Self {
            rt,
            class,
            r,
            parent,
            fields,
            post,
            frames,
            listeners,
            parsed,
            updates,
        }
    }
    fn call(&self, key: &str, args: Vec<V>) -> V {
        call(at(&self.r, key), obj(vec![("isDisposed", b(true))]), args).unwrap()
    }
    fn message(&self, data: &str) {
        self.rt
            .method(&self.r, "onMessage", vec![obj(vec![("data", s(data))])])
            .unwrap();
    }
    fn frame_values(&self) -> Vec<V> {
        self.rt.stringified.borrow().clone()
    }
}
fn assert_frames(f: &Fixture, expected: &[(&str, Option<&str>)]) {
    let frames = f.frame_values();
    assert_eq!(
        frames.len(),
        expected.len(),
        "KUCOIN-PUBLIC independent frame count"
    );
    for (frame, (kind, topic)) in frames.iter().zip(expected) {
        assert_eq!(
            keys(frame),
            if topic.is_some() {
                vec!["id", "type", "topic", "privateChannel", "response"]
            } else {
                vec!["id", "type"]
            }
        );
        assert_number(&at(frame, "id"), 41.);
        assert_text(&at(frame, "type"), kind);
        if let Some(topic) = topic {
            assert_text(&at(frame, "topic"), topic);
            assert!(matches!(at(frame, "privateChannel").as_ref(), Js::B(false)));
            assert!(matches!(at(frame, "response").as_ref(), Js::B(true)));
        }
    }
}
fn layout() -> Value {
    let f = Fixture::new();
    assert!(f.rt.timers.borrow().is_empty());
    assert_eq!(f.rt.next_timer.get(), 0);
    assert_eq!(f.fields, FIELDS, "KUCOIN-PUBLIC fourteen ordered fields");
    let other = f.rt.construct(&f.class, f.parent.clone());
    assert_eq!(
        keys(&other)[2..],
        FIELDS,
        "KUCOIN-PUBLIC complete clean instance fields"
    );
    let definitions =
        f.rt.definitions
            .borrow()
            .iter()
            .filter(|(r, _)| Rc::ptr_eq(r, &other))
            .map(|(_, name)| name.clone())
            .collect::<Vec<_>>();
    assert_eq!(definitions, FIELDS, "KUCOIN-PUBLIC fourteen definitions");
    assert!(matches!(at(&other, "token").as_ref(), Js::Null));
    assert!(matches!(at(&other, "socketEndpoint").as_ref(), Js::Null));
    assert_number(&at(&other, "pingInterval"), 5000.);
    for (name, arity, asynchronous) in ARROWS {
        let first = at(&f.r, name);
        let second = at(&other, name);
        assert!(!Rc::ptr_eq(&first, &second), "KUCOIN-PUBLIC fresh arrow");
        let metadata =
            f.rt.metas
                .borrow()
                .iter()
                .find(|(v, _, _, _)| Rc::ptr_eq(v, &first))
                .cloned()
                .unwrap();
        assert_eq!(
            (&*metadata.1, metadata.2, metadata.3),
            (name, arity, asynchronous),
            "KUCOIN-PUBLIC native arrow metadata"
        );
    }
    let proto = f.rt.prototypes.borrow()[0].1.clone();
    assert!(
        keys(&proto).is_empty(),
        "KUCOIN-PUBLIC no prototype methods"
    );
    json!({"fields":f.fields,"arrows":ARROWS,"prototype":[],"interval":5000})
}
fn ingress() -> Box<dyn FnOnce() -> Value> {
    let f = Fixture::new();
    let returned = f.call("connectAndSubscribe", vec![]);
    assert!(matches!(returned.as_ref(), Js::P(_)));
    assert_eq!(*f.rt.trace.borrow(), ["post:/api/v1/bullet-public"]);
    assert!(matches!(at(&f.r, "token").as_ref(), Js::Null));
    resolve(
        &f.post,
        Ok(obj(vec![(
            "data",
            obj(vec![
                ("code", s("200000")),
                (
                    "data",
                    obj(vec![
                        ("token", s("token-23")),
                        (
                            "instanceServers",
                            arr(vec![obj(vec![
                                ("endpoint", s("wss://socket.example")),
                                ("pingInterval", n(61.)),
                            ])]),
                        ),
                    ]),
                ),
            ]),
        )])),
    );
    f.rt.pump();
    assert_text(&at(&f.r, "token"), "token-23");
    assert_number(&at(&f.r, "pingInterval"), 61.);
    assert_text(
        &at(&f.r, "socketEndpoint"),
        "wss://socket.example?token=token-23&connectId=41",
    );
    assert_eq!(
        f.listeners.borrow().len(),
        3,
        "KUCOIN-PUBLIC three actual listeners"
    );
    for (event, field) in [
        ("open", "onOpen"),
        ("message", "onMessage"),
        ("close", "onClose"),
    ] {
        let callback = f
            .listeners
            .borrow()
            .iter()
            .find(|(name, _)| name == event)
            .unwrap()
            .1
            .clone();
        assert!(
            Rc::ptr_eq(&callback, &at(&f.r, field)),
            "KUCOIN-PUBLIC original listener handle"
        );
    }
    set(
        &f.r,
        "messageHandlers",
        obj(vec![
            ("candle_BTC-USDT_1min", arr(vec![])),
            ("orderBook_BTC-USDT", arr(vec![])),
            ("ignored", arr(vec![])),
        ]),
    );
    f.call("onOpen", vec![]);
    assert_frames(
        &f,
        &[
            ("subscribe", Some("/market/ticker:all")),
            ("subscribe", Some("/market/candles:BTC-USDT_1min")),
            ("subscribe", Some("/spotMarket/level2Depth50:BTC-USDT")),
            ("ping", None),
        ],
    );
    let live = Rc::new(Cell::new(0));
    let sent = live.clone();
    let r = f.r.clone();
    f.rt.setters.borrow_mut().push((
        f.r.clone(),
        "pingAt".into(),
        Rc::new(move |v| {
            assert_number(&v, 79.);
            let sent = sent.clone();
            set(
                &r,
                "send",
                function(move |_, _| {
                    sent.set(sent.get() + 1);
                    Ok(u())
                }),
            );
            Ok(())
        }),
    ));
    f.call("ping", vec![]);
    assert_eq!(live.get(), 1, "KUCOIN-PUBLIC late send after pingAt setter");
    set(&f.r, "isDisposed", b(true));
    let r = f.r.clone();
    let event = obj(vec![]);
    f.rt.getters.borrow_mut().push((
        event.clone(),
        "data".into(),
        Rc::new(move || {
            set(&r, "isDisposed", b(false));
            Ok(s("{\"type\":\"pong\"}"))
        }),
    ));
    f.call("onMessage", vec![event]);
    assert_eq!(f.rt.trace.borrow().last().unwrap(), "handlePongEvent:61");
    Box::new(
        move || json!({"url":text(&at(&f.r,"socketEndpoint")),"interval":61,"frames":f.frames.borrow().iter().map(|v|serde_json::from_str::<Value>(&text(v)).unwrap()).collect::<Vec<_>>(),"trace":*f.rt.trace.borrow(),"listeners":f.listeners.borrow().iter().map(|(name,_)|name.clone()).collect::<Vec<_>>(),"liveSend":live.get()}),
    )
}
fn streams() -> Box<dyn FnOnce() -> Value> {
    let f = Fixture::new();
    let candles = Rc::new(RefCell::new(Vec::new()));
    let books = Rc::new(RefCell::new(Vec::new()));
    let collected = candles.clone();
    let sink = function(move |_, a| {
        collected.borrow_mut().push(a[0].clone());
        Ok(u())
    });
    let clean = f.call(
        "listenOHLCV",
        vec![
            obj(vec![("symbol", s("BTC-USDT")), ("interval", s("1m"))]),
            sink.clone(),
        ],
    );
    let other = f.call(
        "listenOHLCV",
        vec![
            obj(vec![("symbol", s("BTC-USDT")), ("interval", s("1m"))]),
            sink,
        ],
    );
    let collected = books.clone();
    let clean_book = f.call(
        "listenOrderBook",
        vec![
            s("BTC-USDT"),
            function(move |_, a| {
                collected.borrow_mut().push(a[0].clone());
                Ok(u())
            }),
        ],
    );
    for cleanup in [&clean, &other, &clean_book] {
        let meta =
            f.rt.metas
                .borrow()
                .iter()
                .find(|(v, _, _, _)| Rc::ptr_eq(v, cleanup))
                .cloned()
                .unwrap();
        assert_eq!(
            (&*meta.1, meta.2, meta.3),
            ("", 0, false),
            "KUCOIN-PUBLIC cleanup callable metadata"
        );
    }
    let handlers = at(&f.r, "messageHandlers");
    let installed = rows(&at(&handlers, "candle_BTC-USDT_1min"));
    assert_eq!(installed.len(), 2, "KUCOIN-PUBLIC two original parsers");
    assert!(!Rc::ptr_eq(&installed[0], &installed[1]));
    f.message("{\"topic\":\"/market/candles:BTC-USDT_1min\",\"data\":{\"candles\":[\"1700000000123\",\"101\",\"102\",\"103\",\"97\",\"6\"]}}");
    f.message("{\"topic\":\"/spotMarket/level2Depth50:BTC-USDT\",\"data\":{\"bids\":[[\"99\",\"4\"],[\"100\",\"2\"]],\"asks\":[[\"111\",\"3\"],[\"110\",\"5\"]]}}");
    assert_eq!(
        candles.borrow().len(),
        2,
        "KUCOIN-PUBLIC two candle callbacks"
    );
    for candle in candles.borrow().iter() {
        assert_eq!(
            keys(candle),
            ["timestamp", "open", "close", "high", "low", "volume"]
        );
        for (key, want) in [
            ("timestamp", 1700000000.),
            ("open", 101.),
            ("close", 102.),
            ("high", 103.),
            ("low", 97.),
            ("volume", 6.),
        ] {
            assert_number(&at(candle, key), want);
        }
    }
    assert_eq!(books.borrow().len(), 1, "KUCOIN-PUBLIC one book callback");
    let book = books.borrow()[0].clone();
    assert_eq!(keys(&book), ["bids", "asks"]);
    for (side, wants) in [
        ("bids", [(100., 2., 2.), (99., 4., 6.)]),
        ("asks", [(110., 5., 5.), (111., 3., 8.)]),
    ] {
        let levels = rows(&at(&book, side));
        assert_eq!(levels.len(), 2, "KUCOIN-PUBLIC independent two levels");
        for (level, (price, amount, total)) in levels.iter().zip(wants) {
            assert_eq!(keys(level), ["price", "amount", "total"]);
            for (key, want) in [("price", price), ("amount", amount), ("total", total)] {
                assert_number(&at(level, key), want);
            }
        }
    }
    f.message("{\"topic\":\"/market/ticker:all\",\"subject\":\"BTC-USDT\",\"data\":{\"bestBid\":\"11\",\"bestAsk\":\"13\",\"price\":\"17\",\"changeRate\":\"0.19\",\"volume\":\"23\",\"turnover\":\"29\"}}");
    assert_eq!(
        f.updates.borrow().len(),
        1,
        "KUCOIN-PUBLIC one ticker update"
    );
    let update = f.updates.borrow()[0].clone();
    assert_eq!(
        keys(&update),
        [
            "bid",
            "ask",
            "last",
            "mark",
            "index",
            "percentage",
            "volume",
            "quoteVolume"
        ]
    );
    for (key, want) in [
        ("bid", 11.),
        ("ask", 13.),
        ("last", 17.),
        ("mark", 17.),
        ("index", 17.),
        ("percentage", 19.),
        ("volume", 23.),
        ("quoteVolume", 29.),
    ] {
        assert_number(&at(&update, key), want);
    }
    let callbacks = arr(vec![]);
    let changed = callbacks.clone();
    let seen = Rc::new(RefCell::new(None));
    let seen_second = seen.clone();
    let first = function(move |_, _| {
        let seen = seen_second.clone();
        let replacement = function(move |_, a| {
            *seen.borrow_mut() = Some(a[0].clone());
            Ok(u())
        });
        let Js::A(values) = changed.as_ref() else {
            panic!("array")
        };
        values.borrow_mut()[1] = replacement;
        Ok(u())
    });
    let Js::A(values) = callbacks.as_ref() else {
        panic!("array")
    };
    values
        .borrow_mut()
        .extend([first, function(|_, _| panic!("stale second callback"))]);
    set(&handlers, "candle_ETH-USDT_1min", callbacks);
    f.message("{\"topic\":\"/market/candles:ETH-USDT_1min\"}");
    assert_eq!(
        f.parsed.borrow().len(),
        4,
        "KUCOIN-PUBLIC four parsed source objects"
    );
    assert!(
        Rc::ptr_eq(seen.borrow().as_ref().unwrap(), &f.parsed.borrow()[3]),
        "KUCOIN-PUBLIC original live parsed alias"
    );
    call(clean, u(), vec![]).unwrap();
    let remaining = rows(&at(&handlers, "candle_BTC-USDT_1min"));
    assert_eq!(
        remaining.len(),
        1,
        "KUCOIN-PUBLIC partial cleanup retains one"
    );
    assert!(
        Rc::ptr_eq(&remaining[0], &installed[1]),
        "KUCOIN-PUBLIC cleanup retains original second parser"
    );
    call(other, u(), vec![]).unwrap();
    call(clean_book, u(), vec![]).unwrap();
    let Js::O(fields) = handlers.as_ref() else {
        panic!("handlers")
    };
    fields
        .borrow_mut()
        .retain(|(k, _)| k != "candle_ETH-USDT_1min");
    assert!(
        keys(&handlers).is_empty(),
        "KUCOIN-PUBLIC final handler deletion"
    );
    assert_frames(
        &f,
        &[
            ("subscribe", Some("/market/candles:BTC-USDT_1min")),
            ("subscribe", Some("/market/candles:BTC-USDT_1min")),
            ("subscribe", Some("/spotMarket/level2Depth50:BTC-USDT")),
            ("unsubscribe", Some("/market/candles:BTC-USDT_1min")),
            ("unsubscribe", Some("/spotMarket/level2Depth50:BTC-USDT")),
        ],
    );
    Box::new(
        move || json!({"candles":candles.borrow().iter().map(literal).collect::<Vec<_>>(),"books":books.borrow().iter().map(literal).collect::<Vec<_>>(),"update":literal(&update),"frames":f.frames.borrow().iter().map(|v|serde_json::from_str::<Value>(&text(v)).unwrap()).collect::<Vec<_>>(),"trace":*f.rt.trace.borrow(),"liveAlias":true,"remaining":[]}),
    )
}
#[test]
fn native_spot_public_bindings_before_maintained_source() {
    let layout = layout();
    let ingress = ingress();
    let streams = streams();
    let native = [layout, ingress(), streams()];
    for ((kind, native), index) in ["layout", "ingress", "streams"]
        .into_iter()
        .zip(native)
        .zip(0..3)
    {
        let output = Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/spot-public-class.cjs"),
            )
            .arg(kind)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "source {kind}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            native, source,
            "KUCOIN-PUBLIC source comparison {index}/{kind}"
        );
    }
}
