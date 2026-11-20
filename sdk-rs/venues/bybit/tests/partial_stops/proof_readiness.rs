//! Unrelated normal orders have no trigger source; the edited stop must have one.
use super::*;

fn peer(h: &Harness, trigger: Option<serde_json::Value>) -> serde_json::Value {
    let mut row = serde_json::json!({
        "orderId":"ordinary-peer", "symbol":"BTCUSDT", "side":"Buy",
        "orderStatus":"New", "qty":"1", "cumExecQty":"0", "triggerPrice":"",
        "orderType":"Limit", "price":"80", "positionIdx":0, "stopOrderType":"",
        "tpslMode":"", "orderLinkId":"peer-link", "parentOrderLinkId":"",
        "createdTime":((h.now as u64) - 1).to_string()
    });
    if let Some(trigger) = trigger {
        row["triggerBy"] = trigger;
    }
    row
}

fn page(h: &mut Harness, path: &str, rows: Vec<serde_json::Value>) {
    h.respond(
        path,
        200,
        &serde_json::json!({
            "retCode":0, "result":{"category":"linear", "list":rows, "nextPageCursor":""}
        })
        .to_string(),
    );
}

fn frame_fed(kind: &str) -> Harness {
    let profile = PROFILES[0];
    let mut source = ready(profile);
    seed(&mut source, profile, "Sell", kind, "MarkPrice", false, true);
    // The real normal-order mapper already admits this non-trigger row.
    let normal = peer(&source, None);
    source.message(
        "/v5/private",
        &serde_json::json!({"topic":"order", "data":[normal]}).to_string(),
    );
    assert!(source.engine.store().order("ordinary-peer").is_some());
    let snapshot = source.engine.snapshot_frame(source.now).to_vec();
    let mut engine = support::engine(Options {
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        ..Options::default()
    });
    engine.set_frame_fed().unwrap();
    let mut h = Harness::started_engine(engine);
    h.respond("status=PreLaunch", 200, support::PRELAUNCH);
    h.respond("/v5/market/instruments-info", 200, support::INSTRUMENTS);
    h.engine.apply_frame(h.now, &snapshot, true).unwrap();
    h.collect();
    assert!(h.requests.is_empty());
    h
}

fn original(h: &Harness) -> serde_json::Value {
    let order = h.engine.store().order("partial").unwrap();
    let mut row = native_partial_target(h, PROFILES[0], order);
    row["orderStatus"] = "Untriggered".into();
    row
}

fn start(h: &mut Harness, kind: &str) {
    amend(h, Some(if kind == "StopLoss" { 95.0 } else { 115.0 }), None);
    assert!(!h.has_request("/v5/order/cancel"));
    assert!(h.has_request("&orderId=partial"), "{:?}", h.result(91));
}

fn finish(h: &mut Harness, kind: &str, trigger: Option<serde_json::Value>) {
    let mut terminal = original(h);
    terminal["orderStatus"] = "Cancelled".into();
    h.respond(PROFILES[0].cancel_path(), 200, r#"{"retCode":0,"result":{"list":[{"orderId":"partial"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    page(h, "&orderId=partial", vec![terminal]);
    let row = peer(h, trigger);
    page(h, "&openOnly=0&limit=50", vec![row.clone()]);
    page(h, "/v5/order/history", vec![row]);
    let replacement = h.take("/v5/position/trading-stop");
    let (field, reference, size, price) = if kind == "StopLoss" {
        ("stopLoss", "slTriggerBy", "slSize", "95")
    } else {
        ("takeProfit", "tpTriggerBy", "tpSize", "115")
    };
    let mut expected = serde_json::json!({"category":"linear", "symbol":"BTCUSDT", "positionIdx":0, "tpslMode":"Partial"});
    expected[field] = price.into();
    expected[reference] = "MarkPrice".into();
    expected[size] = "7".into();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body(&replacement)).unwrap(),
        expected
    );
    h.reply(replacement.id, 200, OK);
    assert!(matches!(h.result(91), Some(Ok(_))), "{:?}", h.result(91));
    assert!(h.requests.is_empty());
}

