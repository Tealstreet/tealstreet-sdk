mod trusted;
// Payload shapes: docs.blofin.com, instruments, tickers and WS order book.
use safe_cex_engine::{Action, Command, Engine, Event, RequestId, Response, SocketId};
use safe_cex_venue_blofin::{Blofin, Options};
const INSTRUMENT: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
fn request(engine: &mut Engine<Blofin>) -> RequestId {
    safe_cex_engine::test_support::prepared_actions(engine, 0.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap()
}
fn reply(engine: &mut Engine<Blofin>, id: RequestId, body: &str) {
    engine.handle(
        0.0,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn seeded() -> (Engine<Blofin>, SocketId) {
    let mut engine = trusted::trusted_engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    engine.start(0.0);
    let id = request(&mut engine);
    reply(&mut engine, id, INSTRUMENT);
    let actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.);
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
    let id = actions
        .iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap();
    reply(
        &mut engine,
        id,
        r#"{"code":"0","data":[{"instId":"BTC-USDT","last":"100","open24h":"80","bidPrice":"99","askPrice":"101","volCurrency24h":"42","vol24h":"4200"}]}"#,
    );
    engine.handle(0.0, Event::WsOpened { socket });
    engine.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    engine.drain_actions().for_each(drop);
    (engine, socket)
}
#[test]
fn full_depth_snapshot_update_gap_and_recovery() {
    let (mut e, s) = seeded();
    let mut send = |text| e.handle(100.0, Event::WsMessage { socket: s, text });
    send(
        r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"snapshot","data":[{"seqId":"10","bids":[["99","2"],["98","3"]],"asks":[["101","4"]]}]}"#,
    );
    send(
        r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"update","data":[{"prevSeqId":"10","seqId":"11","bids":[["99","0"],["97","5"]],"asks":[]}]}"#,
    );
    assert_eq!(
        e.book("BTCUSDT").unwrap().bids(),
        [(98.0, 3.0), (97.0, 5.0)]
    );
    e.handle(200.0,Event::WsMessage { socket:s,text:r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"update","data":{"prevSeqId":"9","seqId":"12","bids":[],"asks":[]}}"# });
    assert!(e.book("BTCUSDT").unwrap().is_empty());
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        2
    );
    e.handle(201.0,Event::WsMessage { socket:s,text:r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"snapshot","data":{"seqId":"20","bids":[["99","1"]],"asks":[["101","1"]]}}"# });
    assert_eq!(e.book("BTCUSDT").unwrap().best_bid(), Some(99.0));
}
#[test]
fn http_200_api_error_does_not_load_markets() {
    let mut e = trusted::trusted_engine(Options::default());
    e.start(0.0);
    let id = request(&mut e);
    reply(
        &mut e,
        id,
        r#"{"data":[],"code":"152401","msg":"Access key does not exist"}"#,
    );
    assert!(!e.store().loaded().markets);
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(_))));
}
#[test]
fn watchdog_closes_silent_socket_but_accepts_inbound_traffic() {
    let (mut e, s) = seeded();
    e.handle(
        1000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let reconnect = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2000.0, Event::TimerFired { timer: reconnect });
    let actions: Vec<_> = e.drain_actions().collect();
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(23_000.0, Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
}

#[test]
fn top_five_snapshot_replaces_price_range_without_duplicate_or_unsorted_levels() {
    let (mut e, s) = seeded();
    e.handle(100.0, Event::WsMessage {socket:s,text:r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"snapshot","data":{"seqId":"1","bids":[["99","1"],["98","2"],["97","3"],["96","4"]],"asks":[["101","1"],["102","2"],["103","3"],["104","4"]]}}"#});
    e.handle(101.0, Event::WsMessage {socket:s,text:r#"{"arg":{"channel":"books5","instId":"BTC-USDT"},"data":{"bids":[["98","5"],["97","6"]],"asks":[["102","5"],["103","6"]]}}"#});
    assert_eq!(
        e.book("BTCUSDT").unwrap().bids(),
        [(98.0, 5.0), (97.0, 6.0)]
    );
    assert_eq!(
        e.book("BTCUSDT").unwrap().asks(),
        [(102.0, 5.0), (103.0, 6.0)]
    );
    e.handle(102.0, Event::WsMessage {socket:s,text:r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"update","data":{"prevSeqId":"1","seqId":"2","bids":[["97","0"]],"asks":[["103","8"]]}}"#});
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(98.0, 5.0)]);
    assert_eq!(e.book("BTCUSDT").unwrap().asks()[1], (103.0, 8.0));
}

#[test]
fn candles_use_documented_volume_and_request_window() {
    let (mut e, s) = seeded();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(60_000.0,Event::WsMessage {socket:s,text:r#"{"arg":{"channel":"candle1m","instId":"BTC-USDT"},"data":[["60000","99","102","98","100","9","10","42"]]}"#});
    assert!(e.take_frame(60_000.0).is_some());
    e.handle(60_001.0,Event::WsMessage {socket:s,text:r#"{"arg":{"channel":"books5","instId":"BTC-USDT"},"data":{"ts":"60001","bids":[["99","1"]],"asks":[["101","1"]]}}"#});
    // books5 changed the book, but its median did not change the candle.
    let frame = e.take_frame(60_001.0).unwrap();
    let mut offset = 16;
    while offset < frame.len() {
        let kind = u16::from_le_bytes(frame[offset..offset + 2].try_into().unwrap());
        assert_ne!(kind, 6);
        let len = u32::from_le_bytes(frame[offset + 4..offset + 8].try_into().unwrap()) as usize;
        offset += 8 + len;
    }
    e.handle(
        120_000.0,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: Some(1.0),
            to_ms: None,
        }),
    );
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 120_000.)
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
        r.url
            .ends_with("instId=BTC-USDT&bar=1m&before=0&after=120000")
    );
    reply(
        &mut e,
        r.id,
        r#"{"code":"0","data":[["60000","99","102","98","100","9","10","42"],["0","90","100","89","99","1","2","3"]]}"#,
    );
    let result = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandDone { command: 7, result }) => {
                Some(result)
            }
            _ => None,
        })
        .unwrap();
    assert!(result.starts_with("[{\"timestamp\":0.0"));
    assert!(result.contains("\"volume\":42.0"));
}
#[test]
fn reconnect_restores_active_subscriptions_and_traffic_satisfies_watchdog() {
    let (mut e, s) = seeded();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1001.0, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
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
    let timer = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1002.0, Event::WsOpened { socket });
    let subscriptions: Vec<_> = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect();
    assert!(subscriptions.iter().any(|t| t.contains("candle1m")));
    assert!(subscriptions.iter().any(|t| t.contains("\"books\"")));
    e.handle(
        20_999.0,
        Event::WsMessage {
            socket,
            text: "{\"event\":\"subscribe\"}",
        },
    );
    e.handle(21_001.0, Event::TimerFired { timer });
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
}

