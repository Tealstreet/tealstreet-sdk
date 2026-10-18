use safe_cex_types::Candle;
use safe_cex_venue_coinbase::{
    public_stream_dispatch::*, starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::Rc,
};
type Failure = Rc<PublicStreamFailure>;
const FIXTURE: &[u8] = include_bytes!("fixtures/public-stream-dispatch.json");
const HASH: &str = "d9b58ec1d0e39c577f315f38ececb917ecdfecc9b52ce7cc000cb1697b4f1ac9";
#[derive(Clone)]
enum Trace {
    Create(String),
    Listener(PublicSocketListener, PublicSocket),
    Reset,
    Reconnect,
    Markets,
    Send(PublicSubscription),
    Track,
    Emit(Option<Frame>),
    ConsoleText(Vec<u16>),
    ConsoleFrame(Frame),
    Pong,
    Stats(String),
    Callback(String, PublicStreamMessage),
}
struct Host {
    trace: Rc<RefCell<Vec<Trace>>>,
    disposed: Cell<bool>,
    testnet: bool,
    reconnect: bool,
    markets: Vec<String>,
    fail: Option<String>,
    failure: Failure,
}
impl Host {
    fn record(&self, kind: &str, value: Trace) -> Result<(), Failure> {
        self.trace.borrow_mut().push(value);
        if self.fail.as_deref() == Some(kind) {
            Err(self.failure.clone())
        } else {
            Ok(())
        }
    }
}
impl PublicStreamHost for Host {
    fn is_disposed(&self) -> bool {
        self.disposed.get()
    }
    fn testnet(&self) -> bool {
        self.testnet
    }
    fn reconnecting_enabled(&self) -> bool {
        self.reconnect
    }
    fn create_websocket(&self, url: &'static str) -> Result<PublicSocket, Failure> {
        self.record("create", Trace::Create(url.into()))?;
        Ok(Rc::new(RefCell::new(Frame::Null)))
    }
    fn add_listener(
        &self,
        socket: PublicSocket,
        listener: PublicSocketListener,
    ) -> Result<(), Failure> {
        self.record("listener", Trace::Listener(listener, socket))
    }
    fn reset_orderbook(&self) -> Result<(), Failure> {
        self.record("reset", Trace::Reset)
    }
    fn emit_ws_reconnected(&self) -> Result<(), Failure> {
        self.record("reconnect", Trace::Reconnect)
    }
    fn markets_symbols(&self) -> Result<Vec<String>, Failure> {
        self.record("markets", Trace::Markets)?;
        Ok(self.markets.clone())
    }
    fn send_subscription(&self, p: PublicSubscription) -> Result<(), Failure> {
        self.record("send", Trace::Send(p))
    }
    fn track_last_received(&self) -> Result<(), Failure> {
        self.record("track", Trace::Track)
    }
    fn json_parse(&self, raw: &str) -> Option<Frame> {
        serde_json::from_str::<Value>(raw)
            .ok()
            .as_ref()
            .map(Frame::from_json)
    }
    fn emit_error(&self, value: Option<Frame>) -> Result<(), Failure> {
        self.record("emit", Trace::Emit(value))
    }
    fn console_error_text(&self, value: Vec<u16>) -> Result<(), Failure> {
        self.record("consoleText", Trace::ConsoleText(value))
    }
    fn console_error_frame(&self, value: Frame) -> Result<(), Failure> {
        self.record("consoleFrame", Trace::ConsoleFrame(value))
    }
    fn handle_pong_event(&self) -> Result<(), Failure> {
        self.record("pong", Trace::Pong)
    }
    fn record_message_handler_stats(&self, topic: &str) -> Result<(), Failure> {
        self.record("stats", Trace::Stats(topic.into()))
    }
}
fn setup(input: &Value) -> Rc<PublicStreamReceiver<Host>> {
    let trace = Rc::new(RefCell::new(vec![]));
    let fail = input["fail"].as_str().map(str::to_owned);
    let failure = Rc::new(PublicStreamFailure {
        name: "Error".into(),
        message: "controlled failure".into(),
        native: None,
    });
    let r = Rc::new(PublicStreamReceiver::new(Host {
        trace: trace.clone(),
        disposed: Cell::new(input["disposed"].as_bool().unwrap_or(false)),
        testnet: input["testnet"].as_bool().unwrap_or(false),
        reconnect: input["reconnect"].as_bool().unwrap_or(false),
        markets: input["markets"]
            .as_array()
            .map(|v| v.iter().map(|v| v.as_str().unwrap().into()).collect())
            .unwrap_or_default(),
        fail: fail.clone(),
        failure: failure.clone(),
    }));
    if let Some(topics) = input["topics"].as_array() {
        for topic in topics {
            r.topics.borrow_mut().insert(
                topic[0].as_str().unwrap().into(),
                topic[1].as_f64().unwrap(),
            );
        }
    }
    if let Some(handlers) = input["handlers"].as_object() {
        for (topic, labels) in handlers {
            let callbacks = labels
                .as_array()
                .unwrap()
                .iter()
                .map(|label| {
                    let label = label.as_str().unwrap().to_owned();
                    let trace = trace.clone();
                    let failure = failure.clone();
                    let fail = fail.clone();
                    PublicStreamHandler::Callback(Rc::new(move |data| {
                        trace
                            .borrow_mut()
                            .push(Trace::Callback(label.clone(), data));
                        if fail.as_deref() == Some("callback") {
                            Err(failure.clone())
                        } else {
                            Ok(())
                        }
                    }))
                })
                .collect();
            r.message_handlers
                .borrow_mut()
                .insert(topic.clone(), Rc::new(RefCell::new(callbacks)));
        }
    }
    r
}
fn assert_number(actual: f64, expected: &Value, label: &str) {
    let bits = expected["$bits"].as_str().expect("source number bits");
    assert_eq!(
        format!("{:016x}", actual.to_bits()),
        bits,
        "{label}: decoded native number bits"
    );
}
fn assert_frame(actual: &Frame, expected: &Value, label: &str) {
    match actual {
        Frame::Undefined => assert_eq!(expected["$undefined"], true, "native undefined"),
        Frame::Null => assert!(expected.is_null(), "{label}: native null"),
        Frame::Bool(v) => assert_eq!(Some(*v), expected.as_bool(), "{label}: native bool"),
        Frame::Number(v) => assert_number(*v, expected, label),
        Frame::String(v) => assert_eq!(
            *v,
            expected
                .as_str()
                .unwrap()
                .encode_utf16()
                .collect::<Vec<_>>(),
            "{label}: native UTF16 string"
        ),
        Frame::Array(v) => {
            let expected = expected.as_array().unwrap();
            let v = v.borrow();
            assert_eq!(
                v.len(),
                expected.len(),
                "{label}: independent source array length"
            );
            for (a, e) in v.iter().zip(expected) {
                assert_frame(a, e, label);
            }
        }
        Frame::Object(v) => {
            let e = expected.as_object().unwrap();
            assert_eq!(
                v.borrow().len(),
                e.len(),
                "{label}: independent source object fields"
            );
            for (key, e) in e {
                assert_frame(&actual.get(key).expect("native own property"), e, label);
            }
        }
    }
}
fn assert_candle(c: &Candle, e: &Value, label: &str) {
    for (key, v) in [
        ("timestamp", c.timestamp),
        ("open", c.open),
        ("high", c.high),
        ("low", c.low),
        ("close", c.close),
        ("volume", c.volume),
    ] {
        assert_number(v, &e[key], &format!("{label}.{key}"));
    }
    assert_eq!(c.last_side, None, "{label}: candle has no native lastSide");
    assert_eq!(
        e.as_object().unwrap().len(),
        6,
        "independent source candle field count"
    );
}
fn assert_trace(a: &Trace, e: &Value, label: &str) {
    let kind = e["kind"].as_str().unwrap();
    match a {
        Trace::Create(url) => {
            assert_eq!(kind, "create", "{label}: native action");
            assert_eq!(url, e["url"].as_str().unwrap(), "{label}: public endpoint");
        }
        Trace::Listener(l, _) => {
            assert_eq!(kind, "listener");
            let name = match l {
                PublicSocketListener::Open => "open",
                PublicSocketListener::Message => "message",
                PublicSocketListener::Close => "close",
            };
            assert_eq!(
                name,
                e["event"].as_str().unwrap(),
                "{label}: native listener event"
            );
            assert_eq!(
                name,
                e["callback"].as_str().unwrap(),
                "{label}: native listener callback"
            );
        }
        Trace::Reset => assert_eq!(kind, "reset"),
        Trace::Reconnect => assert_eq!(kind, "reconnect"),
        Trace::Markets => assert_eq!(kind, "markets"),
        Trace::Track => assert_eq!(kind, "track"),
        Trace::Pong => assert_eq!(kind, "pong"),
        Trace::Send(p) => {
            assert_eq!(kind, "send");
            let e = &e["payload"];
            assert_eq!(
                p.r#type,
                e["type"].as_str().unwrap(),
                "{label}: native subscription type"
            );
            assert_eq!(
                p.channel,
                e["channel"].as_str().unwrap(),
                "{label}: native subscription channel"
            );
            assert_eq!(
                p.product_ids,
                e["product_ids"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>(),
                "{label}: native product IDs"
            );
        }
        Trace::Emit(v) => {
            assert_eq!(kind, "emit");
            match v {
                Some(v) => assert_frame(v, &e["value"], label),
                None => assert_eq!(
                    e["value"],
                    json!({"$undefined":true}),
                    "{label}: missing error message retained"
                ),
            }
        }
        Trace::ConsoleText(v) => {
            assert_eq!(kind, "consoleText");
            assert_eq!(
                *v,
                e["value"]
                    .as_str()
                    .unwrap()
                    .encode_utf16()
                    .collect::<Vec<_>>(),
                "{label}: native diagnostic text"
            );
        }
        Trace::ConsoleFrame(v) => {
            assert_eq!(kind, "consoleFrame");
            assert_frame(v, &e["value"], label);
        }
        Trace::Stats(topic) => {
            assert_eq!(kind, "stats");
            assert_eq!(
                topic,
                e["topic"].as_str().unwrap(),
                "{label}: deduplicated statistic topic"
            );
        }
        Trace::Callback(tag, data) => {
            assert_eq!(kind, "callback");
            assert_eq!(
                tag,
                e["label"].as_str().unwrap(),
                "{label}: callback invocation order"
            );
            match data {
                PublicStreamMessage::Frame(v) => assert_frame(v, &e["data"], label),
                PublicStreamMessage::Candle(v) => assert_candle(&v.borrow(), &e["data"], label),
            }
        }
    }
}
#[test]
fn public_stream_live_native_corpus() {
    assert_eq!(
        format!("{:x}", Sha256::digest(FIXTURE)),
        HASH,
        "content-pinned public stream corpus"
    );
    let inputs: Vec<Value> = serde_json::from_slice(FIXTURE).unwrap();
    assert_eq!(inputs.len(), 28, "independent public stream corpus floor");
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/public-stream-dispatch.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "live public stream oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), 28, "independent oracle output floor");
    for (input, e) in inputs.iter().zip(expected) {
        let label = input["label"].as_str().unwrap();
        let r = setup(input);
        let frame = input
            .get("frame")
            .map(Frame::from_json)
            .unwrap_or(Frame::Null);
        let result = match input["op"].as_str().unwrap() {
            "connect" => connect_and_subscribe(&r),
            "open" => on_open(&r),
            "subscribe" => subscribe(&r),
            "message" => on_message(&r, input["raw"].as_str().unwrap()),
            "candles" => handle_ohlcv_stream_events(&r, &frame),
            "trades" => handle_trade_stream_events(&r, &frame),
            "books" => handle_order_book_stream_events(&r, &frame),
            "ping" => {
                ping(&r);
                Ok(())
            }
            _ => panic!("unknown independently enumerated operation"),
        };
        match &result {
            Ok(()) => assert!(e["error"].is_null(), "{label}: native success"),
            Err(err) => {
                assert_eq!(
                    err.name,
                    e["error"]["name"].as_str().unwrap(),
                    "{label}: native error name"
                );
                assert_eq!(
                    err.message,
                    e["error"]["message"].as_str().unwrap(),
                    "{label}: native error message"
                );
            }
        }
        assert_eq!(
            result
                .as_ref()
                .err()
                .is_some_and(|v| Rc::ptr_eq(v, &r.host.failure)),
            e["sameFailure"].as_bool().unwrap(),
            "{label}: native thrown identity"
        );
        let trace = r.host.trace.borrow();
        let e = e["trace"].as_array().unwrap();
        assert_eq!(
            trace.len(),
            e.len(),
            "{label}: independent source invocation count"
        );
        for (a, e) in trace.iter().zip(e) {
            assert_trace(a, e, label);
        }
        if input["op"] == "connect" && !r.host.disposed.get() {
            assert_eq!(trace.len(), 4, "one create and three independent listeners");
            let socket = r.ws.borrow().clone().unwrap();
            for t in &trace[1..] {
                let Trace::Listener(_, s) = t else {
                    panic!("expected listener")
                };
                assert!(
                    Rc::ptr_eq(s, &socket),
                    "listener retains created socket identity"
                );
            }
        }
    }
}
fn callback(r: &PublicStreamReceiver<Host>, key: &str, cb: PublicStreamCallback) {
    r.message_handlers.borrow_mut().insert(
        key.into(),
        Rc::new(RefCell::new(vec![PublicStreamHandler::Callback(cb)])),
    );
}
fn frame_object(v: &Frame) -> Rc<RefCell<indexmap::IndexMap<Vec<u16>, Frame>>> {
    let Frame::Object(v) = v else {
        panic!("object required")
    };
    v.clone()
}
#[test]
fn public_stream_retains_candle_trade_and_book_roots() {
    let r = setup(&json!({}));
    let seen = Rc::new(RefCell::new(vec![]));
    let roots = seen.clone();
    let cb: PublicStreamCallback = Rc::new(move |data| {
        let PublicStreamMessage::Candle(c) = data else {
            panic!("candle required")
        };
        c.borrow_mut().close = 91.0;
        roots.borrow_mut().push(c);
        Ok(())
    });
    callback(&r, "candles.BTC-USD", cb);
    let first = seen.clone();
    r.message_handlers.borrow()["candles.BTC-USD"]
        .borrow_mut()
        .push(PublicStreamHandler::Callback(Rc::new(move |data| {
            let PublicStreamMessage::Candle(c) = data else {
                panic!("candle required")
            };
            assert!(
                Rc::ptr_eq(&c, &first.borrow()[0]),
                "same native candle root after first callback"
            );
            assert_eq!(
                c.borrow().close,
                91.0,
                "callback mutation reaches next candle listener"
            );
            Ok(())
        })));
    let frame = Frame::from_json(
        &json!({"events":[{"type":"update","candles":[{"product_id":"BTC-USDC","start":"12x","open":"2x","high":"3","low":"1","close":"2","volume":"4"}]}]}),
    );
    // Direct native UTF16 numeric inputs, including the source parseInt/parseFloat prefix semantics.
    let Frame::Array(events) = frame.get("events").unwrap() else {
        panic!()
    };
    let Frame::Array(candles) = events.borrow()[0].get("candles").unwrap() else {
        panic!()
    };
    frame_object(&candles.borrow()[0]).borrow_mut().insert(
        "start".encode_utf16().collect(),
        Frame::String(vec![49, 50, 0xd800]),
    );
    frame_object(&candles.borrow()[0]).borrow_mut().insert(
        "open".encode_utf16().collect(),
        Frame::String(vec![50, 0xd800]),
    );
    handle_ohlcv_stream_events(&r, &frame).unwrap();
    assert_eq!(seen.borrow().len(), 1, "one independent routed candle");
    assert_eq!(
        seen.borrow()[0].borrow().timestamp,
        12.0,
        "surrogate numeric prefix parseInt"
    );
    assert_eq!(
        seen.borrow()[0].borrow().open,
        2.0,
        "surrogate numeric prefix parseFloat"
    );
    for (topic, op, frame) in [
        (
            "market_trades.BTC-USDC",
            0,
            json!({"events":[{"type":"update","trades":[{"product_id":"BTC-USDC","marker":7}]}]}),
        ),
        (
            "level2.BTC-USDC",
            1,
            json!({"events":[{"type":"snapshot","product_id":"BTC-USDC","marker":8}]}),
        ),
    ] {
        let frame = Frame::from_json(&frame);
        let Frame::Array(events) = frame.get("events").unwrap() else {
            panic!()
        };
        let event = events.borrow()[0].clone();
        let root = if op == 0 {
            let Frame::Array(trades) = event.get("trades").unwrap() else {
                panic!()
            };
            let t = trades.borrow()[0].clone();
            t
        } else {
            event
        };
        let expected = frame_object(&root);
        let root_check = expected.clone();
        callback(
            &r,
            topic,
            Rc::new(move |value| {
                let PublicStreamMessage::Frame(value) = value else {
                    panic!()
                };
                assert!(
                    Rc::ptr_eq(&frame_object(&value), &root_check),
                    "original decoded event/trade root reaches callback"
                );
                frame_object(&value)
                    .borrow_mut()
                    .insert("changed".encode_utf16().collect(), Frame::Bool(true));
                Ok(())
            }),
        );
        if op == 0 {
            handle_trade_stream_events(&r, &frame).unwrap();
        } else {
            handle_order_book_stream_events(&r, &frame).unwrap();
        }
        assert!(
            matches!(
                expected
                    .borrow()
                    .get(&"changed".encode_utf16().collect::<Vec<_>>()),
                Some(Frame::Bool(true))
            ),
            "callback mutation reaches original decoded input"
        );
    }
}
#[test]
fn public_stream_live_for_of_and_bounded_for_each() {
    let r = setup(&json!({}));
    let seen = Rc::new(RefCell::new(vec![]));
    let tasks = Rc::new(RefCell::new(vec![]));
    let appended = seen.clone();
    let tasks_cb = tasks.clone();
    let first = seen.clone();
    tasks
        .borrow_mut()
        .push(PublicStreamHandler::Callback(Rc::new(move |_| {
            first.borrow_mut().push("first");
            let second = appended.clone();
            tasks_cb
                .borrow_mut()
                .push(PublicStreamHandler::Callback(Rc::new(move |_| {
                    second.borrow_mut().push("appended");
                    Ok(())
                })));
            Ok(())
        })));
    r.message_handlers
        .borrow_mut()
        .insert("custom".into(), tasks);
    on_message(&r, "{\"channel\":\"custom\"}").unwrap();
    assert_eq!(
        *seen.borrow(),
        ["first", "appended"],
        "onMessage live callback for-of"
    );
    let r = setup(&json!({}));
    let seen = Rc::new(RefCell::new(vec![]));
    let tasks = Rc::new(RefCell::new(vec![]));
    let appended = seen.clone();
    let tasks_cb = tasks.clone();
    let first = seen.clone();
    tasks
        .borrow_mut()
        .push(PublicStreamHandler::Callback(Rc::new(move |_| {
            first.borrow_mut().push("first");
            let second = appended.clone();
            tasks_cb
                .borrow_mut()
                .push(PublicStreamHandler::Callback(Rc::new(move |_| {
                    second.borrow_mut().push("appended");
                    Ok(())
                })));
            Ok(())
        })));
    r.message_handlers
        .borrow_mut()
        .insert("market_trades.BTC-USD".into(), tasks);
    handle_trade_stream_events(
        &r,
        &Frame::from_json(
            &json!({"events":[{"type":"update","trades":[{"product_id":"BTC-USD"}]}]}),
        ),
    )
    .unwrap();
    assert_eq!(
        *seen.borrow(),
        ["first"],
        "trade callback forEach captured length"
    );
    let frame = Frame::from_json(
        &json!({"events":[{"type":"update","trades":[{"product_id":"BTC-USD","trade_id":"A"}]}]}),
    );
    let Frame::Array(events) = frame.get("events").unwrap() else {
        panic!()
    };
    let original = events.clone();
    let seen = Rc::new(RefCell::new(vec![]));
    let ids = seen.clone();
    callback(
        &r,
        "market_trades.BTC-USD",
        Rc::new(move |data| {
            let PublicStreamMessage::Frame(t) = data else {
                panic!()
            };
            let Frame::String(id) = t.get("trade_id").unwrap() else {
                panic!()
            };
            let id = String::from_utf16(&id).unwrap();
            ids.borrow_mut().push(id.clone());
            if id == "A" {
                original.borrow_mut().push(Frame::from_json(
                    &json!({"type":"update","trades":[{"product_id":"BTC-USD","trade_id":"B"}]}),
                ));
            }
            Ok(())
        }),
    );
    handle_trade_stream_events(&r, &frame).unwrap();
    assert_eq!(
        *seen.borrow(),
        ["A", "B"],
        "event for-of sees appended trade event"
    );
}

#[test]
fn public_stream_message_roots_and_listener_loop_bounds() {
    let r = setup(&json!({}));
    let roots = Rc::new(RefCell::new(vec![]));
    let first = roots.clone();
    let second = roots.clone();
    callback(
        &r,
        "custom",
        Rc::new(move |data| {
            let PublicStreamMessage::Frame(v) = data else {
                panic!()
            };
            let root = frame_object(&v);
            root.borrow_mut()
                .insert("marker".encode_utf16().collect(), Frame::Number(91.0));
            first.borrow_mut().push(root);
            Ok(())
        }),
    );
    r.message_handlers.borrow()["custom"]
        .borrow_mut()
        .push(PublicStreamHandler::Callback(Rc::new(move |data| {
            let PublicStreamMessage::Frame(v) = data else {
                panic!()
            };
            assert!(
                Rc::ptr_eq(&frame_object(&v), &second.borrow()[0]),
                "onMessage shared frame root across callbacks"
            );
            assert!(
                matches!(v.get("marker"),Some(Frame::Number(v)) if v==91.0),
                "onMessage first callback mutation reaches second"
            );
            second.borrow_mut().push(frame_object(&v));
            Ok(())
        })));
    on_message(&r, "{\"channel\":\"custom\",\"marker\":1}").unwrap();
    assert_eq!(
        roots.borrow().len(),
        2,
        "both message callbacks reached the same parsed frame root"
    );
    for candles in [true, false] {
        let r = setup(&json!({}));
        let calls = Rc::new(RefCell::new(vec![]));
        let handlers = Rc::new(RefCell::new(vec![]));
        let cb_handlers = handlers.clone();
        let first = calls.clone();
        let later = calls.clone();
        handlers
            .borrow_mut()
            .push(PublicStreamHandler::Callback(Rc::new(move |_| {
                first.borrow_mut().push("first");
                let later = later.clone();
                cb_handlers
                    .borrow_mut()
                    .push(PublicStreamHandler::Callback(Rc::new(move |_| {
                        later.borrow_mut().push("appended");
                        Ok(())
                    })));
                Ok(())
            })));
        if candles {
            r.message_handlers
                .borrow_mut()
                .insert("candles.BTC-USD".into(), handlers);
            handle_ohlcv_stream_events(&r,&Frame::from_json(&json!({"events":[{"type":"update","candles":[{"product_id":"BTC-USDC","start":"1","open":"2","high":"3","low":"1","close":"2","volume":"4"}]}]}))).unwrap();
            assert_eq!(
                *calls.borrow(),
                ["first", "appended"],
                "OHLCV live callback for-of"
            );
        } else {
            r.message_handlers
                .borrow_mut()
                .insert("level2.BTC-USDC".into(), handlers);
            handle_order_book_stream_events(
                &r,
                &Frame::from_json(
                    &json!({"events":[{"type":"snapshot","product_id":"BTC-USDC","updates":[]}]}),
                ),
            )
            .unwrap();
            assert_eq!(
                *calls.borrow(),
                ["first"],
                "orderbook callback forEach captured length"
            );
        }
    }
}
