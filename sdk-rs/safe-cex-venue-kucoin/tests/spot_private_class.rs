#[path = "support/spot_private_values.rs"]
mod values;
use safe_cex_venue_kucoin::spot_private_class::*;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
use values::*;
#[path = "support/private_class_runtime.rs"]
mod runtime;
use runtime::*;
struct Fixture {
    rt: Rc<Runtime>,
    class: V,
    r: V,
    parent: V,
    balance: V,
    post: V,
    updates: Rc<RefCell<Vec<V>>>,
    frames: Rc<RefCell<Vec<V>>>,
    listeners: Rc<RefCell<Vec<(String, V)>>>,
    order_calls: Rc<Cell<u32>>,
    balance_calls: Rc<Cell<u32>>,
    post_urls: Rc<RefCell<Vec<V>>>,
}
impl Fixture {
    fn new() -> Self {
        let rt = Rc::new(Runtime::default());
        *rt.expected_class.borrow_mut() = "KucoinSpotPrivateWebsocket".into();
        let super_rt = rt.clone();
        let base = obj(vec![(
            "prototype",
            obj(vec![(
                "dispose",
                function(move |r, a| {
                    assert!(a.is_empty());
                    super_rt.trace.borrow_mut().push("super.dispose".into());
                    set(&r, "isDisposed", b(true));
                    Ok(n(97.))
                }),
            )]),
        )]);
        rt.globals.borrow_mut().insert("BaseWebSocket".into(), base);
        rt.globals
            .borrow_mut()
            .insert("ENABLE_RECONNECTING_WEBSOCKET".into(), b(true));
        rt.globals.borrow_mut().insert(
            "performance".into(),
            obj(vec![("now", function(|_, _| Ok(n(79.))))]),
        );
        rt.globals.borrow_mut().insert(
            "Date".into(),
            obj(vec![("now", function(|_, _| Ok(n(41.))))]),
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
            "jsonParse".into(),
            function(|_, a| {
                Ok(serde_json::from_str::<Value>(&text(&a[0]))
                    .map(|v| decode(&v))
                    .unwrap_or_else(|_| Rc::new(Js::Null)))
            }),
        );
        let table = obj(safe_cex_venue_kucoin::spot_types::KUCOIN_SPOT_ENDPOINTS
            .iter()
            .map(|(k, v)| (*k, s(v)))
            .collect());
        rt.globals
            .borrow_mut()
            .insert("KUCOIN_SPOT_ENDPOINTS".into(), table);
        let timeout_rt = rt.clone();
        rt.globals.borrow_mut().insert(
            "setTimeout".into(),
            function(move |_, a| {
                assert_eq!(a.len(), 2);
                assert_eq!(number(&a[1]), 250., "KUCOIN-BOUND timer delay");
                let id = timeout_rt.next_timer.get() + 1;
                timeout_rt.next_timer.set(id);
                timeout_rt
                    .timers
                    .borrow_mut()
                    .push((id as f64, a[0].clone()));
                Ok(n(id as f64))
            }),
        );
        let clear_rt = rt.clone();
        rt.globals.borrow_mut().insert(
            "clearTimeout".into(),
            function(move |_, a| {
                clear_rt
                    .trace
                    .borrow_mut()
                    .push(format!("clear:{}", text(&a[0])));
                Ok(u())
            }),
        );
        let balance = pending();
        let post = pending();
        let post_ref = post.clone();
        let post_urls = Rc::new(RefCell::new(Vec::new()));
        let urls = post_urls.clone();
        let order_calls = Rc::new(Cell::new(0));
        let orders = order_calls.clone();
        let balance_calls = Rc::new(Cell::new(0));
        let balances = balance_calls.clone();
        let balance_ref = balance.clone();
        let updates = Rc::new(RefCell::new(Vec::new()));
        let updated = updates.clone();
        let snapshot_rt = rt.clone();
        let error_rt = rt.clone();
        let store = obj(vec![
            ("loaded", obj(vec![("orders", n(43.))])),
            (
                "update",
                function(move |_, a| {
                    updated.borrow_mut().push(a[0].clone());
                    Ok(u())
                }),
            ),
        ]);
        let parent = obj(vec![
            ("store", store),
            (
                "xhr",
                obj(vec![(
                    "post",
                    function(move |_, a| {
                        urls.borrow_mut().extend(a);
                        Ok(post_ref.clone())
                    }),
                )]),
            ),
            (
                "fetchBalance",
                function(move |_, a| {
                    assert!(a.is_empty());
                    balances.set(balances.get() + 1);
                    Ok(balance_ref.clone())
                }),
            ),
            (
                "fetchAndSetOrders",
                function(move |_, a| {
                    assert!(a.is_empty());
                    orders.set(orders.get() + 1);
                    Ok(u())
                }),
            ),
            (
                "fetchAndSetPrivateSnapshot",
                function(move |_, a| {
                    snapshot_rt
                        .trace
                        .borrow_mut()
                        .push(format!("snapshot:{}", text(&a[0])));
                    Ok(u())
                }),
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
        let class = define_spot_private_class(rt.operations()).unwrap();
        let r = rt.construct(&class, parent.clone());
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
        let listened = listeners.clone();
        let socket = obj(vec![(
            "addEventListener",
            function(move |_, a| {
                listened.borrow_mut().push((text(&a[0]), a[1].clone()));
                Ok(u())
            }),
        )]);
        let socket_rt = rt.clone();
        set(
            &r,
            "createWebsocket",
            function(move |_, a| {
                socket_rt
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
            let tr = rt.clone();
            set(
                &r,
                key,
                function(move |_, a| {
                    tr.trace.borrow_mut().push(
                        if [
                            "handlePongEvent",
                            "recordMessageHandlerStats",
                            "retryFailedConnect",
                        ]
                        .contains(&key)
                        {
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
            balance,
            post,
            updates,
            frames,
            listeners,
            order_calls,
            balance_calls,
            post_urls,
        }
    }
    fn fire(&self, index: usize) -> V {
        let f = self.rt.timers.borrow()[index].1.clone();
        call(f, u(), vec![]).unwrap()
    }
    fn dispose(&self, r: &V) -> Result<V, E> {
        let proto = self.rt.prototypes.borrow()[0].1.clone();
        call(get(&proto, "dispose")?, r.clone(), vec![])
    }
}
fn own_keys(v: &V) -> Vec<String> {
    let Js::O(fields) = v.as_ref() else {
        panic!("object")
    };
    fields.borrow().iter().map(|(k, _)| k.clone()).collect()
}
fn assert_number(v: &V, expected: f64, label: &str) {
    assert!(
        matches!(v.as_ref(),Js::N(n) if *n==expected),
        "{label}: native Number"
    );
}
fn assert_undefined(v: &V, label: &str) {
    assert!(matches!(v.as_ref(), Js::U), "{label}: native undefined");
}
fn source(case: &str, native: Value) {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/spot-private-class.cjs"
        ))
        .arg(case)
        .output()
        .expect("maintained source process");
    assert!(
        output.status.success(),
        "source oracle failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Value = serde_json::from_slice(&output.stdout).expect("source oracle JSON");
    assert_eq!(native, expected, "KUCOIN-BOUND maintained class {case}");
}
fn native_layout_and_fresh_lexical_arrows() -> Value {
    let f = Fixture::new();
    let second = f.rt.construct(&f.class, f.parent.clone());
    let fields = [
        "token",
        "socketEndpoint",
        "pingInterval",
        "orderRefreshTimeout",
        "balanceRefreshTimeout",
        "connectAndSubscribe",
        "onOpen",
        "ping",
        "onMessage",
        "handleOrderTopic",
        "subscribe",
        "refreshOrdersSoon",
        "refreshBalanceSoon",
    ];
    assert_eq!(
        &own_keys(&second)[2..],
        fields,
        "KUCOIN-BOUND complete clean-instance field set"
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
        "KUCOIN-BOUND complete thirteen field definitions"
    );
    for key in ["token", "socketEndpoint"] {
        assert!(
            matches!(get(&f.r, key).unwrap().as_ref(), Js::Null),
            "KUCOIN-BOUND null field"
        );
    }
    assert_number(
        &get(&f.r, "pingInterval").unwrap(),
        5000.,
        "KUCOIN-BOUND initial interval",
    );
    for key in ["orderRefreshTimeout", "balanceRefreshTimeout"] {
        assert_undefined(&get(&f.r, key).unwrap(), "KUCOIN-BOUND initial timer");
    }
    let expected = [
        ("connectAndSubscribe", 0, true),
        ("onOpen", 0, false),
        ("ping", 0, false),
        ("onMessage", 1, false),
        ("handleOrderTopic", 1, false),
        ("subscribe", 0, false),
        ("refreshOrdersSoon", 0, false),
        ("refreshBalanceSoon", 0, false),
    ];
    let mut metadata = Vec::new();
    for (key, arity, asynchronous) in expected {
        let value = get(&f.r, key).unwrap();
        assert!(
            matches!(value.as_ref(), Js::F(_)),
            "KUCOIN-BOUND actual callable"
        );
        assert!(
            !Rc::ptr_eq(&value, &get(&second, key).unwrap()),
            "KUCOIN-BOUND fresh arrow identity"
        );
        let all = f.rt.metas.borrow();
        let (_, name, length, kind) = all
            .iter()
            .find(|(v, _, _, _)| Rc::ptr_eq(v, &value))
            .unwrap();
        assert_eq!(
            (name.as_str(), *length, *kind),
            (key, arity, asynchronous),
            "KUCOIN-BOUND callable metadata"
        );
        metadata.push(json!([name, length, kind]));
    }
    let proto = f.rt.prototypes.borrow()[0].1.clone();
    assert_eq!(own_keys(&proto), ["dispose"]);
    let dispose = get(&proto, "dispose").unwrap();
    assert!(matches!(dispose.as_ref(), Js::F(_)));
    let all = f.rt.metas.borrow();
    let (_, name, arity, asynchronous) = all
        .iter()
        .find(|(v, _, _, _)| Rc::ptr_eq(v, &dispose))
        .unwrap();
    assert_eq!(
        (name.as_str(), *arity, *asynchronous),
        ("dispose", 0, false),
        "KUCOIN-BOUND prototype callable metadata"
    );

    json!({"fields":fields,"arrows":metadata,"prototype":["dispose"],"prototypeMeta":["dispose",0,false],"fresh":true,"interval":5000})
}
fn native_connect_open_ping_and_message_share_live_receiver() -> Value {
    let f = Fixture::new();
    let connect = f.rt.method(&f.r, "connectAndSubscribe", vec![]).unwrap();
    assert_eq!(
        f.post_urls.borrow().len(),
        1,
        "KUCOIN-BOUND eager token request"
    );
    assert!(matches!(get(&f.r, "token").unwrap().as_ref(), Js::Null));
    assert!(
        Rc::ptr_eq(&f.rt.jobs.borrow()[0], &connect),
        "KUCOIN-BOUND retained connect task"
    );
    drop(connect);
    resolve(
        &f.post,
        Ok(decode(
            &json!({"data":{"code":"200000","data":{"token":"token-23","instanceServers":[{"endpoint":"wss://socket.example","pingInterval":61}]}}}),
        )),
    );
    f.rt.pump();
    assert!(
        matches!(get(&f.r,"token").unwrap().as_ref(),Js::S(v)if v=="token-23"),
        "KUCOIN-BOUND native token"
    );
    assert_number(
        &get(&f.r, "pingInterval").unwrap(),
        61.,
        "KUCOIN-BOUND negotiated interval",
    );
    assert_eq!(
        text(&get(&f.r, "socketEndpoint").unwrap()),
        "wss://socket.example?token=token-23&connectId=41"
    );
    assert_eq!(f.listeners.borrow().len(), 3, "KUCOIN-BOUND listener count");
    for (index, key, event) in [
        (0, "onOpen", "open"),
        (1, "onMessage", "message"),
        (2, "onClose", "close"),
    ] {
        let all = f.listeners.borrow();
        assert_eq!(all[index].0, event);
        assert!(
            Rc::ptr_eq(&all[index].1, &get(&f.r, key).unwrap()),
            "KUCOIN-BOUND listener native arrow"
        );
    }
    f.rt.method(&f.r, "onOpen", vec![]).unwrap();
    assert_eq!(
        f.frames.borrow().len(),
        3,
        "KUCOIN-BOUND open sends two subscriptions and ping"
    );
    let payloads = f.rt.stringified.borrow().clone();
    assert_eq!(payloads.len(), 3, "KUCOIN-BOUND native payload count");
    for (index, payload) in payloads.iter().enumerate() {
        assert_number(
            &get(payload, "id").unwrap(),
            41.,
            "KUCOIN-BOUND native payload id",
        );
        let kind = get(payload, "type").unwrap();
        assert!(
            matches!(kind.as_ref(),Js::S(v)if v==if index<2{"subscribe"}else{"ping"}),
            "KUCOIN-BOUND native payload type"
        );
        if index < 2 {
            let topic = get(payload, "topic").unwrap();
            assert!(
                matches!(topic.as_ref(),Js::S(v)if v==["/spotMarket/tradeOrdersV2","/account/balance"][index]),
                "KUCOIN-BOUND native payload topic"
            );
            for key in ["privateChannel", "response"] {
                assert!(
                    matches!(get(payload, key).unwrap().as_ref(), Js::B(true)),
                    "KUCOIN-BOUND native payload boolean"
                );
            }
            assert_eq!(
                own_keys(payload),
                ["id", "type", "topic", "privateChannel", "response"],
                "KUCOIN-BOUND native subscription field order"
            );
        } else {
            assert_eq!(
                own_keys(payload),
                ["id", "type"],
                "KUCOIN-BOUND native heartbeat field order"
            );
        }
    }
    assert_number(
        &get(&f.r, "pingAt").unwrap(),
        79.,
        "KUCOIN-BOUND monotonic mark",
    );
    let live = Rc::new(Cell::new(0));
    let sent = live.clone();
    let r = f.r.clone();
    f.rt.setters.borrow_mut().push((
        f.r.clone(),
        "pingAt".into(),
        Rc::new(move |v| {
            set(&r, "pingAt", v);
            let sent = sent.clone();
            set(
                &r,
                "sendWithLog",
                function(move |_, _| {
                    sent.set(sent.get() + 1);
                    Ok(u())
                }),
            );
            Ok(())
        }),
    ));
    call(
        get(&f.r, "ping").unwrap(),
        obj(vec![("isDisposed", b(true))]),
        vec![],
    )
    .unwrap();
    assert_eq!(
        live.get(),
        1,
        "KUCOIN-BOUND live send after pingAt setter and lexical receiver"
    );
    set(&f.r, "isDisposed", b(true));
    let r = f.r.clone();
    let input = obj(vec![]);
    f.rt.getters.borrow_mut().push((
        input.clone(),
        "data".into(),
        Rc::new(move || {
            set(&r, "isDisposed", b(false));
            Ok(s("{\"type\":\"pong\"}"))
        }),
    ));
    f.rt.method(&f.r, "onMessage", vec![input]).unwrap();
    assert_eq!(
        f.rt.trace.borrow().last().unwrap(),
        "handlePongEvent:61",
        "KUCOIN-BOUND destructuring before disposal guard"
    );
    for topic in ["/spotMarket/tradeOrdersV2-extra", "/account/balance-extra"] {
        f.rt.method(
            &f.r,
            "onMessage",
            vec![obj(vec![(
                "data",
                s(&json!({"topic":topic,"data":{}}).to_string()),
            )])],
        )
        .unwrap();
    }
    assert_number(
        &get(&f.r, "orderRefreshTimeout").unwrap(),
        1.,
        "KUCOIN-BOUND real order-topic calls real order timer",
    );
    assert_number(
        &get(&f.r, "balanceRefreshTimeout").unwrap(),
        2.,
        "KUCOIN-BOUND real balance timer",
    );
    assert_eq!(
        f.post_urls.borrow().iter().map(text).collect::<Vec<_>>(),
        ["/api/v1/bullet-private"]
    );
    let frames = f.frames.borrow().iter().map(text).collect::<Vec<_>>();
    json!({"url":text(&get(&f.r,"socketEndpoint").unwrap()),"interval":61,"frames":frames,"listeners":["open","message","close"],"liveSend":live.get(),"timers":[1,2],"trace":f.rt.trace.borrow().clone()})
}
fn native_timers_coalesce_clear_and_retain_completion() -> Value {
    let f = Fixture::new();
    for key in ["refreshOrdersSoon", "refreshBalanceSoon"] {
        f.rt.method(&f.r, key, vec![]).unwrap();
        f.rt.method(&f.r, key, vec![]).unwrap();
    }
    assert_eq!(
        f.rt.timers.borrow().len(),
        2,
        "KUCOIN-BOUND coalesced timers"
    );
    let order_task = f.fire(0);
    let balance_task = f.fire(1);
    for key in ["orderRefreshTimeout", "balanceRefreshTimeout"] {
        assert_undefined(
            &get(&f.r, key).unwrap(),
            "KUCOIN-BOUND timer cleared before await",
        );
    }
    assert_eq!(
        (f.order_calls.get(), f.balance_calls.get()),
        (1, 1),
        "KUCOIN-BOUND eager fetch before callback returns"
    );
    assert_eq!(f.updates.borrow().len(), 0);
    let returned = Rc::downgrade(&balance_task);
    drop(balance_task);
    drop(order_task);
    assert!(
        returned.upgrade().is_some(),
        "KUCOIN-BOUND task retained after caller drop"
    );
    let balance = obj(vec![("used", n(17.)), ("free", n(29.))]);
    resolve(&f.balance, Ok(balance.clone()));
    f.rt.pump();
    assert_eq!(
        f.updates.borrow().len(),
        1,
        "KUCOIN-BOUND retained balance completion"
    );
    let update = f.updates.borrow()[0].clone();
    assert!(
        Rc::ptr_eq(&get(&update, "balance").unwrap(), &balance),
        "KUCOIN-BOUND native balance identity"
    );
    let loaded = get(&update, "loaded").unwrap();
    assert_number(
        &get(&loaded, "orders").unwrap(),
        43.,
        "KUCOIN-BOUND loaded spread",
    );
    assert!(
        matches!(get(&loaded, "balance").unwrap().as_ref(), Js::B(true)),
        "KUCOIN-BOUND loaded balance flag"
    );
    assert_eq!(own_keys(&loaded), ["orders", "balance"]);
    let disposed = Fixture::new();
    disposed
        .rt
        .method(&disposed.r, "refreshBalanceSoon", vec![])
        .unwrap();
    drop(disposed.fire(0));
    set(&disposed.r, "isDisposed", b(true));
    resolve(&disposed.balance, Ok(n(97.)));
    disposed.rt.pump();
    assert_eq!(
        disposed.updates.borrow().len(),
        0,
        "KUCOIN-BOUND disposal after await suppresses update"
    );
    json!({"timers":2,"fetches":[1,1],"cleared":true,"retained":true,"update":literal(&update),"disposedUpdates":0})
}
fn native_dispose_keeps_home_dynamic_receiver_and_error_identity() -> Value {
    let f = Fixture::new();
    let other = f.rt.construct(&f.class, f.parent.clone());
    set(&other, "orderRefreshTimeout", n(101.));
    set(&other, "balanceRefreshTimeout", n(103.));
    let count = Rc::new(Cell::new(0));
    let reads = count.clone();
    let active = Rc::new(Cell::new(true));
    let is_active = active.clone();
    f.rt.getters.borrow_mut().push((
        other.clone(),
        "orderRefreshTimeout".into(),
        Rc::new(move || {
            if !is_active.get() {
                return Ok(u());
            }
            reads.set(reads.get() + 1);
            Ok(n(if reads.get() == 1 { 101. } else { 107. }))
        }),
    ));
    let timer = active.clone();
    let backing = other.clone();
    f.rt.setters.borrow_mut().push((
        other.clone(),
        "orderRefreshTimeout".into(),
        Rc::new(move |v| {
            timer.set(!matches!(v.as_ref(), Js::U));
            set(&backing, "orderRefreshTimeout", v);
            Ok(())
        }),
    ));
    let result = f.dispose(&other).unwrap();
    assert_undefined(&result, "KUCOIN-BOUND ignored superclass return");
    assert_eq!(count.get(), 2, "KUCOIN-BOUND repeated live timer reads");
    assert!(
        matches!(get(&other, "isDisposed").unwrap().as_ref(), Js::B(true)),
        "KUCOIN-BOUND actual method receiver"
    );
    assert!(
        matches!(get(&f.r, "isDisposed").unwrap().as_ref(), Js::B(false)),
        "KUCOIN-BOUND lexical instance untouched"
    );
    assert_eq!(
        f.rt.trace.borrow().as_slice(),
        ["clear:107", "clear:103", "super.dispose"],
        "KUCOIN-BOUND clear before super ordering"
    );
    for key in ["orderRefreshTimeout", "balanceRefreshTimeout"] {
        assert_undefined(
            &f.rt.read(&other, key).unwrap(),
            "KUCOIN-BOUND disposed native timer",
        );
    }
    let replacement = f.rt.construct(&f.class, f.parent.clone());
    let trace = f.rt.clone();
    let parent = obj(vec![(
        "dispose",
        function(move |receiver, _| {
            trace.trace.borrow_mut().push("replacement.dispose".into());
            set(&receiver, "isDisposed", b(true));
            Ok(n(73.))
        }),
    )]);
    f.rt.prototype_parents.borrow_mut()[0].1 = parent;
    assert_undefined(
        &f.dispose(&replacement).unwrap(),
        "KUCOIN-BOUND replacement return ignored",
    );
    assert!(
        matches!(
            get(&replacement, "isDisposed").unwrap().as_ref(),
            Js::B(true)
        ),
        "KUCOIN-BOUND live prototype parent and receiver"
    );
    assert_eq!(
        f.rt.trace.borrow().last().unwrap(),
        "replacement.dispose",
        "KUCOIN-BOUND live super home"
    );
    let original = Rc::new("clear-failure".to_owned());
    let thrown = original.clone();
    f.rt.globals.borrow_mut().insert(
        "clearTimeout".into(),
        function(move |_, _| Err(thrown.clone())),
    );
    set(&f.r, "orderRefreshTimeout", n(109.));
    let error = match f.dispose(&f.r) {
        Err(e) => e,
        Ok(_) => panic!("KUCOIN-BOUND original clear throw"),
    };
    assert!(
        Rc::ptr_eq(&error, &original),
        "KUCOIN-BOUND original thrown identity"
    );
    assert_number(
        &get(&f.r, "orderRefreshTimeout").unwrap(),
        109.,
        "KUCOIN-BOUND no clear after failed call",
    );
    json!({"trace":f.rt.trace.borrow().clone(),"reads":count.get(),"disposed":true,"originalUntouched":true,"resultUndefined":true,"throwIdentity":true,"throwTimer":109})
}

#[test]
fn native_bindings_before_maintained_source() {
    let mut captures = [
        ("layout", native_layout_and_fresh_lexical_arrows()),
        (
            "ingress",
            native_connect_open_ping_and_message_share_live_receiver(),
        ),
        (
            "timers",
            native_timers_coalesce_clear_and_retain_completion(),
        ),
        (
            "dispose",
            native_dispose_keeps_home_dynamic_receiver_and_error_identity(),
        ),
    ];
    for (kind, capture) in &mut captures {
        if *kind == "ingress" {
            let frames = capture["frames"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| serde_json::from_str::<Value>(v.as_str().unwrap()).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                frames,
                vec![
                    json!({"id":41,"type":"subscribe","topic":"/spotMarket/tradeOrdersV2","privateChannel":true,"response":true}),
                    json!({"id":41,"type":"subscribe","topic":"/account/balance","privateChannel":true,"response":true}),
                    json!({"id":41,"type":"ping"})
                ],
                "KUCOIN-BOUND wire payload after native assertions"
            );
            capture["frames"] = json!(frames);
        }
        source(kind, capture.clone());
    }
}
