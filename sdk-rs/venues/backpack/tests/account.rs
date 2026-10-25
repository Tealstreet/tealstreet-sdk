mod support;
use safe_cex_engine::{Action, Event, Notice, SocketId};
use support::*;

/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 10_000.0;

#[test]
fn startup_signs_reads_and_populates_global_settings() {
    let (e, _) = ready();
    assert!(
        e.store().loaded().balance && e.store().loaded().orders && e.store().loaded().positions
    );
    assert!(e.store().hedged());
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().balance().upnl, 5.0);
    for m in e.store().markets() {
        let s = e.store().setting(&m.symbol).unwrap();
        assert_eq!(s.leverage, 10.0);
        assert_eq!(
            s.margin_type,
            Some(safe_cex_engine::model::MarginType::Cross)
        );
    }
}
const BOGUS_BTC: (&str, &str) = (
    r#"{"symbol":"BTC_USDC_PERP","marketType":"PERP","quantityIncrement":"0.001""#,
    r#"{"symbol":"BTC_USDC_PERP","marketType":"PERP","quantityIncrement":"bogus""#,
);
#[test]
fn account_rows_on_a_malformed_market_hold_the_snapshot_and_report_once() {
    let markets = MARKETS.replace(BOGUS_BTC.0, BOGUS_BTC.1);
    let btc_position = POSITIONS.replace("SOL_USDC_PERP", "BTC_USDC_PERP");
    let btc_order = format!("[{}]", ORDER.replace("SOL_USDC_PERP", "BTC_USDC_PERP"));
    let (mut e, socket, logs, _) = ready_catalogue(&markets, &btc_position, &btc_order);
    assert!(e.store().market("BTC_USDC_PERP").is_none());
    assert!(e.store().loaded().balance);
    assert!(!e.store().loaded().positions);
    assert!(!e.store().loaded().orders);
    let reports = logs
        .iter()
        .filter(|l| l.starts_with("Backpack account data for BTC_USDC_PERP is held"))
        .count();
    assert_eq!(reports, 1, "{logs:?}");
    e.handle(1000.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        "account.positionUpdate",
        &LONG.replace("SOL_USDC_PERP", "BTC_USDC_PERP"),
    );
    let logs: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) if message.contains("BTC_USDC_PERP") => Some(message),
            _ => None,
        })
        .collect();
    assert!(logs.is_empty(), "already reported during startup: {logs:?}");
}
#[test]
fn partial_catalogue_is_reread_without_restarting_the_account() {
    let markets = MARKETS.replace(BOGUS_BTC.0, BOGUS_BTC.1);
    let (mut e, socket, _, timers) = ready_catalogue(&markets, POSITIONS, "[]");
    let (timer, at) = *timers
        .iter()
        .find(|(_, at)| jittered(*at - 1000.0, 1000.0))
        .expect("partial catalogue schedules a re-read");
    e.handle(at, Event::TimerFired { timer });
    let reread = requests(&mut e);
    assert_eq!(reread.len(), 1);
    assert!(reread[0].url.ends_with("/api/v1/markets"));
    reply(&mut e, &reread[0], 200, MARKETS);
    assert!(e.store().market("BTC_USDC_PERP").is_some());
    let after: Vec<_> = e.drain_actions().collect();
    assert!(
        !after.iter().any(|a| matches!(
            a,
            Action::Http(_) | Action::WsConnect { .. } | Action::SetTimer { .. }
        )),
        "a complete re-read neither restarts startup nor schedules another"
    );
    e.handle(at, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    ws(
        &mut e,
        socket,
        "account.positionUpdate",
        &LONG.replace("SOL_USDC_PERP", "BTC_USDC_PERP"),
    );
    assert!(
        e.store()
            .positions()
            .iter()
            .any(|p| p.symbol == "BTC_USDC_PERP")
    );
}
#[test]
fn private_auth_is_single_subscription_and_watchdog_is_armed() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    let actions: Vec<_> = e.drain_actions().collect();
    let subs: Vec<_> = actions
        .iter()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                text.contains("SUBSCRIBE").then_some(text)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(subs.len(), 1);
    assert!(subs[0].contains("account.orderUpdate"));
    assert!(subs[0].contains("5000"));
    assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { .. })));
}
const LONG: &str =
    r#"{"s":"SOL_USDC_PERP","b":"140","q":"5","n":"705","P":"5","l":"100","f":"0.1","E":2000000}"#;
const SHORT: &str =
    r#"{"s":"SOL_USDC_PERP","b":"142","q":"-3","n":"426","P":"2","l":"180","f":"0.1","E":3000000}"#;