#[test]
fn all_market_subscriptions_fit_4096_bytes() {
    let row = INSTRUMENT
        .split("\"data\":[")
        .nth(1)
        .unwrap()
        .trim_end_matches("]}");
    let rows = (0..200)
        .map(|i| row.replace("BTC-USDT", &format!("COIN{i}-USDT")))
        .collect::<Vec<_>>()
        .join(",");
    let mut e = trusted::trusted_engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(0.0);
    let id = request(&mut e);
    reply(&mut e, id, &format!("{{\"code\":\"0\",\"data\":[{rows}]}}"));
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
    let sends = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(sends.len() > 3);
    assert!(sends.iter().all(|s| s.len() <= 4096));
    assert_eq!(
        sends
            .iter()
            .map(|s| s.matches("\"instId\"").count())
            .sum::<usize>(),
        400
    );
}

#[test]
fn full_top_five_preserves_deeper_prices_and_thin_or_empty_sides_clear_them() {
    let (mut e, s) = seeded();
    let bids = (90..100).rev().map(|p| vec![p, 1]).collect::<Vec<_>>();
    let text=serde_json::json!({"arg":{"channel":"books","instId":"BTC-USDT"},"action":"snapshot","data":{"seqId":"9007199254740993","bids":bids,"asks":[[101,1]]}}).to_string();
    e.handle(
        1.0,
        Event::WsMessage {
            socket: s,
            text: &text,
        },
    );
    let bids = (94..99).rev().map(|p| vec![p, 2]).collect::<Vec<_>>();
    let text=serde_json::json!({"arg":{"channel":"books5","instId":"BTC-USDT"},"data":{"bids":bids,"asks":[]}}).to_string();
    e.handle(
        2.0,
        Event::WsMessage {
            socket: s,
            text: &text,
        },
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids().len(), 9);
    assert_eq!(e.book("BTCUSDT").unwrap().bids()[5], (93.0, 1.0));
    assert!(e.book("BTCUSDT").unwrap().asks().is_empty());
    e.handle(3.0,Event::WsMessage{socket:s,text:r#"{"arg":{"channel":"books","instId":"BTC-USDT"},"action":"update","data":{"prevSeqId":"9007199254740992","seqId":"9007199254740994","bids":[],"asks":[]}}"#});
    assert!(e.book("BTCUSDT").unwrap().is_empty());
}

#[test]
fn null_ticker_row_is_reported_without_hiding_other_symbols_or_halting_private_startup() {
    let mut engine = trusted::trusted_engine_with(
        Options::default(),
        Some(trusted::Keys::new("key", "secret", "pass")),
    );
    engine.start(0.0);
    let id = request(&mut engine);
    reply(&mut engine, id, INSTRUMENT);
    // Private startup begins at markets, before the ticker response.
    let mut actions = safe_cex_engine::test_support::prepared_actions(&mut engine, 0.);
    let id = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("tickers") => Some(r.id),
            _ => None,
        })
        .unwrap();
    reply(
        &mut engine,
        id,
        r#"{"code":"0","data":[{"instId":"UNKNOWN-USDT","last":null},{"instId":"BTC-USDT","last":"60000","open24h":"59000"}]}"#,
    );
    assert_eq!(engine.store().ticker("BTCUSDT").unwrap().last, 60000.0);
    assert!(engine.store().loaded().tickers);
    actions.extend(safe_cex_engine::test_support::prepared_actions(
        &mut engine,
        0.,
    ));
    assert!(
        actions.iter().any(
            |action| matches!(action, Action::WsConnect{url, ..} if url.ends_with("/private"))
        )
    );
    assert!(actions.iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(event)) if event.message.contains("Malformed BloFin ticker row"))));
}
