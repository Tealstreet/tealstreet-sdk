//! Outbound payloads follow bitunix.com/api-docs/futures/trade and tp_sl.
mod support;
use safe_cex_engine::model::{OrderStatus, PlaceOrderOpts};
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

fn assert_unknown_notice(actions: &[Action], command: u32, placed: &[&str]) {
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        failures.len(),
        1,
        "one journal terminal reaches host telemetry"
    );
    let error = failures[0];
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(
        error.message.contains(ENGINE_UNKNOWN_TEXT),
        "{}",
        error.message
    );
    let ids = error
        .partial
        .as_ref()
        .map(|p| p.order_ids.iter().map(String::as_str).collect::<Vec<_>>())
        .unwrap_or_default();
    assert_eq!(ids, placed);
    assert!(!actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command)
    ));
}

fn after_failure_actions(e: &mut TestEngine) -> Vec<Action> {
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect()
}

fn after_failure_writes(actions: &[Action]) -> Vec<&safe_cex_engine::HttpRequest> {
    actions
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.method == safe_cex_engine::Method::Post => {
                Some(request)
            }
            _ => None,
        })
        .collect()
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:2570-2608
fn unknown_chunk_stops(failure: &str) {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 990,
            orders: (0..6)
                .map(|index| {
                    let mut row = order("");
                    row.client_order_id = Some(format!("after-failure-{index}"));
                    row
                })
                .collect(),
        }),
    );
    let actions = after_failure_actions(&mut e);
    let writes = after_failure_writes(&actions);
    assert_eq!(writes.len(), 1);
    let first = writes[0];
    assert!(first.url.ends_with("/batch_order"));
    let first_id = first.id;
    if failure == "transport" {
        e.handle(
            0.0,
            Event::HttpFailed {
                request: first_id,
                error: "lost batch response",
            },
        );
    } else {
        reply(&mut e, first_id, failure);
    }
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    assert_unknown_notice(&actions, 990, &[]);
}

#[test]
fn unknown_whole_batch_native_error_stops_without_resending() {
    unknown_chunk_stops(r#"{"code":20003,"msg":"Insufficient balance"}"#);
}

#[test]
fn unknown_batch_transport_loss_stops_without_resending() {
    unknown_chunk_stops("transport");
}

#[test]
fn unknown_batch_malformed_ack_stops_without_resending() {
    unknown_chunk_stops(r#"{"code":0,"data":{"successList":[{}]}}"#);
}

#[test]
fn after_failure_single_row_chunk_stops_later_symbols() {
    let (mut e, _) = startup();
    let mut markets = e.store().markets().to_vec();
    let mut other = markets[0].clone();
    other.id = "ETHUSDT".into();
    other.symbol = "ETHUSDT".into();
    markets.push(other);
    e.store_mut().set_markets(markets);
    let mut second = order("");
    second.symbol = "ETHUSDT".into();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 991,
            orders: vec![order(""), second],
        }),
    );
    let actions = after_failure_actions(&mut e);
    let writes = after_failure_writes(&actions);
    assert_eq!(writes.len(), 1);
    assert!(writes[0].url.ends_with("/place_order"));
    reply(
        &mut e,
        writes[0].id,
        r#"{"code":20003,"msg":"Insufficient balance"}"#,
    );
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 991, .. })
    )));
}

#[test]
fn after_failure_tpsl_stops_remaining_protection_and_ordinary_rows() {
    for amount in [0.0, 0.001] {
        let (mut e, _) = startup();
        let mut protection = order("");
        protection.kind = safe_cex_engine::model::OrderType::StopLoss;
        protection.price = Some(110000.0);
        protection.amount = amount;
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 992,
                orders: vec![protection, order("")],
            }),
        );
        let mut actions = after_failure_actions(&mut e);
        let mut reads = 0;
        while after_failure_writes(&actions).is_empty() {
            let pending: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Http(request) => Some((request.id, request.url.clone())),
                    _ => None,
                })
                .collect();
            assert!(!pending.is_empty(), "TP-SL preparation has pending reads");
            for (id, url) in pending {
                reads += 1;
                assert!(reads <= 6);
                let body = if url.contains("get_pending_positions") {
                    POSITIONS
                } else if url.contains("get_pending_orders") {
                    ORDERS
                } else {
                    TPSL
                };
                reply(&mut e, id, body);
            }
            actions = after_failure_actions(&mut e);
        }
        let writes = after_failure_writes(&actions);
        assert_eq!(writes.len(), 1);
        assert!(writes[0].url.contains("/tpsl/"));
        reply(
            &mut e,
            writes[0].id,
            r#"{"code":20003,"msg":"protection refused"}"#,
        );
        let actions = after_failure_actions(&mut e);
        assert!(after_failure_writes(&actions).is_empty());
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 992, .. })
        )));
    }
}

