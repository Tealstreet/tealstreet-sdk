// Payloads follow https://apidocs.lighter.xyz/docs/websocket-reference.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, RequestId, Response, SocketId};
use safe_cex_venue_lighter::Lighter;
use support::Options;
use support::{Timers, advance, drain};
const MARKETS: &str = r#"{"code":200,"order_book_details":[{"market_id":0,"symbol":"ETH","status":"active","market_type":"perp","supported_size_decimals":4,"supported_price_decimals":2,"min_base_amount":"0.001","min_quote_amount":"10","min_initial_margin_fraction":"200"},{"market_id":1,"symbol":"BTC","status":"active","supported_size_decimals":5,"supported_price_decimals":1,"min_initial_margin_fraction":"100"},{"market_id":2048,"symbol":"ETH/USDC","status":"active","market_type":"spot"}]}"#;
fn reply(e: &mut Engine<Lighter>, id: RequestId, body: &str) {
    e.handle(
        1_000.0,
        Event::HttpResponse {
            request: id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    )
}
fn seeded() -> (Engine<Lighter>, SocketId) {
    let (engine, socket, _) = seeded_with_poll();
    (engine, socket)
}
fn seeded_with_poll() -> (Engine<Lighter>, SocketId, safe_cex_engine::TimerId) {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(1_000.0);
    let id = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap();
    reply(&mut e, id, MARKETS);
    let actions: Vec<_> = e.drain_actions().collect();
    let socket = actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let poll = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 3000.0,
            } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(1_000.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    (e, socket, poll)
}
fn send(e: &mut Engine<Lighter>, s: SocketId, t: f64, text: &str) {
    e.handle(t, Event::WsMessage { socket: s, text });
}
fn book(e: &mut Engine<Lighter>) {
    e.handle(
        1_000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH".into(),
        }),
    );
}
#[test]
fn streamed_metadata_filters_spot_and_preserves_precision() {
    let (e, _) = seeded();
    assert_eq!(e.store().markets().len(), 2);
    let m = e.store().market("ETH").unwrap();
    assert_eq!(m.base, "ETH");
    assert_eq!(m.quote, "USDC");
    assert_eq!(m.precision.amount, 0.0001);
    assert_eq!(m.limits.leverage.max, 50.0);
    assert!(e.store().loaded().markets);
}
#[test]
fn keyed_stats_bbo_optional_updates_and_funding_units() {
    let (mut e, s) = seeded();
    send(
        &mut e,
        s,
        2_000.0,
        r#"{"type":"update/market_stats","channel":"market_stats:all","market_stats":{"0":{"market_id":0,"mark_price":"2000","index_price":"2001","current_funding_rate":"0.0012","funding_rate":"0.0008","daily_price_change":40.52,"daily_base_token_volume":10,"daily_quote_token_volume":20000}}}"#,
    );
    let t = e.store().ticker("ETH").unwrap();
    assert_eq!(t.mark, 2000.0);
    assert_eq!(t.funding_rate, 0.0001);
    assert_eq!(t.percentage, 40.52);
    send(
        &mut e,
        s,
        3_000.0,
        r#"{"type":"update/ticker","channel":"ticker:0","ticker":{"s":"ETH","a":{"price":"2002","size":"1"},"b":{"price":"2000","size":"2"}}}"#,
    );
    let t = e.store().ticker("ETH").unwrap();
    assert_eq!((t.bid, t.ask, t.mark), (2000.0, 2002.0, 2000.0));
    send(
        &mut e,
        s,
        4_000.0,
        r#"{"channel":"ticker:0","ticker":{"index_price":"2100"}}"#,
    );
    assert_eq!(e.store().ticker("ETH").unwrap().mark, 2100.0);
}
#[test]
fn book_nonce_chain_ignores_offsets_and_rejects_gap() {
    let (mut e, s) = seeded();
    book(&mut e);
    e.drain_actions().for_each(drop);
    send(
        &mut e,
        s,
        2_000.0,
        r#"{"channel":"order_book:0","type":"subscribed/order_book","offset":500,"order_book":{"nonce":10,"bids":[{"price":"100","size":"1"},{"price":"100","size":"2"}],"asks":[{"price":"102","size":"3"}]}}"#,
    );
    assert_eq!(e.book("ETH").unwrap().bids(), [(100.0, 3.0)]);
    send(
        &mut e,
        s,
        3_000.0,
        r#"{"channel":"order_book:0","type":"update/order_book","offset":9000,"order_book":{"begin_nonce":10,"nonce":20,"bids":[{"price":"100","size":"0"},{"price":"99","size":"5"}]}}"#,
    );
    assert_eq!(e.book("ETH").unwrap().bids(), [(99.0, 5.0)]);
    send(
        &mut e,
        s,
        4_000.0,
        r#"{"channel":"order_book:0","type":"update/order_book","order_book":{"begin_nonce":9,"nonce":21,"bids":[{"price":"98","size":"5"}]}}"#,
    );
    assert!(e.book("ETH").unwrap().is_empty());
    assert_eq!(
        e.drain_actions()
            .filter(|a| matches!(a, Action::WsSend { .. }))
            .count(),
        2
    );
}
#[test]
fn rest_book_seed_cannot_replace_newer_ws_and_depth_is_not_truncated() {
    let (mut e, s) = seeded();
    book(&mut e);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap();
    let bids = (1..=200)
        .map(|n| format!(r#"{{"price":"{n}","size":"1"}}"#))
        .collect::<Vec<_>>()
        .join(",");
    send(
        &mut e,
        s,
        2_000.0,
        &format!(
            r#"{{"channel":"order_book:0","type":"subscribed/order_book","order_book":{{"nonce":10,"bids":[{bids}],"asks":[]}}}}"#
        ),
    );
    reply(
        &mut e,
        request,
        r#"{"code":200,"bids":[{"price":"1","remaining_base_amount":"20"}],"asks":[]}"#,
    );
    assert_eq!(e.book("ETH").unwrap().bids().len(), 200);
    assert_eq!(e.book("ETH").unwrap().bids()[0], (200.0, 1.0));
}
#[test]
fn candle_ms_seconds_ordering_and_invalid_interval() {
    let (mut e, s) = seeded();
    e.handle(
        1_000.0,
        Event::Command(Command::SubscribeCandles {
            symbol: "ETH".into(),
            interval: safe_cex_engine::model::Interval::M1,
        }),
    );
    e.drain_actions().for_each(drop);
    send(
        &mut e,
        s,
        2_000.0,
        r#"{"channel":"candle:0:1m","type":"update/candle","candles":[{"t":1700000040000,"o":10,"h":12,"l":9,"c":11,"v":2}]}"#,
    );
    assert!(e.take_frame(2_000.0).is_some());
    e.handle(
        2_000.0,
        Event::Command(Command::FetchCandles {
            command: 7,
            symbol: "ETH".into(),
            interval: safe_cex_engine::model::Interval::M3,
            limit: None,
            from_ms: None,
            to_ms: None,
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::CommandFailed { command: 7, .. })
    )));
}
#[test]
fn watchdog_reconnects_silent_socket_and_rejects_old_socket() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(1_000.0);
    let id = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap();
    reply(&mut e, id, MARKETS);
    let mut timers = Timers::new();
    let socket = drain(&mut e, &mut timers)
        .iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1_000.0, Event::WsOpened { socket });
    let closed = |actions: &[Action]| {
        actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
    };
    assert!(!closed(&advance(&mut e, &mut timers, 120_999.0)));
    assert!(closed(&advance(&mut e, &mut timers, 121_000.0)));
    send(
        &mut e,
        socket,
        123_000.0,
        r#"{"channel":"ticker:0","ticker":{"mark_price":"999"}}"#,
    );
    assert_eq!(e.store().ticker("ETH").unwrap().mark, 0.0);
}
#[test]
fn metadata_failure_falls_back_but_invalid_success_never_clears() {
    let mut e = support::engine(Options::default());
    e.start(1_000.0);
    let id = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap();
    reply(&mut e, id, r#"{"code":404,"message":"unsupported"}"#);
    assert!(
        safe_cex_engine::test_support::prepared_actions(&mut e, 1000.)
            .into_iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("orderBooks")))
    );
}

