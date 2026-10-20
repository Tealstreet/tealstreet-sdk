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
#[test]
fn candle_fallback_request_and_alias_stream_fanout() {
    let (mut e, s, _) = ready(true);
    for tf in ["4h", "6h", "12h"] {
        e.handle(
            NOW,
            Event::Command(Command::SubscribeCandles {
                symbol: "XBT/USD".into(),
                interval: tf.into(),
            }),
        );
    }
    e.drain_actions().for_each(drop);
    message(
        &mut e,
        s,
        r#"{"channel":"ohlc","type":"update","data":[{"symbol":"BTC/USD","interval":240,"interval_begin":"2023-11-14T22:00:00Z","open":99,"high":102,"low":98,"close":100,"volume":3}]}"#,
    );
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "XBT/USD".into(),
            interval: "12h".into(),
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
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(Notice::CommandDone{command:7,result:Ok(s)})if s.contains("1700000000"))));
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
            interval: "1m".into(),
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
        let actions = e.drain_actions().collect::<Vec<_>>();
        if attempt < 3 {
            let timer = actions
                .into_iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, .. } => Some(timer),
                    _ => None,
                })
                .unwrap();
            e.handle(NOW, Event::TimerFired { timer });
            let next = requests(&mut e).pop().unwrap();
            assert_eq!(request.url, next.url);
            request = next;
        } else {
            assert!(actions.iter().any(|a| matches!(
                a,
                Action::Notice(Notice::CommandDone {
                    command: 30,
                    result: Err(_)
                })
            )));
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
    let mut e = safe_cex_engine::Engine::new(safe_cex_venue_kraken::KrakenSpot::new(
        safe_cex_venue_kraken::Options::default(),
    ));
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
