mod support;
use safe_cex_engine::{Action, Command, Engine, Event, SocketChannel};
use safe_cex_venue_kraken::{Kraken, Options};
use support::*;
fn public() -> (Engine<Kraken>, safe_cex_engine::SocketId) {
    ready_with(Options {
        public_data_only: true,
        ..Default::default()
    })
}
fn sub(e: &mut Engine<Kraken>) {
    e.handle(
        NOW,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "PF_XBTUSD".into(),
        }),
    );
    e.drain_actions().for_each(drop);
}
#[test]
fn markets_and_rest_tickers_use_native_units() {
    let (e, _) = public();
    let m = e.store().market("PF_XBTUSD").unwrap();
    assert_eq!(m.precision.amount, 0.001);
    assert_eq!(m.limits.leverage.max, 50.0);
    let t = e.store().ticker("PF_XBTUSD").unwrap();
    assert_eq!(
        (t.last, t.mark, t.index, t.volume, t.quote_volume),
        (100.0, 101.0, 102.0, 2.0, 200.0)
    );
    assert_eq!(t.funding_rate, 0.0001);
}
#[test]
fn books_keep_full_depth_and_require_contiguous_sequence() {
    let (mut e, s) = public();
    sub(&mut e);
    let levels = (1..=150)
        .map(|p| format!("{{\"price\":{p},\"qty\":2}}"))
        .collect::<Vec<_>>()
        .join(",");
    message(
        &mut e,
        s,
        NOW,
        &format!(
            "{{\"feed\":\"book_snapshot\",\"product_id\":\"PF_XBTUSD\",\"seq\":10,\"bids\":[{levels}],\"asks\":[]}}"
        ),
    );
    assert_eq!(e.book("PF_XBTUSD").unwrap().bids().len(), 150);
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"book","product_id":"PF_XBTUSD","seq":11,"side":"buy","price":150,"qty":0}"#,
    );
    assert_eq!(e.book("PF_XBTUSD").unwrap().best_bid(), Some(149.0));
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"book","product_id":"PF_XBTUSD","seq":13,"side":"buy","price":151,"qty":1}"#,
    );
    assert!(e.book("PF_XBTUSD").is_none());
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsSend{text,..}if text.contains("unsubscribe")))
    );
}
#[test]
fn stale_sockets_and_pre_snapshot_deltas_do_not_mutate() {
    let (mut e, s) = public();
    sub(&mut e);
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"book","product_id":"PF_XBTUSD","seq":1,"side":"buy","price":100,"qty":1}"#,
    );
    assert!(e.book("PF_XBTUSD").is_none());
    e.handle(
        NOW,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":900}"#,
    );
    assert_eq!(e.store().ticker("PF_XBTUSD").unwrap().last, 100.0);
}
#[test]
fn ws_tickers_read_relative_funding_and_partial_updates() {
    let (mut e, s) = public();
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":110,"markPrice":111,"relative_funding_rate":0.000006,"funding_rate":0.207,"next_funding_rate_time":1700000100,"open24h":100}"#,
    );
    let t = e.store().ticker("PF_XBTUSD").unwrap();
    assert_eq!(
        (t.bid, t.ask, t.last, t.mark, t.funding_rate, t.funding_time),
        (99.0, 101.0, 110.0, 111.0, 0.000006, 1700000100.0)
    );
    assert!((t.percentage - 10.0).abs() < 1e-10);
}
#[test]
fn charts_are_public_and_convert_ms_to_seconds() {
    let (mut e, _) = public();
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "PF_XBTUSD".into(),
            interval: "1m".into(),
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.headers.is_empty());
    assert!(
        r.url
            .contains("/api/charts/v1/trade/PF_XBTUSD/1m?from=1699999880&to=1700000000")
    );
    reply(
        &mut e,
        &r,
        200,
        r#"{"candles":[{"time":1700000000000,"open":"1","high":"2","low":"0.5","close":"1.5","volume":"3"}]}"#,
    );
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::CommandDone{command:7,result:Ok(s)})if s.contains("1700000000.0"))));
}
#[test]
fn public_socket_role_is_explicit() {
    let mut e = Engine::new(Kraken::new(Options {
        public_data_only: true,
        ..Default::default()
    }));
    e.start(NOW);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, META);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, TICKERS);
    assert!(e.drain_actions().any(|a| matches!(
        a,
        Action::WsConnect {
            channel: Some(SocketChannel::Public),
            ..
        }
    )));
}
#[test]
fn old_candle_reply_cannot_seed_a_recreated_subscription() {
    let (mut e, _) = public();
    let c = || Command::SubscribeCandles {
        symbol: "PF_XBTUSD".into(),
        interval: "1m".into(),
    };
    e.handle(NOW, Event::Command(c()));
    let old = requests(&mut e).pop().unwrap();
    e.take_frame(NOW);
    e.handle(
        NOW,
        Event::Command(Command::UnsubscribeCandles {
            symbol: "PF_XBTUSD".into(),
            interval: "1m".into(),
        }),
    );
    e.handle(NOW, Event::Command(c()));
    let new = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &old,
        200,
        r#"{"candles":[{"time":1700000000000,"open":"9","high":"9","low":"9","close":"9","volume":"9"}]}"#,
    );
    assert!(e.take_frame(NOW).is_none());
    reply(&mut e, &new, 200, r#"{"candles":[]}"#);
}
#[test]
fn old_or_malformed_snapshot_cannot_replace_synced_depth() {
    let (mut e, s) = public();
    sub(&mut e);
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"book_snapshot","product_id":"PF_XBTUSD","seq":10,"bids":[{"price":100,"qty":2}],"asks":[]}"#,
    );
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"book_snapshot","product_id":"PF_XBTUSD","seq":9,"bids":[{"price":90,"qty":1}],"asks":[]}"#,
    );
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"book_snapshot","product_id":"PF_XBTUSD","seq":11}"#,
    );
    assert_eq!(e.book("PF_XBTUSD").unwrap().best_bid(), Some(100.0));
}
#[test]
fn failed_or_incomplete_candles_are_explicit_errors() {
    for body in [
        r#"{"candles":[],"error":"unavailable"}"#,
        r#"{"candles":[],"more_candles":true}"#,
        r#"{"status":"error"}"#,
        r#"{"candles":null}"#,
        r#"{"candles":{}}"#,
    ] {
        let (mut e, _) = public();
        e.handle(
            NOW,
            Event::Command(Command::FetchCandles {
                command: 9,
                symbol: "PF_XBTUSD".into(),
                interval: "1m".into(),
                limit: Some(2),
                from_ms: None,
                to_ms: None,
            }),
        );
        let r = requests(&mut e).pop().unwrap();
        reply(&mut e, &r, 200, body);
        assert!(e.drain_actions().any(|a| matches!(
            a,
            Action::Notice(safe_cex_engine::Notice::CommandDone {
                command: 9,
                result: Err(_)
            })
        )));
    }
}
#[test]
fn live_instrument_aliases_and_negative_precision_are_valid() {
    let mut e = Engine::new(Kraken::new(Options::default()));
    e.start(NOW);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"{"result":"success","instruments":[{"symbol":"PF_PEPEUSD","base":"PEPE","underlying":"PEPE:USD","quote":"USD","tradeable":true,"contractValueTradePrecision":-3,"contractSize":1,"tickSize":0.0000000001}]}"#,
    );
    assert_eq!(
        e.store().market("PF_PEPEUSD").unwrap().precision.amount,
        1000.0
    );
}
#[test]
fn malformed_credentials_report_a_signing_error_before_private_activation() {
    let mut e = Engine::new(Kraken::new(Options {
        key: Some("fixture".into()),
        secret: Some("invalid base64!".into()),
        ..Default::default()
    }));
    e.start(NOW);
    assert!(e.drain_actions().any(|a|matches!(a,Action::Notice(safe_cex_engine::Notice::Log{level:safe_cex_engine::LogLevel::Error,message})if message.contains("base64"))));
}