#[test]
fn subscription_can_be_cancelled_before_markets_arrive() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(1000.0);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(
        1000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH".into(),
        }),
    );
    e.handle(
        1001.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "ETH".into(),
        }),
    );
    reply(&mut e, request, MARKETS);
    assert!(e.book("ETH").is_none());
}

#[test]
fn rest_book_cannot_establish_websocket_nonce_continuity() {
    let (mut e, s) = seeded();
    book(&mut e);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r.id)
            } else {
                None
            }
        })
        .unwrap();
    reply(
        &mut e,
        request,
        r#"{"code":200,"bids":[{"price":"100","remaining_base_amount":"2"}],"asks":[]}"#,
    );
    assert_eq!(e.book("ETH").unwrap().bids(), [(100.0, 2.0)]);
    send(
        &mut e,
        s,
        2_000.0,
        r#"{"channel":"order_book:0","type":"update/order_book","order_book":{"begin_nonce":10,"nonce":11,"bids":[{"price":"99","size":"1"}],"asks":[]}}"#,
    );
    assert!(e.book("ETH").unwrap().is_empty());
}

#[test]
fn empty_quotes_and_bad_market_rows_do_not_drop_other_market_statistics() {
    let (mut e, s) = seeded();
    send(
        &mut e,
        s,
        2_000.0,
        r#"{"channel":"market_stats:all","type":"subscribed/market_stats","market_stats":{"0":{"market_id":0,"mark_price":"100","best_bid_price":"","best_ask_price":""},"1":{"market_id":1,"mark_price":"200","current_funding_rate":"0.0012"},"bad":{"market_id":999,"mark_price":{}}}}"#,
    );
    assert_eq!(e.store().ticker("ETH").unwrap().mark, 100.0);
    assert_eq!(e.store().ticker("BTC").unwrap().mark, 200.0);
    assert_eq!(e.store().ticker("BTC").unwrap().funding_rate, 0.0001);
}

