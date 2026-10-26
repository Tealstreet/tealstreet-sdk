mod spot_support;
use safe_cex_engine::{Action, Command, Event, Notice};
use spot_support::*;
#[test]
fn public_catalog_ticker_identity_filters_and_no_private_io() {
    let (e, _, private) = ready(true);
    assert!(private.is_none());
    let s = e.store();
    assert_eq!(s.markets().len(), 2);
    let m = s.market("XBT/USD").unwrap();
    assert_eq!(m.id, "BTC/USD");
    assert_eq!(m.base, "BTC");
    assert_eq!(m.precision.price, 0.1);
    assert_eq!(m.limits.leverage.max, 1.);
    assert_eq!(m.is_spot, Some(true));
    let t = s.ticker("XBT/USD").unwrap();
    assert_eq!(
        (t.last, t.mark, t.index, t.percentage, t.quote_volume),
        (100., 100., 100., 25., 400.)
    );
    assert!(s.loaded().balance && s.loaded().positions && s.loaded().orders);
}
#[test]
fn ws_v2_alias_ticker_book_and_depth_window() {
    let (mut e, s, _) = ready(true);
    e.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "XBT/USD".into(),
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a|matches!(a,Action::WsSend{text,..}if text.contains("BTC/USD")&&!text.contains("XBT/USD"))));
    message(
        &mut e,
        s,
        r#"{"channel":"ticker","type":"update","data":[{"symbol":"BTC/USD","last":123,"bid":122}]}"#,
    );
    assert_eq!(e.store().ticker("XBT/USD").unwrap().mark, 123.);
    let bids = (1..=101)
        .map(|p| format!(r#"{{"price":{p},"qty":1}}"#))
        .collect::<Vec<_>>()
        .join(",");
    message(
        &mut e,
        s,
        &format!(
            r#"{{"channel":"book","type":"snapshot","data":[{{"symbol":"BTC/USD","bids":[{bids}],"asks":[{{"price":102,"qty":2}}]}}]}}"#
        ),
    );
    assert_eq!(e.venue().book("XBT/USD").unwrap().bids().len(), 100);
    message(
        &mut e,
        s,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":101,"qty":0},{"price":103,"qty":4}]}]}"#,
    );
    assert_eq!(e.venue().book("XBT/USD").unwrap().best_bid(), Some(103.));
}
// Kraken Spot has no 6h/12h OHLC. TS served 4h candles under those intervals;
// they are now rejected so a chart never shows another interval's bars.
#[test]
fn inexact_candle_intervals_are_rejected_not_served_from_a_smaller_one() {
    use safe_cex_engine::model::Interval;
    let (mut e, s, _) = ready(true);
    for tf in [Interval::H4, Interval::H6, Interval::H12] {
        e.handle(
            NOW,
            Event::Command(Command::SubscribeCandles {
                symbol: "XBT/USD".into(),
                interval: tf,
            }),
        );
    }
    let errors: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => Some(message),
            _ => None,
        })
        .collect();
    assert_eq!(
        errors,
        [
            "Kraken Spot does not support 6h candles",
            "Kraken Spot does not support 12h candles"
        ]
    );
    message(
        &mut e,
        s,
        r#"{"channel":"ohlc","type":"update","data":[{"symbol":"BTC/USD","interval":240,"interval_begin":"2023-11-14T22:00:00Z","open":99,"high":102,"low":98,"close":100,"volume":3}]}"#,
    );
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 6,
            symbol: "XBT/USD".into(),
            interval: Interval::H12,
            from_ms: Some(NOW),
            to_ms: None,
            limit: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 6, error: e }) if e.legacy_text() == "Kraken Spot does not support 12h candles")));
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "XBT/USD".into(),
            interval: Interval::H4,
            from_ms: Some(NOW),
            to_ms: None,
            limit: None,
        }),
    );
    let rs = requests(&mut e);
    assert_eq!(rs.len(), 1);
    assert!(
        rs[0]
            .url
            .contains("pair=XXBTZUSD&interval=240&since=1700000000")
    );
    reply(
        &mut e,
        &rs[0],
        r#"{"error":[],"result":{"XXBTZUSD":[[1700000000,"1","2","0.5","1.5","1","3",4]],"last":1700000001}}"#,
    );
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command:7,result:s})if s.contains("1700000000"))));
}
#[test]
fn disconnect_clears_book_and_ignores_old_socket() {
    let (mut e, s, _) = ready(true);
    e.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH/USD".into(),
        }),
    );
    message(
        &mut e,
        s,
        r#"{"channel":"book","type":"snapshot","data":[{"symbol":"ETH/USD","bids":[{"price":49,"qty":1}],"asks":[]}]}"#,
    );
    assert!(e.venue().book("ETH/USD").is_some());
    e.handle(
        NOW,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    assert!(e.venue().book("ETH/USD").is_none());
    message(
        &mut e,
        s,
        r#"{"channel":"ticker","data":[{"symbol":"ETH/USD","last":777}]}"#,
    );
    assert_eq!(e.store().ticker("ETH/USD").unwrap().last, 50.);
}
use safe_cex_engine::Venue;

