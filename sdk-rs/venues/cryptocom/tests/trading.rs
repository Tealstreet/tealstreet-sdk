mod support;
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, StopTriggerType};
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;
fn order() -> PlaceOrderOpts {
    serde_json::from_str(r#"{"symbol":"BTCUSD-PERP","type":"limit","side":"buy","amount":0.0021,"price":90.04,"clientOrderId":"caller-123"}"#).unwrap()
}
fn command(h: &mut Harness, c: Command) {
    h.event(Event::Command(c));
}
fn pace(h: &mut Harness) {
    let i = h
        .actions
        .iter()
        .position(|a| matches!(a, Action::SetTimer { at_ms, .. } if *at_ms == h.now + 20.0))
        .unwrap();
    let Action::SetTimer { timer, at_ms } = h.actions.remove(i) else {
        unreachable!()
    };
    h.now = at_ms;
    h.event(Event::TimerFired { timer });
}
#[test]
fn caller_id_and_precision_are_sent_verbatim() {
    let mut h = Harness::new();
    h.load();
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order()],
        },
    );
    let r = h.http("private/create-order");
    let body = r.body.as_deref().unwrap();
    assert!(body.contains(r#""client_oid":"caller-123""#));
    assert!(body.contains(r#""quantity":"0.0021""#));
    assert!(body.contains(r#""price":"90.04""#));
    h.respond(&r, r#"{"code":0,"result":{"order_id":9007199254740993}}"#);
    pace(&mut h);
    assert!(h.actions.iter().any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 1, result: Ok(s) }) if s == r#"["9007199254740993"]"#)));
}
#[test]
fn invalid_or_duplicate_caller_ids_reject_entire_batch_before_writes() {
    for invalid in ["", "caller-123", "abcdefghijklmnopqrstuvwxyz12345678901"] {
        let mut h = Harness::new();
        h.load();
        let mut second = order();
        second.client_order_id = Some(invalid.into());
        command(
            &mut h,
            Command::PlaceOrders {
                command: 1,
                orders: vec![order(), second],
            },
        );
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
        assert!(h.actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 1,
                result: Err(_)
            })
        )));
    }
}
#[test]
fn reduce_only_market_sell_uses_quantity_and_advanced_stop_uses_reference_price() {
    let mut h = Harness::new();
    h.load();
    let mut opts = order();
    opts.kind = OrderType::Market;
    opts.side = OrderSide::Sell;
    opts.reduce_only = Some(true);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        },
    );
    let r = h.http("private/create-order");
    let body = r.body.as_deref().unwrap();
    assert!(body.contains("REDUCE_ONLY"));
    assert!(!body.contains("notional"));
    assert!(!body.contains("time_in_force"));
    h.respond(&r, r#"{"code":0,"result":{"order_id":"1"}}"#);
    pace(&mut h);
    let mut opts = order();
    opts.client_order_id = None;
    opts.kind = OrderType::StopLoss;
    opts.trigger_price = Some(80.0);
    opts.sl_trigger_type = Some(StopTriggerType::IndexPrice);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 2,
            orders: vec![opts],
        },
    );
    let r = h.http("advanced/create-order");
    let body = r.body.unwrap();
    assert!(body.contains(r#""ref_price":"80""#));
    assert!(body.contains("INDEX_PRICE"));
    assert!(!body.contains(r#""price":"#));
}
#[test]
fn ambiguous_write_failure_is_not_retried_and_batch_continues() {
    let mut h = Harness::new();
    h.load();
    let mut second = order();
    second.client_order_id = Some("second".into());
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![order(), second],
        },
    );
    let r = h.http("private/create-order");
    h.event(Event::HttpFailed {
        request: r.id,
        error: "response lost after acceptance",
    });
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    pace(&mut h);
    assert!(
        h.http("private/create-order")
            .body
            .unwrap()
            .contains("second")
    );
}
#[test]
fn native_amend_backfills_quantity_and_never_rewrites_client_id() {
    let mut h = Harness::new();
    h.load();
    h.ws("user.order", ORDER);
    command(
        &mut h,
        Command::AmendOrder {
            command: 1,
            id: "9007199254740993".into(),
            price: Some(95.0),
            amount: None,
        },
    );
    let r = h.http("private/amend-order");
    let body = r.body.unwrap();
    assert!(body.contains(r#""new_quantity":"1""#));
    assert!(body.contains(r#""new_price":"95""#));
    assert!(!body.contains("client_oid"));
}
#[test]
fn leverage_uses_account_identity_and_global_numeric_leverage() {
    let mut h = Harness::new();
    h.load();
    command(
        &mut h,
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSD-PERP".into(),
            leverage: 20.0,
        },
    );
    h.reply(
        "private/user-balance",
        r#"{"code":0,"result":{"data":[{"account_id":"account-123"}]}}"#,
    );
    pace(&mut h);
    let r = h.http("private/change-account-leverage");
    let body = r.body.as_deref().unwrap();
    assert!(body.contains(r#""account_id":"account-123""#));
    assert!(body.contains(r#""leverage":20"#));
    assert!(!body.contains("instrument_name"));
    h.respond(&r, r#"{"code":0}"#);
    pace(&mut h);
    assert_eq!(
        h.engine.store().setting("BTCUSD-PERP").unwrap().leverage,
        20.0
    );
}
#[test]
fn attached_brackets_and_margin_switching_fail_before_mutations() {
    let mut h = Harness::new();
    h.load();
    let mut opts = order();
    opts.client_order_id = None;
    opts.stop_loss = Some(80.0);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        },
    );
    command(
        &mut h,
        Command::SetMarginType {
            command: 2,
            symbol: "BTCUSD-PERP".into(),
            isolated: true,
        },
    );
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
}

#[test]
fn post_only_uses_execution_instructions_and_can_combine_reduce_only() {
    let mut h = Harness::new();
    h.load();
    let mut opts = order();
    opts.time_in_force = Some(safe_cex_engine::model::TimeInForce::PostOnly);
    opts.reduce_only = Some(true);
    command(
        &mut h,
        Command::PlaceOrders {
            command: 1,
            orders: vec![opts],
        },
    );
    let body = h.http("private/create-order").body.unwrap();
    assert!(body.contains(r#""exec_inst":["REDUCE_ONLY","POST_ONLY"]"#));
    assert!(body.contains(r#""time_in_force":"GOOD_TILL_CANCEL""#));
}
#[test]
fn missing_cancel_id_does_not_prevent_canceling_live_order() {
    let mut h = Harness::new();
    h.load();
    h.ws("user.order", ORDER);
    command(
        &mut h,
        Command::CancelOrders {
            command: 1,
            ids: vec!["missing".into(), "9007199254740993".into()],
        },
    );
    assert!(
        h.http("private/cancel-order")
            .body
            .unwrap()
            .contains("9007199254740993")
    );
}
#[test]
fn unsuccessful_http_or_missing_success_code_never_reports_write_success() {
    for status in [200, 503] {
        let mut h = Harness::new();
        h.load();
        command(
            &mut h,
            Command::CancelSymbolOrders {
                command: 1,
                symbol: "BTCUSD-PERP".into(),
            },
        );
        let r = h.http("cancel-all-orders");
        h.event(Event::HttpResponse {
            request: r.id,
            response: safe_cex_engine::Response {
                status,
                headers: "",
                body: "{}",
            },
        });
        pace(&mut h);
        assert!(h.actions.iter().any(|a| matches!(
            a,
            Action::Notice(Notice::CommandDone {
                command: 1,
                result: Err(_)
            })
        )));
    }
}
