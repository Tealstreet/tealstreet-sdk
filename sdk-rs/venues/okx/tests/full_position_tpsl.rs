//! AGT-68/AGT-98: OKX holds one entire-position TP/SL algo per position (51088).
//! A full SL and TP placed together become one `oco`; a lone full leg beside a
//! held one-leg full algo joins it through amend-algos; anything else beside a
//! held full algo is refused before any write. Cancelling one leg of a two-leg
//! algo deletes that leg and keeps the other.
#![allow(dead_code)]
mod support;

use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, Method, Notice, Outcome};
use safe_cex_venue_okx::Okx;
use support::*;

fn full_leg(id: &str, kind: OrderType, side: OrderSide) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side,
        price: if kind == OrderType::TakeProfit {
            42000.
        } else {
            36000.
        },
        amount: 0.,
        filled: 0.,
        remaining: 0.,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 4.,
        trigger_type: Some(StopTriggerType::MarkPrice),
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

fn place(e: &mut Engine<Okx>, orders: &[&str]) -> Vec<Action> {
    let orders = orders
        .iter()
        .map(|text| serde_json::from_str(text).unwrap())
        .collect();
    e.handle(
        5.,
        Event::Command(Command::PlaceOrders { command: 1, orders }),
    );
    prepared(e, 5.)
}

fn writes(out: &[Action]) -> Vec<&Action> {
    out.iter()
        .filter(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        .collect()
}

fn path(a: &Action) -> &str {
    let Action::Http(r) = a else {
        panic!("not HTTP")
    };
    r.url.split('?').next().unwrap()
}

fn body(a: &Action) -> serde_json::Value {
    let Action::Http(r) = a else {
        panic!("not HTTP")
    };
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}

const FULL_TP: &str =
    r#"{"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0,"price":42000}"#;
const FULL_SL: &str =
    r#"{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"price":36000}"#;

fn refusal(out: &[Action]) -> &safe_cex_engine::VenueError {
    out.iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 1, error }) => Some(error),
            _ => None,
        })
        .unwrap_or_else(|| panic!("refusal reaches the caller: {out:?}"))
}

const HELD: &str =
    "OKX allows one full-position TP/SL. Edit the existing order or remove it first.";

fn done(out: &[Action]) -> Option<&str> {
    out.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: 1, result }) => Some(result.as_str()),
        _ => None,
    })
}

/// Same kind, both legs already held, or both legs at once beside a held leg:
/// OKX would answer 51088, so nothing is sent.
#[test]
fn a_full_leg_beside_a_held_full_algo_is_refused_before_any_write() {
    let held_stop = vec![full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell)];
    let held_take_profit = vec![full_leg("78_tp", OrderType::TakeProfit, OrderSide::Sell)];
    let held_both = vec![
        full_leg("79_sl", OrderType::StopLoss, OrderSide::Sell),
        full_leg("79_tp", OrderType::TakeProfit, OrderSide::Sell),
    ];
    let cases: [(&Vec<Order>, &[&str]); 7] = [
        (&held_stop, &[FULL_SL]),
        (&held_take_profit, &[FULL_TP]),
        (&held_both, &[FULL_TP]),
        (&held_both, &[FULL_SL]),
        // Both legs in one command while one is held: never send one leg and
        // collect 51088 on the other.
        (&held_stop, &[FULL_SL, FULL_TP]),
        (&held_take_profit, &[FULL_TP, FULL_SL]),
        // Two joins onto one algo: the second would replace the first.
        (&held_stop, &[FULL_TP, FULL_TP]),
    ];
    for (held, orders) in cases {
        let mut e = trading_engine();
        for o in held {
            e.store_mut().upsert_order(o.clone());
        }
        let out = place(&mut e, orders);
        assert!(writes(&out).is_empty(), "{orders:?}: {out:?}");
        let error = refusal(&out);
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{error:?}");
        assert_eq!(error.message, HELD, "{error:?}");
        for o in held {
            assert_eq!(e.store().order(&o.id), Some(o));
        }
    }
}