#[test]
fn after_failure_cancel_request_loss_stops_kept_parent_rebuild() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 993,
            ids: vec!["one__tp".into()],
        }),
    );
    let actions = after_failure_actions(&mut e);
    let writes = after_failure_writes(&actions);
    assert_eq!(writes.len(), 1);
    e.handle(
        2.0,
        Event::HttpFailed {
            request: writes[0].id,
            error: "cancel response lost",
        },
    );
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 993, .. })
    )));
}

#[test]
fn after_failure_resolved_cancel_refusal_still_rebuilds_parent_like_ts() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 997,
            ids: vec!["one__tp".into()],
        }),
    );
    let actions = after_failure_actions(&mut e);
    let writes = after_failure_writes(&actions);
    assert_eq!(writes.len(), 1);
    reply(
        &mut e,
        writes[0].id,
        r#"{"code":20003,"msg":"cancellation refused"}"#,
    );
    let actions = after_failure_actions(&mut e);
    let writes = after_failure_writes(&actions);
    assert_eq!(
        writes.len(),
        1,
        "TS rebuilds after a resolved cancellation response"
    );
    assert!(writes[0].url.ends_with("/place_order"));
    reply(
        &mut e,
        writes[0].id,
        r#"{"code":0,"data":{"orderId":"rebuilt"}}"#,
    );
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 997, .. })
    )));
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:2061-2065
fn unknown_unsupported_amend_http_503_stops_before_fallback() {
    for body in [
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
        r#"{"msg":"The order does not support this operation"}"#,
        r#"{"code":{},"msg":"The order does not support this operation"}"#,
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::AmendOrder {
                command: 998,
                id: "one__tp".into(),
                price: Some(79.0),
                amount: None,
            }),
        );
        let actions = after_failure_actions(&mut e);
        let writes = after_failure_writes(&actions);
        assert_eq!(writes.len(), 1);
        e.handle(
            0.0,
            Event::HttpResponse {
                request: writes[0].id,
                response: safe_cex_engine::Response {
                    status: 503,
                    headers: "",
                    body,
                },
            },
        );
        let actions = after_failure_actions(&mut e);
        assert!(after_failure_writes(&actions).is_empty());
        assert_unknown_notice(&actions, 998, &[]);
    }
}

#[test]
fn after_failure_fallback_places_without_an_additional_detail_gate() {
    let (mut e, _) = startup();
    let replacement = fallback_to_replacement(&mut e, 999);
    assert_eq!(
        replacement.method,
        safe_cex_engine::Method::Post,
        "TS goes directly from cancellation to replacement"
    );
    assert!(replacement.url.ends_with("/place_order"));
    reply(
        &mut e,
        replacement.id,
        r#"{"code":20003,"msg":"replacement refused"}"#,
    );
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 999, .. })
    )));
}

#[test]
fn after_failure_unsupported_amend_enters_fallback_but_cancel_refusal_stops() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 994,
            id: "one__tp".into(),
            price: Some(79.0),
            amount: None,
        }),
    );
    let actions = after_failure_actions(&mut e);
    let writes = after_failure_writes(&actions);
    assert_eq!(writes.len(), 1);
    reply(
        &mut e,
        writes[0].id,
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
    );
    let actions = after_failure_actions(&mut e);
    let writes = after_failure_writes(&actions);
    assert_eq!(
        writes.len(),
        1,
        "unsupported attached edit enters the TS fallback"
    );
    assert!(writes[0].url.ends_with("/cancel_orders"));
    reply(
        &mut e,
        writes[0].id,
        r#"{"code":10006,"msg":"request too frequently"}"#,
    );
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    assert!(actions.iter().any(|a| matches!(
        a,
        Action::Notice(Notice::CommandFailed { command: 994, .. })
    )));
}

