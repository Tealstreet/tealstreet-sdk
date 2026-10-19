//! Official Classic Futures public examples: docs-new/3470082w0 and /3470086w0.
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, SocketId};
use safe_cex_venue_kucoin::{Kucoin, Options};

const MARKET: &str = r#"{"symbol":"XBTUSDTM","baseCurrency":"XBT","settleCurrency":"USDT","status":"Open","lotSize":1,"tickSize":0.1,"maxOrderQty":1000000,"maxLeverage":125,"multiplier":0.001,"expireDate":null,"isInverse":false}"#;
fn requests(e: &mut Engine<Kucoin>) -> Vec<HttpRequest> {
    e.drain_actions()
        .filter_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .collect()
}
fn answer(e: &mut Engine<Kucoin>, request: &HttpRequest, body: &str) {
    e.handle(
        0.0,
        Event::HttpStart {
            request: request.id,
            status: 200,
            headers: "",
        },
    );
    for data in body.as_bytes().chunks(13) {
        e.handle(
            0.0,
            Event::HttpChunk {
                request: request.id,
                data,
            },
        );
    }
    e.handle(
        0.0,
        Event::HttpEnd {
            request: request.id,
        },
    );
}
fn seeded(count: usize) -> (Engine<Kucoin>, Vec<HttpRequest>) {
    let mut e = Engine::new(Kucoin::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    let markets = (0..count)
        .map(|i| {
            if i == 0 {
                MARKET.into()
            } else {
                MARKET.replace("XBTUSDTM", &format!("T{i}USDTM"))
            }
        })
        .collect::<Vec<String>>()
        .join(",");
    answer(
        &mut e,
        &r,
        &format!(r#"{{"code":"200000","data":[{markets}]}}"#),
    );
    let r = requests(&mut e).remove(0);
    answer(
        &mut e,
        &r,
        r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"100","bestBidPrice":"99","bestAskPrice":"101","size":12}]}"#,
    );
    let r = requests(&mut e);
    (e, r)
}
fn ready() -> (Engine<Kucoin>, SocketId) {
    let (mut e, requests) = seeded(1);
    for (i, r) in requests.iter().enumerate() {
        answer(
            &mut e,
            r,
            &format!(
                r#"{{"code":"200000","data":{{"token":"public-token-{i}","instanceServers":[{{"endpoint":"wss://ws-api-futures.kucoin.com/","pingInterval":18000,"pingTimeout":10000}}]}}}}"#
            ),
        );
    }
    let sockets = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    for socket in &sockets {
        e.handle(0.0, Event::WsOpened { socket: *socket });
    }
    e.drain_actions().for_each(drop);
    (e, sockets[0])
}
#[test]
fn streamed_seeds_keep_native_lots_and_base_contract_multiplier() {
    let (e, requests) = seeded(1);
    let market = &e.store().markets()[0];
    assert_eq!(market.symbol, "BTCUSDT");
    assert_eq!(market.base, "BTC");
    assert_eq!(market.contract_size, Some(0.001));
    assert_eq!(market.precision.amount, 1.0);
    assert_eq!(e.store().tickers()[0].volume, 12.0);
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|r| r.method == safe_cex_engine::Method::Post && r.url.ends_with("bullet-public"))
    );
}
#[test]
fn snapshot_pool_stays_under_session_and_frame_limits() {
    let (mut e, requests) = seeded(761);
    assert_eq!(requests.len(), 4);
    for (i, r) in requests.iter().enumerate() {
        answer(
            &mut e,
            r,
            &format!(
                r#"{{"code":"200000","data":{{"token":"token{i}","instanceServers":[{{"endpoint":"wss://example.test/ws","pingInterval":18000,"pingTimeout":10000}}]}}}}"#
            ),
        );
    }
    let sockets = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsConnect { socket, url } = a {
                assert!(url.contains("?token="));
                Some(socket)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let mut total = 0;
    for socket in sockets {
        e.handle(0.0, Event::WsOpened { socket });
        let mut owned = 0;
        for a in e.drain_actions() {
            if let Action::WsSend { text, .. } = a
                && text.contains("/contractMarket/snapshot:")
            {
                let symbols = text
                    .split("/contractMarket/snapshot:")
                    .nth(1)
                    .unwrap()
                    .split('"')
                    .next()
                    .unwrap();
                let n = symbols.split(',').count();
                assert!(n <= 100);
                owned += n;
            }
        }
        assert!(owned <= 380);
        total += owned;
    }
    assert_eq!(total, 761);
}
#[test]
fn ticker_families_preserve_disjoint_fields() {
    let (mut e, socket) = ready();
    for text in [
        r#"{"type":"message","topic":"/contractMarket/snapshot:XBTUSDTM","data":{"symbol":"XBTUSDTM","lastPrice":"102","priceChgPct":"0.02","volume":10,"turnover":1000,"fundingRate":"0.0001"}}"#,
        r#"{"type":"message","topic":"/contractMarket/tickerV2:XBTUSDTM","data":{"symbol":"XBTUSDTM","bestBidPrice":"101","bestAskPrice":"103"}}"#,
        r#"{"type":"message","topic":"/contract/instrument:XBTUSDTM","data":{"markPrice":102.5,"indexPrice":102.4}}"#,
    ] {
        e.handle(1.0, Event::WsMessage { socket, text });
    }
    let t = &e.store().tickers()[0];
    assert_eq!(
        (
            t.bid,
            t.ask,
            t.last,
            t.mark,
            t.index,
            t.percentage,
            t.volume,
            t.quote_volume
        ),
        (101.0, 103.0, 102.0, 102.5, 102.4, 2.0, 10.0, 1000.0)
    );
}
#[test]
fn depth_buffers_duplicates_recovers_gap_and_keeps_full_native_sizes() {
    let (mut e, socket) = ready();
    e.handle(
        1.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let request = requests(&mut e).remove(0);
    e.handle(2.0,Event::WsMessage{socket,text:r#"{"type":"message","topic":"/contractMarket/level2:XBTUSDTM","data":{"sequence":102,"change":"99,buy,7"}}"#});
    answer(
        &mut e,
        &request,
        r#"{"code":"200000","data":{"sequence":101,"bids":[[100,5],[99,2]],"asks":[[101,3]]}}"#,
    );
    assert_eq!(
        e.book("BTCUSDT").unwrap().bids(),
        [(100.0, 5.0), (99.0, 7.0)]
    );
    e.handle(3.0,Event::WsMessage{socket,text:r#"{"type":"message","topic":"/contractMarket/level2:XBTUSDTM","data":{"sequence":102,"change":"99,buy,99"}}"#});
    assert_eq!(e.book("BTCUSDT").unwrap().bids()[1].1, 7.0);
    e.handle(4.0,Event::WsMessage{socket,text:r#"{"type":"message","topic":"/contractMarket/level2:XBTUSDTM","data":{"sequence":104,"change":"101,sell,0"}}"#});
    assert!(e.book("BTCUSDT").is_none());
    let request = requests(&mut e).remove(0);
    answer(
        &mut e,
        &request,
        r#"{"code":"200000","data":{"sequence":103,"bids":[[100,8]],"asks":[[101,9],[102,4]]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().asks(), [(102.0, 4.0)]);
}
#[test]
fn late_snapshot_after_unsubscribe_cannot_replace_a_new_subscription() {
    let (mut e, _) = ready();
    e.handle(
        1.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let old = requests(&mut e).remove(0);
    e.handle(
        2.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(
        3.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let new = requests(&mut e).remove(0);
    answer(
        &mut e,
        &new,
        r#"{"code":"200000","data":{"sequence":200,"bids":[[100,8]],"asks":[]}}"#,
    );
    answer(
        &mut e,
        &old,
        r#"{"code":"200000","data":{"sequence":100,"bids":[[99,2]],"asks":[]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(100.0, 8.0)]);
}
#[test]
fn missed_pong_closes_socket_and_gets_a_fresh_bullet() {
    let (mut e, tokens) = seeded(1);
    answer(
        &mut e,
        &tokens[0],
        r#"{"code":"200000","data":{"token":"once","instanceServers":[{"endpoint":"wss://example.test","pingInterval":18000,"pingTimeout":10000}]}}"#,
    );
    let actions = e.drain_actions().collect::<Vec<_>>();
    let socket = actions
        .iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(0.0, Event::WsOpened { socket });
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer {
                timer,
                at_ms: 10000.0,
            } = a
            {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(10000.0, Event::TimerFired { timer });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
    );
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer {
                timer,
                at_ms: 11000.0,
            } = a
            {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(11000.0, Event::TimerFired { timer });
    assert!(requests(&mut e)[0].url.ends_with("bullet-public"));
}
#[test]
fn fetched_candles_use_minutes_and_milliseconds_on_wire() {
    let (mut e, _) = ready();
    e.handle(
        0.0,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "BTCUSDT".into(),
            interval: "1h".into(),
            limit: Some(1),
            from_ms: None,
            to_ms: Some(7200000.0),
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(
        r.url
            .ends_with("symbol=XBTUSDTM&granularity=60&from=3600000&to=7200000")
    );
    answer(
        &mut e,
        &r,
        r#"{"code":"200000","data":[[3600000,100,110,90,105,7]]}"#,
    );
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::CommandDone{command:7,result:Ok(s)})if s.contains(r#""timestamp":3600.0"#)&&s.contains(r#""volume":7.0"#))));
}
#[test]
fn rejects_sandbox_before_any_request() {
    let mut e = Engine::new(Kucoin::new(Options {
        testnet: true,
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    assert!(requests(&mut e).is_empty());
}
#[test]
fn api_rejection_never_commits_a_seed() {
    let mut e = Engine::new(Kucoin::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    let r = requests(&mut e).remove(0);
    e.handle(
        0.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":"429000","msg":"rate limit","data":[]}"#,
            },
        },
    );
    assert!(e.store().markets().is_empty());
    assert!(!e.store().loaded().markets);
}

#[test]
fn disconnected_generation_cannot_collide_with_recreated_book() {
    let (mut e, socket) = ready();
    e.handle(
        1.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let old = requests(&mut e).remove(0);
    e.handle(2.0, Event::WsClosed { socket, code: 1006 });
    e.handle(
        3.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(
        4.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    answer(
        &mut e,
        &old,
        r#"{"code":"200000","data":{"sequence":100,"bids":[[99,2]],"asks":[]}}"#,
    );
    assert!(e.book("BTCUSDT").is_none());
}

#[test]
fn missing_private_passphrase_rejects_before_public_or_private_reads() {
    let mut e = Engine::new(Kucoin::new(Options {
        key: Some("key".into()),
        secret: Some("secret".into()),
        ..Default::default()
    }));
    e.start(0.0);
    assert!(requests(&mut e).is_empty());
    assert!(!e.store().loaded().balance);
    assert!(!e.store().loaded().markets);
}

#[test]
fn book_subscription_before_market_seed_is_retained_and_cancelled_by_reference_count() {
    let mut e = Engine::new(Kucoin::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(0.0);
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(
        0.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let request = requests(&mut e).remove(0);
    answer(
        &mut e,
        &request,
        &format!(r#"{{"code":"200000","data":[{MARKET}]}}"#),
    );
    let request = requests(&mut e).remove(0);
    answer(
        &mut e,
        &request,
        r#"{"code":"200000","data":[{"symbol":"XBTUSDTM","price":"100"}]}"#,
    );
    let tokens = requests(&mut e);
    answer(
        &mut e,
        &tokens[0],
        r#"{"code":"200000","data":{"token":"early-token","instanceServers":[{"endpoint":"wss://example.test","pingInterval":18000,"pingTimeout":10000}]}}"#,
    );
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(0.0, Event::WsOpened { socket });
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(
        |a| matches!(a,Action::WsSend{text,..}if text.contains("/contractMarket/level2:XBTUSDTM"))
    ));
    let request = actions
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert!(
        request
            .url
            .ends_with("/api/v1/level2/snapshot?symbol=XBTUSDTM")
    );
    answer(
        &mut e,
        &request,
        r#"{"code":"200000","data":{"sequence":100,"bids":[[99,2]],"asks":[]}}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(99.0, 2.0)]);
    e.handle(
        1.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    assert!(e.book("BTCUSDT").is_none());
}

#[test]
fn ticker_interest_hints_are_safe_before_and_after_startup() {
    let mut e = Engine::new(Kucoin::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.handle(
        0.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["BTCUSDT".into()],
        }),
    );
    assert!(!e.has_actions());
    let (mut e, _) = ready();
    e.handle(
        1.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["BTCUSDT".into()],
        }),
    );
    assert!(!e.has_actions());
}
