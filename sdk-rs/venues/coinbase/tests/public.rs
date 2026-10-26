mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Venue};
use safe_cex_venue_coinbase::{Coinbase, Options};
use support::*;
const SYMBOL: &str = "BTC-PERP-INTX";

fn candle_rows(e: &mut Driver, now: f64) -> Vec<[f64; 6]> {
    let Some(frame) = e.take_frame(now) else {
        return vec![];
    };
    let mut rows = vec![];
    for (kind, payload) in safe_cex_engine::wire::sections(frame) {
        if kind != 6 {
            continue;
        }
        let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
        let start = (4 + count * 8 + 7) & !7;
        for row in payload[start..start + count * 48].as_chunks::<48>().0 {
            rows.push(std::array::from_fn(|i| {
                f64::from_le_bytes(row[i * 8..i * 8 + 8].try_into().unwrap())
            }));
        }
    }
    rows
}

#[test]
fn seconds_and_three_minute_streams_use_trades_and_roll_over() {
    for (interval, timestamp, expected) in [
        ("1s", 182., [110., 110., 110., 110., 3.]),
        ("3m", 180., [100., 110., 100., 110., 5.]),
    ] {
        let (mut e, socket) = ready();
        e.take_frame(1000.);
        e.handle(
            1000.,
            Event::Command(Command::SubscribeCandles {
                symbol: SYMBOL.into(),
                interval: safe_cex_engine::model::Interval::parse(interval).unwrap(),
            }),
        );
        let subscriptions: Vec<_> = e
            .drain_actions()
            .filter_map(|action| match action {
                Action::WsSend { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert!(
            subscriptions
                .iter()
                .any(|text| text.contains("market_trades"))
        );
        message(
            &mut e,
            socket,
            r#"{"channel":"candles","sequence_num":0,"events":[{"type":"update","candles":[{"product_id":"BTC-PERP-INTX","start":"0","open":"1","high":"2","low":"1","close":"2","volume":"99"}]}]}"#,
        );
        assert!(candle_rows(&mut e, 2000.).is_empty());
        message(
            &mut e,
            socket,
            r#"{"channel":"market_trades","sequence_num":1,"events":[{"type":"update","trades":[{"product_id":"BTC-PERP-INTX","time":"1970-01-01T00:03:01Z","price":"100","size":"2"},{"product_id":"BTC-PERP-INTX","time":"1970-01-01T00:03:02Z","price":"110","size":"3"}]}]}"#,
        );
        let rows = candle_rows(&mut e, 2000.);
        assert_eq!(rows.len(), if interval == "1s" { 2 } else { 1 });
        assert_eq!(
            rows.last(),
            Some(&[
                timestamp,
                expected[0],
                expected[1],
                expected[2],
                expected[3],
                expected[4]
            ])
        );
        message(
            &mut e,
            socket,
            r#"{"channel":"market_trades","sequence_num":2,"events":[{"type":"update","trades":[{"product_id":"BTC-PERP-INTX","time":"1970-01-01T00:06:00Z","price":"90","size":"4"}]}]}"#,
        );
        assert_eq!(
            candle_rows(&mut e, 2000.),
            vec![[360., 90., 90., 90., 90., 4.]]
        );
    }
}

#[test]
fn four_hour_stream_forwards_five_minute_candles_and_trade_prices() {
    let (mut e, socket) = ready();
    e.take_frame(1000.);
    e.handle(
        1000.,
        Event::Command(Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::H4,
        }),
    );
    assert!(e.drain_actions().any(|action| matches!(action,
        Action::WsSend { text, .. } if text.contains("candles"))));
    message(
        &mut e,
        socket,
        r#"{"channel":"candles","sequence_num":0,"events":[{"type":"update","candles":[{"product_id":"BTC-PERP-INTX","start":"300","open":"2","high":"4","low":"1","close":"3","volume":"5"}]}]}"#,
    );
    assert_eq!(candle_rows(&mut e, 2000.), vec![[300., 2., 4., 1., 3., 5.]]);
    message(
        &mut e,
        socket,
        r#"{"channel":"market_trades","sequence_num":1,"events":[{"type":"update","trades":[{"product_id":"BTC-PERP-INTX","time":"1970-01-01T00:05:01Z","price":"3.5","size":"2"}]}]}"#,
    );
    assert_eq!(
        candle_rows(&mut e, 2000.),
        vec![[300., 2., 4., 1., 3.5, 5.]]
    );
}

#[test]
fn streaming_intervals_do_not_expand_the_rest_granularity_whitelist() {
    let (mut e, _) = ready();
    for interval in ["1s", "3m", "4h"] {
        e.handle(
            2000.,
            Event::Command(Command::FetchCandles {
                command: 42,
                symbol: SYMBOL.into(),
                interval: safe_cex_engine::model::Interval::parse(interval).unwrap(),
                limit: Some(2),
                from_ms: None,
                to_ms: None,
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        assert!(actions.iter().any(|action| matches!(action,
            Action::Notice(Notice::CommandFailed { command: 42, error })
                if error.legacy_text().starts_with("Coinbase does not support"))));
    }
}
#[test]
fn discovers_instruments_without_credentials_and_keeps_full_depth() {
    let (mut e, socket) = ready();
    assert_eq!(e.store().markets()[0].symbol, SYMBOL);
    assert_eq!(e.store().tickers()[0].last, 60000.);
    e.handle(
        1000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":0,"events":[{"type":"snapshot","product_id":"BTC-PERP-INTX","updates":[{"side":"bid","price_level":"60000","new_quantity":"2"},{"side":"bid","price_level":"59999","new_quantity":"3"},{"side":"offer","price_level":"60001","new_quantity":"4"}]}]}"#,
    );
    assert_eq!(
        e.venue().book(SYMBOL).unwrap().bids(),
        [(60000., 2.), (59999., 3.)]
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":1,"events":[{"type":"update","product_id":"BTC-PERP-INTX","updates":[{"side":"bid","price_level":"60000","new_quantity":"0"},{"side":"bid","price_level":"59998","new_quantity":"8"}]}]}"#,
    );
    assert_eq!(
        e.venue().book(SYMBOL).unwrap().bids(),
        [(59999., 3.), (59998., 8.)]
    );
}
#[test]
fn sequence_gap_invalidates_all_books_and_reconnects() {
    let (mut e, socket) = ready();
    e.handle(
        1000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":0,"events":[{"type":"snapshot","product_id":"BTC-PERP-INTX","updates":[{"side":"bid","price_level":"60000","new_quantity":"2"}]}]}"#,
    );
    assert!(e.venue().book(SYMBOL).is_some());
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":2,"events":[{"type":"update","product_id":"BTC-PERP-INTX","updates":[]}]}"#,
    );
    assert!(e.venue().book(SYMBOL).is_none());
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose {socket:s} if s==socket))
    );
}
#[test]
fn malformed_snapshot_cannot_partially_replace_book() {
    let (mut e, socket) = ready();
    e.handle(
        1000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":0,"events":[{"type":"snapshot","product_id":"BTC-PERP-INTX","updates":[{"side":"bid","price_level":"60000","new_quantity":"2"},{"side":"offer","price_level":"60001","new_quantity":"-1"}]}]}"#,
    );
    assert!(e.venue().book(SYMBOL).is_none());
}
#[test]
fn failed_poll_preserves_last_known_good_tickers() {
    let (mut e, _) = ready();
    // Poll timer was already allocated before support drained its action.
    for n in 1..8 {
        e.handle(
            3000.,
            Event::TimerFired {
                timer: safe_cex_engine::TimerId(n),
            },
        );
    }
    for r in requests(&mut e) {
        reply(&mut e, &r, 503, "temporarily unavailable");
    }
    assert_eq!(e.store().tickers()[0].last, 60000.);
}
#[test]
fn intx_error_status_with_valid_instruments_is_usable() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 500, INSTRUMENTS);
    assert_eq!(e.store().markets().len(), 1);
}
#[test]
fn candle_fetch_uses_public_route_and_orders_chronologically() {
    let (mut e, _) = ready();
    e.handle(
        200000000.,
        Event::Command(Command::FetchCandles {
            command: 31,
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::M1,
            limit: Some(2),
            from_ms: None,
            to_ms: None,
        }),
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("/market/products/BTC-PERP-INTX/candles"));
    assert!(r.headers.is_empty());
    reply(
        &mut e,
        &r,
        200,
        r#"{"candles":[{"start":"180","open":"2","high":"4","low":"1","close":"3","volume":"5"},{"start":"120","open":"1","high":"2","low":"1","close":"2","volume":"3"}]}"#,
    );
    let result = e
        .drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone {
                command: 31,
                result,
            }) = a
            {
                Some(result)
            } else {
                None
            }
        })
        .unwrap();
    assert!(result.find("120").unwrap() < result.find("180").unwrap());
}
#[test]
fn duplicate_depth_message_is_ignored() {
    let (mut e, socket) = ready();
    e.handle(
        1000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    let snapshot = r#"{"channel":"l2_data","sequence_num":0,"events":[{"type":"snapshot","product_id":"BTC-PERP-INTX","updates":[{"side":"bid","price_level":"60000","new_quantity":"2"}]}]}"#;
    message(&mut e, socket, snapshot);
    message(&mut e, socket, snapshot);
    assert_eq!(e.venue().book(SYMBOL).unwrap().bids(), [(60000., 2.)]);
}
#[test]
fn socket_sequence_advances_across_every_channel_and_product() {
    let (mut e, socket) = ready();
    e.handle(
        1000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"subscriptions","sequence_num":0,"events":[]}"#,
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"heartbeats","sequence_num":1,"events":[{"heartbeat_counter":"1"}]}"#,
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":2,"events":[{"type":"snapshot","product_id":"ETH-PERP-INTX","updates":[]}]}"#,
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":3,"events":[{"type":"snapshot","product_id":"BTC-PERP-INTX","updates":[{"side":"bid","price_level":"60000","new_quantity":"2"}]}]}"#,
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"candles","sequence_num":4,"events":[]}"#,
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"market_trades","sequence_num":5,"events":[]}"#,
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":6,"events":[{"type":"update","product_id":"BTC-PERP-INTX","updates":[{"side":"bid","price_level":"60000","new_quantity":"3"}]}]}"#,
    );
    assert_eq!(e.venue().book(SYMBOL).unwrap().bids(), [(60000., 3.)]);
}
#[test]
fn watchdog_covers_socket_that_never_opens() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, INSTRUMENTS);
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
            if let Action::SetTimer { timer, at_ms } = a {
                (*at_ms == 6000.).then_some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(21000., Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose {socket:s} if s==socket))
    );
}
#[test]
fn daily_candle_seed_retains_earlier_day_and_counts_bucket_volume_once() {
    let (mut e, socket) = ready();
    e.handle(
        90500000.,
        Event::Command(Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::D1,
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"candles","events":[{"type":"update","candles":[{"product_id":"BTC-PERP-INTX","start":"90000","open":"2","high":"4","low":"1","close":"3","volume":"5"},{"product_id":"BTC-PERP-INTX","start":"90000","open":"2","high":"4","low":"1","close":"3","volume":"7"},{"product_id":"BTC-PERP-INTX","start":"90300","open":"3","high":"5","low":"2","close":"4","volume":"2"}]}]}"#,
    );
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("granularity=FIVE_MINUTE"));
    assert!(r.url.contains("start=86400"));
    reply(
        &mut e,
        &r,
        200,
        r#"{"candles":[{"start":"86400","open":"1","high":"3","low":"0.5","close":"2","volume":"100"},{"start":"90000","open":"2","high":"4","low":"1","close":"3","volume":"6"}]}"#,
    );
    let frame = e.take_frame(2000.).unwrap();
    let mut at = 16;
    let mut volume = None;
    while at < frame.len() {
        let kind = u16::from_le_bytes(frame[at..at + 2].try_into().unwrap());
        let len = u32::from_le_bytes(frame[at + 4..at + 8].try_into().unwrap()) as usize;
        if kind == 6 {
            let payload = &frame[at + 8..at + 8 + len];
            let count = u32::from_le_bytes(payload[..4].try_into().unwrap()) as usize;
            assert_eq!(count, 1);
            let start = 8 + 8 * count;
            volume = Some(f64::from_le_bytes(
                payload[start + 40..start + 48].try_into().unwrap(),
            ));
        }
        at += 8 + len;
    }
    assert_eq!(volume, Some(109.));
}

