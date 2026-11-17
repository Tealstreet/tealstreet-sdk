//! Bitget Classic V2 limits a subscription to 4096 bytes:
//! https://www.bitget.com/docs/classic/websocket/intro#subscribe
//! Keep the legacy three-way market partition and opaque catalogue IDs; only
//! the wire envelopes may change. These are synthetic, offline catalogue rows.
mod support;

use safe_cex_engine::{Command, Event, SocketId, Venue, model::Interval};
use safe_cex_venue_bitget::Product;
use serde_json::{Value, json};
use support::{FixtureOptions, Harness};

const MAX_BYTES: usize = 4096;

fn loaded(product: Product, testnet: bool, ids: &[String]) -> Harness {
    let quote = match (product, testnet) {
        (Product::Usdt, false) => "USDT",
        (Product::Usdt, true) => "SUSDT",
        (Product::Usdc, false) => "USDC",
        (Product::Usdc, true) => "SUSDC",
        (Product::Inverse, _) => "USD",
    };
    let contracts: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| {
            json!({
                "symbol": id, "baseCoin": format!("ASSET{i}"), "quoteCoin": quote,
                "symbolStatus": "normal", "pricePlace": "1", "priceEndStep": "1",
                "sizeMultiplier": "0.001", "minTradeNum": "0.001", "maxOrderQty": "120"
            })
        })
        .collect();
    let mut h = Harness::started(FixtureOptions {
        public_data_only: true,
        product,
        testnet,
        ..Default::default()
    });
    h.respond(
        "/contracts",
        200,
        &json!({"code":"00000", "data":contracts}).to_string(),
    );
    h.respond(
        "/tickers",
        200,
        &json!({"code":"00000", "data":[{"symbol":ids[0], "lastPr":"100"}]}).to_string(),
    );
    assert_eq!(h.engine.store().markets().len(), ids.len());
    assert_eq!(h.sockets.len(), 3);
    for (socket, _) in h.sockets.clone() {
        h.event(Event::WsOpened { socket });
    }
    h
}

fn topic_frames(h: &Harness, socket: SocketId) -> Vec<Value> {
    h.sent
        .iter()
        .filter(|(s, wire)| *s == socket && wire != "ping")
        .map(|(_, wire)| {
            assert!(
                wire.len() <= MAX_BYTES,
                "{} byte subscription exceeds 4096",
                wire.len()
            );
            let frame: Value = serde_json::from_str(wire).unwrap();
            assert_eq!(frame["op"], "subscribe");
            assert!(!frame["args"].as_array().unwrap().is_empty());
            frame
        })
        .collect()
}

fn assert_partition(h: &Harness, sockets: &[SocketId], ids: &[String], inst_type: &str) {
    for (index, socket) in sockets.iter().enumerate() {
        let mut actual = vec![];
        for frame in topic_frames(h, *socket) {
            for arg in frame["args"].as_array().unwrap() {
                assert_eq!(arg["instType"], inst_type);
                if arg["channel"] == "ticker" {
                    actual.push(arg["instId"].as_str().unwrap().to_owned());
                }
            }
        }
        // This is the legacy TS public/secondary-public selection contract.
        let expected: Vec<_> = ids
            .iter()
            .enumerate()
            .filter(|(i, _)| i % 3 == index)
            .map(|(_, id)| id.clone())
            .collect();
        assert_eq!(actual, expected);
    }
}

fn large_catalogue(product: Product, testnet: bool, inst_type: &str, suffix: &str) {
    let mut ids: Vec<_> = (0..303).map(|i| format!("ASSET{i:04}{suffix}")).collect();
    // An ID's spelling alone is not evidence that the selected catalogue's
    // instrument is unavailable. Preserve stock-style and multiplier names.
    ids[0] = format!("AAPL{suffix}");
    ids[1] = format!("NVDA{suffix}");
    ids[2] = format!("1000PEPE{suffix}");
    let h = loaded(product, testnet, &ids);
    let sockets: Vec<_> = h.sockets.iter().map(|(id, _)| *id).collect();
    assert_partition(&h, &sockets, &ids, inst_type);
    assert!(h.events.iter().all(|e| e.code != "subscription.rejected"));
}

#[test]
fn usdt_public_ticker_batches_preserve_legacy_selection_within_wire_limit() {
    large_catalogue(Product::Usdt, false, "USDT-FUTURES", "USDT");
    large_catalogue(Product::Usdt, true, "SUSDT-FUTURES", "SUSDT");
}

#[test]
fn usdc_public_ticker_batches_preserve_legacy_selection_within_wire_limit() {
    large_catalogue(Product::Usdc, false, "USDC-FUTURES", "USDC");
    large_catalogue(Product::Usdc, true, "SUSDC-FUTURES", "SUSDC");
}

#[test]
fn inverse_public_ticker_batches_preserve_legacy_selection_within_wire_limit() {
    large_catalogue(Product::Inverse, false, "COIN-FUTURES", "USD");
    large_catalogue(Product::Inverse, true, "SCOIN-FUTURES", "USD");
}

#[test]
fn public_ticker_batches_measure_utf8_and_json_escaping_not_symbol_count() {
    let ids: Vec<_> = (0..12)
        .map(|i| format!("{i}{}USDT", "é\\\"".repeat(180)))
        .collect();
    let h = loaded(Product::Usdt, false, &ids);
    let sockets: Vec<_> = h.sockets.iter().map(|(id, _)| *id).collect();
    assert_partition(&h, &sockets, &ids, "USDT-FUTURES");
}