const ZERO: &str = r#"{"s":"SOL_USDC_PERP","b":"0","q":"0","n":"0","P":"0","f":"0.1","E":4000000}"#;
#[test]
fn side_flip_zero_and_old_event_cannot_resurrect_a_closed_position() {
    let (mut e, socket) = ready();
    ws(&mut e, socket, "account.positionUpdate", LONG);
    ws(&mut e, socket, "account.positionUpdate", SHORT);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(
        e.store().positions()[0].side,
        safe_cex_engine::model::PositionSide::Short
    );
    ws(&mut e, socket, "account.positionUpdate", ZERO);
    assert!(e.store().positions().is_empty());
    ws(&mut e, socket, "account.positionUpdate", LONG);
    assert!(e.store().positions().is_empty());
}
#[test]
fn late_and_failed_snapshots_preserve_stream_state() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    let reads = requests(&mut e);
    let positions = reads.iter().find(|r| r.url.ends_with("/position")).unwrap();
    ws(&mut e, socket, "account.positionUpdate", SHORT);
    reply(&mut e, positions, 200, POSITIONS);
    assert_eq!(
        e.store().positions()[0].side,
        safe_cex_engine::model::PositionSide::Short
    );
    let orders = reads.iter().find(|r| r.url.ends_with("/orders")).unwrap();
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        r#"{"e":"orderAccepted","i":"123","c":42,"E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"0","f":"GTC"}"#,
    );
    reply(&mut e, orders, 200, &format!("[{ORDER}]"));
    assert_eq!(e.store().orders().len(), 1);
    let balance = reads
        .iter()
        .find(|r| r.url.ends_with("/collateral"))
        .unwrap();
    reply(&mut e, balance, 503, "unavailable");
    assert_eq!(e.store().balance().total, 1005.0);
}
#[test]
fn duplicate_execution_emits_one_fill_and_never_regresses_order_state() {
    let (mut e, socket) = ready();
    let fill = r#"{"e":"orderFill","i":"123","c":42,"E":3000000,"X":"Filled","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"5","l":"5","L":"139.5","t":999,"f":"GTC"}"#;
    e.take_frame(1000.0);
    ws(&mut e, socket, "account.orderUpdate", fill);
    let first = e.take_market_frame(2000.0).unwrap().to_vec();
    assert!(!first.is_empty());
    ws(&mut e, socket, "account.orderUpdate", fill);
    assert!(e.take_market_frame(2000.0).is_none());
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        r#"{"e":"orderAccepted","i":"123","E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"0","f":"GTC"}"#,
    );
    assert_eq!(
        e.store().order("123").unwrap().status,
        safe_cex_engine::model::OrderStatus::Closed
    );
    assert_eq!(
        e.store().order("123").unwrap().client_order_id.as_deref(),
        Some("42")
    );
}

#[test]
fn documented_trigger_quantity_is_y_and_trigger_failure_is_terminal() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        r#"{"e":"triggerFailed","i":"456","E":3000000,"X":"TriggerFailed","s":"SOL_USDC_PERP","o":"Market","S":"Ask","P":"130","Y":"3","Q":"999","z":"0","f":"GTC"}"#,
    );
    let order = e.store().order("456").unwrap();
    assert_eq!(order.amount, 3.0);
    assert_eq!(order.status, safe_cex_engine::model::OrderStatus::Canceled);
}
#[test]
fn entry_price_uses_b_uppercase_from_the_documented_position_event() {
    let (mut e, socket) = ready();
    ws(
        &mut e,
        socket,
        "account.positionUpdate",
        r#"{"s":"SOL_USDC_PERP","b":123,"B":122,"q":5,"n":732,"P":"0","l":"0","f":0.1,"E":3000000}"#,
    );
    assert_eq!(e.store().positions()[0].entry_price, 122.0);
}

#[test]
fn newer_rest_snapshot_wins_even_without_an_intervening_stream_event() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    let first = requests(&mut e);
    e.handle(1001.0, Event::WsClosed { socket, code: 1006 });
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
    e.handle(2001.0, Event::TimerFired { timer: reconnect });
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
    e.handle(2001.0, Event::WsOpened { socket });
    let second = requests(&mut e);
    let newer = POSITIONS.replace("\"netQuantity\":\"5\"", "\"netQuantity\":\"7\"");
    reply(
        &mut e,
        second
            .iter()
            .find(|r| r.url.ends_with("/position"))
            .unwrap(),
        200,
        &newer,
    );
    reply(
        &mut e,
        first.iter().find(|r| r.url.ends_with("/position")).unwrap(),
        200,
        POSITIONS,
    );
    assert_eq!(e.store().positions()[0].contracts, 7.0);
}

#[test]
fn a_failed_position_refresh_keeps_positions_and_does_not_block_other_reads() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    let reads = requests(&mut e);
    let positions = reads.iter().find(|r| r.url.ends_with("/position")).unwrap();
    reply(&mut e, positions, 503, "unavailable");
    let balance = reads
        .iter()
        .find(|r| r.url.ends_with("/collateral"))
        .unwrap();
    reply(&mut e, balance, 200, &BALANCE.replace("1005", "2005"));
    assert_eq!(e.store().positions()[0].contracts, 5.0);
    assert_eq!(e.store().balance().total, 2005.0);
}

#[test]
fn dns_not_found_never_becomes_an_empty_position_snapshot() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    let r = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/position"))
        .unwrap();
    e.handle(
        2000.0,
        Event::HttpFailed {
            request: r.id,
            error: "host not found",
        },
    );
    assert_eq!(e.store().positions()[0].contracts, 5.0);
}
#[test]
fn startup_position_failure_still_loads_balance_orders_and_connects_private_socket() {
    let (e, _) = ready_position(503, "upstream not found");
    assert!(e.store().loaded().balance && e.store().loaded().orders);
    assert!(!e.store().loaded().positions);
    let (e, _) = ready_position(404, "endpoint not found");
    assert!(!e.store().loaded().positions);
    assert!(e.store().positions().is_empty());
}