#[test]
fn public_candle_network_read_retries_three_times_and_settles() {
    let (mut e, _, _) = ready(true);
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 30,
            symbol: "XBT/USD".into(),
            interval: safe_cex_engine::model::Interval::M1,
            from_ms: None,
            to_ms: None,
            limit: None,
        }),
    );
    let mut request = requests(&mut e).pop().unwrap();
    for attempt in 0..4 {
        e.handle(
            NOW,
            Event::HttpFailed {
                request: request.id,
                error: "reset",
            },
        );
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        if attempt < 3 {
            let next = actions
                .into_iter()
                .find_map(|a| match a {
                    Action::Http(request) => Some(request),
                    _ => None,
                })
                .unwrap();
            assert_eq!(request.url, next.url);
            request = next;
        } else {
            assert!(
                actions.iter().any(|a| matches!(
                    a,
                    Action::Notice(Notice::CommandFailed { command: 30, .. })
                ))
            );
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(_) | Action::SetTimer { .. }))
            );
        }
    }
}

#[test]
fn missing_credentials_automatically_select_public_only_like_ts() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_kraken::KrakenSpot::new(safe_cex_venue_kraken::Options::default()),
    );
    e.start(NOW);
    let m = requests(&mut e).pop().unwrap();
    reply(&mut e, &m, META);
    let t = requests(&mut e).pop().unwrap();
    reply(&mut e, &t, TICKERS);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(!actions.iter().any(|a| matches!(
        a,
        Action::Http(_)
            | Action::WsConnect {
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            }
    )));
    assert!(
        e.store().loaded().balance && e.store().loaded().orders && e.store().loaded().positions
    );
}

#[test]
fn dogecoin_rest_identity_is_preserved_while_v2_ticker_and_book_use_doge() {
    let (mut e, s, _) = ready_with_public_unopened(DOGE_META, DOGE_TICKERS, BALANCE);
    for quote in ["USD", "USDT", "USDC"] {
        let symbol = format!("XDG/{quote}");
        let market = e.store().market(&symbol).unwrap();
        assert_eq!(market.base, "DOGE");
        assert_eq!(market.id, format!("DOGE/{quote}"));
        assert!(e.store().market(&format!("DOGE/{quote}")).is_none());
    }
    e.handle(NOW, Event::WsOpened { socket: s });
    let actions = e.drain_actions().collect::<Vec<_>>();
    let subscription = actions
        .iter()
        .find_map(|a| match a {
            Action::WsSend { text, .. } if text.contains("ticker") => Some(text),
            _ => None,
        })
        .unwrap();
    for quote in ["USD", "USDT", "USDC"] {
        assert!(subscription.contains(&format!("DOGE/{quote}")));
        assert!(!subscription.contains(&format!("XDG/{quote}")));
    }
    e.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "XDG/USD".into(),
        }),
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a|matches!(a,Action::WsSend{text,..}if text.contains("book")&&text.contains("DOGE/USD")&&!text.contains("XDG/USD"))));
    message(
        &mut e,
        s,
        r#"{"channel":"ticker","type":"update","data":[{"symbol":"DOGE/USD","last":0.16,"bid":0.159},{"symbol":"DOGE/USDT","last":0.161},{"symbol":"DOGE/USDC","last":0.162}]}"#,
    );
    assert_eq!(e.store().ticker("XDG/USD").unwrap().mark, 0.16);
    assert_eq!(e.store().ticker("XDG/USDT").unwrap().last, 0.161);
    assert_eq!(e.store().ticker("XDG/USDC").unwrap().last, 0.162);
    message(
        &mut e,
        s,
        r#"{"channel":"book","type":"snapshot","data":[{"symbol":"DOGE/USD","bids":[{"price":0.159,"qty":100}],"asks":[{"price":0.161,"qty":200}]}]}"#,
    );
    assert_eq!(e.venue().book("XDG/USD").unwrap().best_bid(), Some(0.159));
    message(
        &mut e,
        s,
        r#"{"channel":"book","type":"update","data":[{"symbol":"DOGE/USD","bids":[{"price":0.159,"qty":0},{"price":0.16,"qty":120}]}]}"#,
    );
    assert_eq!(e.venue().book("XDG/USD").unwrap().bids(), &[(0.16, 120.)]);
}