#[test]
fn public_subscription_accepts_exact_wire_limit_and_refuses_only_an_oversized_topic() {
    let first = "X".repeat(1000);
    let envelope = json!({"op":"subscribe", "args":[
        {"instType":"USDT-FUTURES", "channel":"ticker", "instId":first},
        {"instType":"USDT-FUTURES", "channel":"ticker", "instId":""}
    ]})
    .to_string();
    let last = "Z".repeat(MAX_BYTES - envelope.len());
    let oversized = "Y".repeat(5000);
    // Indices 0 and 3 fill one frame on the first socket exactly. Refusing
    // index 1's unencodable topic must not suppress index 4's valid one.
    let ids = vec![
        first.clone(),
        oversized.clone(),
        "BTCUSDT".into(),
        last.clone(),
        "ETHUSDT".into(),
    ];
    let h = loaded(Product::Usdt, false, &ids);
    let mut actual = vec![];
    let mut sizes = vec![];
    for (socket, _) in &h.sockets {
        for frame in topic_frames(&h, *socket) {
            actual.extend(
                frame["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| a["instId"].as_str().unwrap().to_owned()),
            );
        }
        sizes.extend(
            h.sent
                .iter()
                .filter(|(s, text)| s == socket && text != "ping")
                .map(|(_, text)| text.len()),
        );
    }
    assert!(sizes.contains(&MAX_BYTES));
    actual.sort();
    let mut expected = vec![first, last, "BTCUSDT".into(), "ETHUSDT".into()];
    expected.sort();
    assert_eq!(actual, expected);
    assert!(!actual.contains(&oversized));
    let refused: Vec<_> = h
        .events
        .iter()
        .filter(|e| e.code == "subscription.rejected")
        .collect();
    assert_eq!(refused.len(), 1);
    assert!(refused[0].message.contains("4096"));
    assert!(
        refused[0].message.len() < 200,
        "do not log the oversized ID"
    );
}

#[test]
fn a_book_topic_that_fits_can_also_be_unsubscribed_within_the_limit() {
    let envelope = json!({"op":"unsubscribe", "args":[{
        "instType":"USDT-FUTURES", "channel":"books", "instId":""
    }]})
    .to_string();
    let id = "X".repeat(MAX_BYTES - envelope.len());
    let mut h = loaded(Product::Usdt, false, std::slice::from_ref(&id));
    h.sent.clear();
    h.command(Command::SubscribeOrderBook { symbol: id.clone() });
    h.command(Command::UnsubscribeOrderBook { symbol: id.clone() });
    assert_eq!(h.sent.len(), 2);
    for ((_, wire), op) in h.sent.iter().zip(["subscribe", "unsubscribe"]) {
        assert!(wire.len() <= MAX_BYTES);
        let frame: Value = serde_json::from_str(wire).unwrap();
        assert_eq!(frame["op"], op);
        assert_eq!(frame["args"][0]["channel"], "books");
        assert_eq!(frame["args"][0]["instId"], id);
    }
    assert_eq!(h.sent[1].1.len(), MAX_BYTES);
    assert_eq!(h.engine.venue().books().count(), 0);
}

#[test]
fn public_reconnect_keeps_bounded_tickers_and_live_book_and_candle_topics() {
    let ids: Vec<_> = (0..303).map(|i| format!("ASSET{i:04}USDT")).collect();
    let mut h = loaded(Product::Usdt, false, &ids);
    h.command(Command::SubscribeOrderBook {
        symbol: ids[0].clone(),
    });
    h.command(Command::SubscribeCandles {
        symbol: ids[1].clone(),
        interval: Interval::M1,
    });
    let old: Vec<_> = h.sockets.iter().map(|(s, _)| *s).collect();
    h.sent.clear();
    h.now += 75_000.0;
    for (timer, _) in h.timers.clone() {
        h.event(Event::TimerFired { timer });
    }
    for (timer, _) in h.timers.clone() {
        h.event(Event::TimerFired { timer });
    }
    assert_eq!(h.sockets.len(), 6);
    let new: Vec<_> = h.sockets[3..].iter().map(|(s, _)| *s).collect();
    for socket in &new {
        h.event(Event::WsOpened { socket: *socket });
    }
    assert!(h.sent.iter().all(|(s, _)| !old.contains(s)));
    assert_partition(&h, &new, &ids, "USDT-FUTURES");
    let topics: Vec<_> = topic_frames(&h, new[0])
        .into_iter()
        .flat_map(|f| f["args"].as_array().unwrap().clone())
        .collect();
    assert!(
        topics
            .iter()
            .any(|a| a["channel"] == "books" && a["instId"] == ids[0])
    );
    assert!(
        topics
            .iter()
            .any(|a| a["channel"] == "candle1m" && a["instId"] == ids[1])
    );
    let stale = format!(
        r#"{{"arg":{{"channel":"ticker"}},"data":[{{"instId":"{}","lastPr":"999"}}]}}"#,
        ids[0]
    );
    h.event(Event::WsMessage {
        socket: old[0],
        text: &stale,
    });
    assert_eq!(h.engine.store().ticker(&ids[0]).unwrap().last, 100.0);
}