#[test]
fn primary_and_fallback_catalogues_retry_with_backoff_then_keep_valid_neighbors() {
    let mut e = support::engine(Options::default());
    e.start(0.);
    let mut transport_now = 0.;
    for delay in [1000., 2000., 4000.] {
        for _ in 0..2 {
            let id = safe_cex_engine::test_support::prepared_actions(&mut e, transport_now)
                .into_iter()
                .find_map(|a| match a {
                    Action::Http(r) => Some(r.id),
                    _ => None,
                })
                .unwrap();
            transport_now = 1000.;
            reply(
                &mut e,
                id,
                r#"{"code":500,"message":"temporarily unavailable"}"#,
            );
        }
        let (timer, at) = e
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .unwrap();
        assert!(support::jittered(at - 1000., delay), "{at}");
        transport_now = 1000. + delay;
        e.handle(transport_now, Event::TimerFired { timer });
        assert!(!e.store().loaded().markets);
    }
    let id = safe_cex_engine::test_support::prepared_actions(&mut e, transport_now)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    reply(&mut e, id, &MARKETS.replacen("[", "[{},", 1));
    assert_eq!(e.store().markets().len(), 2);
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message == "Skipped malformed public discovery row")));
}

#[test]
fn disconnect_clears_old_book_and_fences_inflight_rest_seed() {
    let (mut e, s) = seeded();
    book(&mut e);
    let rest = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    send(
        &mut e,
        s,
        2000.0,
        r#"{"channel":"order_book:0","type":"subscribed/order_book","order_book":{"nonce":10,"bids":[{"price":"99","size":"1"}],"asks":[{"price":"101","size":"1"}]}}"#,
    );
    assert_eq!(e.book("ETH").unwrap().best_bid(), Some(99.0));
    e.take_frame(2001.0);
    e.handle(
        3000.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    assert!(e.book("ETH").unwrap().is_empty());
    let frame = e.take_frame(3001.0).unwrap();
    assert!(
        safe_cex_engine::wire::sections(frame).any(|(kind, payload)| {
            kind == safe_cex_engine::wire::section::BOOK && payload[4..12] == [0; 8]
        })
    );
    reply(
        &mut e,
        rest,
        r#"{"bids":[{"price":"77","size":"2"}],"asks":[]}"#,
    );
    assert!(e.book("ETH").unwrap().is_empty());
    send(
        &mut e,
        s,
        3100.0,
        r#"{"channel":"order_book:0","type":"subscribed/order_book","order_book":{"nonce":12,"bids":[{"price":"88","size":"1"}],"asks":[]}}"#,
    );
    assert!(e.book("ETH").unwrap().is_empty());
}

#[test]
fn silent_book_is_cleared_while_other_channels_keep_socket_alive() {
    let (mut e, socket, poll) = seeded_with_poll();
    book(&mut e);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r.id),
            _ => None,
        })
        .unwrap();
    reply(&mut e, request, r#"{"bids":[],"asks":[]}"#);
    send(
        &mut e,
        socket,
        2000.0,
        r#"{"channel":"order_book:0","type":"subscribed/order_book","order_book":{"nonce":10,"bids":[{"price":"99","size":"1"}],"asks":[]}}"#,
    );
    e.take_frame(2001.0);
    send(&mut e, socket, 93000.0, r#"{"type":"pong"}"#);
    e.handle(93001.0, Event::TimerFired { timer: poll });
    assert!(e.book("ETH").unwrap().is_empty());
    let frame = e.take_frame(93001.0).unwrap();
    assert!(
        safe_cex_engine::wire::sections(frame).any(|(kind, payload)| kind
            == safe_cex_engine::wire::section::BOOK
            && payload[4..12] == [0; 8])
    );
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 93001.0);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("orderBookOrders")))
    );
    send(
        &mut e,
        socket,
        94000.0,
        r#"{"channel":"order_book:0","type":"update/order_book","order_book":{"begin_nonce":10,"nonce":11,"bids":[{"price":"88","size":"1"}]}}"#,
    );
    assert!(e.book("ETH").unwrap().is_empty());
    send(
        &mut e,
        socket,
        95000.0,
        r#"{"channel":"order_book:0","type":"subscribed/order_book","order_book":{"nonce":12,"bids":[{"price":"97","size":"1"}],"asks":[]}}"#,
    );
    assert_eq!(e.book("ETH").unwrap().best_bid(), Some(97.0));
}

