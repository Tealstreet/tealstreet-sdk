mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome};
use safe_cex_venue_lighter::Lighter;
use serde_json::{Value, json};
use support::*;
fn original() -> Value {
    json!({"order_index":"42","client_order_index":"123","market_id":1,"status":"open","type":2,"time_in_force":0,
        "is_ask":true,"reduce_only":true,"initial_base_amount":"2","filled_base_amount":"0","remaining_base_amount":"2",
        "price":"79","trigger_price":"80","order_expiry":NOW as u64+1_000_000,"trigger_status":"untriggered",
        "parent_order_index":0,"parent_order_id":"0","to_trigger_order_id_0":"0","to_trigger_order_id_1":"0","to_cancel_order_id_0":"0","order_flags":0})
}
fn boot() -> (Engine<Lighter>, Feeds, Timers) {
    let (mut e, feeds, timers) = ready_feeds(
        Options {
            testnet: true,
            ..options()
        },
        true,
    );
    message(
        &mut e,
        feeds.private,
        NOW,
        &json!({"channel":"account_all_orders", "orders":[original()]}).to_string(),
    );
    mark(&mut e, &feeds, NOW, json!({"mark_price":"100"}));
    drain_at(&mut e, &mut vec![], NOW);
    (e, feeds, timers)
}
fn mark(e: &mut Engine<Lighter>, feeds: &Feeds, now: f64, fields: Value) {
    let mut row = fields;
    row["market_id"] = json!(1);
    message(
        e,
        feeds.public,
        now,
        &json!({"channel":"market_stats/all","market_stats":[row]}).to_string(),
    );
}
fn amend(e: &mut Engine<Lighter>, now: f64) {
    e.handle(
        now,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(85.0),
            amount: Some(1.0),
        }),
    );
}
fn request(e: &mut Engine<Lighter>, now: f64, path: &str) -> HttpRequest {
    let out = safe_cex_engine::test_support::prepared_actions(e, now);
    out.iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains(path) => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{path}: {out:?}"))
}
fn ack(e: &mut Engine<Lighter>, r: &HttpRequest, now: f64) {
    reply_at(e, now, r, 200, r#"{"code":200,"tx_hash":"accepted"}"#);
}
fn cancel(e: &mut Engine<Lighter>, now: f64) -> HttpRequest {
    amend(e, now);
    let nonce = request(e, now, "nextNonce");
    reply_at(e, now, &nonce, 200, r#"{"code":200,"nonce":10}"#);
    request(e, now, "sendTx")
}
fn proof(e: &mut Engine<Lighter>, t: &mut Timers, cancel: &HttpRequest) -> HttpRequest {
    ack(e, cancel, NOW);
    let out = advance(e, t, NOW + 100.0);
    out.iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("accountOrders") => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{out:?}"))
}
fn replacement(e: &mut Engine<Lighter>, t: &mut Timers) -> HttpRequest {
    let c = cancel(e, NOW);
    let p = proof(e, t, &c);
    let mut row = original();
    row["status"] = json!("canceled");
    reply_at(
        e,
        NOW + 100.0,
        &p,
        200,
        &json!({"code":200,"orders":[row]}).to_string(),
    );
    request(e, NOW + 100.0, "sendTx")
}
#[test]
fn wrong_side_and_stale_mark_never_cancel() {
    for (now, fields) in [
        (NOW + 4999.0, json!({"mark_price":"85"})),
        (NOW + 5001.0, json!({})),
    ] {
        let (mut e, feeds, _) = boot();
        if !fields.as_object().unwrap().is_empty() {
            mark(&mut e, &feeds, NOW, fields);
        }
        amend(&mut e, now);
        let mut out = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        if now > NOW + 5000.0 {
            let reference = out
                .iter()
                .find_map(|a| match a {
                    Action::Http(r) if r.url.ends_with("/api/v1/exchangeStats") => Some(r.clone()),
                    _ => None,
                })
                .expect("a stale native receipt gets one public read");
            assert!(
                !out.iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.contains("sendTx")))
            );
            reply_at(
                &mut e,
                now,
                &reference,
                200,
                r#"{"code":200,"order_book_stats":[]}"#,
            );
            out = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        }
        assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{out:?}");
        assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.outcome==Some(Outcome::NotSent))),"{out:?}");
        assert!(e.store().order("42").is_some());
    }
}
#[test]
fn cancel_ack_requires_exact_unfilled_terminal_target_before_replacement() {
    for change in [
        "empty",
        "foreign",
        "filled",
        "quote-filled",
        "open",
        "valid",
    ] {
        let (mut e, _, mut t) = boot();
        let c = cancel(&mut e, NOW);
        let p = proof(&mut e, &mut t, &c);
        assert!(p.url.contains("client_order_indexes=123"));
        assert!(
            p.headers
                .iter()
                .any(|(key, value)| key.eq_ignore_ascii_case("authorization") && !value.is_empty())
        );
        assert!(!p.url.contains("auth="));
        let mut row = original();
        row["status"] = json!("canceled");
        match change {
            "foreign" => row["order_index"] = json!("43"),
            "filled" => row["filled_base_amount"] = json!("0.1"),
            "quote-filled" => row["filled_quote_amount"] = json!("10"),
            "open" => row["status"] = json!("open"),
            _ => {}
        }
        let rows = if change == "empty" { vec![] } else { vec![row] };
        reply_at(
            &mut e,
            NOW + 100.0,
            &p,
            200,
            &json!({"code":200,"orders":rows}).to_string(),
        );
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 100.0);
        assert_eq!(
            out.iter()
                .filter(|a| matches!(a,Action::Http(r) if r.url.contains("sendTx")))
                .count(),
            usize::from(change == "valid"),
            "{change}: {out:?}"
        );
        if change != "valid" {
            assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("outcome unknown")&&error.protection.is_none())),"{out:?}");
        }
    }
}
#[test]
fn guarded_rate_and_uncertain_replacement_never_put_back_or_resend() {
    for (status, body) in [
        (429, r#"{"code":23000,"message":"throttle"}"#),
        (429, r#"{"code":21104,"message":"invalid nonce"}"#),
        (405, r#"{"code":21104,"message":"invalid nonce"}"#),
        (200, r#"{"code":23000,"message":"throttle"}"#),
        (200, r#"{"code":23100,"message":"throttle"}"#),
        (408, r#"{"code":21104,"message":"invalid nonce"}"#),
        (500, r#"{"code":21702,"message":"refused"}"#),
        (503, r#"{"code":21702,"message":"refused"}"#),
        (200, "bad json"),
    ] {
        let (mut e, _, mut t) = boot();
        let r = replacement(&mut e, &mut t);
        reply_at(&mut e, NOW + 100.0, &r, status, body);
        let out = advance(&mut e, &mut t, NOW + 1000.0);
        assert!(!out.iter().any(|a|matches!(a,Action::Http(r) if r.url.contains("sendTx")||r.url.contains("nextNonce"))),"{status}: {out:?}");
        assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.outcome==Some(Outcome::Unknown)&&error.protection.is_none())),"{out:?}");
        let error = out
            .iter()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
                _ => None,
            })
            .unwrap();
        let message = error.message.to_ascii_lowercase();
        assert!(
            message.contains("unknown") && message.contains("check open orders"),
            "{error:?}"
        );
        assert!(
            !message.contains("refused") && !message.contains("no stop"),
            "{error:?}"
        );
    }
}
fn tx(r: &HttpRequest) -> Value {
    let raw = r
        .body
        .as_ref()
        .unwrap()
        .split('&')
        .find_map(|p| p.strip_prefix("tx_info="))
        .unwrap();
    let mut out = Vec::new();
    let mut it = raw.bytes();
    while let Some(c) = it.next() {
        if c == b'%' {
            let a = [it.next().unwrap(), it.next().unwrap()];
            out.push(u8::from_str_radix(std::str::from_utf8(&a).unwrap(), 16).unwrap());
        } else {
            out.push(c);
        }
    }
    serde_json::from_slice(&out).unwrap()
}
#[test]
fn definitive_refusal_puts_back_original_size_and_parameters_once() {
    let (mut e, _, mut t) = boot();
    let r = replacement(&mut e, &mut t);
    assert_eq!(tx(&r)["BaseAmount"], 1000);
    reply_at(
        &mut e,
        NOW + 100.0,
        &r,
        200,
        r#"{"code":21702,"message":"replacement refused"}"#,
    );
    let nonce = request(&mut e, NOW + 100.0, "nextNonce");
    reply_at(
        &mut e,
        NOW + 100.0,
        &nonce,
        200,
        r#"{"code":200,"nonce":20}"#,
    );
    let restore = request(&mut e, NOW + 100.0, "sendTx");
    let wire = tx(&restore);
    assert_eq!(wire["BaseAmount"], 2000);
    assert_eq!(wire["TriggerPrice"], 8000);
    assert_eq!(wire["Price"], 7900);
    assert_eq!(wire["Type"], 2);
    assert_eq!(wire["ReduceOnly"], 1);
    assert_eq!(wire["OrderExpiry"], NOW as u64 + 1_000_000);
    reply_at(
        &mut e,
        NOW + 100.0,
        &restore,
        200,
        r#"{"code":21702,"message":"restore refused"}"#,
    );
    let out = advance(&mut e, &mut t, NOW + 2000.0);
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("sendTx"))),
        "{out:?}"
    );
    assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.protection==Some(safe_cex_engine::error::Protection::Unprotected))),"{out:?}");
}

