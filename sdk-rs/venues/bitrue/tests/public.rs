use safe_cex_engine::{
    Action, Command, Engine, Event, Notice, Response, SocketChannel, SocketId, Venue,
};
use safe_cex_venue_bitrue::{Bitrue, Options};
fn contracts() -> &'static str {
    r#"[{"symbol":"E-BTC-USDT","pricePrecision":2,"multiplier":0.001,"minOrderVolume":1,"maxMarketVolume":1000,"maxLimitVolume":2000,"minOrderMoney":1,"status":1},{"symbol":"E-ETH-USDT","pricePrecision":2,"multiplier":0.01,"minOrderVolume":1,"maxMarketVolume":1000,"status":1}]"#
}
fn boot() -> (Engine<Bitrue>, SocketId) {
    let mut e = Engine::new(Bitrue::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(0.0);
    let request = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(
        1.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: contracts(),
            },
        },
    );
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                url,
                channel,
                ..
            } = a
            {
                assert_eq!(url, "wss://fmarket-ws.bitrue.com/kline-api/ws");
                assert_eq!(channel, Some(SocketChannel::Public));
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2.0, Event::WsOpened { socket });
    e.drain_actions();
    (e, socket)
}
fn message(e: &mut Engine<Bitrue>, socket: SocketId, text: &str) {
    e.handle(3.0, Event::WsMessage { socket, text });
}
#[test]
fn markets_have_native_multiplier_and_conservative_leverage() {
    let (e, _) = boot();
    let m = e.store().market("E-BTC-USDT").unwrap();
    assert_eq!(
        (m.precision.amount, m.limits.amount.min, m.limits.amount.max),
        (0.001, 0.001, 1.0)
    );
    assert_eq!(m.contract_size, Some(0.001));
    assert_eq!(m.limits.leverage.max, 100.0);
}

#[test]
fn public_reconnect_preserves_the_declared_role_and_upstream_url() {
    let (mut e, socket) = boot();
    e.handle(4.0, Event::WsClosed { socket, code: 1006 });
    let (timer, at) = e
        .drain_actions()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .expect("public reconnect timer");
    e.handle(at, Event::TimerFired { timer });
    let (replacement, url, channel) = e
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                url,
                channel,
                ..
            } => Some((socket, url, channel)),
            _ => None,
        })
        .expect("replacement public socket");
    assert_ne!(replacement, socket);
    assert_eq!(url, "wss://fmarket-ws.bitrue.com/kline-api/ws");
    assert_eq!(channel, Some(SocketChannel::Public));
}
#[test]
fn ticker_contract_volume_is_base_units() {
    let (mut e, socket) = boot();
    message(
        &mut e,
        socket,
        r#"{"channel":"market_e_btcusdt_ticker","tick":{"close":"60000","rose":"0.02","vol":"1000","amount":"60000000"}}"#,
    );
    let t = e.store().ticker("E-BTC-USDT").unwrap();
    assert_eq!(
        (t.last, t.mark, t.percentage, t.volume, t.quote_volume),
        (60000.0, 60000.0, 2.0, 1.0, 60000.0)
    );
}
#[test]
fn snapshots_replace_and_clear_old_levels_without_truncation() {
    let (mut e, socket) = boot();
    e.handle(
        2.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"market_e_btcusdt_simple_depth_step0","tick":{"buys":[[60000,1000],[59999,2000]],"asks":[[60001,3000]]}}"#,
    );
    let b = e.venue().book("E-BTC-USDT").unwrap();
    assert_eq!(b.bids(), &[(60000.0, 1.0), (59999.0, 2.0)]);
    message(
        &mut e,
        socket,
        r#"{"channel":"market_e_btcusdt_simple_depth_step0","tick":{"buys":[[59998,500]],"asks":[]}}"#,
    );
    let b = e.venue().book("E-BTC-USDT").unwrap();
    assert_eq!(b.bids(), &[(59998.0, 0.5)]);
    assert!(b.asks().is_empty());
    assert_eq!(e.store().ticker("E-BTC-USDT").unwrap().ask, 0.0);
}
#[test]
fn invalid_snapshot_does_not_clear_last_good_book() {
    let (mut e, socket) = boot();
    e.handle(
        2.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "E-BTC-USDT".into(),
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"market_e_btcusdt_simple_depth_step0","tick":{"buys":[[60000,1000]],"asks":[]}}"#,
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"market_e_btcusdt_simple_depth_step0","tick":{"buys":[[60000,-1]],"asks":[]}}"#,
    );
    assert_eq!(
        e.venue().book("E-BTC-USDT").unwrap().bids(),
        &[(60000.0, 1.0)]
    );
}
#[test]
fn fallback_only_after_no_primary_and_full_unsubscribe() {
    let (mut e, socket) = boot();
    e.handle(
        2.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "E-BTC-USDT".into(),
        }),
    );
    let timer = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (at_ms == 3002.0).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(3002.0, Event::TimerFired { timer });
    assert!(e.drain_actions().any(
        |a| matches!(a,Action::WsSend{text,..}if text.contains("market_e_btcusdt_depth_step0"))
    ));
    e.handle(
        3003.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "E-BTC-USDT".into(),
        }),
    );
    let sends = e
        .drain_actions()
        .filter(|a| matches!(a,Action::WsSend{text,..}if text.contains("unsub")))
        .count();
    assert_eq!(sends, 2);
    message(
        &mut e,
        socket,
        r#"{"channel":"market_e_btcusdt_depth_step0","tick":{"buys":[[1,1]],"asks":[]}}"#,
    );
    assert!(e.venue().book("E-BTC-USDT").is_none());
}
#[test]
fn public_ping_echoes_numeric_token() {
    let (mut e, socket) = boot();
    message(&mut e, socket, r#"{"ping":1721131206}"#);
    let sends: Vec<_> = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(sends, ["pong", r#"{"pong":1721131206}"#]);
}
#[test]
fn candle_history_uses_singapore_time_and_filters_requested_window() {
    let (mut e, socket) = boot();
    e.handle(
        2.0,
        Event::Command(Command::FetchCandles {
            command: 1,
            symbol: "E-BTC-USDT".into(),
            interval: "1m".into(),
            limit: Some(2),
            from_ms: Some(1704067200000.0),
            to_ms: Some(1704067260000.0),
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"event_rep":"rep","channel":"market_e_btcusdt_kline_1min","data":[{"ds":"2024-01-01 08:00:00","open":1,"high":3,"low":1,"close":2,"vol":1000},{"ds":"2024-01-01 07:59:00","open":1,"high":3,"low":1,"close":2,"vol":1000}]}"#,
    );
    let result = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { result, .. }) = a {
                Some(result.unwrap())
            } else {
                None
            }
        })
        .unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&result).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        (
            rows[0]["timestamp"].as_f64().unwrap(),
            rows[0]["volume"].as_f64().unwrap()
        ),
        (1704067200.0, 1.0)
    );
}
#[test]
fn subscriptions_before_markets_are_retained() {
    let mut e = Engine::new(Bitrue::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    e.start(0.0);
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }),
    );
    let request = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(
        1.0,
        Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body: contracts(),
            },
        },
    );
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Public),
                ..
            } = a
            {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(2.0, Event::WsOpened { socket });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsSend{text,..}if text.contains("simple_depth_step0")))
    );
}
