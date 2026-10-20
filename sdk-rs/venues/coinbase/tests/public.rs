mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Notice, Venue};
use safe_cex_venue_coinbase::{Coinbase, Options};
use support::*;
const SYMBOL: &str = "BTC-PERP-INTX";
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
    let mut e = Engine::new(
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
            interval: "1m".into(),
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
                Some(result.unwrap())
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
    let mut e = Engine::new(
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
            interval: "1d".into(),
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
    let mut e = Engine::new(
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
    e.handle(1000., Event::WsOpened { socket });
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
                (at_ms == 12000.).then_some(timer)
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
    let mut e = Engine::new(
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
    let mut e = Engine::new(
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
    let mut e = Engine::new(
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
        |a| matches!(a,Action::Notice(Notice::Log {message,..}) if message.contains("cursor"))
    ));
}
#[test]
fn unknown_envelopes_cannot_keep_stale_socket_alive() {
    let mut e = Engine::new(
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
    e.handle(1000., Event::WsOpened { socket });
    e.handle(20000., Event::WsMessage { socket, text: "{}" });
    e.handle(21000., Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose {socket:s} if s==socket))
    );
}

#[test]
fn future_product_without_native_details_cannot_become_spot() {
    let mut e = Engine::new(
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
    let mut e = Engine::new(
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
    let mut e = Engine::new(
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
    let mut e = Engine::new(
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
