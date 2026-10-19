use super::*;
#[path = "../../../safe-cex-venue-phemex/tests/support/binding_harness.rs"]
#[allow(dead_code)]
mod phemex;
use base64::Engine;
use safe_cex_exchange::websocket::WsEvent;
use safe_cex_types::ExchangeName;
use sha2::{Digest, Sha256};
use std::process::Command;

fn fixture() -> Value {
    let bytes = include_bytes!("../../../safe-cex-venue-phemex/tests/fixtures/public-feed.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "f6ce4b8639e9296d44e818fc3d1835956e3066a377e308b07c4c33112c84d717"
    );
    let input: Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(input["frames"].as_array().unwrap().len(), 906);
    assert_eq!(
        input["bundleSha256"],
        "69847e02ed3622d2081f8b667af8fee0910b7ad1780d853ad958b2bf816f62fa"
    );
    input
}
fn source() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-venue-phemex/tests/public-feed-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn constructed(input: &Value) -> phemex::Harness {
    let h = phemex::harness(
        ExchangeName::Phemex,
        serde_json::from_value(json!({"key":"","secret":"","publicDataOnly":true})).unwrap(),
        |http| {
            http.ok("GET", "/public/products", input["products"].clone());
            http.ok("GET", "/md/v3/ticker/24hr/all", input["tickers"].clone());
        },
    );
    h.runtime.now.set(1.0);
    h.runtime
        .wall_offset
        .set(input["epochMs"].as_f64().unwrap());
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    h
}
fn raw(frame: &Value) -> String {
    String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(frame["base64"].as_str().unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn deliver(h: &phemex::Harness, frame: &Value) {
    h.runtime.sockets.borrow()[0]
        .1
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(raw(frame)),
        ));
    h.runtime.pump();
}
fn command(
    h: &phemex::Harness,
    account: &Rc<NativeAccount>,
    name: &str,
    args: Value,
) -> NativeValue {
    h.drive(async {
        native_account_call(
            account.clone(),
            name.into(),
            args.as_array()
                .unwrap()
                .iter()
                .cloned()
                .map(NativeValue::from)
                .collect(),
        )
        .await
        .map_err(|error| ExchangeError::new(format!("{error:?}")))
    })
    .unwrap()
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn recorded_phemex_candles_reach_native_account_unchanged() {
    let input = fixture();
    let expected = source();
    assert_eq!(expected["candles"].as_array().unwrap().len(), 19);
    let h = constructed(&input);
    let exchange: Rc<dyn Exchange> = h.exchange.clone();
    let account = Rc::new(NativeAccount {
        replay_runtime: None,
        symbol_metadata_throttler: symbol_metadata_throttler(exchange.clone()),
        exchange,
        store: h.store.clone(),
        signer: None,
        streams: RefCell::default(),
        next_stream: Cell::new(1),
        startup_token: Cell::new(0),
        store_deliveries: Rc::default(),
        stream_listeners: Rc::default(),
    });
    let candles = Rc::new(RefCell::new(Vec::<NativeValue>::new()));
    let received = candles.clone();
    let unwatch = native_account_watch(
        &account,
        Rc::new(move |event, payload| {
            if event == "listenOHLCV" {
                assert_eq!(
                    payload.get("subscriptionId"),
                    Some(&NativeValue::Number(1.0))
                );
                received
                    .borrow_mut()
                    .push(payload.get("data").unwrap().clone());
            }
        }),
        Rc::new(|_| {}),
    );
    let id = command(
        &h,
        &account,
        "listenOHLCV",
        json!([{"symbol":input["symbol"],"interval":"1m"}]),
    );
    assert_eq!(id, NativeValue::Number(1.0));
    for frame in input["frames"].as_array().unwrap() {
        h.runtime.now.set(frame["atMs"].as_f64().unwrap() + 1.0);
        deliver(&h, frame);
    }
    assert_eq!(
        candles.borrow().len(),
        19,
        "recorded native-account candles: all906 frames, source19 callbacks"
    );
    for (actual, expected) in candles
        .borrow()
        .iter()
        .zip(expected["candles"].as_array().unwrap())
    {
        let native = actual.to_js();
        assert_eq!(
            native.get("timestamp").unwrap().as_f64().unwrap().to_bits(),
            expected["timestamp"].as_f64().unwrap().to_bits()
        );
        for key in ["open", "high", "low", "close", "volume"] {
            assert_eq!(
                native.get(key).unwrap().as_str().unwrap().as_ref(),
                expected[key]
                    .as_str()
                    .unwrap()
                    .encode_utf16()
                    .collect::<Vec<_>>(),
                "original candle UTF16 string: {key}"
            );
        }
        assert_eq!(native.own_keys().len(), expected.as_object().unwrap().len());
    }
    assert!(
        h.runtime.reports.borrow().is_empty(),
        "{:?}",
        h.runtime.reports.borrow()
    );
    command(&h, &account, "unlisten", json!([1]));
    let kline = input["frames"]
        .as_array()
        .unwrap()
        .iter()
        .find(|frame| raw(frame).contains("\"kline_p\""))
        .unwrap();
    deliver(&h, kline);
    assert_eq!(
        candles.borrow().len(),
        19,
        "actual unsubscribe suppresses later frames"
    );
    unwatch();
    command(&h, &account, "dispose", json!([]));
}

#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn recorded_phemex_pongs_use_the_source_performance_clock() {
    let input = fixture();
    let expected = source();
    let h = constructed(&input);
    let socket = h
        .exchange
        .base()
        .get_socket(safe_cex_exchange::socket_lifecycle::SocketSlot::Public)
        .unwrap();
    let mut index = 0;
    for frame in input["frames"].as_array().unwrap() {
        let json: Value = serde_json::from_str(&raw(frame)).unwrap();
        if json["result"] != "pong" && json["method"] != "server.ping" {
            continue;
        }
        // Controlled elapsed times keep scheduled pings outside these recorded pong cases.
        let at = 100.0 + index as f64 * 100.0;
        let delay = [0., 7., 18., 1., 30., 4.][index % 6];
        h.runtime.now.set(at - delay);
        h.runtime.pump();
        socket.ping().unwrap();
        h.runtime.now.set(at);
        deliver(&h, frame);
        assert_eq!(
            h.store.latency().unwrap(),
            expected["latencies"][index].as_f64().unwrap(),
            "recorded pong performance clock: {index}"
        );
        index += 1;
    }
    assert_eq!(index, 12);
    h.venue().dispose().unwrap();
}