#[test]
fn base_and_usd_open_interest_can_coexist_in_a_live_stats_row() {
    let (mut e, socket) = seeded();
    send(
        &mut e,
        socket,
        2_000.0,
        r#"{"channel":"market_stats:all","market_stats":{"0":{"symbol":"ETH","market_id":0,"mark_price":"2333.18","index_price":"2333.93","last_trade_price":"2333.21","open_interest":"76059298.189074","open_interest_usd":"152118596.378148"}}}"#,
    );
    let ticker = e.store().ticker("ETH").unwrap();
    assert_eq!(ticker.mark, 2333.18);
    assert_eq!(ticker.index, 2333.93);
    assert_eq!(ticker.last, 2333.21);
    assert_eq!(ticker.open_interest, 76059298.189074);
}

#[test]
fn rest_depth_aggregates_native_lots_without_binary_residue() {
    let (mut e, _) = seeded();
    book(&mut e);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("orderBookOrders") => Some(request),
            _ => None,
        })
        .unwrap();
    reply(
        &mut e,
        request.id,
        r#"{"code":200,"bids":[{"price":"100","remaining_base_amount":"0.1"},{"price":"100","remaining_base_amount":"0.2"}],"asks":[{"price":"101","remaining_base_amount":"0.1"},{"price":"101","remaining_base_amount":"0.2"}]}"#,
    );
    let book = e.book("ETH").unwrap();
    assert_eq!(book.bids(), [(100.0, 0.3)]);
    assert_eq!(book.asks(), [(101.0, 0.3)]);
}

#[test]
fn public_close_primes_the_book_before_the_reconnect_timer() {
    let (mut e, socket) = seeded();
    book(&mut e);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("orderBookOrders") => Some(request),
            _ => None,
        })
        .unwrap();
    reply(
        &mut e,
        request.id,
        r#"{"code":200,"bids":[{"price":"100","remaining_base_amount":"1"}],"asks":[]}"#,
    );
    e.drain_actions().for_each(drop);
    // The previous successful snapshot's two-second pacing interval has elapsed.
    e.handle(4_000.0, Event::WsClosed { socket, code: 1006 });
    let recovery = safe_cex_engine::test_support::prepared_actions(&mut e, 4_000.0)
        .into_iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.url.contains("orderBookOrders") => Some(request),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(recovery.len(), 1, "close must not wait for another poll");
    e.handle(
        4_001.0,
        Event::HttpResponse {
            request: recovery[0].id,
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"code":200,"bids":[{"price":"99","remaining_base_amount":"2"}],"asks":[]}"#,
            },
        },
    );
    assert_eq!(e.book("ETH").unwrap().bids(), [(99.0, 2.0)]);
    for now in [4_002.0, 4_003.0, 4_004.0] {
        e.handle(now, Event::WsClosed { socket, code: 1006 });
    }
    assert!(
        safe_cex_engine::test_support::prepared_actions(&mut e, 4_004.0)
            .into_iter()
            .all(|action| !matches!(action, Action::Http(request) if request.url.contains("orderBookOrders")))
    );
}