fn begin_restore(e: &mut Engine<Lighter>, t: &mut Timers) -> HttpRequest {
    let r = replacement(e, t);
    reply_at(
        e,
        NOW + 100.0,
        &r,
        200,
        r#"{"code":21702,"message":"replacement refused"}"#,
    );
    let nonce = request(e, NOW + 100.0, "nextNonce");
    reply_at(e, NOW + 100.0, &nonce, 200, r#"{"code":200,"nonce":20}"#);
    request(e, NOW + 100.0, "sendTx")
}
#[test]
fn putback_unknown_never_claims_restored_or_both_refused() {
    for (status, body) in [
        (429, r#"{"code":23000,"message":"throttle"}"#),
        (405, r#"{"code":21104,"message":"invalid nonce"}"#),
        (200, r#"{"code":23000,"message":"throttle"}"#),
        (408, ""),
        (500, r#"{"code":21702,"message":"refused"}"#),
        (200, r#"{"code":200}"#),
    ] {
        let (mut e, _, mut t) = boot();
        let r = begin_restore(&mut e, &mut t);
        reply_at(&mut e, NOW + 100.0, &r, status, body);
        let out = advance(&mut e, &mut t, NOW + 1500.0);
        assert!(!out.iter().any(|a|matches!(a,Action::Http(r) if r.url.contains("sendTx")||r.url.contains("nextNonce"))),"{out:?}");
        assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.outcome==Some(Outcome::Unknown)&&error.protection.is_none()&&error.message.contains("check Open Orders"))),"{out:?}");
    }
}
#[test]
fn putback_receipt_requires_a_positive_exact_open_row() {
    for valid in [false, true] {
        let (mut e, _, mut t) = boot();
        let r = begin_restore(&mut e, &mut t);
        let wire = tx(&r);
        ack(&mut e, &r, NOW + 100.0);
        let out = advance(&mut e, &mut t, NOW + 200.0);
        let p = out
            .iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.contains("accountOrders") => Some(r.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{out:?}"));
        let mut row = original();
        row["order_index"] = json!("142");
        row["client_order_index"] = wire["ClientOrderIndex"].clone();
        if !valid {
            row["initial_base_amount"] = json!("1");
        }
        reply_at(
            &mut e,
            NOW + 200.0,
            &p,
            200,
            &json!({"code":200,"orders":[row]}).to_string(),
        );
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 200.0);
        assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.protection==valid.then_some(safe_cex_engine::error::Protection::Restored))),"{valid}: {out:?}");
    }
}
#[test]
fn mark_freshness_boundaries_and_sparse_or_substituted_fields() {
    for (age, sparse, allowed) in [
        (4999.0, false, true),
        (5000.0, false, true),
        (5001.0, false, false),
        (5001.0, true, false),
    ] {
        let (mut e, feeds, _) = boot();
        if sparse {
            mark(
                &mut e,
                &feeds,
                NOW + 5000.0,
                json!({"index_price":"100","last_trade_price":"100"}),
            );
        }
        amend(&mut e, NOW + age);
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + age);
        assert_eq!(
            out.iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.contains("nextNonce"))),
            allowed,
            "{age}, {sparse}: {out:?}"
        );
    }
}
#[test]
fn every_unprovable_or_grouped_shape_refuses_before_cancel() {
    for field in [
        "parent_order_index",
        "parent_order_id",
        "to_trigger_order_id_0",
        "to_cancel_order_id_0",
        "order_flags",
        "order_expiry",
        "client_order_index",
    ] {
        let (mut e, feeds, _) = boot();
        let mut row = original();
        row.as_object_mut().unwrap().remove(field);
        message(
            &mut e,
            feeds.private,
            NOW + 1.0,
            &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
        );
        amend(&mut e, NOW + 1.0);
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.0);
        assert!(
            !out.iter().any(|a| matches!(a, Action::Http(_))),
            "{field}: {out:?}"
        );
        assert!(e.store().order("42").is_some());
    }
}
#[test]
fn cancel_nonce_retry_rechecks_mark_and_preserves_original() {
    let (mut e, feeds, mut t) = boot();
    let c = cancel(&mut e, NOW);
    reply_at(
        &mut e,
        NOW,
        &c,
        200,
        r#"{"code":21104,"message":"invalid nonce"}"#,
    );
    mark(&mut e, &feeds, NOW + 1.0, json!({"mark_price":"85"}));
    let out = advance(&mut e, &mut t, NOW + 150.0);
    let n = out
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("nextNonce") => Some(r.clone()),
            _ => None,
        })
        .unwrap_or_else(|| panic!("{out:?}"));
    reply_at(&mut e, NOW + 150.0, &n, 200, r#"{"code":200,"nonce":20}"#);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 150.0);
    assert!(
        !out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("sendTx"))),
        "{out:?}"
    );
    assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("trigger crossed"))),"{out:?}");
    assert!(e.store().order("42").is_some());
}
#[test]
fn final_claim_rechecks_stale_changed_and_crossed_after_signing() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    for cause in ["crossed", "changed", "position", "stale"] {
        let (mut e, feeds, _) = boot();
        let positions = e.store().positions().to_vec();
        amend(&mut e, NOW);
        let n = request(&mut e, NOW, "nextNonce");
        reply_at(&mut e, NOW, &n, 200, r#"{"code":200,"nonce":10}"#);
        e.prepare_dispatch(NOW);
        let out = e.drain_actions().collect::<Vec<_>>();
        let prep = out
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } if request.url.contains("sendTx") => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or_else(|| panic!("{out:?}"));
        e.http_prepared(
            NOW,
            prep,
            Prepared {
                id: prep.attempt().request.0,
                expires_at: None,
            },
        );
        let out = e.drain_actions().collect::<Vec<_>>();
        let permit = out
            .into_iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        match cause {
            "crossed" => mark(&mut e, &feeds, NOW + 1.0, json!({"mark_price":"85"})),
            "changed" => {
                e.store_mut().update_order("42", |o| o.remaining = 1.0);
            }
            "position" => e.store_mut().update_positions(|p| p.contracts = 0.0),
            _ => {}
        }
        let now = if cause == "stale" {
            NOW + 5001.0
        } else {
            NOW + 1.0
        };
        assert!(e.claim_http(now, permit).is_err(), "{cause}");
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        assert!(
            !out.iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.contains("sendTx"))),
            "{out:?}"
        );
        assert!(e.store().order("42").is_some());
        mark(&mut e, &feeds, now, json!({"mark_price":"100"}));
        e.store_mut().update_order("42", |o| o.remaining = 2.0);
        e.store_mut().set_positions(positions);
        e.handle(
            now,
            Event::Command(Command::AmendOrder {
                command: 9,
                id: "42".into(),
                price: Some(85.0),
                amount: None,
            }),
        );
        let n = request(&mut e, now, "nextNonce");
        reply_at(&mut e, now, &n, 200, r#"{"code":200,"nonce":20}"#);
        assert!(request(&mut e, now, "sendTx").url.contains("sendTx"));
    }
}

