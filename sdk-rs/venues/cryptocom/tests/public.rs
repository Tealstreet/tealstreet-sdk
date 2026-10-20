//! Payload shapes: https://exchange-developer.crypto.com/exchange/v1/
use safe_cex_engine::{Action, Command, Engine, Event, Response, SocketId};
use safe_cex_venue_cryptocom::{Cryptocom, Options};

fn answer(engine: &mut Engine<Cryptocom>, body: &str) {
    let request = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpStart {
            request,
            status: 200,
            headers: "",
        },
    );
    for chunk in body.as_bytes().chunks(7) {
        engine.handle(
            0.0,
            Event::HttpChunk {
                request,
                data: chunk,
            },
        );
    }
    engine.handle(0.0, Event::HttpEnd { request });
}
fn ready() -> (Engine<Cryptocom>, SocketId) {
    let mut engine = Engine::new(Cryptocom::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    engine.start(0.0);
    answer(
        &mut engine,
        r#"{"id":1,"code":0,"result":{"data":[{"symbol":"BTCUSD-PERP","inst_type":"PERPETUAL_SWAP","tradable":true,"base_ccy":"BTC","quote_ccy":"USD","price_tick_size":"0.1","qty_tick_size":"0.0001","quote_decimals":1,"quantity_decimals":4,"max_leverage":"50","underlying_symbol":"BTCUSD-INDEX"}]}}"#,
    );
    answer(
        &mut engine,
        r#"{"id":2,"result":{"data":[{"i":"SHIB_BTC","b":null,"oi":null,"a":null},{"i":"BTCUSD-PERP","b":null,"k":"101","a":"100.5","c":"-0.01","v":"2","vv":"201"}]},"code":0}"#,
    );
    let socket = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    engine.handle(0.0, Event::WsOpened { socket });
    (engine, socket)
}
#[test]
fn nested_chunked_lists_seed_markets_and_tickers() {
    let (engine, _) = ready();
    assert_eq!(engine.store().markets().len(), 1);
    assert_eq!(engine.store().tickers()[0].percentage, -1.0);
    assert_eq!(engine.store().tickers()[0].bid, 0.0);
    assert_eq!(engine.store().markets()[0].precision.amount, 0.0001);
}
#[test]
fn automatic_seed_budget_ignores_early_books_and_counts_unique_channels() {
    for market_count in [200, 351] {
        let mut engine = Engine::new(Cryptocom::new(Options {
            public_data_only: true,
            ..Default::default()
        }));
        engine.start(0.0);
        for symbol in ["M000USD-PERP", "M001USD-PERP"] {
            engine.handle(
                0.0,
                Event::Command(Command::SubscribeOrderBook {
                    symbol: symbol.into(),
                }),
            );
        }
        let instruments = (0..market_count).map(|i| format!(
        r#"{{"symbol":"M{i:03}USD-PERP","inst_type":"PERPETUAL_SWAP","tradable":true,"base_ccy":"M{i:03}","quote_ccy":"USD","quote_decimals":1,"quantity_decimals":4,"max_leverage":"50","underlying_symbol":"SHAREDUSD-INDEX"}}"#
    )).collect::<Vec<_>>().join(",");
        answer(
            &mut engine,
            &format!(r#"{{"code":0,"result":{{"data":[{instruments}]}}}}"#),
        );
        answer(&mut engine, r#"{"code":0,"result":{"data":[]}}"#);
        let socket = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(socket),
                _ => None,
            })
            .unwrap();
        engine.handle(0.0, Event::WsOpened { socket });
        let timer = engine
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer {
                    timer,
                    at_ms: 1000.0,
                } => Some(timer),
                _ => None,
            })
            .unwrap();
        engine.handle(1000.0, Event::TimerFired { timer });
        let channels = engine
            .drain_actions()
            .filter_map(|a| match a {
                Action::WsSend { text, .. } => Some(text),
                _ => None,
            })
            .flat_map(|text| {
                let body: serde_json::Value = serde_json::from_str(&text).unwrap();
                let channels = body["params"]["channels"].as_array().unwrap();
                assert!(channels.len() <= 100);
                channels
                    .iter()
                    .map(|channel| channel.as_str().unwrap().to_owned())
                    .collect::<Vec<_>>()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(channels.len(), 352);
        for i in 0..market_count.min(350) {
            assert!(channels.contains(&format!("ticker.M{i:03}USD-PERP")));
        }
        if market_count > 350 {
            assert!(!channels.contains("ticker.M350USD-PERP"));
        } else {
            assert!(channels.contains("index.SHAREDUSD-INDEX"));
            assert!(channels.contains("mark.M074USD-PERP"));
            assert!(!channels.contains("funding.M074USD-PERP"));
        }
        assert!(channels.contains("book.M000USD-PERP.50"));
        assert!(channels.contains("book.M001USD-PERP.50"));
    }
}
#[test]
fn snapshots_replace_both_sides_and_heartbeats_keep_the_id() {
    let (mut engine, socket) = ready();
    engine.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSD-PERP".into(),
        }),
    );
    for text in [
        r#"{"id":-1,"code":0,"result":{"channel":"book","instrument_name":"BTCUSD-PERP","data":[{"bids":[["100","1","1"],["99","2","1"]],"asks":[["101","3","1"]]}]}}"#,
        r#"{"id":-1,"code":0,"result":{"channel":"book","instrument_name":"BTCUSD-PERP","data":[{"bids":[["99","4","1"]],"asks":[]}]}}"#,
    ] {
        engine.handle(10.0, Event::WsMessage { socket, text });
    }
    let book = engine.book("BTCUSD-PERP").unwrap();
    assert_eq!(book.bids(), [(99.0, 4.0)]);
    assert!(book.asks().is_empty());
    engine.handle(
        20.0,
        Event::WsMessage {
            socket,
            text: r#"{"id":1587523073344,"method":"public/heartbeat","code":0}"#,
        },
    );
    assert!(engine.drain_actions().any(|a| matches!(a, Action::WsSend { text, .. } if text == r#"{"id":1587523073344,"method":"public/respond-heartbeat"}"#)));
}
#[test]
fn valuations_distinguish_current_funding_and_index_instruments() {
    let (mut engine, socket) = ready();
    for text in [
        r#"{"result":{"channel":"index","instrument_name":"BTCUSD-INDEX","data":[{"v":"102"}]}}"#,
        r#"{"result":{"channel":"mark","instrument_name":"BTCUSD-PERP","data":[{"v":"103"}]}}"#,
        r#"{"result":{"channel":"mark","instrument_name":"BTCUSD-PERP","data":[{"v":"0"}]}}"#,
        r#"{"result":{"channel":"funding","instrument_name":"BTCUSD-PERP","data":[{"v":"-0.0001"}]}}"#,
        r#"{"result":{"channel":"estimatedfunding","instrument_name":"BTCUSD-PERP","data":[{"v":"1"}]}}"#,
    ] {
        engine.handle(1.0, Event::WsMessage { socket, text });
    }
    let t = &engine.store().tickers()[0];
    assert_eq!((t.index, t.mark, t.funding_rate), (102.0, 103.0, -0.0001));
}
#[test]
fn code_after_array_is_checked_before_committing() {
    let mut engine = Engine::new(Cryptocom::new(Options::default()));
    engine.start(0.0);
    answer(
        &mut engine,
        r#"{"result":{"data":[]},"code":40101,"message":"bad key"}"#,
    );
    assert!(!engine.store().loaded().markets);
}
#[test]
fn failed_candle_reads_complete_with_error() {
    let (mut engine, _) = ready();
    engine.handle(
        0.0,
        Event::Command(Command::FetchCandles {
            command: 9,
            symbol: "BTCUSD-PERP".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    let request = engine
        .drain_actions()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    engine.handle(
        0.0,
        Event::HttpResponse {
            request,
            response: Response {
                status: 500,
                headers: "",
                body: "unavailable",
            },
        },
    );
    assert!(engine.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone {
            command: 9,
            result: Err(_)
        })
    )));
}

#[test]
fn malformed_streamed_array_never_commits_empty() {
    for body in [
        r#"{"result":{"data":[,]},"code":0}"#,
        r#"{"result":{"data":[{},]},"code":0}"#,
    ] {
        let mut engine = Engine::new(Cryptocom::new(Options::default()));
        engine.start(0.0);
        answer(&mut engine, body);
        assert!(!engine.store().loaded().markets);
    }
}
