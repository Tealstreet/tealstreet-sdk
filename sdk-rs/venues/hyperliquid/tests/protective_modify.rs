//! Protective amendments use genuine preflight reads and physical signed writes.
mod support;
use safe_cex_engine::model::{OrderStatus, OrderType};
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Notice, Outcome, SocketId};
use safe_cex_venue_hyperliquid::Hyperliquid;
use serde_json::{Value, json};
use support::*;

fn order(kind: &str, trigger: &str, size: &str) -> Value {
    json!({"coin":"BTC","oid":9,"side":"A","limitPx":"70","triggerPx":trigger,
        "origSz":size,"sz":size,"reduceOnly":true,"orderType":kind,
        "isTrigger":true,"isPositionTpsl":size=="0","cloid":"0x11111111111111111111111111111111"})
}
fn setup(row: &Value) -> (Engine<Hyperliquid>, SocketId) {
    let (mut e, s) = ready("disabled");
    let frame =
        json!({"channel":"orderUpdates","data":[{"order":row,"status":"open"}]}).to_string();
    message(&mut e, s, 1500., &frame);
    prepared_now(&mut e);
    assert!(matches!(
        e.store().order("9").unwrap().kind,
        OrderType::StopLoss | OrderType::TakeProfit
    ));
    (e, s)
}
fn amend(e: &mut Engine<Hyperliquid>, price: f64, amount: Option<f64>) -> HttpRequest {
    e.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 90,
            id: "9".into(),
            price: Some(price),
            amount,
        }),
    );
    let a = prepared_now(e);
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    a.into_iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(&r) == "orderStatus" => Some(r),
            _ => None,
        })
        .expect("owner-bound fresh sizing before any write")
}
fn size_reply(e: &mut Engine<Hyperliquid>, read: &HttpRequest, row: &Value) -> Vec<Action> {
    let text = json!({"status":"order","order":{"order":row,"status":"open"}}).to_string();
    reply(e, read, 200, &text);
    prepared_now(e)
}
fn write(a: &[Action]) -> &HttpRequest {
    a.iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.ends_with("/exchange") => Some(r),
            _ => None,
        })
        .expect("physical signed write")
}
fn action(r: &HttpRequest) -> Value {
    serde_json::from_str::<Value>(r.body.as_deref().unwrap()).unwrap()["action"].clone()
}
#[test]
fn same_kind_protection_uses_guarded_cancel_place_with_preserved_execution_semantics() {
    for (kind, old, target, tag) in [
        ("Stop Market", "80", 75., "sl"),
        ("Stop Limit", "80", 75., "sl"),
        ("Take Profit Market", "120", 125., "tp"),
        ("Take Profit Limit", "120", 125., "tp"),
    ] {
        for size in ["0", "1"] {
            let row = order(kind, old, size);
            let (mut e, _) = setup(&row);
            let read = amend(&mut e, target, None);
            let r = replace_after_read(&mut e, &read, &row);
            let sent = action(&r);
            assert_eq!(sent["type"], "order");
            assert!(sent.get("a").is_none(), "never use always_place");
            let wire = &sent["orders"][0];
            assert_eq!(wire["t"]["trigger"]["tpsl"], tag);
            assert_eq!(wire["t"]["trigger"]["isMarket"], !kind.ends_with("Limit"));
            assert_eq!(wire["t"]["trigger"]["triggerPx"], target.to_string());
            assert_eq!(wire["p"], "70");
            assert_eq!(wire["s"], size);
            assert_eq!(wire["r"], true);
            assert_ne!(wire["c"], row["cloid"]);
            assert!(e.store().order("9").is_none());
        }
    }
}