#[test]
fn cancellation_missing_from_store_errors_before_any_write_even_in_a_mixed_batch() {
    for mixed in [false, true] {
        let (mut e, _) = startup();
        assert!(e.store().loaded().orders);
        assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
        safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .for_each(drop);
        e.handle(
            2000.0,
            Event::Command(Command::CancelOrders {
                command: 991,
                ids: if mixed {
                    vec!["one".into(), "missing-native-order".into()]
                } else {
                    vec!["missing-native-order".into()]
                },
            }),
        );
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_))),
            "cancellation wrote before rejecting an unknown ID: {actions:?}"
        );
        assert!(
            actions.iter().any(|action| matches!(
                action,
                Action::Notice(Notice::CommandFailed { command: 991, error })
                    if error.legacy_text().contains("missing-native-order") && error.legacy_text().contains("not found")
            )),
            "missing cancellation error: {actions:?}"
        );
        assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
    }
}

#[test]
fn an_empty_cancellation_list_remains_a_successful_noop() {
    let (mut e, _) = startup();
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    e.handle(
        2000.0,
        Event::Command(Command::CancelOrders {
            command: 992,
            ids: vec![],
        }),
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(_)))
    );
    assert!(actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: 992, result })
            if result == "[]"
    )));
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}

#[test]
fn nonmultiple_splits_conserve_native_quantity_and_honor_maximum() {
    for (amount, maximum, step, expected) in [
        (250.0, 100.0, 0.001, vec![100.0, 100.0, 50.0]),
        (250.0, 100.0, 1.0, vec![100.0, 100.0, 50.0]),
        (1.0, 0.3, 0.1, vec![0.3, 0.3, 0.3, 0.1]),
        (0.75, 0.5, 0.25, vec![0.5, 0.25]),
    ] {
        let (mut e, _) = startup();
        let mut markets = e.store().markets().to_vec();
        markets[0].precision.amount = step;
        markets[0].limits.amount.max = maximum;
        markets[0].limits.amount.min = step;
        e.store_mut().set_markets(markets);
        let mut opts = order("");
        opts.amount = amount;
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 901,
                orders: vec![opts],
            }),
        );
        let writes = requests(&mut e);
        assert_eq!(writes.len(), 1);
        let body: serde_json::Value =
            serde_json::from_str(writes[0].body.as_deref().unwrap()).unwrap();
        let rows = body["orderList"].as_array().unwrap();
        let quantities: Vec<f64> = rows
            .iter()
            .map(|row| row["qty"].as_str().unwrap().parse().unwrap())
            .collect();
        assert_eq!(quantities, expected);
        assert!(quantities.iter().all(|qty| *qty <= maximum));
        assert_eq!(
            quantities
                .iter()
                .map(|qty| (qty / step).round() as u64)
                .sum::<u64>(),
            (amount / step).round() as u64
        );
    }
}

