//! Stop edits BloFin cannot make in place (a leg of a partly filled entry): the
//! cancel is re-checked when it is finally sent, and a refused replacement puts
//! the original entry and its legs back once.
#[allow(dead_code)]
mod support;
use safe_cex_engine::error::Protection;
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
fn a_refused_replacement_puts_the_original_entry_and_legs_back_once() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    assert_eq!(
        body(&replacement)[0]["attachAlgoOrders"][0]["slTriggerPrice"],
        "56000"
    );
    h.respond(replacement, REFUSED);
    let restore = h.take("batch-orders");
    let p = body(&restore);
    assert_eq!(p[0]["side"], "buy");
    assert_eq!(p[0]["orderType"], "limit");
    assert_eq!(p[0]["price"], "60000");
    assert_eq!(
        p[0]["size"], "6",
        "the unfilled remainder, as before the edit"
    );
    assert_eq!(p[0]["tpTriggerPrice"], "65000");
    assert_eq!(
        p[0]["slTriggerPrice"], "55000",
        "the original stop, not the edit"
    );
    let client = p[0]["clientOrderId"].as_str().unwrap();
    h.respond(
        restore,
        &format!(r#"{{"code":"0","data":[{{"orderId":"13","clientOrderId":"{client}","code":"0","msg":null}}]}}"#),
    );
    let error = failure(&h).expect("the edit did not happen");
    assert!(
        error.message.contains("is unchanged") && error.message.contains("put back"),
        "{}",
        error.message
    );
    assert_eq!(error.protection, Some(Protection::Restored));
    assert!(writes(&h).is_empty(), "one put-back only: {:?}", writes(&h));
}

#[test]
fn a_refused_put_back_is_typed_unprotected_and_never_retried() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond(replacement, REFUSED);
    let restore = h.take("batch-orders");
    h.respond(restore, REFUSED);
    let error = failure(&h).expect("nothing stands");
    assert_eq!(error.protection, Some(Protection::Unprotected));
    assert!(writes(&h).is_empty(), "{:?}", writes(&h));
}

const RATE_LIMITED: &str = r#"{"code":"429","msg":"Too Many Requests"}"#;

/// A 429 may have placed the order, so it is unknown: never "refused" or Unprotected.
fn assert_unknown_stop(h: &Harness) {
    let error = failure(h).expect("the edit did not finish");
    assert!(
        error
            .message
            .contains("outcome is unknown. Check Open Orders"),
        "{}",
        error.message
    );
    assert!(!error.message.contains("refused"), "{}", error.message);
    assert_eq!(error.outcome, Some(Outcome::Unknown));
    assert_eq!(error.protection, None);
    assert!(writes(h).is_empty(), "{:?}", writes(h));
}

#[test]
fn a_rate_limited_put_back_is_unknown_not_unprotected() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond(replacement, REFUSED);
    let restore = h.take("batch-orders");
    h.respond_status(restore, 429, RATE_LIMITED);
    assert_unknown_stop(&h);
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
fn a_put_back_keeps_each_legs_trigger_reference() {
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
            h.respond(replacement, REFUSED);
            let restore = body(&h.take("batch-orders"));
            for p in [&restore[0], &restore[0]["attachAlgoOrders"][0]] {
                assert_eq!(p["tpTriggerPrice"], "65000", "{leg}: {restore}");
                assert_eq!(p["slTriggerPrice"], "55000", "{leg}: {restore}");
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
        1,
        "one put-back"
    );
}

#[test]
fn a_later_cancel_of_the_original_follows_it_to_the_put_back_entry() {
    let mut h = Harness::new();
    partly_filled(&mut h);
    edit_stop(&mut h, 56000.0);
    let replacement = to_replacement(&mut h);
    h.respond(replacement, REFUSED);
    let restore = h.take("batch-orders");
    let client = body(&restore)[0]["clientOrderId"]
        .as_str()
        .unwrap()
        .to_owned();
    h.respond(
        restore,
        &format!(r#"{{"code":"0","data":[{{"orderId":"13","clientOrderId":"{client}","code":"0","msg":null}}]}}"#),
    );
    // A screen still showing the original cancels it by its old id.
    h.handle(
        h.now,
        Event::Command(Command::CancelOrders {
            command: 8,
            ids: vec!["12__stop_loss".into()],
        }),
    );
    h.collect();
    let cancel = h.take("cancel-batch-orders");
    assert!(
        cancel.body.as_deref().unwrap().contains("\"13\""),
        "{:?}",
        cancel.body
    );
}

/// A put-back the engine seals on a 408 or 5xx says its outcome is unknown,
/// never the replacement's refusal and never to re-place a second stop.
#[test]
fn a_put_back_sealed_on_a_5xx_says_its_outcome_is_unknown() {
    for status in [408, 503] {
        let mut h = Harness::new();
        partly_filled(&mut h);
        edit_stop(&mut h, 56000.0);
        let replacement = to_replacement(&mut h);
        h.respond(replacement, REFUSED);
        let restore = h.take("batch-orders");
        h.respond_status(restore, status, "");
        let error = failure(&h).expect("the engine seals the Plan");
        assert!(
            error.message.contains("put-back's outcome is unknown")
                && error.message.contains("Check Open Orders")
                && !error.message.contains("trigger refused"),
            "{status}: {}",
            error.message
        );
        assert_eq!(error.outcome, Some(Outcome::Unknown), "{status}");
        assert_eq!(error.protection, None, "{status}");
        assert!(writes(&h).is_empty(), "{status}: never resent");
    }
}