#[test]
fn unproven_partial_restore_is_not_attempted_and_refusal_is_loudly_unprotected() {
    for attached in [false, true] {
        let row = order("Stop Market", "80", "1");
        let (mut e, _) = setup(&row);
        if attached {
            e.store_mut()
                .update_order("9", |o| o.parent_id = Some("8".into()));
        }
        let read = amend(&mut e, 75., None);
        let r = replace_after_read(&mut e, &read, &row);
        reply(
            &mut e,
            &r,
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Invalid TP/SL price."}]}}}"#,
        );
        let a = prepared_now(&mut e);
        let errors: Vec<_> = a
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 90, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].protection,
            Some(safe_cex_engine::error::Protection::Unprotected)
        );
        assert!(
            errors[0].message.contains("stop removed")
                && errors[0].message.contains("please re-place"),
            "{}",
            errors[0].message
        );
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
        );
        assert!(e.store().order("9").is_none());
    }
}

#[test]
fn unproven_source_identity_refuses_before_any_write() {
    let row = order("Stop Market", "80", "1");
    let (mut e, _) = setup(&row);
    let read = amend(&mut e, 75., None);
    let mut foreign = row.clone();
    foreign["oid"] = json!(10);
    let a = size_reply(&mut e, &read, &foreign);
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange"))),
        "foreign read cannot release a modification or cancel"
    );
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:90,error})if error.outcome==Some(Outcome::NotSent))));
    assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
}

fn mark(e: &mut Engine<Hyperliquid>, socket: SocketId, at: f64, px: f64) {
    use base64::Engine as _;
    let raw = json!({"BTC":{"markPx":px.to_string()}}).to_string();
    let zipped = miniz_oxide::deflate::compress_to_vec(raw.as_bytes(), 6);
    let text=json!({"channel":"fastAssetCtxs","data":base64::engine::general_purpose::STANDARD.encode(zipped)}).to_string();
    message(e, socket, at, &text);
    assert_eq!(e.store().ticker("BTC-PERP").unwrap().mark, px);
}
fn replace_after_read(e: &mut Engine<Hyperliquid>, read: &HttpRequest, row: &Value) -> HttpRequest {
    let a = size_reply(e, read, row);
    let cancel = write(&a);
    assert_eq!(action(cancel)["type"], "cancel");
    reply(
        e,
        cancel,
        200,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    let a = prepared_now(e);
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/info")))
    );
    let a = advance_packet_timers(e, a, 2334.);
    write(&a).clone()
}
fn fallback_place(e: &mut Engine<Hyperliquid>, row: &Value) -> HttpRequest {
    let read = amend(e, 75., None);
    replace_after_read(e, &read, row)
}

