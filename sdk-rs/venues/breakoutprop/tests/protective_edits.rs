//! Conditional native protective edits never remove the old protection first.
mod support;
use safe_cex_engine::{Command, Event, Method, Outcome, Response};
use serde_json::{Value, json};
use support::*;

fn native(kind: &str, side: &str, quantity: f64) -> Value {
    json!({"account":"default:00002","orderId":42,"orderCode":"protect","clientOrderId":"client-protect","actionCode":"action-1","version":1,"legCount":1,"type":kind,"instrument":"ETHUSD","status":"WORKING","finalStatus":false,"side":side,"tif":"GTC","transactionTime":"2024-12-11T07:55:11Z","issueTime":"2024-12-11T07:55:11Z","links":[],"legs":[{"instrument":"ETHUSD","side":side,"positionEffect":"CLOSE","quantity":quantity,"filledQuantity":0,"remainingQuantity":quantity,"price":3650}],"stopPrice":if kind=="STOP" {Some(if side=="SELL" {3650.}else{3660.})}else{None},"limitPrice":if kind=="LIMIT" {Some(if side=="SELL" {3660.}else{3650.})}else{None}})
}
fn portfolio(row: &Value) -> String {
    let side = if row["side"] == "SELL" { "BUY" } else { "SELL" };
    json!({"portfolios":[{"account":"default:00002","version":1,"balances":[],"positions":[{"account":"default:00002","symbol":"ETHUSD","side":side,"positionCode":"pos","quantity":10,"quantityNotional":36560,"openPrice":3656,"marginRate":0.2,"lastUpdateTime":"2024-12-11T07:55:11Z"}],"orders":[row]}]}).to_string()
}
fn fixture(row: &Value) -> (Rig, f64) {
    let mut r = Rig::new(safe_cex_venue_breakoutprop::Options {
        key: "example-user".into(),
        secret: "example-password".into(),
        tick_interval: 1_000_000.,
        ..Default::default()
    });
    r.boot();
    for _ in 0..2 {
        let p = r.next_request();
        assert!(p.url.ends_with("/portfolio"));
        r.reply(&p, 200, &portfolio(row));
    }
    let q = r.next_request();
    assert!(q.url.ends_with("/marketdata"));
    r.reply(&q, 200, QUOTES);
    let at = r.now;
    (r, at)
}
fn reply(r: &mut Rig, req: &safe_cex_engine::HttpRequest, status: u16, headers: &str, body: &str) {
    r.now += 1.;
    r.engine.handle(
        r.now,
        Event::HttpResponse {
            request: req.id,
            response: Response {
                status,
                headers,
                body,
            },
        },
    );
    r.collect();
}
fn amend(r: &mut Rig, price: f64, amount: Option<f64>) -> safe_cex_engine::HttpRequest {
    r.command(Command::AmendOrder {
        command: 91,
        id: "protect".into(),
        price: Some(price),
        amount,
    });
    let q = r.next_request();
    assert_eq!(q.method, Method::Get);
    assert!(
        q.url.ends_with("/accounts/default%3A00002/orders"),
        "{} {:?}",
        q.url,
        r.failures
    );
    q
}
fn inspect(r: &mut Rig, q: &safe_cex_engine::HttpRequest, row: &Value) {
    reply(
        r,
        q,
        200,
        "eTaG: \"orders-1\"",
        &json!({"orders":[row]}).to_string(),
    );
}
#[test]
fn native_stop_and_closing_tp_edits_preserve_provenance_without_cancel() {
    for kind in ["STOP", "LIMIT"] {
        for side in ["BUY", "SELL"] {
            for amount in [None, Some(2.)] {
                let mut row = native(kind, side, 1.);
                row["metadata"] = json!({"desk":"original"});
                row["tif"] = json!("GTD");
                row["expireDate"] = json!("2028-01-01T00:00:00Z");
                let (mut r, _) = fixture(&row);
                r.now += 1000.;
                let price = if (kind == "STOP") == (side == "BUY") {
                    3670.
                } else {
                    3640.
                };
                let q = amend(&mut r, price, amount);
                inspect(&mut r, &q, &row);
                let p = r.next_request();
                assert_eq!(p.method, Method::Put);
                assert_eq!(
                    p.headers
                        .iter()
                        .find(|(k, _)| k.eq_ignore_ascii_case("if-match"))
                        .unwrap()
                        .1,
                    "\"orders-1\""
                );
                let b: Value = serde_json::from_str(p.body.as_deref().unwrap()).unwrap();
                assert_eq!(b["orderCode"], "protect");
                assert_eq!(b["quantity"], amount.unwrap_or(1.));
                assert_eq!(b["side"], side);
                assert_eq!(b["positionEffect"], "CLOSE");
                assert_eq!(b["tif"], "GTD");
                assert_eq!(b["metadata"], row["metadata"]);
                assert_eq!(b["expireDate"], row["expireDate"]);
                assert_eq!(
                    b[if kind == "STOP" {
                        "stopPrice"
                    } else {
                        "limitPrice"
                    }],
                    price
                );
                assert!(b.get("type").is_none());
                reply(&mut r, &p, 200, "", r#"{"orderId":42,"updateOrderId":43}"#);
                assert_eq!(r.result(91), Ok("[\"protect\"]".into()));
            }
        }
    }
}
#[test]
fn bbo_direction_and_local_receipt_boundary_are_final_native_admission() {
    for (kind, side) in [
        ("STOP", "SELL"),
        ("STOP", "BUY"),
        ("LIMIT", "BUY"),
        ("LIMIT", "SELL"),
    ] {
        for age in [4999., 5000., 5001.] {
            for crossed in [false, true] {
                let row = native(kind, side, 1.);
                let (mut r, at) = fixture(&row);
                r.now = at + age - 1.;
                let below = (kind == "STOP") == (side == "SELL");
                // The display substitutes must not authorize the final native send.
                let mut display = r.engine.store().ticker("ETHUSD").unwrap().clone();
                display.last = if below { 4000. } else { 3500. };
                display.mark = display.last;
                display.index = display.last;
                r.engine.store_mut().set_tickers(vec![display]);
                let price = if crossed {
                    if below { 3656.01 } else { 3656.02 }
                } else if below {
                    3640.
                } else {
                    3670.
                };
                let q = amend(&mut r, price, None);
                inspect(&mut r, &q, &row);
                let allowed = age <= 5000. && !crossed;
                assert_eq!(
                    r.requests.iter().any(|q| q.method == Method::Put),
                    allowed,
                    "{kind} {side} {age} {crossed}"
                );
                assert!(!r.requests.iter().any(|q| q.method == Method::Delete));
                if !allowed {
                    let e = r.failures.last().unwrap();
                    assert_eq!(e.outcome, Some(Outcome::NotSent));
                    assert!(
                        e.message.contains(if age > 5000. {
                            "reference stale"
                        } else {
                            "crossed"
                        }),
                        "{e:?}"
                    );
                    assert_eq!(
                        r.engine.store().order("protect").unwrap().status,
                        safe_cex_engine::model::OrderStatus::Open
                    );
                }
            }
        }
    }
}
#[test]
fn native_refusal_never_falls_back_and_rate_limit_never_claims_not_sent() {
    for status in [404, 409, 412, 429, 500] {
        let row = native("STOP", "SELL", 1.);
        let (mut r, _) = fixture(&row);
        r.now += 1000.;
        let q = amend(&mut r, 3640., None);
        inspect(&mut r, &q, &row);
        let p = r.next_request();
        assert_eq!(p.method, Method::Put);
        reply(
            &mut r,
            &p,
            status,
            "",
            r#"{"errorCode":33,"description":"refused"}"#,
        );
        assert!(
            !r.requests
                .iter()
                .any(|q| matches!(q.method, Method::Delete | Method::Post))
        );
        let e = r.failures.last().unwrap();
        assert_eq!(
            e.outcome,
            Some(if status == 429 || status >= 500 {
                Outcome::Unknown
            } else {
                Outcome::NotSent
            })
        );
        assert!(e.protection.is_none());
        if status == 429 || status >= 500 {
            assert!(e.message.contains("Open Orders"));
        }
    }
}
#[test]
fn missing_or_grouped_native_provenance_never_reaches_a_write() {
    for mutation in [
        "etag",
        "links",
        "field",
        "filled",
        "changed",
        "identity",
        "client",
        "duplicate",
    ] {
        let row = native("STOP", "SELL", 1.);
        let (mut r, _) = fixture(&row);
        r.now += 1000.;
        let q = amend(&mut r, 3640., None);
        let mut changed = row.clone();
        match mutation {
            "links" => changed["links"] = json!([{"linkType":"OCO","linkedOrder":"peer"}]),
            "field" => {
                changed.as_object_mut().unwrap().remove("tif");
            }
            "filled" => changed["legs"][0]["filledQuantity"] = json!(0.5),
            "changed" => changed["stopPrice"] = json!(3649.),
            "identity" => changed["orderCode"] = json!("different-chain"),
            "client" => changed["clientOrderId"] = json!("different-client"),
            _ => {}
        }
        reply(
            &mut r,
            &q,
            200,
            if mutation == "etag" {
                ""
            } else {
                "ETag: \"orders-1\""
            },
            &json!({"orders":if mutation=="duplicate" {vec![changed.clone(),changed]}else{vec![changed]}}).to_string(),
        );
        assert!(
            !r.requests
                .iter()
                .any(|q| matches!(q.method, Method::Put | Method::Delete | Method::Post)),
            "{mutation}"
        );
        assert_eq!(r.failures.last().unwrap().outcome, Some(Outcome::NotSent));
    }
}
#[test]
fn full_position_price_edit_preserves_zero_and_native_position_code() {
    let mut row = native("STOP", "SELL", 0.);
    row["legs"][0]["positionCode"] = json!("pos");
    for amount in [None, Some(0.)] {
        let (mut r, _) = fixture(&row);
        r.now += 1000.;
        let q = amend(&mut r, 3640., amount);
        inspect(&mut r, &q, &row);
        let p = r.next_request();
        assert_eq!(p.method, Method::Put);
        let b: Value = serde_json::from_str(p.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["positionCode"], "pos");
        assert_eq!(b["quantity"], 0.);
        assert_eq!(b["positionEffect"], "CLOSE");
    }
}

#[test]
fn re_login_replay_rechecks_reference_and_store_without_a_second_write() {
    for changed in ["healthy", "stale", "filled", "removed", "position"] {
        let row = native("STOP", "SELL", 1.);
        let (mut r, at) = fixture(&row);
        r.now += 1000.;
        let q = amend(&mut r, 3640., None);
        inspect(&mut r, &q, &row);
        let p = r.next_request();
        assert_eq!(p.method, Method::Put);
        reply(&mut r, &p, 401, "", r#"{"errorCode":1}"#);
        let login = r.next_request();
        assert!(login.url.ends_with("/login"));
        match changed {
            "stale" => r.now = at + 5001.,
            "filled" => {
                let mut o = r.engine.store().order("protect").unwrap().clone();
                o.filled = 0.1;
                r.engine.store_mut().upsert_order(o);
            }
            "removed" => r.engine.store_mut().set_orders(vec![]),
            "position" => r.engine.store_mut().set_positions(vec![]),
            _ => {}
        }
        reply(&mut r, &login, 200, "", LOGIN);
        if changed == "healthy" {
            let replay = r.next_request();
            assert_eq!(replay.method, Method::Put);
            assert_eq!(replay.body, p.body);
            assert_eq!(replay.headers, p.headers);
            reply(&mut r, &replay, 200, "", r#"{"orderId":42}"#);
            assert!(r.result(91).is_ok());
        } else {
            assert!(
                !r.requests
                    .iter()
                    .any(|q| matches!(q.method, Method::Put | Method::Delete | Method::Post)),
                "{changed}"
            );
            assert_eq!(r.failures.last().unwrap().outcome, Some(Outcome::NotSent));
        }
    }
}

#[test]
fn deferred_native_send_rechecks_freshness_and_releases_the_amend_lock() {
    let row = native("STOP", "SELL", 1.);
    let (mut r, at) = fixture(&row);
    r.now = at + 4500.;
    for i in 0..4 {
        r.command(Command::PlaceOrders {
            command: 100 + i,
            orders: vec![
                serde_json::from_value(
                    json!({"symbol":"ETHUSD","type":"limit","side":"buy","amount":1,"price":3500}),
                )
                .unwrap(),
            ],
        });
        let p = r.next_request();
        assert_eq!(p.method, Method::Post);
    }
    let q = amend(&mut r, 3640., None);
    r.now = at + 4998.;
    inspect(&mut r, &q, &row);
    assert!(!r.requests.iter().any(|q| q.method == Method::Put));
    r.fire_next_timer();
    assert_eq!(r.failures.last().unwrap().outcome, Some(Outcome::NotSent));
    assert!(
        r.failures
            .last()
            .unwrap()
            .message
            .contains("reference stale")
    );
    assert!(
        !r.requests
            .iter()
            .any(|q| q.method == Method::Put || q.method == Method::Delete)
    );
    // A second amend reaches inspection rather than a stuck mutation lock.
    r.command(Command::AmendOrder {
        command: 92,
        id: "protect".into(),
        price: Some(3640.),
        amount: None,
    });
    assert_eq!(r.next_request().method, Method::Get);
}

#[test]
fn inspection_delay_and_changes_cannot_authorize_native_write() {
    for change in ["filled", "changed", "removed", "position", "late"] {
        let row = native("STOP", "SELL", 1.);
        let (mut r, at) = fixture(&row);
        r.now += 1000.;
        let q = amend(&mut r, 3640., None);
        match change {
            "filled" | "changed" => {
                let mut o = r.engine.store().order("protect").unwrap().clone();
                if change == "filled" {
                    o.filled = 0.1;
                } else {
                    o.price = 3649.;
                }
                r.engine.store_mut().upsert_order(o);
            }
            "removed" => r.engine.store_mut().set_orders(vec![]),
            "position" => r.engine.store_mut().set_positions(vec![]),
            "late" => r.now = at + 5001.,
            _ => unreachable!(),
        }
        inspect(&mut r, &q, &row);
        assert!(
            !r.requests
                .iter()
                .any(|q| matches!(q.method, Method::Put | Method::Delete | Method::Post)),
            "{change}"
        );
        assert_eq!(r.failures.last().unwrap().outcome, Some(Outcome::NotSent));
    }
}

#[test]
fn weak_duplicate_and_invalid_etags_never_authorize_modify() {
    for headers in [
        "ETag: *",
        "ETag: W/\"weak\"",
        "ETag: \"first\"\r\nETag: \"second\"",
        "ETag: \"bad tag\"",
        "ETag: \"a\",\"b\"",
    ] {
        let row = native("STOP", "SELL", 1.);
        let (mut r, _) = fixture(&row);
        r.now += 1000.;
        let q = amend(&mut r, 3640., None);
        reply(
            &mut r,
            &q,
            200,
            headers,
            &json!({"orders":[row]}).to_string(),
        );
        assert!(r.requests.is_empty());
        assert_eq!(r.failures.last().unwrap().outcome, Some(Outcome::NotSent));
    }
}

#[test]
fn unproven_native_ack_or_transport_never_repeats_or_claims_protection() {
    for ack in [
        None,
        Some(r#"{}"#),
        Some(r#"{"orderId":43}"#),
        Some(r#"{"orderId":0}"#),
    ] {
        let row = native("STOP", "SELL", 1.);
        let (mut r, _) = fixture(&row);
        r.now += 1000.;
        let q = amend(&mut r, 3640., None);
        inspect(&mut r, &q, &row);
        let p = r.next_request();
        if let Some(ack) = ack {
            reply(&mut r, &p, 200, "", ack);
        } else {
            r.failure(&p);
        }
        assert!(r.requests.is_empty());
        let e = r.failures.last().unwrap();
        assert_eq!(e.outcome, Some(Outcome::Unknown));
        assert!(e.message.contains("Open Orders"));
        assert!(e.protection.is_none());
    }
}

#[test]
fn linked_full_position_size_cannot_be_silently_converted_to_fixed_quantity() {
    let mut row = native("STOP", "SELL", 0.);
    row["legs"][0]["positionCode"] = json!("pos");
    let (mut r, _) = fixture(&row);
    r.now += 1000.;
    let q = amend(&mut r, 3640., Some(2.));
    inspect(&mut r, &q, &row);
    assert!(r.requests.is_empty());
    assert_eq!(r.failures.last().unwrap().outcome, Some(Outcome::NotSent));
    assert!(
        r.failures
            .last()
            .unwrap()
            .message
            .contains("position-linked")
    );
}

#[test]
fn documented_standalone_omissions_preserve_exact_native_controls() {
    for client in ["client-protect", ""] {
        let mut row = native("STOP", "SELL", 1.);
        row["clientOrderId"] = json!(client);
        let (mut r, _) = fixture(&row);
        row.as_object_mut().unwrap().remove("links");
        row["legs"][0].as_object_mut().unwrap().remove("side");
        r.now += 1000.;
        let q = amend(&mut r, 3640., None);
        inspect(&mut r, &q, &row);
        let p = r.next_request();
        assert_eq!(p.method, Method::Put);
        let b: Value = serde_json::from_str(p.body.as_deref().unwrap()).unwrap();
        assert_eq!(b["side"], "SELL");
        assert_eq!(b["quantity"], 1.);
    }
}

#[test]
fn sparse_or_failed_quote_poll_does_not_refresh_the_display_substitutes() {
    for quote in [
        None,
        Some(
            r#"{"events":[{"symbol":"ETHUSD","type":"Quote","bid":3656.01,"time":"2024-12-11T07:55:11Z"}]}"#,
        ),
        Some(r#"{"events":[]}"#),
    ] {
        let row = native("STOP", "SELL", 1.);
        let (mut r, _) = fixture(&row);
        let display = r.engine.store().ticker("ETHUSD").unwrap().clone();
        r.fire_next_timer();
        let p = r.next_request();
        assert!(p.url.ends_with("/portfolio"));
        reply(&mut r, &p, 200, "", &portfolio(&row));
        let q = r.next_request();
        assert!(q.url.ends_with("/marketdata"));
        if let Some(body) = quote {
            reply(&mut r, &q, 200, "", body);
        } else {
            reply(&mut r, &q, 500, "", "");
        }
        // A host-visible substitute cannot manufacture a receipt.
        r.engine.store_mut().set_tickers(vec![display]);
        let q = amend(&mut r, 3640., None);
        inspect(&mut r, &q, &row);
        assert!(
            !r.requests
                .iter()
                .any(|q| matches!(q.method, Method::Put | Method::Delete))
        );
        assert!(
            r.failures
                .last()
                .unwrap()
                .message
                .contains("reference stale")
        );
    }
}

#[test]
fn explicit_tp_uses_closing_limit_direction_and_never_changes_native_class() {
    for side in ["BUY", "SELL"] {
        for kind in ["LIMIT", "STOP"] {
            let row = native(kind, side, 1.);
            let (mut r, _) = fixture(&row);
            let mut o = r.engine.store().order("protect").unwrap().clone();
            o.kind = safe_cex_engine::model::OrderType::TakeProfit;
            r.engine.store_mut().upsert_order(o);
            r.now += 1000.;
            let q = amend(&mut r, if side == "BUY" { 3640. } else { 3670. }, None);
            inspect(&mut r, &q, &row);
            if kind == "LIMIT" {
                assert_eq!(r.next_request().method, Method::Put);
            } else {
                assert!(r.requests.is_empty());
                assert_eq!(r.failures.last().unwrap().outcome, Some(Outcome::NotSent));
            }
        }
    }
}