#[test]
fn invalid_later_split_prevents_every_earlier_plain_or_protection_write() {
    for protection in [false, true] {
        let (mut e, _) = startup();
        let mut markets = e.store().markets().to_vec();
        markets[0].precision.amount = 0.25;
        markets[0].limits.amount.min = 0.5;
        markets[0].limits.amount.max = 1.0;
        e.store_mut().set_markets(markets);
        let mut first = order("");
        first.amount = 0.5;
        if protection {
            first.kind = safe_cex_engine::model::OrderType::StopLoss;
        }
        let mut last = order("");
        last.amount = 1.25;
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 903,
                orders: vec![first, last],
            }),
        );
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        assert!(
            !actions
                .iter()
                .any(|action| matches!(action, Action::Http(_)))
        );
        assert!(actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandFailed { command: 903, .. })
        )));
        assert!(
            !e.store()
                .orders()
                .iter()
                .any(|row| row.status == OrderStatus::Creating)
        );
    }
}
fn order(extra: &str) -> PlaceOrderOpts {
    serde_json::from_str(&format!(r#"{{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90500.04,"amount":0.0021{extra}}}"#)).unwrap()
}
fn done(e: &mut TestEngine, command: u32) -> Result<String, String> {
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandDone { command: c, result }) if c == command => {
                Some(Ok(result))
            }
            Action::Notice(Notice::CommandFailed { command: c, error }) if c == command => {
                Some(Err(error.legacy_text()))
            }
            _ => None,
        })
        .expect("command completion")
}
#[test]
fn unsupported_stp_on_final_batch_member_prevents_all_io() {
    for mode in ["EXPIRE_TAKER", "EXPIRE_MAKER", "EXPIRE_BOTH"] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 1,
                orders: vec![order(""), order(&format!(r#", "stpMode":"{mode}""#))],
            }),
        );
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandFailed { command: 1, error })
                if error.legacy_text().contains("self-trade prevention")
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
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 1, .. })))
        );
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
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
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
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    // Only the uncertain seal's order census follows; no further write.
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:1663-1729
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
        r#"{"code":10006,"msg":"request too frequently"}"#,
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
            side: None,
        }),
    );
    let r = requests(&mut e).remove(0);
    reply(&mut e, r.id, r#"{"code":20003,"msg":"rejected"}"#);
    assert!(done(&mut e, 7).is_err());
    assert_eq!(e.store().setting("BTCUSDT").unwrap().leverage, 20.0);
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:1663-1729
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
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    // Only the uncertain seal's order census follows; no further write.
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
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
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
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
    // Live: a separate SL on the position was copied here and a second SL appeared.
    assert!(!b.contains("slPrice"), "{b}");
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
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
    assert!(
        !e.store()
            .orders()
            .iter()
            .any(|o| o.status == OrderStatus::Creating)
    );
}
#[test]
fn full_tpsl_without_credentials_completes_with_error() {
    let mut e = safe_cex_engine::test_support::seeded_engine(
        safe_cex_venue_bitunix::BitunixFutures::new(Default::default()),
    );
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
fn fallback_uses_the_ts_prepared_quantity_without_a_detail_read() {
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
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    reply(
        &mut e,
        cancel.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    let replacement = requests(&mut e).remove(0);
    assert!(replacement.url.ends_with("place_order"));
    let b = replacement.body.as_deref().unwrap();
    assert!(b.contains(r#""qty":"3""#));
    assert!(b.contains(r#""tpPrice":"79""#));
    assert!(b.contains(r#""slPrice":"110""#));
    assert!(!b.contains(r#""clientId":"web""#));
}
#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:1663-1729
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
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .collect();
    // Only the uncertain seal's order census follows; no further write.
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Http(r) if r.method != safe_cex_engine::Method::Get))
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
    );
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
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
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
        assert!(
            !safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::Http(_)))
        );
    }
}
fn orders_refresh(
    e: &mut TestEngine,
    s: safe_cex_engine::SocketId,
    now: f64,
    pending: Option<(safe_cex_engine::TimerId, f64)>,
) {
    e.handle(
        now,
        Event::WsMessage {
            socket: s,
            text: r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","dealAmount":"1"}}"#,
        },
    );
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect();
    let (timer, at) = actions
        .into_iter()
        .filter_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                if at_ms == now + 1000.0 {
                    Some((timer, at_ms))
                } else {
                    None
                }
            } else {
                None
            }
        })
        .chain(pending)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .expect("new or previously armed order repair");
    // A command's earlier repair must not be postponed by this notification.
    e.handle(at, Event::TimerFired { timer });
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
    safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .for_each(drop);
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
    orders_refresh(&mut e, s, 100.0, None);
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
    let pending = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == 2101.0 => Some((timer, at_ms)),
            _ => None,
        });
    assert!(pending.is_some(), "failed write schedules an order repair");
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
    orders_refresh(&mut e, s, 1300.0, pending);
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceling
    );
    e.handle(
        2200.0,
        Event::HttpFailed {
            request: write.id,
            error: "response lost",
        },
    );
    assert_eq!(e.store().order("one").unwrap().status, OrderStatus::Open);
}

