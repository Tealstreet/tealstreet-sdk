//! Oracles: WEEX futures Market_API/GetExchangeInfo, GetDepthData and streams
//! public/{Depth-Channel,Tickers-Channel,Kline-Channel} documented examples.
use safe_cex_engine::{Action, Command, Engine, Event, RequestId, Response, SocketId, TimerId};
mod support;
use safe_cex_venue_weex::WeexFutures;
use support::setup::Options;
fn engine() -> Engine<WeexFutures> {
    support::setup::engine(Options {
        public_data_only: true,
        ..Options::default()
    })
}
fn response(e: &mut Engine<WeexFutures>, id: RequestId, body: &str) {
    response_at(e, id, 1_773_295_700_000.0, body);
}
fn response_at(e: &mut Engine<WeexFutures>, id: RequestId, now: f64, body: &str) {
    e.handle(
        now,
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
fn request(actions: Vec<Action>, path: &str) -> RequestId {
    actions
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.id),
            _ => None,
        })
        .unwrap()
}
const INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"contractVal":0.000001,"minOrderSize":0.0001,"maxOrderSize":10000,"maxLeverage":408}]}"#;
fn started() -> (Engine<WeexFutures>, SocketId) {
    let mut e = engine();
    e.start(0.0);
    let id = request(e.drain_actions().collect(), "exchangeInfo");
    response(&mut e, id, INFO);
    let actions: Vec<_> = e.drain_actions().collect();
    let daily = request(actions.clone(), "24hr");
    let book = request(actions, "bookTicker");
    response(
        &mut e,
        daily,
        r#"[{"symbol":"BTCUSDT","lastPrice":"102623.90","priceChangePercent":"-1.96","volume":"176145.66489","quoteVolume":"18115688543.1"}]"#,
    );
    response(
        &mut e,
        book,
        r#"[{"symbol":"BTCUSDT","bidPrice":"102623.8","askPrice":"102623.9"}]"#,
    );
    let socket = e
        .drain_actions()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(1_773_295_700_000.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket)
}
#[test]
fn documented_public_payloads_and_ping() {
    let (mut e, socket) = started();
    assert_eq!(
        e.store().market("BTCUSDT").unwrap().precision.amount,
        0.0001
    );
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().bid, 102623.8);
    e.handle(1.0, Event::WsMessage { socket, text: r#"{"e":"ticker","s":"BTCUSDT","d":[{"c":"102620.0","m":"102621.0","i":"102615.50","P":"0.1","v":"18.2","q":"65035.88"}]}"# });
    assert_eq!(e.store().ticker("BTCUSDT").unwrap().percentage, 10.0);
    e.handle(
        2.0,
        Event::WsMessage {
            socket,
            text: r#"{"event":"ping","time":"1693208170000"}"#,
        },
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("PONG")))
    );
}
#[test]
fn depth_ignores_old_frames_and_heals_gaps_with_buffered_diffs() {
    let (mut e, socket) = started();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let id = request(e.drain_actions().collect(), "/depth");
    response(
        &mut e,
        id,
        r#"{"lastUpdateId":160,"bids":[["103435.9","2"]],"asks":[["103436.1","1.215"]]}"#,
    );
    e.handle(
        1.0,
        Event::WsMessage {
            socket,
            text: r#"{"e":"depth","s":"BTCUSDT","U":161,"u":161,"b":[["103435.9","2.1"]],"a":[]}"#,
        },
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(103435.9, 2.1)]);
    e.handle(
        2.0,
        Event::WsMessage {
            socket,
            text: r#"{"e":"depth","s":"BTCUSDT","U":160,"u":160,"b":[["103435.9","9"]],"a":[]}"#,
        },
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids()[0].1, 2.1);
    e.drain_actions().for_each(drop);
    e.handle(
        3.0,
        Event::WsMessage {
            socket,
            text: r#"{"e":"depth","s":"BTCUSDT","U":163,"u":163,"b":[["103435.9","0"]],"a":[]}"#,
        },
    );
    assert!(e.book("BTCUSDT").is_none());
    let id = request(e.drain_actions().collect(), "/depth");
    response(
        &mut e,
        id,
        r#"{"lastUpdateId":162,"bids":[["103435.9","2.1"]],"asks":[["103436.1","1.215"]]}"#,
    );
    assert!(e.book("BTCUSDT").unwrap().bids().is_empty());
}
#[test]
fn truncated_stream_does_not_load_markets() {
    let mut e = engine();
    e.start(0.0);
    let id = request(e.drain_actions().collect(), "exchangeInfo");
    e.handle(
        1.0,
        Event::HttpStart {
            request: id,
            status: 200,
            headers: "",
        },
    );
    e.handle(
        1.0,
        Event::HttpChunk {
            request: id,
            data: br#"{"symbols":[{"symbol":"BTCUSDT"}"#,
        },
    );
    e.handle(2.0, Event::HttpEnd { request: id });
    assert!(!e.store().loaded().markets);
}
#[test]
fn websocket_candles_allow_two_hours_but_rest_rejects_it() {
    let (mut e, socket) = started();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::H2,
        }),
    );
    assert!(
        e.drain_actions().any(
            |a| matches!(a, Action::WsSend { text, .. } if text.contains("kline_2h_LAST_PRICE"))
        )
    );
    e.handle(1.0, Event::WsMessage { socket, text: r#"{"e":"kline","s":"BTCUSDT","d":[{"i":"2h","t":1773295680000,"o":"3572.1","c":"3573.4","h":"3574","l":"3571.8","v":"18.2"}]}"# });
    assert!(e.take_frame(2.0).is_some());
    e.handle(
        3.0,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::H2,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 7, .. })
    )));
}
#[test]
fn silent_socket_is_closed_and_scheduled_for_reconnect() {
    let mut e = engine();
    e.start(0.0);
    let id = request(e.drain_actions().collect(), "exchangeInfo");
    response(&mut e, id, INFO);
    let reqs: Vec<_> = e.drain_actions().collect();
    response(
        &mut e,
        request(reqs.clone(), "24hr"),
        r#"[{"symbol":"BTCUSDT","lastPrice":"100"}]"#,
    );
    response(&mut e, request(reqs, "bookTicker"), "[]");
    let actions: Vec<_> = e.drain_actions().collect();
    let (socket, timer): (SocketId, TimerId) = (
        actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap(),
        actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, .. } => Some(*timer),
                _ => None,
            })
            .unwrap(),
    );
    e.handle(1_773_295_740_000.0, Event::TimerFired { timer });
    assert!(e.drain_actions().any(|a| a == Action::WsClose { socket }));
}