#[test]
fn daily_candles_backfill_reconnect_gaps_and_reject_old_seed_generations() {
    fn reopen(
        e: &mut Driver,
        socket: safe_cex_engine::SocketId,
        now: f64,
    ) -> (safe_cex_engine::SocketId, safe_cex_engine::HttpRequest) {
        e.drain_actions().for_each(drop);
        e.handle(now, Event::WsClosed { socket, code: 1006 });
        let timer = e
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, .. } => Some(timer),
                _ => None,
            })
            .unwrap();
        e.handle(now + 1000., Event::TimerFired { timer });
        let socket = e
            .drain_actions()
            .find_map(|action| match action {
                Action::WsConnect { socket, .. } => Some(socket),
                _ => None,
            })
            .unwrap();
        e.handle(now + 1000., Event::WsOpened { socket });
        let request = requests(e).pop().unwrap();
        assert!(request.url.contains("granularity=FIVE_MINUTE"));
        (socket, request)
    }

    let (mut e, socket) = ready();
    e.take_frame(1000.);
    e.handle(
        86500000.,
        Event::Command(Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::D1,
        }),
    );
    let initial = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &initial,
        200,
        r#"{"candles":[{"start":"86400","open":"100","high":"110","low":"90","close":"105","volume":"5"}]}"#,
    );
    assert_eq!(
        candle_rows(&mut e, 86500000.),
        vec![[86400., 100., 110., 90., 105., 5.]]
    );

    let (socket, stale) = reopen(&mut e, socket, 87399000.);
    assert!(stale.url.contains("start=86400"));
    message(
        &mut e,
        socket,
        r#"{"channel":"candles","sequence_num":0,"events":[{"type":"update","candles":[{"product_id":"BTC-PERP-INTX","start":"87300","open":"120","high":"125","low":"115","close":"124","volume":"2"}]}]}"#,
    );
    assert!(candle_rows(&mut e, 87400000.).is_empty());
    let (_, fresh) = reopen(&mut e, socket, 87401000.);
    reply(
        &mut e,
        &stale,
        200,
        r#"{"candles":[{"start":"86400","open":"1","high":"999","low":"1","close":"1","volume":"999"}]}"#,
    );
    assert!(candle_rows(&mut e, 87402000.).is_empty());
    reply(
        &mut e,
        &fresh,
        200,
        r#"{"candles":[{"start":"86400","open":"100","high":"115","low":"80","close":"106","volume":"7"},{"start":"86700","open":"106","high":"120","low":"100","close":"110","volume":"3"},{"start":"87000","open":"110","high":"130","low":"105","close":"120","volume":"4"},{"start":"87300","open":"120","high":"125","low":"115","close":"123","volume":"1"}]}"#,
    );
    assert_eq!(
        candle_rows(&mut e, 87402000.),
        vec![[86400., 100., 130., 80., 124., 16.]]
    );
}