#[test]
fn app_stop_payload_with_trigger_equal_to_price_places_protection() {
    for (kind, field) in [
        (safe_cex_engine::model::OrderType::StopLoss, "slPrice"),
        (safe_cex_engine::model::OrderType::TakeProfit, "tpPrice"),
    ] {
        let (mut e, _) = startup();
        let mut o = order(r#","triggerPrice":100500"#);
        o.kind = kind;
        o.price = Some(100500.0);
        o.amount = 0.01;
        e.handle(
            1.0,
            Event::Command(Command::PlaceOrders {
                command: 940,
                orders: vec![o],
            }),
        );
        let write = requests(&mut e)
            .into_iter()
            .find(|r| r.url.ends_with("/api/v1/futures/tpsl/place_order"))
            .expect("protective trigger equal to price is placed from price");
        let body: serde_json::Value = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
        assert_eq!(body[field], "100500");
    }
    let (mut e, _) = startup();
    let mut o = order(r#","triggerPrice":90000"#);
    o.kind = safe_cex_engine::model::OrderType::StopLoss;
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 941,
            orders: vec![o],
        }),
    );
    assert!(
        done(&mut e, 941).is_err(),
        "a distinct trigger stays rejected"
    );
}
#[test]
fn a_stale_tpsl_does_not_abort_cancel_all() {
    let (mut e, _) = startup();
    e.handle(
        1.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 8,
            symbol: "BTCUSDT".into(),
        }),
    );
    let mut sent = requests(&mut e);
    let mut failed = false;
    let mut stale_answered = false;
    let mut swept = false;
    while let Some(r) = sent.pop() {
        let body = if r.url.contains("tpsl/cancel_order") {
            stale_answered = true;
            r#"{"code":20007,"msg":"Order not found. It may have been filled or canceled"}"#
                .to_owned()
        } else if r.url.contains("cancel_all_orders") {
            swept = true;
            r#"{"code":0,"data":null}"#.to_owned()
        } else {
            // Every remaining regular order is acknowledged by its ID.
            let sent: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
            let rows: Vec<_> = sent["orderList"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| serde_json::json!({"orderId": o["orderId"]}))
                .collect();
            serde_json::json!({"code":0,"data":{"successList":rows,"failureList":[]}}).to_string()
        };
        reply(&mut e, r.id, &body);
        let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(&mut e)
            .into_iter()
            .collect();
        failed |= actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 8, .. })));
        sent.extend(actions.into_iter().filter_map(|a| match a {
            Action::Http(r) if r.method != safe_cex_engine::Method::Get => Some(r),
            _ => None,
        }));
    }
    assert!(stale_answered);
    assert!(swept, "the by-symbol cancel follows the row cancels");
    assert!(!failed);
}

#[test]
fn symbol_cancel_hits_the_venue_with_no_order_in_the_store() {
    for answer in [r#"{"code":0}"#, r#"{"code":0,"data":{"successList":[]}}"#] {
        let (mut e, _) = startup();
        e.store_mut().set_orders(Vec::new());
        e.handle(
            1.0,
            Event::Command(Command::CancelSymbolOrders {
                command: 8,
                symbol: "BTCUSDT".into(),
            }),
        );
        let sent = requests(&mut e);
        let sweep = sent
            .iter()
            .find(|r| r.url.contains("cancel_all_orders"))
            .expect("swept");
        assert_eq!(sweep.body.as_deref(), Some(r#"{"symbol":"BTCUSDT"}"#));
        reply(&mut e, sweep.id, answer);
        assert!(
            safe_cex_engine::test_support::prepared_now(&mut e)
                .into_iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 8, .. })))
        );
    }
}

// Live: this body came back while two of the batch's orders rested on the venue.
#[test]
fn a_refused_batch_stays_unknown_because_its_orders_may_rest() {
    let (mut e, _) = startup();
    let order = |client: &str| {
        let mut o: safe_cex_engine::model::PlaceOrderOpts =
            serde_json::from_value(serde_json::json!({
                "symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":90000.0
            }))
            .unwrap();
        o.client_order_id = Some(client.into());
        o
    };
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 77,
            orders: vec![order("a"), order("b")],
        }),
    );
    let r = requests(&mut e).remove(0);
    assert!(r.url.ends_with("batch_order"));
    e.handle(
        2.0,
        Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"code":20003,"data":null,"msg":"Insufficient balance"}"#,
            },
        },
    );
    let outcome = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error.outcome),
            _ => None,
        });
    assert_eq!(outcome, Some(Some(safe_cex_engine::Outcome::Unknown)));
}