#[test]
fn all_mark_trigger_directions_allow_only_the_safe_side_including_equality() {
    for (kind, ask, trigger, safe, crossed) in [
        (2, true, 80.0, 85.0, 100.0),
        (2, false, 120.0, 115.0, 100.0),
        (4, true, 120.0, 115.0, 100.0),
        (4, false, 80.0, 85.0, 100.0),
    ] {
        for (price, allowed) in [(safe, true), (crossed, false)] {
            let (mut e, feeds, _) = boot();
            if !ask {
                e.store_mut()
                    .update_positions(|p| p.side = safe_cex_engine::model::PositionSide::Short);
            }
            let mut row = original();
            row["type"] = json!(kind);
            row["is_ask"] = json!(ask);
            row["trigger_price"] = json!(trigger.to_string());
            row["price"] = json!((if ask { trigger - 1.0 } else { trigger + 1.0 }).to_string());
            message(
                &mut e,
                feeds.private,
                NOW + 1.0,
                &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
            );
            e.handle(
                NOW + 1.0,
                Event::Command(Command::AmendOrder {
                    command: 8,
                    id: "42".into(),
                    price: Some(price),
                    amount: None,
                }),
            );
            let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 1.0);
            assert_eq!(
                out.iter()
                    .any(|a| matches!(a,Action::Http(r) if r.url.contains("nextNonce"))),
                allowed,
                "{kind}/{ask}/{price}: {out:?}"
            );
        }
    }
}

