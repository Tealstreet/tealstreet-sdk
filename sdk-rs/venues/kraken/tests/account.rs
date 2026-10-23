mod support;
use safe_cex_engine::{Action, Engine, Event, SocketChannel, SocketId, TimerId};
use safe_cex_venue_kraken::Kraken;
use support::*;
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 5_000.0;

const ORDER: &str = r#"{"instrument":"PF_XBTUSD","order_id":"order-1","qty":2,"filled":0.5,"type":"stop","direction":1,"stop_price":90,"reduce_only":true,"triggerSignal":"last","last_update_time":1700000000000}"#;
fn snapshot(e: &mut Engine<Kraken>, socket: SocketId) {
    message(
        e,
        socket,
        NOW,
        &format!(r#"{{"feed":"open_orders_snapshot","orders":[{ORDER}]}}"#),
    );
}
fn poll(e: &mut Engine<Kraken>, socket: SocketId) -> Vec<safe_cex_engine::HttpRequest> {
    message(e, socket, NOW, r#"{"feed":"fills","fills":[]}"#);
    // Empty execution payload schedules the reconciliation debounce.
    let timer = e.drain_actions().find_map(|a| {
        if let Action::SetTimer { timer, at_ms } = a
            && (NOW + 1000.0..=NOW + 1001.0).contains(&at_ms)
        {
            Some((timer, at_ms))
        } else {
            None
        }
    });
    let (timer, at_ms) = timer.expect("heal timer");
    e.handle(at_ms, Event::TimerFired { timer });
    requests(e)
}
#[test]
fn mapped_orders_keep_trigger_provenance_and_never_emit_cumulative_fills() {
    let (mut e, socket) = ready();
    snapshot(&mut e, socket);
    let o = &e.store().orders()[0];
    assert_eq!(o.amount, 2.0);
    assert_eq!(o.remaining, 1.5);
    assert_eq!(o.filled, 0.5);
    assert!(o.reduce_only);
    assert_eq!(
        o.trigger_type,
        Some(safe_cex_engine::model::StopTriggerType::LastPrice)
    );
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 0);
}
#[test]
fn executions_dedup_by_native_fill_id_across_snapshot_and_reconnect() {
    let (mut e, socket) = ready();
    let fill = r#"{"fill_id":"exec-1","order_id":"order-1","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":false}"#;
    message(
        &mut e,
        socket,
        NOW,
        &format!(r#"{{"feed":"fills_snapshot","fills":[{fill}]}}"#),
    );
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 0);
    message(
        &mut e,
        socket,
        NOW,
        &format!(
            r#"{{"feed":"fills","fills":[{fill},{}]}}"#,
            fill.replace("exec-1", "exec-2")
        ),
    );
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 1);
    message(
        &mut e,
        socket,
        NOW,
        &format!(
            r#"{{"feed":"fills","fills":[{}]}}"#,
            fill.replace("exec-1", "exec-2")
        ),
    );
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 0);
    let (next, _) = fresh_private(&mut e, socket);
    message(
        &mut e,
        next,
        NOW + 1001.0,
        r#"{"event":"challenge","message":"reconnected"}"#,
    );
    for feed in ["fills", "open_orders", "open_positions"] {
        message(
            &mut e,
            next,
            NOW + 1002.0,
            &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
        );
    }
    message(
        &mut e,
        next,
        NOW + 1003.0,
        &format!(
            r#"{{"feed":"fills","fills":[{}]}}"#,
            fill.replace("exec-1", "exec-2")
        ),
    );
    assert_eq!(
        e.take_frame(NOW + 1003.0).map_or(0, fill_count),
        0,
        "recent execution replay after a fresh challenge must stay deduplicated"
    );
}
#[test]
fn fills_without_execution_identity_fail_closed() {
    let (mut e, s) = ready();
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"fills","fills":[{"order_id":"order-1","instrument":"PF_XBTUSD","qty":1,"price":100,"buy":true}]}"#,
    );
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 0);
}
#[test]
fn stale_rest_cannot_resurrect_ws_terminal_order_and_errors_preserve_balance() {
    let (mut e, socket) = ready();
    snapshot(&mut e, socket);
    // Force the existing +5s account timer preserved by bootstrap helper.
    let reqs = poll(&mut e, socket);
    {
        let r = reqs
            .iter()
            .find(|r| r.url.contains("openorders"))
            .expect("orders reconciliation");
        message(
            &mut e,
            socket,
            NOW + 5001.0,
            r#"{"feed":"open_orders","is_cancel":true,"order_id":"order-1","reason":"full_fill"}"#,
        );
        reply_at(
            &mut e,
            NOW + 5002.0,
            r,
            200,
            r#"{"result":"success","openOrders":[{"order_id":"order-1","symbol":"PF_XBTUSD","orderType":"lmt","side":"sell","unfilledSize":2,"filledSize":0,"limitPrice":100}]}"#,
        );
        assert!(e.store().orders().is_empty());
    }
    assert_eq!(e.store().balance().total, 1000.0);
    {
        let r = reqs
            .iter()
            .find(|r| r.url.contains("accounts"))
            .expect("balance reconciliation");
        reply(
            &mut e,
            r,
            200,
            r#"{"result":"error","error":"invalidNonce"}"#,
        );
        assert_eq!(e.store().balance().total, 1000.0);
    }
}
#[test]
fn stopped_order_may_transition_to_native_limit_without_tombstone() {
    let (mut e, s) = ready();
    snapshot(&mut e, s);
    message(
        &mut e,
        s,
        NOW + 1.0,
        r#"{"feed":"open_orders","is_cancel":true,"order_id":"order-1","reason":"stop_order_triggered"}"#,
    );
    assert!(e.store().orders().is_empty());
    message(
        &mut e,
        s,
        NOW + 2.0,
        &format!(
            r#"{{"feed":"open_orders","order":{}}}"#,
            ORDER
                .replace("\"stop\"", "\"limit\"")
                .replace("1700000000000", "1700000000002")
        ),
    );
    assert_eq!(e.store().orders().len(), 1);
}
#[test]
fn old_ws_update_does_not_overwrite_price_or_cancel_newer_order() {
    let (mut e, s) = ready();
    snapshot(&mut e, s);
    message(
        &mut e,
        s,
        NOW + 1.0,
        &format!(
            r#"{{"feed":"open_orders","order":{}}}"#,
            ORDER
                .replace("1700000000000", "1699999999999")
                .replace(":90", ":80")
        ),
    );
    assert_eq!(e.store().orders()[0].price, 90.0);
    message(
        &mut e,
        s,
        NOW + 2.0,
        &format!(r#"{{"feed":"open_orders","is_cancel":true,"order":{ORDER}}}"#),
    );
    assert!(e.store().orders().is_empty());
}
#[test]
fn authenticated_reconnect_requests_fresh_challenge_and_ignores_old_socket() {
    let (mut e, s) = ready();
    e.handle(
        NOW + 1.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
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
    e.handle(NOW + 1001.0, Event::TimerFired { timer });
    let next = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } = a
            {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    assert_ne!(s, next);
    e.handle(NOW + 1001.0, Event::WsOpened { socket: next });
    let sent = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(sent.len(), 1);
    assert!(sent[0].contains("challenge"));
    assert!(!sent[0].contains("signed_challenge"));
    message(
        &mut e,
        s,
        NOW + 1002.0,
        r#"{"event":"challenge","message":"old"}"#,
    );
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    message(
        &mut e,
        next,
        NOW + 1003.0,
        r#"{"event":"challenge","message":"fresh"}"#,
    );
    let sent = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::WsSend { text, .. } = a {
                Some(text)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(sent.len(), 3);
    for msg in sent {
        assert!(msg.contains("fresh"));
        assert!(msg.contains("signed_challenge"));
        assert!(!msg.contains("account_balances"));
    }
}
type Timers = Vec<(TimerId, f64)>;
fn track(timers: &mut Timers, actions: &[Action]) {
    for action in actions {
        match action {
            Action::SetTimer { timer, at_ms } => timers.push((*timer, *at_ms)),
            Action::ClearTimer { timer } => timers.retain(|(t, _)| t != timer),
            _ => (),
        }
    }
}
fn advance(e: &mut Engine<Kraken>, timers: &mut Timers, until: f64) -> Vec<Action> {
    let mut out = vec![];
    while let Some((timer, at)) = timers
        .iter()
        .copied()
        .filter(|(_, at)| *at <= until)
        .min_by(|a, b| a.1.total_cmp(&b.1))
    {
        timers.retain(|(t, _)| *t != timer);
        e.handle(at, Event::TimerFired { timer });
        let actions: Vec<_> = e.drain_actions().collect();
        track(timers, &actions);
        out.extend(actions);
    }
    out
}
fn fresh_private(e: &mut Engine<Kraken>, old: SocketId) -> (SocketId, Timers) {
    e.drain_actions().for_each(drop);
    e.handle(
        NOW,
        Event::WsClosed {
            socket: old,
            code: 1006,
        },
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
    e.handle(NOW + 1000.0, Event::TimerFired { timer });
    let socket = e
        .drain_actions()
        .find_map(|a| {
            if let Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } = a
            {
                Some(socket)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(NOW + 1000.0, Event::WsOpened { socket });
    let mut timers = vec![];
    track(&mut timers, &e.drain_actions().collect::<Vec<_>>());
    (socket, timers)
}
#[test]
fn private_auth_requires_every_feed_ack_with_a_deadline() {
    let (mut e, s) = ready();
    let (s, mut timers) = fresh_private(&mut e, s);
    message(
        &mut e,
        s,
        NOW + 1001.0,
        r#"{"event":"challenge","message":"fresh"}"#,
    );
    for feed in ["fills", "open_orders"] {
        message(
            &mut e,
            s,
            NOW + 1002.0,
            &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
        );
    }
    let mut actions = e.drain_actions().collect::<Vec<_>>();
    track(&mut timers, &actions);
    actions = advance(&mut e, &mut timers, NOW + 16000.0);
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==s))
    );
    actions = advance(&mut e, &mut timers, NOW + 21000.0);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==s))
    );
}
#[test]
fn heartbeat_and_busy_fills_cannot_hide_absent_position_payloads() {
    let (mut e, s) = ready();
    let (s, mut timers) = fresh_private(&mut e, s);
    message(
        &mut e,
        s,
        NOW + 1001.0,
        r#"{"event":"challenge","message":"fresh"}"#,
    );
    for feed in ["fills", "open_orders", "open_positions"] {
        message(
            &mut e,
            s,
            NOW + 1002.0,
            &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
        );
    }
    track(&mut timers, &e.drain_actions().collect::<Vec<_>>());
    for step in 1..=18 {
        let now = NOW + 1000.0 + 10000.0 * f64::from(step);
        let actions = advance(&mut e, &mut timers, now);
        let closed = actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==s));
        if step == 18 {
            assert!(
                closed,
                "missing positions/orders payload must recover even with pongs/fills"
            );
        } else {
            assert!(!closed);
        }
        message(&mut e, s, now, r#"{"event":"pong"}"#);
        message(&mut e, s, now, r#"{"feed":"fills","fills":[]}"#);
        track(&mut timers, &e.drain_actions().collect::<Vec<_>>());
    }
}
#[test]
fn private_payload_failed_subscription_reconnects_without_a_balance_feed() {
    let (mut e, s) = ready();
    message(
        &mut e,
        s,
        NOW,
        r#"{"event":"subscribed_failed","feed":"fills","message":"permission denied"}"#,
    );
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose{socket}if socket==s))
    );
}
const POSITION: &str = r#"{"feed":"open_positions","positions":[{"instrument":"PF_XBTUSD","balance":2,"entry_price":100,"mark_price":120,"pnl":40,"liquidation_threshold":50,"maxFixedLeverage":10}]}"#;
#[test]
fn malformed_full_snapshots_retain_confirmed_orders_and_exposure() {
    let (mut e, s) = ready();
    snapshot(&mut e, s);
    message(&mut e, s, NOW, POSITION);
    for text in [
        r#"{"feed":"open_orders_snapshot","orders":[{}]}"#,
        r#"{"feed":"open_positions","positions":[{}]}"#,
        r#"{"feed":"open_positions","positions":[{"instrument":"PF_XBTUSD","entry_price":100}]}"#,
    ] {
        message(&mut e, s, NOW + 1.0, text);
        assert_eq!(e.store().orders().len(), 1);
        assert_eq!(e.store().positions()[0].contracts, 2.0);
    }
    let reqs = poll(&mut e, s);
    let orders = reqs.iter().find(|r| r.url.contains("openorders")).unwrap();
    reply(
        &mut e,
        orders,
        200,
        r#"{"result":"success","openOrders":[{}]}"#,
    );
    assert_eq!(e.store().orders().len(), 1);
    let positions = reqs
        .iter()
        .find(|r| r.url.contains("openpositions"))
        .unwrap();
    reply(
        &mut e,
        positions,
        200,
        r#"{"result":"success","openPositions":[{"symbol":"PF_XBTUSD"}]}"#,
    );
    assert_eq!(e.store().positions()[0].contracts, 2.0);
}
#[test]
fn bad_rest_order_row_keeps_its_stored_copy_while_peers_commit() {
    for bad in [
        ORDER.replace(r#""type":"stop""#, r#""type":"mystery""#),
        r#"{"order_id":"order-1","qty":"x"}"#.to_string(),
        r#"{"qty":"x"}"#.to_string(),
    ] {
        let (mut e, s) = ready();
        snapshot(&mut e, s);
        let reqs = poll(&mut e, s);
        let orders = reqs.iter().find(|r| r.url.contains("openorders")).unwrap();
        let peer = ORDER.replace("order-1", "order-2");
        reply(
            &mut e,
            orders,
            200,
            &format!(r#"{{"result":"success","openOrders":[{bad},{peer}]}}"#),
        );
        let mut ids: Vec<_> = e.store().orders().iter().map(|o| o.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["order-1", "order-2"], "{bad}");
    }
}
#[test]
fn bad_rest_position_row_keeps_its_stored_legs_while_peers_commit() {
    for bad in [
        r#"{"symbol":"PF_XBTUSD","side":"sideways","size":3,"price":100}"#,
        r#"{"symbol":"PF_XBTUSD","size":"x"}"#,
        r#"{"size":3}"#,
    ] {
        let (mut e, s) = ready();
        message(&mut e, s, NOW, POSITION);
        let reqs = poll(&mut e, s);
        let positions = reqs
            .iter()
            .find(|r| r.url.contains("openpositions"))
            .unwrap();
        let peer = r#"{"symbol":"PF_ETHUSD","side":"short","size":1,"price":10}"#;
        reply(
            &mut e,
            positions,
            200,
            &format!(r#"{{"result":"success","openPositions":[{bad},{peer}]}}"#),
        );
        let btc = e.store().position("PF_XBTUSD:long").unwrap();
        assert_eq!(btc.contracts, 2.0, "{bad}");
        assert!(e.store().position("PF_ETHUSD:short").is_some(), "{bad}");
    }
}
#[test]
fn bad_stream_order_row_keeps_its_stored_copy_while_peers_commit() {
    for (bad, kept) in [
        (
            ORDER.replace(r#""type":"stop""#, r#""type":"mystery""#),
            &["order-1", "order-2"][..],
        ),
        (
            r#"{"order_id":"order-1","qty":"x"}"#.to_string(),
            &["order-1", "order-2"],
        ),
        (
            r#"{"qty":"x"}"#.to_string(),
            &["order-1", "order-2", "order-3"],
        ),
    ] {
        let (mut e, s) = ready();
        let reqs = poll(&mut e, s);
        let orders = reqs.iter().find(|r| r.url.contains("openorders")).unwrap();
        let stale = ORDER.replace("order-1", "order-3");
        reply(
            &mut e,
            orders,
            200,
            &format!(r#"{{"result":"success","openOrders":[{ORDER},{stale}]}}"#),
        );
        let peer = ORDER.replace("order-1", "order-2");
        message(
            &mut e,
            s,
            NOW + 1.0,
            &format!(r#"{{"feed":"open_orders_snapshot","orders":[{bad},{peer}]}}"#),
        );
        let mut ids: Vec<_> = e.store().orders().iter().map(|o| o.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, kept, "{bad}");
    }
}
#[test]
fn bad_stream_position_row_keeps_its_stored_legs_while_peers_commit() {
    for bad in [
        r#"{"instrument":"PF_XBTUSD","side":"sideways","size":3,"price":100}"#,
        r#"{"instrument":"PF_XBTUSD","balance":"x"}"#,
        r#"{"size":3}"#,
    ] {
        let (mut e, s) = ready();
        message(&mut e, s, NOW, POSITION);
        let peer = r#"{"instrument":"PF_ETHUSD","balance":-1,"entry_price":10,"mark_price":10}"#;
        message(
            &mut e,
            s,
            NOW + 1.0,
            &format!(r#"{{"feed":"open_positions","positions":[{bad},{peer}]}}"#),
        );
        let btc = e.store().position("PF_XBTUSD:long").unwrap();
        assert_eq!(btc.contracts, 2.0, "{bad}");
        assert!(e.store().position("PF_ETHUSD:short").is_some(), "{bad}");
    }
}
#[test]
fn rest_positions_preserve_ws_notional_liquidation_and_confirm_actual_isolation() {
    let (mut e, s) = ready();
    message(&mut e, s, NOW, POSITION);
    assert_eq!(e.store().positions()[0].notional, 240.0);
    assert_eq!(e.store().setting("PF_XBTUSD").unwrap().leverage, 10.0);
    let reqs = poll(&mut e, s);
    let positions = reqs
        .iter()
        .find(|r| r.url.contains("openpositions"))
        .unwrap();
    reply(
        &mut e,
        positions,
        200,
        r#"{"result":"success","openPositions":[{"symbol":"PF_XBTUSD","side":"long","size":2,"price":100,"unrealizedPnl":40,"maxFixedLeverage":10}]}"#,
    );
    let p = &e.store().positions()[0];
    assert_eq!(p.notional, 240.0);
    assert_eq!(p.liquidation_price, 50.0);
    assert_eq!(p.leverage, 10.0);
}
#[test]
fn closing_private_session_fences_old_rest_snapshots() {
    let (mut e, s) = ready();
    snapshot(&mut e, s);
    message(&mut e, s, NOW, POSITION);
    let reqs = poll(&mut e, s);
    e.handle(
        NOW + 1001.0,
        Event::WsClosed {
            socket: s,
            code: 1006,
        },
    );
    for r in reqs {
        if r.url.contains("openorders") {
            reply(&mut e, &r, 200, r#"{"result":"success","openOrders":[]}"#);
        } else if r.url.contains("openpositions") {
            reply(
                &mut e,
                &r,
                200,
                r#"{"result":"success","openPositions":[]}"#,
            );
        }
    }
    assert_eq!(e.store().orders().len(), 1);
    assert_eq!(e.store().positions()[0].contracts, 2.0);
}
#[test]
fn empty_positions_are_absent_and_default_settings_remain_available() {
    let (e, _) = ready();
    assert!(e.store().positions().is_empty());
    assert_eq!(e.store().setting("PF_XBTUSD").unwrap().leverage, 50.0);
}
#[test]
fn execution_remaining_updates_orders_without_counting_cumulative_progress_twice() {
    let (mut e, s) = ready();
    snapshot(&mut e, s);
    let execution = r#"{"feed":"fills","fills":[{"fill_id":"f1","order_id":"order-1","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":false,"remaining_order_qty":1,"time":1700000000001}]}"#;
    message(&mut e, s, NOW + 1.0, execution);
    assert_eq!(e.store().orders()[0].filled, 1.0);
    assert_eq!(e.store().orders()[0].remaining, 1.0);
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 1);
    message(&mut e, s, NOW + 2.0, execution);
    assert_eq!(e.store().orders()[0].filled, 1.0);
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 0);
    // The cumulative order update for the same execution is not another increment.
    message(
        &mut e,
        s,
        NOW + 3.0,
        &format!(
            r#"{{"feed":"open_orders","order":{}}}"#,
            ORDER
                .replace("\"filled\":0.5", "\"filled\":1")
                .replace("1700000000000", "1700000000002")
        ),
    );
    assert_eq!(e.store().orders()[0].filled, 1.0);
    let delayed = execution
        .replace("f1", "f2")
        .replace("1700000000001", "1699999999999")
        .replace("\"remaining_order_qty\":1", "\"remaining_order_qty\":0.5");
    message(&mut e, s, NOW + 4.0, &delayed);
    assert_eq!(e.store().orders()[0].filled, 1.0);
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 1);
}
#[test]
fn cumulative_order_before_execution_does_not_double_count_and_final_remaining_closes() {
    let (mut e, s) = ready();
    snapshot(&mut e, s);
    message(
        &mut e,
        s,
        NOW + 1.0,
        &format!(
            r#"{{"feed":"open_orders","order":{}}}"#,
            ORDER
                .replace("\"filled\":0.5", "\"filled\":1")
                .replace("1700000000000", "1700000000001")
        ),
    );
    message(
        &mut e,
        s,
        NOW + 2.0,
        r#"{"feed":"fills","fills":[{"fill_id":"f1","order_id":"order-1","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":false,"remaining_order_qty":1,"time":1700000000001}]}"#,
    );
    assert_eq!(e.store().orders()[0].filled, 1.0);
    message(
        &mut e,
        s,
        NOW + 3.0,
        r#"{"feed":"fills","fills":[{"fill_id":"f2","order_id":"order-1","instrument":"PF_XBTUSD","qty":1,"price":100,"buy":false,"remaining_order_qty":0,"time":1700000000003}]}"#,
    );
    assert!(e.store().orders().is_empty());
    assert_eq!(e.take_frame(NOW).map_or(0, fill_count), 2);
}
#[test]
fn quiet_authenticated_private_feeds_stay_open_after_initial_empty_snapshots() {
    let (mut e, s) = ready();
    let (s, mut timers) = fresh_private(&mut e, s);
    message(
        &mut e,
        s,
        NOW + 1001.0,
        r#"{"event":"challenge","message":"fresh"}"#,
    );
    for feed in ["fills", "open_orders", "open_positions"] {
        message(
            &mut e,
            s,
            NOW + 1002.0,
            &format!(r#"{{"event":"subscribed","feed":"{feed}"}}"#),
        );
    }
    for snapshot in [
        r#"{"feed":"fills_snapshot","fills":[]}"#,
        r#"{"feed":"open_orders_snapshot","orders":[]}"#,
        r#"{"feed":"open_positions","positions":[]}"#,
    ] {
        message(&mut e, s, NOW + 1002.0, snapshot);
    }
    track(&mut timers, &e.drain_actions().collect::<Vec<_>>());
    // Event-driven fills/orders legitimately stay quiet for hours after snapshots.
    for step in 1..=180 {
        let now = NOW + 1000.0 + 10000.0 * f64::from(step);
        let actions = advance(&mut e, &mut timers, now);
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::WsClose{socket}if *socket==s)),
            "quiet authenticated account reconnected at step {step}"
        );
        message(&mut e, s, now, r#"{"event":"pong"}"#);
        track(&mut timers, &e.drain_actions().collect::<Vec<_>>());
        assert!(!timers.is_empty(), "private watchdog remains armed");
    }
}
fn delayed_placement(e: &mut Engine<Kraken>, id: &str) {
    let order = serde_json::from_str(
        r#"{"symbol":"PF_XBTUSD","type":"limit","side":"buy","amount":2,"price":100}"#,
    )
    .unwrap();
    e.handle(
        NOW,
        Event::Command(safe_cex_engine::Command::PlaceOrders {
            command: 700,
            orders: vec![order],
        }),
    );
    let request = requests(e)
        .into_iter()
        .find(|r| r.url.ends_with("sendorder"))
        .unwrap();
    reply_at(
        e,
        NOW + 100.0,
        &request,
        200,
        &format!(r#"{{"result":"success","sendStatus":{{"status":"placed","order_id":"{id}"}}}}"#),
    );
    e.drain_actions().for_each(drop);
}
#[test]
fn authoritative_cumulative_fills_before_local_ack_arrival_are_merged_on_every_read_shape() {
    for shape in ["delta", "snapshot", "rest"] {
        let (mut e, mut s) = ready();
        delayed_placement(&mut e, "ack-order");
        let native = r#"{"order_id":"ack-order","instrument":"PF_XBTUSD","type":"lmt","direction":0,"qty":2,"filled":0.5,"limit_price":100,"last_update_time":1700000000090}"#;
        match shape {
            "delta" => message(
                &mut e,
                s,
                NOW + 110.0,
                &format!(r#"{{"feed":"open_orders","order":{native}}}"#),
            ),
            "snapshot" => {
                let (next, _) = fresh_private(&mut e, s);
                s = next;
                message(
                    &mut e,
                    s,
                    NOW + 1001.0,
                    r#"{"event":"challenge","message":"fresh"}"#,
                );
                message(
                    &mut e,
                    s,
                    NOW + 1002.0,
                    &format!(r#"{{"feed":"open_orders_snapshot","orders":[{native}]}}"#),
                );
            }
            "rest" => {
                let reads = poll(&mut e, s);
                let request = reads.iter().find(|r| r.url.contains("openorders")).unwrap();
                reply_at(
                    &mut e,
                    NOW + 1100.0,
                    request,
                    200,
                    r#"{"result":"success","openOrders":[{"order_id":"ack-order","symbol":"PF_XBTUSD","orderType":"lmt","side":"buy","unfilledSize":1.5,"filledSize":0.5,"limitPrice":100,"lastUpdateTime":1700000000090}]}"#,
                );
            }
            _ => unreachable!(),
        }
        let order = e.store().order("ack-order").unwrap();
        assert_eq!(
            (order.filled, order.remaining),
            (0.5, 1.5),
            "{shape} lost an authoritative partial fill before local ACK arrival"
        );
        assert_eq!((order.price, order.amount), (100.0, 2.0));
    }
}
#[test]
fn execution_remaining_before_local_ack_arrival_advances_placement_without_doublecount() {
    let (mut e, s) = ready();
    delayed_placement(&mut e, "ack-order");
    let fill = r#"{"feed":"fills","fills":[{"fill_id":"before-ack","order_id":"ack-order","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":true,"remaining_order_qty":1.5,"time":1700000000090}]}"#;
    message(&mut e, s, NOW + 110.0, fill);
    let order = e.store().order("ack-order").unwrap();
    assert_eq!((order.filled, order.remaining), (0.5, 1.5));
    assert_eq!(e.take_frame(NOW + 110.0).map_or(0, fill_count), 1);
    message(&mut e, s, NOW + 111.0, fill);
    assert_eq!(e.store().order("ack-order").unwrap().filled, 0.5);
    assert_eq!(e.take_frame(NOW + 111.0).map_or(0, fill_count), 0);
}
#[test]
fn cumulative_progress_before_edit_ack_keeps_accepted_economics_and_ambiguous_remaining_is_not_inferred()
 {
    let (mut e, s) = ready();
    delayed_placement(&mut e, "ack-order");
    e.handle(
        NOW + 110.0,
        Event::Command(safe_cex_engine::Command::AmendOrder {
            command: 701,
            id: "ack-order".into(),
            price: Some(110.0),
            amount: Some(4.0),
        }),
    );
    let request = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("editorder"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 200.0,
        &request,
        200,
        r#"{"result":"success","editStatus":{"status":"edited","orderId":"ack-order"}}"#,
    );
    e.drain_actions().for_each(drop);
    message(
        &mut e,
        s,
        NOW + 210.0,
        r#"{"feed":"fills","fills":[{"fill_id":"pre-edit-size","order_id":"ack-order","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":true,"remaining_order_qty":1.5,"time":1700000000150}]}"#,
    );
    let order = e.store().order("ack-order").unwrap();
    assert_eq!(
        (order.amount, order.filled),
        (4.0, 0.0),
        "remaining from the earlier native total must not manufacture 2.5 filled"
    );
    message(
        &mut e,
        s,
        NOW + 211.0,
        r#"{"feed":"open_orders","order":{"order_id":"ack-order","instrument":"PF_XBTUSD","type":"lmt","direction":0,"qty":2,"filled":0.5,"limit_price":100,"last_update_time":1700000000150}}"#,
    );
    let order = e.store().order("ack-order").unwrap();
    assert_eq!(
        (order.price, order.amount, order.filled, order.remaining),
        (110.0, 4.0, 0.5, 3.5)
    );
}
#[test]
fn quantity_ack_ambiguity_survives_a_second_price_only_ack_and_shrink_stays_bounded() {
    for amount in [1.0, 4.0] {
        let (mut e, s) = ready();
        delayed_placement(&mut e, "ack-order");
        for (command, price, size, at) in
            [(702, 110.0, Some(amount), 200.0), (703, 115.0, None, 300.0)]
        {
            e.handle(
                NOW + at - 50.0,
                Event::Command(safe_cex_engine::Command::AmendOrder {
                    command,
                    id: "ack-order".into(),
                    price: Some(price),
                    amount: size,
                }),
            );
            let request = requests(&mut e)
                .into_iter()
                .find(|r| r.url.ends_with("editorder"))
                .unwrap();
            reply_at(
                &mut e,
                NOW + at,
                &request,
                200,
                r#"{"result":"success","editStatus":{"status":"edited","orderId":"ack-order"}}"#,
            );
            e.drain_actions().for_each(drop);
        }
        message(
            &mut e,
            s,
            NOW + 310.0,
            r#"{"feed":"fills","fills":[{"fill_id":"pre-size","order_id":"ack-order","instrument":"PF_XBTUSD","qty":0.5,"price":100,"buy":true,"remaining_order_qty":1.5,"time":1700000000150}]}"#,
        );
        assert_eq!(
            e.store().order("ack-order").unwrap().filled,
            0.0,
            "a later price ACK cannot relabel an earlier total's remaining quantity"
        );
        message(
            &mut e,
            s,
            NOW + 311.0,
            r#"{"feed":"open_orders","order":{"order_id":"ack-order","instrument":"PF_XBTUSD","type":"lmt","direction":0,"qty":2,"filled":1.5,"limit_price":100,"last_update_time":1700000000150}}"#,
        );
        let order = e.store().order("ack-order").unwrap();
        assert_eq!((order.price, order.amount), (115.0, amount));
        assert_eq!(order.filled, 1.5f64.min(amount));
        assert_eq!(order.remaining, (amount - 1.5).max(0.0));
        assert!(order.filled <= order.amount);
    }
}

#[test]
fn local_price_ack_preserves_the_last_native_execution_clock() {
    let (mut e, s) = ready();
    delayed_placement(&mut e, "ack-order");
    message(
        &mut e,
        s,
        NOW + 110.0,
        r#"{"feed":"open_orders","order":{"order_id":"ack-order","instrument":"PF_XBTUSD","type":"lmt","direction":0,"qty":2,"filled":0.25,"limit_price":100,"last_update_time":1700000000090}}"#,
    );
    e.handle(
        NOW + 120.0,
        Event::Command(safe_cex_engine::Command::AmendOrder {
            command: 704,
            id: "ack-order".into(),
            price: Some(110.0),
            amount: None,
        }),
    );
    let request = requests(&mut e)
        .into_iter()
        .find(|r| r.url.ends_with("editorder"))
        .unwrap();
    reply_at(
        &mut e,
        NOW + 200.0,
        &request,
        200,
        r#"{"result":"success","editStatus":{"status":"edited","orderId":"ack-order"}}"#,
    );
    e.drain_actions().for_each(drop);
    message(
        &mut e,
        s,
        NOW + 210.0,
        r#"{"feed":"fills","fills":[{"fill_id":"old-native-time","order_id":"ack-order","instrument":"PF_XBTUSD","qty":0.25,"price":100,"buy":true,"remaining_order_qty":0.5,"time":1700000000080}]}"#,
    );
    assert_eq!(e.store().order("ack-order").unwrap().filled, 0.25);
    message(
        &mut e,
        s,
        NOW + 211.0,
        r#"{"feed":"fills","fills":[{"fill_id":"new-native-time","order_id":"ack-order","instrument":"PF_XBTUSD","qty":0.25,"price":100,"buy":true,"remaining_order_qty":1.5,"time":1700000000150}]}"#,
    );
    let order = e.store().order("ack-order").unwrap();
    assert_eq!(
        (order.price, order.filled, order.remaining),
        (110.0, 0.5, 1.5)
    );
}

#[test]
fn periodic_account_policy_changes_preserve_inflight_reads_and_fence_stale_timers() {
    let (mut e, _) = ready();
    let now = NOW + 1000.;
    e.drain_actions().for_each(drop);
    e.handle(
        now,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    let stale: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::ClearTimer { timer } => Some(timer),
            _ => None,
        })
        .collect();
    assert_eq!(stale.len(), 1, "one recurring account registration");
    let mut obsolete = stale;
    let mut current = None;
    for interval in [100_000_f64, 1_500.] {
        e.handle(
            now,
            Event::PollScaleChanged {
                scale: interval / POLL_MS,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        let timers: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(timers.len(), 1);
        assert_eq!(timers[0].1, now + interval.max(0.0));
        if let Some((timer, _)) = current.replace(timers[0]) {
            obsolete.push(timer);
        }
    }
    for timer in obsolete {
        e.handle(now, Event::TimerFired { timer });
        assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
    }
    let (timer, due) = current.unwrap();
    e.handle(due, Event::TimerFired { timer });
    let mut pending: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .collect();
    assert!(
        pending.iter().any(|r| r.url.contains("/accounts")),
        "actual native account request"
    );
    // Pause while REST is in flight. Completion may finish the current read
    // chain, but must not recreate an active recurring host timer.
    e.handle(
        due,
        Event::PollScaleChanged {
            scale: f64::INFINITY,
        },
    );
    e.drain_actions().for_each(drop);
    for _ in 0..12 {
        if pending.is_empty() {
            break;
        }
        for r in std::mem::take(&mut pending) {
            let body = if r.url.contains("/accounts") {
                ACCOUNT
            } else if r.url.contains("/openorders") {
                OPEN
            } else if r.url.contains("leveragepreferences") {
                r#"{"result":"success","leveragePreferences":[]}"#
            } else {
                r#"{"result":"success","openPositions":[]}"#
            };
            e.handle(
                due + 1.,
                Event::HttpResponse {
                    request: r.id,
                    response: safe_cex_engine::Response {
                        status: 200,
                        headers: "",
                        body,
                    },
                },
            );
            pending.extend(e.drain_actions().filter_map(|a| match a {
                Action::Http(r) => Some(r),
                _ => None,
            }));
        }
    }
    assert!(pending.is_empty(), "all fixture account reads complete");
    e.handle(
        due + 2.,
        Event::PollScaleChanged {
            scale: 10_000. / POLL_MS,
        },
    );
    let resumed: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(
        resumed.len(),
        1,
        "completion retains one paused registration"
    );
    assert_eq!(resumed[0].1, due + 2. + 10_000_f64.max(0.0));
}
#[test]
fn filtered_market_rows_are_skipped_while_listed_rows_commit() {
    let (mut e, s) = ready();
    // PF_UNKNOWNUSD stands for a market the catalogue filtered (untradeable) or delisted.
    message(
        &mut e,
        s,
        NOW,
        &format!(
            r#"{{"feed":"open_orders_snapshot","orders":[{{"instrument":"PF_UNKNOWNUSD","order_id":"unknown","qty":2}},{ORDER}]}}"#
        ),
    );
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"open_positions_snapshot","positions":[{"instrument":"PF_UNKNOWNUSD","balance":2},{"instrument":"PF_XBTUSD","balance":2,"entry_price":100,"mark_price":120,"pnl":40,"liquidation_threshold":50,"maxFixedLeverage":10}]}"#,
    );
    let ids = |e: &Engine<Kraken>| {
        e.store()
            .orders()
            .iter()
            .map(|o| o.id.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&e), ["order-1"]);
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, "PF_XBTUSD");
    let logs: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                message,
                ..
            })) => Some(message),
            _ => None,
        })
        .collect();
    assert!(
        logs.iter()
            .any(|l| l.contains("PF_UNKNOWNUSD is not a listed market"))
    );
    let reqs = poll(&mut e, s);
    for (path, body) in [
        (
            "openorders",
            r#"{"result":"success","openOrders":[{"symbol":"PF_UNKNOWNUSD","order_id":"unknown","qty":2},{"order_id":"order-2","symbol":"PF_XBTUSD","orderType":"lmt","side":"buy","unfilledSize":1,"filledSize":0,"limitPrice":90}]}"#,
        ),
        (
            "openpositions",
            r#"{"result":"success","openPositions":[{"symbol":"PF_UNKNOWNUSD","size":2},{"symbol":"PF_ETHUSD","side":"long","size":3,"price":50,"unrealizedPnl":0,"maxFixedLeverage":10}]}"#,
        ),
    ] {
        let request = reqs.iter().find(|r| r.url.contains(path)).unwrap();
        reply(&mut e, request, 200, body);
    }
    assert!(e.store().loaded().orders && e.store().loaded().positions);
    assert!(e.store().order("order-2").is_some());
    assert!(
        e.store()
            .positions()
            .iter()
            .any(|p| p.symbol == "PF_ETHUSD")
    );
    assert!(
        e.store()
            .positions()
            .iter()
            .all(|p| p.symbol != "PF_UNKNOWNUSD")
    );
    // Unsupported dated products and explicit zero positions carry no owned
    // perpetual exposure; they do not make otherwise valid empty scopes fatal.
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"open_orders","orders":[{"instrument":"FI_XBTUSD_260626","order_id":"dated","qty":2}]}"#,
    );
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"open_positions_snapshot","positions":[{"instrument":"FI_XBTUSD_260626","size":2},{"instrument":"PF_UNKNOWNUSD","balance":0}]}"#,
    );
    assert!(e.store().orders().is_empty());
    assert!(e.store().positions().is_empty());
}

