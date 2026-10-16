#[path = "support/private_class_runtime.rs"]
mod runtime;
#[path = "support/spot_private_values.rs"]
mod values;
use runtime::Runtime;
use safe_cex_venue_kucoin::futures_private_class::define_futures_private_class;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
    task::Poll,
};
use values::*;
struct Fixture {
    rt: Rc<Runtime>,
    class: V,
    r: V,
    parent: V,
    post: V,
    frames: Rc<RefCell<Vec<V>>>,
    listeners: Rc<RefCell<Vec<(String, V)>>>,
    removed: Rc<RefCell<Vec<(String, V)>>>,
    mapped: Rc<RefCell<Vec<V>>>,
    parsed: Rc<RefCell<Vec<V>>>,
    orders: V,
    updates: Rc<RefCell<Vec<(V, V)>>>,
    fills: Rc<RefCell<Vec<V>>>,
    snapshots: Rc<Cell<u32>>,
    snapshot_timers: Rc<RefCell<Vec<V>>>,
    urls: Rc<RefCell<Vec<V>>>,
}
impl Fixture {
    fn new() -> Self {
        let rt = Rc::new(Runtime::default());
        *rt.expected_class.borrow_mut() = "KucoinPrivateWebsocket".into();
        let base_rt = rt.clone();
        let base = obj(vec![(
            "prototype",
            obj(vec![(
                "dispose",
                function(move |r, a| {
                    assert!(a.is_empty());
                    base_rt.trace.borrow_mut().push("super.dispose".into());
                    set(&r, "isDisposed", b(true));
                    Ok(n(97.))
                }),
            )]),
        )]);
        let globals = [
            ("BaseWebSocket", base),
            ("ENABLE_RECONNECTING_WEBSOCKET", b(true)),
            ("Date", obj(vec![("now", function(|_, _| Ok(n(41.))))])),
            (
                "performance",
                obj(vec![("now", function(|_, _| Ok(n(79.))))]),
            ),
            (
                "ENDPOINTS",
                obj(safe_cex_venue_kucoin::types::ENDPOINTS
                    .iter()
                    .map(|(k, v)| (*k, s(v)))
                    .collect()),
            ),
            (
                "OrderStatus",
                obj(vec![("Canceled", s("canceled")), ("Closed", s("closed"))]),
            ),
        ];
        for (k, v) in globals {
            rt.globals.borrow_mut().insert(k.into(), v);
        }
        let encoded = rt.clone();
        rt.globals.borrow_mut().insert(
            "JSON".into(),
            obj(vec![(
                "stringify",
                function(move |_, a| {
                    encoded.stringified.borrow_mut().push(a[0].clone());
                    Ok(s(&stringify(&a[0])))
                }),
            )]),
        );
        let parsed = Rc::new(RefCell::new(Vec::new()));
        let parse_results = parsed.clone();
        rt.globals.borrow_mut().insert(
            "jsonParse".into(),
            function(move |_, a| {
                let value = serde_json::from_str::<Value>(&text(&a[0]))
                    .map(|v| decode(&v))
                    .unwrap_or_else(|_| Rc::new(Js::Null));
                parse_results.borrow_mut().push(value.clone());
                Ok(value)
            }),
        );
        rt.globals.borrow_mut().insert(
            "parseFloat".into(),
            function(|_, a| Ok(n(safe_cex_num::parse_float(&text(&a[0]))))),
        );
        let timers = rt.clone();
        rt.globals.borrow_mut().insert(
            "setTimeout".into(),
            function(move |_, a| {
                assert!(
                    matches!(a[1].as_ref(),Js::N(v)if *v==500.),
                    "KUCOIN-FUTURE native timer delay"
                );
                let id = timers.next_timer.get() + 1;
                timers.next_timer.set(id);
                timers.timers.borrow_mut().push((id as f64, a[0].clone()));
                Ok(n(id as f64))
            }),
        );
        let clears = rt.clone();
        rt.globals.borrow_mut().insert(
            "clearTimeout".into(),
            function(move |_, a| {
                clears
                    .trace
                    .borrow_mut()
                    .push(format!("clear:{}", text(&a[0])));
                Ok(u())
            }),
        );
        let post = pending();
        let response = post.clone();
        let urls = Rc::new(RefCell::new(Vec::new()));
        let sent_urls = urls.clone();
        let snapshots = Rc::new(Cell::new(0));
        let snapshot_count = snapshots.clone();
        let receiver = Rc::new(RefCell::new(None::<V>));
        let current = receiver.clone();
        let snapshot_timers = Rc::new(RefCell::new(Vec::new()));
        let observed = snapshot_timers.clone();
        let snapshot_trace = rt.clone();
        let mapped = Rc::new(RefCell::new(Vec::new()));
        let mapped_args = mapped.clone();
        let orders = arr(vec![
            obj(vec![
                ("id", s("parent-13")),
                ("side", s("sell")),
                ("symbol", s("BTC/USDT")),
            ]),
            obj(vec![
                ("id", s("child-17")),
                ("side", s("buy")),
                ("symbol", s("ETH/USDT")),
            ]),
        ]);
        let mapped_orders = orders.clone();
        let updates = Rc::new(RefCell::new(Vec::new()));
        let statuses = updates.clone();
        let live_orders = orders.clone();
        let fills = Rc::new(RefCell::new(Vec::new()));
        let emitted = fills.clone();
        let store = obj(vec![
            (
                "marketsBySymbol",
                obj(vec![("BTC/USDT", obj(vec![("contractSize", n(7.))]))]),
            ),
            (
                "updateOrderStatus",
                function(move |_, a| {
                    statuses.borrow_mut().push((a[0].clone(), a[1].clone()));
                    if statuses.borrow().len() == 1 {
                        let Js::A(rows) = live_orders.as_ref() else {
                            panic!("orders array")
                        };
                        rows.borrow_mut()[1] = obj(vec![
                            ("id", s("child-19")),
                            ("side", s("buy")),
                            ("symbol", s("ETH/USDT")),
                        ]);
                    }

                    Ok(u())
                }),
            ),
        ]);
        let parent_ref = Rc::new(RefCell::new(None::<V>));
        let map_receiver = parent_ref.clone();
        let parent = obj(vec![
            ("store", store),
            (
                "emitter",
                obj(vec![(
                    "emit",
                    function(move |_, a| {
                        assert!(matches!(a[0].as_ref(),Js::S(v)if v=="fill"));
                        emitted.borrow_mut().push(a[1].clone());
                        Ok(u())
                    }),
                )]),
            ),
            (
                "xhr",
                obj(vec![(
                    "post",
                    function(move |_, a| {
                        sent_urls.borrow_mut().extend(a);
                        Ok(response.clone())
                    }),
                )]),
            ),
            (
                "mapOrder",
                function(move |this, a| {
                    assert!(
                        Rc::ptr_eq(&this, map_receiver.borrow().as_ref().unwrap()),
                        "KUCOIN-FUTURE original parent mapper receiver"
                    );
                    mapped_args.borrow_mut().push(a[0].clone());
                    Ok(mapped_orders.clone())
                }),
            ),
            (
                "fetchAndSetPrivateSnapshot",
                function(move |_, a| {
                    assert!(a.is_empty(), "KUCOIN-FUTURE snapshot without Spot limit");
                    snapshot_count.set(snapshot_count.get() + 1);
                    observed.borrow_mut().push(get(
                        current.borrow().as_ref().unwrap(),
                        "snapshotRefreshTimer",
                    )?);
                    snapshot_trace.trace.borrow_mut().push("snapshot".into());
                    Ok(u())
                }),
            ),
            (
                "emitAndLogError",
                function(|_, a| panic!("unexpected venue error: {}", text(&a[0]))),
            ),
        ]);
        *parent_ref.borrow_mut() = Some(parent.clone());
        let class = define_futures_private_class(rt.operations()).unwrap();
        let r = rt.construct(&class, parent.clone());
        *receiver.borrow_mut() = Some(r.clone());
        let frames = Rc::new(RefCell::new(Vec::new()));
        let sent = frames.clone();
        set(
            &r,
            "sendWithLog",
            function(move |_, a| {
                sent.borrow_mut().extend(a);
                Ok(u())
            }),
        );
        let listeners = Rc::new(RefCell::new(Vec::new()));
        let registered = listeners.clone();
        let removed = Rc::new(RefCell::new(Vec::new()));
        let removals = removed.clone();
        let socket_ref = Rc::new(RefCell::new(None::<V>));
        let expected = socket_ref.clone();
        let expected_close = socket_ref.clone();
        let closing = rt.clone();
        let instance = r.clone();
        let socket = obj(vec![
            (
                "addEventListener",
                function(move |_, a| {
                    registered.borrow_mut().push((text(&a[0]), a[1].clone()));
                    Ok(u())
                }),
            ),
            (
                "removeEventListener",
                function(move |this, a| {
                    assert!(
                        Rc::ptr_eq(&this, expected.borrow().as_ref().unwrap()),
                        "KUCOIN-FUTURE previous socket receiver"
                    );
                    removals.borrow_mut().push((text(&a[0]), a[1].clone()));
                    Ok(u())
                }),
            ),
            (
                "close",
                function(move |this, _| {
                    assert!(Rc::ptr_eq(&this, expected_close.borrow().as_ref().unwrap()));
                    undefined(
                        &get(&instance, "ws")?,
                        "KUCOIN-FUTURE ws cleared before close",
                    );
                    closing.trace.borrow_mut().push("close".into());
                    Ok(u())
                }),
            ),
        ]);
        *socket_ref.borrow_mut() = Some(socket.clone());
        let creation = rt.clone();
        set(
            &r,
            "createWebsocket",
            function(move |_, a| {
                creation
                    .trace
                    .borrow_mut()
                    .push(format!("socket:{}", text(&a[0])));
                Ok(socket.clone())
            }),
        );
        set(&r, "onClose", function(|_, _| Ok(u())));
        for key in [
            "trackLastReceived",
            "emitWsDiagnosticReceive",
            "handlePongEvent",
            "recordMessageHandlerStats",
            "retryFailedConnect",
        ] {
            let tracing = rt.clone();
            set(
                &r,
                key,
                function(move |_, a| {
                    if key == "handlePongEvent" {
                        assert!(a.is_empty(), "KUCOIN-FUTURE pong has no Spot interval");
                    }
                    tracing.trace.borrow_mut().push(
                        if ["recordMessageHandlerStats", "retryFailedConnect"].contains(&key) {
                            format!("{key}:{}", text(&a[0]))
                        } else {
                            key.into()
                        },
                    );
                    Ok(u())
                }),
            );
        }
        Self {
            rt,
            class,
            r,
            parent,
            post,
            frames,
            listeners,
            removed,
            mapped,
            parsed,
            orders,
            updates,
            fills,
            snapshots,
            snapshot_timers,
            urls,
        }
    }
    fn resolve_token(&self) {
        resolve(
            &self.post,
            Ok(decode(
                &json!({"data":{"code":"200000","data":{"token":"token-23","instanceServers":[{"endpoint":"wss://future.example"}]}}}),
            )),
        );
        self.rt.pump();
    }
    fn message(&self, value: Value) {
        self.rt
            .method(
                &self.r,
                "onMessage",
                vec![obj(vec![("data", s(&value.to_string()))])],
            )
            .unwrap();
    }
    fn fire(&self, index: usize) {
        let callback = self.rt.timers.borrow()[index].1.clone();
        call(callback, u(), vec![]).unwrap();
    }
}
fn keys(v: &V) -> Vec<String> {
    let Js::O(fields) = v.as_ref() else {
        panic!("object")
    };
    fields.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn numeric(v: &V, expected: f64, label: &str) {
    assert!(matches!(v.as_ref(),Js::N(v)if *v==expected), "{label}");
}
fn undefined(v: &V, label: &str) {
    assert!(matches!(v.as_ref(), Js::U), "{label}");
}
fn string(v: &V, expected: &str, label: &str) {
    assert!(matches!(v.as_ref(),Js::S(v)if v==expected), "{label}");
}
fn layout() -> Value {
    let f = Fixture::new();
    let second = f.rt.construct(&f.class, f.parent.clone());
    let fields = [
        "token",
        "socketEndpoint",
        "subscribed",
        "snapshotRefreshTimer",
        "connectAndSubscribe",
        "onOpen",
        "ping",
        "onMessage",
        "refreshSnapshotSoon",
        "reconnect",
        "dispose",
        "handleOrderTopic",
        "handleAlgoOrdersTopic",
        "subscribe",
    ];
    assert_eq!(
        &keys(&second)[2..],
        fields,
        "KUCOIN-FUTURE complete field set"
    );
    let definitions =
        f.rt.definitions
            .borrow()
            .iter()
            .filter(|(r, _)| Rc::ptr_eq(r, &second))
            .map(|(_, key)| key.clone())
            .collect::<Vec<_>>();
    assert_eq!(
        definitions, fields,
        "KUCOIN-FUTURE complete fourteen definitions"
    );
    for field in ["token", "socketEndpoint"] {
        assert!(matches!(get(&second, field).unwrap().as_ref(), Js::Null));
    }
    assert!(
        matches!(get(&second, "subscribed").unwrap().as_ref(), Js::B(false)),
        "KUCOIN-FUTURE initial subscription flag"
    );
    undefined(
        &get(&second, "snapshotRefreshTimer").unwrap(),
        "KUCOIN-FUTURE initial timer",
    );
    let mut metadata = Vec::new();
    for (name, arity, asynchronous) in [
        ("connectAndSubscribe", 0, true),
        ("onOpen", 0, false),
        ("ping", 0, false),
        ("onMessage", 1, false),
        ("refreshSnapshotSoon", 0, false),
        ("reconnect", 0, true),
        ("dispose", 0, false),
        ("handleOrderTopic", 1, false),
        ("handleAlgoOrdersTopic", 1, false),
        ("subscribe", 0, false),
    ] {
        let arrow = get(&f.r, name).unwrap();
        assert!(matches!(arrow.as_ref(), Js::F(_)));
        assert!(
            !Rc::ptr_eq(&arrow, &get(&second, name).unwrap()),
            "KUCOIN-FUTURE fresh lexical arrow"
        );
        let all = f.rt.metas.borrow();
        let (_, actual, length, kind) = all
            .iter()
            .find(|(v, _, _, _)| Rc::ptr_eq(v, &arrow))
            .unwrap();
        assert_eq!(
            (actual.as_str(), *length, *kind),
            (name, arity, asynchronous),
            "KUCOIN-FUTURE native callable metadata"
        );
        metadata.push(json!([actual, length, kind]));
    }
    assert!(
        keys(&f.rt.prototypes.borrow()[0].1).is_empty(),
        "KUCOIN-FUTURE dispose is an own arrow"
    );
    json!({"fields":fields,"arrows":metadata,"prototype":[],"fresh":true})
}
fn ingress() -> Value {
    let f = Fixture::new();
    let pending = f.rt.method(&f.r, "connectAndSubscribe", vec![]).unwrap();
    assert_eq!(f.urls.borrow().len(), 1, "KUCOIN-FUTURE eager request");
    assert!(matches!(get(&f.r, "token").unwrap().as_ref(), Js::Null));
    assert_eq!(f.rt.jobs.borrow().len(), 1);
    drop(pending);
    f.resolve_token();
    string(
        &get(&f.r, "token").unwrap(),
        "token-23",
        "KUCOIN-FUTURE negotiated token",
    );
    string(
        &get(&f.r, "socketEndpoint").unwrap(),
        "wss://future.example?token=token-23&connectId=41",
        "KUCOIN-FUTURE endpoint",
    );
    assert_eq!(f.listeners.borrow().len(), 3);
    for (index, event, field) in [
        (0, "open", "onOpen"),
        (1, "message", "onMessage"),
        (2, "close", "onClose"),
    ] {
        let rows = f.listeners.borrow();
        assert_eq!(rows[index].0, event);
        assert!(
            Rc::ptr_eq(&rows[index].1, &get(&f.r, field).unwrap()),
            "KUCOIN-FUTURE actual listener identity"
        );
    }
    call(
        get(&f.r, "onOpen").unwrap(),
        obj(vec![("isDisposed", b(true))]),
        vec![],
    )
    .unwrap();
    assert!(matches!(
        get(&f.r, "subscribed").unwrap().as_ref(),
        Js::B(true)
    ));
    assert_eq!(
        f.frames.borrow().len(),
        7,
        "KUCOIN-FUTURE six subscriptions plus ping"
    );
    numeric(
        &get(&f.r, "pingAt").unwrap(),
        79.,
        "KUCOIN-FUTURE monotonic ping mark",
    );
    f.message(json!({"type":"welcome"}));
    assert_eq!(
        f.frames.borrow().len(),
        7,
        "KUCOIN-FUTURE welcome subscription guard"
    );
    set(&f.r, "subscribed", b(false));
    f.message(json!({"type":"welcome"}));
    f.message(json!({"type":"pong"}));
    f.message(json!({"topic":"/contractMarket/tradeOrders","data":{"type":"canceled","matchSize":"3","matchPrice":"11","price":"17"}}));
    f.message(json!({"topic":"/contractMarket/advancedOrders","data":{"type":"cancel"}}));
    assert_eq!(
        f.mapped.borrow().len(),
        2,
        "KUCOIN-FUTURE actual native order and algo bindings"
    );
    assert_eq!(
        f.parsed.borrow().len(),
        5,
        "KUCOIN-FUTURE independent parsed message count before account events"
    );
    for (mapped_index, message_index) in [(0, 3), (1, 4)] {
        assert!(
            Rc::ptr_eq(
                &f.mapped.borrow()[mapped_index],
                &get(&f.parsed.borrow()[message_index], "data").unwrap()
            ),
            "KUCOIN-FUTURE original raw order reference"
        );
    }
    assert_eq!(
        f.fills.borrow().len(),
        1,
        "KUCOIN-FUTURE incremental native fill"
    );
    let fill = f.fills.borrow()[0].clone();
    for (key, value) in [
        ("id", "parent-13"),
        ("side", "sell"),
        ("symbol", "BTC/USDT"),
    ] {
        string(
            &get(&fill, key).unwrap(),
            value,
            "KUCOIN-FUTURE native fill field",
        );
    }
    numeric(
        &get(&fill, "price").unwrap(),
        11.,
        "KUCOIN-FUTURE native fill price",
    );
    numeric(
        &get(&fill, "amount").unwrap(),
        21.,
        "KUCOIN-FUTURE native contract amount",
    );
    let updates = f.updates.borrow();
    assert_eq!(
        updates.len(),
        3,
        "KUCOIN-FUTURE normal siblings and algo parent"
    );
    for (index, order_index) in [(0, "0"), (1, "1"), (2, "0")] {
        assert!(
            Rc::ptr_eq(&updates[index].0, &get(&f.orders, order_index).unwrap()),
            "KUCOIN-FUTURE original native order identity"
        );
        string(
            &get(&updates[index].0, "id").unwrap(),
            if index == 1 { "child-19" } else { "parent-13" },
            "KUCOIN-FUTURE live array element before callback",
        );
        string(&updates[index].1, "canceled", "KUCOIN-FUTURE native status");
    }
    drop(updates);
    f.message(json!({"topic":"/contractAccount/wallet"}));
    f.message(json!({"topic":"/contract/positionAll"}));
    assert_eq!(
        f.rt.timers.borrow().len(),
        2,
        "KUCOIN-FUTURE debounce replacement"
    );
    f.fire(1);
    undefined(
        &get(&f.r, "snapshotRefreshTimer").unwrap(),
        "KUCOIN-FUTURE timer cleared",
    );
    assert_eq!(
        f.snapshots.get(),
        2,
        "KUCOIN-FUTURE open and timer snapshots"
    );
    assert_eq!(f.snapshot_timers.borrow().len(), 2);
    for timer in f.snapshot_timers.borrow().iter() {
        undefined(timer, "KUCOIN-FUTURE clear before snapshot");
    }
    let payloads = f.rt.stringified.borrow();
    assert_eq!(payloads.len(), 13, "KUCOIN-FUTURE native payload count");
    let topics = [
        "/contractMarket/tradeOrders",
        "/contractMarket/advancedOrders",
        "/contract/positionAll",
        "/contractAccount/wallet",
        "/contract/marginMode",
        "/contract/crossLeverage",
    ];
    for (index, payload) in payloads.iter().enumerate() {
        numeric(
            &get(payload, "id").unwrap(),
            41.,
            "KUCOIN-FUTURE native frame id",
        );
        if index == 6 {
            string(
                &get(payload, "type").unwrap(),
                "ping",
                "KUCOIN-FUTURE native ping type",
            );
            assert_eq!(keys(payload), ["id", "type"]);
        } else {
            let topic = topics[if index < 6 { index } else { index - 7 }];
            string(
                &get(payload, "type").unwrap(),
                "subscribe",
                "KUCOIN-FUTURE native subscription type",
            );
            string(
                &get(payload, "topic").unwrap(),
                topic,
                "KUCOIN-FUTURE native subscription topic",
            );
            for key in ["privateChannel", "response"] {
                assert!(
                    matches!(get(payload, key).unwrap().as_ref(), Js::B(true)),
                    "KUCOIN-FUTURE native subscription boolean"
                );
            }
            assert_eq!(
                keys(payload),
                ["id", "type", "topic", "privateChannel", "response"]
            );
        }
    }
    string(
        &f.urls.borrow()[0],
        "/api/v1/bullet-private",
        "KUCOIN-FUTURE endpoint table binding",
    );
    let statuses = f
        .updates
        .borrow()
        .iter()
        .map(|(order, status)| json!([text(&get(order, "id").unwrap()), text(status)]))
        .collect::<Vec<_>>();
    json!({"url":text(&get(&f.r,"socketEndpoint").unwrap()),"frames":f.frames.borrow().iter().map(text).collect::<Vec<_>>(),"fill":literal(&fill),"statuses":statuses,"snapshots":f.snapshots.get(),"trace":f.rt.trace.borrow().clone()})
}
fn reconnect_and_dispose() -> Value {
    let f = Fixture::new();
    let old = call(
        get(&f.r, "createWebsocket").unwrap(),
        f.r.clone(),
        vec![s("previous")],
    )
    .unwrap();
    set(&f.r, "ws", old);
    set(&f.r, "token", s("previous-token"));
    set(&f.r, "socketEndpoint", s("previous-endpoint"));
    set(&f.r, "subscribed", b(true));
    let outer = f.rt.method(&f.r, "reconnect", vec![]).unwrap();
    assert_eq!(
        f.removed.borrow().len(),
        3,
        "KUCOIN-FUTURE eager listener removal"
    );
    for (index, event, field) in [
        (0, "open", "onOpen"),
        (1, "message", "onMessage"),
        (2, "close", "onClose"),
    ] {
        let rows = f.removed.borrow();
        assert_eq!(rows[index].0, event);
        assert!(
            Rc::ptr_eq(&rows[index].1, &get(&f.r, field).unwrap()),
            "KUCOIN-FUTURE remove actual arrows"
        );
    }
    undefined(
        &get(&f.r, "ws").unwrap(),
        "KUCOIN-FUTURE cleared socket while awaiting token",
    );
    assert!(matches!(get(&f.r, "token").unwrap().as_ref(), Js::Null));
    assert!(matches!(
        get(&f.r, "socketEndpoint").unwrap().as_ref(),
        Js::Null
    ));
    assert!(matches!(
        get(&f.r, "subscribed").unwrap().as_ref(),
        Js::B(false)
    ));
    assert_eq!(
        f.urls.borrow().len(),
        1,
        "KUCOIN-FUTURE reconnect invokes actual native connect eagerly"
    );
    assert_eq!(
        f.rt.jobs.borrow().len(),
        2,
        "KUCOIN-FUTURE retain outer and inner promises"
    );
    assert!(
        matches!(poll(&outer), Poll::Pending),
        "KUCOIN-FUTURE reconnect waits for inner completion"
    );
    let retained = Rc::downgrade(&outer);
    drop(outer);
    f.resolve_token();
    let completion = match poll(&retained.upgrade().expect("retained reconnect")) {
        Poll::Ready(Ok(v)) => v,
        _ => panic!("KUCOIN-FUTURE reconnect completion"),
    };
    undefined(&completion, "KUCOIN-FUTURE reconnect completion value");
    assert_eq!(
        f.listeners.borrow().len(),
        3,
        "KUCOIN-FUTURE dropped reconnect still installs socket"
    );
    string(
        &get(&f.r, "token").unwrap(),
        "token-23",
        "KUCOIN-FUTURE reconnect completion",
    );
    let other = f.rt.construct(&f.class, f.parent.clone());
    let replacement_rt = f.rt.clone();
    let parent = obj(vec![(
        "dispose",
        function(move |r, _| {
            replacement_rt
                .trace
                .borrow_mut()
                .push("replacement.dispose".into());
            set(&r, "isDisposed", b(true));
            Ok(n(73.))
        }),
    )]);
    f.rt.prototype_parents.borrow_mut()[0].1 = parent;
    set(&f.r, "snapshotRefreshTimer", n(31.));
    let result = call(get(&f.r, "dispose").unwrap(), other.clone(), vec![]).unwrap();
    undefined(&result, "KUCOIN-FUTURE ignore super return");
    assert!(
        matches!(get(&f.r, "isDisposed").unwrap().as_ref(), Js::B(true)),
        "KUCOIN-FUTURE lexical dispose receiver"
    );
    assert!(matches!(
        get(&other, "isDisposed").unwrap().as_ref(),
        Js::B(false)
    ));
    undefined(
        &get(&f.r, "snapshotRefreshTimer").unwrap(),
        "KUCOIN-FUTURE clear disposal timer",
    );
    assert_eq!(
        f.rt.trace.borrow().last().unwrap(),
        "replacement.dispose",
        "KUCOIN-FUTURE live class home"
    );
    f.rt.method(&f.r, "refreshSnapshotSoon", vec![]).unwrap();
    assert_eq!(
        f.rt.timers.borrow().len(),
        1,
        "KUCOIN-FUTURE disposed refresh still schedules"
    );
    f.fire(0);
    undefined(
        &get(&f.r, "snapshotRefreshTimer").unwrap(),
        "KUCOIN-FUTURE disposed timer clears before guard",
    );
    assert_eq!(
        f.snapshots.get(),
        0,
        "KUCOIN-FUTURE disposed callback skips snapshot"
    );
    let failure = Rc::new("clear-failure".to_owned());
    let thrown = failure.clone();
    f.rt.globals.borrow_mut().insert(
        "clearTimeout".into(),
        function(move |_, _| Err(thrown.clone())),
    );
    set(&f.r, "snapshotRefreshTimer", n(37.));
    let error = match f.rt.method(&f.r, "dispose", vec![]) {
        Err(e) => e,
        Ok(_) => panic!("KUCOIN-FUTURE original clear error"),
    };
    assert!(
        Rc::ptr_eq(&error, &failure),
        "KUCOIN-FUTURE original error identity"
    );
    numeric(
        &get(&f.r, "snapshotRefreshTimer").unwrap(),
        37.,
        "KUCOIN-FUTURE failed clear retains timer",
    );
    json!({"removed":["open","message","close"],"retained":true,"disposed":true,"otherDisposed":false,"throwIdentity":true,"throwTimer":37,"snapshots":f.snapshots.get(),"trace":f.rt.trace.borrow().clone()})
}
#[test]
fn native_futures_bindings_before_maintained_source() {
    let cases = [
        ("layout", layout()),
        ("ingress", ingress()),
        ("reconnect", reconnect_and_dispose()),
    ];
    for (kind, mut capture) in cases {
        if kind == "ingress" {
            capture["frames"] = json!(
                capture["frames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| serde_json::from_str::<Value>(v.as_str().unwrap()).unwrap())
                    .collect::<Vec<_>>()
            );
        }
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/futures-private-class.cjs"
            ))
            .arg(kind)
            .output()
            .expect("maintained source process");
        assert!(
            output.status.success(),
            "source oracle failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let expected: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(capture, expected, "KUCOIN-FUTURE maintained class {kind}");
    }
}
