mod support;
use safe_cex_engine::Notice;
use safe_cex_venue_breakoutprop::Options;
use support::{QUOTES, Rig};
/// Native cadence of the venue's recurring account poll; policy scales it.
const POLL_MS: f64 = 2_000.0;

const PORTFOLIO: &str = r#"{"portfolios":[{"account":"default:00002","version":86,"balances":[{"account":"default:00002","currency":"USD","value":4999.96}],"positions":[{"account":"default:00002","positionCode":"76515503","symbol":"ETHUSD","side":"BUY","quantity":0.01,"quantityNotional":0.01,"openPrice":3656.01,"marginRate":0.2,"lastUpdateTime":"2024-12-11T06:00:49.087Z"},{"account":"default:00002","positionCode":"76515542","symbol":"ETHUSD","side":"BUY","quantity":0.01,"quantityNotional":0.01,"openPrice":3660,"marginRate":0.2,"lastUpdateTime":"2024-12-11T05:56:26.400Z"}],"orders":[]}]}"#;

#[test]
fn periodic_account_policy_resumes_initially_paused_public_and_private_profiles() {
    use safe_cex_engine::Event;
    for public_data_only in [true, false] {
        let mut h = Rig::new(Options {
            key: "example-user".into(),
            secret: "example-password".into(),
            public_data_only,
            ..Default::default()
        });
        h.engine.handle(
            h.now,
            Event::PollScaleChanged {
                scale: f64::INFINITY,
            },
        );
        h.boot();
        if !public_data_only {
            for _ in 0..2 {
                let request = h.next_request();
                assert!(request.url.ends_with("/portfolio"));
                h.reply(&request, 200, PORTFOLIO);
            }
            let quote = h.next_request();
            assert!(quote.url.ends_with("/marketdata"));
            h.reply(&quote, 200, QUOTES);
        }
        assert!(h.timers.is_empty());
        let mut old = Vec::new();
        for interval in [20_000., 1_500.] {
            h.engine.handle(
                h.now,
                Event::PollScaleChanged {
                    scale: interval / POLL_MS,
                },
            );
            h.collect();
            assert_eq!(h.timers.len(), 1);
            let (&timer, &at) = h.timers.iter().next().unwrap();
            assert_eq!(at, h.now + interval);
            old.push(timer);
        }
        h.engine.handle(h.now, Event::TimerFired { timer: old[0] });
        h.collect();
        assert!(h.requests.is_empty());
        h.fire_next_timer();
        let request = h.next_request();
        assert!(request.url.ends_with(if public_data_only {
            "/marketdata"
        } else {
            "/portfolio"
        }));
        // Completion must register against the current paused policy.
        h.engine.handle(
            h.now,
            Event::PollScaleChanged {
                scale: f64::INFINITY,
            },
        );
        h.collect();
        h.reply(
            &request,
            200,
            if public_data_only { QUOTES } else { PORTFOLIO },
        );
        if !public_data_only {
            let quote = h.next_request();
            assert!(quote.url.ends_with("/marketdata"));
            h.reply(&quote, 200, QUOTES);
        }
        assert!(h.timers.is_empty());
        h.engine.handle(
            h.now,
            Event::PollScaleChanged {
                scale: 1000. / POLL_MS,
            },
        );
        h.collect();
        assert_eq!(
            h.timers.values().copied().collect::<Vec<_>>(),
            [h.now + 1000.]
        );
    }
}
const ZERO_CLOSE: &str = r#"{"account":"default:00002","orderId":76515504,"orderCode":"4471153:55640","clientOrderId":"55640","instrument":"ETHUSD","type":"STOP","status":"WORKING","finalStatus":false,"transactionTime":"2024-12-11T05:55:49.900Z","legs":[{"instrument":"ETHUSD","side":"SELL","positionEffect":"CLOSE","price":3500,"quantity":0,"filledQuantity":0,"remainingQuantity":0}]}"#;
fn rig() -> Rig {
    let mut rig = Rig::new(Options {
        key: "example-user".into(),
        secret: "example-password".into(),
        ..Options::default()
    });
    rig.boot();
    for _ in 0..2 {
        let request = rig.next_request();
        assert!(request.url.ends_with("/portfolio"));
        rig.reply(&request, 200, PORTFOLIO);
    }
    let quote = rig.next_request();
    assert!(quote.url.ends_with("/marketdata"));
    rig.reply(&quote, 200, QUOTES);
    rig
}
#[test]
fn documented_lots_keep_unique_position_codes_and_atomic_cash_estimates() {
    let rig = rig();
    let store = rig.engine.store();
    assert!(store.loaded().balance && store.loaded().positions && store.loaded().orders);
    assert_eq!(store.positions().len(), 2);
    assert_eq!(store.positions()[0].id, "76515503");
    assert_eq!(store.positions()[1].id, "76515542");
    assert!((store.balance().upnl - (3656.01 - 3660.0) * 0.01).abs() < 1e-10);
    assert_eq!(store.balance().used, 0.004);
    assert_eq!(store.setting("ETHUSD").unwrap().leverage, 5.0);
}
#[test]
fn failed_portfolio_keeps_every_section_and_quote_polling_recovers() {
    let mut rig = rig();
    let before = rig.engine.store().balance().clone();
    let request = rig.next_request();
    assert!(request.url.ends_with("/portfolio"));
    rig.reply(&request, 500, "{}");
    assert_eq!(*rig.engine.store().balance(), before);
    assert_eq!(rig.engine.store().positions().len(), 2);
    let quote = rig.next_request();
    assert!(quote.url.ends_with("/marketdata"));
    rig.reply(&quote, 200, QUOTES);
}
#[test]
fn malformed_nested_account_rejects_all_snapshot_sections() {
    let mut rig = rig();
    let before = rig.engine.store().balance().clone();
    let request = rig.next_request();
    let bad = PORTFOLIO.replacen(
        "\"account\":\"default:00002\",\"currency\"",
        "\"account\":\"default:12\",\"currency\"",
        1,
    );
    rig.reply(&request, 200, &bad);
    assert_eq!(*rig.engine.store().balance(), before);
    assert_eq!(rig.engine.store().positions().len(), 2);
}
#[test]
fn lower_portfolio_version_cannot_rollback_cash() {
    let mut rig = rig();
    let request = rig.next_request();
    rig.reply(
        &request,
        200,
        &PORTFOLIO
            .replace("\"version\":86", "\"version\":85")
            .replace("4999.96", "100"),
    );
    assert!(rig.engine.store().balance().total > 4000.0);
}
#[test]
fn missing_selected_portfolio_does_not_clear_account() {
    let mut rig = rig();
    let request = rig.next_request();
    rig.reply(&request, 200, "{\"portfolios\":[]}");
    assert_eq!(rig.engine.store().positions().len(), 2);
}