const BAD_ROW: &str = r#"{"symbol":"PF_SOLUSD","base":"SOL","quote":"USD","tradeable":true,"tickSize":0.01,"contractSize":1,"contractValueTradePrecision":2.5,"marginLevels":[{"initialMargin":0.1}]}"#;

fn with_row(row: &str) -> String {
    META.replace("}]}]}", &format!("}}]}},{row}]}}"))
}

#[test]
fn exposure_on_a_malformed_catalogue_row_holds_the_previous_snapshot() {
    let (mut e, s, _) = ready_with_catalogue(options(), &with_row(BAD_ROW));
    assert!(e.store().market("PF_SOLUSD").is_none());
    snapshot(&mut e, s);
    message(&mut e, s, NOW, POSITION);
    let orders = e.store().orders().to_vec();
    let positions = e.store().positions().to_vec();
    assert_eq!((orders.len(), positions.len()), (1, 1));
    for text in [
        r#"{"feed":"open_orders_snapshot","orders":[{"instrument":"PF_SOLUSD","order_id":"bad","qty":2}]}"#,
        r#"{"feed":"open_positions_snapshot","positions":[{"instrument":"PF_SOLUSD","balance":2}]}"#,
    ] {
        message(&mut e, s, NOW, text);
        assert_eq!(e.store().orders(), orders);
        assert_eq!(e.store().positions(), positions);
    }
    let reqs = poll(&mut e, s);
    for (path, body) in [
        (
            "openorders",
            r#"{"result":"success","openOrders":[{"symbol":"PF_SOLUSD","order_id":"bad","qty":2}]}"#,
        ),
        (
            "openpositions",
            r#"{"result":"success","openPositions":[{"symbol":"PF_SOLUSD","size":2}]}"#,
        ),
    ] {
        let request = reqs.iter().find(|r| r.url.contains(path)).unwrap();
        reply(&mut e, request, 200, body);
        assert_eq!(e.store().orders(), orders);
        assert_eq!(e.store().positions(), positions);
    }
}