#[test]
fn definitive_fallback_refusal_restores_exact_original_once() {
    // Protective put-back kept per Sam's decision, SPLIT_PLAN_INVARIANT.md exception 2.
    for refusal in [
        "Order has invalid price.",
        "Invalid TP/SL price.",
        "Price must be divisible by tick size.",
    ] {
        let row = order("Take Profit Market", "120", "0");
        let (mut e, socket) = setup(&row);
        let replacement = fallback_place(&mut e, &row);
        assert_eq!(
            action(&replacement)["orders"][0]["t"]["trigger"]["tpsl"],
            "sl"
        );
        reply(
        &mut e,
        &replacement,
        200,
        &json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"error":refusal}]}}}).to_string(),
    );
        let a = prepared_now(&mut e);
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 90, .. }))),
            "one caller terminal only after recovery"
        );
        let a = advance_packet_timers(&mut e, a, 2668.);
        let restore = write(&a);
        let restored = action(restore);
        assert_eq!(restored["type"], "order");
        assert_eq!(restored["grouping"], "positionTpsl");
        let restore_cloid = restored["orders"][0]["c"]
            .as_str()
            .expect("fresh put-back identity");
        assert_ne!(restored["orders"][0]["c"], row["cloid"]);
        assert_ne!(
            restored["orders"][0]["c"],
            action(&replacement)["orders"][0]["c"]
        );
        assert_eq!(restore_cloid.len(), 34);
        assert!(
            restore_cloid.starts_with("0x")
                && restore_cloid[2..].bytes().all(|b| b.is_ascii_hexdigit())
        );
        assert_eq!(
            restored["orders"][0],
            json!({"a":0,"b":false,"p":"70","s":"0","r":true,"t":{"trigger":{"isMarket":true,"triggerPx":"120","tpsl":"tp"}},"c":restore_cloid})
        );
        reply(
            &mut e,
            restore,
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
        );
        let a = prepared_now(&mut e);
        let errors: Vec<_> = a
            .iter()
            .filter_map(|a| match a {
                Action::Notice(Notice::CommandFailed { command: 90, error }) => Some(error),
                _ => None,
            })
            .collect();
        assert_eq!(errors.len(), 1);
        assert_eq!(
            errors[0].protection,
            Some(safe_cex_engine::error::Protection::Restored)
        );
        let elements = errors[0].elements.as_ref().expect("recovery journal");
        assert_eq!(elements.len(), 3);
        use safe_cex_engine::write_plan::{AppliedEffect, ElementResult};
        assert!(matches!(&elements[0].result,
            ElementResult::Applied { effect: AppliedEffect::Cancel, target_order_id: Some(id), .. } if id == "9"));
        assert!(matches!(&elements[1].result,
            ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::Rejected)));
        assert!(matches!(&elements[2].result,
            ElementResult::Applied { effect: AppliedEffect::PositionTrigger, position_key: Some(symbol), .. } if symbol == "BTC-PERP"));
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange"))),
            "never restore twice"
        );
        let mut live = row.clone();
        live["oid"] = 42.into();
        live["cloid"] = restore_cloid.into();
        message(
            &mut e,
            socket,
            2669.,
            &json!({"channel":"orderUpdates","data":[{"order":live,"status":"open"}]}).to_string(),
        );
        let visible = e
            .store()
            .order("42")
            .expect("put-back publishes under its fresh identity");
        assert_eq!(visible.client_order_id.as_deref(), Some(restore_cloid));
        assert_eq!(visible.price, 120.);
        assert_eq!(visible.kind, OrderType::TakeProfit);
    }
}

