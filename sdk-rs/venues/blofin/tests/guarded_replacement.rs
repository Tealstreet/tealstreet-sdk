//! Stop edits BloFin cannot make in place (a leg of a partly filled entry): the
//! cancel is re-checked when it is finally sent; a failed replacement never
//! dispatches a restore or rollback.
#[allow(dead_code)]
mod support;
use safe_cex_engine::{Action, Command, Event, Notice, Outcome};
use support::*;

fn failure(h: &Harness) -> Option<safe_cex_engine::VenueError> {
    h.actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { error, .. }) => Some(error.clone()),
        _ => None,
    })
}
fn body(r: &safe_cex_engine::HttpRequest) -> serde_json::Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}
fn writes(h: &Harness) -> Vec<String> {
    h.pending
        .iter()
        .filter(|r| r.method == safe_cex_engine::Method::Post)
        .map(|r| r.url.clone())
        .collect()
}

/// A buy limit entry 4/10 filled, carrying TP 65000 and SL 55000.
fn partly_filled(h: &mut Harness) {
    partly_filled_with(h, r#""tpTriggerPrice":"65000","slTriggerPrice":"55000""#);
}
/// The same entry with `legs` as its row's TP/SL fields.
fn partly_filled_with(h: &mut Harness, legs: &str) {
    h.modes();
    h.reply("account/positions", r#"{"code":"0","data":[]}"#);
    h.reply(
        "balances",
        r#"{"code":"0","data":[{"balance":"1000","available":"1000","frozen":"0"}]}"#,
    );
    h.reply("orders-pending",&format!(r#"{{"code":"0","data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"12","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","filledSize":"4","price":"60000","state":"partially_filled",{legs}}}]}}"#));
    for path in [
        "orders-pending",
        "orders-tpsl-pending",
        "orders-algo-pending",
    ] {
        h.reply(path, r#"{"code":"0","data":[]}"#);
    }
    h.e.store_mut().update_ticker("BTCUSDT", |t| {
        t.mark = 60010.0;
        t.last = 60005.0;
        t.index = 60000.0;
    });
}
/// Edits the stop to `price`; the cancel is planned but not yet sent.
fn edit_stop(h: &mut Harness, price: f64) {
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12__stop_loss".into(),
            price: Some(price),
            amount: None,
        }),
    );
}
/// The cancel lands and the entry reads as cancelled: the replacement is next.
fn to_replacement(h: &mut Harness) -> safe_cex_engine::HttpRequest {
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0","msg":null}]}"#,
    );
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == h.now + 350.0 => Some(*timer),
            _ => None,
        })
        .unwrap();
    let at = h.now + 350.0;
    h.handle(at, Event::TimerFired { timer });
    h.collect();
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"4"}}"#);
    h.take("batch-orders")
}
const REFUSED: &str = r#"{"code":"1","msg":"All operations failed","data":[{"orderId":"0","code":"102014","msg":"trigger refused"}]}"#;

#[test]
fn a_refused_replacement_never_restores_the_original() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    assert_eq!(
        body(&replacement)[0]["attachAlgoOrders"][0]["slTriggerPrice"],
        "56000"
    );
    h.respond(replacement, REFUSED);
    let error = failure(&h).expect("replacement refused");
    let elements = error.elements.as_ref().expect("placement outcome");
    assert_eq!(elements.len(), 1);
    assert!(
        matches!(&elements[0].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) } if error.outcome == Some(Outcome::Rejected))
    );
    assert!(writes(&h).is_empty(), "no automatic restore or rollback");
}

#[test]
fn a_refused_replacement_preserves_native_refusal_detail() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond(replacement, REFUSED);
    let error = failure(&h).expect("replacement refused");
    let outcomes = error.elements.as_ref().expect("native placement outcome");
    assert!(
        matches!(&outcomes[0].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) } if error.venue_code.as_deref() == Some("102014"))
    );
    assert!(writes(&h).is_empty());
}

const RATE_LIMITED: &str = r#"{"code":"429","msg":"Too Many Requests"}"#;

/// A 429 may have placed the order, so it is unknown, never Unprotected.
fn assert_unknown_stop(h: &Harness) {
    let error = failure(h).expect("the edit did not finish");
    assert!(
        error
            .message
            .contains("outcome is unknown. Check Open Orders")
            || error.message == "HTTP response does not prove the write outcome",
        "{}",
        error.message
    );
    assert!(
        !error.message.contains("refused")
            || error
                .message
                .contains("replacement was refused and its put-back's outcome is unknown"),
        "{}",
        error.message
    );
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.protection, None);
    assert!(writes(h).is_empty(), "{:?}", writes(h));
}