#[test]
fn full_ticker_omitted_funding_resets_zero() {
    let (mut e, s) = public();
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":110,"relative_funding_rate":0.0002}"#,
    );
    assert_eq!(e.store().ticker("PF_XBTUSD").unwrap().funding_rate, 0.0002);
    message(
        &mut e,
        s,
        NOW + 1000.0,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":111}"#,
    );
    assert_eq!(e.store().ticker("PF_XBTUSD").unwrap().funding_rate, 0.0);
}

#[test]
fn full_ticker_refreshes_native_statistics_after_rest_seed() {
    let (mut e, s) = public();
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":110,"open":100,"change":9.3,"volume":12,"volumeQuote":1320,"openInterest":7}"#,
    );
    let t = e.store().ticker("PF_XBTUSD").unwrap();
    assert_eq!(
        (t.percentage, t.volume, t.quote_volume, t.open_interest),
        (9.3, 12.0, 1320.0, 7.0)
    );
    message(
        &mut e,
        s,
        NOW + 1000.0,
        r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":120,"open":100,"volume":0,"volumeQuote":0,"openInterest":0}"#,
    );
    let t = e.store().ticker("PF_XBTUSD").unwrap();
    assert_eq!(
        (t.percentage, t.volume, t.quote_volume, t.open_interest),
        (20.0, 0.0, 0.0, 0.0)
    );
}
#[test]
fn capped_candle_page_continues_to_the_newest_bars() {
    let (mut e, _) = public();
    e.handle(
        NOW,
        Event::Command(Command::FetchCandles {
            command: 8,
            symbol: "PF_XBTUSD".into(),
            interval: "1m".into(),
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"{"candles":[{"time":1699999880000,"open":"1","high":"2","low":"0.5","close":"1.5","volume":"3"}],"more_candles":true}"#,
    );
    let next = requests(&mut e).pop().expect("next page");
    assert!(
        next.url.contains("from=1699999940&to=1700000000"),
        "{}",
        next.url
    );
    reply(
        &mut e,
        &next,
        200,
        r#"{"candles":[{"time":1699999940000,"open":"2","high":"3","low":"1","close":"2.5","volume":"4"}],"more_candles":false}"#,
    );
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::CommandDone { command: 8, result: Ok(s) }) if s.contains("1699999880.0") && s.contains("1699999940.0"))));
}
