#[path = "support/secondary_ws.rs"]
mod support;
use safe_cex_exchange::native_store::DefaultStore;
use safe_cex_exchange::{
    capabilities::ExchangeCapabilities,
    exchange::{BaseExchange, Exchange, ExchangeSettings},
    runtime::ExchangeError,
    websocket::WsEvent,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::{ExchangeName, Market, PlaceOrderOpts, Ticker};
use safe_cex_venue_bitget::secondary_public::BitgetSecondaryPublicWebsocket;
use serde_json::{Value, json};
use std::{cell::Cell, process::Command, rc::Rc};
use support::Runtime;
struct Parent {
    base: BaseExchange,
    capabilities: ExchangeCapabilities,
}
impl Exchange for Parent {
    type PlaceOpts = PlaceOrderOpts;
    fn base(&self) -> &BaseExchange {
        &self.base
    }
    fn capabilities(&self) -> &ExchangeCapabilities {
        &self.capabilities
    }
}
const SYMBOLS: [&str; 9] = [
    "7",
    "BTC",
    "ETH",
    "2",
    "SOL",
    "XRP",
    "1",
    "BNB",
    "__proto__",
];
fn fixture(
    name: ExchangeName,
    testnet: bool,
    index: f64,
) -> (
    Rc<dyn Exchange>,
    Rc<DefaultStore>,
    Rc<Runtime>,
    Rc<BitgetSecondaryPublicWebsocket>,
) {
    let store = Rc::new(DefaultStore::new());
    let markets:Vec<Market>=SYMBOLS.iter().map(|symbol|serde_json::from_value(json!({"id":symbol,"symbol":symbol,"base":"BTC","quote":"USDT","active":true,"precision":{"amount":0.01,"price":0.1},"limits":{"amount":{"min":0.01,"max":100},"leverage":{"min":1,"max":125}}})).unwrap()).collect();
    let ticker:Ticker=serde_json::from_value(json!({"id":"btc","symbol":"BTC","bid":0,"ask":0,"last":0,"mark":0,"index":0,"percentage":0,"openInterest":7001.5,"fundingRate":0,"fundingTime":0,"volume":0,"quoteVolume":0})).unwrap();
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(markets),
            tickers: Some(vec![ticker]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let runtime = Rc::new(Runtime::default());
    runtime.io.ready.set(1);
    runtime.monotonic_time.set(Some(123.25));
    let options = serde_json::from_value(json!({"key":"","secret":"","testnet":testnet})).unwrap();
    let parent: Rc<dyn Exchange> = Rc::new(Parent {
        base: BaseExchange::new(
            name,
            options,
            store.clone(),
            runtime.clone(),
            ExchangeSettings::default(),
        ),
        capabilities: ExchangeCapabilities::default(),
    });
    let socket = BitgetSecondaryPublicWebsocket::new(43, &parent, runtime.clone(), index);
    (parent, store, runtime, socket)
}
fn pump(runtime: &Runtime) {
    for _ in 0..6 {
        runtime.pump();
    }
}
fn topic_keys(socket: &BitgetSecondaryPublicWebsocket) -> Vec<String> {
    socket
        .topics()
        .borrow()
        .get("ticker")
        .unwrap()
        .keys()
        .cloned()
        .collect()
}
fn assert_packet(text: &str, product: &str, channel: &str, symbols: &[&str]) {
    let packet: Value = serde_json::from_str(text).expect("SECONDARY decode outgoing packet");
    assert_eq!(packet["op"], "subscribe", "SECONDARY native subscribe op");
    let args = packet["args"].as_array().unwrap();
    assert_eq!(args.len(), symbols.len(), "SECONDARY native packet count");
    for (arg, symbol) in args.iter().zip(symbols) {
        assert_eq!(arg["instType"], product, "SECONDARY native product type");
        assert_eq!(arg["channel"], channel, "SECONDARY native channel");
        assert_eq!(arg["instId"], *symbol, "SECONDARY native symbol order");
        assert_eq!(
            arg.as_object().unwrap().len(),
            3,
            "SECONDARY native argument fields"
        );
    }
}
fn encode_numbers(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            let n = number.as_f64().unwrap();
            json!({"number":if n==0.0&&n.is_sign_negative(){"-0".to_owned()}else{safe_cex_num::js_number_string(n)},"bits":format!("{:016x}",n.to_bits())})
        }
        Value::Array(rows) => Value::Array(rows.into_iter().map(encode_numbers).collect()),
        Value::Object(rows) => Value::Object(
            rows.into_iter()
                .map(|(k, v)| (k, encode_numbers(v)))
                .collect(),
        ),
        value => value,
    }
}
struct Outcome {
    keys: Vec<String>,
    sent: Vec<String>,
    count: f64,
    latency: f64,
    ticker: Ticker,
}
impl Outcome {
    fn wire(&self) -> Value {
        encode_numbers(
            json!({"symbols":self.keys,"sent":self.sent,"count":self.count,"latency":self.latency,"ticker":serde_json::to_value(&self.ticker).unwrap()}),
        )
    }
}
fn exercise(name: ExchangeName, testnet: bool, index: f64) -> Outcome {
    let (parent, store, runtime, socket) = fixture(name.clone(), testnet, index);
    assert_eq!(
        socket.index().get().to_bits(),
        index.to_bits(),
        "SECONDARY native constructor index"
    );
    assert_eq!(
        socket.message_count(),
        0.0,
        "SECONDARY native constructor message count"
    );
    assert_eq!(
        socket.last_message_count(),
        0.0,
        "SECONDARY native constructor previous count"
    );
    assert_eq!(
        socket.connection_attempts(),
        0.0,
        "SECONDARY native constructor attempts"
    );
    assert!(
        !socket.use_fallback_url(),
        "SECONDARY native constructor fallback"
    );
    assert!(
        socket.topics().borrow().is_empty(),
        "SECONDARY native constructor topics"
    );
    assert!(
        socket.base().message_handlers.borrow().is_empty(),
        "SECONDARY native constructor handlers"
    );
    socket.connect_and_subscribe().unwrap();
    assert_eq!(
        socket.connection_attempts(),
        1.0,
        "SECONDARY native first attempt"
    );
    assert!(
        !socket.base().has_websocket(),
        "SECONDARY owned dial window"
    );
    let expected_symbols = match index as usize {
        0 => vec!["1", "2", "7"],
        1 => vec!["BTC", "SOL", "BNB"],
        2 => vec!["ETH", "XRP", "__proto__"],
        _ => unreachable!(),
    };
    assert_eq!(
        topic_keys(&socket),
        expected_symbols,
        "SECONDARY native initial topics"
    );
    runtime.io.events.borrow_mut().push_back(WsEvent::Open);
    pump(&runtime);
    assert_eq!(
        runtime.dials.borrow().as_slice(),
        ["wss://ws.bitget.com/v2/ws/public"],
        "SECONDARY native direct dial"
    );
    let product = match (&name, testnet) {
        (ExchangeName::Bitget, false) => "USDT-FUTURES",
        (ExchangeName::Bitget, true) => "SUSDT-FUTURES",
        (ExchangeName::Bitgetusdc, false) => "USDC-FUTURES",
        (ExchangeName::Bitgetusdc, true) => "SUSDC-FUTURES",
        (ExchangeName::Bitgetinverse, false) => "COIN-FUTURES",
        (ExchangeName::Bitgetinverse, true) => "SCOIN-FUTURES",
        _ => unreachable!(),
    };
    assert_eq!(
        runtime.io.sent.borrow().len(),
        2,
        "SECONDARY native open sends"
    );
    assert_packet(
        &runtime.io.sent.borrow()[0],
        product,
        "ticker",
        &expected_symbols,
    );
    assert_eq!(
        runtime.io.sent.borrow()[1],
        "ping",
        "SECONDARY native bound ping"
    );
    // Opening twice must replace this socket's listener, not register two.
    socket.on_open().unwrap();
    let added = ["__proto__", "4", "toString", "11", "NEW", "LAST"];
    parent
        .base()
        .emit(
            "markets:changed",
            safe_cex_exchange::js_runtime::value::from_json(
                &json!({"added":added.iter().map(|s|json!({"symbol":s})).collect::<Vec<_>>()}),
            ),
        )
        .unwrap();
    assert_eq!(
        runtime.io.sent.borrow().len(),
        5,
        "SECONDARY native one market listener"
    );
    let expected_added = match index as usize {
        0 => vec!["__proto__", "11"],
        1 => vec!["4", "NEW"],
        2 => vec!["toString", "LAST"],
        _ => unreachable!(),
    };
    assert_packet(
        &runtime.io.sent.borrow()[4],
        product,
        "ticker",
        &expected_added,
    );
    if index == 0.0 {
        assert!(
            !socket.topics().borrow()["ticker"].contains_key("__proto__"),
            "SECONDARY inherited proto setter"
        );
    }
    let frame = r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTC","bidPr":"11.25","askPr":"22.5","lastPr":"33.75","markPrice":"44.5","indexPrice":"55.25","change24h":"0.665","fundingRate":"0.00775","nextFundingTime":"88000","baseVolume":"99.5","quoteVolume":"110.75"}]}"#;
    runtime
        .io
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.into())));
    pump(&runtime);
    assert_eq!(
        socket.message_count(),
        1.0,
        "SECONDARY native delivered message count"
    );
    {
        let rows = store.tickers().unwrap();
        assert_eq!(rows.len(), 1, "SECONDARY native ticker count");
        let t = &rows[0];
        assert_eq!(t.id, "btc", "SECONDARY native ticker id");
        assert_eq!(t.symbol, "BTC", "SECONDARY native ticker symbol");
        for (actual, expected) in [
            (t.bid, 11.25_f64),
            (t.ask, 22.5),
            (t.last, 33.75),
            (t.mark, 44.5),
            (t.index, 55.25),
            (t.percentage, 66.5),
            (t.open_interest, 7001.5),
            (t.funding_rate, 0.00775),
            (t.funding_time, 88000.0),
            (t.volume, 99.5),
            (t.quote_volume, 110.75),
        ] {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "SECONDARY native ticker field bits"
            );
        }
    }
    runtime.monotonic_time.set(Some(137.25));
    runtime
        .io
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text("pong".into())));
    pump(&runtime);
    assert_eq!(
        store.latency().unwrap(),
        7.0,
        "SECONDARY native bound pong latency"
    );
    assert_eq!(socket.message_count(), 2.0, "SECONDARY native pong counted");
    socket
        .on_message(&SocketFrame::Binary(b"pong".to_vec()))
        .unwrap();
    assert_eq!(
        store.latency().unwrap(),
        7.0,
        "SECONDARY binary pong is not text pong"
    );
    assert_eq!(
        socket.message_count(),
        3.0,
        "SECONDARY binary frame counted"
    );
    let sends = runtime.io.sent.borrow().clone();
    let keys = topic_keys(&socket);
    let output = Outcome {
        keys,
        sent: sends,
        count: socket.message_count(),
        latency: store.latency().unwrap(),
        ticker: store.tickers().unwrap()[0].clone(),
    };
    socket.base().dispose();
    runtime.discard_tasks();
    output
}
#[test]
fn secondary_socket_is_assembled_and_delivers_frames() {
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        for testnet in [false, true] {
            for index in [0.0, 1.0, 2.0] {
                exercise(name.clone(), testnet, index);
            }
        }
    }
}
#[test]
fn secondary_fallback_and_cancellation() {
    let (parent, _, runtime, socket) = fixture(ExchangeName::Bitget, false, 1.0);
    socket.connect_and_subscribe().unwrap();
    pump(&runtime);
    let diagnostics = Rc::new(Cell::new(0));
    let count = diagnostics.clone();
    parent.base().on(
        "grafana-info",
        Rc::new(move |_, _| {
            count.set(count.get() + 1);
            Err(ExchangeError::new("diagnostic failed"))
        }),
    );
    socket.connect_and_subscribe().unwrap();
    pump(&runtime);
    assert_eq!(
        socket.connection_attempts(),
        2.0,
        "SECONDARY native fallback attempts"
    );
    assert!(socket.use_fallback_url(), "SECONDARY native fallback latch");
    assert_eq!(
        runtime.dials.borrow()[1],
        "wss://bitget-public-proxy-ja.tealstreet.io/v2/ws/public",
        "SECONDARY native fallback URL"
    );
    assert_eq!(diagnostics.get(), 1, "SECONDARY ignored diagnostic error");
    socket
        .on_message(&SocketFrame::Text("unrelated".into()))
        .unwrap();
    socket.connect_and_subscribe().unwrap();
    pump(&runtime);
    assert_eq!(
        socket.connection_attempts(),
        2.0,
        "SECONDARY traffic stops fallback increment"
    );
    assert_eq!(
        socket.last_message_count(),
        1.0,
        "SECONDARY native prior frame count"
    );
    assert_eq!(socket.message_count(), 0.0, "SECONDARY native frame reset");
    assert!(
        socket.use_fallback_url(),
        "SECONDARY fallback remains latched"
    );
    socket.base().dispose();
    runtime.discard_tasks();
    let (parent2, _, runtime2, socket2) = fixture(ExchangeName::Bitget, false, -0.0);
    runtime2.pending_dial.set(true);
    socket2.connect_and_subscribe().unwrap();
    pump(&runtime2);
    assert!(
        !socket2.base().has_websocket(),
        "SECONDARY pending dial boundary"
    );
    socket2.base().dispose();
    runtime2.pending_dial.set(false);
    pump(&runtime2);
    assert!(
        !socket2.base().has_websocket(),
        "SECONDARY cancelled dial cannot install socket"
    );
    // Independently prove that releasing this fixture can finish an existing dial.
    let (parent3, _, runtime3, socket3) = fixture(ExchangeName::Bitget, false, 0.0);
    runtime3.pending_dial.set(true);
    socket3.connect_and_subscribe().unwrap();
    pump(&runtime3);
    assert!(
        !socket3.base().has_websocket(),
        "SECONDARY resumable dial is pending"
    );
    runtime3.pending_dial.set(false);
    pump(&runtime3);
    assert!(
        socket3.base().has_websocket(),
        "SECONDARY released dial installs silent socket"
    );
    socket3.base().dispose();
    runtime3.discard_tasks();
    drop(parent3);
    drop(parent2);
    drop(parent);
    runtime2.discard_tasks();
}