#[test]
fn an_accepted_replacement_is_not_repeated() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond(
        replacement,
        r#"{"code":"0","data":[{"orderId":"13","code":"0"}]}"#,
    );
    assert!(failure(&h).is_none());
    assert!(writes(&h).is_empty());
}

#[test]
fn a_rate_limited_replacement_is_unknown_and_never_put_back() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond_status(replacement, 429, RATE_LIMITED);
    assert_unknown_stop(&h);
}

#[test]
fn a_replacement_keeps_each_legs_trigger_reference() {
    for (legs, tp, sl) in [
        (
            r#""tpTriggerPrice":"65000","tpTriggerPriceType":"mark","slTriggerPrice":"55000","slTriggerPriceType":"index""#,
            "mark",
            "index",
        ),
        (
            r#""attachAlgoOrders":[{"tpTriggerPrice":"65000","tpTriggerPriceType":"index","slTriggerPrice":"55000","slTriggerPriceType":"mark"}]"#,
            "index",
            "mark",
        ),
        // An untyped row is BloFin's default, sent explicitly.
        (
            r#""tpTriggerPrice":"65000","slTriggerPrice":"55000""#,
            "last",
            "last",
        ),
    ] {
        for leg in ["12__stop_loss", "12__take_profit"] {
            let mut h = Harness::new();
            partly_filled_with(&mut h, legs);
            h.handle(
                1.0,
                Event::Command(Command::AmendOrder {
                    command: 7,
                    id: leg.into(),
                    price: Some(if leg.ends_with("loss") {
                        56000.0
                    } else {
                        66000.0
                    }),
                    amount: None,
                }),
            );
            let replacement = to_replacement(&mut h);
            let restore = body(&replacement);
            h.respond(replacement, REFUSED);
            assert!(writes(&h).is_empty());
            for p in [&restore[0], &restore[0]["attachAlgoOrders"][0]] {
                assert_eq!(
                    p["tpTriggerPrice"],
                    if leg.ends_with("loss") {
                        "65000"
                    } else {
                        "66000"
                    },
                    "{leg}: {restore}"
                );
                assert_eq!(
                    p["slTriggerPrice"],
                    if leg.ends_with("loss") {
                        "56000"
                    } else {
                        "55000"
                    },
                    "{leg}: {restore}"
                );
                assert_eq!(p["tpTriggerPriceType"], tp, "{leg}: {restore}");
                assert_eq!(p["slTriggerPriceType"], sl, "{leg}: {restore}");
            }
        }
    }
}

#[test]
fn a_bracket_leg_size_edit_is_refused_before_any_write() {
    for leg in ["12__stop_loss", "12__take_profit"] {
        let mut h = Harness::new();
        partly_filled(&mut h);
        h.handle(
            1.0,
            Event::Command(Command::AmendOrder {
                command: 7,
                id: leg.into(),
                price: Some(56000.0),
                amount: Some(3.0),
            }),
        );
        h.collect();
        let error = failure(&h).expect("refused");
        assert_eq!(
            error.message, "BloFin attached stop size follows its entry",
            "{leg}"
        );
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{leg}");
        assert_eq!(error.protection, None, "{leg}");
        assert!(
            !h.pending
                .iter()
                .any(|r| r.method != safe_cex_engine::Method::Get),
            "{leg}: no write: {:?}",
            h.pending.iter().map(|r| &r.url).collect::<Vec<_>>()
        );
        assert!(h.e.store().order(leg).is_some(), "{leg}: untouched");
    }
}

#[test]
fn an_unknown_replacement_is_never_doubled() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond(replacement, r#"{"code":"103013","msg":"Internal error; unable to process your request. Please try again.","data":null}"#);
    let error = failure(&h).expect("unknown");
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.protection, None);
    assert!(
        writes(&h).is_empty(),
        "no put-back over an unknown: {:?}",
        writes(&h)
    );
}