fn check_restore_failure(case: &str, retry_after_seconds: Option<f64>) {
    let row = order("Take Profit Market", "120", "0");
    let (mut e, _) = setup(&row);
    let replacement = fallback_place(&mut e, &row);
    reply(
        &mut e,
        &replacement,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order has invalid price."}]}}}"#,
    );
    let a = prepared_now(&mut e);
    let a = advance_packet_timers(&mut e, a, 2668.);
    let restore = write(&a);
    if case == "unknown" {
        e.handle(
            2668.,
            Event::HttpFailed {
                request: restore.id,
                error: "transport reply lost",
            },
        );
    } else if case == "rate_limited" {
        let headers = retry_after_seconds
            .map(|seconds| format!("Retry-After: {seconds}"))
            .unwrap_or_default();
        let now = safe_cex_engine::test_support::dispatch_now(&e).max(1000.0);
        e.handle(
            now,
            Event::HttpResponse {
                request: restore.id,
                response: safe_cex_engine::Response {
                    status: 429,
                    headers: &headers,
                    body: "Too many requests",
                },
            },
        );
    } else {
        reply(
            &mut e,
            restore,
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order has invalid size."}]}}}"#,
        );
    }
    let a = prepared_now(&mut e);
    let failure = a
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 90, error }) => Some(error),
            _ => None,
        })
        .expect("one visible restore terminal");
    if case == "refused" {
        assert_eq!(
            failure.protection,
            Some(safe_cex_engine::error::Protection::Unprotected)
        );
        assert!(failure.message.contains("UNPROTECTED"));
        assert!(failure.message.contains("could not be restored"));
    } else {
        assert_eq!(failure.protection, None, "{case}");
        assert_eq!(failure.outcome, Some(Outcome::Unknown));
        assert_eq!(failure.message, ENGINE_UNKNOWN_TEXT);
        assert!(
            !failure.message.contains("UNPROTECTED")
                && !failure.message.contains("could not be restored")
        );
    }
    assert_eq!(
        failure.kind,
        Some(if case == "rate_limited" {
            safe_cex_engine::ErrorKind::RateLimited
        } else {
            safe_cex_engine::ErrorKind::Rejected
        }),
        "a later put-back 429 must retain its cooldown kind"
    );
    assert_eq!(
        failure.retry_after_ms,
        retry_after_seconds.map(|seconds| seconds * 1000.0)
    );
    assert_eq!(
        a.iter()
            .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 90, .. })))
            .count(),
        1
    );
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
}
#[test]
fn definitive_restore_refusal_is_unprotected_without_retry() {
    check_restore_failure("refused", None);
}
#[test]
fn unknown_restore_is_uncertain_without_protection_or_retry() {
    check_restore_failure("unknown", None);
}
#[test]
fn rate_limited_restore_is_uncertain_without_protection_or_retry() {
    for retry_after_seconds in [None, Some(30.0)] {
        check_restore_failure("rate_limited", retry_after_seconds);
    }
}
#[test]
fn ambiguous_replacement_never_creates_a_second_protection() {
    let row = order("Take Profit Market", "120", "0");
    let (mut e, _) = setup(&row);
    let replacement = fallback_place(&mut e, &row);
    e.handle(
        2334.,
        Event::HttpFailed {
            request: replacement.id,
            error: "reply lost after send",
        },
    );
    let a = prepared_now(&mut e);
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:90,error})if error.outcome==Some(Outcome::Unknown))));
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange"))),
        "an unknown replacement must be reconciled, never restored/resubmitted"
    );
}
#[test]
fn held_cancel_checks_fresh_mark_and_source_fills_at_final_claim() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    for case in ["crossed", "stale", "filled"] {
        let row = order("Take Profit Market", "120", "0");
        let (mut e, s) = setup(&row);
        let read = amend(&mut e, 75., None);
        let text = json!({"status":"order","order":{"order":row,"status":"open"}}).to_string();
        reply(&mut e, &read, 200, &text);
        let (preparation, r) = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } if request.url.ends_with("/exchange") => Some((preparation, request)),
                    _ => None,
                },
                _ => None,
            })
            .expect("genuine held cancel preparation");
        e.http_prepared(
            2000.,
            preparation,
            Prepared {
                id: r.id.0,
                expires_at: None,
            },
        );
        let permit = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let at = if case == "stale" { 6001. } else { 2500. };
        if case == "crossed" {
            mark(&mut e, s, at, 70.);
        }
        if case == "filled" {
            message(
                &mut e,
                s,
                at,
                &json!({"channel":"orderUpdates","data":[{"order":row,"status":"filled"}]})
                    .to_string(),
            );
        }
        assert!(
            e.claim_http(at, permit).is_err(),
            "{case} source/reference must prevent physical cancel"
        );
        let a = prepared_now(&mut e);
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
        );
        if case != "filled" {
            assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
        }
    }
}

