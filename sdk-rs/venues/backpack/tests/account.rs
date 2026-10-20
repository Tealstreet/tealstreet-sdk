mod support;
use safe_cex_engine::{Action, Event};
use support::*;

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
        Event::TickIntervalChanged {
            interval_ms: 60_000.0,
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
    engine.handle(now + 1.0, Event::TickIntervalChanged { interval_ms: 500.0 });
    let fast: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(fast.len(), 1);
    assert_eq!(fast[0].1, now + 501.0);
    engine.handle(
        now + 2.0,
        Event::TickIntervalChanged {
            interval_ms: f64::INFINITY,
        },
    );
    let paused: Vec<_> = engine.drain_actions().collect();
    assert!(
        paused
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == fast[0].0))
    );
    assert!(!paused.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    engine.handle(now + 501.0, Event::TimerFired { timer: fast[0].0 });
    assert!(!engine.drain_actions().any(|a| matches!(a, Action::Http(_))));
    engine.handle(
        now + 90_000.0,
        Event::TickIntervalChanged { interval_ms: 500.0 },
    );
    let resumed: Vec<_> = engine
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(resumed.len(), 1);
    assert_eq!(resumed[0].1, now + 90_500.0);
    engine.handle(
        now + 90_500.0,
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
    let completed_at = now + 90_501.0;
    engine.handle(
        completed_at,
        Event::TickIntervalChanged {
            interval_ms: f64::INFINITY,
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
        Event::TickIntervalChanged {
            interval_ms: 30_000.0,
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