fn edit_with_peer(axis: usize, trigger: Option<serde_json::Value>) {
    for kind in ["StopLoss", "TakeProfit"] {
        let mut h = frame_fed(kind);
        let target = original(&h);
        let row = peer(&h, trigger.clone());
        start(&mut h, kind);
        for (index, (path, mut rows)) in [
            ("&orderId=partial", vec![target]),
            ("&openOnly=0&limit=50", vec![]),
            ("/v5/order/history", vec![]),
        ]
        .into_iter()
        .enumerate()
        {
            if index == axis {
                rows.push(row.clone());
            }
            page(&mut h, path, rows);
            assert!(
                h.result(91).is_none(),
                "{kind}, axis={axis}: {:?}",
                h.result(91)
            );
        }
        assert!(h.has_request(PROFILES[0].cancel_path()));
        finish(&mut h, kind, trigger.clone());
    }
}

#[test]
fn live_census_without_an_unrelated_orders_trigger_source_completes_the_edit() {
    edit_with_peer(1, None);
}

#[test]
fn history_census_without_an_unrelated_orders_trigger_source_completes_the_edit() {
    edit_with_peer(2, None);
}

#[test]
fn live_census_with_an_unrelated_null_trigger_source_completes_the_edit() {
    edit_with_peer(1, Some(serde_json::Value::Null));
}

#[test]
fn history_census_with_an_unrelated_null_trigger_source_completes_the_edit() {
    edit_with_peer(2, Some(serde_json::Value::Null));
}

#[test]
fn explicit_empty_unrelated_trigger_source_is_a_working_control() {
    for axis in [1, 2] {
        edit_with_peer(axis, Some("".into()));
    }
}

#[test]
fn edited_stop_still_requires_its_exact_known_trigger_source_on_every_axis() {
    for axis in 0..3 {
        for trigger in [
            None,
            Some(serde_json::Value::Null),
            Some("LastPrice".into()),
            Some(1.into()),
        ] {
            let mut h = frame_fed("StopLoss");
            let mut target = original(&h);
            match trigger {
                None => {
                    target.as_object_mut().unwrap().remove("triggerBy");
                }
                Some(trigger) => target["triggerBy"] = trigger,
            }
            let healthy = original(&h);
            let order = h.engine.store().order("partial").unwrap().clone();
            start(&mut h, "StopLoss");
            for (index, path) in [
                "&orderId=partial",
                "&openOnly=0&limit=50",
                "/v5/order/history",
            ]
            .into_iter()
            .enumerate()
            {
                page(
                    &mut h,
                    path,
                    vec![if index == axis {
                        target.clone()
                    } else {
                        healthy.clone()
                    }],
                );
                if index == axis {
                    break;
                }
            }
            assert_eq!(h.engine.store().order("partial"), Some(&order));
            assert!(
                !h.requests
                    .iter()
                    .any(|r| r.method == safe_cex_engine::Method::Post)
            );
            assert!(
                h.notices.iter().any(|notice| matches!(notice,
                    safe_cex_engine::Notice::CommandFailed { command:91, error }
                    if error.kind == Some(safe_cex_engine::ErrorKind::Network)
                        && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
                )),
                "axis={axis}: {:?}",
                h.result(91)
            );
        }
    }
}