#[test]
fn daily_subscriptions_before_first_open_or_during_reconnect_seed_at_open() {
    for reconnect in [false, true] {
        let mut e = safe_cex_engine::test_support::seeded_engine(
            Coinbase::new(Options {
                public_data_only: true,
                ..Options::default()
            })
            .unwrap(),
        );
        e.start(86696000.);
        let request = requests(&mut e).pop().unwrap();
        e.handle(
            86696000.,
            Event::HttpResponse {
                request: request.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: INSTRUMENTS,
                },
            },
        );
        let mut socket = e
            .drain_actions()
            .find_map(|action| match action {
                Action::WsConnect { socket, .. } => Some(socket),
                _ => None,
            })
            .unwrap();
        if reconnect {
            e.handle(86696000., Event::WsOpened { socket });
            e.drain_actions().for_each(drop);
            e.handle(86697000., Event::WsClosed { socket, code: 1006 });
            let timer = e
                .drain_actions()
                .find_map(|action| match action {
                    Action::SetTimer { timer, .. } => Some(timer),
                    _ => None,
                })
                .unwrap();
            e.handle(86698000., Event::TimerFired { timer });
            socket = e
                .drain_actions()
                .find_map(|action| match action {
                    Action::WsConnect { socket, .. } => Some(socket),
                    _ => None,
                })
                .unwrap();
        }
        e.take_frame(86698000.);
        e.handle(
            86699000.,
            Event::Command(Command::SubscribeCandles {
                symbol: SYMBOL.into(),
                interval: safe_cex_engine::model::Interval::D1,
            }),
        );
        assert!(requests(&mut e).is_empty());
        assert!(candle_rows(&mut e, 86699000.).is_empty());
        e.handle(86701000., Event::WsOpened { socket });
        let seed = requests(&mut e).pop().unwrap();
        assert!(seed.url.contains("start=86400&end=86701"));
        reply(
            &mut e,
            &seed,
            200,
            r#"{"candles":[{"start":"86400","open":"100","high":"110","low":"90","close":"105","volume":"7"},{"start":"86700","open":"105","high":"115","low":"100","close":"110","volume":"2"}]}"#,
        );
        assert_eq!(
            candle_rows(&mut e, 86701000.),
            vec![[86400., 100., 115., 90., 110., 9.]]
        );
    }
}