#[test]
fn secondary_socket_presence_and_parent_lifetime() {
    use std::{
        future::Future,
        task::{Context, Poll, Waker},
    };
    fn ready<T>(future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(value) => value,
            Poll::Pending => panic!("SECONDARY direct fixture dial pending"),
        }
    }
    for state in [0, 3] {
        let (parent, _, runtime, socket) = fixture(ExchangeName::Bitget, false, 1.0);
        runtime.io.ready.set(state);
        ready(
            socket
                .base()
                .create_websocket("wss://fixture.invalid".into(), vec![], None),
        )
        .unwrap();
        assert!(
            socket.base().has_websocket(),
            "SECONDARY native socket presence includes non-open state"
        );
        assert!(
            !socket.base().is_connected(),
            "SECONDARY native socket state is not OPEN"
        );
        assert!(
            !socket.base().diagnostics().initialized,
            "SECONDARY initialized is traffic, not presence"
        );
        socket
            .on_markets_changed(&["ZERO".into(), "NEW".into(), "OTHER".into()])
            .unwrap();
        assert_eq!(
            topic_keys(&socket),
            ["NEW"],
            "SECONDARY native connecting/closed market selection"
        );
        socket.base().dispose();
        assert!(
            !socket.base().has_websocket(),
            "SECONDARY native disposed socket presence"
        );
        runtime.discard_tasks();
        drop(parent);
    }
    let (parent, _, runtime, socket) = fixture(ExchangeName::Bitget, false, 1.0);
    drop(parent);
    assert!(
        socket.base().is_disposed(),
        "SECONDARY chosen weak parent lifetime"
    );
    socket.connect_and_subscribe().unwrap();
    pump(&runtime);
    assert!(
        runtime.dials.borrow().is_empty(),
        "SECONDARY dropped parent prevents dial"
    );
    runtime.discard_tasks();
}
#[test]
#[ignore = "requires Node and TypeScript; included in venue oracle"]
fn secondary_socket_matches_actual_class() {
    // Finish every native assertion before source output comparison.
    secondary_native_ingress_and_error_projection();
    let mut native = Vec::new();
    let mut expected_inputs = Vec::new();
    for name in [
        ExchangeName::Bitget,
        ExchangeName::Bitgetusdc,
        ExchangeName::Bitgetinverse,
    ] {
        for testnet in [false, true] {
            for index in [0.0, 1.0, 2.0] {
                let expected_name = match name {
                    ExchangeName::Bitget => "bitget",
                    ExchangeName::Bitgetusdc => "bitgetusdc",
                    ExchangeName::Bitgetinverse => "bitgetinverse",
                    _ => unreachable!(),
                };
                expected_inputs.push((expected_name, testnet, index));
                native.push(exercise(name.clone(), testnet, index));
            }
        }
    }
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/secondary-public-capture.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "SECONDARY actual class capture {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 18, "SECONDARY independent source floor");
    assert_eq!(native.len(), 18, "SECONDARY independent native floor");
    for (index, (name, testnet, source_index)) in expected_inputs.iter().enumerate() {
        assert_eq!(
            cases[index]["input"]["name"].as_str(),
            Some(*name),
            "SECONDARY source input family"
        );
        assert_eq!(
            cases[index]["input"]["testnet"].as_bool(),
            Some(*testnet),
            "SECONDARY source input network"
        );
        assert_eq!(
            cases[index]["input"]["index"].as_f64().unwrap().to_bits(),
            source_index.to_bits(),
            "SECONDARY source input index bits"
        );
    }
    for (index, expected) in native.iter().enumerate() {
        assert_eq!(
            cases[index]["output"],
            expected.wire(),
            "SECONDARY source output {index}"
        );
    }
    assert_eq!(
        source["synchronousDialHasSocket"], true,
        "SECONDARY source sync dial witness"
    );
    assert_eq!(
        source["retainsParent"], true,
        "SECONDARY source strong parent witness"
    );
    assert_eq!(
        source["utf16FirstRow"].as_f64(),
        Some(93.0),
        "SECONDARY source UTF16 partial processing witness"
    );
}