/// Live okx-t, AGT-98: amend-algos adding a TP to a closeFraction=1 SL-only
/// `conditional` returned sCode 0, the algo became an `oco` with both legs under
/// the same algoId, and the added TP fired (actualSide `tp`).
#[test]
fn a_full_take_profit_joins_the_held_full_stop_algo() {
    let mut e = trading_engine();
    let stop = full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell);
    e.store_mut().upsert_order(stop.clone());
    let out = place(&mut e, &[FULL_TP]);
    let sent = writes(&out);
    assert_eq!(sent.len(), 1, "{out:?}");
    assert!(
        path(sent[0]).ends_with("/api/v5/trade/amend-algos"),
        "{out:?}"
    );
    let b = body(sent[0]);
    assert_eq!(b["instId"], "BTC-USDT-SWAP");
    assert_eq!(b["algoId"], "77");
    assert_eq!(b["cxlOnFail"], false);
    assert_eq!(b["newTpTriggerPx"], "42000");
    assert_eq!(b["newTpOrdPx"], "-1");
    assert_eq!(b["newTpTriggerPxType"], "last");
    // The held stop leg and the close-all sentinel are untouched.
    for absent in [
        "newSlTriggerPx",
        "newSlOrdPx",
        "newSlTriggerPxType",
        "newSz",
    ] {
        assert!(b.get(absent).is_none(), "{absent}: {b}");
    }
    reply(
        &mut e,
        sent[0],
        r#"{"code":"0","msg":"","data":[{"algoId":"77","algoClOrdId":"","reqId":"","sCode":"0","sMsg":""}]}"#,
        6.,
    );
    let out = prepared(&mut e, 6.);
    assert!(writes(&out).is_empty(), "{out:?}");
    assert_eq!(done(&out), Some("[\"77\"]"), "{out:?}");
    assert_eq!(e.store().order("77_sl"), Some(&stop));
}

#[test]
fn a_full_stop_joins_the_held_full_take_profit_algo() {
    let mut e = trading_engine();
    e.store_mut()
        .upsert_order(full_leg("78_tp", OrderType::TakeProfit, OrderSide::Sell));
    let out = place(&mut e, &[FULL_SL]);
    let sent = writes(&out);
    assert_eq!(sent.len(), 1, "{out:?}");
    assert!(
        path(sent[0]).ends_with("/api/v5/trade/amend-algos"),
        "{out:?}"
    );
    let b = body(sent[0]);
    assert_eq!(b["algoId"], "78");
    assert_eq!(b["newSlTriggerPx"], "36000");
    assert_eq!(b["newSlOrdPx"], "-1");
    assert_eq!(b["newSlTriggerPxType"], "mark");
    for absent in [
        "newTpTriggerPx",
        "newTpOrdPx",
        "newTpTriggerPxType",
        "newSz",
    ] {
        assert!(b.get(absent).is_none(), "{absent}: {b}");
    }
}

/// cxlOnFail:false keeps the held algo when OKX refuses the amendment.
#[test]
fn a_refused_join_is_a_rejection_that_leaves_the_held_stop() {
    let mut e = trading_engine();
    let stop = full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell);
    e.store_mut().upsert_order(stop.clone());
    let out = place(&mut e, &[FULL_TP]);
    let sent = writes(&out);
    reply(
        &mut e,
        sent[0],
        r#"{"code":"1","msg":"","data":[{"algoId":"77","sCode":"51279","sMsg":"TP trigger price cannot be lower than the last price."}]}"#,
        6.,
    );
    let out = prepared(&mut e, 6.);
    assert!(
        writes(&out).is_empty(),
        "never cancels or re-places: {out:?}"
    );
    let error = refusal(&out);
    assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    assert_eq!(error.venue_code.as_deref(), Some("51279"));
    assert_eq!(e.store().order("77_sl"), Some(&stop));
}