#[test]
fn delayed_websocket_bucket_cannot_regress_a_newer_rest_daily_seed() {
    let (mut e, socket) = ready();
    e.take_frame(1000.);
    e.handle(
        86500000.,
        Event::Command(Command::SubscribeCandles {
            symbol: SYMBOL.into(),
            interval: safe_cex_engine::model::Interval::D1,
        }),
    );
    let seed = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &seed,
        200,
        r#"{"candles":[{"start":"86400","open":"100","high":"150","low":"90","close":"130","volume":"10"}]}"#,
    );
    candle_rows(&mut e, 86500000.);
    message(
        &mut e,
        socket,
        r#"{"channel":"candles","sequence_num":0,"events":[{"type":"update","candles":[{"product_id":"BTC-PERP-INTX","start":"86400","open":"100","high":"140","low":"90","close":"120","volume":"8"}]}]}"#,
    );
    assert_eq!(
        candle_rows(&mut e, 86500000.),
        vec![[86400., 100., 150., 90., 130., 10.]]
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"candles","sequence_num":1,"events":[{"type":"update","candles":[{"product_id":"BTC-PERP-INTX","start":"86400","open":"100","high":"160","low":"80","close":"135","volume":"11"}]}]}"#,
    );
    assert_eq!(
        candle_rows(&mut e, 86500000.),
        vec![[86400., 100., 160., 80., 135., 11.]]
    );
}

