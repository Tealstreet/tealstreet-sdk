//! Outbound payloads follow bitunix.com/api-docs/futures/trade and tp_sl.
mod support;
use safe_cex_engine::model::{OrderStatus, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;
fn order(extra: &str) -> PlaceOrderOpts {
    serde_json::from_str(&format!(r#"{{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500.04,"amount":0.0021{extra}}}"#)).unwrap()
}
fn done(e: &mut TestEngine, command: u32) -> Result<String, String> {
    e.drain_actions()
        .find_map(|a| {
            if let Action::Notice(Notice::CommandDone { command: c, result }) = a {
                if c == command { Some(result) } else { None }
            } else {
                None
            }
        })
        .expect("command completion")
}
#[test]
fn unsupported_bybit_controls_on_final_batch_member_prevent_all_io() {
    for control in [
        r#", "bybitBbo":{"sideType":"Queue","level":1}"#,
        r#", "bybitSlippageToleranceTicks":2"#,
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order(""), order(control)],
            }),
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 1,
                result: Err(_)
            })
        )));
        assert!(
            !e.store()
                .orders()
                .iter()
                .any(|o| o.status == OrderStatus::Creating)
        );
    }
}

#[test]
fn native_client_id_is_verbatim_and_whole_batch_rejects_before_writes() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 1,
            orders: vec![order(r#", "clientOrderId":"bare-client""#)],
        }),
    );
    assert!(
        e.store()
            .orders()
            .iter()
            .any(|o| o.status == OrderStatus::Creating)
    );
    let r = requests(&mut e).remove(0);
    let body = r.body.as_deref().unwrap();
    assert!(body.contains(r#""clientId":"bare-client""#));
    assert!(body.contains(r#""qty":"0.002""#));
    assert!(body.contains(r#""price":"90500""#));
    assert!(!body.contains("ts_bare"));
    reply(&mut e, r.id, r#"{"code":0,"data":{"orderId":"new"}}"#);
    assert_eq!(done(&mut e, 1).unwrap(), r#"["new"]"#);
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.status == OrderStatus::Creating)
    );
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![
                order(r#", "clientOrderId":"duplicate""#),
                order(r#", "clientOrderId":"duplicate""#),
            ],
        }),
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}
#[test]
fn chunks_at_five_with_one_second_cooldown_and_never_retries_unknown_writes() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 3,
            orders: (0..6).map(|_| order("")).collect(),
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("batch_order"));
    assert_eq!(r.body.as_ref().unwrap().matches("clientId").count(), 5);
    reply(
        &mut e,
        r.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"1"},{"orderId":"2"},{"orderId":"3"},{"orderId":"4"},{"orderId":"5"}],"failureList":[]}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    let timer = actions
        .iter()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if *at_ms == 1000.0 { Some(*timer) } else { None }
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1000.0, Event::TimerFired { timer });
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("place_order"));
    e.handle(
        1001.0,
        Event::HttpFailed {
            request: r.id,
            error: "response lost",
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
}
#[test]
fn native_amend_preserves_order_identity_and_attached_siblings() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 4,
            id: "one__tp".into(),
            price: Some(79.04),
            amount: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("trade/modify_order"));
    let body = r.body.as_deref().unwrap();
    for field in [
        r#""orderId":"one""#,
        r#""tpPrice":"79""#,
        r#""slPrice":"110""#,
        r#""qty":"3""#,
    ] {
        assert!(body.contains(field), "{body}");
    }
    assert!(!body.contains("clientId"));
    reply(
        &mut e,
        r.id,
        r#"{"code":0,"data":{"orderId":"replacement"}}"#,
    );
    assert_eq!(done(&mut e, 4).unwrap(), r#"["replacement__tp"]"#);
}
#[test]
fn standalone_tpsl_amend_uses_raw_id_and_preserves_sibling() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 5,
            id: "protect__position-1__sl".into(),
            price: Some(100100.04),
            amount: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("tpsl/modify_order"));
    let b = r.body.as_deref().unwrap();
    for field in [
        r#""orderId":"protect""#,
        r#""slPrice":"100100""#,
        r#""tpPrice":"90000""#,
        r#""tpQty":"0.01""#,
        r#""slQty":"0.01""#,
    ] {
        assert!(b.contains(field), "{b}");
    }
}
#[test]
fn cancel_failure_restores_open_status_without_replacing_parent() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 6,
            ids: vec!["one__tp".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("trade/cancel_orders"));
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceling
    );
    reply(
        &mut e,
        r.id,
        r#"{"code":10001,"msg":"request too frequently"}"#,
    );
    assert!(done(&mut e, 6).is_err());
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}
#[test]
fn rejected_settings_write_never_changes_settings() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::SetLeverage {
            command: 7,
            symbol: "BTCUSDT".into(),
            leverage: 30.0,
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(&mut e, r.id, r#"{"code":10001,"msg":"rejected"}"#);
    assert!(done(&mut e, 7).is_err());
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
}

#[test]
fn cancel_item_failure_aborts_replacement_and_keeps_parent_open() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["one__tp".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        r.id,
        r#"{"code":0,"data":{"successList":[],"failureList":[{"orderId":"one","errorMsg":"cancel rejected"}]}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}
#[test]
fn native_amend_accepts_limit_protection_and_targeted_standalone_leg() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","tpOrderType":"LIMIT","tpOrderPrice":"79"}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 9,
            id: "one__tp".into(),
            price: Some(80.0),
            amount: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(
        r.body
            .as_deref()
            .unwrap()
            .contains(r#""tpOrderType":"LIMIT""#)
    );
    event(
        &mut e,
        s,
        r#"{"ch":"tpsl","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"second","positionId":"position-1","side":"Buy","type":"TPSL","tpPrice":"85000","tpQty":"0.01"}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 10,
            id: "second".into(),
            price: Some(86000.0),
            amount: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    let b = r.body.as_deref().unwrap();
    assert!(b.contains(r#""orderId":"second""#));
    assert!(b.contains(r#""tpPrice":"86000""#));
    assert!(b.contains(r#""slPrice":"100000""#));
}
#[test]
fn split_order_rejection_cannot_resend_already_accepted_chunks() {
    let (mut e, _) = startup();
    let mut o = order("");
    o.amount = 600.0;
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 11,
            orders: vec![o],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("batch_order"));
    reply(
        &mut e,
        r.id,
        r#"{"code":0,"data":{"successList":[{"id":"1"},{"id":"2"},{"id":"3"},{"id":"4"},{"id":"5"}],"failureList":[]}}"#,
    );
    let timer = e
        .drain_actions()
        .rev()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if at_ms == 1000.0 { Some(timer) } else { None }
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1000.0, Event::TimerFired { timer });
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("place_order"));
    reply(
        &mut e,
        r.id,
        r#"{"code":20003,"msg":"Position does not exist"}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.status == OrderStatus::Creating)
    );
}
#[test]
fn full_tpsl_without_credentials_completes_with_error() {
    let mut e = safe_cex_engine::Engine::new(safe_cex_venue_bitunix::BitunixFutures::new(
        Default::default(),
    ));
    let mut o = order("");
    o.kind = safe_cex_engine::model::OrderType::StopLoss;
    o.amount = 0.0;
    e.handle(
        0.0,
        Event::Command(Command::PlaceOrders {
            command: 12,
            orders: vec![o],
        }),
    );
    assert!(done(&mut e, 12).is_err());
}

#[test]
fn replacement_waits_for_final_canceled_quantity_before_placing() {
    let (mut e, s) = startup();
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 13,
            id: "one__tp".into(),
            price: Some(79.0),
            amount: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        r.id,
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
    );
    let cancel = requests(&mut e).remove(0);
    assert!(cancel.url.ends_with("cancel_orders"));
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","side":"Sell","qty":"3","dealAmount":"2","averagePrice":"100"}}"#,
    );
    e.drain_actions().for_each(drop);
    reply(
        &mut e,
        cancel.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    let detail = requests(&mut e).remove(0);
    assert_eq!(detail.method, safe_cex_engine::Method::Get);
    assert!(detail.url.contains("get_order_detail?orderId=one"));
    reply(
        &mut e,
        detail.id,
        r#"{"code":0,"data":{"symbol":"BTCUSDT","orderId":"one","qty":"3","tradeQty":"2","status":"CANCELED","orderType":"LIMIT","side":"SELL"}}"#,
    );
    let replacement = requests(&mut e).remove(0);
    assert!(replacement.url.ends_with("place_order"));
    let b = replacement.body.as_deref().unwrap();
    assert!(b.contains(r#""qty":"1""#));
    assert!(b.contains(r#""tpPrice":"79""#));
    assert!(b.contains(r#""slPrice":"110""#));
    assert!(!b.contains(r#""clientId":"web""#));
}
#[test]
fn incomplete_cancel_ack_never_places_replacement() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 14,
            ids: vec!["one__tp".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(
        &mut e,
        r.id,
        r#"{"code":0,"data":{"successList":[],"failureList":[]}}"#,
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandDone { result: Err(_), .. })
    )));
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}
#[test]
fn cancelling_both_attached_limit_legs_needs_no_recreation() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","tpOrderType":"LIMIT","tpOrderPrice":"79"}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 15,
            ids: vec!["one__tp".into(), "one__sl".into()],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("cancel_orders"));
    reply(
        &mut e,
        r.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    assert_eq!(done(&mut e, 15).unwrap(), "[]");
}

#[test]
fn malformed_or_stale_final_fill_totals_never_create_replacements() {
    for detail in [
        r#"{"symbol":"BTCUSDT","orderId":"one","qty":"3","status":"CANCELED"}"#,
        r#"{"symbol":"BTCUSDT","orderId":"one","qty":"3","tradeQty":"bad","status":"CANCELED"}"#,
        r#"{"symbol":"BTCUSDT","orderId":"one","qty":"3","tradeQty":"0","status":"CANCELED"}"#,
        r#"{"symbol":"BTCUSDT","orderId":"one","qty":"3","tradeQty":"1","status":"FILLED"}"#,
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::CancelOrders {
                command: 16,
                ids: vec!["one__tp".into()],
            }),
        );
        let r = requests(&mut e).remove(0);
        reply(
            &mut e,
            r.id,
            r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
        );
        let r = requests(&mut e).remove(0);
        reply(&mut e, r.id, &format!(r#"{{"code":0,"data":{detail}}}"#));
        assert!(!e.drain_actions().any(|a| matches!(a, Action::Http(_))));
    }
}
fn orders_refresh(e: &mut TestEngine, s: safe_cex_engine::SocketId) {
    event(
        e,
        s,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"1"}}"#,
    );
    let timer = e
        .drain_actions()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if at_ms == 1100.0 { Some(timer) } else { None }
            } else {
                None
            }
        })
        .next_back()
        .unwrap();
    e.handle(1100.0, Event::TimerFired { timer });
    for r in requests(e) {
        reply(
            e,
            r.id,
            if r.url.contains("/tpsl/") {
                TPSL
            } else {
                ORDERS
            },
        );
    }
    e.drain_actions().for_each(drop);
}
#[test]
fn reconciliations_preserve_inflight_creating_and_canceling_rows() {
    let (mut e, s) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 17,
            orders: vec![order("")],
        }),
    );
    let write = requests(&mut e).remove(0);
    orders_refresh(&mut e, s);
    assert!(
        e.store()
            .orders()
            .iter()
            .any(|o| o.status == OrderStatus::Creating)
    );
    e.handle(
        1101.0,
        Event::HttpFailed {
            request: write.id,
            error: "response lost",
        },
    );
    e.drain_actions().for_each(drop);
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.status == OrderStatus::Creating)
    );
    e.handle(
        1200.0,
        Event::Command(Command::CancelOrders {
            command: 18,
            ids: vec!["one".into()],
        }),
    );
    let write = requests(&mut e).remove(0);
    orders_refresh(&mut e, s);
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceling
    );
    e.handle(
        1300.0,
        Event::HttpFailed {
            request: write.id,
            error: "response lost",
        },
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}