#[test]
fn partial_fills_size_guarded_replacement_from_original_total_without_touching_siblings() {
    let mut row = order("Stop Limit", "80", "2");
    row["sz"] = json!("1");
    let (mut e, s) = setup(&row);
    let mut sibling = order("Take Profit Market", "120", "0");
    sibling["oid"] = json!(11);
    sibling["cloid"] = json!("0x22222222222222222222222222222222");
    message(
        &mut e,
        s,
        1600.,
        &json!({"channel":"orderUpdates","data":[{"order":sibling,"status":"open"}]}).to_string(),
    );
    let before = e.store().order("11").unwrap().clone();
    let read = amend(&mut e, 75., Some(1.5));
    let request = replace_after_read(&mut e, &read, &row);
    let r = &request;
    assert_eq!(action(r)["type"], "order");
    assert_eq!(
        action(r)["orders"][0]["s"],
        "0.5",
        "amended total minus the real filled amount"
    );
    reply(
        &mut e,
        r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":12}}]}}}"#,
    );
    let a = prepared_now(&mut e);
    assert!(
        a.iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 90, .. })))
    );
    assert_eq!(e.store().order("11"), Some(&before));
    assert_eq!(e.store().order("12").unwrap().remaining, 0.5);
    assert!(e.store().order("9").is_none());
}
#[test]
fn amount_only_below_minimum_never_cancels_or_modifies_the_original() {
    for size in ["0", "1"] {
        let row = order("Stop Market", "80", size);
        let (mut e, _) = setup(&row);
        let read = amend(&mut e, 80., Some(0.01));
        let a = size_reply(&mut e, &read, &row);
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
        );
        assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:90,error})if error.outcome==Some(Outcome::NotSent))));
        assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
    }
}
#[test]
fn partial_kind_conversion_remains_available_without_unproven_restore() {
    let row = order("Take Profit Market", "120", "1");
    let (mut e, _) = setup(&row);
    let r = fallback_place(&mut e, &row);
    assert_eq!(action(&r)["orders"][0]["t"]["trigger"]["tpsl"], "sl");
    reply(
        &mut e,
        &r,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Invalid TP/SL price."}]}}}"#,
    );
    let a = prepared_now(&mut e);
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:90,error})if error.protection==Some(safe_cex_engine::error::Protection::Unprotected))));
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
}

#[test]
fn a_closed_position_cannot_be_resurrected_by_restoring_an_obsolete_stop() {
    let row = order("Take Profit Market", "120", "0");
    let (mut e, s) = setup(&row);
    let replacement = fallback_place(&mut e, &row);
    message(
        &mut e,
        s,
        2400.,
        r#"{"channel":"clearinghouseState","data":{"user":"0x14791697260e4c9a71f18484c9f997b308e59325","dex":"","clearinghouseState":{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":2400}}}"#,
    );
    assert!(
        e.store().positions().is_empty(),
        "genuine private state reports the position closed"
    );
    reply(
        &mut e,
        &replacement,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order has invalid price."}]}}}"#,
    );
    let a = prepared_now(&mut e);
    let a = advance_packet_timers(&mut e, a, 2800.);
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange"))),
        "never resurrect closed-position protection"
    );
}
#[test]
fn a_newer_protection_cannot_be_overwritten_by_restoring_the_old_one() {
    let row = order("Take Profit Market", "120", "0");
    let (mut e, s) = setup(&row);
    let replacement = fallback_place(&mut e, &row);
    let mut newer = order("Take Profit Market", "130", "0");
    newer["oid"] = json!(12);
    newer["cloid"] = json!("0x22222222222222222222222222222222");
    message(
        &mut e,
        s,
        2400.,
        &json!({"channel":"orderUpdates","data":[{"order":newer,"status":"open"}]}).to_string(),
    );
    let before = e.store().order("12").unwrap().clone();
    reply(
        &mut e,
        &replacement,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order has invalid price."}]}}}"#,
    );
    let a = prepared_now(&mut e);
    let a = advance_packet_timers(&mut e, a, 2800.);
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange"))),
        "newer protection wins over an obsolete restore"
    );
    assert_eq!(e.store().order("12"), Some(&before));
}