#[test]
fn a_duplicate_client_id_stays_unknown() {
    let (mut e, _) = startup();
    let mut order: safe_cex_engine::model::PlaceOrderOpts =
        serde_json::from_value(serde_json::json!({
            "symbol":"BTCUSDT","type":"limit","side":"buy","amount":0.001,"price":9000
        }))
        .unwrap();
    order.client_order_id = Some("a".into());
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 78,
            orders: vec![order],
        }),
    );
    let r = requests(&mut e).remove(0);
    e.handle(
        2.0,
        Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status: 200,
                headers: "",
                body: r#"{"code":30042,"data":null,"msg":"Client ID duplicate"}"#,
            },
        },
    );
    let outcome = safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error.outcome),
            _ => None,
        });
    assert_eq!(outcome, Some(Some(safe_cex_engine::Outcome::Unknown)));
}

/// The command's failure text, and whether any write went out, from one batch of actions.
fn failed_without_write(e: &mut TestEngine, command: u32) -> (String, bool) {
    let actions: Vec<_> = safe_cex_engine::test_support::prepared_now(e)
        .into_iter()
        .collect();
    let wrote = actions
        .iter()
        .any(|a| matches!(a, Action::Http(r) if r.method == safe_cex_engine::Method::Post));
    let error = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
                Some(error.legacy_text())
            }
            _ => None,
        })
        .expect("command failure");
    (error, wrote)
}
fn fallback_to_replacement(e: &mut TestEngine, command: u32) -> safe_cex_engine::HttpRequest {
    e.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command,
            id: "one__tp".into(),
            price: Some(79.0),
            amount: None,
        }),
    );
    let r = requests(e).remove(0);
    reply(
        e,
        r.id,
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
    );
    let cancel = requests(e).remove(0);
    reply(
        e,
        cancel.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    requests(e).remove(0)
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:1803-1815
fn unknown_fallback_cancel_without_proof_stops_before_replacement() {
    for cancel_body in [
        r#"{"code":0}"#,
        r#"{"code":0,"data":{"successList":[],"failureList":[]}}"#,
        r#"{"code":0,"data":{"successList":[],"failureList":[{"orderId":"one","errorCode":20003,"errorMsg":"row refused"}]}}"#,
    ] {
        let (mut e, _) = startup();
        e.handle(
            1.0,
            Event::Command(Command::AmendOrder {
                command: 13,
                id: "one__tp".into(),
                price: Some(79.0),
                amount: None,
            }),
        );
        let amend = requests(&mut e).remove(0);
        reply(
            &mut e,
            amend.id,
            r#"{"code":20003,"msg":"The order does not support this operation"}"#,
        );
        let cancel = requests(&mut e).remove(0);
        reply(&mut e, cancel.id, cancel_body);
        let actions = after_failure_actions(&mut e);
        assert!(after_failure_writes(&actions).is_empty());
        assert_unknown_notice(&actions, 13, &[]);
    }
}

// Legacy TS uses the parent order's reduceOnly/default-false value when its
// attached edit falls back, including orders first observed on the private WS.
#[test]
fn after_failure_ws_only_attached_edit_uses_the_ts_fallback() {
    let (mut e, s) = startup();
    event(
        &mut e,
        s,
        r#"{"ch":"order","data":{"event":"CREATE","symbol":"BTCUSDT","orderId":"stray","side":"Buy","type":"LIMIT","qty":"1","price":"90","dealAmount":"0","tpPrice":"120"}}"#,
    );
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    e.handle(
        2.0,
        Event::Command(Command::AmendOrder {
            command: 15,
            id: "stray".into(),
            price: Some(91.0),
            amount: None,
        }),
    );
    let modify = requests(&mut e).remove(0);
    assert!(
        modify.url.ends_with("/trade/modify_order"),
        "{}",
        modify.url
    );
    reply(
        &mut e,
        modify.id,
        r#"{"code":0,"data":{"orderId":"stray"}}"#,
    );
    safe_cex_engine::test_support::prepared_now(&mut e)
        .into_iter()
        .for_each(drop);
    assert!(
        e.store().order("stray__tp").is_some(),
        "the attached take profit"
    );

    e.handle(
        3.0,
        Event::Command(Command::AmendOrder {
            command: 16,
            id: "stray__tp".into(),
            price: Some(121.0),
            amount: None,
        }),
    );
    let drag = requests(&mut e).remove(0);
    assert!(drag.url.ends_with("/trade/modify_order"), "{}", drag.url);
    reply(
        &mut e,
        drag.id,
        r#"{"code":20003,"msg":"The order does not support this operation"}"#,
    );
    let cancel = requests(&mut e)
        .into_iter()
        .find(|request| request.url.ends_with("/cancel_orders"))
        .expect("TS fallback does not gate on reduce-only provenance");
    reply(
        &mut e,
        cancel.id,
        r#"{"code":10006,"msg":"cancel refused"}"#,
    );
    let (_, wrote) = failed_without_write(&mut e, 16);
    assert!(!wrote, "a failed fallback cancellation still stops");
}

fn parity_next_write(e: &mut TestEngine) -> safe_cex_engine::HttpRequest {
    for _ in 0..8 {
        let actions = after_failure_actions(e);
        if let Some(write) = after_failure_writes(&actions).first() {
            return (*write).clone();
        }
        let reads: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) => Some(r.clone()),
                _ => None,
            })
            .collect();
        assert!(!reads.is_empty(), "preparation has a pending read");
        for read in reads {
            let body = if read.url.contains("get_pending_positions") {
                POSITIONS
            } else if read.url.contains("get_pending_orders") {
                ORDERS
            } else {
                TPSL
            };
            reply(e, read.id, body);
        }
    }
    panic!("preparation did not release a write");
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitunix | serial-batch-and-five-row-order-chunks
// TS packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:2278-2288,2541-2574.
fn parity_sixth_off_tick_attached_trigger_is_sent_after_five_accepted_rows() {
    let (mut e, _) = startup();
    let rows = (0..6)
        .map(|i| {
            let mut row = order("");
            row.take_profit = Some(if i == 5 { 110000.04 } else { 110000.0 });
            row
        })
        .collect();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 1100,
            orders: rows,
        }),
    );
    let first = parity_next_write(&mut e);
    assert!(first.url.ends_with("/batch_order"));
    let body: serde_json::Value = serde_json::from_str(first.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["orderList"].as_array().unwrap().len(), 5);
    reply(
        &mut e,
        first.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"1"},{"orderId":"2"},{"orderId":"3"},{"orderId":"4"},{"orderId":"5"}],"failureList":[]}}"#,
    );
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 1000.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(1000.0, Event::TimerFired { timer });
    let last = parity_next_write(&mut e);
    assert!(last.url.ends_with("/place_order"));
    let body: serde_json::Value = serde_json::from_str(last.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["tpPrice"], "110000.04");
    reply(&mut e, last.id, r#"{"code":0,"data":{"orderId":"6"}}"#);
    assert_eq!(done(&mut e, 1100).unwrap(), r#"["1","2","3","4","5","6"]"#);
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitunix | standalone/full-position TP-SL
// TS packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:2432-2456,2492-2515.
fn parity_standalone_tpsl_sends_below_minimum_second_row() {
    let (mut e, _) = startup();
    let mut markets = e.store().markets().to_vec();
    markets[0].limits.amount.min = 0.01;
    e.store_mut().set_markets(markets);
    let rows = [0.01, 0.001].map(|amount| {
        let mut row = order("");
        row.kind = safe_cex_engine::model::OrderType::StopLoss;
        row.price = Some(110000.04);
        row.amount = amount;
        row
    });
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 1101,
            orders: rows.into(),
        }),
    );
    for (qty, id) in [("0.01", "first"), ("0.001", "tail")] {
        let write = parity_next_write(&mut e);
        assert!(write.url.ends_with("/tpsl/place_order"));
        let body: serde_json::Value = serde_json::from_str(write.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["slQty"], qty);
        assert_eq!(body["slPrice"], "110000");
        reply(
            &mut e,
            write.id,
            &format!(r#"{{"code":0,"data":{{"orderId":"{id}"}}}}"#),
        );
    }
    assert!(done(&mut e, 1101).is_ok());
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitunix | cancel-kept-sibling-parent-rebuild
// TS packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:1663-1729,2278-2288.
fn parity_parent_rebuild_sends_preserved_off_tick_sibling_after_cancel() {
    let (mut e, socket) = startup();
    event(
        &mut e,
        socket,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","slPrice":"110.04"}}"#,
    );
    after_failure_actions(&mut e);
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 1102,
            ids: vec!["one__tp".into()],
        }),
    );
    let cancel = parity_next_write(&mut e);
    assert!(cancel.url.ends_with("/cancel_orders"));
    reply(
        &mut e,
        cancel.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one"}],"failureList":[]}}"#,
    );
    let reads = requests(&mut e);
    assert_eq!(reads.len(), 1);
    assert!(reads[0].url.contains("get_order_detail"));
    reply(
        &mut e,
        reads[0].id,
        r#"{"code":0,"data":{"symbol":"BTCUSDT","orderId":"one","qty":"3","tradeQty":"1","status":"CANCELED","orderType":"LIMIT","side":"SELL"}}"#,
    );
    let replacement = parity_next_write(&mut e);
    assert!(replacement.url.ends_with("/place_order"));
    let body: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["qty"], "2");
    assert_eq!(body["slPrice"], "110.04");
    assert!(body.get("tpPrice").is_none());
    reply(
        &mut e,
        replacement.id,
        r#"{"code":0,"data":{"orderId":"rebuilt"}}"#,
    );
    assert!(done(&mut e, 1102).is_ok());
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bitunix | cancel-recreate-edit-fallback
// TS packages/safe-cex/src/__tests__/reference/bitunix/bitunix.exchange.ts:1751-1815,2278-2288.
fn parity_amend_fallback_sends_rounded_payload_directly_after_cancel() {
    let (mut e, socket) = startup();
    event(
        &mut e,
        socket,
        r#"{"ch":"order","data":{"event":"UPDATE","symbol":"BTCUSDT","orderId":"one","slPrice":"110.04"}}"#,
    );
    after_failure_actions(&mut e);
    let replacement = fallback_to_replacement(&mut e, 1103);
    assert!(replacement.url.ends_with("/place_order"));
    let body: serde_json::Value =
        serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["qty"], "3");
    assert_eq!(body["slPrice"], "110");
    assert_eq!(body["tpPrice"], "79");
    reply(
        &mut e,
        replacement.id,
        r#"{"code":0,"data":{"orderId":"replaced"}}"#,
    );
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    let error = actions
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 1103,
                error,
            }) => Some(error),
            _ => None,
        })
        .expect("journal retains the original amend refusal with successful replacement");
    assert_eq!(error.outcome, None);
    assert!(
        error
            .partial
            .as_ref()
            .unwrap()
            .order_ids
            .contains(&"replaced__tp".to_owned())
    );
}

#[test]
// The engine seals a replacement's 503 after the earlier definite amendment refusal.
fn unknown_fallback_replacement_http503_keeps_engine_sealed_wording() {
    let (mut e, _) = startup();
    let replacement = fallback_to_replacement(&mut e, 1107);
    e.handle(
        0.0,
        Event::HttpResponse {
            request: replacement.id,
            response: safe_cex_engine::Response {
                status: 503,
                headers: "",
                body: "{}",
            },
        },
    );
    let actions = after_failure_actions(&mut e);
    assert!(after_failure_writes(&actions).is_empty());
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed {
                command: 1107,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(
        failures.len(),
        1,
        "one journal terminal reaches host telemetry"
    );
    assert_eq!(failures[0].outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(
        failures[0].message,
        "The outcome of this order change is unknown: a later request was not confirmed. Check Open Orders"
    );
}