#[test]
fn blank_stream_quantities_cannot_delete_a_book_level_or_zero_a_live_quote() {
    let (mut engine, socket, _) = ready(true);
    engine.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "XBT/USD".into(),
        }),
    );
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"snapshot","data":[{"symbol":"BTC/USD","bids":[{"price":99,"qty":2}],"asks":[{"price":101,"qty":3}]}]}"#,
    );
    message(
        &mut engine,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":99,"qty":""}]}]}"#,
    );
    assert_eq!(
        engine.venue().book("XBT/USD").unwrap().bids(),
        [(99.0, 2.0)]
    );
    message(
        &mut engine,
        socket,
        r#"{"channel":"ticker","type":"update","data":[{"symbol":"BTC/USD","last":""}]}"#,
    );
    assert_eq!(engine.store().ticker("XBT/USD").unwrap().last, 100.0);
    let blank = TICKERS.replace("\"100\"", "\"\"");
    let (rest, _, _) = ready_with_payloads(true, META, &blank, BALANCE);
    assert_eq!(rest.store().ticker("XBT/USD").unwrap().last, 0.0);
}

#[test]
fn first_rest_seed_retains_live_quotes_and_values_early_native_holdings_with_them() {
    let mut engine = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_kraken::KrakenSpot::new(options()),
    );
    engine.start(NOW);
    let metadata = requests(&mut engine).pop().unwrap();
    reply(&mut engine, &metadata, META);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW);
    let socket = actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Public),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let ticker = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("Ticker") => Some(request),
            _ => None,
        })
        .unwrap();
    let balance = actions
        .iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.ends_with("Balance") => Some(request),
            _ => None,
        })
        .unwrap();
    reply(&mut engine, balance, BALANCE);
    assert!(!engine.store().loaded().balance);
    assert_eq!(engine.store().currency_balances().unwrap().len(), 4);
    engine.handle(NOW, Event::WsOpened { socket });
    message(
        &mut engine,
        socket,
        r#"{"channel":"ticker","type":"update","data":[{"symbol":"BTC/USD","last":120},{"symbol":"ETH/USD","last":55}]}"#,
    );
    let mut body: serde_json::Value = serde_json::from_str(TICKERS).unwrap();
    body["result"].as_object_mut().unwrap().remove("XETHZUSD");
    reply(&mut engine, ticker, &body.to_string());
    assert_eq!(engine.store().ticker("XBT/USD").unwrap().last, 120.);
    assert_eq!(engine.store().ticker("ETH/USD").unwrap().last, 55.);
    assert_eq!(engine.store().ticker("XBT/USD").unwrap().bid, 99.);
    assert_eq!(engine.store().balance().total, 1410.);
    assert_eq!(engine.store().currency_balances().unwrap().len(), 4);
}

