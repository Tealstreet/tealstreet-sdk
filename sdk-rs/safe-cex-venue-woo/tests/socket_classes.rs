#[path = "support/lifecycle.rs"]
mod fixture;
#[path = "support/socket_classes.rs"]
mod frames;
use fixture::*;
use frames::Frame;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_venue_woo::{public_streams::StreamOps, socket_classes::*};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
type Meta = (V, String, usize, bool);
struct Harness {
    runtime: Rc<Runtime>,
    public: V,
    private: V,
    metas: Rc<RefCell<Vec<Meta>>>,
    definitions: Rc<RefCell<Vec<(V, String)>>>,
    args: Rc<RefCell<Vec<Vec<V>>>>,
    trace: Rc<RefCell<Vec<String>>>,
    payloads: Rc<RefCell<Vec<V>>>,
    views: Rc<RefCell<Vec<(V, Rc<Value>)>>>,
    store: Rc<DefaultStore>,
    error: E,
    destructure: Rc<RefCell<Option<(V, V, V)>>>,
}
fn meta(m: &Rc<RefCell<Vec<Meta>>>, v: &V) -> (String, usize, bool) {
    m.borrow()
        .iter()
        .find(|(f, _, _, _)| Rc::ptr_eq(f, v))
        .map(|(_, n, a, k)| (n.clone(), *a, *k))
        .unwrap()
}
impl Harness {
    fn new() -> Self {
        let runtime = Rc::new(Runtime::default());
        let mut life = runtime.operations();
        let metas = Rc::new(RefCell::new(vec![]));
        let definitions = Rc::new(RefCell::new(vec![]));
        let args = Rc::new(RefCell::new(vec![]));
        let trace = Rc::new(RefCell::new(vec![]));
        let payloads = Rc::new(RefCell::new(vec![]));
        let views = Rc::new(RefCell::new(Vec::<(V, Rc<Value>)>::new()));
        let destructure = Rc::new(RefCell::new(None::<(V, V, V)>));
        let error: E = Rc::new("disposed getter sentinel".into());
        let ops = Rc::get_mut(&mut life).unwrap();
        let value = Rc::get_mut(&mut ops.value).unwrap();
        let m = metas.clone();
        value.function = Rc::new(move |name, arity, body| {
            let f = function(move |_, a| body(a));
            m.borrow_mut()
                .push((f.clone(), name.to_owned(), arity, false));
            f
        });
        let d = destructure.clone();
        let e = error.clone();
        value.get = Rc::new(move |v, k| {
            if k == "isDisposed" && truthy(&get(v, "throwDisposed")?) {
                return Err(e.clone());
            }
            if k == "data" {
                if let Some((target, receiver, replacement)) = d.borrow().as_ref() {
                    if Rc::ptr_eq(target, v) {
                        set(receiver, "handleBBOStreamEvents", replacement.clone());
                    }
                }
            }
            get(v, k)
        });
        let t = trace.clone();
        let p = payloads.clone();
        value.global = Rc::new(move |name| {
            Ok(match name {
                "performance" => obj(vec![(
                    "now",
                    function({
                        let t = t.clone();
                        move |_, _| {
                            t.borrow_mut().push("now".into());
                            Ok(n(79.))
                        }
                    }),
                )]),
                "JSON" => obj(vec![(
                    "stringify",
                    function({
                        let t = t.clone();
                        let p = p.clone();
                        move |_, a| {
                            assert!(
                                matches!(a[0].as_ref(), Js::O(_)),
                                "WOO-CLASS native outgoing object"
                            );
                            assert!(
                                matches!(get(&a[0], "event")?.as_ref(), Js::S(_)),
                                "WOO-CLASS native outgoing event"
                            );
                            p.borrow_mut().push(a[0].clone());
                            t.borrow_mut().push("stringify".into());
                            let encoded = safe_cex_num::js_json_stringify(
                                &safe_cex_num::JsJsonValue::from(literal(&a[0])),
                            )
                            .unwrap()
                            .unwrap();
                            Ok(s(&encoded))
                        }
                    }),
                )]),
                "Array" => obj(vec![(
                    "isArray",
                    function(|_, a| Ok(b(matches!(a[0].as_ref(), Js::A(_))))),
                )]),
                _ => panic!("global outside fixture: {name}"),
            })
        });
        let base = function(|_, _| panic!("derive owns standard base construction fixture"));
        runtime
            .imports
            .borrow_mut()
            .insert("BaseWebSocket".into(), base.clone());
        let setter_trace = trace.clone();
        let stream = Rc::new(StreamOps {
            lifecycle: life,
            set: Rc::new(move |r, k, v| {
                let key = text(&k);
                if key == "pingAt" {
                    setter_trace.borrow_mut().push("set:pingAt".into());
                    if !matches!(get(&r, "replacementSend")?.as_ref(), Js::U) {
                        set(&r, "send", get(&r, "replacementSend")?);
                    }
                }
                set(&r, &key, v);
                Ok(())
            }),
            define: Rc::new({
                let d = definitions.clone();
                move |r, k, v| {
                    d.borrow_mut().push((r.clone(), k.into()));
                    set(&r, k, v);
                    Ok(())
                }
            }),
            delete: Rc::new(|_, _| panic!("cleanup outside class fixture")),
            divide: Rc::new(|a, b| Ok(n(number(&a) / number(&b)))),
            greater: Rc::new(|a, b| Ok(number(&a) > number(&b))),
            template: Rc::new(|_, _| panic!("listener interpolation outside class fixture")),
            null: Rc::new(|| Rc::new(Js::Null)),
        });
        let a = args.clone();
        let expected_base = base.clone();
        let derive = Rc::new(
            move |name: &str, super_value: V, arity: usize, initialize: Initializer<V, E>| {
                assert!(
                    Rc::ptr_eq(&super_value, &expected_base),
                    "WOO-CLASS native superclass identity"
                );
                assert_eq!(arity, 0);
                assert!(["WooPublicWebsocket", "WooPrivateWebscoket"].contains(&name));
                let a = a.clone();
                Ok(function(move |_, arguments| {
                    a.borrow_mut().push(arguments.clone());
                    let r = obj(vec![
                        ("parent", arguments.first().cloned().unwrap_or_else(u)),
                        ("isDisposed", b(true)),
                    ]);
                    initialize(r.clone())?;
                    Ok(r)
                }))
            },
        );
        let store = Rc::new(DefaultStore::new());
        let ticker=serde_json::from_value(json!({"id":"ticker-59","symbol":"BTCUSDT","bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"openInterest":7,"fundingRate":8,"fundingTime":9,"volume":10,"quoteVolume":20})).unwrap();
        Store::update(
            store.as_ref(),
            StoreUpdate {
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let json: Rc<dyn Fn(&V) -> Result<Rc<Value>, E>> = Rc::new({
            let views = views.clone();
            move |v: &V| match v.as_ref() {
                Js::Null => Ok(Rc::new(Value::Null)),
                Js::B(v) => Ok(Rc::new(json!(v))),
                Js::N(v) => Ok(Rc::new(json!(v))),
                Js::S(v) => Ok(Rc::new(json!(v))),
                _ => Ok(views
                    .borrow()
                    .iter()
                    .find(|(input, _)| Rc::ptr_eq(input, v))
                    .expect("existing JSON payload slot")
                    .1
                    .clone()),
            }
        });
        let operations = Rc::new(SocketClassOps {
            stream: stream.clone(),
            derive,
            frame: Rc::new({
                let ops = stream.clone();
                move |r: &V| Frame {
                    receiver: r.clone(),
                    ops: ops.clone(),
                }
            }),
            store: Rc::new({
                let store = store.clone();
                move |_| store.clone() as Rc<dyn Store>
            }),
            json: json.clone(),
            ticker_data: Rc::new(move |value: &V| {
                safe_cex_types::retained_json::RetainedJsonInput::from_value(
                    (*json(value)?).clone(),
                )
                .map_err(|error| Rc::new(error.to_string()))
            }),
            exchange_error: Rc::new(|e| Rc::new(e.message)),
        });
        let public = define_public_class(operations.clone()).unwrap();
        let private = define_private_class(Rc::new(PrivateClassOps {
            common: operations,
            recovery: Rc::new({
                let ops = stream.clone();
                move |r: &V| Frame {
                    receiver: r.clone(),
                    ops: ops.clone(),
                }
            }),
            coercion_error: Rc::new(|e| Rc::new(e.message.to_owned())),
            async_arrow: Rc::new({
                let m = metas.clone();
                let runtime = runtime.clone();
                move |name, arity, body| {
                    let rt = runtime.clone();
                    let f = function(move |_, a| Ok(rt.promise(body(a))));
                    m.borrow_mut()
                        .push((f.clone(), name.to_owned(), arity, true));
                    f
                }
            }),
        }))
        .unwrap();
        Self {
            runtime,
            public,
            private,
            metas,
            definitions,
            args,
            trace,
            payloads,
            views,
            store,
            error,
            destructure,
        }
    }
    fn construct(&self, private: bool, parent: V) -> V {
        call(
            if private {
                self.private.clone()
            } else {
                self.public.clone()
            },
            u(),
            vec![parent, n(83.), s("extra")],
        )
        .unwrap()
    }
    fn own(&self, r: &V) -> Vec<String> {
        self.definitions
            .borrow()
            .iter()
            .filter(|(v, _)| Rc::ptr_eq(v, r))
            .map(|(_, k)| k.clone())
            .collect()
    }
    fn payload(&self, value: Value) -> V {
        let v = decode(&value);
        self.views.borrow_mut().push((v.clone(), Rc::new(value)));
        v
    }
    fn method(&self, r: &V, k: &str, args: Vec<V>) -> Result<V, E> {
        call(get(r, k)?, r.clone(), args)
    }
}
const PUBLIC: [&str; 14] = [
    "topics",
    "messageHandlers",
    "connectAndSubscribe",
    "onOpen",
    "subscribe",
    "onMessage",
    "ping",
    "handlePingEvent",
    "handleTickersStreamEvents",
    "handleBBOStreamEvents",
    "handleMarkPricesStreamEvents",
    "listenOHLCV",
    "listenOrderBook",
    "processOrderBookUpdate",
];
const PRIVATE: [&str; 8] = [
    "parseTimestampToSeconds",
    "connectAndSubscribe",
    "onOpen",
    "subscribe",
    "ping",
    "onMessage",
    "handleExecutionReport",
    "handleAlgoExecutionReport",
];
fn layout(h: &Harness, r: &V, private: bool) -> Value {
    let keys = h.own(r);
    assert_eq!(
        keys,
        if private {
            PRIVATE.to_vec()
        } else {
            PUBLIC.to_vec()
        },
        "WOO-CLASS native field order"
    );
    let functions = keys
        .iter()
        .filter_map(|key| {
            let v = get(r, key).unwrap();
            matches!(v.as_ref(), Js::F(_)).then(|| {
                let (name, arity, kind) = meta(&h.metas, &v);
                assert_eq!(name, *key);
                json!([name, arity, kind])
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(functions.len(), if private { 8 } else { 12 });
    let expected = if private {
        vec![1, 0, 0, 0, 0, 1, 1, 1]
    } else {
        vec![0, 0, 0, 1, 0, 0, 1, 1, 1, 2, 2, 2]
    };
    assert_eq!(
        functions
            .iter()
            .map(|v| v[1].as_u64().unwrap())
            .collect::<Vec<_>>(),
        expected,
        "WOO-CLASS native member arity"
    );
    assert_eq!(
        functions.iter().filter(|v| v[2] == true).count(),
        usize::from(private),
        "WOO-CLASS native async kind count"
    );
    if private {
        assert!(meta(&h.metas, &get(r, "onOpen").unwrap()).2);
    }
    json!({"keys":keys,"functions":functions})
}
#[test]
fn native_class_layout_freshness_and_live_wrappers() {
    let h = Harness::new();
    let parent = obj(vec![]);
    let public = h.construct(false, parent.clone());
    let public_two = h.construct(false, parent.clone());
    let private = h.construct(true, parent.clone());
    let private_two = h.construct(true, parent.clone());
    let public_layout = layout(&h, &public, false);
    assert_eq!(layout(&h, &public_two, false), public_layout);
    let private_layout = layout(&h, &private, true);
    assert_eq!(layout(&h, &private_two, true), private_layout);
    assert_eq!(h.definitions.borrow().len(), 44);
    assert_eq!(h.metas.borrow().len(), 50);
    assert_eq!(h.args.borrow().len(), 4);
    for args in h.args.borrow().iter() {
        assert_eq!(args.len(), 3);
        assert!(Rc::ptr_eq(&args[0], &parent));
        assert_eq!(number(&args[1]), 83.);
        assert_eq!(text(&args[2]), "extra");
    }
    for (name, r, other) in PUBLIC
        .iter()
        .map(|n| (*n, &public, &public_two))
        .chain(PRIVATE.iter().map(|n| (*n, &private, &private_two)))
    {
        assert!(
            !Rc::ptr_eq(&get(r, name).unwrap(), &get(other, name).unwrap()),
            "WOO-CLASS native instance field freshness"
        );
    }
    let handlers = get(&public, "messageHandlers").unwrap();
    let other = get(&public_two, "messageHandlers").unwrap();
    let keys = ["ping", "pong", "tickers", "bbos", "markprices"];
    let mut returns = Vec::new();
    for (index, key) in keys.iter().enumerate() {
        let array = get(&handlers, key).unwrap();
        let other_array = get(&other, key).unwrap();
        assert!(!Rc::ptr_eq(&array, &other_array));
        let f = get(&array, "0").unwrap();
        let other_f = get(&other_array, "0").unwrap();
        assert!(!Rc::ptr_eq(&f, &other_f));
        assert_eq!(number(&get(&array, "length").unwrap()), 1.);
        assert_eq!(
            meta(&h.metas, &f),
            ("".into(), usize::from(index > 1), false)
        );
        let called = Rc::new(RefCell::new(0));
        let count = called.clone();
        let expected = public.clone();
        let payload = obj(vec![]);
        let expected_payload = payload.clone();
        let replacement = function(move |r, args| {
            assert!(Rc::ptr_eq(&r, &expected));
            assert_eq!(args.len(), usize::from(index > 1));
            if index > 1 {
                assert!(Rc::ptr_eq(&args[0], &expected_payload));
            }
            *count.borrow_mut() += 1;
            Ok(n(89. + index as f64))
        });
        let method = [
            "handlePingEvent",
            "handlePongEvent",
            "handleTickersStreamEvents",
            "handleBBOStreamEvents",
            "handleMarkPricesStreamEvents",
        ][index];
        let input = obj(vec![("data", payload)]);
        if *key == "bbos" {
            set(&public, method, function(|_, _| panic!("stale handler")));
            *h.destructure.borrow_mut() = Some((input.clone(), public.clone(), replacement));
        } else {
            set(&public, method, replacement);
        }
        let result = call(f, obj(vec![]), vec![input]).unwrap();
        assert_eq!(
            number(&result),
            89. + index as f64,
            "WOO-CLASS native wrapper return"
        );
        assert_eq!(*called.borrow(), 1);
        returns.push(number(&result));
    }
    let reference = source("layout");
    assert_eq!(reference["public"], public_layout);
    assert_eq!(reference["private"], private_layout);
    assert_eq!(reference["returns"].as_array().unwrap().len(), 5);
    for (index, native) in returns.iter().enumerate() {
        assert_eq!(reference["returns"][index].as_f64(), Some(*native));
    }
}
fn source(case: &str) -> Value {
    use std::process::Command;
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/socket-classes-oracle.cjs"
        ))
        .arg(case)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn environment(h: &Harness, r: &V) -> Rc<RefCell<Vec<(bool, String)>>> {
    let sent = Rc::new(RefCell::new(vec![]));
    for (key, logged) in [("send", false), ("sendWithLog", true)] {
        let out = sent.clone();
        let instance = r.clone();
        set(
            r,
            key,
            function(move |this, a| {
                assert!(
                    Rc::ptr_eq(&this, &instance),
                    "WOO-CLASS native send receiver"
                );
                assert_eq!(a.len(), 1);
                assert!(matches!(a[0].as_ref(), Js::S(_)));
                out.borrow_mut().push((logged, text(&a[0])));
                Ok(u())
            }),
        );
    }
    for key in [
        "resetOrderbook",
        "emitWsReconnected",
        "trackLastReceived",
        "emitWsDiagnosticReceive",
        "recordMessageHandlerStats",
        "handlePongEvent",
    ] {
        let trace = h.trace.clone();
        set(
            r,
            key,
            function(move |_, args| {
                if key == "recordMessageHandlerStats" {
                    assert_eq!(args.len(), 1);
                    assert_eq!(text(&args[0]), "ping", "WOO-CLASS native routed stats key");
                }
                trace.borrow_mut().push(key.into());
                Ok(u())
            }),
        );
    }
    sent
}
#[test]
fn native_public_bindings_execute_and_retain_shared_state() {
    let h = Harness::new();
    let parent = obj(vec![(
        "options",
        obj(vec![("applicationId", s("app")), ("testnet", b(false))]),
    )]);
    let r = h.construct(false, parent);
    let sent = environment(&h, &r);
    set(&r, "isDisposed", b(false));
    let listeners = Rc::new(RefCell::new(vec![]));
    let log = listeners.clone();
    let instance = r.clone();
    let ws = obj(vec![(
        "addEventListener",
        function(move |_, a| {
            let event = text(&a[0]);
            let key = match event.as_str() {
                "open" => "onOpen",
                "message" => "onMessage",
                "close" => "onClose",
                _ => panic!("event"),
            };
            assert!(Rc::ptr_eq(&a[1], &get(&instance, key)?));
            log.borrow_mut().push(event);
            Ok(u())
        }),
    )]);
    let socket = ws.clone();
    let url = Rc::new(RefCell::new(String::new()));
    let ulog = url.clone();
    set(
        &r,
        "createWebsocket",
        function(move |_, a| {
            *ulog.borrow_mut() = text(&a[0]);
            Ok(socket.clone())
        }),
    );
    set(&r, "onClose", function(|_, _| Ok(u())));
    call(get(&r, "connectAndSubscribe").unwrap(), obj(vec![]), vec![]).unwrap();
    assert_eq!(
        &*url.borrow(),
        "wss://wss.woox.io/ws/stream/app",
        "WOO-CLASS native public URL"
    );
    assert_eq!(*listeners.borrow(), ["open", "message", "close"]);
    assert!(Rc::ptr_eq(&get(&r, "ws").unwrap(), &ws));
    h.method(&r, "onOpen", vec![]).unwrap();
    assert_eq!(number(&get(&r, "pingAt").unwrap()), 79.);
    assert_eq!(h.payloads.borrow().len(), 4);
    for (index, event) in ["ping", "subscribe", "subscribe", "subscribe"]
        .iter()
        .enumerate()
    {
        assert_eq!(
            text(&get(&h.payloads.borrow()[index], "event").unwrap()),
            *event
        );
    }
    for (index, topic) in ["tickers", "bbos", "markprices"].iter().enumerate() {
        assert_eq!(
            text(&get(&h.payloads.borrow()[index + 1], "topic").unwrap()),
            *topic
        );
    }
    assert_eq!(sent.borrow().len(), 4);
    assert!(h.trace.borrow().contains(&"resetOrderbook".into()));
    for (method, rows) in [
        (
            "handleTickersStreamEvents",
            json!([{"symbol":"PERP_BTC_USDT","close":31,"amount":37,"volume":41}]),
        ),
        (
            "handleBBOStreamEvents",
            json!([{"symbol":"PERP_BTC_USDT","bid":43,"ask":47}]),
        ),
        (
            "handleMarkPricesStreamEvents",
            json!([{"symbol":"PERP_BTC_USDT","price":53}]),
        ),
    ] {
        h.method(&r, method, vec![h.payload(rows)]).unwrap();
    }
    let tickers = h.store.tickers().unwrap();
    assert_eq!(tickers.len(), 1);
    let ticker = &tickers[0];
    assert_eq!(
        [
            ticker.last,
            ticker.quote_volume,
            ticker.volume,
            ticker.bid,
            ticker.ask,
            ticker.mark
        ],
        [31., 37., 41., 43., 47., 53.],
        "WOO-CLASS native shared ticker fields"
    );
    drop(tickers);
    h.runtime.imports.borrow_mut().insert(
        "jsonParse".into(),
        function(|_, a| {
            Ok(decode(
                &safe_cex_transport::how::json_parse(&text(&a[0]))
                    .into_value()
                    .unwrap(),
            ))
        }),
    );
    h.method(
        &r,
        "onMessage",
        vec![obj(vec![("data", s(r#"{"event":"ping"}"#))])],
    )
    .unwrap();
    assert_eq!(sent.borrow().len(), 5, "WOO-CLASS native routed ping count");
    assert_eq!(sent.borrow()[4], (false, r#"{"event":"pong"}"#.into()));
    assert!(
        h.trace
            .borrow()
            .contains(&"recordMessageHandlerStats".into()),
        "WOO-CLASS native router stats"
    );
    let book = obj(vec![]);
    let update = obj(vec![(
        "data",
        obj(vec![
            ("bids", arr(vec![arr(vec![n(101.), n(103.)])])),
            ("asks", arr(vec![])),
        ]),
    )]);
    let calls = Rc::new(RefCell::new(0));
    let count = calls.clone();
    let expected = book.clone();
    h.runtime.imports.borrow_mut().insert(
        "updateOrderBookByPrice".into(),
        function(move |_, a| {
            assert!(Rc::ptr_eq(&a[0], &expected));
            assert_eq!(text(&a[1]), "bids");
            assert_eq!([number(&a[2]), number(&a[3])], [101., 103.]);
            *count.borrow_mut() += 1;
            Ok(u())
        }),
    );
    h.method(&r, "processOrderBookUpdate", vec![book, update])
        .unwrap();
    assert_eq!(*calls.borrow(), 1, "WOO-CLASS native book binding call");
    let failure: E = Rc::new("reverse symbol sentinel".into());
    let symbol_calls = Rc::new(RefCell::new(vec![]));
    let c = symbol_calls.clone();
    let e = failure.clone();
    h.runtime.imports.borrow_mut().insert(
        "reverseSymbol".into(),
        function(move |_, a| {
            assert!(matches!(a[0].as_ref(), Js::S(_)));
            c.borrow_mut().push(text(&a[0]));
            Err(e.clone())
        }),
    );
    for (method, argument) in [
        ("listenOHLCV", obj(vec![("symbol", s("BTC"))])),
        ("listenOrderBook", s("ETH")),
    ] {
        let error = h
            .method(&r, method, vec![argument, function(|_, _| Ok(u()))])
            .err()
            .unwrap();
        assert!(Rc::ptr_eq(&error, &failure));
    }
    assert_eq!(
        *symbol_calls.borrow(),
        ["BTC", "ETH"],
        "WOO-CLASS native listener root arguments"
    );
    let reference = source("public");
    assert_eq!(reference["ticker"], json!([31, 37, 41, 43, 47, 53]));
    assert_eq!(reference["url"], *url.borrow());
    assert_eq!(reference["listeners"], json!(&*listeners.borrow()));
    assert_eq!(reference["sent"], json!(&*sent.borrow()));
    assert_eq!(reference["symbolCalls"], json!(&*symbol_calls.borrow()));
    assert_eq!(reference["bookCalls"], 1);
}
#[test]
fn native_private_bindings_preserve_async_and_live_heartbeat() {
    let h = Harness::new();
    let maps = Rc::new(RefCell::new(vec![]));
    let limit = maps.clone();
    let algo = maps.clone();
    let parent = obj(vec![(
        "options",
        obj(vec![
            ("key", s("fixture-key")),
            ("secret", s("fixture-secret")),
            ("applicationId", s("app")),
            ("testnet", b(false)),
        ]),
    )]);
    let limit_input = obj(vec![]);
    let algo_input = obj(vec![("algoStatus", s("NEW"))]);
    let expected_parent = parent.clone();
    let expected = limit_input.clone();
    set(
        &parent,
        "mapLimitOrder",
        function(move |receiver, args| {
            assert!(
                Rc::ptr_eq(&receiver, &expected_parent),
                "WOO-CLASS native limit mapper receiver"
            );
            assert_eq!(args.len(), 1);
            assert!(
                Rc::ptr_eq(&args[0], &expected),
                "WOO-CLASS native limit report identity"
            );
            limit.borrow_mut().push("limit");
            Ok(u())
        }),
    );
    let expected_parent = parent.clone();
    let expected = algo_input.clone();
    set(
        &parent,
        "mapAlgoOrder",
        function(move |receiver, args| {
            assert!(
                Rc::ptr_eq(&receiver, &expected_parent),
                "WOO-CLASS native algo mapper receiver"
            );
            assert_eq!(args.len(), 1);
            assert!(
                Rc::ptr_eq(&args[0], &expected),
                "WOO-CLASS native algo report identity"
            );
            algo.borrow_mut().push("algo");
            Ok(arr(vec![u()]))
        }),
    );
    let r = h.construct(true, parent);
    let sent = environment(&h, &r);
    set(&r, "isDisposed", b(false));
    let listener_count = Rc::new(RefCell::new(0));
    let count = listener_count.clone();
    let ws = obj(vec![(
        "addEventListener",
        function(move |_, _| {
            *count.borrow_mut() += 1;
            Ok(u())
        }),
    )]);
    let socket = ws.clone();
    let url = Rc::new(RefCell::new(String::new()));
    let log = url.clone();
    set(
        &r,
        "createWebsocket",
        function(move |_, a| {
            *log.borrow_mut() = text(&a[0]);
            Ok(socket.clone())
        }),
    );
    set(&r, "onClose", function(|_, _| Ok(u())));
    h.method(&r, "connectAndSubscribe", vec![]).unwrap();
    assert_eq!(
        &*url.borrow(),
        "wss://wss.woox.io/v2/ws/private/stream/app",
        "WOO-CLASS native private URL"
    );
    assert_eq!(*listener_count.borrow(), 3);

    let auth = h.method(&r, "onOpen", vec![]).unwrap();
    assert!(
        matches!(auth.as_ref(), Js::P(_)),
        "WOO-CLASS native async return kind"
    );
    assert!(matches!(
        outcome(&h.runtime, &auth).unwrap().as_ref(),
        Js::U
    ));
    assert_eq!(h.payloads.borrow().len(), 1);
    let payload = h.payloads.borrow()[0].clone();
    assert_eq!(text(&get(&payload, "event").unwrap()), "auth");
    let params = get(&payload, "params").unwrap();
    assert_eq!(text(&get(&params, "apikey").unwrap()), "fixture-key");
    assert_eq!(
        text(&get(&params, "sign").unwrap()),
        "4d545c1dd0f808dbe2303f94342a0081655b14ee71990743ea9f687ccfa2b181",
        "WOO-CLASS native auth signature"
    );
    assert_eq!(number(&get(&params, "timestamp").unwrap()), 73.);
    h.method(&r, "handleExecutionReport", vec![limit_input])
        .unwrap();
    h.method(&r, "handleAlgoExecutionReport", vec![algo_input])
        .unwrap();
    assert_eq!(
        *maps.borrow(),
        ["limit", "algo"],
        "WOO-CLASS native execution roots"
    );
    let timestamp = h
        .method(&r, "parseTimestampToSeconds", vec![n(1500000000123.)])
        .unwrap();
    assert_eq!(number(&timestamp), 1500000000.);
    h.method(&r, "ping", vec![]).unwrap();
    assert_eq!(number(&get(&r, "pingAt").unwrap()), 79.);
    assert_eq!(sent.borrow().len(), 2);
    assert_eq!(
        text(&get(&h.payloads.borrow()[1], "event").unwrap()),
        "ping"
    );
    assert_eq!(sent.borrow()[1], (true, r#"{"event":"ping"}"#.into()));
    h.method(&r, "subscribe", vec![]).unwrap();
    assert_eq!(
        sent.borrow().len(),
        5,
        "WOO-CLASS native private subscription count"
    );
    for (index, topic) in [(2, "executionreport"), (3, "algoexecutionreportv2")] {
        assert_eq!(
            text(&get(&h.payloads.borrow()[index], "topic").unwrap()),
            topic
        );
        assert_eq!(
            text(&get(&h.payloads.borrow()[index], "event").unwrap()),
            "subscribe"
        );
    }
    set(&r, "throwDisposed", b(true));
    let returned = h
        .method(&r, "onOpen", vec![])
        .expect("async member must return promise, not synchronous error");
    assert!(matches!(returned.as_ref(), Js::P(_)));
    let error = outcome(&h.runtime, &returned).err().unwrap();
    assert!(
        Rc::ptr_eq(&error, &h.error),
        "WOO-CLASS native async rejection identity"
    );
    set(&r, "throwDisposed", b(false));
    let subscribed = Rc::new(RefCell::new(0));
    let sub = subscribed.clone();
    set(
        &r,
        "subscribe",
        function(move |_, _| {
            *sub.borrow_mut() += 1;
            Ok(u())
        }),
    );
    h.runtime.imports.borrow_mut().insert(
        "jsonParse".into(),
        function(|_, a| {
            Ok(decode(
                &safe_cex_transport::how::json_parse(&text(&a[0]))
                    .into_value()
                    .unwrap(),
            ))
        }),
    );
    h.method(
        &r,
        "onMessage",
        vec![obj(vec![("data", s(r#"{"event":"auth","success":true}"#))])],
    )
    .unwrap();
    assert_eq!(
        *subscribed.borrow(),
        1,
        "WOO-CLASS native recursive live subscribe"
    );
    let reference = source("private");
    assert_eq!(reference["sent"], json!(&*sent.borrow()));
    assert_eq!(reference["maps"], json!(&*maps.borrow()));
    assert_eq!(reference["timestamp"], 1500000000);
    assert_eq!(reference["subscribed"], 1);
    assert_eq!(reference["asyncError"], true);
    assert_eq!(reference["url"], *url.borrow());
    assert_eq!(reference["listeners"], 3);
}
#[test]
fn native_heartbeat_resolves_send_after_ping_setter() {
    let h = Harness::new();
    let r = h.construct(false, obj(vec![]));
    environment(&h, &r);
    set(&r, "isDisposed", b(false));
    let calls = Rc::new(RefCell::new(vec![]));
    let c = calls.clone();
    set(&r, "send", function(|_, _| panic!("stale send")));
    let instance = r.clone();
    let replacement = function(move |receiver, a| {
        assert!(Rc::ptr_eq(&receiver, &instance));
        c.borrow_mut().push(text(&a[0]));
        Ok(u())
    });
    set(&r, "replacementSend", replacement);
    h.method(&r, "ping", vec![]).unwrap();
    h.method(&r, "handlePingEvent", vec![]).unwrap();
    assert_eq!(
        *calls.borrow(),
        [r#"{"event":"ping"}"#, r#"{"event":"pong"}"#],
        "WOO-CLASS native replaced send"
    );
    assert_eq!(
        *h.trace.borrow(),
        ["now", "set:pingAt", "stringify", "stringify"]
    );
    assert_eq!(number(&get(&r, "pingAt").unwrap()), 79.);
    let reference = source("heartbeat");
    assert_eq!(reference["calls"], json!(&*calls.borrow()));
    assert_eq!(reference["trace"], json!(&*h.trace.borrow()));
}

#[test]
fn native_immutable_ingress_prices_caller_mutation() {
    let h = Harness::new();
    let r = h.construct(false, obj(vec![]));
    let input = h.payload(json!([{"symbol":"PERP_BTC_USDT","close":31,"amount":37,"volume":41}]));
    let row = get(&input, "0").unwrap();
    set(&row, "close", n(59.));
    assert_eq!(
        number(&get(&row, "close").unwrap()),
        59.,
        "WOO-CLASS native caller mutation"
    );
    h.method(&r, "handleTickersStreamEvents", vec![input])
        .unwrap();
    assert_eq!(
        h.store.tickers().unwrap()[0].last,
        31.,
        "WOO-CLASS native immutable ingress cost"
    );
    let reference = source("payload-alias");
    assert_eq!(
        reference["last"], 59,
        "WOO-CLASS source live payload mutation"
    );
}