#[test]
fn rejected_authentication_reconnects_despite_later_pong_replies() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    e.handle(
        2000.0,
        Event::WsMessage {
            socket,
            text: r#"{"error":{"code":4001,"message":"signature expired"}}"#,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(actions.contains(&Action::WsClose { socket }));
    let timer = actions
        .into_iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, .. } = a {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(
        2100.0,
        Event::WsMessage {
            socket,
            text: r#"{"error":{"code":4002}}"#,
        },
    );
    e.handle(3000.0, Event::TimerFired { timer });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
}

#[test]
fn auth_window_clamps_before_signing_private_requests() {
    for (input, expected) in [
        (5000.9, 5000),
        (120000.0, 60000),
        (f64::MAX, 60000),
        (0.0, 60000),
        (-1.0, 60000),
        (0.5, 0),
    ] {
        ready_window(200, POSITIONS, Some((input, expected)));
    }
}

// Drive the actual account loop through slow/fast updates, a parked stale callback,
// and a resumed read. Other timers are deliberately not fired.
fn policy_poll_read<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
    endpoint: &str,
) -> (f64, Vec<safe_cex_engine::HttpRequest>) {
    use safe_cex_engine::{Action, Event};
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 60_000.0 / POLL_MS,
        },
    );
    let slow: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            Action::Http(_) => panic!("policy updates must not issue account reads"),
            _ => None,
        })
        .collect();
    assert_eq!(slow.len(), 1, "exactly one recurring account poll");
    assert_eq!(slow[0].1, now + 60_000.0);
    engine.handle(
        now + 1.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 1_501.0);
    engine.handle(
        now + 2.0,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 1_501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        now + 90_000.0,
        Event::PollScaleChanged {
            scale: 1_500.0 / POLL_MS,
        },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 91_500.0);
    engine.handle(
        now + 91_500.0,
        Event::TimerFired {
            timer: resumed[0].0,
        },
    );
    let reads: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(request) => Some(request),
            _ => None,
        })
        .collect();
    assert!(
        reads.iter().any(|r| r.url.contains(endpoint)),
        "actual resumed account HTTP: {reads:?}"
    );
    let completed_at = now + 91_501.0;
    engine.handle(
        completed_at,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    assert!(
        !engine
            .drain_actions()
            .any(|a| matches!(a, Action::SetTimer { .. }))
    );
    (completed_at, reads)
}

fn policy_poll_after_completion<V: safe_cex_engine::Venue>(
    engine: &mut safe_cex_engine::Engine<V>,
    now: f64,
) {
    use safe_cex_engine::{Action, Event};
    // A response may schedule an ordinary repair/deadline, but the parked poll
    // itself must still be registered and adopt this newest policy.
    engine.drain_actions().for_each(drop);
    engine.handle(
        now,
        Event::PollScaleChanged {
            scale: 30_000.0 / POLL_MS,
        },
    );
    let times: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { at_ms, .. } => Some(at_ms),
            _ => None,
        })
        .collect();
    assert_eq!(times, vec![now + 30_000.0]);
}

