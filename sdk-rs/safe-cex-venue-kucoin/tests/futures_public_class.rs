#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::*;
use safe_cex_venue_kucoin::futures_public_class::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    task::Poll,
};
use values::*;
const ARROWS: [(&str, usize, bool); 17] = [
    ("connectAndSubscribe", 0, true),
    ("reconnect", 0, true),
    ("onOpen", 0, false),
    ("onMarketsChanged", 1, false),
    ("ping", 0, false),
    ("subscribe", 0, false),
    ("snapshotShardSymbols", 0, false),
    ("subscribeActiveTicker", 1, false),
    ("unsubscribeActiveTicker", 1, false),
    ("sendChannelOp", 3, false),
    ("onMessage", 1, false),
    ("tickerForApiSymbol", 1, false),
    ("handleTickerV2", 1, false),
    ("handleSnapshot", 1, false),
    ("handleInstrument", 1, false),
    ("listenOHLCV", 2, false),
    ("listenOrderBook", 2, false),
];
const FIELDS: [&str; 23] = [
    "messageHandlers",
    "token",
    "socketEndpoint",
    "role",
    "shardIndex",
    "shardCount",
    "connectAndSubscribe",
    "reconnect",
    "onOpen",
    "onMarketsChanged",
    "ping",
    "subscribe",
    "snapshotShardSymbols",
    "subscribeActiveTicker",
    "unsubscribeActiveTicker",
    "sendChannelOp",
    "onMessage",
    "tickerForApiSymbol",
    "handleTickerV2",
    "handleSnapshot",
    "handleInstrument",
    "listenOHLCV",
    "listenOrderBook",
];
fn at(v: &V, key: &str) -> V {
    get(v, key).unwrap()
}
fn rows(v: &V) -> Vec<V> {
    let Js::A(values) = v.as_ref() else {
        panic!("array")
    };
    values.borrow().clone()
}
fn keys(v: &V) -> Vec<String> {
    let Js::O(fields) = v.as_ref() else {
        panic!("object")
    };
    fields.borrow().iter().map(|(key, _)| key.clone()).collect()
}
fn num(v: &V, want: f64) {
    assert!(
        matches!(v.as_ref(), Js::N(_)),
        "KUCOIN-PUB-FUT native number kind"
    );
    assert_eq!(
        number(v).to_bits(),
        want.to_bits(),
        "KUCOIN-PUB-FUT native number bits"
    )
}
fn txt(v: &V, want: &str) {
    assert!(
        matches!(v.as_ref(), Js::S(_)),
        "KUCOIN-PUB-FUT native string kind"
    );
    assert_eq!(text(v), want, "KUCOIN-PUB-FUT native string value")
}
struct Constructors {
    bodies: Rc<RefCell<Vec<(V, Constructor<V, E>)>>>,
    ops: Rc<ConstructionOps<V, E>>,
}
impl Constructors {
    fn new(rt: &Rc<Runtime>) -> Self {
        let bodies = Rc::new(RefCell::new(Vec::new()));
        let saved = bodies.clone();
        let runtime = rt.clone();
        let base_rt = rt.clone();
        let super_rt = rt.clone();
        let ops: Rc<ConstructionOps<V, E>> = Rc::new(ConstructionOps {
            derive: Rc::new(move |name, base, length, body| {
                assert_eq!(name, "KucoinPublicWebsocket");
                assert_eq!(length, 2, "KUCOIN-PUB-FUT native constructor length");
                let class = obj(vec![("base", base.clone())]);
                let proto = obj(vec![]);
                runtime
                    .prototypes
                    .borrow_mut()
                    .push((class.clone(), proto.clone()));
                runtime
                    .prototype_parents
                    .borrow_mut()
                    .push((proto, at(&base, "prototype")));
                runtime
                    .metas
                    .borrow_mut()
                    .push((class.clone(), name.into(), length, false));
                saved.borrow_mut().push((class.clone(), body));
                Ok(class)
            }),
            superclass: Rc::new(move |active| base_rt.read(active, "base")),
            super_construct: Rc::new(move |base, target, args| {
                assert!(
                    Rc::ptr_eq(
                        &base,
                        super_rt.globals.borrow().get("BaseWebSocket").unwrap()
                    ),
                    "KUCOIN-PUB-FUT actual superclass"
                );
                assert!(
                    super_rt
                        .prototypes
                        .borrow()
                        .iter()
                        .any(|(class, _)| Rc::ptr_eq(class, &target)),
                    "KUCOIN-PUB-FUT original newTarget"
                );
                assert_eq!(args.len(), 1, "KUCOIN-PUB-FUT super only original parent");
                Ok(obj(vec![
                    ("parent", args[0].clone()),
                    ("isDisposed", b(false)),
                ]))
            }),
            construct: Rc::new(|constructor, args| {
                txt(&constructor, "Set");
                let entries = rows(&args[0]);
                Ok(obj(vec![(
                    "has",
                    function(move |_, args| {
                        Ok(b(entries.iter().any(|entry| {
                            match (entry.as_ref(), args[0].as_ref()) {
                                (Js::S(a), Js::S(b)) => a == b,
                                (Js::N(a), Js::N(b)) => a == b || a.is_nan() && b.is_nan(),
                                _ => Rc::ptr_eq(entry, &args[0]),
                            }
                        })))
                    }),
                )]))
            }),
        });
        Self { bodies, ops }
    }
    fn construct(&self, class: &V, parent: V, opts: V) -> V {
        let body = self
            .bodies
            .borrow()
            .iter()
            .find(|(value, _)| Rc::ptr_eq(value, class))
            .unwrap()
            .1
            .clone();
        body(class.clone(), class.clone(), vec![parent, opts]).unwrap()
    }
}
struct Fixture {
    rt: Rc<Runtime>,
    clocks: [Rc<Cell<u32>>; 2],
    ctors: Constructors,
    class: V,
    r: V,
    parent: V,
    fields: Vec<String>,
    posts: [V; 2],
    snapshot: V,
    frames: Rc<RefCell<Vec<V>>>,
    listeners: Rc<RefCell<Vec<(String, V)>>>,
    removed: Rc<RefCell<Vec<String>>>,
    updates: Rc<RefCell<Vec<V>>>,
    requests: Rc<RefCell<Vec<(V, V)>>>,
}
impl Fixture {
    fn new(opts: V) -> Self {
        let rt = Rc::new(Runtime::default());
        *rt.expected_class.borrow_mut() = "KucoinPublicWebsocket".into();
        rt.globals.borrow_mut().insert(
            "BaseWebSocket".into(),
            obj(vec![("prototype", obj(vec![]))]),
        );
        for (name, value) in [
            ("ENABLE_RECONNECTING_WEBSOCKET", b(true)),
            ("Set", s("Set")),
            ("SNAPSHOT_CHANNEL", s("/contractMarket/snapshot")),
            (
                "ACTIVE_DETAIL_TICKER_CHANNELS",
                arr(vec![
                    s("/contractMarket/tickerV2"),
                    s("/contract/instrument"),
                ]),
            ),
        ] {
            rt.globals.borrow_mut().insert(name.into(), value);
        }
        rt.globals
            .borrow_mut()
            .insert("Boolean".into(), function(|_, a| Ok(b(truthy(&a[0])))));
        let clocks = [Rc::new(Cell::new(0)), Rc::new(Cell::new(0))];
        let date = clocks[0].clone();
        let performance = clocks[1].clone();
        rt.globals.borrow_mut().insert(
            "Date".into(),
            obj(vec![(
                "now",
                function(move |_, _| {
                    date.set(date.get() + 1);
                    Ok(n(41.))
                }),
            )]),
        );
        rt.globals.borrow_mut().insert(
            "performance".into(),
            obj(vec![(
                "now",
                function(move |_, _| {
                    performance.set(performance.get() + 1);
                    Ok(n(79.))
                }),
            )]),
        );
        let string_rt = rt.clone();
        rt.globals.borrow_mut().insert(
            "JSON".into(),
            obj(vec![(
                "stringify",
                function(move |_, a| {
                    string_rt.stringified.borrow_mut().push(a[0].clone());
                    Ok(s(&stringify(&a[0])))
                }),
            )]),
        );
        rt.globals.borrow_mut().insert(
            "jsonParse".into(),
            function(|_, a| {
                Ok(serde_json::from_str::<Value>(&text(&a[0]))
                    .map(|value| decode(&value))
                    .unwrap_or_else(|_| Rc::new(Js::Null)))
            }),
        );
        for (name, table) in [
            ("ENDPOINTS", safe_cex_venue_kucoin::types::ENDPOINTS),
            ("WS_INTERVAL", safe_cex_venue_kucoin::types::WS_INTERVAL),
        ] {
            rt.globals.borrow_mut().insert(
                name.into(),
                obj(table.iter().map(|(key, value)| (*key, s(value))).collect()),
            );
        }
        rt.globals.borrow_mut().insert(
            "parseFloat".into(),
            function(|_, a| Ok(n(safe_cex_num::coercion::parse_float(&text(&a[0]))))),
        );
        rt.globals.borrow_mut().insert(
            "parseInt".into(),
            function(|_, a| {
                num(&a[1], 10.);
                Ok(n(safe_cex_num::coercion::parse_int(&text(&a[0]), Some(10))))
            }),
        );
        rt.globals.borrow_mut().insert(
            "calcOrderBookTotal".into(),
            function(|_, a| {
                for side in ["bids", "asks"] {
                    let mut sum = 0.;
                    for row in rows(&at(&a[0], side)) {
                        sum += number(&at(&row, "amount"));
                        set(&row, "total", n(sum));
                    }
                }
                Ok(u())
            }),
        );
        rt.globals.borrow_mut().insert(
            "updateOrderBookByPrice".into(),
            function(|_, a| {
                let side = text(&a[1]);
                let values = at(&a[0], &side);
                let Js::A(entries) = values.as_ref() else {
                    panic!("levels")
                };
                let price = number(&a[2]);
                let amount = number(&a[3]);
                let mut entries = entries.borrow_mut();
                if let Some(index) = entries
                    .iter()
                    .position(|row| number(&at(row, "price")) == price)
                {
                    if amount == 0. {
                        entries.remove(index);
                    } else {
                        set(&entries[index], "amount", n(amount));
                    }
                } else if amount != 0. {
                    entries.push(obj(vec![
                        ("price", n(price)),
                        ("amount", n(amount)),
                        ("total", n(0.)),
                    ]));
                    entries.sort_by(|a, b| {
                        let cmp = number(&at(a, "price")).total_cmp(&number(&at(b, "price")));
                        if side == "bids" { cmp.reverse() } else { cmp }
                    });
                }
                Ok(u())
            }),
        );
        let posts = [pending(), pending()];
        let post_values = posts.clone();
        let post_count = Rc::new(Cell::new(0));
        let post_trace = rt.clone();
        let count = post_count.clone();
        let xhr = obj(vec![(
            "post",
            function(move |_, a| {
                post_trace
                    .trace
                    .borrow_mut()
                    .push(format!("post:{}", text(&a[0])));
                let i = count.get();
                count.set(i + 1);
                Ok(post_values[i].clone())
            }),
        )]);
        let ticker = obj(vec![("id", s("ticker-1")), ("bid", n(7.)), ("ask", n(9.))]);
        let expected = ticker.clone();
        let updates = Rc::new(RefCell::new(Vec::new()));
        let changed = updates.clone();
        let error_rt = rt.clone();
        let emitter = obj(vec![]);
        let parent = obj(vec![
            (
                "store",
                obj(vec![
                    (
                        "markets",
                        arr((0..205)
                            .map(|i| {
                                obj(vec![
                                    ("id", s(&format!("API-{i}"))),
                                    ("symbol", s(&format!("S-{i}"))),
                                ])
                            })
                            .collect()),
                    ),
                    ("tickersBySymbol", obj(vec![("S-1", ticker)])),
                    (
                        "updateTicker",
                        function(move |_, a| {
                            assert!(
                                Rc::ptr_eq(&a[0], &expected),
                                "KUCOIN-PUB-FUT original ticker identity"
                            );
                            changed.borrow_mut().push(a[1].clone());
                            Ok(u())
                        }),
                    ),
                ]),
            ),
            (
                "toApiSymbol",
                function(|_, a| Ok(s(&text(&a[0]).replacen("S-", "API-", 1)))),
            ),
            (
                "toInternalSymbol",
                function(|_, a| Ok(s(&text(&a[0]).replacen("API-", "S-", 1)))),
            ),
            ("xhr", xhr.clone()),
            ("emitter", emitter.clone()),
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
        let ctors = Constructors::new(&rt);
        let class = define_futures_public_class(rt.operations(), ctors.ops.clone()).unwrap();
        let r = ctors.construct(&class, parent.clone(), opts);
        let fields = keys(&r)[2..].to_vec();
        set(&r, "isConnected", b(true));
        set(&r, "onClose", function(|_, _| Ok(u())));
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
        let removed = Rc::new(RefCell::new(Vec::new()));
        let removed_events = removed.clone();
        let receiver = r.clone();
        let socket_rt = rt.clone();
        set(
            &r,
            "createWebsocket",
            function(move |_, a| {
                socket_rt
                    .trace
                    .borrow_mut()
                    .push(format!("socket:{}", text(&a[0])));
                let socket = obj(vec![]);
                let target = socket.clone();
                let installed = installed.clone();
                set(
                    &socket,
                    "addEventListener",
                    function(move |actual, a| {
                        assert!(
                            Rc::ptr_eq(&actual, &target),
                            "KUCOIN-PUB-FUT listener socket receiver"
                        );
                        installed.borrow_mut().push((text(&a[0]), a[1].clone()));
                        Ok(u())
                    }),
                );
                let target = socket.clone();
                let r = receiver.clone();
                let removed = removed_events.clone();
                set(
                    &socket,
                    "removeEventListener",
                    function(move |actual, a| {
                        assert!(
                            Rc::ptr_eq(&actual, &target),
                            "KUCOIN-PUB-FUT previous socket receiver"
                        );
                        let event = text(&a[0]);
                        let field = match event.as_str() {
                            "open" => "onOpen",
                            "message" => "onMessage",
                            _ => "onClose",
                        };
                        assert!(
                            Rc::ptr_eq(&a[1], &at(&r, field)),
                            "KUCOIN-PUB-FUT removed original listener"
                        );
                        removed.borrow_mut().push(event);
                        Ok(u())
                    }),
                );
                let target = socket.clone();
                let r = receiver.clone();
                let trace = socket_rt.clone();
                set(
                    &socket,
                    "close",
                    function(move |actual, _| {
                        assert!(
                            Rc::ptr_eq(&actual, &target),
                            "KUCOIN-PUB-FUT close previous socket"
                        );
                        assert!(
                            matches!(at(&r, "ws").as_ref(), Js::U),
                            "KUCOIN-PUB-FUT clear socket before close"
                        );
                        trace.trace.borrow_mut().push("close".into());
                        Ok(u())
                    }),
                );
                Ok(socket)
            }),
        );
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
        for key in ["off", "on"] {
            let expected = emitter.clone();
            let r = r.clone();
            let rt = rt.clone();
            set(
                &emitter,
                key,
                function(move |actual, a| {
                    assert!(
                        Rc::ptr_eq(&actual, &expected),
                        "KUCOIN-PUB-FUT emitter receiver"
                    );
                    txt(&a[0], "markets:changed");
                    assert!(
                        Rc::ptr_eq(&a[1], &at(&r, "onMarketsChanged")),
                        "KUCOIN-PUB-FUT original market listener"
                    );
                    rt.trace.borrow_mut().push(format!("emitter.{key}"));
                    Ok(u())
                }),
            );
        }
        let snapshot = pending();
        let loaded = snapshot.clone();
        let requests = Rc::new(RefCell::new(Vec::new()));
        let actual_requests = requests.clone();
        let expected = xhr.clone();
        let get = function(move |actual, a| {
            assert!(
                Rc::ptr_eq(&actual, &expected),
                "KUCOIN-PUB-FUT snapshot xhr receiver"
            );
            assert!(
                matches!(a[0].as_ref(), Js::S(_)),
                "KUCOIN-PUB-FUT native route string kind"
            );
            assert_eq!(
                text(&a[0]),
                "/probe-orderbook",
                "KUCOIN-PUB-FUT endpoint follows callee getter"
            );
            txt(&at(&at(&a[1], "params"), "symbol"), "API-1");
            actual_requests
                .borrow_mut()
                .push((a[0].clone(), a[1].clone()));
            Ok(loaded.clone())
        });
        let endpoints = rt.globals.borrow().get("ENDPOINTS").unwrap().clone();
        rt.getters.borrow_mut().push((
            xhr,
            "get".into(),
            Rc::new(move || {
                set(&endpoints, "ORDERBOOK", s("/probe-orderbook"));
                Ok(get.clone())
            }),
        ));
        Self {
            rt,
            clocks,
            ctors,
            class,
            r,
            parent,
            fields,
            posts,
            snapshot,
            frames,
            listeners,
            removed,
            updates,
            requests,
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
    fn assert_clocks(&self, want: [u32; 2]) {
        assert_eq!(
            [self.clocks[0].get(), self.clocks[1].get()],
            want,
            "KUCOIN-PUB-FUT independent clock counts"
        );
    }
    fn wire_frames(&self) -> Vec<Value> {
        self.frames
            .borrow()
            .iter()
            .map(|v| serde_json::from_str(&text(v)).unwrap())
            .collect()
    }
}
fn snapshot_opts() -> V {
    obj(vec![
        ("role", s("snapshot")),
        ("shardIndex", n(1.)),
        ("shardCount", n(2.)),
    ])
}
fn frame(frame: &V, kind: &str, topic: Option<&str>, response: bool) {
    num(&at(frame, "id"), 41.);
    txt(&at(frame, "type"), kind);
    assert_eq!(
        keys(frame),
        if topic.is_some() {
            vec!["id", "type", "topic", "privateChannel", "response"]
        } else {
            vec!["id", "type"]
        }
    );
    if let Some(topic) = topic {
        txt(&at(frame, "topic"), topic);
        assert!(matches!(at(frame, "privateChannel").as_ref(), Js::B(false)));
        assert!(matches!(at(frame,"response").as_ref(),Js::B(v) if *v==response));
    }
}
fn layout() -> Value {
    let f = Fixture::new(u());
    assert!(f.rt.timers.borrow().is_empty());
    assert_eq!(f.rt.next_timer.get(), 0);
    assert_eq!(
        f.fields, FIELDS,
        "KUCOIN-PUB-FUT complete twenty-three fields"
    );
    let other = f.ctors.construct(
        &f.class,
        f.parent.clone(),
        obj(vec![
            ("role", Rc::new(Js::Null)),
            ("shardIndex", n(0.)),
            ("shardCount", n(0.)),
        ]),
    );
    assert_eq!(
        keys(&other)[2..],
        FIELDS,
        "KUCOIN-PUB-FUT complete clean fields"
    );
    let definitions =
        f.rt.definitions
            .borrow()
            .iter()
            .filter(|(r, _)| Rc::ptr_eq(r, &other))
            .map(|(_, key)| key.clone())
            .collect::<Vec<_>>();
    assert_eq!(
        definitions, FIELDS,
        "KUCOIN-PUB-FUT twenty-three definitions"
    );
    assert!(
        !Rc::ptr_eq(&at(&f.r, "messageHandlers"), &at(&other, "messageHandlers")),
        "KUCOIN-PUB-FUT fresh handler dictionary"
    );
    assert!(matches!(at(&f.r, "token").as_ref(), Js::Null));
    assert!(matches!(at(&f.r, "socketEndpoint").as_ref(), Js::Null));
    for (name, arity, asynchronous) in ARROWS {
        let value = at(&f.r, name);
        assert!(
            !Rc::ptr_eq(&value, &at(&other, name)),
            "KUCOIN-PUB-FUT fresh native arrow"
        );
        let metadata =
            f.rt.metas
                .borrow()
                .iter()
                .find(|(v, _, _, _)| Rc::ptr_eq(v, &value))
                .cloned()
                .unwrap();
        assert_eq!(
            (&*metadata.1, metadata.2, metadata.3),
            (name, arity, asynchronous),
            "KUCOIN-PUB-FUT native arrow metadata"
        );
    }
    txt(&at(&f.r, "role"), "detail");
    num(&at(&f.r, "shardIndex"), 0.);
    num(&at(&f.r, "shardCount"), 1.);
    txt(&at(&other, "role"), "detail");
    num(&at(&other, "shardIndex"), 0.);
    num(&at(&other, "shardCount"), 0.);
    assert!(keys(&f.rt.prototypes.borrow()[0].1).is_empty());
    f.assert_clocks([0, 0]);
    json!({"clocks":[0,0],"fields":f.fields,"arrows":ARROWS,"prototype":[],"defaults":["detail",0,1],"nullish":["detail",0,0]})
}
fn snapshot() -> Box<dyn FnOnce() -> Value> {
    let f = Fixture::new(snapshot_opts());
    let ids = f.call("snapshotShardSymbols", vec![]);
    let actual = rows(&ids);
    assert_eq!(actual.len(), 102, "KUCOIN-PUB-FUT independent shard length");
    for (index, id) in actual.iter().enumerate() {
        txt(id, &format!("API-{}", 2 * index + 1));
    }
    f.call("subscribe", vec![]);
    assert_eq!(f.frame_values().len(), 2, "KUCOIN-PUB-FUT two batches");
    let first = (0..100)
        .map(|i| format!("API-{}", 2 * i + 1))
        .collect::<Vec<_>>()
        .join(",");
    frame(
        &f.frame_values()[0],
        "subscribe",
        Some(&format!("/contractMarket/snapshot:{first}")),
        false,
    );
    frame(
        &f.frame_values()[1],
        "subscribe",
        Some("/contractMarket/snapshot:API-201,API-203"),
        false,
    );
    f.call("subscribeActiveTicker", vec![s("API-1")]);
    f.call("unsubscribeActiveTicker", vec![s("API-1")]);
    assert_eq!(
        f.frame_values().len(),
        6,
        "KUCOIN-PUB-FUT six initial frames"
    );
    for (index, (kind, topic)) in [
        ("subscribe", "/contractMarket/tickerV2:API-1"),
        ("subscribe", "/contract/instrument:API-1"),
        ("unsubscribe", "/contractMarket/tickerV2:API-1"),
        ("unsubscribe", "/contract/instrument:API-1"),
    ]
    .into_iter()
    .enumerate()
    {
        frame(&f.frame_values()[index + 2], kind, Some(topic), false);
    }
    set(&f.r, "ws", obj(vec![]));
    f.call(
        "onMarketsChanged",
        vec![obj(vec![(
            "added",
            arr(vec![
                obj(vec![("id", s("API-1")), ("symbol", s("S-1"))]),
                obj(vec![("id", s("API-2")), ("symbol", s("S-2"))]),
                obj(vec![("id", s("")), ("symbol", s("S-3"))]),
            ]),
        )])],
    );
    assert_eq!(
        f.frame_values().len(),
        7,
        "KUCOIN-PUB-FUT one owned addition frame"
    );
    frame(
        &f.frame_values()[6],
        "subscribe",
        Some("/contractMarket/snapshot:API-1,API-3"),
        false,
    );
    f.assert_clocks([7, 0]);
    Box::new(move || json!({"clocks":[7,0],"ids":literal(&ids),"frames":f.wire_frames()}))
}
fn resolve_token(promise: &V, token: &str, endpoint: &str) {
    resolve(
        promise,
        Ok(obj(vec![(
            "data",
            obj(vec![
                ("code", s("200000")),
                (
                    "data",
                    obj(vec![
                        ("token", s(token)),
                        (
                            "instanceServers",
                            arr(vec![obj(vec![("endpoint", s(endpoint))])]),
                        ),
                    ]),
                ),
            ]),
        )])),
    );
}
fn reconnect_case() -> Box<dyn FnOnce() -> Value> {
    let f = Fixture::new(snapshot_opts());
    f.call("connectAndSubscribe", vec![]);
    assert_eq!(*f.rt.trace.borrow(), ["post:/api/v1/bullet-public"]);
    assert!(matches!(at(&f.r, "token").as_ref(), Js::Null));
    resolve_token(&f.posts[0], "token-23", "wss://socket.example");
    f.rt.pump();
    assert_eq!(
        f.listeners.borrow().len(),
        3,
        "KUCOIN-PUB-FUT three first listeners"
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
            "KUCOIN-PUB-FUT actual listener identity"
        );
    }
    f.call("onOpen", vec![]);
    num(&at(&f.r, "pingAt"), 79.);
    assert_eq!(
        f.frame_values().len(),
        3,
        "KUCOIN-PUB-FUT subscribe then ping"
    );
    let first = (0..100)
        .map(|i| format!("API-{}", 2 * i + 1))
        .collect::<Vec<_>>()
        .join(",");
    frame(
        &f.frame_values()[0],
        "subscribe",
        Some(&format!("/contractMarket/snapshot:{first}")),
        false,
    );
    frame(
        &f.frame_values()[1],
        "subscribe",
        Some("/contractMarket/snapshot:API-201,API-203"),
        false,
    );
    frame(&f.frame_values()[2], "ping", None, false);
    assert_eq!(
        &f.rt.trace.borrow()[2..],
        ["emitter.off", "emitter.on", "emitWsReconnected"]
    );
    let outer = f.call("reconnect", vec![]);
    assert_eq!(*f.removed.borrow(), ["open", "message", "close"]);
    assert!(matches!(at(&f.r, "ws").as_ref(), Js::U));
    assert!(matches!(at(&f.r, "token").as_ref(), Js::Null));
    assert!(matches!(at(&f.r, "socketEndpoint").as_ref(), Js::Null));
    assert!(
        matches!(poll(&outer), Poll::Pending),
        "KUCOIN-PUB-FUT reconnect remains pending"
    );
    f.rt.pump();
    assert!(
        matches!(poll(&outer), Poll::Pending),
        "KUCOIN-PUB-FUT reconnect awaits inner token"
    );
    let weak = Rc::downgrade(&outer);
    drop(outer);
    resolve_token(&f.posts[1], "token-29", "wss://other.example");
    f.rt.pump();
    assert!(
        matches!(poll(&weak.upgrade().unwrap()),Poll::Ready(Ok(value)) if matches!(value.as_ref(),Js::U)),
        "KUCOIN-PUB-FUT retained reconnect completion"
    );
    txt(
        &at(&f.r, "socketEndpoint"),
        "wss://other.example?token=token-29&connectId=41",
    );
    assert_eq!(
        f.listeners.borrow().len(),
        6,
        "KUCOIN-PUB-FUT six total listeners"
    );
    f.assert_clocks([5, 1]);
    Box::new(
        move || json!({"clocks":[5,1],"url":text(&at(&f.r,"socketEndpoint")),"trace":*f.rt.trace.borrow(),"removed":*f.removed.borrow(),"listeners":f.listeners.borrow().iter().map(|(name,_)|name.clone()).collect::<Vec<_>>(),"frames":f.wire_frames(),"retained":true}),
    )
}
fn assert_book(book: &V, expected: &[(f64, f64, f64)]) {
    assert_eq!(keys(book), ["bids", "asks"]);
    let bids = rows(&at(book, "bids"));
    assert_eq!(
        bids.len(),
        expected.len(),
        "KUCOIN-PUB-FUT independent book bid count"
    );
    for (row, (price, amount, total)) in bids.iter().zip(expected) {
        assert_eq!(keys(row), ["price", "amount", "total"]);
        num(&at(row, "price"), *price);
        num(&at(row, "amount"), *amount);
        num(&at(row, "total"), *total);
    }
    let asks = rows(&at(book, "asks"));
    assert_eq!(asks.len(), 1, "KUCOIN-PUB-FUT independent ask count");
    num(&at(&asks[0], "price"), 110.);
    num(&at(&asks[0], "amount"), 3.);
    num(&at(&asks[0], "total"), 3.);
}
fn streams() -> Box<dyn FnOnce() -> Value> {
    let f = Fixture::new(u());
    let candles = Rc::new(RefCell::new(Vec::new()));
    let books = Rc::new(RefCell::new(Vec::new()));
    let collected = candles.clone();
    let clean_candle = f.call(
        "listenOHLCV",
        vec![
            obj(vec![("symbol", s("S-1")), ("interval", s("1m"))]),
            function(move |_, a| {
                collected.borrow_mut().push(a[0].clone());
                Ok(u())
            }),
        ],
    );
    let collected = books.clone();
    let clean_book = f.call(
        "listenOrderBook",
        vec![
            s("S-1"),
            function(move |_, a| {
                collected.borrow_mut().push(a[0].clone());
                Ok(u())
            }),
        ],
    );
    assert_eq!(
        f.requests.borrow().len(),
        1,
        "KUCOIN-PUB-FUT one eager snapshot request"
    );
    assert!(books.borrow().is_empty());
    f.message("{\"topic\":\"/contractMarket/limitCandle:API-1_1min\",\"data\":{\"candles\":[\"1700000000\",\"101\",\"102\",\"103\",\"97\",\"6\"]}}");
    assert_eq!(
        candles.borrow().len(),
        1,
        "KUCOIN-PUB-FUT one routed candle"
    );
    let candle = candles.borrow()[0].clone();
    assert_eq!(
        keys(&candle),
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
        num(&at(&candle, key), want);
    }
    f.message("{\"topic\":\"/contractMarket/level2:API-1\",\"type\":\"update\",\"data\":{\"sequence\":101,\"change\":\"100,buy,5\"}}");
    resolve(
        &f.snapshot,
        Ok(obj(vec![(
            "data",
            obj(vec![
                ("code", s("200000")),
                (
                    "data",
                    obj(vec![
                        ("sequence", n(100.)),
                        ("bids", arr(vec![arr(vec![s("100"), s("2")])])),
                        ("asks", arr(vec![arr(vec![s("110"), s("3")])])),
                    ]),
                ),
            ]),
        )])),
    );
    f.rt.pump();
    assert_eq!(
        books.borrow().len(),
        1,
        "KUCOIN-PUB-FUT one buffered snapshot callback"
    );
    assert_book(&books.borrow()[0], &[(100., 5., 5.)]);
    f.message("{\"topic\":\"/contractMarket/level2:API-1\",\"type\":\"update\",\"data\":{\"sequence\":102,\"change\":\"99,buy,7\"}}");
    assert_eq!(books.borrow().len(), 2, "KUCOIN-PUB-FUT two book callbacks");
    assert!(
        Rc::ptr_eq(&books.borrow()[0], &books.borrow()[1]),
        "KUCOIN-PUB-FUT original shared book alias"
    );
    let book = books.borrow()[1].clone();
    assert_book(&book, &[(100., 5., 5.), (99., 7., 12.)]);
    let captured = obj(vec![
        ("bids", arr(rows(&at(&book, "bids")))),
        ("asks", arr(rows(&at(&book, "asks")))),
    ]);
    for message in [
        "{\"topic\":\"/contractMarket/tickerV2:API-1\",\"data\":{\"symbol\":\"API-1\",\"bestBidPrice\":\"11\",\"bestAskPrice\":\"13\"}}",
        "{\"topic\":\"/contractMarket/snapshot:API-1\",\"data\":{\"symbol\":\"API-1\",\"lastPrice\":\"17\",\"priceChgPct\":\"0.19\",\"fundingRate\":\"0.023\",\"volume\":\"29\",\"turnover\":\"31\"}}",
        "{\"topic\":\"/contract/instrument:API-1\",\"data\":{\"markPrice\":\"37\",\"indexPrice\":\"41\"}}",
    ] {
        f.message(message);
    }
    assert_eq!(
        f.updates.borrow().len(),
        3,
        "KUCOIN-PUB-FUT three distinct ticker updates"
    );
    for (index, expected) in [
        vec![("bid", 11.), ("ask", 13.)],
        vec![
            ("last", 17.),
            ("percentage", 19.),
            ("fundingRate", 0.023),
            ("volume", 29.),
            ("quoteVolume", 31.),
        ],
        vec![("mark", 37.), ("index", 41.)],
    ]
    .into_iter()
    .enumerate()
    {
        let update = f.updates.borrow()[index].clone();
        assert_eq!(
            keys(&update),
            expected.iter().map(|(key, _)| *key).collect::<Vec<_>>()
        );
        for (key, want) in expected {
            num(&at(&update, key), want);
        }
    }
    call(clean_candle, u(), vec![]).unwrap();
    call(clean_book, u(), vec![]).unwrap();
    let handlers = at(&f.r, "messageHandlers");
    assert_eq!(
        keys(&handlers),
        ["candle_S-1_1min", "orderBook_S-1"],
        "KUCOIN-PUB-FUT retained empty handler keys"
    );
    for key in ["candle_S-1_1min", "orderBook_S-1"] {
        assert!(
            rows(&at(&handlers, key)).is_empty(),
            "KUCOIN-PUB-FUT zero retained callbacks"
        );
    }
    assert!(rows(&at(&book, "bids")).is_empty());
    assert!(rows(&at(&book, "asks")).is_empty());
    let frames = f.frame_values();
    assert_eq!(frames.len(), 8, "KUCOIN-PUB-FUT eight stream frames");
    for (index, (kind, topic, response)) in [
        ("subscribe", "/contractMarket/limitCandle:API-1_1min", true),
        ("subscribe", "/contractMarket/level2:API-1", true),
        ("subscribe", "/contractMarket/tickerV2:API-1", false),
        ("subscribe", "/contract/instrument:API-1", false),
        (
            "unsubscribe",
            "/contractMarket/limitCandle:API-1_1min",
            true,
        ),
        ("unsubscribe", "/contractMarket/level2:API-1", true),
        ("unsubscribe", "/contractMarket/tickerV2:API-1", false),
        ("unsubscribe", "/contract/instrument:API-1", false),
    ]
    .into_iter()
    .enumerate()
    {
        frame(&frames[index], kind, Some(topic), response);
    }
    f.assert_clocks([8, 0]);
    Box::new(
        move || json!({"clocks":[8,0],"candles":candles.borrow().iter().map(literal).collect::<Vec<_>>(),"book":literal(&captured),"updates":f.updates.borrow().iter().map(literal).collect::<Vec<_>>(),"frames":f.wire_frames(),"trace":*f.rt.trace.borrow(),"requests":f.requests.borrow().iter().map(|(route,opts)|vec![literal(route),literal(opts)]).collect::<Vec<_>>(),"liveBook":true,"cleared":true,"remaining":["candle_S-1_1min","orderBook_S-1"]}),
    )
}
#[test]
fn native_futures_public_bindings_before_maintained_source() {
    let layout = layout();
    let snapshot = snapshot();
    let reconnect = reconnect_case();
    let streams = streams();
    let native = [layout, snapshot(), reconnect(), streams()];
    for (kind, native) in ["layout", "snapshot", "reconnect", "streams"]
        .into_iter()
        .zip(native)
    {
        let output = Command::new("node")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("scripts/futures-public-class.cjs"),
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
        assert_eq!(native, source, "KUCOIN-PUB-FUT source {kind}");
    }
}