#[test]
fn old_snapshots_and_failures_cannot_replace_a_resubscribed_book() {
    let (mut e, _) = started();
    let subscribe = || {
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        })
    };
    e.handle(1.0, subscribe());
    let old = request(e.drain_actions().collect(), "/depth");
    e.handle(
        2.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(3.0, subscribe());
    let new = request(e.drain_actions().collect(), "/depth");
    response(
        &mut e,
        new,
        r#"{"lastUpdateId":200,"bids":[["100","2"]],"asks":[]}"#,
    );
    response(
        &mut e,
        old,
        r#"{"lastUpdateId":100,"bids":[["100","1"]],"asks":[]}"#,
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(100.0, 2.0)]);
    e.handle(
        4.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(5.0, subscribe());
    let old = request(e.drain_actions().collect(), "/depth");
    e.handle(
        6.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.handle(7.0, subscribe());
    let new = request(e.drain_actions().collect(), "/depth");
    response(
        &mut e,
        new,
        r#"{"lastUpdateId":300,"bids":[["100","3"]],"asks":[]}"#,
    );
    e.handle(
        8.0,
        Event::HttpFailed {
            request: old,
            error: "late reset",
        },
    );
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(100.0, 3.0)]);
}
#[test]
fn malformed_depth_is_not_an_authoritative_deletion() {
    let (mut e, socket) = started();
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let id = request(e.drain_actions().collect(), "/depth");
    response(
        &mut e,
        id,
        r#"{"lastUpdateId":160,"bids":[["100","2"]],"asks":[]}"#,
    );
    e.handle(
        1.0,
        Event::WsMessage {
            socket,
            text: r#"{"e":"depth","s":"BTCUSDT","U":161,"u":161,"b":[["100","oops"]],"a":[]}"#,
        },
    );
    assert!(e.book("BTCUSDT").is_none());
}
#[test]
fn malformed_envelopes_cannot_commit_streamed_markets() {
    for body in [
        INFO.trim_end_matches('}').to_owned(),
        INFO.replace("]}", ",,]}"),
        format!("{INFO} garbage"),
    ] {
        let mut e = engine();
        e.start(0.0);
        let id = request(e.drain_actions().collect(), "exchangeInfo");
        response(&mut e, id, &body);
        assert!(!e.store().loaded().markets, "{body}");
    }
}

