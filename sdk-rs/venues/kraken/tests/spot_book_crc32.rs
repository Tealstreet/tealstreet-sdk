#[path = "spot_support/mod.rs"]
mod support;
use safe_cex_engine::{Command, Event, checksum::Crc32};
use support::*;

// Fixed wire CRCs were calculated independently with Python zlib.crc32.
#[test]
fn shared_crc_preserves_native_digit_spelling_and_recovery() {
    let mut crc = Crc32::new();
    crc.update(b"101003000");
    crc.update(b"990010009800200");
    assert_eq!(crc.finish(), 1_094_800_602);

    let (mut engine, socket, _) = ready(true);
    engine.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "XBT/USD".into(),
        }),
    );
    engine.drain_actions().for_each(drop);
    let snapshot = r#"{"channel":"book","type":"snapshot","data":[{"symbol":"BTC/USD","bids":[{"price":"98.00","qty":"2.00"},{"price":"99.00","qty":"1.000"}],"asks":[{"price":"101.00","qty":"3.000"}],"checksum":1094800602}]}"#;
    message(&mut engine, socket, snapshot);
    assert_eq!(engine.book("XBT/USD").unwrap().best_bid(), Some(99.0));
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":"99.00","qty":"1.500"}]}]}"#,
    );
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","checksum":2808651678}]}"#,
    );
    assert_eq!(engine.book("XBT/USD").unwrap().bids()[0], (99.0, 1.5));
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":"99.00","qty":"0"},{"price":"98.00","qty":"2.500"},{"price":"97.000","qty":"4.0"}],"checksum":893107030}]}"#,
    );
    assert_eq!(
        engine.book("XBT/USD").unwrap().bids(),
        [(98.0, 2.5), (97.0, 4.0)]
    );
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":"98.00","qty":"5"}],"checksum":1}]}"#,
    );
    assert!(engine.book("XBT/USD").is_none());
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","checksum":1094800602}]}"#,
    );
    assert!(engine.book("XBT/USD").is_none());
    message(&mut engine, socket, snapshot);
    assert_eq!(engine.book("XBT/USD").unwrap().best_bid(), Some(99.0));
}

#[test]
fn shared_crc_keeps_asks_first_and_top_10_per_side() {
    assert_eq!(Crc32::default().finish(), 0);
    let (mut engine, socket, _) = ready(true);
    engine.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "XBT/USD".into(),
        }),
    );
    let levels = |from, to| {
        (from..=to)
            .map(|price| format!(r#"{{"price":{price},"qty":1}}"#))
            .collect::<Vec<_>>()
            .join(",")
    };
    message(
        &mut engine,
        socket,
        &format!(
            r#"{{"channel":"book","type":"snapshot","data":[{{"symbol":"BTC/USD","bids":[{}],"asks":[{}],"checksum":3289024467}}]}}"#,
            levels(90, 100),
            levels(101, 111)
        ),
    );
    assert_eq!(engine.book("XBT/USD").unwrap().bids().len(), 11);
    // No checksum still performs native cache retention before the next check.
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":100,"qty":0}]}]}"#,
    );
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","checksum":1762989104}]}"#,
    );
    assert_eq!(engine.book("XBT/USD").unwrap().best_bid(), Some(99.0));
}