#[test]
fn an_entry_and_a_joining_full_leg_send_the_entry_then_the_join() {
    let mut e = trading_engine();
    e.store_mut()
        .upsert_order(full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell));
    let out = place(
        &mut e,
        &[
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000}"#,
            FULL_TP,
        ],
    );
    let sent = writes(&out);
    assert_eq!(sent.len(), 1, "serial: {out:?}");
    assert!(path(sent[0]).ends_with("/api/v5/trade/batch-orders"));
    reply(
        &mut e,
        sent[0],
        r#"{"code":"0","msg":"","data":[{"ordId":"500","clOrdId":"","sCode":"0","sMsg":""}]}"#,
        6.,
    );
    let out = prepared(&mut e, 6.);
    let sent = writes(&out);
    assert_eq!(sent.len(), 1, "{out:?}");
    assert!(path(sent[0]).ends_with("/api/v5/trade/amend-algos"));
    assert_eq!(body(sent[0])["algoId"], "77");
}

fn cancel(e: &mut Engine<Okx>, ids: &[&str]) -> Vec<Action> {
    e.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: ids.iter().map(|id| id.to_string()).collect(),
        }),
    );
    prepared(e, 5.)
}

fn two_leg_algo(e: &mut Engine<Okx>) {
    e.store_mut()
        .upsert_order(full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell));
    e.store_mut()
        .upsert_order(full_leg("77_tp", OrderType::TakeProfit, OrderSide::Sell));
}

/// Live okx-t, AGT-98: deleting one leg of an `oco` (trigger and order price
/// "0") left a one-leg `conditional` with the other leg unchanged.
#[test]
fn cancelling_one_leg_of_a_two_leg_algo_deletes_only_that_leg() {
    for (id, kept, trigger, order) in [
        ("77_tp", "77_sl", "newTpTriggerPx", "newTpOrdPx"),
        ("77_sl", "77_tp", "newSlTriggerPx", "newSlOrdPx"),
    ] {
        let mut e = trading_engine();
        two_leg_algo(&mut e);
        let out = cancel(&mut e, &[id]);
        let sent = writes(&out);
        assert_eq!(sent.len(), 1, "{out:?}");
        assert!(
            path(sent[0]).ends_with("/api/v5/trade/amend-algos"),
            "{out:?}"
        );
        let b = body(sent[0]);
        assert_eq!(b["algoId"], "77");
        assert_eq!(b["cxlOnFail"], false);
        assert_eq!(b[trigger], "0");
        assert_eq!(b[order], "0");
        assert_eq!(b.as_object().unwrap().len(), 5, "{b}");
        reply(
            &mut e,
            sent[0],
            r#"{"code":"0","msg":"","data":[{"algoId":"77","algoClOrdId":"","reqId":"","sCode":"0","sMsg":""}]}"#,
            6.,
        );
        let out = prepared(&mut e, 6.);
        assert!(writes(&out).is_empty(), "{out:?}");
        assert_eq!(done(&out), Some("[]"), "{out:?}");
        assert_eq!(
            e.store().order(kept).map(|o| o.status),
            Some(OrderStatus::Open)
        );
    }
}

/// Live okx-t, AGT-98: amend-algos on an algo that is already gone answers
/// 51527 with an empty algoId. The leg is gone, so the cancellation is satisfied.
#[test]
fn a_gone_algo_satisfies_a_leg_cancellation() {
    let mut e = trading_engine();
    two_leg_algo(&mut e);
    let out = cancel(&mut e, &["77_tp"]);
    let sent = writes(&out);
    reply(
        &mut e,
        sent[0],
        r#"{"code":"1","msg":"","data":[{"algoClOrdId":"","algoId":"","reqId":"","sCode":"51527","sMsg":"Order modification failed. At least 1 of the attached TP/SL orders does not exist. "}]}"#,
        6.,
    );
    let out = prepared(&mut e, 6.);
    assert_eq!(done(&out), Some("[]"), "{out:?}");
}

/// The same 51527 on a join is a refusal: the leg was not added.
#[test]
fn venue_51527_on_a_join_stays_a_rejection() {
    let mut e = trading_engine();
    e.store_mut()
        .upsert_order(full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell));
    let out = place(&mut e, &[FULL_TP]);
    let sent = writes(&out);
    reply(
        &mut e,
        sent[0],
        r#"{"code":"1","msg":"","data":[{"algoClOrdId":"","algoId":"","reqId":"","sCode":"51527","sMsg":"Order modification failed. At least 1 of the attached TP/SL orders does not exist. "}]}"#,
        6.,
    );
    let out = prepared(&mut e, 6.);
    assert!(done(&out).is_none(), "{out:?}");
    assert!(refusal(&out).message.contains("51527"), "{out:?}");
}

