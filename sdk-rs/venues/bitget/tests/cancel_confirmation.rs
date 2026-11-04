//! Unknown cancels keep their result; only a fresh native census proves open/absent.
mod support;
use safe_cex_engine::model::OrderStatus;
use safe_cex_engine::{Command, Outcome, VenueAction};
use support::*;

const EMPTY: &str = r#"{"code":"00000","data":[]}"#;
const PLANS: &str = r#"{"code":"00000","data":{"entrustedList":[{"orderId":"target","symbol":"BTCUSDT","size":"0","triggerPrice":"120","side":"buy","holdSide":"short","planType":"pos_loss","status":"live"},{"orderId":"sibling","symbol":"BTCUSDT","size":"0","triggerPrice":"80","side":"buy","holdSide":"short","planType":"pos_profit","status":"live"}]}}"#;
const SIBLING: &str = r#"{"code":"00000","data":{"entrustedList":[{"orderId":"sibling","symbol":"BTCUSDT","size":"0","triggerPrice":"80","side":"buy","holdSide":"short","planType":"pos_profit","status":"live"}]}}"#;

fn ready() -> Harness {
    let mut h = Harness::classic();
    while h.requests.iter().any(|r| r.url.contains("/orders-pending")) {
        h.orders(OPEN_ORDER);
    }
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"target","symbol":"BTCUSDT","size":"0","triggerPrice":"120","side":"buy","holdSide":"short","planType":"pos_loss","status":"live"},{"orderId":"sibling","symbol":"BTCUSDT","size":"0","triggerPrice":"80","side":"buy","holdSide":"short","planType":"pos_profit","status":"live"}]}"#);
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Open
    );
    h
}

fn cancel_unknown(h: &mut Harness) {
    h.command(Command::CancelOrders {
        command: 90,
        ids: vec!["target".into()],
    });
    let request = h.take("/cancel-plan-order");
    assert!(request.body.as_deref().unwrap().contains("pos_loss"));
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"successList":[],"failureList":[]}}"#,
    );
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Unknown));
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Open
    );
}

fn census(h: &mut Harness, plans: &str) {
    h.respond("/orders-pending", 200, OPEN_ORDER);
    h.respond("planType=profit_loss", 200, plans);
    h.respond("planType=track_plan", 200, EMPTY);
}

fn confirmations(h: &Harness, result: &str) -> usize {
    h.events
        .iter()
        .filter(|e| {
            e.code == "bitget.cancel.confirmed"
                && e.fields.iter().any(|(key, value)| {
                    *key == "result"
                        && matches!(value, safe_cex_engine::Value::Str(s) if s == result)
                })
        })
        .count()
}

#[test]
fn clean_native_absence_beats_cached_algo_retention_without_claiming_canceled() {
    let mut h = ready();
    cancel_unknown(&mut h);
    census(&mut h, SIBLING);
    assert!(
        h.engine.store().order("target").is_none(),
        "fresh native absence must beat cached algo retention"
    );
    assert_eq!(
        h.engine.store().order("sibling").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(confirmations(&h, "absent"), 1);
    assert_eq!(h.failures.len(), 1, "original Unknown is not rewritten");
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("/cancel-plan-order"))
    );
}

#[test]
fn a_clean_native_present_target_is_reported_still_open() {
    let mut h = ready();
    cancel_unknown(&mut h);
    census(&mut h, PLANS);
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(confirmations(&h, "present"), 1);
    assert_eq!(h.failures.len(), 1);
}

#[test]
fn a_pre_cancel_census_proves_nothing_and_owes_one_fresh_successor() {
    let mut h = ready();
    h.command(Command::Venue {
        command: 80,
        action: VenueAction::RefreshOrders { symbols: None },
    });
    cancel_unknown(&mut h);
    census(&mut h, SIBLING);
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(confirmations(&h, "absent"), 0);
    assert_eq!(
        h.requests
            .iter()
            .filter(|r| r.url.contains("/orders-pending"))
            .count(),
        1
    );
    census(&mut h, SIBLING);
    assert!(h.engine.store().order("target").is_none());
    assert_eq!(confirmations(&h, "absent"), 1);
}

fn refresh(h: &mut Harness) {
    h.command(Command::Venue {
        command: 99,
        action: VenueAction::RefreshOrders { symbols: None },
    });
}