#[test]
fn partial_catalogue_is_reread_and_the_repaired_market_releases_its_rows() {
    let (mut e, s, timers) = ready_with_catalogue(options(), &with_row(BAD_ROW));
    let (timer, at) = *timers
        .iter()
        .find(|(_, at)| jittered(*at - NOW, 1000.0))
        .expect("partial catalogue schedules a re-read");
    e.handle(at, Event::TimerFired { timer });
    let reread = requests(&mut e);
    assert_eq!(reread.len(), 1);
    assert!(reread[0].url.ends_with("/derivatives/api/v3/instruments"));
    reply(
        &mut e,
        &reread[0],
        200,
        &with_row(&BAD_ROW.replace("2.5", "2")),
    );
    assert!(e.store().market("PF_SOLUSD").is_some());
    // The refresh neither restarts the account nor reconnects or re-seeds.
    let after: Vec<_> = e.drain_actions().collect();
    assert!(!after.iter().any(|a| matches!(
        a,
        Action::Http(_) | Action::WsConnect { .. } | Action::SetTimer { .. }
    )));
    message(
        &mut e,
        s,
        NOW,
        r#"{"feed":"open_positions_snapshot","positions":[{"instrument":"PF_SOLUSD","balance":2,"entry_price":100,"mark_price":120,"pnl":40,"liquidation_threshold":50,"maxFixedLeverage":10}]}"#,
    );
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, "PF_SOLUSD");
}