#[test]
fn failed_startup_portfolio_waits_for_poll_interval_before_retry() {
    let mut rig = Rig::new(Options {
        key: "example-user".into(),
        secret: "example-password".into(),
        tick_interval: 10_000.0,
        ..Options::default()
    });
    rig.boot();
    let first = rig.next_request();
    assert!(first.url.ends_with("/portfolio"));
    rig.reply(&first, 500, "{}");
    let quote = rig.next_request();
    rig.reply(&quote, 200, QUOTES);
    let quote_at = rig.now;
    let second = rig.next_request();
    assert!(second.url.ends_with("/portfolio"));
    assert!(rig.now >= quote_at + 10_000.0);
    assert!(!rig.engine.store().loaded().balance);
}

#[test]
fn failed_refresh_login_drains_portfolio_without_starting_another_login() {
    let mut rig = rig();
    let before = rig.engine.store().balance().clone();
    let portfolio = rig.next_request();
    rig.reply(&portfolio, 401, "{}");
    let login = rig.next_request();
    assert!(login.url.ends_with("/login"));
    rig.reply(&login, 500, "{}");
    assert!(rig.requests.is_empty());
    assert!(!rig.notices.contains(&Notice::Deactivate));
    assert_eq!(*rig.engine.store().balance(), before);
    let failure_at = rig.now;
    let retry = rig.next_request();
    assert!(retry.url.ends_with("/login"));
    assert!(rig.now >= failure_at + 2_000.0);
}