#[test]
fn late_usdc_alias_inherits_live_usd_book_and_refcount() {
    let (mut e, socket) = ready();
    let mut usd = e.store().markets()[0].clone();
    usd.id = "BTC-USD".into();
    usd.symbol = usd.id.clone();
    let mut usdc = usd.clone();
    usdc.id = "BTC-USDC".into();
    usdc.symbol = usdc.id.clone();
    e.store_mut().set_markets(vec![usd, usdc]);
    e.handle(
        1000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-USD".into(),
        }),
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":0,"events":[{"type":"snapshot","product_id":"BTC-USD","updates":[{"side":"bid","price_level":"60000","new_quantity":"2"}]}]}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        2000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "BTC-USDC".into(),
        }),
    );
    assert_eq!(
        e.venue().book("BTC-USD").unwrap().bids(),
        e.venue().book("BTC-USDC").unwrap().bids()
    );
    e.handle(
        2000.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "BTC-USD".into(),
        }),
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a,Action::WsSend {text,..} if text.contains("unsubscribe")))
    );
    message(
        &mut e,
        socket,
        r#"{"channel":"l2_data","sequence_num":1,"events":[{"type":"update","product_id":"BTC-USD","updates":[{"side":"bid","price_level":"60000","new_quantity":"3"}]}]}"#,
    );
    assert_eq!(e.venue().book("BTC-USDC").unwrap().bids(), [(60000., 3.)]);
}
#[test]
fn initial_socket_uses_primary_route_and_missing_snapshot_recovers() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, INSTRUMENTS);
    let actions: Vec<_> = e.drain_actions().collect();
    let (socket, context) = actions
        .iter()
        .find_map(|a| {
            if let Action::WsConnect {
                socket, context, ..
            } = a
            {
                Some((*socket, context.as_ref().unwrap()))
            } else {
                None
            }
        })
        .unwrap();
    assert!(!context.recover);
    e.handle(1000., Event::WsOpened { socket });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 6000.,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(
        1000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    e.handle(11000.,Event::WsMessage {socket,text:r#"{"channel":"heartbeats","sequence_num":0,"events":[{"heartbeat_counter":"10"}]}"#});
    e.handle(11000., Event::TimerFired { timer });
    let reconnect = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                ((11750. ..=12000.).contains(&at_ms)).then_some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(12000., Event::TimerFired { timer: reconnect });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsConnect {context:Some(c),..} if c.recover))
    );
}

