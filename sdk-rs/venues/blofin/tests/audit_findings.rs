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

fn stop_amend_to_replacement(h: &mut Harness) {
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
    h.reply(
        "cancel-tpsl",
        r#"{"code":"0","data":[{"tpslId":"91","code":"0","msg":null}]}"#,
    );
    fire_next_timer(h);
}

fn seed_limit(h: &mut Harness, id: &str, extra: &str) {
    h.ws(&format!(r#"{{"arg":{{"channel":"orders"}},"data":[{{"instId":"BTC-USDT","orderCategory":"normal","orderId":"{id}","orderType":"limit","side":"buy","marginMode":"cross","positionSide":"net","size":"10","price":"60000","state":"live"{extra}}}]}}"#));
}

#[test]
fn an_internal_error_on_a_stop_replacement_says_it_may_be_live() {
    let mut h = Harness::new();
    stop_amend_to_replacement(&mut h);
    let r = h.take("order-tpsl");
    h.respond(
        r,
        r#"{"code":"103013","msg":"Internal error; unable to process your request. Please try again.","data":null}"#,
    );
    let e = failure(&h).expect("failure");
    assert!(e.message.contains("may have no stop"), "{}", e.message);
    assert!(!e.message.contains("refused"), "{}", e.message);
}

#[test]
fn a_row_internal_error_on_a_stop_replacement_says_it_may_be_live() {
    let mut h = Harness::new();
    stop_amend_to_replacement(&mut h);
    let r = h.take("order-tpsl");
    h.respond(
        r,
        r#"{"code":"0","msg":"success","data":{"tpslId":null,"clientOrderId":null,"code":"103013","msg":"Internal error; unable to process your request. Please try again."}}"#,
    );
    let e = failure(&h).expect("failure");
    assert!(e.message.contains("may have no stop"), "{}", e.message);
    assert!(!e.message.contains("refused"), "{}", e.message);
}

#[test]
fn a_documented_refusal_on_a_stop_replacement_still_says_refused() {
    let mut h = Harness::new();
    stop_amend_to_replacement(&mut h);
    let r = h.take("order-tpsl");
    h.respond(
        r,
        r#"{"code":"102049","msg":"stop loss trigger price must be higher than the best bid price","data":null}"#,
    );
    let e = failure(&h).expect("failure");
    assert!(e.message.contains("has no stop"), "{}", e.message);
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
fn a_stop_already_gone_before_its_move_says_no_replacement_was_sent() {
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
    h.reply("cancel-tpsl", r#"{"code":"1","data":[{"tpslId":"91","code":"1000","msg":"The order has been canceled."}]}"#);
    let e = failure(&h);
    let message = e.expect("the move fails").message;
    assert!(message.contains("no replacement was sent"), "{message}");
    assert!(!message.contains("replacement refused"), "{message}");
}

#[test]
fn cancelling_one_leg_of_a_sized_bracket_re_places_the_other() {
    let mut h = Harness::new();
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
    let r = h.take("cancel-tpsl");
    h.respond(
        r,
        r#"{"code":"0","data":[{"tpslId":"500","code":"0","msg":null}]}"#,
    );
    // Release any pause and see whether a re-placement of the stop follows.
    let at = h.now + 2_000.0;
    h.collect_at(at);
    for a in &h.actions {
        if let Action::SetTimer { timer, at_ms } = a {
            if *at_ms > 1.0 && *at_ms < 3_000.0 {
                let (t, m) = (*timer, *at_ms);
                h.handle(m, Event::TimerFired { timer: t });
                break;
            }
        }
    }
    h.collect();
    let resent = h.pending.iter().any(|r| {
        r.url.contains("order-tpsl")
            && r.body
                .as_deref()
                .is_some_and(|b| b.contains("55000") && b.contains("\"4\""))
    });
    assert!(
        resent || failure(&h).is_some_and(|e| e.message.contains("no stop")),
        "{:?}",
        h.pending
            .iter()
            .map(|r| (&r.url, &r.body))
            .collect::<Vec<_>>()
    );
    assert!(resent, "the surviving stop is re-placed at its size");
}