#[test]
fn recurring_account_poll_obeys_live_policy_and_completion() {
    let (mut e, _) = ready();
    let engine = &mut e;
    let (now, mut reads) = policy_poll_read(engine, 2000.0, "/api/v1/account");
    for _ in 0..12 {
        if reads.is_empty() {
            break;
        }
        for r in reads {
            let body = if r.url.ends_with("/account") {
                r#"{"leverageLimit":"10"}"#
            } else if r.url.contains("/position") {
                POSITIONS
            } else if r.url.contains("collateral") {
                BALANCE
            } else {
                "[]"
            };
            engine.handle(
                now,
                safe_cex_engine::Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
        }
        reads = engine
            .drain_actions()
            .filter_map(|a| match a {
                safe_cex_engine::Action::Http(r) => Some(r),
                _ => None,
            })
            .collect();
    }
    assert!(reads.is_empty(), "account cycle must complete");
    policy_poll_after_completion(engine, now);
}
#[test]
fn rows_on_hidden_or_unlisted_markets_are_skipped_and_the_snapshot_commits() {
    let markets = MARKETS.replace(
        "]",
        r#",{"symbol":"HID_USDC_PERP","marketType":"PERP","visible":false,"quantityIncrement":"0.01","priceIncrement":"0.01"}]"#,
    );
    for symbol in ["HID_USDC_PERP", "MISSING_USDC_PERP"] {
        let positions = POSITIONS.replace(
            "[",
            &format!(
                "[{},",
                POSITIONS[1..POSITIONS.len() - 1].replace("SOL_USDC_PERP", symbol)
            ),
        );
        let orders = format!(
            "[{},{ORDER}]",
            ORDER
                .replace("SOL_USDC_PERP", symbol)
                .replace("\"123\"", "\"456\"")
        );
        let (e, _, logs, _) = ready_catalogue(&markets, &positions, &orders);
        assert!(e.store().market(symbol).is_none());
        let loaded = e.store().loaded();
        assert!(
            loaded.balance && loaded.positions && loaded.orders,
            "{symbol}"
        );
        assert_eq!(e.store().positions().len(), 1);
        assert_eq!(e.store().positions()[0].symbol, SYMBOL);
        assert_eq!(e.store().orders().len(), 1);
        assert_eq!(e.store().orders()[0].id, "123");
        assert_eq!(e.store().balance().total, 1005.0);
        assert!(
            logs.iter()
                .any(|l| l.contains(&format!("{symbol} is not a listed market"))),
            "{logs:?}"
        );
    }
}

#[test]
fn working_orders_on_unlisted_markets_are_skipped() {
    for (symbol, status) in [
        ("MISSING_USDC_PERP", "New"),
        ("MISSING_USDC_PERP", "Filled"),
        ("MISSING_USDC", "New"),
    ] {
        let order = ORDER
            .replace("SOL_USDC_PERP", symbol)
            .replace("New", status);
        let (e, _) = ready_orders(&format!("[{order}]"));
        assert!(e.store().loaded().orders, "{symbol} {status}");
        assert!(e.store().orders().is_empty());
    }
}

#[test]
fn every_positions_404_preserves_the_last_successful_snapshot() {
    for body in [
        "endpoint not found",
        r#"{"code":"NOT_FOUND","message":"gateway route missing"}"#,
    ] {
        let (mut e, socket) = ready();
        e.handle(1000.0, Event::WsOpened { socket });
        let read = requests(&mut e)
            .into_iter()
            .find(|r| r.url.ends_with("/position"))
            .unwrap();
        reply(&mut e, &read, 404, body);
        assert_eq!(e.store().positions()[0].contracts, 5.0);
    }
}
#[test]
fn malformed_spot_or_unnamed_rows_do_not_reread_the_catalogue() {
    let markets = MARKETS.replace(
        "]",
        r#",{"symbol":"SOL_USDC","marketType":"SPOT","quantityIncrement":"bogus"},{"quantityIncrement":"bogus"}]"#,
    );
    let (e, _, logs, timers) = ready_catalogue(&markets, POSITIONS, "[]");
    assert!(
        logs.iter()
            .any(|l| l.starts_with("Skipped 2 malformed Backpack market rows")),
        "{logs:?}"
    );
    assert!(
        !timers.iter().any(|(_, at)| jittered(*at - 1000.0, 1000.0)),
        "no catalogue re-read: {timers:?}"
    );
    assert!(e.store().loaded().positions);
}
#[test]
fn only_a_working_order_on_a_malformed_market_holds_the_orders() {
    let markets = MARKETS.replace(BOGUS_BTC.0, BOGUS_BTC.1);
    for (status, held) in [("New", true), ("Filled", false), ("Cancelled", false)] {
        let order = ORDER
            .replace("SOL_USDC_PERP", "BTC_USDC_PERP")
            .replace("\"123\"", "\"456\"")
            .replace("New", status);
        let (e, _, _, _) = ready_catalogue(&markets, POSITIONS, &format!("[{order},{ORDER}]"));
        assert_eq!(e.store().loaded().orders, !held, "{status}");
        if !held {
            assert_eq!(e.store().orders().len(), 1);
            assert_eq!(e.store().orders()[0].id, "123");
        }
    }
}
#[test]
fn refresh_that_omits_a_held_market_keeps_it_with_its_position_and_order() {
    let markets = MARKETS.replace(BOGUS_BTC.0, BOGUS_BTC.1);
    let (mut e, socket, _, timers) = ready_catalogue(&markets, POSITIONS, &format!("[{ORDER}]"));
    assert_eq!(e.store().positions().len(), 1);
    assert!(e.store().order("123").is_some());
    let listed = e.store().market(SYMBOL).cloned().unwrap();
    let (timer, at) = *timers
        .iter()
        .find(|(_, at)| jittered(*at - 1000.0, 1000.0))
        .expect("partial catalogue schedules a re-read");
    e.handle(at, Event::TimerFired { timer });
    let reread = requests(&mut e);
    // SOL_USDC_PERP drops out of the list while BTC_USDC_PERP stays malformed.
    reply(
        &mut e,
        &reread[0],
        200,
        &markets.replace(SYMBOL, "ETH_USDC_PERP"),
    );
    assert_eq!(e.store().market(SYMBOL), Some(&listed));
    assert!(e.store().market("ETH_USDC_PERP").is_some());
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, SYMBOL);
    assert!(e.store().order("123").is_some());
    e.handle(at, Event::WsOpened { socket });
    e.drain_actions().for_each(drop);
    ws(&mut e, socket, "account.positionUpdate", LONG);
    assert_eq!(e.store().positions()[0].symbol, SYMBOL);
}
#[test]
fn persistently_malformed_catalogue_stops_rereading_after_five_refreshes() {
    let markets = MARKETS.replace(BOGUS_BTC.0, BOGUS_BTC.1);
    let (mut e, _, _, timers) = ready_catalogue(&markets, POSITIONS, "[]");
    let mut retry = timers
        .iter()
        .find(|(_, at)| jittered(*at - 1000.0, 1000.0))
        .copied();
    let (mut rereads, mut warned) = (0, false);
    while let Some((timer, at)) = retry.take() {
        e.handle(at, Event::TimerFired { timer });
        let reread = requests(&mut e);
        reply(&mut e, &reread[0], 200, &markets);
        rereads += 1;
        for action in e.drain_actions() {
            match action {
                Action::SetTimer { timer, at_ms } => retry = Some((timer, at_ms)),
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) if message.contains("stops re-reading") => warned = true,
                _ => {}
            }
        }
        assert!(rereads <= 5, "re-read is bounded");
    }
    assert_eq!(rereads, 5);
    assert!(warned);
}

