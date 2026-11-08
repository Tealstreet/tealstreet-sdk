//! BloFin answers the 2026-11-02 audit found misreported: each pinned to its ruling.
mod support;
use safe_cex_engine::model::OrderType;
use safe_cex_engine::{Action, Command, Event, Notice};
use support::*;

fn failure(h: &Harness) -> Option<safe_cex_engine::VenueError> {
    h.actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandFailed { error, .. }) => Some(error.clone()),
        _ => None,
    })
}

fn fire_next_timer(h: &mut Harness) {
    let timer = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > h.now && *at_ms < h.now + 5_000.0 => {
                Some((*timer, *at_ms))
            }
            _ => None,
        })
        .unwrap();
    h.handle(timer.1, Event::TimerFired { timer: timer.0 });
    h.collect();
}

/// A stop on a standalone tpsl order, edited in place by amend-tpsl.
fn stop_amend_native(h: &mut Harness) {
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"last","state":"live"}]}"#);
    let id =
        h.e.store()
            .orders()
            .iter()
            .find(|o| o.kind == OrderType::StopLoss)
            .unwrap()
            .id
            .clone();
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id,
            price: Some(56000.0),
            amount: None,
        }),
    );
    h.collect();
}

fn no_cancel_or_placement(h: &Harness) {
    assert!(
        !h.pending
            .iter()
            .any(|r| r.url.contains("cancel") || r.url.contains("order-tpsl")),
        "{:?}",
        h.pending.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
}

fn seed_limit(h: &mut Harness, id: &str, extra: &str) {
    h.ws(&format!(r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"{extra}}}]}}"#));
}

#[test]
fn an_internal_error_on_a_stop_edit_says_its_outcome_is_unknown() {
    let mut h = Harness::new();
    stop_amend_native(&mut h);
    let r = h.take("amend-tpsl");
    h.respond(
        r,
        r#"{"code":"103013","msg":"Internal error; unable to process your request. Please try again.","data":null}"#,
    );
    let e = failure(&h).expect("failure");
    assert!(e.message.contains("is unknown"), "{}", e.message);
    assert_eq!(e.outcome, Some(safe_cex_engine::Outcome::Unknown));
    no_cancel_or_placement(&h);
}

#[test]
fn a_row_internal_error_on_a_stop_edit_says_its_outcome_is_unknown() {
    let mut h = Harness::new();
    stop_amend_native(&mut h);
    let r = h.take("amend-tpsl");
    h.respond(
        r,
        r#"{"code":"0","msg":"success","data":{"tpslId":null,"clientOrderId":null,"code":"103013","msg":"Internal error; unable to process your request. Please try again."}}"#,
    );
    let e = failure(&h).expect("failure");
    assert!(e.message.contains("is unknown"), "{}", e.message);
    no_cancel_or_placement(&h);
}

#[test]
fn a_documented_refusal_on_a_stop_edit_leaves_the_stop_unchanged() {
    let mut h = Harness::new();
    stop_amend_native(&mut h);
    let r = h.take("amend-tpsl");
    h.respond(
        r,
        r#"{"code":"102049","msg":"stop loss trigger price must be higher than the best bid price","data":null}"#,
    );
    let e = failure(&h).expect("failure");
    assert!(
        e.message.contains("stop is unchanged") && e.message.contains("refused"),
        "{}",
        e.message
    );
    assert_eq!(e.outcome, Some(safe_cex_engine::Outcome::Rejected));
    assert_eq!(
        e.protection, None,
        "nothing was removed, so nothing needs restoring"
    );
    no_cancel_or_placement(&h);
}

#[test]
fn an_internal_error_on_an_order_replacement_says_it_may_be_gone() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    seed_limit(&mut h, "12", "");
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "12".into(),
            price: Some(61000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply(
        "cancel-batch-orders",
        r#"{"code":"0","data":[{"orderId":"12","code":"0","msg":null}]}"#,
    );
    fire_next_timer(&mut h);
    h.reply("order-detail", r#"{"code":"0","data":{"instId":"BTC-USDT","orderId":"12","state":"canceled","filledSize":"0"}}"#);
    let r = h.take("batch-orders");
    h.respond(r, r#"{"code":"103013","msg":"Internal error; unable to process your request. Please try again.","data":null}"#);
    let e = failure(&h).unwrap();
    assert!(e.message.contains("may be gone"), "{}", e.message);
}

#[test]
fn a_tpsl_blofin_says_is_already_gone_is_a_done_cancel() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"1009","side":"sell","size":"4","marginMode":"cross","positionSide":"net","slTriggerPrice":"55000","slOrderPrice":"-1","state":"live"}]}"#);
    h.handle(
        2.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["1009__stop_loss".into()],
        }),
    );
    h.collect();
    h.reply("cancel-tpsl", r#"{"code":"0","msg":"success","data":[{"tpslId":"1009","clientOrderId":null,"code":"500","msg":"Cancel failed as the order has been filled, triggered, canceled or does not exist."}]}"#);
    let e = failure(&h);
    assert!(e.is_none(), "{:?}", e.map(|e| e.message));
}

