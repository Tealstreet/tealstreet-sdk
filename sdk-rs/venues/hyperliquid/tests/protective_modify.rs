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
fn same_kind_protection_is_one_native_modify_with_exact_execution_semantics() {
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
            let a = size_reply(&mut e, &read, &row);
            let sent = action(write(&a));
            assert_eq!(
                sent["type"], "modify",
                "same-kind protection must never release a cancel"
            );
            assert_eq!(sent["oid"], 9);
            assert_ne!(
                sent.get("a"),
                Some(&Value::Bool(true)),
                "never place if the original no longer exists"
            );
            assert_eq!(sent["order"]["t"]["trigger"]["tpsl"], tag);
            assert_eq!(
                sent["order"]["t"]["trigger"]["isMarket"],
                !kind.ends_with("Limit")
            );
            assert_eq!(
                sent["order"]["t"]["trigger"]["triggerPx"],
                target.to_string()
            );
            assert_eq!(sent["order"]["p"], "70", "retain native execution price");
            assert_eq!(sent["order"]["s"], size);
            assert_eq!(sent["order"]["r"], true);
            assert_eq!(sent["order"]["c"], row["cloid"]);
            assert_eq!(
                e.store().order("9").unwrap().status,
                OrderStatus::Open,
                "no local canceling state"
            );
        }
    }
}
#[test]
fn native_modify_refusal_retains_original_and_surfaces_unchanged_with_venue_reason() {
    let row = order("Stop Market", "80", "1");
    let (mut e, _) = setup(&row);
    let before = e.store().order("9").unwrap().clone();
    let read = amend(&mut e, 75., None);
    let a = size_reply(&mut e, &read, &row);
    let request = write(&a);
    assert_eq!(action(request)["type"], "modify");
    reply(
        &mut e,
        request,
        200,
        r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"error":"Order would immediately trigger."}]}}}"#,
    );
    let a = prepared_now(&mut e);
    let failures: Vec<_> = a
        .iter()
        .filter_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 90, error }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0].outcome, Some(Outcome::Rejected));
    assert!(
        failures[0]
            .message
            .contains("Order would immediately trigger.")
    );
    assert!(failures[0].message.contains("order unchanged"));
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange"))),
        "no restore or resend on native refusal"
    );
    let after = e
        .store()
        .order("9")
        .expect("original protective order remains");
    assert_eq!(
        (after.price, after.remaining, after.kind, after.status),
        (
            before.price,
            before.remaining,
            before.kind,
            OrderStatus::Open
        )
    );
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
fn fallback_place(e: &mut Engine<Hyperliquid>, row: &Value) -> HttpRequest {
    let read = amend(e, 75., None);
    let a = size_reply(e, &read, row);
    let cancel = write(&a);
    assert_eq!(
        action(cancel)["type"],
        "cancel",
        "kind-changing edit retains a guarded fallback"
    );
    reply(
        e,
        cancel,
        200,
        r#"{"status":"ok","response":{"type":"cancel","data":{"statuses":["success"]}}}"#,
    );
    let a = prepared_now(e);
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/info"))),
        "no optional read between cancel and replacement"
    );
    let a = advance_packet_timers(e, a, 2334.);
    write(&a).clone()
}
#[test]
fn definitive_fallback_refusal_restores_exact_original_once() {
    for refusal in [
        "Order has invalid price.",
        "Invalid TP/SL price.",
        "Price must be divisible by tick size.",
    ] {
        let row = order("Take Profit Market", "120", "0");
        let (mut e, _) = setup(&row);
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
        assert_eq!(
            restored["orders"][0],
            json!({"a":0,"b":false,"p":"70","s":"0","r":true,"t":{"trigger":{"isMarket":true,"triggerPx":"120","tpsl":"tp"}},"c":row["cloid"]})
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
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange"))),
            "never restore twice"
        );
    }
}