#[test]
fn known_group_parent_never_uses_the_ordinary_limit_edit_path() {
    let (mut e, feeds, _) = boot();
    let mut row = original();
    row["type"] = json!(0);
    row["reduce_only"] = json!(false);
    row["trigger_price"] = json!("0");
    row["to_trigger_order_id_0"] = json!("99");
    message(
        &mut e,
        feeds.private,
        NOW + 1.0,
        &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
    );
    message(
        &mut e,
        feeds.private,
        NOW + 2.0,
        r#"{"channel":"account_all_orders","orders":[{"order_index":"42","market_id":1,"price":"79","status":"open"}]}"#,
    );
    amend(&mut e, NOW + 2.0);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW + 2.0);
    assert!(!out.iter().any(|a| matches!(a, Action::Http(_))), "{out:?}");
    assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.message.contains("grouped or attached"))),"{out:?}");
}

#[test]
fn accepted_rest_mark_receipt_can_admit_without_a_websocket_refresh() {
    let (mut e, feeds, _) = ready_feeds(
        Options {
            testnet: true,
            ..options()
        },
        true,
    );
    message(
        &mut e,
        feeds.private,
        NOW,
        &json!({"channel":"account_all_orders","orders":[original()]}).to_string(),
    );
    amend(&mut e, NOW);
    let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
    assert!(
        out.iter()
            .any(|a| matches!(a,Action::Http(r) if r.url.contains("nextNonce"))),
        "{out:?}"
    );
}

