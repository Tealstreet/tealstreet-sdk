//! Oracles: WEEX futures Market_API/GetExchangeInfo, GetDepthData and streams
//! public/{Depth-Channel,Tickers-Channel,Kline-Channel} documented examples.
use safe_cex_engine::{Action, Command, Engine, Event, RequestId, Response, SocketId, TimerId};
use safe_cex_venue_weex::{Options, WeexFutures};
fn engine() -> Engine<WeexFutures> {
    Engine::new(WeexFutures::new(Options {
        public_data_only: true,
        ..Options::default()
    }))
}
fn response(e: &mut Engine<WeexFutures>, id: RequestId, body: &str) {
    e.handle(
        1_773_295_700_000.0,
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
            interval: "2h".into(),
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
            interval: "2h".into(),
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone {
            command: 7,
            result: Err(_)
        })
    )));
}
#[test]
fn silent_socket_is_closed_and_scheduled_for_reconnect() {
    let mut e = engine();
    e.start(0.0);
    let id = request(e.drain_actions().collect(), "exchangeInfo");
    response(&mut e, id, INFO);
    let reqs: Vec<_> = e.drain_actions().collect();
    response(&mut e, request(reqs.clone(), "24hr"), "[]");
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
