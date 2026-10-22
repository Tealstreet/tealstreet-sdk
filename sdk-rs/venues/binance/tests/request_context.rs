//! Native facts enter the shared journal; late replies have no reducer authority.
mod support;
use safe_cex_engine::{Command, Event, Outcome};
use support::{Harness, Options};

fn ready() -> Harness {
    let mut h = Harness::started(Options {
        key: Some("api-key".into()),
        secret: Some("secret".into()),
        ..Options::default()
    });
    h.load_public();
    h
}

#[test]
fn terminal_native_reports_follow_logical_plan_not_callback_order() {
    let mut h = ready();
    h.event(Event::Command(Command::CancelSymbolOrders {
        command: 701,
        symbol: "BTCUSDT".into(),
    }));
    let first = h.take("/fapi/v1/allOpenOrders");
    let second = h.take("/fapi/v1/algoOpenOrders");
    h.reply(
        second.id,
        400,
        "",
        r#"{"code":-1100,"msg":"second native refusal"}"#,
    );
    assert!(h.result(701).is_none());
    h.reply(
        first.id,
        400,
        "",
        r#"{"code":-1102,"msg":"first native refusal"}"#,
    );
    let failure = &h.failures.iter().find(|(id, _)| *id == 701).unwrap().1;
    assert_eq!(failure.outcome, Some(Outcome::Rejected));
    assert!(failure.message.contains("first native refusal"));
    let reports: Vec<_> = h
        .command_errors
        .iter()
        .filter(|(id, _)| *id == 701)
        .map(|(_, text)| text.as_str())
        .collect();
    assert_eq!(
        reports,
        vec!["first native refusal", "second native refusal"]
    );
}

#[test]
fn empty_job_completes_without_declaring_a_write_or_emitting_http() {
    let mut h = ready();
    let before = h.requests.len();
    h.event(Event::Command(Command::PlaceOrders {
        command: 702,
        orders: vec![],
    }));
    assert_eq!(h.result(702), Some(&Ok("[]".into())));
    assert_eq!(h.requests.len(), before);
    assert!(!h.failures.iter().any(|(id, _)| *id == 702));
}

fn put_order(h: &mut Harness) {
    use safe_cex_engine::model::*;
    h.engine.store_mut().upsert_order(Order {
        id: "12".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 100.,
        amount: 1.,
        filled: 0.,
        remaining: 1.,
        reduce_only: false,
        close: None,
        client_order_id: Some("client12".into()),
        time_in_force: None,
        last_update: 1000.,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    });
}

#[test]
fn gateway_codes_never_gain_cancel_or_position_mode_success_authority() {
    for cancel in [true, false] {
        let mut h = ready();
        put_order(&mut h);
        let (command, path, body) = if cancel {
            (
                Command::CancelOrders {
                    command: 703,
                    ids: vec!["12".into()],
                },
                "/fapi/v1/order",
                r#"{"code":-2011,"msg":"unknown order"}"#,
            )
        } else {
            (
                Command::SetPositionMode {
                    command: 703,
                    hedged: true,
                    symbol: None,
                },
                "/fapi/v1/positionSide/dual",
                r#"{"code":-4059,"msg":"already set"}"#,
            )
        };
        h.requests.clear(); // Exclude the unrelated startup GET for position mode.
        h.event(Event::Command(command));
        let write = h.take(path);
        h.reply(write.id, 500, "", body);
        assert_eq!(
            h.failures
                .iter()
                .find(|(id, _)| *id == 703)
                .unwrap()
                .1
                .outcome,
            Some(Outcome::Unknown)
        );
        assert_eq!(
            h.engine.store().order("12").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
        assert!(!h.engine.store().hedged());
    }
}

#[test]
fn duplicate_command_event_cannot_replace_a_live_managed_owner() {
    for nonempty in [false, true] {
        let mut h = ready();
        h.event(Event::Command(Command::CancelSymbolOrders {
            command: 704,
            symbol: "BTCUSDT".into(),
        }));
        let first = h.take("/fapi/v1/allOpenOrders");
        let second = h.take("/fapi/v1/algoOpenOrders");
        let order = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        h.event(Event::Command(Command::PlaceOrders {
            command: 704,
            orders: if nonempty { vec![order] } else { vec![] },
        }));
        assert!(
            h.result(704).is_none(),
            "duplicate must not seal the original"
        );
        assert!(!h.has_request("/fapi/v1/order"));
        h.reply(second.id, 200, "", r#"{"code":200}"#);
        h.reply(first.id, 200, "", r#"{"code":200}"#);
        assert_eq!(h.result(704), Some(&Ok("[]".into())));
        assert!(!h.failures.iter().any(|(id, _)| *id == 704));
        h.event(Event::Command(Command::PlaceOrders {
            command: 704,
            orders: vec![],
        }));
        let support::Harness { done, .. } = &h;
        assert_eq!(
            done.iter().filter(|(id, _)| *id == 704).count(),
            2,
            "completed numeric IDs may get a fresh authority"
        );
        assert!(!h.failures.iter().any(|(id, _)| *id == 704));
    }
}

#[test]
fn unproven_first_batch_stops_later_lots_but_item_refusals_do_not() {
    for transport in [true, false] {
        let mut h = ready();
        let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_str(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
        )
        .unwrap();
        h.event(Event::Command(Command::PlaceOrders {
            command: 705,
            orders: vec![order; 6],
        }));
        let batch = h.take("/fapi/v1/batchOrders");
        if transport {
            h.event(Event::HttpFailed {
                request: batch.id,
                error: "connection lost",
            });
        } else {
            h.reply(
                batch.id,
                500,
                "",
                r#"{"code":-2015,"msg":"authorization failed"}"#,
            );
        }
        assert!(!h.has_request("/fapi/v1/order"));
        assert_eq!(
            h.failures
                .iter()
                .find(|(id, _)| *id == 705)
                .unwrap()
                .1
                .outcome,
            Some(Outcome::Unknown)
        );
    }
}

#[test]
fn expired_native_write_retires_unknown_without_reducing_a_late_ack() {
    let mut h = ready();
    put_order(&mut h);
    let before = h.engine.store().order("12").unwrap().clone();
    h.event(Event::Command(Command::CancelOrders {
        command: 706,
        ids: vec!["12".into()],
    }));
    let write = h.take("/fapi/v1/order");
    h.advance(20_001.);
    let failures: Vec<_> = h.failures.iter().filter(|(id, _)| *id == 706).collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].1.outcome, Some(Outcome::Unknown));
    h.reply(
        write.id,
        200,
        "",
        r#"{"symbol":"BTCUSDT","orderId":12,"status":"CANCELED","origQty":"1","executedQty":"0"}"#,
    );
    assert_eq!(h.engine.store().order("12"), Some(&before));
    assert_eq!(h.failures.iter().filter(|(id, _)| *id == 706).count(), 1);
}

#[test]
fn mismatched_batch_rows_are_unproven_and_never_retained_as_accepted_ids() {
    let mut h = ready();
    let order: safe_cex_engine::model::PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":100}"#,
    )
    .unwrap();
    h.event(Event::Command(Command::PlaceOrders {
        command: 707,
        orders: vec![order; 6],
    }));
    let batch = h.take("/fapi/v1/batchOrders");
    h.reply(
        batch.id,
        200,
        "",
        r#"[{"orderId":1},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5},{"orderId":6}]"#,
    );
    let error = &h.failures.iter().find(|(id, _)| *id == 707).unwrap().1;
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert!(
        error.partial.is_none(),
        "unmatched rows are not accepted identity facts"
    );
    assert!(!h.has_request("/fapi/v1/order"));
}