#[test]
fn fills_faster_than_the_collateral_read_cannot_starve_the_balance() {
    let (mut e, socket) = ready();
    e.handle(1000.0, Event::WsOpened { socket });
    let collateral = |actions: &[Action]| {
        actions.iter().find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/collateral") => Some(r.clone()),
            _ => None,
        })
    };
    let mut read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/collateral"))
        .unwrap();
    for step in 1..=3u32 {
        let now = 1000.0 + f64::from(step) * 1000.0;
        let fill = format!(
            r#"{{"stream":"account.orderUpdate","data":{{"e":"orderFill","i":"123","c":42,"E":{},"X":"PartiallyFilled","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"{step}","l":"1","L":"139.5","t":{step},"f":"GTC"}}}}"#,
            now * 1000.0
        );
        e.handle(
            now,
            Event::WsMessage {
                socket,
                text: &fill,
            },
        );
        let next = collateral(&e.drain_actions().collect::<Vec<_>>()).unwrap();
        let body = BALANCE.replace("1005", &(2000 + step).to_string());
        e.handle(
            now + 1.0,
            Event::HttpResponse {
                request: read.id,
                response: safe_cex_engine::Response {
                    status: 200,
                    headers: "",
                    body: &body,
                },
            },
        );
        let reread = collateral(&e.drain_actions().collect::<Vec<_>>());
        if step <= 2 {
            assert_eq!(e.store().balance().total, 1005.0, "step {step}");
            assert!(reread.is_none(), "step {step}");
        } else {
            assert_eq!(e.store().balance().total, 2003.0);
            assert!(reread.is_some(), "an admitted stale read is re-read");
        }
        read = next;
    }
}

// Private reads start before the ticker seed. A trigger order read then was
// labelled StopLoss against a missing price and stayed so until the next poll.
#[test]
fn trigger_order_read_before_the_ticker_seed_is_relabelled_when_it_lands() {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut e = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_backpack::Backpack::new(safe_cex_venue_backpack::Options {
            identity: None,
            read_identity: None,
            read_credentials_status: None,
            read_credentials: None,
            public_data_only: false,
            key: Some("test-public-key".into()),
            secret: Some(STANDARD.encode([7u8; 32])),
            recv_window: None,
        }),
    );
    e.start(1000.0);
    let r = requests(&mut e).pop().unwrap();
    reply(&mut e, &r, 200, MARKETS);
    let take_profit = r#"[{"id":"tp","symbol":"SOL_USDC_PERP","side":"Ask","orderType":"Market","triggerPrice":"150","quantity":"5","executedQuantity":"0","status":"TriggerPending","createdAt":1000}]"#;
    let mut seed = vec![];
    for _ in 0..6 {
        for r in requests(&mut e) {
            if r.url.contains("/tickers") || r.url.contains("/markPrices") {
                seed.push(r);
                continue;
            }
            let body = if r.url.ends_with("/account") {
                r#"{"leverageLimit":"10"}"#
            } else if r.url.ends_with("/position") {
                POSITIONS
            } else if r.url.ends_with("/collateral") {
                BALANCE
            } else {
                take_profit
            };
            reply(&mut e, &r, 200, body);
        }
    }
    let early = e
        .store()
        .order("tp")
        .expect("order read before the seed")
        .kind;
    assert_eq!(early, safe_cex_engine::model::OrderType::StopLoss);
    for r in seed {
        let body = if r.url.contains("/tickers") {
            r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
        } else {
            "[]"
        };
        reply(&mut e, &r, 200, body);
    }
    assert_eq!(
        e.store().order("tp").unwrap().kind,
        safe_cex_engine::model::OrderType::TakeProfit
    );
}

/// Answers one position and order re-read and returns its logs.
fn reread(e: &mut Driver, socket: SocketId, positions: &str, orders: &str) -> Vec<String> {
    e.handle(1000.0, Event::WsOpened { socket });
    for r in requests(e) {
        if r.url.ends_with("/position") {
            reply(e, &r, 200, positions);
        } else if r.url.ends_with("/orders") {
            reply(e, &r, 200, orders);
        }
    }
    e.drain_actions()
        .filter_map(|a| match a {
            Action::Notice(Notice::Event(event)) => Some(event.message),
            _ => None,
        })
        .collect()
}