#[test]
fn stream_row_without_max_fixed_leverage_keeps_proven_isolation() {
    let (mut e, s) = ready();
    message(&mut e, s, NOW, POSITION);
    assert_eq!(
        e.store().positions()[0].margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    message(
        &mut e,
        s,
        NOW + 1.0,
        r#"{"feed":"open_positions","positions":[{"instrument":"PF_XBTUSD","balance":2,"entry_price":100,"mark_price":121,"pnl":42,"liquidation_threshold":50}]}"#,
    );
    let p = &e.store().positions()[0];
    assert_eq!(
        p.margin_type,
        Some(safe_cex_engine::model::MarginType::Isolated)
    );
    assert_eq!(p.leverage, 10.0);
}

#[test]
fn refresh_that_filters_a_held_market_keeps_it_with_its_position_and_order() {
    let (mut e, s, timers) = ready_with_catalogue(options(), &with_row(BAD_ROW));
    snapshot(&mut e, s);
    message(&mut e, s, NOW, POSITION);
    let listed = e.store().market("PF_XBTUSD").cloned().unwrap();
    let (timer, at) = *timers
        .iter()
        .find(|(_, at)| jittered(*at - NOW, 1000.0))
        .expect("partial catalogue schedules a re-read");
    e.handle(at, Event::TimerFired { timer });
    let reread = requests(&mut e);
    // PF_XBTUSD is now untradeable, which the catalogue filters out.
    let untradeable = with_row(BAD_ROW).replacen(r#""tradeable":true"#, r#""tradeable":false"#, 1);
    reply(&mut e, &reread[0], 200, &untradeable);
    assert_eq!(e.store().market("PF_XBTUSD"), Some(&listed));
    snapshot(&mut e, s);
    message(
        &mut e,
        s,
        NOW,
        &POSITION.replace("open_positions", "open_positions_snapshot"),
    );
    assert!(e.store().loaded().orders && e.store().loaded().positions);
    assert!(e.store().order("order-1").is_some());
    assert_eq!(e.store().positions().len(), 1);
    assert_eq!(e.store().positions()[0].symbol, "PF_XBTUSD");
}

#[test]
fn persistently_malformed_catalogue_stops_rereading_after_five_refreshes() {
    let (mut e, _, timers) = ready_with_catalogue(options(), &with_row(BAD_ROW));
    let mut retry = timers
        .iter()
        .find(|(_, at)| jittered(*at - NOW, 1000.0))
        .copied();
    let (mut rereads, mut warned) = (0, false);
    while let Some((timer, at)) = retry.take() {
        e.handle(at, Event::TimerFired { timer });
        let reread = requests(&mut e);
        assert!(reread[0].url.ends_with("/derivatives/api/v3/instruments"));
        reply(&mut e, &reread[0], 200, &with_row(BAD_ROW));
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
fn failed_reconciliation_heals_are_jittered_while_event_heals_keep_their_debounce() {
    let (mut e, s) = ready();
    message(&mut e, s, NOW, r#"{"feed":"fills","fills":[]}"#);
    let debounce: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms > NOW && at_ms <= NOW + 1001.0 => {
                Some((timer, at_ms - NOW))
            }
            _ => None,
        })
        .collect();
    assert_eq!(debounce.len(), 1);
    assert_eq!(debounce[0].1, 1000.0, "event heal is a fixed debounce");
    let mut now = NOW + 1000.0;
    e.handle(
        now,
        Event::TimerFired {
            timer: debounce[0].0,
        },
    );
    let mut reads = requests(&mut e);
    let (mut failures, mut delays) = (0u32, vec![]);
    for _ in 0..6 {
        assert!(!reads.is_empty());
        // The first failure in a batch schedules the heal at its failure count.
        let nominal = (1000.0 * 2f64.powi((failures + 1).min(5) as i32)).min(30_000.0);
        for read in &reads {
            reply_at(&mut e, now, read, 500, "{}");
        }
        failures += reads.len() as u32;
        let (timer, at_ms) = e
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if at_ms > now + 1001.0 => Some((timer, at_ms)),
                _ => None,
            })
            .expect("failed read schedules a heal");
        assert!(
            jittered(at_ms - now, nominal),
            "{} vs {nominal}",
            at_ms - now
        );
        delays.push((at_ms - now, nominal));
        now = at_ms;
        e.handle(now, Event::TimerFired { timer });
        reads = requests(&mut e);
    }
    assert!(delays.iter().any(|(delay, nominal)| delay < nominal));
}

#[test]
fn rejected_order_rows_report_error_only_without_a_retained_copy_and_rearm() {
    use safe_cex_engine::{Level, Notice, log::code};
    let (mut e, socket) = ready();
    snapshot(&mut e, socket);
    e.drain_actions().for_each(drop);
    let bad = ORDER.replace(r#""type":"stop""#, r#""type":"unsupported""#);
    let missing = bad.replace("order-1", "order-2");
    let send = |e: &mut Engine<Kraken>, rows: &str| {
        message(
            e,
            socket,
            NOW,
            &format!(r#"{{"feed":"open_orders","orders":[{rows}]}}"#),
        );
        e.drain_actions()
            .filter_map(|a| match a {
                Action::Notice(Notice::Event(event)) if event.code == code::ROW_SKIPPED => {
                    Some((event.level, event.message))
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let events = send(&mut e, &format!("{bad},{missing}"));
    assert_eq!(
        events,
        [
            (
                Level::Warn,
                "Kraken skipped unmappable order order-1".into()
            ),
            (
                Level::Error,
                "Kraken skipped unmappable order order-2".into()
            )
        ]
    );
    assert!(e.store().order("order-1").is_some());
    assert!(e.store().order("order-2").is_none());
    assert!(send(&mut e, &format!("{bad},{missing}")).is_empty());
    send(&mut e, ORDER);
    assert_eq!(
        send(&mut e, &bad),
        [(
            Level::Warn,
            "Kraken skipped unmappable order order-1".into()
        )]
    );
    message(
        &mut e,
        socket,
        NOW,
        r#"{"feed":"open_orders","is_cancel":true,"order_id":"order-1","reason":"full_fill"}"#,
    );
    e.drain_actions().for_each(drop);
    assert_eq!(
        send(&mut e, &bad),
        [(
            Level::Error,
            "Kraken skipped unmappable order order-1".into()
        )]
    );
    assert!(send(&mut e, &bad).is_empty());
}

fn row_reports(e: &mut Engine<Kraken>) -> Vec<(safe_cex_engine::Level, String)> {
    e.drain_actions()
        .filter_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == safe_cex_engine::log::code::ROW_SKIPPED =>
            {
                Some((event.level, event.message))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn row_report_unlisted_symbols_share_permanent_memory_across_feeds_and_reconnect() {
    use safe_cex_engine::Level;
    let (mut e, socket) = ready();
    row_reports(&mut e);
    message(
        &mut e,
        socket,
        NOW,
        r#"{"feed":"open_positions","positions":[{"instrument":"PF_ZEROUSD","balance":0}]}"#,
    );
    assert!(
        row_reports(&mut e).is_empty(),
        "zero quantity must not consume memory"
    );
    for symbol in ["PF_ZEROUSD", "PF_OTHERUSD"] {
        message(
            &mut e,
            socket,
            NOW,
            &format!(
                r#"{{"feed":"open_orders","orders":[{{"instrument":"{symbol}","order_id":"unlisted","qty":2}}]}}"#
            ),
        );
        assert_eq!(
            row_reports(&mut e),
            [(
                Level::Warn,
                format!("Kraken {symbol} is not a listed market; its rows are skipped")
            )]
        );
        message(
            &mut e,
            socket,
            NOW,
            &format!(
                r#"{{"feed":"open_positions","positions":[{{"instrument":"{symbol}","balance":2}}]}}"#
            ),
        );
        assert!(row_reports(&mut e).is_empty());
    }
    message(&mut e, socket, NOW, r#"{"feed":"open_orders","orders":[]}"#);
    assert!(row_reports(&mut e).is_empty());
    let (next, _) = fresh_private(&mut e, socket);
    message(
        &mut e,
        next,
        NOW + 1001.0,
        r#"{"event":"challenge","message":"reconnected"}"#,
    );
    row_reports(&mut e);
    message(
        &mut e,
        next,
        NOW + 1002.0,
        r#"{"feed":"open_orders_snapshot","orders":[{"instrument":"PF_ZEROUSD","order_id":"again","qty":2}]}"#,
    );
    assert!(
        row_reports(&mut e).is_empty(),
        "clean reads and reconnect never rearm unlisted symbols"
    );
    message(
        &mut e,
        next,
        NOW + 1003.0,
        r#"{"feed":"open_orders_snapshot","orders":[{"instrument":"PF_FRESHUSD","order_id":"new-symbol","qty":2}]}"#,
    );
    assert_eq!(
        row_reports(&mut e),
        [(
            Level::Warn,
            "Kraken PF_FRESHUSD is not a listed market; its rows are skipped".into()
        )],
        "new-socket frames are actively processed"
    );
}

#[test]
fn row_report_unlisted_read_is_remembered_before_stale_or_failed_admission() {
    for failed in [false, true] {
        let (mut e, socket) = ready();
        snapshot(&mut e, socket);
        let reads = poll(&mut e, socket);
        let read = reads
            .iter()
            .find(|read| read.url.contains("openorders"))
            .unwrap();
        message(
            &mut e,
            socket,
            NOW,
            r#"{"feed":"open_orders","is_cancel":true,"order_id":"order-1","reason":"full_fill"}"#,
        );
        row_reports(&mut e);
        let result = if failed { "error" } else { "success" };
        reply(
            &mut e,
            read,
            200,
            &format!(
                r#"{{"result":"{result}","openOrders":[{{"symbol":"PF_UNKNOWNUSD","order_id":"unknown","qty":2}}]}}"#
            ),
        );
        assert_eq!(
            row_reports(&mut e).len(),
            1,
            "mapping reports before admission"
        );
        assert!(e.store().order("order-1").is_none());
        message(
            &mut e,
            socket,
            NOW,
            r#"{"feed":"open_positions","positions":[{"instrument":"PF_UNKNOWNUSD","balance":2}]}"#,
        );
        assert!(
            row_reports(&mut e).is_empty(),
            "held read already consumed this symbol"
        );
    }
}

#[test]
fn row_report_rejected_duplicate_identity_cannot_rearm_from_its_accepted_peer() {
    let (mut e, socket) = ready();
    snapshot(&mut e, socket);
    row_reports(&mut e);
    let bad = ORDER.replace(r#""type":"stop""#, r#""type":"unsupported""#);
    let send = |e: &mut Engine<Kraken>| {
        message(
            e,
            socket,
            NOW,
            &format!(r#"{{"feed":"open_orders","orders":[{bad}]}}"#),
        );
        row_reports(e)
    };
    assert_eq!(send(&mut e).len(), 1);
    let reads = poll(&mut e, socket);
    let read = reads
        .iter()
        .find(|read| read.url.contains("openorders"))
        .unwrap();
    reply(
        &mut e,
        read,
        200,
        &format!(r#"{{"result":"success","openOrders":[{ORDER},{bad}]}}"#),
    );
    assert!(row_reports(&mut e).is_empty());
    assert!(
        send(&mut e).is_empty(),
        "the rejected identity remains protected"
    );
}

#[test]
fn row_report_stale_ws_snapshot_does_not_rearm_but_a_current_clean_update_does() {
    let (mut e, socket) = ready();
    snapshot(&mut e, socket);
    row_reports(&mut e);
    let bad = ORDER.replace(r#""type":"stop""#, r#""type":"unsupported""#);
    let send = |e: &mut Engine<Kraken>, feed: &str, rows: &str| {
        message(
            e,
            socket,
            NOW,
            &format!(r#"{{"feed":"{feed}","orders":[{rows}]}}"#),
        );
        row_reports(e)
    };
    assert_eq!(send(&mut e, "open_orders", &bad).len(), 1);
    assert!(send(&mut e, "open_orders_snapshot", ORDER).is_empty());
    assert!(
        send(&mut e, "open_orders", &bad).is_empty(),
        "stale snapshot rows do not rearm"
    );
    assert!(send(&mut e, "open_orders", ORDER).is_empty());
    assert_eq!(
        send(&mut e, "open_orders", &bad).len(),
        1,
        "the current clean read still rearms"
    );
}