#[test]
fn secondary_native_ingress_and_error_projection() {
    let (parent, store, runtime, socket) = fixture(ExchangeName::Bitget, false, 1.0);
    let raw = r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTC","lastPr":"93"},{"instId":"\ud800","lastPr":"1"}]}"#;
    let error = socket
        .on_message(&SocketFrame::Text(raw.into()))
        .unwrap_err();
    assert!(
        error.message.contains("invalid utf-16"),
        "SECONDARY explicit typed-frame UTF16 projection refusal"
    );
    assert_eq!(
        store.tickers().unwrap()[0].last,
        0.0,
        "SECONDARY forced whole JSON ingress refusal"
    );
    assert_eq!(
        socket.message_count(),
        1.0,
        "SECONDARY refused frame still counted"
    );
    let mut original = ExchangeError::new("subscriber failed");
    original.name = "RootFailure".into();
    original.response = Some(json!({"witness":42}));
    let cause = Rc::new(original);
    let failure = safe_cex_types::StoreSubscriberError::new("subscriber failed").with_cause(cause);
    let _unsubscribe = Store::subscribe(store.as_ref(), Box::new(move |_, _| Err(failure.clone())));
    let raw = r#"{"arg":{"channel":"ticker"},"data":[{"instId":"BTC","lastPr":"17"},{"instId":"BTC","lastPr":"19"}]}"#;
    let error = socket
        .on_message(&SocketFrame::Text(raw.into()))
        .unwrap_err();
    assert_eq!(
        error.name, "RootFailure",
        "SECONDARY native subscriber error name"
    );
    assert_eq!(
        error.message, "subscriber failed",
        "SECONDARY native subscriber error message"
    );
    assert_eq!(
        error.response,
        Some(json!({"witness":42})),
        "SECONDARY native subscriber response"
    );
    // BaseExchange installed subscriber 0; the failing subscriber is 1 here.
    assert_eq!(
        error.store_subscriber_ids,
        [1],
        "SECONDARY native subscriber identifier"
    );
    assert_eq!(
        store.tickers().unwrap()[0].last,
        17.0,
        "SECONDARY failed notification commits first row"
    );
    drop(parent);
    runtime.discard_tasks();
}