#[test]
fn one_malformed_row_keeps_its_stored_copy_while_valid_peers_commit() {
    let row = |list: &str| list[1..list.len() - 1].to_owned();
    let btc_position = row(&POSITIONS
        .replace("SOL_USDC_PERP", "BTC_USDC_PERP")
        .replace("\"5\"", "\"3\""));
    let btc_order = ORDER
        .replace("SOL_USDC_PERP", "BTC_USDC_PERP")
        .replace("\"123\"", "\"456\"");
    let positions =
        |sol: &str, btc: &str| format!("[{},{btc}]", row(POSITIONS).replace("\"5\"", sol));
    let orders = |price: &str, btc: &str| format!("[{},{btc}]", ORDER.replace("\"140\"", price));
    let (mut e, socket, _, _) = ready_catalogue(
        MARKETS,
        &positions("\"5\"", &btc_position),
        &orders("\"140\"", &btc_order),
    );
    let stored_position = e.store().position("BTC_USDC_PERP-long").unwrap().clone();
    let stored_order = e.store().order("456").unwrap().clone();
    e.drain_actions().for_each(drop);

    let bad_position = btc_position.replace(r#""entryPrice":"140","#, "");
    let bad_order = btc_order.replace(r#""side":"Bid","#, "");
    for round in 0..2 {
        let logs = reread(
            &mut e,
            socket,
            &positions("\"7\"", &bad_position),
            &orders("\"141\"", &bad_order),
        );
        let store = e.store();
        assert_eq!(store.position("SOL_USDC_PERP-long").unwrap().contracts, 7.0);
        assert_eq!(store.position("BTC_USDC_PERP-long"), Some(&stored_position));
        assert_eq!(store.order("123").unwrap().price, 141.0);
        assert_eq!(store.order("456"), Some(&stored_order));
        let skips: Vec<_> = logs.iter().filter(|l| l.contains("skipped")).collect();
        if round == 0 {
            assert_eq!(
                skips,
                [
                    "Backpack skipped malformed order row 456",
                    "Backpack skipped malformed position row BTC_USDC_PERP"
                ],
                "{logs:?}"
            );
        } else {
            assert!(skips.is_empty(), "logged once: {logs:?}");
        }
        assert!(!logs.iter().any(|l| l.contains('{') || l.contains("140")));
    }

    let sol_only = POSITIONS.replace("\"5\"", "\"7\"");
    reread(&mut e, socket, &sol_only, &format!("[{ORDER}]"));
    let store = e.store();
    assert_eq!(store.positions().len(), 1);
    assert!(store.position("BTC_USDC_PERP-long").is_none());
    assert_eq!(store.orders().len(), 1);
    assert!(store.order("456").is_none());
}
#[test]
fn undecodable_rows_on_a_malformed_market_still_hold_the_snapshot() {
    let markets = MARKETS.replace(BOGUS_BTC.0, BOGUS_BTC.1);
    let position = r#"[{"symbol":"BTC_USDC_PERP","netQuantity":null},{"symbol":"SOL_USDC_PERP","netQuantity":"5","entryPrice":"140","netExposureNotional":"705","pnlUnrealized":"5","estLiquidationPrice":"100"}]"#;
    for (order, held) in [
        (
            r#"{"id":"456","symbol":"BTC_USDC_PERP","quantity":null}"#,
            true,
        ),
        (r#"{"id":456,"symbol":"BTC_USDC_PERP","status":7}"#, true),
        (
            r#"{"id":"456","symbol":"BTC_USDC_PERP","status":"Filled","quantity":null}"#,
            false,
        ),
    ] {
        let (e, _, logs, _) = ready_catalogue(&markets, position, &format!("[{order},{ORDER}]"));
        let loaded = e.store().loaded();
        assert_eq!((loaded.positions, loaded.orders), (false, !held), "{order}");
        assert!(
            logs.iter()
                .any(|l| l.starts_with("Backpack account data for BTC_USDC_PERP is held")),
            "{logs:?}"
        );
    }
}

#[test]
fn a_market_without_a_positive_step_is_malformed_and_holds_its_exposure() {
    for step in ["0", "-0.001"] {
        let markets = MARKETS.replace(
            BOGUS_BTC.0,
            &BOGUS_BTC.0.replace("\"0.001\"", &format!("\"{step}\"")),
        );
        let btc_position = POSITIONS.replace("SOL_USDC_PERP", "BTC_USDC_PERP");
        let (e, _, logs, _) = ready_catalogue(&markets, &btc_position, "[]");
        assert!(e.store().market("BTC_USDC_PERP").is_none());
        assert!(!e.store().loaded().positions);
        assert!(
            logs.iter()
                .any(|l| l.starts_with("Backpack account data for BTC_USDC_PERP is held")),
            "{logs:?}"
        );
    }
}

#[test]
fn null_or_missing_order_quantity_keeps_proven_trigger_size_and_valid_peers() {
    for missing in [false, true] {
        let mut trigger: serde_json::Value = serde_json::from_str(ORDER).unwrap();
        trigger["id"] = "trigger".into();
        trigger["quantity"] = serde_json::Value::Null;
        if missing {
            trigger.as_object_mut().unwrap().remove("quantity");
        }
        trigger["triggerPrice"] = "130".into();
        trigger["triggerQuantity"] = "3".into();
        trigger["triggerBy"] = "MarkPrice".into();
        let orders = format!("[{ORDER},{trigger}]");
        let (e, _, _, _) = ready_catalogue(MARKETS, POSITIONS, &orders);
        assert!(e.store().loaded().orders);
        assert!(e.store().order("123").is_some());
        let order = e.store().order("trigger").unwrap();
        assert_eq!(
            (order.amount, order.remaining, order.price),
            (3.0, 3.0, 130.0)
        );
    }
}

#[test]
fn unproven_missing_quantities_keep_prior_orders_and_never_start_repair() {
    for case in [
        "ordinary_null",
        "ordinary_missing",
        "zero_price",
        "zero_size",
        "negative_size",
        "overfilled",
        "blank",
        "bad",
        "nonfinite",
        "boolean",
    ] {
        let (mut e, socket) = ready_orders(&format!("[{ORDER}]"));
        let before = e.store().orders().to_vec();
        e.handle(1000.0, Event::WsOpened { socket });
        let read = requests(&mut e)
            .into_iter()
            .find(|r| r.url.ends_with("/orders"))
            .unwrap();
        let mut row: serde_json::Value = serde_json::from_str(ORDER).unwrap();
        row["quantity"] = serde_json::Value::Null;
        row["triggerPrice"] = "130".into();
        row["triggerQuantity"] = "3".into();
        match case {
            "ordinary_null" | "ordinary_missing" => {
                row.as_object_mut().unwrap().remove("triggerPrice");
                row.as_object_mut().unwrap().remove("triggerQuantity");
                if case == "ordinary_missing" {
                    row.as_object_mut().unwrap().remove("quantity");
                }
            }
            "zero_price" => row["triggerPrice"] = "0".into(),
            "zero_size" => row["triggerQuantity"] = "0".into(),
            "negative_size" => row["triggerQuantity"] = "-1".into(),
            "overfilled" => row["executedQuantity"] = "4".into(),
            "blank" => row["quantity"] = "".into(),
            "bad" => row["quantity"] = "broken".into(),
            "nonfinite" => row["quantity"] = "NaN".into(),
            "boolean" => row["quantity"] = true.into(),
            _ => unreachable!(),
        }
        reply(&mut e, &read, 200, &format!("[{row}]"));
        assert_eq!(e.store().orders(), before, "{case}");
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get)),
            "{case}"
        );
    }
}