#[test]
fn rejected_refresh_login_stops_account_polling_without_clearing_snapshot() {
    let mut rig = rig();
    let before = rig.engine.store().balance().clone();
    let portfolio = rig.next_request();
    rig.reply(&portfolio, 401, "{}");
    let login = rig.next_request();
    rig.reply(&login, 403, "{}");
    assert!(rig.requests.is_empty());
    assert!(rig.timers.is_empty());
    assert!(rig.notices.contains(&Notice::Deactivate));
    assert_eq!(*rig.engine.store().balance(), before);
    assert_eq!(rig.engine.store().positions().len(), 2);
}

#[test]
fn zero_close_order_reconciles_with_cash_positions_and_following_polls() {
    let mut rig = rig();
    let request = rig.next_request();
    let body = PORTFOLIO
        .replace("\"version\":86", "\"version\":87")
        .replace("4999.96", "5100")
        .replace("\"orders\":[]", &format!("\"orders\":[{ZERO_CLOSE}]"));
    rig.reply(&request, 200, &body);
    let store = rig.engine.store();
    assert_eq!(store.orders().len(), 1);
    let order = &store.orders()[0];
    assert_eq!(order.id, "4471153:55640");
    assert!(order.reduce_only);
    assert_eq!(
        (order.amount, order.filled, order.remaining),
        (0.0, 0.0, 0.0)
    );
    assert_eq!(store.positions().len(), 2);
    assert_eq!(store.balance().used, 0.004);
    assert!((store.balance().total - (5100.0 + store.balance().upnl)).abs() < 1e-10);
    let quote = rig.next_request();
    assert!(quote.url.ends_with("/marketdata"));
    rig.reply(&quote, 200, QUOTES);
    assert!(rig.next_request().url.ends_with("/portfolio"));
}

#[test]
fn zero_open_or_nonzero_execution_on_zero_close_rejects_the_whole_snapshot() {
    for invalid in [
        ZERO_CLOSE.replace("CLOSE", "OPEN"),
        ZERO_CLOSE.replace("\"filledQuantity\":0", "\"filledQuantity\":1e-13"),
        ZERO_CLOSE.replace("\"remainingQuantity\":0", "\"remainingQuantity\":1e-13"),
    ] {
        let mut rig = rig();
        let before = rig.engine.store().balance().clone();
        let request = rig.next_request();
        let body = PORTFOLIO
            .replace("\"version\":86", "\"version\":87")
            .replace("4999.96", "100")
            .replace("\"orders\":[]", &format!("\"orders\":[{invalid}]"));
        rig.reply(&request, 200, &body);
        assert_eq!(*rig.engine.store().balance(), before);
        assert_eq!(rig.engine.store().positions().len(), 2);
        assert!(rig.engine.store().orders().is_empty());
    }
}

#[test]
fn empty_and_missing_client_ids_keep_distinct_native_order_identities() {
    let mut rig = rig();
    let first = ZERO_CLOSE.replace("\"clientOrderId\":\"55640\"", "\"clientOrderId\":\"\"");
    let second = ZERO_CLOSE
        .replace("\"clientOrderId\":\"55640\",", "")
        .replace("4471153:55640", "4471153:55641")
        .replace("76515504", "76515505");
    let request = rig.next_request();
    rig.reply(
        &request,
        200,
        &PORTFOLIO.replace("\"orders\":[]", &format!("\"orders\":[{first},{second}]")),
    );
    let orders = rig.engine.store().orders();
    assert_eq!(orders.len(), 2);
    assert_eq!(orders[0].id, "4471153:55640");
    assert_eq!(orders[1].id, "4471153:55641");
    assert!(orders.iter().all(|order| order.client_order_id.is_none()));
}