#[test]
fn held_restore_rechecks_position_newer_protection_and_fresh_original_trigger() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    for case in ["closed", "newer", "crossed", "stale", "filled"] {
        let row = order("Take Profit Market", "120", "0");
        let (mut e, s) = setup(&row);
        let replacement = fallback_place(&mut e, &row);
        reply(
            &mut e,
            &replacement,
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order has invalid price."}]}}}"#,
        );
        let (preparation, r) = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Prepare {
                        preparation,
                        request,
                        ..
                    } if request.url.ends_with("/exchange") => Some((preparation, request)),
                    _ => None,
                },
                _ => None,
            })
            .expect("held one-shot restore");
        e.http_prepared(
            2668.,
            preparation,
            Prepared {
                id: r.id.0,
                expires_at: None,
            },
        );
        let permit = e
            .drain_actions()
            .find_map(|a| match a {
                Action::ManagedHttp(a) => match *a {
                    HttpAction::Bind { permit, .. } => Some(permit),
                    _ => None,
                },
                _ => None,
            })
            .unwrap();
        let at = if case == "stale" { 6501. } else { 2700. };
        match case {
            "closed" => message(
                &mut e,
                s,
                at,
                r#"{"channel":"clearinghouseState","data":{"user":"0x14791697260e4c9a71f18484c9f997b308e59325","dex":"","clearinghouseState":{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":2700}}}"#,
            ),
            "newer" => {
                let mut newer = order("Take Profit Market", "130", "0");
                newer["oid"] = json!(12);
                newer["cloid"] = json!("0x22222222222222222222222222222222");
                message(
                    &mut e,
                    s,
                    at,
                    &json!({"channel":"orderUpdates","data":[{"order":newer,"status":"open"}]})
                        .to_string(),
                );
            }
            "crossed" => mark(&mut e, s, at, 125.),
            "filled" => message(
                &mut e,
                s,
                at,
                &json!({"channel":"orderUpdates","data":[{"order":row,"status":"filled"}]})
                    .to_string(),
            ),
            _ => {}
        }
        assert!(
            e.claim_http(at, permit).is_err(),
            "{case}: obsolete or immediately firing restore must not be released"
        );
        let a = prepared_now(&mut e);
        assert!(
            !a.iter()
                .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
        );
        assert!(
            a.iter()
                .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 90, .. }))),
            "{case}: visible terminal"
        );
    }
}

