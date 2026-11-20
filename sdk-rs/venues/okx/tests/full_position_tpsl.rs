//! AGT-68: OKX holds one entire-position TP/SL algo per position (51088). A full
//! SL and TP placed together become one `oco`; a full leg beside a held full
//! algo is refused before any write.
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

/// Adding a leg to an existing close-all algo through amend-algos is
/// undocumented, and ccxt warns a two-leg `conditional` applies only its SL. A
/// false "applied" protective leg is worse than a visible refusal, so a full leg
/// beside a held full algo is refused before any write.
#[test]
fn a_full_leg_beside_a_held_full_algo_is_refused_before_any_write() {
    let held_stop = full_leg("77_sl", OrderType::StopLoss, OrderSide::Sell);
    let held_take_profit = full_leg("78_tp", OrderType::TakeProfit, OrderSide::Sell);
    let cases: [(&Order, &[&str]); 6] = [
        (&held_stop, &[FULL_TP]),
        (&held_take_profit, &[FULL_SL]),
        (&held_stop, &[FULL_SL]),
        // Both legs in one command while one is held: never send one leg and
        // collect 51088 on the other.
        (&held_stop, &[FULL_SL, FULL_TP]),
        (&held_take_profit, &[FULL_TP, FULL_SL]),
        (
            &held_stop,
            &[
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000}"#,
                FULL_TP,
            ],
        ),
    ];
    for (held, orders) in cases {
        let mut e = trading_engine();
        e.store_mut().upsert_order(held.clone());
        let out = place(&mut e, orders);
        assert!(writes(&out).is_empty(), "{orders:?}: {out:?}");
        let error = refusal(&out);
        assert_eq!(error.outcome, Some(Outcome::NotSent), "{error:?}");
        assert_eq!(error.message, HELD, "{error:?}");
        assert_eq!(e.store().order(&held.id), Some(held));
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