#[test]
fn a_stale_crossed_or_changed_entry_at_the_final_send_cancels_nothing() {
    for case in ["stale", "crossed", "filled"] {
        let mut h = Harness::new();
        partly_filled(&mut h);
        edit_stop(&mut h, 56000.0);
        match case {
            "stale" => h.collect_at(1.0 + 6_000.0),
            "crossed" => {
                // The last price fell through the new stop before the cancel left.
                h.e.store_mut()
                    .update_ticker("BTCUSDT", |t| t.last = 55900.0);
                h.collect();
            }
            _ => {
                h.e.store_mut().update_order("12", |o| o.filled = 5.0);
                h.collect();
            }
        }
        assert!(
            !h.pending.iter().any(|r| r.url.contains("cancel")),
            "{case}: {:?}",
            h.pending.iter().map(|r| &r.url).collect::<Vec<_>>()
        );
        let error = failure(&h).expect(case);
        assert!(
            error.message.contains("was not changed"),
            "{case}: {}",
            error.message
        );
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{case}");
    }
}

#[test]
fn a_cancel_of_the_stop_while_it_is_replaced_is_refused_visibly() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.handle(
        h.now,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["12__stop_loss".into()],
        }),
    );
    h.collect();
    assert!(
        h.failures
            .iter()
            .any(|e| e.message.contains("already in flight")),
        "the cancel waits out the edit, visibly"
    );
    h.respond(replacement, REFUSED);
    assert_eq!(
        h.pending
            .iter()
            .filter(|r| r.url.ends_with("/batch-orders"))
            .count(),
        0,
        "no put-back"
    );
}

#[test]
fn a_later_cancel_never_targets_an_invented_restore() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond(replacement, REFUSED);
    h.handle(
        h.now,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["12__stop_loss".into()],
        }),
    );
    h.collect();
    assert!(writes(&h).is_empty());
}

/// Engine-sealed replacement failures retain the placement journal.
#[test]
fn an_unproven_replacement_stays_unknown_without_restoration() {
    for status in [408, 503] {
        let mut h = Harness::new();
        partly_filled(&mut h);
        edit_stop(&mut h, 56000.0);
        let replacement = to_replacement(&mut h);
        h.respond_status(replacement, status, "");
        let error = failure(&h).expect("the engine seals the Plan");
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        assert!(matches!(
            &error.elements.as_ref().expect("complete placement outcome")[0].result,
            safe_cex_engine::write_plan::ElementResult::Unknown { .. }
        ));
        assert!(writes(&h).is_empty());
    }
}

#[test]
fn engine_ambiguity_bare_429_replacement_stays_unknown_in_journal() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond_status(replacement, 429, "{}");
    assert_unknown_stop(&h);
}

#[test]
fn dispatched_ambiguity_protective_cancel_and_replacement_never_restore() {
    for (status, response) in [
        (429, "{}"),
        (429, REFUSED),
        (200, RATE_LIMITED),
        (408, REFUSED),
        (500, REFUSED),
        (503, REFUSED),
    ] {
        for replacement in [false, true] {
            let mut h = Harness::new();
            partly_filled(&mut h);
            edit_stop(&mut h, 56000.0);
            let request = if replacement {
                to_replacement(&mut h)
            } else {
                h.collect();
                h.take("cancel-batch-orders")
            };
            h.respond_status(request, status, response);
            let error = failure(&h).expect("one terminal failure");
            assert_eq!(
                error.outcome,
                Some(Outcome::Unknown),
                "{status}: {response}"
            );
            assert_eq!(error.protection, None);
            assert!(
                error.message.to_lowercase().contains("unknown"),
                "{error:?}"
            );
            assert!(error.message.contains("Check Open Orders"), "{error:?}");
            assert!(!error.message.contains("refused"), "{error:?}");
            assert!(!error.message.contains("position has no stop"), "{error:?}");
            assert!(writes(&h).is_empty(), "no resend, replacement or put-back");
        }
    }
}

#[test]
fn dispatched_ambiguity_native_rate_row_cannot_authorize_put_back() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond(replacement, r#"{"code":"1","msg":"All operations failed","data":[{"orderId":"0","code":"429","msg":"Too Many Requests"}]}"#);
    assert!(
        writes(&h).is_empty(),
        "a native rate row must never release a put-back"
    );
    assert_unknown_stop(&h);
    let error = failure(&h).unwrap();
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::RateLimited));
    assert_eq!(error.venue_code.as_deref(), Some("429"));
    assert_eq!(error.venue_message.as_deref(), Some("Too Many Requests"));
}