#[test]
fn a_partial_fill_after_cancel_binding_preserves_remaining_protection() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    let row = order("Stop Limit", "80", "2");
    let (mut e, s) = setup(&row);
    let read = amend(&mut e, 75., None);
    reply(
        &mut e,
        &read,
        200,
        &json!({"status":"order","order":{"order":row,"status":"open"}}).to_string(),
    );
    let (preparation, r) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.ends_with("/exchange") => Some((preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.http_prepared(
        2000.,
        preparation,
        Prepared {
            id: r.id.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    let mut filled = row.clone();
    filled["sz"] = json!("1");
    message(
        &mut e,
        s,
        2500.,
        &json!({"channel":"orderUpdates","data":[{"order":filled,"status":"open"}]}).to_string(),
    );
    let remaining = e.store().order("9").unwrap().clone();
    assert_eq!(remaining.remaining, 1.);
    assert!(
        e.claim_http(2500., permit).is_err(),
        "a new fill must veto the stale physical cancel"
    );
    prepared_now(&mut e);
    assert_eq!(e.store().order("9"), Some(&remaining));
}

#[test]
fn accepted_replacement_retains_price_in_both_stream_ack_orderings_without_losing_fills() {
    for early in [false, true] {
        let row = order("Stop Market", "80", "1");
        let (mut e, s) = setup(&row);
        let r = fallback_place(&mut e, &row);
        let mut current = order("Stop Market", "75", "1");
        current["oid"] = json!(12);
        current["sz"] = json!("0.6");
        current["cloid"] = action(&r)["orders"][0]["c"].clone();
        if early {
            message(
                &mut e,
                s,
                2400.,
                &json!({"channel":"orderUpdates","data":[{"order":current,"status":"open"}]})
                    .to_string(),
            );
        }
        reply(
            &mut e,
            &r,
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":12}}]}}}"#,
        );
        prepared_now(&mut e);
        if !early {
            message(
                &mut e,
                s,
                2400.,
                &json!({"channel":"orderUpdates","data":[{"order":current,"status":"open"}]})
                    .to_string(),
            );
        }
        let live = e.store().order("12").unwrap();
        assert_eq!(live.price, 75.);
        assert_eq!(live.filled, 0.4);
        assert_eq!(live.remaining, 0.6);
        assert_eq!(live.status, OrderStatus::Open);
    }
}

#[test]
fn review_3120_stream_before_ack_retains_accepted_trigger() {
    for full in [false, true] {
        let size = if full { "0" } else { "1" };
        let row = order("Stop Market", "80", size);
        let (mut e, s) = setup(&row);
        let r = fallback_place(&mut e, &row);
        let mut early = order("Stop Market", "75", size);
        early["oid"] = json!(12);
        early["cloid"] = action(&r)["orders"][0]["c"].clone();
        message(
            &mut e,
            s,
            2400.,
            &json!({"channel":"orderUpdates","data":[{"order":early,"status":"open"}]}).to_string(),
        );
        let reply_body = if full {
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#
        } else {
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":12}}]}}}"#
        };
        reply(&mut e, &r, 200, reply_body);
        prepared_now(&mut e);
        assert_eq!(e.store().order("12").unwrap().price, 75.);
        message(
            &mut e,
            s,
            2500.,
            &json!({"channel":"orderUpdates","data":[{"order":row,"status":"canceled"}]})
                .to_string(),
        );
        message(
            &mut e,
            s,
            2600.,
            &json!({"channel":"orderUpdates","data":[{"order":early,"status":"open"}]}).to_string(),
        );
        assert_eq!(
            e.store().order("12").unwrap().price,
            75.,
            "accepted price must survive late original terminal and subsequent native stream"
        );
    }
}

#[test]
fn terminal_replacement_stream_before_ack_is_never_resurrected() {
    for terminal in ["filled", "canceled"] {
        for full in [false, true] {
            let size = if full { "0" } else { "1" };
            let row = order("Stop Market", "80", size);
            let (mut e, s) = setup(&row);
            let r = fallback_place(&mut e, &row);
            let mut ended = order("Stop Market", "75", size);
            ended["oid"] = json!(12);
            ended["cloid"] = action(&r)["orders"][0]["c"].clone();
            message(
                &mut e,
                s,
                2400.,
                &json!({"channel":"orderUpdates","data":[{"order":ended,"status":terminal}]})
                    .to_string(),
            );
            let text = if full {
                r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#
            } else {
                r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":12}}]}}}"#
            };
            reply(&mut e, &r, 200, text);
            let a = prepared_now(&mut e);
            assert!(
                e.store().order("12").is_none(),
                "{terminal}: later ACK must never undo the terminal stream"
            );
            assert!(
                !a.iter()
                    .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
            );
        }
    }
}

#[test]
fn final_cancel_veto_preserves_the_order_when_its_position_closed_after_sizing() {
    use safe_cex_engine::request_context::{HttpAction, Prepared};
    let row = order("Stop Market", "80", "1");
    let (mut e, s) = setup(&row);
    let read = amend(&mut e, 75., None);
    reply(
        &mut e,
        &read,
        200,
        &json!({"status":"order","order":{"order":row,"status":"open"}}).to_string(),
    );
    let (preparation, r) = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Prepare {
                    preparation,
                    request,
                    ..
                } if request.url.ends_with("/exchange") => Some((preparation, request)),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    e.http_prepared(
        2000.,
        preparation,
        Prepared {
            id: r.id.0,
            expires_at: None,
        },
    );
    let permit = e
        .drain_actions()
        .find_map(|a| match a {
            Action::ManagedHttp(a) => match *a {
                HttpAction::Bind { permit, .. } => Some(permit),
                _ => None,
            },
            _ => None,
        })
        .unwrap();
    message(
        &mut e,
        s,
        2100.,
        r#"{"channel":"clearinghouseState","data":{"user":"0x14791697260e4c9a71f18484c9f997b308e59325","dex":"","clearinghouseState":{"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":2100}}}"#,
    );
    assert!(e.store().positions().is_empty());
    assert!(e.claim_http(2100., permit).is_err());
    prepared_now(&mut e);
    assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
}

#[test]
fn dispatched_ambiguity_replacement_is_uncertain_without_restore_or_resend() {
    for (status, text) in [
        (429, "Too many requests"),
        (
            429,
            r#"{"status":"err","response":"User or API Wallet does not exist."}"#,
        ),
        (
            408,
            r#"{"status":"err","response":"User or API Wallet does not exist."}"#,
        ),
        (
            500,
            r#"{"status":"err","response":"User or API Wallet does not exist."}"#,
        ),
        (
            503,
            r#"{"status":"err","response":"User or API Wallet does not exist."}"#,
        ),
    ] {
        for size in ["0", "1"] {
            let row = order("Take Profit Market", "120", size);
            let (mut e, _) = setup(&row);
            let replacement = fallback_place(&mut e, &row);
            reply(&mut e, &replacement, status, text);
            let actions = prepared_now(&mut e);
            let failures: Vec<_> = actions
                .iter()
                .filter_map(|a| match a {
                    Action::Notice(Notice::CommandFailed { command: 90, error }) => Some(error),
                    _ => None,
                })
                .collect();
            assert_eq!(failures.len(), 1);
            let error = failures[0];
            assert_eq!(
                error.kind,
                Some(match status {
                    429 => safe_cex_engine::ErrorKind::RateLimited,
                    408 => safe_cex_engine::ErrorKind::Timeout,
                    _ => safe_cex_engine::ErrorKind::VenueUnavailable,
                })
            );
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            assert_eq!(error.protection, None);
            assert!(
                error
                    .message
                    .contains("outcome is unknown. Check Open Orders"),
                "{}",
                error.message
            );
            assert!(
                !error.message.contains("replacement refused"),
                "{}",
                error.message
            );
            assert!(!error.message.contains("UNPROTECTED"));
            assert!(
                !actions
                    .iter()
                    .any(|a| matches!(a, Action::Http(r) if r.url.ends_with("/exchange")))
            );
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, Action::Notice(Notice::CommandFailed { .. })))
                    .count(),
                1
            );
        }
    }
}