#[test]
fn cancelling_both_legs_or_a_one_leg_algo_cancels_the_algo() {
    for (store, ids) in [(true, &["77_sl", "77_tp"][..]), (false, &["77_sl"][..])] {
        let mut e = trading_engine();
        if store {
            two_leg_algo(&mut e);
        } else {
            e.store_mut()
                .upsert_order(full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell));
        }
        let out = cancel(&mut e, ids);
        let sent = writes(&out);
        assert_eq!(sent.len(), 1, "{ids:?}: {out:?}");
        assert!(
            path(sent[0]).ends_with("/api/v5/trade/cancel-algos"),
            "{ids:?}"
        );
        assert_eq!(
            body(sent[0]),
            serde_json::json!([{"instId":"BTC-USDT-SWAP","algoId":"77"}])
        );
    }
}

#[test]
fn a_full_leg_is_placed_when_the_position_holds_no_full_algo() {
    let partial = {
        let mut o = full_leg("70_sl", OrderType::StopLoss, OrderSide::Sell);
        o.amount = 1.;
        o.remaining = 1.;
        o
    };
    let other_position = full_leg("71_sl", OrderType::StopLoss, OrderSide::Buy);
    let canceled = {
        let mut o = full_leg("73_sl", OrderType::StopLoss, OrderSide::Sell);
        o.status = OrderStatus::Canceled;
        o
    };
    for store in [vec![], vec![partial], vec![other_position], vec![canceled]] {
        let mut e = trading_engine();
        for o in &store {
            e.store_mut().upsert_order(o.clone());
        }
        let out = place(&mut e, &[FULL_TP]);
        let sent = writes(&out);
        assert_eq!(sent.len(), 1, "{store:?}: {out:?}");
        assert!(
            path(sent[0]).ends_with("/api/v5/trade/order-algo"),
            "{store:?}"
        );
        let b = body(sent[0]);
        assert_eq!(b["ordType"], "conditional");
        assert_eq!(b["closeFraction"], "1");
    }
}

#[test]
fn full_stop_and_take_profit_in_one_command_are_one_two_way_algo() {
    let mut e = trading_engine();
    let out = place(&mut e, &[FULL_TP, FULL_SL]);
    let sent = writes(&out);
    assert_eq!(
        sent.len(),
        1,
        "one entire-position algo carries both legs: {out:?}"
    );
    assert!(path(sent[0]).ends_with("/api/v5/trade/order-algo"));
    let b = body(sent[0]);
    assert_eq!(b["ordType"], "oco");
    assert_eq!(b["closeFraction"], "1");
    assert_eq!(b["slTriggerPx"], "36000");
    assert_eq!(b["slOrdPx"], "-1");
    assert_eq!(b["slTriggerPxType"], "mark");
    assert_eq!(b["tpTriggerPx"], "42000");
    assert_eq!(b["tpOrdPx"], "-1");
    assert_eq!(b["tpTriggerPxType"], "last");
    assert!(b.get("sz").is_none());
}

#[test]
fn venue_51088_on_a_placement_is_a_definitive_rejection() {
    let mut e = trading_engine();
    let out = place(&mut e, &[FULL_TP]);
    let sent = writes(&out);
    assert!(path(sent[0]).ends_with("/api/v5/trade/order-algo"));
    reply(
        &mut e,
        sent[0],
        r#"{"code":"1","msg":"","data":[{"algoId":"","algoClOrdId":"","sCode":"51088","sMsg":"You can only place 1 TP/SL order to close an entire position"}]}"#,
        6.,
    );
    let out = prepared(&mut e, 6.);
    let error = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 1, error }) => Some(error),
            _ => None,
        })
        .expect("refusal reaches the caller");
    assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    assert_eq!(error.venue_code.as_deref(), Some("51088"));
    assert!(error.partial.is_none(), "{error:?}");
}