#[test]
fn core_and_robinhood_refuse_crossed_protection_before_builder_or_cancel() {
    for robinhood in [false, true] {
        let (mut e, feeds, _) = ready_feeds(
            Options {
                robinhood,
                testnet: false,
                ..options()
            },
            true,
        );
        message(
            &mut e,
            feeds.private,
            NOW,
            &json!({"channel":"account_all_orders","orders":[original()]}).to_string(),
        );
        mark(&mut e, &feeds, NOW, json!({"mark_price":"85"}));
        amend(&mut e, NOW);
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
        assert!(
            !out.iter().any(|a| matches!(a, Action::Http(_))),
            "{robinhood}: {out:?}"
        );
        assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.outcome==Some(Outcome::NotSent)&&error.message.contains("trigger"))),"{out:?}");
    }
}

fn native_boot() -> (Engine<Lighter>, Feeds, Timers) {
    let (mut e, feeds, timers) = boot();
    let mut row = original();
    row["trigger_status"] = json!("mark-price");
    message(
        &mut e,
        feeds.private,
        NOW,
        &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
    );
    drain_at(&mut e, &mut vec![], NOW);
    (e, feeds, timers)
}
#[test]
fn untriggered_same_type_standalone_stop_modifies_in_place_without_cancel() {
    for robinhood in [false, true] {
        for (kind, ask, old_trigger, old_price, new_trigger, expected_price) in [
            (2, true, 80., 79.2, 70., 6930),
            (2, false, 120., 121.2, 130., 13130),
            (4, true, 120., 118.8, 110., 10890),
            (4, false, 80., 80.8, 90., 9090),
            (3, true, 80., 79., 85., 7900),
            (3, false, 120., 121., 115., 12100),
            (5, true, 120., 119., 125., 11900),
            (5, false, 80., 81., 75., 8100),
            (3, true, 80., 79., 79., 7900),
            (3, false, 120., 121., 121., 12100),
            (5, true, 120., 119., 119., 11900),
            (5, false, 80., 81., 81., 8100),
        ] {
            let (mut e, feeds, mut t) = ready_feeds(
                Options {
                    robinhood,
                    testnet: true,
                    ..options()
                },
                true,
            );
            let mut row = original();
            row["type"] = json!(kind);
            row["is_ask"] = json!(ask);
            row["trigger_status"] = json!("mark-price");
            row["trigger_price"] = json!(old_trigger.to_string());
            row["price"] = json!(old_price.to_string());
            e.store_mut().update_positions(|p| {
                p.side = if ask {
                    safe_cex_engine::model::PositionSide::Long
                } else {
                    safe_cex_engine::model::PositionSide::Short
                }
            });
            message(
                &mut e,
                feeds.private,
                NOW,
                &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
            );
            mark(&mut e, &feeds, NOW, json!({"mark_price":"100"}));
            drain_at(&mut e, &mut vec![], NOW);
            e.handle(
                NOW,
                Event::Command(Command::AmendOrder {
                    command: 8,
                    id: "42".into(),
                    price: Some(new_trigger),
                    amount: Some(1.),
                }),
            );
            let n = request(&mut e, NOW, "nextNonce");
            reply_at(&mut e, NOW, &n, 200, r#"{"code":200,"nonce":10}"#);
            let r = request(&mut e, NOW, "sendTx");
            assert_eq!(new_type(&r), 17);
            let wire = tx(&r);
            assert!(wire.get("Type").is_none());
            assert_eq!(wire["Index"], 42);
            assert_eq!(wire["BaseAmount"], 1000);
            assert_eq!(wire["Price"], expected_price, "kind={kind}, ask={ask}");
            assert_eq!(wire["TriggerPrice"], (new_trigger * 100.) as u64);
            ack(&mut e, &r, NOW);
            let out = advance(&mut e, &mut t, NOW + 1000.);
            assert!(!out.iter().any(|a|matches!(a,Action::Http(r) if r.url.contains("sendTx")||r.url.contains("accountOrders"))),"{out:?}");
            assert!(
                out.iter()
                    .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 8, .. }))),
                "{out:?}"
            );
        }
    }
}
fn new_type(r: &HttpRequest) -> u64 {
    r.body
        .as_ref()
        .unwrap()
        .split('&')
        .find_map(|s| s.strip_prefix("tx_type="))
        .unwrap()
        .parse()
        .unwrap()
}
#[test]
fn market_stop_edits_preserve_the_native_orders_slippage_on_both_paths() {
    for robinhood in [false, true] {
        for native_modify in [false, true] {
            for (kind, ask, old_trigger, bound, new_trigger, expected_price) in [
                (2, true, 80., 76., 70., 6650),
                (2, false, 120., 126., 130., 13650),
                (4, true, 120., 114., 110., 10450),
                (4, false, 80., 84., 90., 9450),
                (2, true, 80., 80., 70., 7000),
                (2, false, 120., 120., 130., 13000),
                (2, true, 80., 40., 70., 3500),
                (2, false, 120., 180., 130., 19500),
                (2, true, 80., 78.99, 70., 6912),
                (2, false, 120., 121.01, 130., 13109),
            ] {
                let (mut e, feeds, mut timers) = ready_feeds(
                    Options {
                        robinhood,
                        testnet: true,
                        stop_slippage_pct: 2.,
                        ..options()
                    },
                    true,
                );
                let mut row = original();
                row["type"] = json!(kind);
                row["is_ask"] = json!(ask);
                row["trigger_status"] = json!(if native_modify {
                    "mark-price"
                } else {
                    "untriggered"
                });
                row["trigger_price"] = json!(old_trigger.to_string());
                row["price"] = json!(bound.to_string());
                e.store_mut().update_positions(|p| {
                    p.side = if ask {
                        safe_cex_engine::model::PositionSide::Long
                    } else {
                        safe_cex_engine::model::PositionSide::Short
                    }
                });
                message(
                    &mut e,
                    feeds.private,
                    NOW,
                    &json!({"channel":"account_all_orders","orders":[row.clone()]}).to_string(),
                );
                mark(&mut e, &feeds, NOW, json!({"mark_price":"100"}));
                drain_at(&mut e, &mut vec![], NOW);
                e.handle(
                    NOW,
                    Event::Command(Command::AmendOrder {
                        command: 8,
                        id: "42".into(),
                        price: Some(new_trigger),
                        amount: None,
                    }),
                );
                if robinhood && !native_modify {
                    let tier = request(&mut e, NOW, "accountLimits");
                    reply_at(
                        &mut e,
                        NOW,
                        &tier,
                        200,
                        r#"{"code":200,"user_tier_name":"plus"}"#,
                    );
                    let approval = request(&mut e, NOW, "/account?");
                    reply_at(&mut e, NOW, &approval, 200, &json!({"code":200,"accounts":[{"approved_integrators":[{"account_index":47296,"approval_expiry":NOW as u64+100_000,"max_perps_taker_fee":300,"max_perps_maker_fee":0}]}]}).to_string());
                }
                let nonce = request(&mut e, NOW, "nextNonce");
                reply_at(&mut e, NOW, &nonce, 200, r#"{"code":200,"nonce":10}"#);
                let mut send = request(&mut e, NOW, "sendTx");
                if native_modify {
                    assert_eq!(new_type(&send), 17);
                } else {
                    assert_eq!(new_type(&send), 15);
                    let read = proof(&mut e, &mut timers, &send);
                    row["status"] = json!("canceled");
                    reply_at(
                        &mut e,
                        NOW + 100.,
                        &read,
                        200,
                        &json!({"code":200,"orders":[row]}).to_string(),
                    );
                    send = request(&mut e, NOW + 100., "sendTx");
                    assert_eq!(new_type(&send), 14);
                }
                let wire = tx(&send);
                assert_eq!(
                    wire["Price"], expected_price,
                    "RH={robinhood}, modify={native_modify}, kind={kind}, ask={ask}, bound={bound}"
                );
                assert_eq!(wire["TriggerPrice"], (new_trigger * 100.) as u64);
                assert_eq!(wire["BaseAmount"], 2000);
            }
        }
    }
}

#[test]
fn unsupported_original_slippage_refuses_before_any_write_and_keeps_the_stop() {
    for robinhood in [false, true] {
        for native_modify in [false, true] {
            for (ask, trigger, bound, new_trigger) in [
                (true, 80., 81., 70.),
                (false, 120., 119., 130.),
                (true, 80., 39.99, 70.),
                (false, 120., 180.01, 130.),
            ] {
                let (mut e, feeds, _) = ready_feeds(
                    Options {
                        robinhood,
                        testnet: true,
                        stop_slippage_pct: 2.,
                        ..options()
                    },
                    true,
                );
                let mut row = original();
                row["is_ask"] = json!(ask);
                row["trigger_status"] = json!(if native_modify {
                    "mark-price"
                } else {
                    "untriggered"
                });
                row["trigger_price"] = json!(trigger.to_string());
                row["price"] = json!(bound.to_string());
                e.store_mut().update_positions(|p| {
                    p.side = if ask {
                        safe_cex_engine::model::PositionSide::Long
                    } else {
                        safe_cex_engine::model::PositionSide::Short
                    }
                });
                message(
                    &mut e,
                    feeds.private,
                    NOW,
                    &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
                );
                mark(&mut e, &feeds, NOW, json!({"mark_price":"100"}));
                drain_at(&mut e, &mut vec![], NOW);
                let old = e.store().order("42").unwrap().clone();
                e.handle(
                    NOW,
                    Event::Command(Command::AmendOrder {
                        command: 8,
                        id: "42".into(),
                        price: Some(new_trigger),
                        amount: None,
                    }),
                );
                let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
                assert!(
                    !out.iter().any(|a| matches!(a, Action::Http(_))),
                    "RH={robinhood}, modify={native_modify}, ask={ask}, bound={bound}: {out:?}"
                );
                assert_eq!(e.store().order("42"), Some(&old));
                assert_eq!(old.status, safe_cex_engine::model::OrderStatus::Open);
                assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(Outcome::NotSent) && error.message == "Lighter stop not changed: original slippage bound unsupported; re-place it")), "{out:?}");
            }
        }
    }
}

