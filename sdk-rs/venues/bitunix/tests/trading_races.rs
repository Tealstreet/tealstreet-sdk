//! Races between write acknowledgments, private events, and staged snapshots.
mod support;
use safe_cex_engine::model::{MarginType, OrderStatus, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice, RequestId, SocketId};
use support::*;

fn order(extra: &str) -> PlaceOrderOpts {
    serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500,"amount":0.002{extra}}}"#
    ))
    .unwrap()
}
fn protection(kind: &str, price: f64) -> PlaceOrderOpts {
    serde_json::from_str(&format!(r#"{{"symbol":"BTCUSDT","type":"{kind}","side":"buy","price":{price},"amount":0,"tpTriggerType":"mark_price","slTriggerType":"last_price"}}"#)).unwrap()
}
fn prepared_writes(e: &mut TestEngine) -> Vec<HttpRequest> {
    let mut writes = vec![];
    for _ in 0..10 {
        let reads = requests(e);
        if reads.is_empty() {
            return writes;
        }
        for r in reads {
            if r.method == Method::Post {
                writes.push(r);
            } else {
                reply(
                    e,
                    r.id,
                    if r.url.contains("get_pending_positions") {
                        POSITIONS
                    } else if r.url.contains("/tpsl/") {
                        TPSL
                    } else {
                        ORDERS
                    },
                );
            }
        }
    }
    panic!("preparation did not settle")
}
fn pending_read(e: &mut TestEngine, socket: SocketId, channel: &str) -> RequestId {
    event(e, socket, &format!(r#"{{"ch":"{channel}","data":{{}}}}"#));
    let timer = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer {
                timer,
                at_ms: 1100.0,
            } => Some(timer),
            _ => None,
        })
        .next_back()
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    requests(e).remove(0).id
}
fn done(e: &mut TestEngine, command: u32) -> Result<String, String> {
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone {
                command: id,
                result,
            }) if id == command => Some(Ok(result)),
            Action::Notice(Notice::CommandFailed { command: id, error }) if id == command => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("command completion")
}

#[test]
fn full_position_batch_coalesces_requested_legs_before_any_write() {
    for reverse in [false, true] {
        let (mut e, _) = startup();
        let mut orders = vec![
            protection("take_profit_market", 85000.0),
            protection("stop_market", 105000.0),
        ];
        if reverse {
            orders.reverse();
        }
        orders.push(order(r#", "clientOrderId":"plain-caller""#));
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders { command: 1, orders }),
        );
        let writes = prepared_writes(&mut e);
        assert_eq!(writes.len(), 1);
        let write = &writes[0];
        assert!(write.url.ends_with("/tpsl/position/place_order"));
        let body = write.body.as_deref().unwrap();
        for field in [
            r#""positionId":"position-1""#,
            r#""tpPrice":"85000""#,
            r#""slPrice":"105000""#,
            r#""tpStopType":"MARK_PRICE""#,
            r#""slStopType":"LAST_PRICE""#,
        ] {
            assert!(body.contains(field), "{body}");
        }
        reply(
            &mut e,
            write.id,
            r#"{"code":0,"data":{"orderId":"protect"}}"#,
        );
        let writes = requests(&mut e);
        assert_eq!(writes.len(), 1);
        assert!(writes[0].url.ends_with("/trade/place_order"));
        assert!(
            writes[0]
                .body
                .as_deref()
                .unwrap()
                .contains(r#""clientId":"plain-caller""#)
        );
        reply(
            &mut e,
            writes[0].id,
            r#"{"code":0,"data":{"orderId":"plain"}}"#,
        );
        assert_eq!(done(&mut e, 1).unwrap(), r#"["protect","plain"]"#);
    }
}

#[test]
fn full_position_batch_keeps_whole_batch_caller_id_preflight() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![
                protection("take_profit_market", 85000.0),
                order(r#", "clientOrderId":"duplicate""#),
                order(r#", "clientOrderId":"duplicate""#),
            ],
        }),
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 2, .. })))
    );
}