#[test]
fn unproven_trigger_quantity_keeps_its_identity_while_valid_peers_commit_without_repair() {
    let peer = ORDER.replace("\"id\":\"123\"", "\"id\":\"456\"");
    let (mut e, socket, _, _) = ready_catalogue(MARKETS, POSITIONS, &format!("[{ORDER},{peer}]"));
    let retained = e.store().order("123").unwrap().clone();
    e.drain_actions().for_each(drop);
    let mut bad: serde_json::Value = serde_json::from_str(ORDER).unwrap();
    bad["quantity"] = serde_json::Value::Null;
    bad["triggerPrice"] = "130".into();
    bad["triggerQuantity"] = "0".into();
    for (round, price) in ["141", "142"].into_iter().enumerate() {
        let good = peer.replace("\"price\":\"140\"", &format!("\"price\":\"{price}\""));
        e.handle(1000.0, Event::WsOpened { socket });
        for request in requests(&mut e) {
            if request.url.ends_with("/position") {
                reply(&mut e, &request, 200, POSITIONS);
            } else if request.url.ends_with("/orders") {
                reply(&mut e, &request, 200, &format!("[{bad},{good}]"));
            }
        }
        let actions = e.drain_actions().collect::<Vec<_>>();
        let skipped = actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    Action::Notice(safe_cex_engine::Notice::Event(event))
                        if event.message == "Backpack skipped malformed order row 123"
                )
            })
            .count();
        assert_eq!(skipped, usize::from(round == 0), "row rejection logs once");
        assert_eq!(e.store().order("123"), Some(&retained));
        assert_eq!(
            e.store().order("456").unwrap().price,
            price.parse::<f64>().unwrap()
        );
        assert!(!actions.iter().any(|action|matches!(action,Action::Http(request) if request.method!=safe_cex_engine::Method::Get)));
    }
    reread(&mut e, socket, POSITIONS, "[]");
    assert!(e.store().orders().is_empty());
}

/// Keep the actual startup ticker replies parked while the private account chain completes.
fn snapshot_unpriced_trigger() -> (Driver, SocketId, Vec<safe_cex_engine::HttpRequest>) {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let mut e = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_backpack::Backpack::new(safe_cex_venue_backpack::Options {
            identity: None,
            read_identity: None,
            read_credentials_status: None,
            read_credentials: None,
            public_data_only: false,
            key: Some("test-public-key".into()),
            secret: Some(STANDARD.encode([7u8; 32])),
            recv_window: None,
        }),
    );
    e.start(1000.0);
    let markets = requests(&mut e).pop().unwrap();
    reply(&mut e, &markets, 200, MARKETS);
    let trigger = r#"[{"id":"tp","symbol":"SOL_USDC_PERP","side":"Ask","orderType":"Market","triggerPrice":"150","triggerBy":"MarkPrice","quantity":"5","executedQuantity":"0","status":"TriggerPending","createdAt":1000}]"#;
    let mut seeds = vec![];
    let mut private = None;
    for _ in 0..6 {
        let actions = e.drain_actions().collect::<Vec<_>>();
        for action in actions {
            match action {
                Action::Http(r) if r.url.contains("/tickers") || r.url.contains("/markPrices") => {
                    seeds.push(r)
                }
                Action::Http(r) => {
                    let body = if r.url.ends_with("/account") {
                        r#"{"leverageLimit":"10"}"#
                    } else if r.url.ends_with("/position") {
                        POSITIONS
                    } else if r.url.ends_with("/collateral") {
                        BALANCE
                    } else {
                        assert!(
                            r.url.ends_with("/orders"),
                            "unexpected startup request {}",
                            r.url
                        );
                        trigger
                    };
                    reply(&mut e, &r, 200, body);
                }
                Action::WsConnect {
                    socket,
                    channel: Some(safe_cex_engine::SocketChannel::Private),
                    ..
                } => private = Some(socket),
                _ => {}
            }
        }
    }
    assert_eq!(seeds.len(), 2);
    assert_eq!(
        e.store().order("tp").unwrap().kind,
        safe_cex_engine::model::OrderType::StopLoss
    );
    (e, private.unwrap(), seeds)
}

fn snapshot_finish_ticker_seed(e: &mut Driver, seeds: Vec<safe_cex_engine::HttpRequest>) {
    for r in seeds {
        let body = if r.url.contains("/tickers") {
            r#"[{"symbol":"SOL_USDC_PERP","lastPrice":"141","priceChangePercent":"0.01","volume":"10","quoteVolume":"1410"}]"#
        } else {
            "[]"
        };
        reply(e, &r, 200, body);
    }
    assert_eq!(e.store().ticker(SYMBOL).unwrap().last, 141.0);
}