#[test]
fn market_snapshot_seeds_last_price_before_socket_stats() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(1_000.0);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("orderBookDetails") => Some(request),
            _ => None,
        })
        .unwrap();
    let markets = MARKETS.replacen(
        "\"market_id\":0,",
        "\"market_id\":0,\"last_trade_price\":\"2333.21\",",
        1,
    );
    reply(&mut e, request.id, &markets);
    assert_eq!(e.store().ticker("ETH").unwrap().last, 2333.21);
    assert_eq!(e.store().ticker("BTC").unwrap().last, 0.0);
}

#[test]
fn reconnect_keeps_one_snapshot_and_fresh_socket_depth_wins() {
    let (mut e, socket) = seeded();
    book(&mut e);
    let initial = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("orderBookOrders") => Some(request),
            _ => None,
        })
        .unwrap();
    reply(&mut e, initial.id, r#"{"bids":[],"asks":[]}"#);
    e.drain_actions().for_each(drop);
    e.handle(4_000.0, Event::WsClosed { socket, code: 1006 });
    let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 4_000.0);
    let recovery: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.url.contains("orderBookOrders") => Some(request.id),
            _ => None,
        })
        .collect();
    assert_eq!(recovery.len(), 1);
    let (retry, retry_at) = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms > 4_000.0 && *at_ms < 10_000.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .unwrap();
    e.handle(retry_at, Event::TimerFired { timer: retry });
    let socket = e
        .drain_actions()
        .find_map(|action| match action {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        })
        .unwrap();
    e.handle(retry_at + 1.0, Event::WsOpened { socket });
    assert!(safe_cex_engine::test_support::prepared_actions(&mut e, retry_at + 1.0)
        .into_iter().all(|action| !matches!(action, Action::Http(request) if request.url.contains("orderBookOrders"))));
    send(
        &mut e,
        socket,
        retry_at + 2.0,
        r#"{"channel":"order_book:0","type":"subscribed/order_book","order_book":{"nonce":50,"bids":[{"price":"102","size":"3"}],"asks":[]}}"#,
    );
    e.handle(
        retry_at + 1_000.0,
        Event::HttpResponse {
            request: recovery[0],
            response: Response {
                status: 200,
                headers: "",
                body: r#"{"bids":[{"price":"99","remaining_base_amount":"1"}],"asks":[]}"#,
            },
        },
    );
    assert_eq!(e.book("ETH").unwrap().bids(), [(102.0, 3.0)]);
    e.handle(retry_at + 1_001.0, Event::WsClosed { socket, code: 1006 });
    assert!(safe_cex_engine::test_support::prepared_actions(&mut e, retry_at + 1_001.0)
        .into_iter().all(|action| !matches!(action, Action::Http(request) if request.url.contains("orderBookOrders"))));
}

#[test]
fn stats_prefer_base_interest_independently_of_json_order_and_keep_usd_fallback() {
    let (mut e, socket) = seeded();
    send(
        &mut e,
        socket,
        2_000.0,
        r#"{"channel":"ticker:0","ticker":{"open_interest_usd":"20","open_interest":"10","last_trade_price":"100"}}"#,
    );
    assert_eq!(e.store().ticker("ETH").unwrap().open_interest, 10.0);
    send(
        &mut e,
        socket,
        2_001.0,
        r#"{"channel":"ticker:0","ticker":{"openInterestUsd":"30"}}"#,
    );
    assert_eq!(e.store().ticker("ETH").unwrap().open_interest, 30.0);
    assert_eq!(e.store().ticker("ETH").unwrap().last, 100.0);
}

#[test]
fn catalogue_starts_ticker_hydration_before_the_socket_opens() {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(1_000.0);
    let catalogue = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("orderBookDetails") => Some(request),
            _ => None,
        })
        .unwrap();
    reply(&mut e, catalogue.id, MARKETS);
    let stats = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|action| match action {
            Action::Http(request) if request.url.contains("exchangeStats") => Some(request),
            _ => None,
        })
        .expect("ticker hydration must not depend on a socket Open");
    reply(
        &mut e,
        stats.id,
        r#"{"code":200,"order_book_stats":[{"symbol":"ETH","last_trade_price":"2333.21"}]}"#,
    );
    assert_eq!(e.store().ticker("ETH").unwrap().last, 2333.21);
}
