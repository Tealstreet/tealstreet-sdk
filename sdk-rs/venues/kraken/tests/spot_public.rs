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

#[test]
fn dogecoin_rest_identity_is_preserved_while_v2_ticker_and_book_use_doge() {
    let (mut e, s, _) = ready_with_payloads(true, DOGE_META, DOGE_TICKERS, BALANCE);
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
    let mut engine =
        safe_cex_engine::Engine::new(safe_cex_venue_kraken::KrakenSpot::new(options()));
    engine.start(NOW);
    let metadata = requests(&mut engine).pop().unwrap();
    reply(&mut engine, &metadata, META);
    let actions: Vec<_> = engine.drain_actions().collect();
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
    assert_eq!(engine.store().balance().total, 1005.);
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