#[test]
fn a_stop_already_gone_before_its_edit_says_nothing_was_edited() {
    let mut h = Harness::new();
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"91","side":"sell","size":"-1","marginMode":"cross","positionSide":"net","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"last","state":"live"}]}"#);
    h.handle(
        1.0,
        Event::Command(Command::AmendOrder {
            command: 7,
            id: "91__stop_loss".into(),
            price: Some(56000.0),
            amount: None,
        }),
    );
    h.collect();
    h.reply("amend-tpsl", r#"{"code":"1","data":[{"tpslId":"91","code":"1000","msg":"The order has been canceled."}]}"#);
    let e = failure(&h);
    let message = e.expect("the edit fails").message;
    assert!(
        message.contains("already filled, triggered or canceled"),
        "{message}"
    );
    no_cancel_or_placement(&h);
}

fn sized_bracket(h: &mut Harness) {
    h.open_private();
    h.seed();
    h.ws(r#"{"arg":{"channel":"orders-algo"},"data":[{"instId":"BTC-USDT","tpslId":"500","orderId":"","side":"sell","size":"4","marginMode":"cross","positionSide":"net","tpTriggerPrice":"65000","tpOrderPrice":"-1","tpTriggerPriceType":"last","slTriggerPrice":"55000","slOrderPrice":"-1","slTriggerPriceType":"last","state":"live"}]}"#);
    h.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 7,
            ids: vec!["500__take_profit".into()],
        }),
    );
    h.collect();
}

/// Ruled, after a live check that BloFin folds both legs into one tpsl order: one amend
/// drops the cancelled leg, so the other never leaves the venue.
#[test]
fn cancelling_one_leg_of_a_sized_bracket_amends_it_away() {
    let mut h = Harness::new();
    sized_bracket(&mut h);
    let r = h.take("amend-tpsl");
    let body: serde_json::Value = serde_json::from_str(r.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["tpslId"], "500");
    assert_eq!(body["newTpTriggerPrice"], "0");
    assert!(body.get("newSlTriggerPrice").is_none(), "{body}");
    // deaws2's live answer, TRX-USDT, 2026-11-02, with this fixture's ids.
    h.respond(
        r,
        r#"{"code":"0","msg":"success","data":{"tpslId":"500","code":"0","ts":"1762100000000"}}"#,
    );
    let read = h.take("tpslId=500");
    assert!(read.url.contains("orders-tpsl-pending"), "{}", read.url);
    h.respond(
        read,
        r#"{"code":"0","msg":"success","data":[{"tpslId":"500","instId":"BTC-USDT","tpTriggerPrice":null,"slTriggerPrice":"55000","state":"live"}]}"#,
    );
    assert!(
        failure(&h).is_none(),
        "{:?}",
        failure(&h).map(|e| e.message)
    );
    assert!(!h.pending.iter().any(|r| r.url.contains("cancel-tpsl")));
}

/// Read-back failure must not trigger cancellation or replacement.
#[test]
fn an_amend_whose_read_back_lacks_the_kept_leg_stops_without_repair() {
    let mut h = Harness::new();
    sized_bracket(&mut h);
    let r = h.take("amend-tpsl");
    h.respond(
        r,
        r#"{"code":"0","msg":"success","data":{"tpslId":"500","code":"0"}}"#,
    );
    let read = h.take("tpslId=500");
    h.respond(read, r#"{"code":"0","data":[]}"#);
    let error = failure(&h).expect("unverified accepted edit");
    assert_eq!(error.outcome, None);
    assert!(
        matches!(&error.elements.as_ref().expect("applied leg-removal effect")[0].result,
        safe_cex_engine::write_plan::ElementResult::Applied { effect: safe_cex_engine::write_plan::AppliedEffect::Amend, target_order_id: Some(id), .. } if id == "500")
    );
    no_cancel_or_placement(&h);
}

#[test]
fn a_refused_amend_stops_without_cancel_or_replacement() {
    let mut h = Harness::new();
    sized_bracket(&mut h);
    let r = h.take("amend-tpsl");
    h.respond(r, r#"{"code":"102014","msg":"amend refused","data":null}"#);
    let error = failure(&h).expect("amend refused");
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Rejected));
    assert_eq!(error.venue_code.as_deref(), Some("102014"));
    no_cancel_or_placement(&h);
}