// Official Kraken v2 checksum example: decimal precision and trailing zeros are significant.
// https://docs.kraken.com/exchange/guides/websockets/book-checksum-v2
const CHECKSUM_SNAPSHOT: &str = r#"{"channel":"book","type":"snapshot","data":[{"symbol":"BTC/USD","bids":[{"price":"45283.5","qty":"0.10000000"},{"price":"45283.4","qty":"1.54582015"},{"price":"45282.1","qty":"0.10000000"},{"price":"45281.0","qty":"0.10000000"},{"price":"45280.3","qty":"1.54592586"},{"price":"45279.0","qty":"0.07990000"},{"price":"45277.6","qty":"0.03310103"},{"price":"45277.5","qty":"0.30000000"},{"price":"45277.3","qty":"1.54602737"},{"price":"45276.6","qty":"0.15445238"}],"asks":[{"price":"45285.2","qty":"0.00100000"},{"price":"45286.4","qty":"1.54571953"},{"price":"45286.6","qty":"1.54571109"},{"price":"45289.6","qty":"1.54560911"},{"price":"45290.2","qty":"0.15890660"},{"price":"45291.8","qty":"1.54553491"},{"price":"45294.7","qty":"0.04454749"},{"price":"45296.1","qty":"0.35380000"},{"price":"45297.5","qty":"0.09945542"},{"price":"45299.5","qty":"0.18772827"}],"checksum":3310070434}]}"#;

#[test]
fn checksum_mismatch_clears_consumer_book_and_requires_fresh_snapshot() {
    for numeric_tokens in [false, true] {
        let (mut e, socket, _) = ready(true);
        e.handle(
            NOW,
            Event::Command(Command::SubscribeOrderBook {
                symbol: "XBT/USD".into(),
            }),
        );
        e.drain_actions().for_each(drop);
        let snapshot = if numeric_tokens {
            CHECKSUM_SNAPSHOT
                .replace("\"price\":\"", "\"price\":")
                .replace("\",\"qty\":\"", ",\"qty\":")
                .replace("\"}", "}")
        } else {
            CHECKSUM_SNAPSHOT.into()
        };
        message(&mut e, socket, &snapshot);
        assert_eq!(e.book("XBT/USD").unwrap().best_bid(), Some(45283.5));
        e.take_frame(NOW);
        // A missing or changed level must not become an apparently synchronized book.
        message(
            &mut e,
            socket,
            r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":45283.5,"qty":5}],"checksum":3310070434}]}"#,
        );
        assert!(e.book("XBT/USD").is_none());
        let frame = e.take_frame(NOW).unwrap();
        assert!(
            safe_cex_engine::wire::sections(frame).any(|(kind, payload)| {
                kind == safe_cex_engine::wire::section::BOOK && payload[4..12] == [0; 8]
            })
        );
        let sent: Vec<_> = e
            .drain_actions()
            .filter_map(|a| match a {
                Action::WsSend { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(sent.iter().filter(|s| s.contains("book")).count(), 2);
        assert!(sent[0].contains("unsubscribe") && sent[1].contains("subscribe"));
        message(
            &mut e,
            socket,
            r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":45283.5,"qty":1}]}]}"#,
        );
        assert!(e.book("XBT/USD").is_none());
        message(&mut e, socket, &snapshot);
        assert_eq!(e.book("XBT/USD").unwrap().best_bid(), Some(45283.5));
    }
}

#[test]
fn checksum_valid_updates_apply_all_rows_and_trim_subscribed_depth() {
    let (mut e, socket, _) = ready(true);
    e.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "XBT/USD".into(),
        }),
    );
    let bids = (1..=101)
        .map(|p| format!(r#"{{"price":{p},"qty":1}}"#))
        .collect::<Vec<_>>()
        .join(",");
    // Independently computed IEEE CRC32 of "1022" then top-ten (price,qty) digits.
    message(
        &mut e,
        socket,
        &format!(
            r#"{{"channel":"book","type":"snapshot","data":[{{"symbol":"BTC/USD","bids":[{bids}],"asks":[{{"price":102,"qty":2}}],"checksum":2319673332}}]}}"#
        ),
    );
    assert_eq!(e.book("XBT/USD").unwrap().bids().len(), 100);
    assert_eq!(e.book("XBT/USD").unwrap().bids().last().unwrap().0, 2.0);
    message(
        &mut e,
        socket,
        r#"{"channel":"book","type":"update","data":[{"symbol":"BTC/USD","bids":[{"price":101,"qty":0},{"price":101.5,"qty":1}],"checksum":1711568279}]}"#,
    );
    let book = e.book("XBT/USD").expect("valid complete update CRC");
    assert_eq!(book.best_bid(), Some(101.5));
    assert_eq!(book.bids().len(), 100);
}