#[test]
fn spot_only_public_profile_has_keyless_real_spot_products() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    assert!(r.url.contains("/market/products?limit=1000"));
    assert!(r.headers.is_empty());
    reply(
        &mut e,
        &r,
        200,
        r#"{"products":[{"product_type":"SPOT","product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.00000001","price_increment":"0.01","base_min_size":"0.00000001","base_max_size":"10000","price":"60000","volume_24h":"100","price_percentage_change_24h":"9.43%"},{"product_type":"SPOT","product_id":"ETH-USD","base_currency_id":"ETH","quote_currency_id":"USD","status":"TRADING","base_increment":"0.00000001","price_increment":"0.01","base_min_size":"0.00000001","base_max_size":"100000","price":"3000","volume_24h":"200","price_percentage_change_24h":"3"}],"pagination":{"has_next":false}}"#,
    );
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers()[0].last, 60000.);
    assert_eq!(e.store().tickers()[0].percentage, 9.43);
    assert!(e.store().markets().iter().all(|m| m.is_spot == Some(true)));
}
#[test]
fn spot_product_pages_commit_only_after_the_last_page() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let first = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &first,
        200,
        r#"{"products":[{"product_type":"SPOT","product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.00000001","price_increment":"0.01","base_min_size":"0.00000001","base_max_size":"10000","price":"60000","volume_24h":"100","price_percentage_change_24h":"2"}],"pagination":{"has_next":true,"next_cursor":"next/a+ b"}}"#,
    );
    assert!(e.store().markets().is_empty());
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
    e.handle(1334., Event::TimerFired { timer });
    let next = requests(&mut e).pop().unwrap();
    assert!(next.url.ends_with("cursor=next%2Fa%2B%20b"));
    reply(
        &mut e,
        &next,
        200,
        r#"{"products":[{"product_type":"SPOT","product_id":"ETH-USD","base_currency_id":"ETH","quote_currency_id":"USD","status":"TRADING","base_increment":"0.00000001","price_increment":"0.01","base_min_size":"0.00000001","base_max_size":"10000","price":"3000","volume_24h":"100","price_percentage_change_24h":"3"}],"pagination":{"has_next":false}}"#,
    );
    assert_eq!(e.store().markets().len(), 2);
    assert_eq!(e.store().tickers().len(), 2);
}
#[test]
fn repeated_product_cursor_cannot_commit_a_partial_market_list() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let first = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &first,
        200,
        r#"{"products":[],"pagination":{"has_next":true,"next_cursor":"same"}}"#,
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
    e.handle(1334., Event::TimerFired { timer });
    let next = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &next,
        200,
        r#"{"products":[],"pagination":{"has_next":true,"next_cursor":"same"}}"#,
    );
    assert!(e.store().markets().is_empty());
    assert!(e.drain_actions().any(
        |a| matches!(a,Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("cursor"))
    ));
}
#[test]
fn unknown_envelopes_cannot_keep_stale_socket_alive() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, INSTRUMENTS);
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
    e.handle(1000., Event::WsOpened { socket });
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 6000.,
            } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(20000., Event::WsMessage { socket, text: "{}" });
    e.handle(21000., Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose {socket:s} if s==socket))
    );
}

#[test]
fn future_product_without_native_details_cannot_become_spot() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"{"products":[{"product_type":"FUTURE","product_id":"BTC-CDE","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"1","price_increment":"0.01","base_min_size":"1","base_max_size":"10000","price":"60000","volume_24h":"100","price_percentage_change_24h":"2"}]}"#,
    );
    assert!(e.store().markets().is_empty());
    assert!(!e.store().loaded().markets);
}

