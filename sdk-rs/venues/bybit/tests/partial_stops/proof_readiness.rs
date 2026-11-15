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

#[test]
fn missing_unrelated_trigger_source_does_not_waive_peer_lineage() {
    let mut h = replacement_after_peer_census(true);
    h.respond(
        "/v5/position/trading-stop",
        200,
        r#"{"retCode":10001,"retMsg":"replacement refused","result":{}}"#,
    );
    assert!(!h.has_request("/v5/position/trading-stop"));
    assert!(h.notices.iter().any(|notice| matches!(notice,
        safe_cex_engine::Notice::CommandFailed { command:91, error }
        if error.protection == Some(safe_cex_engine::error::Protection::Unprotected)
    )));
}

#[test]
fn cancelled_target_without_its_trigger_source_never_authorizes_replacement() {
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
    page(&mut h, "&orderId=partial", vec![terminal]);
    assert_eq!(h.unknown, vec![91]);
    assert!(
        !h.requests
            .iter()
            .any(|r| r.method == safe_cex_engine::Method::Post)
    );
}