// DX REST 9.5 prices belong to Order; legacy hosts additionally expose leg.price.
#[test]
fn documented_order_prices_publish_the_complete_selected_portfolio() {
    for (kind, field, price) in [
        ("LIMIT", Some("limitPrice"), 3529.47),
        ("STOP", Some("stopPrice"), 3500.0),
        ("MARKET", None, 0.0),
    ] {
        let mut rig = rig();
        let mut row: serde_json::Value = serde_json::from_str(ZERO_CLOSE).unwrap();
        row["type"] = kind.into();
        row["legs"][0].as_object_mut().unwrap().remove("price");
        if let Some(field) = field {
            row[field] = price.into();
        }
        let request = rig.next_request();
        rig.reply(
            &request,
            200,
            &PORTFOLIO
                .replace("4999.96", "5100")
                .replace("\"orders\":[]", &format!("\"orders\":[{row}]")),
        );
        let store = rig.engine.store();
        assert_eq!(store.orders().len(), 1, "{kind}");
        assert_eq!(store.orders()[0].price, price);
        assert_eq!(store.positions().len(), 2);
        assert!((store.balance().total - (5100.0 + store.balance().upnl)).abs() < 1e-10);
    }
}

#[test]
fn missing_or_invalid_documented_price_cannot_replace_any_account_section() {
    for (field, value) in [
        ("stopPrice", None),
        ("stopPrice", Some(0.0)),
        ("stopPrice", Some(-1.0)),
        ("limitPrice", None),
        ("limitPrice", Some(0.0)),
        ("limitPrice", Some(-1.0)),
    ] {
        let mut rig = rig();
        let before = rig.engine.store().balance().clone();
        let mut row: serde_json::Value = serde_json::from_str(ZERO_CLOSE).unwrap();
        row["type"] = if field == "stopPrice" {
            "STOP"
        } else {
            "LIMIT"
        }
        .into();
        // An explicitly invalid documented price must not hide behind a legacy value.
        if let Some(value) = value {
            row[field] = value.into();
        } else {
            row["legs"][0].as_object_mut().unwrap().remove("price");
        }
        let request = rig.next_request();
        rig.reply(
            &request,
            200,
            &PORTFOLIO
                .replace("4999.96", "100")
                .replace("\"orders\":[]", &format!("\"orders\":[{row}]")),
        );
        assert_eq!(*rig.engine.store().balance(), before, "{field} {value:?}");
        assert_eq!(rig.engine.store().positions().len(), 2);
        assert!(rig.engine.store().orders().is_empty());
    }
}

#[test]
fn amount_only_stop_amendment_uses_documented_trigger_after_reconciliation() {
    for legacy_price in [None, Some(0.0)] {
        let mut rig = rig();
        let mut row: serde_json::Value = serde_json::from_str(ZERO_CLOSE).unwrap();
        row["stopPrice"] = 3500.0.into();
        if let Some(price) = legacy_price {
            row["legs"][0]["price"] = price.into();
        } else {
            row["legs"][0].as_object_mut().unwrap().remove("price");
        }
        let portfolio = rig.next_request();
        rig.reply(
            &portfolio,
            200,
            &PORTFOLIO.replace("\"orders\":[]", &format!("\"orders\":[{row}]")),
        );
        let quote = rig.next_request();
        rig.reply(&quote, 200, QUOTES);
        rig.command(safe_cex_engine::Command::AmendOrder {
            command: 90,
            id: "4471153:55640".into(),
            price: None,
            amount: Some(0.02),
        });
        let cancel = rig.next_request();
        assert_eq!(cancel.method, safe_cex_engine::Method::Delete);
        rig.reply(&cancel, 200, r#"{"orderId":76515504}"#);
        let proof = rig.next_request();
        assert!(
            proof
                .url
                .ends_with("/orders/history?with-order-id=76515504&limit=2")
        );
        row["status"] = "CANCELED".into();
        row["finalStatus"] = true.into();
        rig.reply(&proof, 200, &format!("{{\"orders\":[{row}]}}"));
        let replacement = rig.next_request();
        assert_eq!(replacement.method, safe_cex_engine::Method::Post);
        let body: serde_json::Value =
            serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["stopPrice"], 3500.0);
        assert_eq!(body["quantity"], 0.02);
        assert_eq!(body["positionEffect"], "CLOSE");
        rig.reply(&replacement, 200, r#"{"orderId":42}"#);
        assert_eq!(rig.result(90), Ok("[\"42\"]".into()));
    }
}