#[test]
fn book_subscriptions_before_discovery_are_replayed_for_both_products() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let request = requests(&mut e).pop().unwrap();
    for symbol in ["BTC-USD", "ETH-USD"] {
        e.handle(
            1000.,
            Event::Command(Command::SubscribeOrderBook {
                symbol: symbol.into(),
            }),
        );
    }
    let btc = r#"{"product_type":"SPOT","product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.001","price_increment":"0.01","base_min_size":"0.001","base_max_size":"100","price":"100","volume_24h":"1","price_percentage_change_24h":"0"}"#;
    let eth = btc
        .replace("BTC-USD", "ETH-USD")
        .replace("\"BTC\"", "\"ETH\"");
    reply(
        &mut e,
        &request,
        200,
        &format!(r#"{{"products":[{btc},{eth}]}}"#),
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
    e.handle(1000., Event::WsOpened { socket });
    let subscriptions = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    let depth = subscriptions.iter().find(|s| s.contains("level2")).unwrap();
    assert!(depth.contains("BTC-USD") && depth.contains("ETH-USD"));
    for (sequence, symbol) in ["BTC-USD", "ETH-USD"].iter().enumerate() {
        message(
            &mut e,
            socket,
            &format!(
                r#"{{"channel":"l2_data","sequence_num":{sequence},"events":[{{"type":"snapshot","product_id":"{symbol}","updates":[{{"side":"bid","price_level":"100","new_quantity":"2"}}]}}]}}"#
            ),
        );
        assert_eq!(e.venue().book(symbol).unwrap().bids(), [(100., 2.)]);
    }
}
#[test]
fn unsubscribe_before_discovery_does_not_leave_a_pending_book() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let request = requests(&mut e).pop().unwrap();
    e.handle(
        1000.,
        Event::Command(Command::SubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    e.handle(
        1000.,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: SYMBOL.into(),
        }),
    );
    reply(&mut e, &request, 200, INSTRUMENTS);
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
    e.handle(1000., Event::WsOpened { socket });
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a,Action::WsSend{text,..} if text.contains("level2")))
    );
}

#[test]
fn product_online_status_respects_disabled_flags_and_unknown_statuses() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    let rows = [
        ("BTC", "online", false, false, true),
        ("ETH", "TRADING", false, false, true),
        ("SOL", "online", true, false, false),
        ("LTC", "TRADING", false, true, false),
        ("ADA", "delisted", false, false, false),
        ("DOGE", "unknown", false, false, false),
    ];
    let body = format!("{{\"products\":[{}],\"pagination\":{{\"has_next\":false}}}}", rows.iter().map(|(base,status,disabled,trading,_)| format!(r#"{{"product_type":"SPOT","product_id":"{base}-USD","base_currency_id":"{base}","quote_currency_id":"USD","status":"{status}","is_disabled":{disabled},"trading_disabled":{trading},"base_increment":"0.001","price_increment":"0.01","base_min_size":"0.001","base_max_size":"10000"}}"#)).collect::<Vec<_>>().join(","));
    reply(&mut e, &r, 200, &body);
    assert_eq!(e.store().markets().len(), rows.len());
    for (base, _, _, _, active) in rows {
        assert_eq!(
            e.store().market(&format!("{base}-USD")).unwrap().active,
            active
        );
        assert_eq!(
            e.store()
                .ticker(&format!("{base}-USD"))
                .unwrap()
                .funding_time,
            0.
        );
    }
}

#[test]
fn malformed_future_product_keeps_valid_spot_peer_and_warns() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    // Skipped discovery rows log at Debug.
    e.set_log_level(safe_cex_engine::Level::Debug);
    let r = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &r,
        200,
        r#"{"products":[
        {"product_type":"FUTURE","product_id":"BTC-CDE","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"1","price_increment":"0.01","base_min_size":"1","base_max_size":"10000","price":"60000","volume_24h":"100","price_percentage_change_24h":"2"},
        {"product_type":"SPOT","product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.0001","price_increment":"0.01","base_min_size":"0.0001","base_max_size":"10000","price":"60000","volume_24h":"100","price_percentage_change_24h":"2"}
    ]}"#,
    );
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().markets()[0].symbol, "BTC-USD");
    assert!(e.drain_actions().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
}