#[test]
fn released_cancel_keeps_replacement_and_restored_result_after_original_intent_expiry() {
    let row = order("Take Profit Market", "120", "0");
    let (mut e, _) = setup(&row);
    e.handle_command(
        2000.,
        Command::AmendOrder {
            command: 90,
            id: "9".into(),
            price: Some(75.),
            amount: None,
        },
        Some(10),
    )
    .unwrap();
    let actions = prepared_now(&mut e);
    let timer = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == 2010. => Some(*timer),
            _ => None,
        })
        .expect("original caller intent timer");
    let read = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if kind(r) == "orderStatus" => Some(r),
            _ => None,
        })
        .unwrap();
    let actions = size_reply(&mut e, read, &row);
    let cancel = write(&actions).clone();
    assert_eq!(action(&cancel)["type"], "cancel");
    e.handle(2010., Event::TimerFired { timer });
    assert!(
        !prepared_now(&mut e)
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandFailed { command: 90, .. })))
    );
    e.handle(2020., Event::HttpResponse {
        request: cancel.id,
        response: safe_cex_engine::Response { status: 200, headers: "", body:
            r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"# },
    });
    let actions = prepared_now(&mut e);
    let actions = advance_packet_timers(&mut e, actions, 2334.);
    let replacement = write(&actions);
    assert_eq!(
        action(replacement)["orders"][0]["t"]["trigger"]["triggerPx"],
        "75"
    );
    reply(
        &mut e,
        replacement,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Invalid TP/SL price."}]}}}"#,
    );
    let actions = prepared_now(&mut e);
    let actions = advance_packet_timers(&mut e, actions, 2668.);
    let restore = write(&actions);
    assert_eq!(
        action(restore)["orders"][0]["t"]["trigger"]["triggerPx"],
        "120"
    );
    reply(
        &mut e,
        restore,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":["waitingForTrigger"]}}}"#,
    );
    let actions = prepared_now(&mut e);
    let failures: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 90, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(
        failures[0].protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
    assert!(failures[0].message.contains("restored"));
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Notice(Notice::CommandDone { command: 90, .. })))
    );
}
