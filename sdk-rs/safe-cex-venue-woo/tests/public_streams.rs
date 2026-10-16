#[allow(dead_code)]
#[path = "../../safe-cex-candle-cache/src/calendar.rs"]
mod calendar;
#[path = "support/lifecycle.rs"]
mod fixture;
use fixture::*;
use safe_cex_venue_woo::{legacy_lifecycle, public_streams::*};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};
fn template_text(v: &V) -> Result<String, E> {
    if matches!(v.as_ref(), Js::O(_)) {
        let result = call(get(v, "toString")?, v.clone(), vec![])?;
        Ok(text(&result))
    } else {
        Ok(text(v))
    }
}
struct Streams {
    runtime: Rc<Runtime>,
    ops: Rc<StreamOps<V, E>>,
    timers: Rc<RefCell<Vec<(f64, V, f64)>>>,
    sent: Rc<RefCell<Vec<String>>>,
}
impl Streams {
    fn new() -> Self {
        let runtime = Rc::new(Runtime::default());
        let mut life = runtime.operations();
        let timers = Rc::new(RefCell::new(vec![]));
        let sent = Rc::new(RefCell::new(vec![]));
        let original = {
            let life = Rc::get_mut(&mut life).unwrap();
            let value = Rc::get_mut(&mut life.value).unwrap();
            let base_get = value.get.clone();
            value.get = Rc::new(move |v, k| {
                if let Js::A(_) = v.as_ref() {
                    if matches!(k, "map" | "filter" | "forEach") {
                        let kind = k.to_owned();
                        return Ok(function(move |r, a| {
                            let Js::A(rows) = r.as_ref() else {
                                panic!("array method")
                            };
                            let length = rows.borrow().len();
                            let mut out = vec![];
                            for i in 0..length {
                                let value = rows.borrow().get(i).cloned().unwrap_or_else(u);
                                let result = call(
                                    a[0].clone(),
                                    u(),
                                    vec![value.clone(), n(i as f64), r.clone()],
                                )?;
                                match kind.as_str() {
                                    "map" => out.push(result),
                                    "filter" => {
                                        if truthy(&result) {
                                            out.push(value);
                                        }
                                    }
                                    "forEach" => {}
                                    _ => unreachable!(),
                                }
                            }
                            Ok(if kind == "forEach" { u() } else { arr(out) })
                        }));
                    }
                }
                if let Js::S(_) = v.as_ref() {
                    if k == "endsWith" {
                        return Ok(function(|r, a| Ok(b(text(&r).ends_with(&text(&a[0]))))));
                    }
                }
                base_get(v, k)
            });
            value.get.clone()
        };
        let lifetime = Rc::get_mut(&mut life).unwrap();
        let value = Rc::get_mut(&mut lifetime.value).unwrap();
        let previous = value.global.clone();
        let t = timers.clone();
        let r = runtime.clone();
        value.global = Rc::new(move |name| {
            Ok(match name {
                "Math" => obj(vec![
                    (
                        "max",
                        function(|_, a| Ok(n(number(&a[0]).max(number(&a[1]))))),
                    ),
                    (
                        "min",
                        function(|_, a| Ok(n(number(&a[0]).min(number(&a[1]))))),
                    ),
                ]),
                "parseFloat" => function(|_, a| Ok(n(safe_cex_num::parse_float(&text(&a[0]))))),
                "JSON" => obj(vec![(
                    "stringify",
                    function(|_, a| Ok(s(&serde_json::to_string(&literal(&a[0])).unwrap()))),
                )]),
                "Number" => obj(vec![(
                    "isFinite",
                    function(|_, a| Ok(b(number(&a[0]).is_finite()))),
                )]),
                "console" => {
                    let r = r.clone();
                    obj(vec![(
                        "warn",
                        function(move |_, a| {
                            r.args.borrow_mut().push(("warn".into(), a));
                            Ok(u())
                        }),
                    )])
                }
                "setTimeout" => {
                    let t = t.clone();
                    function(move |_, a| {
                        let mut rows = t.borrow_mut();
                        let id = rows.last().map(|(i, _, _)| i + 1.).unwrap_or(1.);
                        rows.push((id, a[0].clone(), number(&a[1])));
                        Ok(n(id))
                    })
                }
                "clearTimeout" => {
                    let t = t.clone();
                    function(move |_, a| {
                        let id = number(&a[0]);
                        t.borrow_mut().retain(|(i, _, _)| *i != id);
                        Ok(u())
                    })
                }
                _ => return previous(name),
            })
        });
        // Number is callable and has isFinite; the fixture resolves its member separately.
        value.invoke = Rc::new(move |f, r, a| {
            if let Js::O(fields) = f.as_ref() {
                if fields.borrow().iter().any(|(k, _)| k == "isFinite") {
                    return Ok(n(number(&a[0])));
                }
            }
            call(f, r, a)
        });
        let g = original.clone();
        value.get_key = Rc::new(move |v, k| g(v, &text(k)));
        let ops = Rc::new(StreamOps {
            lifecycle: life,
            set: Rc::new(|v, k, new| {
                set(&v, &text(&k), new);
                Ok(())
            }),
            delete: Rc::new(|v, k| {
                let Js::O(fields) = v.as_ref() else {
                    panic!("delete")
                };
                fields.borrow_mut().retain(|(key, _)| key != &text(&k));
                Ok(())
            }),
            define: Rc::new(|v, k, new| {
                set(&v, k, new);
                Ok(())
            }),
            divide: Rc::new(|a, b| Ok(n(number(&a) / number(&b)))),
            greater: Rc::new(|a, b| Ok(number(&a) > number(&b))),
            template: Rc::new(|parts, values| {
                let mut output = parts[0].to_owned();
                for (v, p) in values.iter().zip(&parts[1..]) {
                    output.push_str(&template_text(v)?);
                    output.push_str(p);
                }
                Ok(s(&output))
            }),
            null: Rc::new(|| Rc::new(Js::Null)),
        });
        runtime.imports.borrow_mut().insert(
            "reverseSymbol".into(),
            function(|_, a| Ok(s(&safe_cex_venue_woo::utils::reverse_symbol(&text(&a[0]))))),
        );
        runtime.imports.borrow_mut().insert(
            "getCandleStartTime".into(),
            function(|_, a| {
                Ok(n(calendar::get_candle_start_time(
                    &text(&a[0]),
                    number(&a[1]),
                )))
            }),
        );
        runtime.imports.borrow_mut().insert(
            "add".into(),
            function(|_, a| Ok(n(safe_cex_num::add(number(&a[0]), number(&a[1]))))),
        );
        runtime
            .imports
            .borrow_mut()
            .insert("nanoid".into(), function(|_, _| Ok(s("native-inner"))));
        runtime.imports.borrow_mut().insert(
            "ENDPOINTS".into(),
            obj(vec![("ORDERBOOK", s("/v1/public/orderbook"))]),
        );
        Self {
            runtime,
            ops,
            timers,
            sent,
        }
    }
    fn receiver(&self, public_only: bool) -> V {
        let r = receiver(&self.runtime);
        set(&r, "isConnected", b(true));
        set(&r, "messageHandlers", obj(vec![]));
        set(&r, "topics", obj(vec![]));
        set(&r, "orderBookInnerState", obj(vec![]));
        set(
            &r,
            "parent",
            obj(vec![(
                "options",
                obj(vec![("publicDataOnly", b(public_only))]),
            )]),
        );
        let sent = self.sent.clone();
        let trace = self.runtime.clone();
        set(
            &r,
            "send",
            function(move |_, a| {
                trace
                    .trace
                    .borrow_mut()
                    .push(format!("send:{}", text(&a[0])));
                sent.borrow_mut().push(text(&a[0]));
                Ok(u())
            }),
        );
        set(
            &r,
            "cleanupMessageHandlerStats",
            self.runtime.bind("cleanup-stats", u()),
        );
        r
    }
    fn timer(&self, delay: f64) {
        let index = self
            .timers
            .borrow()
            .iter()
            .position(|(_, _, ms)| *ms == delay)
            .expect("STREAM-NATIVE retained timer");
        let (_, callback, _) = self.timers.borrow_mut().remove(index);
        call(callback, u(), vec![]).unwrap();
        self.runtime.pump();
    }
    fn handlers(&self, r: &V, topic: &str) -> Vec<V> {
        let rows = get(&get(r, "messageHandlers").unwrap(), topic).unwrap();
        let Js::A(rows) = rows.as_ref() else {
            panic!("handlers")
        };
        let out = rows.borrow().clone();
        out
    }
    fn feed(&self, r: &V, topic: &str, data: Value) {
        let handlers = self.handlers(r, topic);
        assert_eq!(handlers.len(), 1, "STREAM-NATIVE parser count");
        call(handlers[0].clone(), u(), vec![decode(&data)]).unwrap();
        self.runtime.pump();
    }
}
fn assert_topics(r: &V, expected: &[&str]) {
    let topics = get(r, "topics").unwrap();
    let Js::O(fields) = topics.as_ref() else {
        panic!("topics")
    };
    assert_eq!(
        fields
            .borrow()
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        expected,
        "STREAM-NATIVE subscribed topic floor"
    );
    for topic in expected {
        assert_eq!(
            text(&get(&topics, topic).unwrap()),
            *topic,
            "STREAM-NATIVE topic native value"
        );
    }
}
fn assert_stats(h: &Streams, expected: &[&str]) {
    let args = h.runtime.args.borrow();
    let calls = args
        .iter()
        .filter(|(k, _)| k == "cleanup-stats")
        .collect::<Vec<_>>();
    assert_eq!(
        calls.len(),
        expected.len(),
        "STREAM-NATIVE cleanup stats floor"
    );
    for ((_, args), topic) in calls.iter().zip(expected) {
        assert_eq!(args.len(), 1);
        assert_eq!(text(&args[0]), *topic, "STREAM-NATIVE cleanup stats topic");
    }
    let events = h.runtime.trace.borrow();
    let positions = events
        .iter()
        .enumerate()
        .filter_map(|(i, v)| (v == "cleanup-stats").then_some(i))
        .collect::<Vec<_>>();
    for (i, topic) in positions.iter().zip(expected) {
        assert_eq!(
            events[*i + 1],
            format!("send:{{\"event\":\"unsubscribe\",\"topic\":\"{topic}\"}}"),
            "STREAM-NATIVE stats before unsubscribe"
        );
    }
}
fn assert_requests(h: &Streams, count: usize) {
    let args = h.runtime.args.borrow();
    let requests = args.iter().filter(|(k, _)| k == "http").collect::<Vec<_>>();
    assert_eq!(requests.len(), count, "STREAM-NATIVE request floor");
    for (_, args) in requests {
        assert_eq!(args.len(), 1);
        assert_eq!(
            text(&args[0]),
            "/v1/public/orderbook/PERP_BTC_USDT",
            "STREAM-NATIVE snapshot URL"
        );
    }
}
fn assert_loaded(inner: &V, expected: bool) {
    assert!(
        matches!(get(inner,"isSnapshotLoaded").unwrap().as_ref(),Js::B(v)if *v==expected),
        "STREAM-NATIVE loaded boolean {expected}"
    );
}
fn captured(h: &Streams) -> (V, Rc<RefCell<Vec<V>>>) {
    let output = Rc::new(RefCell::new(vec![]));
    let values = output.clone();
    let f = function(move |_, a| {
        values.borrow_mut().push(a[0].clone());
        Ok(u())
    });
    let _ = h;
    (f, output)
}
fn assert_candle(value: &V, expected: [f64; 6]) {
    let Js::O(fields) = value.as_ref() else {
        panic!("candle")
    };
    assert_eq!(
        fields
            .borrow()
            .iter()
            .map(|(k, _)| k.clone())
            .collect::<Vec<_>>(),
        ["timestamp", "open", "high", "low", "close", "volume"],
        "STREAM-NATIVE candle keys"
    );
    for (k, expected) in ["timestamp", "open", "high", "low", "close", "volume"]
        .iter()
        .zip(expected)
    {
        let value = get(value, k).unwrap();
        assert!(
            matches!(value.as_ref(), Js::N(_)),
            "STREAM-NATIVE candle field type {k}"
        );
        assert_eq!(number(&value), expected, "STREAM-NATIVE candle {k}");
    }
}
fn stream_source(cases: &[Value]) -> Value {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-streams-oracle.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(cases).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "source streams: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn candle_native_fields_live_interval_backward_guards_and_cleanup() {
    let h = Streams::new();
    let r = h.receiver(false);
    let opts = obj(vec![("symbol", s("BTCUSDT")), ("interval", s("1m"))]);
    let (callback, values) = captured(&h);
    let cleanup = listen_ohlcv(h.ops.clone(), r.clone(), opts.clone(), callback).unwrap();
    let main = "PERP_BTC_USDT@kline_1m";
    let secondary = "PERP_BTC_USDT@trades";
    assert_topics(&r, &[main, secondary]);
    h.feed(
        &r,
        main,
        json!({"data":{"startTime":60000,"open":11,"high":13,"low":7,"close":17,"volume":19}}),
    );
    h.feed(
        &r,
        main,
        json!({"data":{"startTime":60000,"open":997,"high":"23","low":"5","close":29,"volume":31}}),
    );
    h.feed(&r,main,json!({"data":{"startTime":30000,"open":991,"high":991,"low":991,"close":991,"volume":991}}));
    h.feed(
        &r,
        secondary,
        json!({"ts":65000,"data":[{"p":"37","a":"0.2"}]}),
    );
    h.feed(
        &r,
        secondary,
        json!({"ts":1000,"data":[{"p":"997","a":"991"}]}),
    );
    h.feed(
        &r,
        secondary,
        json!({"ts":120000,"data":[{"p":"41","a":"0.3"}]}),
    );
    set(&opts, "interval", s("1s"));
    set(&opts, "symbol", s("ETHUSDT"));
    h.feed(
        &r,
        secondary,
        json!({"ts":125250,"data":[{"p":"47","a":"0.4"}]}),
    );
    h.feed(&r, secondary, json!({"ts":125500,"data":[]}));
    let expected = [
        [60., 11., 13., 7., 17., 19.],
        [60., 11., 23., 5., 29., 31.],
        [60., 11., 37., 5., 37., 31.2],
        [120., 37., 41., 37., 41., 0.3],
        [125., 41., 47., 47., 47., 0.4],
    ];
    assert_eq!(
        values.borrow().len(),
        5,
        "STREAM-NATIVE candle callback floor"
    );
    for (value, expected) in values.borrow().iter().zip(expected) {
        assert_candle(value, expected);
    }
    assert!(
        !Rc::ptr_eq(&values.borrow()[0], &values.borrow()[1]),
        "STREAM-NATIVE fresh candle updates"
    );
    let warnings = h
        .runtime
        .args
        .borrow()
        .iter()
        .filter(|(k, _)| k == "warn")
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(warnings.len(), 1);
    assert_eq!(
        text(&warnings[0].1[0]),
        "Received trade with timestamp in the past, ignoring:"
    );
    assert!(h.timers.borrow().is_empty());
    call(cleanup, u(), vec![]).unwrap();
    assert!(h.handlers(&r, main).is_empty());
    assert!(h.handlers(&r, secondary).is_empty());
    let topics = get(&r, "topics").unwrap();
    assert!(matches!(get(&topics, main).unwrap().as_ref(), Js::U));
    assert!(matches!(get(&topics, secondary).unwrap().as_ref(), Js::U));
    assert_stats(&h, &[main, secondary]);
    let expected_wire = [
        format!("{{\"event\":\"subscribe\",\"topic\":\"{main}\"}}"),
        format!("{{\"event\":\"subscribe\",\"topic\":\"{secondary}\"}}"),
        format!("{{\"event\":\"unsubscribe\",\"topic\":\"{main}\"}}"),
        format!("{{\"event\":\"unsubscribe\",\"topic\":\"{secondary}\"}}"),
    ];
    assert_eq!(*h.sent.borrow(), expected_wire);
    let source = stream_source(&[json!({"kind":"candle","case":"live"})]);
    for (row, expected) in source[0]["candles"]
        .as_array()
        .unwrap()
        .iter()
        .zip(expected)
    {
        for (field, expected) in row.as_array().unwrap().iter().zip(expected) {
            assert_eq!(field.as_f64().unwrap(), expected);
        }
    }
    assert_eq!(source[0]["sent"], json!(expected_wire));
}
#[test]
fn candle_pending_connection_outlives_cleanup_and_public_only_native() {
    let mut rows = vec![];
    for case in ["retry-after-cleanup", "public-only", "disposed"] {
        let h = Streams::new();
        let r = h.receiver(case == "public-only");
        let (callback, values) = captured(&h);
        set(&r, "isConnected", b(case == "public-only"));
        if case == "disposed" {
            set(&r, "isDisposed", b(true));
            set(&r, "isConnected", b(true));
        }
        let cleanup = listen_ohlcv(
            h.ops.clone(),
            r.clone(),
            obj(vec![("symbol", s("BTCUSDT")), ("interval", s("1s"))]),
            callback,
        )
        .unwrap();
        if case == "retry-after-cleanup" {
            assert_eq!(h.timers.borrow().len(), 1);
            call(cleanup.clone(), u(), vec![]).unwrap();
            assert_eq!(
                h.timers.borrow().len(),
                1,
                "STREAM-NATIVE candle retry not cancelled"
            );
            set(&r, "isConnected", b(true));
            h.timer(100.);
        }
        let main = "PERP_BTC_USDT@kline_1s";
        let secondary = "PERP_BTC_USDT@trades";
        let count = if case == "disposed" { 0 } else { 1 };
        let primary = get(&get(&r, "messageHandlers").unwrap(), main).unwrap();
        assert_eq!(
            if let Js::A(a) = primary.as_ref() {
                a.borrow().len()
            } else {
                0
            },
            count
        );
        let extra = get(&get(&r, "messageHandlers").unwrap(), secondary).unwrap();
        assert_eq!(
            if let Js::A(a) = extra.as_ref() {
                a.borrow().len()
            } else {
                0
            },
            if case == "retry-after-cleanup" { 1 } else { 0 },
            "STREAM-NATIVE public-only secondary guard"
        );
        assert_eq!(values.borrow().len(), 0);
        let both = [main, secondary];
        let primary = [main];
        assert_topics(
            &r,
            match case {
                "public-only" => &primary,
                "retry-after-cleanup" => &both,
                _ => &[],
            },
        );
        if case != "disposed" {
            call(cleanup, u(), vec![]).unwrap();
        }
        assert_topics(&r, &[]);
        assert_stats(&h, if case == "disposed" { &[] } else { &both });
        rows.push(json!({"kind":"candle","case":case,"sent":h.sent.borrow().clone()}));
    }
    let cases = rows
        .iter()
        .map(|v| json!({"kind":"candle","case":v["case"]}))
        .collect::<Vec<_>>();
    let source = stream_source(&cases);
    for (i, v) in rows.iter().enumerate() {
        assert_eq!(source[i]["sent"], v["sent"]);
    }
}
fn typed_book(v: &V) -> safe_cex_types::OrderBook {
    fn levels(v: V) -> Vec<safe_cex_types::OrderBookOrders> {
        let Js::A(a) = v.as_ref() else {
            panic!("levels")
        };
        a.borrow()
            .iter()
            .map(|v| safe_cex_types::OrderBookOrders {
                price: number(&get(v, "price").unwrap()),
                amount: number(&get(v, "amount").unwrap()),
                total: number(&get(v, "total").unwrap()),
            })
            .collect()
    }
    safe_cex_types::OrderBook {
        bids: levels(get(v, "bids").unwrap()),
        asks: levels(get(v, "asks").unwrap()),
    }
}
fn store_book(v: &V, book: safe_cex_types::OrderBook) {
    for (name, rows) in [("bids", book.bids), ("asks", book.asks)] {
        set(
            v,
            name,
            arr(rows
                .into_iter()
                .map(|r| {
                    obj(vec![
                        ("price", n(r.price)),
                        ("amount", n(r.amount)),
                        ("total", n(r.total)),
                    ])
                })
                .collect()),
        );
    }
}
fn setup_book(h: &Streams, r: &V, gates: Vec<V>) {
    let requests = Rc::new(RefCell::new(gates));
    let rt = h.runtime.clone();
    let xhr = obj(vec![(
        "get",
        function(move |_, args| {
            rt.args.borrow_mut().push(("http".into(), args));
            assert!(
                !requests.borrow().is_empty(),
                "STREAM-NATIVE planned HTTP floor"
            );
            Ok(requests.borrow_mut().remove(0))
        }),
    )]);
    let parent = get(r, "parent").unwrap();
    set(&parent, "xhr", xhr);
    let ops = h.ops.lifecycle.clone();
    set(
        r,
        "processOrderBookUpdate",
        function(move |_, args| {
            legacy_lifecycle::process_order_book_update(&ops, args[0].clone(), args[1].clone())
        }),
    );
    h.runtime.imports.borrow_mut().insert(
        "updateOrderBookByPrice".into(),
        function(|_, a| {
            let mut book = typed_book(&a[0]);
            safe_cex_store::update_order_book::update_order_book_by_price(
                &mut book,
                text(&a[1]) == "bids",
                number(&a[2]),
                number(&a[3]),
                true,
            );
            store_book(&a[0], book);
            Ok(u())
        }),
    );
    h.runtime.imports.borrow_mut().insert(
        "calcOrderBookTotal".into(),
        function(|_, a| {
            let mut book = typed_book(&a[0]);
            safe_cex_store::orderbook::calc_order_book_total(&mut book);
            store_book(&a[0], book);
            Ok(u())
        }),
    );
}
fn assert_book(value: &V, bids: &[[f64; 3]], asks: &[[f64; 3]]) {
    for (key, expected) in [("bids", bids), ("asks", asks)] {
        let levels = get(value, key).unwrap();
        let Js::A(levels) = levels.as_ref() else {
            panic!("native book side")
        };
        assert_eq!(
            levels.borrow().len(),
            expected.len(),
            "STREAM-NATIVE book side floor {key}"
        );
        for (row, expected) in levels.borrow().iter().zip(expected) {
            for (field, value) in ["price", "amount", "total"].iter().zip(expected) {
                let actual = get(row, field).unwrap();
                assert!(matches!(actual.as_ref(), Js::N(_)));
                assert_eq!(number(&actual), *value, "STREAM-NATIVE book {key}.{field}");
            }
        }
    }
}
fn snapshot() -> V {
    decode(
        &json!({"data":{"timestamp":100,"bids":[{"price":19,"quantity":23}],"asks":[{"price":31,"quantity":37}]}}),
    )
}
#[test]
fn book_snapshot_buffer_filter_replay_alias_and_live_delta_native() {
    let h = Streams::new();
    let r = h.receiver(false);
    let gate = pending();
    setup_book(&h, &r, vec![gate.clone()]);
    let (callback, values) = captured(&h);
    let cleanup = listen_order_book(h.ops.clone(), r.clone(), s("BTCUSDT"), callback).unwrap();
    let topic = "PERP_BTC_USDT@orderbookupdate";
    assert_topics(&r, &[topic]);
    h.feed(
        &r,
        topic,
        json!({"ts":99,"data":{"bids":[[997,41]],"asks":[]}}),
    );
    h.feed(
        &r,
        topic,
        json!({"ts":101,"data":{"bids":[[19,43]],"asks":[]}}),
    );
    h.feed(
        &r,
        topic,
        json!({"ts":"bad","data":{"bids":[],"asks":[[31,47]]}}),
    );
    h.feed(&r, topic, json!({"event":"ack"}));
    assert_eq!(
        h.runtime
            .args
            .borrow()
            .iter()
            .filter(|(k, _)| k == "http")
            .count(),
        1,
        "STREAM-NATIVE one in-flight snapshot"
    );
    assert_eq!(values.borrow().len(), 0);
    let inner = get(&get(&r, "orderBookInnerState").unwrap(), "native-inner").unwrap();
    let buffer = get(&inner, "updates").unwrap();
    let Js::A(buffer) = buffer.as_ref() else {
        panic!("updates")
    };
    assert_eq!(buffer.borrow().len(), 3);
    resolve(&gate, Ok(snapshot()));
    h.runtime.pump();
    assert_eq!(values.borrow().len(), 1);
    let book = values.borrow()[0].clone();
    assert_book(&book, &[[19., 43., 43.]], &[[31., 47., 47.]]);
    assert_loaded(&inner, true);
    let updates = get(&inner, "updates").unwrap();
    assert_eq!(number(&get(&updates, "length").unwrap()), 0.);
    h.feed(
        &r,
        topic,
        json!({"ts":102,"data":{"bids":[[17,53]],"asks":[[31,0]]}}),
    );
    assert_eq!(values.borrow().len(), 2);
    assert!(
        Rc::ptr_eq(&values.borrow()[0], &values.borrow()[1]),
        "STREAM-NATIVE retained book callback alias"
    );
    assert_book(&book, &[[19., 43., 43.], [17., 53., 96.]], &[]);
    call(cleanup, u(), vec![]).unwrap();
    assert_book(&book, &[], &[]);
    assert_loaded(&inner, false);
    assert!(h.timers.borrow().is_empty());
    assert_requests(&h, 1);
    assert_stats(&h, &[topic]);
    let source = stream_source(&[json!({"kind":"book","case":"buffer"})]);
    assert_eq!(source[0]["callbacks"], json!(2));
    assert_eq!(source[0]["sent"], json!(h.sent.borrow().clone()));
}
#[test]
fn book_retry_guard_timer_cleanup_and_late_snapshot_native() {
    let mut rows = vec![];
    for case in ["retry", "late-after-cleanup", "connect-cleanup", "disposed"] {
        let h = Streams::new();
        let r = h.receiver(false);
        let first = pending();
        let second = pending();
        setup_book(&h, &r, vec![first.clone(), second.clone()]);
        let (callback, values) = captured(&h);
        if case == "connect-cleanup" {
            set(&r, "isConnected", b(false));
        }
        if case == "disposed" {
            set(&r, "isDisposed", b(true));
        }
        let cleanup = listen_order_book(h.ops.clone(), r.clone(), s("BTCUSDT"), callback).unwrap();
        if case == "connect-cleanup" {
            assert_eq!(h.timers.borrow().len(), 1);
            call(cleanup, u(), vec![]).unwrap();
            assert_eq!(
                h.timers.borrow().len(),
                0,
                "STREAM-NATIVE book connection timer cancelled"
            );
        } else if case == "disposed" {
            assert_eq!(
                h.runtime
                    .args
                    .borrow()
                    .iter()
                    .filter(|(k, _)| k == "http")
                    .count(),
                0
            );
            assert!(h.timers.borrow().is_empty());
        } else if case == "retry" {
            resolve(&first, Err(Rc::new("transient".into())));
            h.runtime.pump();
            assert_eq!(h.timers.borrow().len(), 1);
            assert_eq!(h.timers.borrow()[0].2, 1000.);
            h.timer(1000.);
            assert_eq!(
                h.runtime
                    .args
                    .borrow()
                    .iter()
                    .filter(|(k, _)| k == "http")
                    .count(),
                2,
                "STREAM-NATIVE retry loading reset"
            );
            resolve(&second, Ok(snapshot()));
            h.runtime.pump();
            assert_eq!(values.borrow().len(), 1);
            assert_book(&values.borrow()[0], &[[19., 23., 23.]], &[[31., 37., 37.]]);
            call(cleanup, u(), vec![]).unwrap();
            assert!(h.timers.borrow().is_empty());
        } else {
            call(cleanup, u(), vec![]).unwrap();
            resolve(&first, Ok(snapshot()));
            h.runtime.pump();
            assert_eq!(
                values.borrow().len(),
                1,
                "STREAM-NATIVE source late snapshot after cleanup"
            );
            assert_book(&values.borrow()[0], &[[19., 23., 23.]], &[[31., 37., 37.]]);
            let inner = get(&get(&r, "orderBookInnerState").unwrap(), "native-inner").unwrap();
            assert_loaded(&inner, true);
            assert!(h.handlers(&r, "PERP_BTC_USDT@orderbookupdate").is_empty());
        }
        assert_requests(
            &h,
            match case {
                "retry" => 2,
                "late-after-cleanup" => 1,
                _ => 0,
            },
        );
        assert_stats(
            &h,
            if matches!(case, "retry" | "late-after-cleanup") {
                &["PERP_BTC_USDT@orderbookupdate"]
            } else {
                &[]
            },
        );
        rows.push(json!({"kind":"book","case":case,"callbacks":values.borrow().len(),"sent":h.sent.borrow().clone()}));
    }
    let cases = rows
        .iter()
        .map(|v| json!({"kind":"book","case":v["case"]}))
        .collect::<Vec<_>>();
    let source = stream_source(&cases);
    for (i, v) in rows.iter().enumerate() {
        assert_eq!(source[i]["callbacks"], v["callbacks"]);
        assert_eq!(source[i]["sent"], v["sent"]);
    }
}
#[test]
fn interpolation_coercion_precedes_later_operands_native() {
    let h = Streams::new();
    let r = h.receiver(true);
    let opts = obj(vec![("symbol", s("BTCUSDT")), ("interval", s("old"))]);
    let live = opts.clone();
    let injected = obj(vec![(
        "toString",
        function(move |_, _| {
            set(&live, "interval", s("5s"));
            Ok(s("CACHE"))
        }),
    )]);
    h.runtime.imports.borrow_mut().insert(
        "reverseSymbol".into(),
        function(move |_, _| Ok(injected.clone())),
    );
    let (callback, values) = captured(&h);
    let cleanup = listen_ohlcv(h.ops.clone(), r.clone(), opts, callback).unwrap();
    assert_eq!(
        h.handlers(&r, "CACHE@kline_5s").len(),
        1,
        "STREAM-NATIVE first topic interpolation before live interval"
    );
    assert_eq!(values.borrow().len(), 0);
    call(cleanup, u(), vec![]).unwrap();
    let b = Streams::new();
    let r = b.receiver(false);
    let gate = pending();
    setup_book(&b, &r, vec![gate]);
    let reverse_calls = Rc::new(RefCell::new(0));
    let count = reverse_calls.clone();
    b.runtime.imports.borrow_mut().insert(
        "reverseSymbol".into(),
        function(move |_, _| {
            *count.borrow_mut() += 1;
            Ok(s("PERP_BTC_USDT"))
        }),
    );
    let endpoint = obj(vec![(
        "toString",
        function(|_, _| Err(Rc::new("endpoint conversion".into()))),
    )]);
    b.runtime
        .imports
        .borrow_mut()
        .insert("ENDPOINTS".into(), obj(vec![("ORDERBOOK", endpoint)]));
    let (callback, values) = captured(&b);
    let cleanup = listen_order_book(b.ops.clone(), r, s("BTCUSDT"), callback).unwrap();
    assert_eq!(
        *reverse_calls.borrow(),
        1,
        "STREAM-NATIVE endpoint conversion precedes reverse symbol"
    );
    assert_eq!(
        b.runtime
            .args
            .borrow()
            .iter()
            .filter(|(k, _)| k == "http")
            .count(),
        0
    );
    assert_eq!(values.borrow().len(), 0);
    assert_eq!(b.timers.borrow().len(), 1);
    assert_eq!(b.timers.borrow()[0].2, 1000.);
    call(cleanup, u(), vec![]).unwrap();
    assert!(b.timers.borrow().is_empty());
    let source = stream_source(&[
        json!({"kind":"interpolation","case":"topic"}),
        json!({"kind":"interpolation","case":"snapshot"}),
    ]);
    assert_eq!(source[0]["topic"], json!("CACHE@kline_5s"));
    assert_eq!(source[1]["reverseCalls"], json!(1));
}