#[test]
fn snapshot_stale_streamed_orders_cannot_erase_accepted_trigger_relabeling() {
    snapshot_rejected_stream_keeps_trigger_relabeling(false);
}

#[test]
fn snapshot_failed_streamed_orders_cannot_erase_accepted_trigger_relabeling() {
    snapshot_rejected_stream_keeps_trigger_relabeling(true);
}

fn snapshot_rejected_stream_keeps_trigger_relabeling(failed: bool) {
    let (mut e, socket, seeds) = snapshot_unpriced_trigger();
    e.handle(1000.0, Event::WsOpened { socket });
    let read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/orders"))
        .unwrap();
    // Stale-only and failed-current controls exercise independent rejection paths.
    if !failed {
        ws(
            &mut e,
            socket,
            "account.orderUpdate",
            r#"{"e":"orderAccepted","i":"peer","E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"140","q":"5","z":"0","f":"GTC"}"#,
        );
        assert!(e.store().order("peer").is_some());
    }
    e.handle(
        2000.0,
        Event::HttpStart {
            request: read.id,
            status: 200,
            headers: "",
        },
    );
    let stale = ORDER.replace("\"id\":\"123\"", "\"id\":\"tp\"");
    let chunk = format!("[{stale}]");
    e.handle(
        2000.0,
        Event::HttpChunk {
            request: read.id,
            data: chunk.as_bytes(),
        },
    );
    if failed {
        e.handle(
            2000.0,
            Event::HttpFailed {
                request: read.id,
                error: "stream interrupted",
            },
        );
    } else {
        e.handle(2000.0, Event::HttpEnd { request: read.id });
    }
    assert_eq!(
        e.store().order("tp").unwrap().side,
        safe_cex_engine::model::OrderSide::Sell
    );
    assert_eq!(e.store().order("peer").is_some(), !failed);
    snapshot_finish_ticker_seed(&mut e, seeds);
    assert_eq!(
        e.store().order("tp").unwrap().kind,
        safe_cex_engine::model::OrderType::TakeProfit,
        "a discarded read must not remove the accepted trigger's pending relabel"
    );
}

#[test]
fn snapshot_explicit_store_invalidation_discards_native_rows_and_repairs() {
    let (mut e, socket) = ready_orders(&format!("[{ORDER}]"));
    e.handle(1000.0, Event::WsOpened { socket });
    let read = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("/orders"))
        .unwrap();
    let before = e.store().orders().to_vec();
    e.store_mut().invalidate_order_snapshots();
    // A missing triggerBy would otherwise dispatch a cancel/replace repair.
    let repair = ORDER.replace(
        "\"orderType\":\"Limit\"",
        "\"orderType\":\"Market\",\"triggerPrice\":\"130\"",
    );
    reply(&mut e, &read, 200, &format!("[{repair}]"));
    assert_eq!(e.store().orders(), before);
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
}

#[test]
fn snapshot_exhaustion_keeps_writes_and_recurring_account_continuation() {
    let (mut e, socket, _, timers) = ready_catalogue(MARKETS, POSITIONS, &format!("[{ORDER}]"));
    let (mut timer, mut at) = timers
        .into_iter()
        .find(|(_, at)| jittered(*at - 1000.0, POLL_MS))
        .unwrap();
    safe_cex_engine::test_support::exhaust_order_snapshots(e.store_mut());
    ws(
        &mut e,
        socket,
        "account.orderUpdate",
        r#"{"e":"orderAccepted","i":"live","E":2000000,"X":"New","s":"SOL_USDC_PERP","o":"Limit","S":"Bid","p":"142","q":"5","z":"0","f":"GTC"}"#,
    );
    assert!(e.store().order("live").is_some());
    e.drain_actions().for_each(drop);
    for _ in 0..2 {
        e.handle(at, Event::TimerFired { timer });
        let mut next = None;
        let mut discarded = 0;
        let mut domains = 0;
        for _ in 0..6 {
            let actions = e.drain_actions().collect::<Vec<_>>();
            for action in actions {
                match action {
                    Action::Http(r) => {
                        assert!(
                            !r.url.ends_with("/orders"),
                            "exhausted reads never dispatch"
                        );
                        let body = if r.url.ends_with("/account") {
                            r#"{"leverageLimit":"10"}"#
                        } else if r.url.ends_with("/position") {
                            POSITIONS
                        } else {
                            assert!(r.url.ends_with("/collateral"));
                            BALANCE
                        };
                        domains += 1;
                        e.handle(
                            at,
                            Event::HttpResponse {
                                request: r.id,
                                response: safe_cex_engine::Response {
                                    status: 200,
                                    headers: "",
                                    body,
                                },
                            },
                        );
                    }
                    Action::Notice(Notice::Event(event))
                        if event.code == safe_cex_engine::log::code::SNAPSHOT_DISCARDED =>
                    {
                        assert_eq!(event.level, safe_cex_engine::Level::Error);
                        assert_eq!(event.message, "Backpack order snapshot authority exhausted");
                        discarded += 1;
                    }
                    Action::SetTimer { timer, at_ms } if jittered(at_ms - at, POLL_MS) => {
                        next = Some((timer, at_ms))
                    }
                    _ => {}
                }
            }
        }
        assert_eq!(domains, 3, "unrelated account domains still refresh");
        assert_eq!(discarded, 1);
        assert!(e.store().order("live").is_some());
        assert!(e.store().order("123").is_some());
        (timer, at) = next.expect("continuation schedules the next native poll");
    }
}