#[test]
fn settings_ack_invalidates_older_position_snapshots() {
    for (command, leverage, margin) in [
        (
            Command::SetLeverage {
                command: 3,
                symbol: "BTCUSDT".into(),
                leverage: 30.0,
                side: None,
            },
            30.0,
            Some(MarginType::Isolated),
        ),
        (
            Command::SetMarginType {
                command: 3,
                symbol: "BTCUSDT".into(),
                isolated: false,
            },
            20.0,
            Some(MarginType::Cross),
        ),
    ] {
        let (mut e, socket) = startup();
        let read = pending_read(&mut e, socket, "position");
        e.handle(1101.0, Event::Command(command));
        let write = requests(&mut e).remove(0);
        reply(&mut e, write.id, r#"{"code":0,"data":{}}"#);
        done(&mut e, 3).unwrap();
        assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, leverage);
        assert_eq!(e.store().setting("BTCUSDT").unwrap().margin_type, margin);
        reply(&mut e, read, POSITIONS);
        assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, leverage);
        assert_eq!(e.store().setting("BTCUSDT").unwrap().margin_type, margin);
        assert_eq!(e.store().positions()[0].leverage, leverage);
        assert_eq!(e.store().positions()[0].margin_type, margin);
        assert_eq!(e.store().positions()[0].realized_pnl, Some(-3.5));
    }
}

#[test]
fn position_mode_ack_invalidates_older_balance_and_position_snapshots() {
    let (mut e, socket) = startup();
    e.store_mut().set_positions(vec![]);
    let balance = pending_read(&mut e, socket, "balance");
    let positions = pending_read(&mut e, socket, "position");
    e.handle(
        1101.0,
        Event::Command(Command::SetPositionMode {
            command: 4,
            hedged: false,
            symbol: None,
        }),
    );
    let write = requests(&mut e).remove(0);
    reply(&mut e, write.id, r#"{"code":0,"data":{}}"#);
    done(&mut e, 4).unwrap();
    assert!(!e.store().hedged());
    reply(&mut e, balance, ACCOUNT);
    assert!(!e.store().hedged());
    reply(&mut e, positions, POSITIONS);
    assert!(e.store().positions().is_empty());
}

#[test]
fn cancel_ack_preserves_newer_filled_parent() {
    let (mut e, socket) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 5,
            ids: vec!["one".into()],
        }),
    );
    let write = requests(&mut e).remove(0);
    event(
        &mut e,
        socket,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"FILLED","dealAmount":"3","averagePrice":"100"}}"#,
    );
    reply(
        &mut e,
        write.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    assert_eq!(e.store().order("one").unwrap().filled, 3.0);
    assert_eq!(done(&mut e, 5).unwrap(), "[]");
}

#[test]
fn cancel_ack_preserves_newer_filled_virtual_tpsl_legs() {
    let (mut e, socket) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 6,
            ids: vec!["protect__position-1__tp".into()],
        }),
    );
    let write = requests(&mut e).remove(0);
    event(
        &mut e,
        socket,
        r#"{"ch":"tpsl","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"protect","status":"FILLED"}}"#,
    );
    reply(&mut e, write.id, r#"{"code":0,"data":{}}"#);
    for id in ["protect__position-1__tp", "protect__position-1__sl"] {
        assert_eq!(e.store().order(id).unwrap().status, OrderStatus::Closed);
    }
    assert_eq!(done(&mut e, 6).unwrap(), "[]");
}

#[test]
fn queued_cancel_does_not_mark_an_already_filled_order_canceling() {
    let (mut e, socket) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["protect__position-1__tp".into(), "one".into()],
        }),
    );
    let first = requests(&mut e).remove(0);
    assert!(first.url.ends_with("/tpsl/cancel_order"));
    event(
        &mut e,
        socket,
        r#"{"ch":"order","data":{"event":"CLOSE","symbol":"BTCUSDT","orderId":"one","orderStatus":"FILLED","dealAmount":"3","averagePrice":"100"}}"#,
    );
    reply(&mut e, first.id, r#"{"code":0,"data":{}}"#);
    let second = requests(&mut e).remove(0);
    assert!(second.url.ends_with("/trade/cancel_orders"));
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    reply(
        &mut e,
        second.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Closed);
    assert_eq!(done(&mut e, 7).unwrap(), "[]");
}