#[test]
fn stop_limits_with_non_executable_kept_limits_refuse_before_any_write() {
    for (kind, ask, old_trigger, limit, new_trigger) in [
        (3, true, 80., 79., 70.),
        (3, false, 120., 121., 130.),
        (5, true, 120., 119., 110.),
        (5, false, 80., 81., 90.),
    ] {
        for status in ["mark-price", "untriggered", "pending", "na"] {
            let (mut e, feeds, _) = boot();
            let mut row = original();
            row["type"] = json!(kind);
            row["is_ask"] = json!(ask);
            row["trigger_status"] = json!(status);
            row["trigger_price"] = json!(old_trigger.to_string());
            row["price"] = json!(limit.to_string());
            e.store_mut().update_positions(|p| {
                p.side = if ask {
                    safe_cex_engine::model::PositionSide::Long
                } else {
                    safe_cex_engine::model::PositionSide::Short
                }
            });
            message(
                &mut e,
                feeds.private,
                NOW,
                &json!({"channel":"account_all_orders","orders":[row]}).to_string(),
            );
            drain_at(&mut e, &mut vec![], NOW);
            let old = e.store().order("42").unwrap().clone();
            e.handle(
                NOW,
                Event::Command(Command::AmendOrder {
                    command: 8,
                    id: "42".into(),
                    price: Some(new_trigger),
                    amount: None,
                }),
            );
            let out = safe_cex_engine::test_support::prepared_actions(&mut e, NOW);
            assert!(
                !out.iter().any(|a| matches!(a, Action::Http(_))),
                "{kind}/{ask}/{status}: {out:?}"
            );
            assert_eq!(e.store().order("42"), Some(&old));
            assert!(out.iter().any(|a| matches!(a, Action::Notice(Notice::CommandFailed { error, .. })
                if error.outcome == Some(Outcome::NotSent)
                    && if status == "mark-price" {
                        error.message.contains("limit") && error.message.contains("trigger") && error.message.contains("re-place")
                    } else {
                        error.message.contains("stop-limit edit is unsupported")
                    })), "{kind}/{ask}/{status}: {out:?}");
        }
    }
}