#[test]
fn requested_candles_decode_variable_length_rows_and_sort_by_open_time() {
    let (mut e, _) = started();
    e.handle(
        1.0,
        Event::Command(Command::FetchCandles {
            command: 8,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(200),
            from_ms: Some(1_700_000_000_000.0),
            to_ms: Some(1_700_000_120_000.0),
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let id = request(actions.clone(), "historyKlines");
    assert!(actions.iter().any(|a| matches!(a, Action::Http(r) if r.url.contains("limit=100") && r.url.contains("priceType=LAST"))));
    response(
        &mut e,
        id,
        r#"[[1700000060000,"3572.1","3574","3571.8","3573.4","18.2"],[1700000000000,"3571","3573","3570","3572","10",1700000059999,"35720",9,"5","17860"]]"#,
    );
    let result = e
        .drain_actions()
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandDone {
                command: 8,
                result: s,
            }) => Some(s),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        result,
        r#"[{"timestamp":1700000000.0,"open":3571.0,"high":3573.0,"low":3570.0,"close":3572.0,"volume":10.0},{"timestamp":1700000060.0,"open":3572.1,"high":3574.0,"low":3571.8,"close":3573.4,"volume":18.2}]"#
    );
}

#[test]
fn active_tickers_send_incremental_diffs_and_identical_updates_send_nothing() {
    let (mut e, _) = started();
    e.handle(
        1.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["ETHUSDT".into()],
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("UNSUBSCRIBE") && text.contains("BTCUSDT@ticker"))));
    e.handle(
        2.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["BTCUSDT".into()],
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("SUBSCRIBE") && text.contains("BTCUSDT@ticker"))));
    e.handle(
        3.0,
        Event::Command(Command::SetTickerSymbols {
            symbols: vec!["BTCUSDT".into()],
        }),
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
}

#[test]
fn public_candle_reads_wait_for_the_shared_ip_ban() {
    let (mut e, _) = started();
    e.handle(
        10.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let depth = request(e.drain_actions().collect(), "/depth");
    e.handle(
        11.0,
        Event::HttpResponse {
            request: depth,
            response: Response {
                status: 429,
                headers: "retry-after: 2\r\n",
                body: "rate limit",
            },
        },
    );
    e.drain_actions().for_each(drop);
    e.handle(
        12.0,
        Event::Command(Command::FetchCandles {
            command: 9,
            symbol: "BTCUSDT".into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let timer = actions
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(2011.0, Event::TimerFired { timer });
    request(e.drain_actions().collect(), "/market/klines");
}
#[test]
fn null_or_malformed_public_rows_are_skipped_without_stopping_startup() {
    let mut e = engine();
    e.start(0.0);
    let id = request(e.drain_actions().collect(), "exchangeInfo");
    let info = r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6,"makerFeeRate":null,"takerFeeRate":"0.0006"},{"symbol":"ETHUSDT","baseAsset":"ETH","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":2,"quantityPrecision":3},{"symbol":"BADUSDT","baseAsset":null,"quoteAsset":"USDT","marginAsset":"USDT"}]}"#;
    response(&mut e, id, info);
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().market("BTCUSDT").unwrap().maker_fee, Some(0.0));
    let actions: Vec<_> = e.drain_actions().collect();
    let warnings: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => Some(message.clone()),
            _ => None,
        })
        .collect();
    assert!(
        warnings.iter().any(|w| w.contains("BADUSDT")),
        "{warnings:?}"
    );
    let daily = request(actions.clone(), "24hr");
    let book = request(actions, "bookTicker");
    response(
        &mut e,
        daily,
        r#"[{"symbol":"BTCUSDT","lastPrice":"102623.90","openInterest":null,"volume":null,"quoteVolume":"1"},{"symbol":"ETHUSDT","lastPrice":null}]"#,
    );
    response(
        &mut e,
        book,
        r#"[{"symbol":"BTCUSDT","bidPrice":null,"askPrice":"102623.9"},{"symbol":"ETHUSDT","bidPrice":"2999","askPrice":"3000"}]"#,
    );
    assert!(e.store().loaded().tickers);
    let btc = e.store().ticker("BTCUSDT").unwrap();
    // BTC's book row was skipped, so its bid falls back to the last price.
    assert_eq!((btc.last, btc.bid, btc.volume), (102623.9, 102623.9, 0.0));
    // ETH has only a book row: no last/mark/index, so no ticker.
    assert!(e.store().ticker("ETHUSDT").is_none());
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}
#[test]
fn exchange_info_with_only_malformed_rows_retries_startup() {
    let mut e = engine();
    e.start(0.0);
    let id = request(e.drain_actions().collect(), "exchangeInfo");
    response(
        &mut e,
        id,
        r#"{"symbols":[{"symbol":"BTCUSDT","baseAsset":null,"quoteAsset":"USDT","marginAsset":"USDT"}]}"#,
    );
    assert!(!e.store().loaded().markets);
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .expect("startup retry");
    e.handle(1_773_295_710_000.0, Event::TimerFired { timer });
    request(e.drain_actions().collect(), "exchangeInfo");
}