#[test]
fn malformed_partial_and_wrong_symbol_native_reads_prove_nothing() {
    for plans in [
        r#"{"code":"00000","data":{}}"#,
        r#"{"code":"00000","data":{"entrustedList":[{"orderId":"bad","symbol":"BTCUSDT","size":"oops"}]}}"#,
        r#"{"code":"00000","data":{"entrustedList":[{"orderId":"target","symbol":"NOTAMARKET","size":"1","triggerPrice":"120","side":"buy","planType":"pos_loss","status":"live"}]}}"#,
    ] {
        let mut h = ready();
        cancel_unknown(&mut h);
        h.respond("/orders-pending", 200, OPEN_ORDER);
        h.respond("planType=profit_loss", 200, plans);
        if h.requests
            .iter()
            .any(|r| r.url.contains("planType=track_plan"))
        {
            h.respond("planType=track_plan", 200, EMPTY);
        }
        assert_eq!(confirmations(&h, "absent"), 0, "{plans}");
        assert_eq!(confirmations(&h, "present"), 0, "{plans}");
        assert_eq!(
            h.engine.store().order("target").unwrap().status,
            OrderStatus::Open
        );
    }
}

#[test]
fn failed_and_busy_reads_keep_the_check_for_a_later_clean_read() {
    for (status, body) in [
        (503, "unavailable"),
        (429, r#"{"code":"429","msg":"busy"}"#),
    ] {
        let mut h = ready();
        cancel_unknown(&mut h);
        h.respond("/orders-pending", status, body);
        assert_eq!(confirmations(&h, "absent"), 0);
        assert_eq!(
            h.engine.store().order("target").unwrap().status,
            OrderStatus::Open
        );
        if status == 429 {
            refresh(&mut h);
            assert!(
                !h.requests.iter().any(|r| r.url.contains("/orders-pending")),
                "native REST pause remains"
            );
            h.now += 30_001.0;
        }
        refresh(&mut h);
        census(&mut h, SIBLING);
        assert_eq!(confirmations(&h, "absent"), 1);
        assert!(h.engine.store().order("target").is_none());
    }
}

#[test]
fn a_frame_fed_cancel_registers_targets_before_one_source_refresh_without_local_reads() {
    for lost in [false, true] {
        let mut source = ready();
        let frame = source.engine.snapshot_frame(source.now).to_vec();
        let mut page = Harness::started_configure(
            FixtureOptions {
                key: Some("key".into()),
                secret: Some("secret".into()),
                passphrase: Some("pass".into()),
                ..FixtureOptions::default()
            },
            false,
            |e| {
                e.set_frame_fed().unwrap();
                e.set_log_level(safe_cex_engine::Level::Error);
            },
        );
        page.respond("/contracts", 200, CONTRACTS);
        page.engine.apply_frame(page.now, &frame, true).unwrap();
        page.collect();
        if lost {
            page.command(Command::CancelOrders {
                command: 90,
                ids: vec!["target".into()],
            });
            let r = page.take("/cancel-plan-order");
            page.reply(r.id, 503, "unavailable");
            assert_eq!(
                page.failures.last().unwrap().outcome,
                Some(Outcome::Unknown)
            );
        } else {
            cancel_unknown(&mut page);
        }
        assert!(page.requests.is_empty(), "frame-fed writer never reads");
        let names: Vec<_> = page
            .events
            .iter()
            .filter(|e| e.code.starts_with("orders."))
            .map(|e| e.code)
            .collect();
        assert_eq!(names, ["orders.cancel_check", "orders.refresh"]);
        let targets = page
            .events
            .iter()
            .find(|e| e.code == "orders.cancel_check")
            .unwrap()
            .fields
            .iter()
            .find_map(|(k, v)| match (*k, v) {
                ("targets", safe_cex_engine::Value::Str(v)) => Some(v),
                _ => None,
            })
            .unwrap();
        let command = safe_cex_engine::command::decode_command(&format!(
            r#"{{"op":"registerCancelChecks","command":98,"orders":{targets}}}"#
        ))
        .unwrap();
        source.command(command);
        refresh(&mut source);
        census(&mut source, SIBLING);
        assert!(source.engine.store().order("target").is_none());
        assert_eq!(confirmations(&source, "absent"), 1);
        assert_eq!(page.failures.len(), 1);
    }
}

fn uta() -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        r#"{"code":"00000","data":{"holdMode":"hedge_mode"}}"#,
    );
    h.respond(
        "/current-position",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond(
        "/account/assets",
        200,
        r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#,
    );
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-orders") || r.url.contains("/unfilled-strategy-orders"))
    {
        if h.requests
            .iter()
            .any(|r| r.url.contains("/unfilled-orders"))
        {
            h.respond("/unfilled-orders", 200, EMPTY);
        } else {
            h.respond("/unfilled-strategy-orders", 200, EMPTY);
        }
    }
    h.login();
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/unfilled-orders") || r.url.contains("/unfilled-strategy-orders"))
    {
        if h.requests
            .iter()
            .any(|r| r.url.contains("/unfilled-orders"))
        {
            h.respond("/unfilled-orders", 200, EMPTY);
        } else {
            h.respond("/unfilled-strategy-orders", 200, EMPTY);
        }
    }
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"target","symbol":"BTCUSDT","qty":"1","stopLoss":"80","side":"sell","posSide":"long","status":"pending"}]}"#);
    h
}