#[test]
fn native_modify_refusals_and_unknowns_never_cancel_restore_or_resend() {
    for (status, body, unknown) in [
        (404, r#"{"code":21702,"message":"not found"}"#, false),
        (200, r#"{"code":21702,"message":"refused"}"#, false),
        (200, r#"{"code":21104,"message":"invalid nonce"}"#, false),
        (429, r#"{"code":23000,"message":"rate"}"#, true),
        (405, r#"{"code":21104,"message":"rate"}"#, true),
        (200, r#"{"code":23000,"message":"rate"}"#, true),
        (200, r#"{"code":23100,"message":"rate"}"#, true),
        (500, r#"{"code":21702,"message":"refused"}"#, true),
        (408, r#"{"code":21104,"message":"timeout"}"#, true),
        (200, r#"{"code":200}"#, true),
        (200, "bad json", true),
    ] {
        let (mut e, _, mut t) = native_boot();
        let r = cancel(&mut e, NOW);
        assert_eq!(new_type(&r), 17);
        reply_at(&mut e, NOW, &r, status, body);
        let out = advance(&mut e, &mut t, NOW + 1000.);
        assert!(!out.iter().any(|a|matches!(a,Action::Http(r) if r.url.contains("sendTx")||r.url.contains("nextNonce"))),"{status}/{body}: {out:?}");
        assert!(e.store().order("42").is_some());
        assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{error,..}) if error.outcome==Some(if unknown{Outcome::Unknown}else{Outcome::Rejected})&&error.protection.is_none())),"{status}/{body}: {out:?}");
    }
}
#[test]
fn native_modify_keeps_the_original_amount_for_a_price_only_edit() {
    let (mut e, _, _) = native_boot();
    e.handle(
        NOW,
        Event::Command(Command::AmendOrder {
            command: 8,
            id: "42".into(),
            price: Some(85.),
            amount: None,
        }),
    );
    let n = request(&mut e, NOW, "nextNonce");
    reply_at(&mut e, NOW, &n, 200, r#"{"code":200,"nonce":10}"#);
    let r = request(&mut e, NOW, "sendTx");
    assert_eq!(new_type(&r), 17);
    assert_eq!(tx(&r)["BaseAmount"], 2000);
}

#[test]
fn native_modify_final_claim_rechecks_after_signing() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    for cause in ["crossed", "changed", "position", "stale"] {
        let (mut e, feeds, _) = native_boot();
        let positions = e.store().positions().to_vec();
        amend(&mut e, NOW);
        let n = request(&mut e, NOW, "nextNonce");
        reply_at(&mut e, NOW, &n, 200, r#"{"code":200,"nonce":10}"#);
        e.prepare_dispatch(NOW);
        let out = e.drain_actions().collect::<Vec<_>>();
        let prep = out
            .iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match a.as_ref() {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } if request.url.contains("sendTx") => Some(*preparation),
                    _ => None,
                },
                _ => None,
            })
            .unwrap_or_else(|| panic!("{out:?}"));
        e.http_prepared(
            NOW,
            prep,
            Prepared {
                id: prep.attempt().request.0,
                expires_at: None,
            },
        );
        let out = e.drain_actions().collect::<Vec<_>>();
        let permit = out
            .into_iter()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        match cause {
            "crossed" => mark(&mut e, &feeds, NOW + 1.0, json!({"mark_price":"85"})),
            "changed" => {
                e.store_mut().update_order("42", |o| o.remaining = 1.0);
            }
            "position" => e.store_mut().update_positions(|p| p.contracts = 0.0),
            _ => {}
        }
        let now = if cause == "stale" {
            NOW + 5001.0
        } else {
            NOW + 1.0
        };
        assert!(e.claim_http(now, permit).is_err(), "{cause}");
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        assert!(
            !out.iter()
                .any(|a| matches!(a,Action::Http(r) if r.url.contains("sendTx"))),
            "{out:?}"
        );
        assert!(e.store().order("42").is_some());
        mark(&mut e, &feeds, now, json!({"mark_price":"100"}));
        e.store_mut().update_order("42", |o| o.remaining = 2.0);
        e.store_mut().set_positions(positions);
        e.handle(
            now,
            Event::Command(Command::AmendOrder {
                command: 9,
                id: "42".into(),
                price: Some(85.0),
                amount: None,
            }),
        );
        let n = request(&mut e, now, "nextNonce");
        reply_at(&mut e, now, &n, 200, r#"{"code":200,"nonce":20}"#);
        assert!(request(&mut e, now, "sendTx").url.contains("sendTx"));
    }
}

#[test]
fn a_pre_modify_census_cannot_regrant_old_provenance_after_success_or_unknown() {
    for unknown in [false, true] {
        let (mut e, feeds, mut timers) = native_boot();
        let now = NOW + 6000.;
        let old_read = advance(&mut e, &mut timers, now)
            .into_iter()
            .find_map(|a| match a {
                Action::Http(r) if r.url.contains("accountActiveOrders") => Some(r),
                _ => None,
            })
            .expect("recurring order census");
        mark(&mut e, &feeds, now, json!({"mark_price":"100"}));
        let modify = cancel(&mut e, now);
        assert_eq!(new_type(&modify), 17);
        if unknown {
            reply_at(
                &mut e,
                now,
                &modify,
                429,
                r#"{"code":23000,"message":"rate"}"#,
            );
        } else {
            ack(&mut e, &modify, now);
        }
        let mut row = original();
        row["trigger_status"] = json!("mark-price");
        row["updated_at"] = json!(e.store().order("42").unwrap().last_update);
        reply_at(
            &mut e,
            now + 1.,
            &old_read,
            200,
            &json!({"code":200,"orders":[row]}).to_string(),
        );
        let _ = advance(&mut e, &mut timers, now + 1.);
        e.handle(
            now + 2.,
            Event::Command(Command::AmendOrder {
                command: 9,
                id: "42".into(),
                price: Some(84.),
                amount: None,
            }),
        );
        let out = safe_cex_engine::test_support::prepared_actions(&mut e, now + 2.);
        assert!(!out.iter().any(|a|matches!(a,Action::Http(r) if r.url.contains("sendTx")||r.url.contains("nextNonce"))),"{unknown}: {out:?}");
        assert!(out.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:9,error}) if (error.message.contains("original unavailable") || (unknown && error.message.contains("outcome is uncertain")))&&error.outcome==Some(Outcome::NotSent))),"{unknown}: {out:?}");
    }
}

#[path = "protective_edits/reference_preflight.rs"]
mod reference_preflight;