#[test]
fn a_market_without_a_positive_step_is_skipped_as_malformed() {
    for bad in [
        r#""pricePrecision":2,"quantityPrecision":3,"minOrderSize":"0""#,
        r#""pricePrecision":2,"quantityPrecision":3,"minOrderSize":"-0.001""#,
        r#""pricePrecision":400,"quantityPrecision":3"#,
    ] {
        let mut e = engine();
        e.start(0.0);
        let id = request(e.drain_actions().collect(), "exchangeInfo");
        let info = format!(
            r#"{{"symbols":[{{"symbol":"BTCUSDT","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","pricePrecision":1,"quantityPrecision":6}},{{"symbol":"ETHUSDT","baseAsset":"ETH","quoteAsset":"USDT","marginAsset":"USDT",{bad}}}]}}"#
        );
        response(&mut e, id, &info);
        assert!(e.store().market("BTCUSDT").is_some());
        assert!(e.store().market("ETHUSDT").is_none(), "{bad}");
        assert!(e.drain_actions().any(|a| matches!(a,
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. }))
                if message.contains("ETHUSDT"))));
    }
}

#[test]
fn rest_book_queries_leave_live_depth_generation_and_ticker_account_state_unchanged() {
    let (mut e, socket) = started();
    let at = 1_773_295_700_000.0;
    e.handle(
        at + 1.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let id = request(e.drain_actions().collect(), "/depth");
    response_at(
        &mut e,
        id,
        at + 1.5,
        r#"{"lastUpdateId":160,"bids":[["100","2"]],"asks":[]}"#,
    );
    e.drain_actions().for_each(drop);
    let before = (
        e.store().tickers().to_vec(),
        e.store().balance().clone(),
        e.store().positions().to_vec(),
        e.store().orders().to_vec(),
    );
    for (command, status, body) in [
        (
            7,
            200,
            r#"{"lastUpdateId":"999999999999999999999999","bids":[["1","3"]],"asks":[]}"#,
        ),
        (8, 200, r#"{"code":40006,"msg":"invalid api key"}"#),
    ] {
        let now = at + 2.0 + f64::from(command - 7) * 2.0;
        e.handle(
            now,
            Event::Command(Command::FetchOrderBook {
                command,
                symbol: "BTCUSDT".into(),
                limit: Some(1),
            }),
        );
        let requests: Vec<_> = safe_cex_engine::test_support::prepared_actions(&mut e, now)
            .into_iter()
            .filter_map(|action| match action {
                Action::Http(request) => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(requests.len(), 1);
        let request = &requests[0];
        e.handle(
            now + 1.0,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        if command == 7 {
            let result = actions
                .iter()
                .find_map(|a| match a {
                    Action::Notice(safe_cex_engine::Notice::CommandDone { command: 7, result }) => {
                        Some(serde_json::from_str::<serde_json::Value>(result).unwrap())
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(result["bids"][0]["amount"], 3.0);
            assert_eq!(result["bids"][0]["total"], 3.0);
        } else {
            assert!(actions.iter().any(|a| matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 8, .. })
            )));
        }
        assert!(!actions.iter().any(|a| matches!(
            a,
            Action::Http(_)
                | Action::WsConnect { .. }
                | Action::WsClose { .. }
                | Action::SetTimer { .. }
                | Action::Notice(safe_cex_engine::Notice::AccountAuth { .. })
        )));
        assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(100.0, 2.0)]);
        assert_eq!(
            (
                e.store().tickers().to_vec(),
                e.store().balance().clone(),
                e.store().positions().to_vec(),
                e.store().orders().to_vec()
            ),
            before
        );
    }
    e.handle(
        at + 6.0,
        Event::WsMessage {
            socket,
            text: r#"{"e":"depth","s":"BTCUSDT","U":161,"u":161,"b":[["100","4"]],"a":[]}"#,
        },
    );
    e.drain_actions().for_each(drop);
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(100.0, 4.0)]);
    e.handle(
        at + 7.0,
        Event::Command(Command::FetchOrderBook {
            command: 9,
            symbol: "BTCUSDT".into(),
            limit: None,
        }),
    );
    let query = request(
        safe_cex_engine::test_support::prepared_actions(&mut e, at + 7.0),
        "/depth",
    );
    e.handle(
        at + 8.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.handle(
        at + 9.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let seed = request(e.drain_actions().collect(), "/depth");
    response_at(
        &mut e,
        query,
        at + 10.0,
        r#"{"lastUpdateId":9999,"bids":[["1","3"]],"asks":[]}"#,
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone { command: 9, .. })
    )));
    assert!(e.book("BTCUSDT").is_none());
    response_at(
        &mut e,
        seed,
        at + 11.0,
        r#"{"lastUpdateId":200,"bids":[["99","2"]],"asks":[]}"#,
    );
    e.drain_actions().for_each(drop);
    assert_eq!(e.book("BTCUSDT").unwrap().bids(), [(99.0, 2.0)]);
}