#[test]
fn malformed_first_product_page_does_not_hide_a_valid_later_page() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            spot_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.start(1000.);
    e.set_log_level(safe_cex_engine::Level::Debug);
    let first = requests(&mut e).pop().unwrap();
    reply(
        &mut e,
        &first,
        200,
        r#"{"products":[{"product_type":"FUTURE","product_id":"BTC-CDE","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"1","price_increment":"0.01","base_min_size":"1","base_max_size":"10000","price":"60000","volume_24h":"100","price_percentage_change_24h":"2"}],"pagination":{"has_next":true,"next_cursor":"after-malformed"}}"#,
    );
    assert!(!e.store().loaded().markets);
    let actions = e.drain_actions().collect::<Vec<_>>();
    assert!(actions.iter().any(|a| matches!(a, Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { message, .. })) if message.contains("Skipped malformed public discovery row"))));
    let timer = actions
        .into_iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, .. } => Some(timer),
            _ => None,
        })
        .unwrap();
    e.handle(1334., Event::TimerFired { timer });
    let next = requests(&mut e).pop().unwrap();
    assert!(next.url.contains("after-malformed"));
    reply(
        &mut e,
        &next,
        200,
        r#"{"products":[{"product_type":"SPOT","product_id":"BTC-USD","base_currency_id":"BTC","quote_currency_id":"USD","status":"TRADING","base_increment":"0.0001","price_increment":"0.01","base_min_size":"0.0001","base_max_size":"10000","price":"60000","volume_24h":"100","price_percentage_change_24h":"2"}],"pagination":{"has_next":false}}"#,
    );
    assert!(e.store().loaded().markets);
    assert_eq!(e.store().markets().len(), 1);
    assert_eq!(e.store().markets()[0].symbol, "BTC-USD");
}

#[test]
fn valid_catalogue_with_rejected_quotes_starts_markets_and_retries_without_clearing_prices() {
    for prior_quote in [false, true] {
        let mut e = safe_cex_engine::test_support::seeded_engine(
            Coinbase::new(Options {
                public_data_only: true,
                ..Options::default()
            })
            .unwrap(),
        );
        if prior_quote {
            let (previous, _) = ready();
            e.store_mut()
                .set_markets(previous.store().markets().to_vec());
            e.store_mut()
                .set_tickers(previous.store().tickers().to_vec());
        }
        e.start(1000.);
        let request = requests(&mut e).pop().unwrap();
        let before = serde_json::to_value(e.store().tickers()).unwrap();
        reply(
            &mut e,
            &request,
            200,
            &INSTRUMENTS.replace("\"trade_price\":\"60000\"", "\"trade_price\":\"-1\""),
        );
        assert!(e.store().loaded().markets);
        assert_eq!(serde_json::to_value(e.store().tickers()).unwrap(), before);
        assert_eq!(e.store().loaded().tickers, prior_quote);
        let timer = e
            .drain_actions()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if jittered(at_ms - 1000., 1000.) => Some(timer),
                _ => None,
            })
            .unwrap();
        e.handle(2000., Event::TimerFired { timer });
        let retry = requests(&mut e).pop().unwrap();
        reply(&mut e, &retry, 200, INSTRUMENTS);
        assert_eq!(e.store().ticker(SYMBOL).unwrap().last, 60000.);
        assert!(e.store().loaded().tickers);
    }
}

#[path = "support/socket.rs"]
mod socket_checks;
#[test]
fn public_supervisor_bounds_handshake_and_backs_off_flapping_connections() {
    let (e, socket) = ready();
    socket_checks::flaps(e, socket, 1100.);
}

#[test]
fn suppressed_public_feed_is_not_supervised() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        Coinbase::new(Options {
            public_data_only: true,
            ..Options::default()
        })
        .unwrap(),
    );
    e.set_public_feed_policy(safe_cex_engine::PublicFeedPolicy::Shared)
        .unwrap();
    e.start(1000.);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, INSTRUMENTS);
    let mut timers = std::collections::HashMap::new();
    let mut fired = 0;
    let mut now = 1000.;
    loop {
        let actions: Vec<_> = e.drain_actions().collect();
        for action in actions {
            match action {
                Action::SetTimer { timer, at_ms } => {
                    timers.insert(timer, at_ms);
                }
                Action::ClearTimer { timer } => {
                    timers.remove(&timer);
                }
                Action::Http(r) => reply(&mut e, &r, 200, INSTRUMENTS),
                Action::WsConnect { .. } | Action::WsClose { .. } => {
                    panic!("suppressed public feed reached the host")
                }
                _ => {}
            }
        }
        let Some((&timer, &at)) = timers
            .iter()
            .filter(|(_, at)| **at <= 601_000.)
            .min_by(|a, b| a.1.total_cmp(b.1))
        else {
            break;
        };
        timers.remove(&timer);
        now = at.max(now);
        fired += 1;
        e.handle(now, Event::TimerFired { timer });
    }
    // One startup timer is venue-owned; an unopened supervised feed fires >100.
    assert!(fired <= 1, "suppressed feed fired {fired} timers");
}