#[test]
fn secondary_state_views_mutate_the_bound_protocol() {
    let (parent, _, runtime, mut socket) = fixture(ExchangeName::Bitget, false, 0.0);
    // The uniquely owned facade exposes borrowed state, never replaceable Rc handles.
    Rc::get_mut(&mut socket).unwrap().index().set(2.0);
    socket.connect_and_subscribe().unwrap();
    assert_eq!(
        topic_keys(&socket),
        ["ETH", "XRP", "__proto__"],
        "SECONDARY native borrowed index changes actual protocol selection"
    );
    runtime.io.events.borrow_mut().push_back(WsEvent::Open);
    pump(&runtime);
    socket
        .topics()
        .borrow_mut()
        .insert("ticker".into(), [("NEW".into(), 1.0)].into_iter().collect());
    socket.subscribe().unwrap();
    let packet: Value = serde_json::from_str(runtime.io.sent.borrow().last().unwrap()).unwrap();
    assert_eq!(
        packet["args"][0]["instId"], "NEW",
        "SECONDARY native borrowed topics feed actual send"
    );
    assert_eq!(
        packet["args"].as_array().unwrap().len(),
        1,
        "SECONDARY native borrowed topic count"
    );
    socket.base().dispose();
    runtime.discard_tasks();
    drop(parent);
}