fn replacement_after_peer_census(missing_lineage: bool) -> Harness {
    let mut h = ready(PROFILES[0]);
    seed(
        &mut h,
        PROFILES[0],
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    let mut terminal = original(&h);
    terminal["orderStatus"] = "Cancelled".into();
    amend(&mut h, Some(95.0), None);
    h.respond(PROFILES[0].cancel_path(), 200, r#"{"retCode":0,"result":{"list":[{"orderId":"partial"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    page(&mut h, "&orderId=partial", vec![terminal]);
    let mut normal = peer(&h, None);
    if missing_lineage {
        normal.as_object_mut().unwrap().remove("parentOrderLinkId");
    }
    page(&mut h, "&openOnly=0&limit=50", vec![normal.clone()]);
    assert!(
        h.has_request("/v5/order/history"),
        "ordinary peer failed: {:?}",
        h.result(91)
    );
    page(&mut h, "/v5/order/history", vec![normal]);
    h.advance(200.0);
    assert!(
        h.has_request("/v5/position/trading-stop"),
        "{:?}",
        h.result(91)
    );
    h
}

#[test]
fn unrelated_orders_trigger_source_is_not_required_for_proven_standalone_put_back() {
    let mut h = replacement_after_peer_census(false);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    let restore = h.take("/v5/position/trading-stop");
    let payload: serde_json::Value = serde_json::from_str(body(&restore)).unwrap();
    assert_eq!(payload["stopLoss"], "90");
    assert_eq!(payload["slTriggerBy"], "MarkPrice");
    assert_eq!(payload["slSize"], "7");
    h.reply(restore.id, 200, OK);
    assert_applied_restored_partial_failure(&h);
}

/// AGT-34: was `missing_unrelated_trigger_source_does_not_waive_peer_lineage`, which
/// pinned NO put-back without lineage and so left no stop. Lineage now only words it.
#[test]
fn missing_peer_lineage_still_puts_the_old_stop_back_as_standalone() {
    let mut h = replacement_after_peer_census(true);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    h.advance(200.0);
    let restore = h.take("/v5/position/trading-stop");
    let payload: serde_json::Value = serde_json::from_str(body(&restore)).unwrap();
    assert_eq!(payload["stopLoss"], "90");
    h.reply(restore.id, 200, OK);
    assert!(h.notices.iter().any(|notice| matches!(notice,
        safe_cex_engine::Notice::CommandFailed { command:91, error }
        if error.protection == Some(safe_cex_engine::error::Protection::Restored)
            && error.message.contains("standalone")
    )));
}

/// AGT-34: was `..._never_authorizes_replacement`. The missing source still fails the
/// proof, but after the landed cancel the replacement goes out instead of no stop.
#[test]
fn cancelled_target_without_its_trigger_source_fails_the_proof_but_sends_the_replacement() {
    let mut h = ready(PROFILES[0]);
    seed(
        &mut h,
        PROFILES[0],
        "Sell",
        "StopLoss",
        "MarkPrice",
        false,
        true,
    );
    let mut terminal = original(&h);
    terminal["orderStatus"] = "Cancelled".into();
    terminal.as_object_mut().unwrap().remove("triggerBy");
    amend(&mut h, Some(95.0), None);
    h.respond(PROFILES[0].cancel_path(), 200, r#"{"retCode":0,"result":{"list":[{"orderId":"partial"}]},"retExtInfo":{"list":[{"code":0,"msg":"OK"}]}}"#);
    page(&mut h, "&orderId=partial", vec![terminal.clone()]);
    // One final orderId read: the old stop's own row, cancelled, proves it gone.
    h.advance(200.0);
    page(&mut h, "&orderId=partial", vec![terminal]);
    h.advance(200.0);
    assert!(h.unknown.is_empty());
    let posts: Vec<_> = h
        .requests
        .iter()
        .filter(|r| r.method == safe_cex_engine::Method::Post)
        .map(|r| r.url.clone())
        .collect();
    assert_eq!(posts.len(), 1, "{posts:?}");
    assert!(posts[0].contains("/v5/position/trading-stop"), "{posts:?}");
}

/// The cursor live Bybit returns beside the single row of an orderId-filtered read.
fn live_cursor(h: &Harness) -> String {
    format!("partial%3A{0}%2Cpartial%3A{0}", h.now as u64)
}

fn cursor_page(h: &mut Harness, path: &str, rows: Vec<serde_json::Value>) {
    let cursor = live_cursor(h);
    h.respond(
        path,
        200,
        &serde_json::json!({
            "retCode":0, "result":{"category":"linear", "list":rows, "nextPageCursor":cursor}
        })
        .to_string(),
    );
}

/// AGT-108: readiness kept the `target-cursor` refusal AGT-34 removed from the
/// post-cancel proof. Live Bybit (probed 2026-11-20 on a live Partial stop) sends a
/// cursor beside the single orderId row, so every frame-fed Partial edit was refused.
#[test]
fn live_target_cursor_beside_the_single_row_does_not_refuse_the_edit() {
    for kind in ["StopLoss", "TakeProfit"] {
        let mut h = frame_fed(kind);
        let target = original(&h);
        start(&mut h, kind);
        for (path, rows) in [
            ("&orderId=partial", vec![target.clone()]),
            ("&openOnly=0&limit=50", vec![target.clone()]),
            ("/v5/order/history", vec![]),
        ] {
            cursor_page(&mut h, path, rows);
            assert!(h.result(91).is_none(), "{kind}, {path}: {:?}", h.result(91));
        }
        assert!(h.has_request(PROFILES[0].cancel_path()), "{kind}");
        finish(&mut h, kind, None);
    }
}

fn refused_with(h: &Harness, kind: &str, expected: &str) {
    assert!(
        h.notices.iter().any(|notice| matches!(notice,
            safe_cex_engine::Notice::CommandFailed { command:91, error }
            if error.message == expected
                && error.kind == Some(safe_cex_engine::ErrorKind::Network)
                && error.outcome == Some(safe_cex_engine::Outcome::NotSent)
        )),
        "{kind}: {:?}",
        h.result(91)
    );
    assert!(!h.has_request(PROFILES[0].cancel_path()), "{kind}");
    assert!(
        !h.requests
            .iter()
            .any(|r| r.method == safe_cex_engine::Method::Post),
        "{kind}"
    );
}

/// AGT-108: an unreadable or unexpected answer says the stop could not be confirmed.
#[test]
fn readiness_without_evidence_says_the_edit_was_not_sent_and_to_try_again() {
    for (kind, what) in [("StopLoss", "stop"), ("TakeProfit", "take profit")] {
        let expected = format!(
            "Couldn't confirm your current {what} with Bybit, so the edit wasn't sent. Your {what} is unchanged; try again."
        );
        for answer in ["http", "empty", "other-order"] {
            let mut h = frame_fed(kind);
            start(&mut h, kind);
            match answer {
                "http" => h.respond("&orderId=partial", 503, "unavailable"),
                "empty" => page(&mut h, "&orderId=partial", vec![]),
                _ => {
                    let mut other = original(&h);
                    other["orderId"] = "other".into();
                    page(&mut h, "&orderId=partial", vec![other]);
                }
            }
            refused_with(&h, &format!("{kind} {answer}"), &expected);
        }
    }
}

/// AGT-108: the edited order's own row, no longer live as it was, says it changed;
/// retrying the same edit cannot succeed, so the copy does not say "try again".
#[test]
fn readiness_on_a_changed_or_triggered_stop_says_so_instead_of_try_again() {
    for (kind, what) in [("StopLoss", "stop"), ("TakeProfit", "take profit")] {
        let expected = format!(
            "Your {what} changed or already triggered, so the edit wasn't sent. Refresh and check it before editing."
        );
        for (axis, change) in [
            (0, "Triggered"),
            (0, "price"),
            (1, "Filled"),
            (2, "Cancelled"),
        ] {
            let mut h = frame_fed(kind);
            let healthy = original(&h);
            let mut changed = healthy.clone();
            if change == "price" {
                changed["triggerPrice"] = "89".into();
            } else {
                changed["orderStatus"] = change.into();
            }
            start(&mut h, kind);
            for (index, path) in [
                "&orderId=partial",
                "&openOnly=0&limit=50",
                "/v5/order/history",
            ]
            .into_iter()
            .enumerate()
            {
                let row = if index == axis {
                    changed.clone()
                } else {
                    healthy.clone()
                };
                page(&mut h, path, vec![row]);
                if index == axis {
                    break;
                }
            }
            refused_with(&h, &format!("{kind} axis={axis} {change}"), &expected);
        }
    }
}
