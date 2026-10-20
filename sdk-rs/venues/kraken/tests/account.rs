mod support;
use safe_cex_engine::{Action, Engine, Event, SocketChannel, SocketId};
use safe_cex_venue_kraken::Kraken;
use support::*;
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
            && at_ms == NOW + 1000.0
        {
            Some(timer)
        } else {
            None
        }
    });
    let timer = timer.expect("heal timer");
    e.handle(NOW + 1000.0, Event::TimerFired { timer });
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
fn fresh_private(e: &mut Engine<Kraken>, old: SocketId) -> (SocketId, safe_cex_engine::TimerId) {
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
    let watchdog = e
        .drain_actions()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a
                && at_ms == NOW + 11000.0
            {
                Some(timer)
            } else {
                None
            }
        })
        .unwrap();
    (socket, watchdog)
}
#[test]
fn private_auth_requires_every_feed_ack_with_a_deadline() {
    let (mut e, s) = ready();
    let (s, t) = fresh_private(&mut e, s);
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
    e.drain_actions().for_each(drop);
    e.handle(NOW + 21000.0, Event::TimerFired { timer: t });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a,Action::WsClose{socket}if socket==s))
    );
}
#[test]
fn heartbeat_and_busy_fills_cannot_hide_absent_position_payloads() {
    let (mut e, s) = ready();
    let (s, mut timer) = fresh_private(&mut e, s);
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
    e.drain_actions().for_each(drop);
    for step in 1..=18 {
        let now = NOW + 1000.0 + 10000.0 * f64::from(step);
        message(&mut e, s, now, r#"{"event":"pong"}"#);
        message(&mut e, s, now, r#"{"feed":"fills","fills":[]}"#);
        e.handle(now, Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
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
            timer = actions
                .into_iter()
                .find_map(|a| {
                    if let Action::SetTimer { timer, at_ms } = a
                        && at_ms == now + 10000.0
                    {
                        Some(timer)
                    } else {
                        None
                    }
                })
                .unwrap();
        }
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
    let (s, mut timer) = fresh_private(&mut e, s);
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
    e.drain_actions().for_each(drop);
    // Event-driven fills/orders legitimately stay quiet for hours after snapshots.
    for step in 1..=180 {
        let now = NOW + 1000.0 + 10000.0 * f64::from(step);
        message(&mut e, s, now, r#"{"event":"pong"}"#);
        e.handle(now, Event::TimerFired { timer });
        let actions = e.drain_actions().collect::<Vec<_>>();
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a,Action::WsClose{socket}if *socket==s)),
            "quiet authenticated account reconnected at step {step}"
        );
        timer = actions
            .into_iter()
            .find_map(|a| {
                if let Action::SetTimer { timer, at_ms } = a
                    && at_ms == now + 10000.0
                {
                    Some(timer)
                } else {
                    None
                }
            })
            .expect("private watchdog remains armed");
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