#[test]
fn failed_or_unknown_restore_is_typed_unprotected_and_never_retried() {
    for unknown in [false, true] {
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
        if unknown {
            e.handle(
                2668.,
                Event::HttpFailed {
                    request: restore.id,
                    error: "transport reply lost",
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
            .expect("visible unprotected terminal");
        assert_eq!(
            failure.protection,
            Some(safe_cex_engine::error::Protection::Unprotected)
        );
        if unknown {
            assert_eq!(failure.outcome, Some(Outcome::Unknown));
        }
        assert!(failure.message.contains("UNPROTECTED"));
        assert!(
            !a.iter()
                .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
        );
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
fn partial_fills_size_native_modify_from_original_total_without_touching_siblings() {
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
    let a = size_reply(&mut e, &read, &row);
    let r = write(&a);
    assert_eq!(action(r)["type"], "modify");
    assert_eq!(
        action(r)["order"]["s"],
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
fn unsupported_unproven_partial_conversion_is_refused_before_cancel() {
    let row = order("Take Profit Market", "120", "1");
    let (mut e, _) = setup(&row);
    let read = amend(&mut e, 75., None);
    let a = size_reply(&mut e, &read, &row);
    assert!(
        !a.iter()
            .any(|a| matches!(a,Action::Http(r)if r.url.ends_with("/exchange")))
    );
    assert!(a.iter().any(|a|matches!(a,Action::Notice(Notice::CommandFailed{command:90,error})if error.outcome==Some(Outcome::NotSent)&&error.message.contains("placement semantics"))));
    assert_eq!(e.store().order("9").unwrap().status, OrderStatus::Open);
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
fn a_partial_fill_after_native_modify_binding_preserves_remaining_protection() {
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
        "never restore filled size with a stale modification"
    );
    prepared_now(&mut e);
    assert_eq!(e.store().order("9"), Some(&remaining));
}

#[test]
fn confirmed_native_modify_retains_current_identity_and_stream_price_after_ack() {
    for same_id in [false, true] {
        let (mut e, s) = ready("disabled");
        let cloid = "0x11111111111111111111111111111111";
        let opts = serde_json::from_value(json!({"symbol":"BTC-PERP","type":"stop_market","side":"sell","price":80,"amount":1,"reduceOnly":true,"clientOrderId":cloid})).unwrap();
        e.handle(
            1500.,
            Event::Command(Command::PlaceOrders {
                command: 80,
                orders: vec![opts],
            }),
        );
        let actions = prepared_now(&mut e);
        let placed = write(&actions);
        reply(
            &mut e,
            placed,
            200,
            r#"{"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":9}}]}}}"#,
        );
        prepared_now(&mut e);
        let row = order("Stop Market", "80", "1");
        message(
            &mut e,
            s,
            1600.,
            &json!({"channel":"orderUpdates","data":[{"order":row,"status":"open"}]}).to_string(),
        );
        let read = amend(&mut e, 75., None);
        let a = size_reply(&mut e, &read, &row);
        let request = write(&a);
        let new_id = if same_id { 9 } else { 12 };
        reply(&mut e,request,200,&json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":new_id}}]}}}).to_string());
        prepared_now(&mut e);
        assert_eq!(
            e.store()
                .order(&new_id.to_string())
                .expect("confirmed native identity remains visible")
                .price,
            75.
        );
        if !same_id {
            message(
                &mut e,
                s,
                2100.,
                &json!({"channel":"orderUpdates","data":[{"order":row,"status":"canceled"}]})
                    .to_string(),
            );
        }
        let mut current = row.clone();
        current["oid"] = json!(new_id);
        current["triggerPx"] = json!("75");
        message(
            &mut e,
            s,
            2200.,
            &json!({"channel":"orderUpdates","data":[{"order":current,"status":"open"}]})
                .to_string(),
        );
        assert_eq!(
            e.store().order(&new_id.to_string()).unwrap().price,
            75.,
            "prior placement provenance must never undo the accepted edit"
        );
    }
}
