//! Payloads from the official info/perpetuals and websocket/subscriptions docs.
//! https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api
use safe_cex_engine::{Action, Command, Engine, Event, RequestId, Response, SocketId};
use safe_cex_venue_hyperliquid::{Hyperliquid, Options};

fn response(engine: &mut Engine<Hyperliquid>, request: RequestId, body: &str) {
    engine.handle(
        1000.0,
        Event::HttpResponse {
            request,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn request(engine: &mut Engine<Hyperliquid>) -> RequestId {
    engine
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap()
}
fn start() -> (Engine<Hyperliquid>, SocketId) {
    let mut e = Engine::new(Hyperliquid::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(1000.0);
    let id = request(&mut e);
    response(
        &mut e,
        id,
        r#"[{"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50},{"name":"ETH","szDecimals":4,"maxLeverage":50}]},[{"midPx":"97500.5","markPx":"97500.5","oraclePx":"97500","prevDayPx":"95000","dayNtlVlm":"195001","openInterest":"2","funding":"0.0001"},{"midPx":"3650.25","markPx":"3650.25","szDecimals":4}]]"#,
    );
    let id = request(&mut e);
    response(&mut e, id, "[null]");
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
    e.handle(1000.0, Event::WsOpened { socket });
    (e, socket)
}
fn message(e: &mut Engine<Hyperliquid>, socket: SocketId, text: &str) {
    e.handle(2000.0, Event::WsMessage { socket, text });
}
#[test]
fn markets_units_and_compressed_partial_fields() {
    let (mut e, s) = start();
    assert_eq!(e.store().markets().len(), 2);
    let btc = e.store().ticker("BTC-PERP").unwrap();
    assert_eq!(
        (btc.volume, btc.quote_volume, btc.open_interest),
        (2.0, 195001.0, 195001.0)
    );
    message(
        &mut e,
        s,
        r#"{"channel":"allMids","data":{"mids":{"BTC":"98000"}}}"#,
    );
    message(
        &mut e,
        s,
        r#"{"channel":"fastAssetCtxs","data":"q1ZyCnFWsqpWyk0syg6oULJSsjQ3NTDQM1Wq1VFyDfFAkTI2MzXQMwJLVVRWWfmFuTiiyBuamOoZKdXWAgA="}"#,
    );
    let btc = e.store().ticker("BTC-PERP").unwrap();
    assert_eq!((btc.last, btc.mark), (98000.0, 97500.5));
    message(&mut e, s, r#"{"channel":"fastAssetCtxs","data":"broken"}"#);
    assert_eq!(e.store().ticker("BTC-PERP").unwrap().last, 98000.0);
}
#[test]
fn full_depth_bbo_overlay_stale_bbo_and_reconnect() {
    let (mut e, s) = start();
    e.handle(
        1000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-PERP".into(),
        }),
    );
    message(
        &mut e,
        s,
        r#"{"channel":"l2Book","data":{"coin":"BTC","time":100,"levels":[[{"px":"100","sz":"2","n":1},{"px":"99","sz":"3","n":1},{"px":"98","sz":"4","n":1}],[{"px":"101","sz":"5","n":1},{"px":"102","sz":"6","n":1}]]}}"#,
    );
    message(
        &mut e,
        s,
        r#"{"channel":"bbo","data":{"coin":"BTC","time":101,"bbo":[{"px":"99","sz":"7","n":1},{"px":"102","sz":"8","n":1}]}}"#,
    );
    let b = e.book("BTC-PERP").unwrap();
    assert_eq!(b.bids(), vec![(99.0, 7.0), (98.0, 4.0)]);
    assert_eq!(b.asks(), vec![(102.0, 8.0)]);
    message(
        &mut e,
        s,
        r#"{"channel":"bbo","data":{"coin":"BTC","time":99,"bbo":[null,null]}}"#,
    );
    assert_eq!(e.book("BTC-PERP").unwrap().bids().len(), 2);
    e.handle(
        3000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    assert!(e.book("BTC-PERP").is_none());
}
#[test]
fn posted_candle_read_preserves_body_and_streams() {
    let (mut e, _) = start();
    e.drain_actions().for_each(drop);
    e.handle(
        1000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTC-PERP".into(),
            interval: "1m".into(),
            limit: Some(1),
            from_ms: None,
            to_ms: Some(60000.0),
        }),
    );
    let req = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(
        req.body.as_deref(),
        Some(
            r#"{"type":"candleSnapshot","req":{"coin":"BTC","interval":"1m","startTime":0.0,"endTime":60000.0}}"#
        )
    );
    response(
        &mut e,
        req.id,
        r#"[{"t":0,"o":"1","h":"2","l":"1","c":"2","v":"3"}]"#,
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone { result: Ok(_), .. })
    )));
}
#[test]
fn silence_watchdog_closes_without_a_close_event() {
    let (mut e, _) = start();
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
    e.handle(50000.0, Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { .. }))
    );
}
#[test]
fn incomplete_metadata_never_commits_and_schedules_a_read_retry() {
    for body in [
        "[]",
        r#"[{"universe":[]}]"#,
        r#"[[],{"universe":[]}]"#,
        r#"[{"universe":[{"name":"BTC","szDecimals":5}]},[]]"#,
    ] {
        let mut e = Engine::new(Hyperliquid::new(Options::default()));
        e.start(0.0);
        let id = request(&mut e);
        response(&mut e, id, body);
        assert!(!e.store().loaded().markets);
        assert!(
            e.drain_actions()
                .any(|a| matches!(a, Action::SetTimer { .. }))
        );
    }
}
#[test]
fn invalid_interval_is_a_command_error_and_limit_caps_at_1500() {
    let (mut e, _) = start();
    e.drain_actions().for_each(drop);
    e.handle(
        1000.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "BTC-PERP".into(),
            interval: "💥".into(),
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone { result: Err(_), .. })
    )));
    e.handle(
        1000.0,
        Event::Command(Command::FetchCandles {
            command: 2,
            symbol: "BTC-PERP".into(),
            interval: "1m".into(),
            limit: Some(2000),
            from_ms: Some(1.0),
            to_ms: Some(90_000_000.0),
        }),
    );
    let r = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    assert!(r.body.as_deref().unwrap().contains("\"startTime\":0.0"));
    response(
        &mut e,
        r.id,
        r#"[{"t":0,"o":"NaN","h":"2","l":"1","c":"2","v":"3"}]"#,
    );
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandDone { result: Err(_), .. })
    )));
}
