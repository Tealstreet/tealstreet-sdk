//! AGT-68: OKX holds one entire-position TP/SL algo per position (51088), so a
//! second full-position leg joins the existing algo instead of placing another.
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

#[test]
fn a_full_take_profit_joins_the_existing_full_stop_algo() {
    let mut e = trading_engine();
    e.store_mut()
        .upsert_order(full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell));
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
    assert_eq!(b["newTpTriggerPxType"], "last");
    // The existing stop leg and the close-all sentinel are untouched (51523/51524).
    for absent in [
        "newSlTriggerPx",
        "newSlTriggerPxType",
        "newTpOrdPx",
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
    assert!(
        out.iter().any(|a| matches!(a,
            Action::Notice(Notice::CommandDone { command: 1, result }) if result == "[\"77\"]")),
        "{out:?}"
    );
}

#[test]
fn a_full_stop_joins_the_existing_full_take_profit_algo() {
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
    assert_eq!(b["newSlTriggerPxType"], "mark");
    for absent in [
        "newTpTriggerPx",
        "newTpTriggerPxType",
        "newSlOrdPx",
        "newSz",
    ] {
        assert!(b.get(absent).is_none(), "{absent}: {b}");
    }
}

#[test]
fn a_refused_join_reports_a_rejection_and_leaves_the_existing_stop() {
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
    let error = out
        .iter()
        .find_map(|a| match a {
            Action::Notice(Notice::CommandFailed { command: 1, error }) => Some(error),
            _ => None,
        })
        .expect("refusal reaches the caller");
    assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    assert_eq!(error.venue_code.as_deref(), Some("51279"));
    assert_eq!(e.store().order("77_sl"), Some(&stop));
}

#[test]
fn a_full_leg_is_placed_when_no_compatible_full_algo_exists() {
    let partial = {
        let mut o = full_leg("70_sl", OrderType::StopLoss, OrderSide::Sell);
        o.amount = 1.;
        o.remaining = 1.;
        o
    };
    let other_position = full_leg("71_sl", OrderType::StopLoss, OrderSide::Buy);
    let both_legs = [
        full_leg("72_sl", OrderType::StopLoss, OrderSide::Sell),
        full_leg("72_tp", OrderType::TakeProfit, OrderSide::Sell),
    ];
    let canceled = {
        let mut o = full_leg("73_sl", OrderType::StopLoss, OrderSide::Sell);
        o.status = OrderStatus::Canceled;
        o
    };
    for store in [
        vec![],
        vec![partial],
        vec![other_position],
        both_legs.to_vec(),
        vec![canceled],
    ] {
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