#[test]
fn uta_null_success_stays_success_and_a_lost_cancel_is_confirmed_from_tpsl_only() {
    for lost in [false, true] {
        let mut h = uta();
        assert_eq!(
            h.engine.store().order("target").unwrap().kind,
            safe_cex_engine::model::OrderType::StopLoss
        );
        h.command(Command::CancelOrders {
            command: 90,
            ids: vec!["target".into()],
        });
        let r = h.take("/cancel-strategy-order");
        assert!(r.body.as_deref().unwrap().contains("target"));
        h.reply(
            r.id,
            if lost { 503 } else { 200 },
            if lost {
                "unavailable"
            } else {
                r#"{"code":"00000","data":null}"#
            },
        );
        if !lost {
            assert!(
                h.done
                    .iter()
                    .any(|(id, result)| *id == 90 && result.is_ok())
            );
            assert!(!h.events.iter().any(|e| e.code == "bitget.cancel.check"));
        } else {
            assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Unknown));
            h.respond("/unfilled-orders", 200, EMPTY);
            h.respond("type=tpsl", 200, EMPTY);
            assert_eq!(
                confirmations(&h, "absent"),
                0,
                "all native sources must finish"
            );
            h.respond("type=trailing_stop", 200, EMPTY);
            assert_eq!(
                confirmations(&h, "absent"),
                1,
                "{:?}",
                (&h.events, &h.failures, h.engine.store().orders())
            );
            assert!(h.engine.store().order("target").is_none());
        }
    }
}

#[test]
fn a_newer_filled_target_is_never_rewritten_to_canceled_or_confirmed_by_a_stale_read() {
    let mut h = ready();
    cancel_unknown(&mut h);
    h.private_message(r#"{"arg":{"channel":"orders-algo"},"data":[{"orderId":"target","symbol":"BTCUSDT","size":"0","triggerPrice":"120","side":"buy","holdSide":"short","planType":"pos_loss","status":"executed"}]}"#);
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Closed
    );
    census(&mut h, SIBLING);
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Closed
    );
    assert_eq!(confirmations(&h, "absent"), 0);
}

#[test]
fn read_only_registration_never_cancels_or_confirms_a_wrong_symbol() {
    let mut h = ready();
    let before = h.requests.len();
    h.command(Command::Venue {
        command: 98,
        action: VenueAction::RegisterCancelChecks {
            orders: vec![safe_cex_engine::model::CancelOrderRef {
                id: "target".into(),
                symbol: "ETHUSDT".into(),
            }],
        },
    });
    assert_eq!(h.requests.len(), before, "registration starts no I/O");
    refresh(&mut h);
    census(&mut h, SIBLING);
    assert_eq!(
        h.engine.store().order("target").unwrap().status,
        OrderStatus::Open
    );
    assert_eq!(confirmations(&h, "absent"), 0);
}

#[test]
fn a_partial_native_failure_keeps_its_error_and_confirms_unaccounted_tpsl_legs() {
    let mut h = uta();
    h.private_message(r#"{"arg":{"topic":"strategy-order"},"data":[{"category":"usdt-futures","orderId":"pair","symbol":"BTCUSDT","qty":"1","stopLoss":"80","takeProfit":"120","side":"sell","posSide":"long","status":"pending"}]}"#);
    h.command(Command::CancelOrders {
        command: 90,
        ids: vec!["pair__stop_loss".into(), "pair__take_profit".into()],
    });
    let r = h.take("/cancel-strategy-order");
    h.reply(r.id,200,r#"{"code":"00000","data":{"successList":[],"failureList":[{"errorCode":"45110","errorMsg":"one leg refused"}]}}"#);
    assert_eq!(h.failures.last().unwrap().outcome, Some(Outcome::Unknown));
    assert!(
        h.failures
            .last()
            .unwrap()
            .message
            .contains("one leg refused")
    );
    h.respond("/unfilled-orders", 200, EMPTY);
    h.respond("type=tpsl", 200, EMPTY);
    h.respond("type=trailing_stop", 200, EMPTY);
    assert_eq!(confirmations(&h, "absent"), 2);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.url.contains("cancel-strategy-order"))
    );
}
