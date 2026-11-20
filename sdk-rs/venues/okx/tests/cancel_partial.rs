//! AGT-69: a partially successful OKX bulk cancel (`code` 1/2 with per-row
//! results) no longer stops the plan before the algo cancels. Every row is
//! settled from its own sCode, and the terminal names exactly what is left.
#![allow(dead_code)]
mod support;

use safe_cex_engine::model::*;
use safe_cex_engine::{Action, Command, Engine, Event, Method, Notice, Outcome};
use safe_cex_venue_okx::Okx;
use support::*;

const OWNER: u32 = 69;

fn row(id: &str, kind: OrderType) -> Order {
    Order {
        id: id.into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind,
        side: if kind == OrderType::StopLoss {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        },
        price: 38000.,
        amount: if kind == OrderType::StopLoss { 0. } else { 1. },
        filled: 0.,
        remaining: 1.,
        reduce_only: kind == OrderType::StopLoss,
        close: None,
        client_order_id: None,
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 4.,
        trigger_type: Some(StopTriggerType::LastPrice),
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}

/// The round-3 book: a full SL, a conditional entry and three limits.
fn round_three() -> (Engine<Okx>, Vec<String>) {
    let mut e = trading_engine();
    let book = [
        ("4114011563_sl", OrderType::StopLoss),
        ("4114014121", OrderType::Limit),
        ("4114014928", OrderType::Conditional),
        ("4114015612", OrderType::Limit),
        ("4114015971", OrderType::Limit),
    ];
    for (id, kind) in book {
        e.store_mut().upsert_order(row(id, kind));
    }
    (e, book.iter().map(|(id, _)| (*id).to_owned()).collect())
}

fn cancel(e: &mut Engine<Okx>, ids: Vec<String>) -> Vec<Action> {
    e.handle(
        5.,
        Event::Command(Command::CancelOrders {
            command: OWNER,
            ids,
        }),
    );
    prepared(e, 5.)
}

fn writes(out: &[Action]) -> Vec<&Action> {
    out.iter()
        .filter(|a| matches!(a, Action::Http(r) if r.method != Method::Get))
        .collect()
}

fn url(a: &Action) -> &str {
    let Action::Http(r) = a else {
        panic!("not HTTP")
    };
    &r.url
}

fn rows(a: &Action) -> Vec<serde_json::Value> {
    let Action::Http(r) = a else {
        panic!("not HTTP")
    };
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}

/// Echo each requested row with the scripted sCode (`"0"` unless named).
fn answer(
    e: &mut Engine<Okx>,
    a: &Action,
    code: &str,
    refusals: &[(&str, &str, &str)],
    now: f64,
) -> Vec<Action> {
    let data: Vec<_> = rows(a)
        .into_iter()
        .map(|mut row| {
            let id = row["ordId"]
                .as_str()
                .or(row["algoId"].as_str())
                .unwrap()
                .to_owned();
            let refusal = refusals.iter().find(|(target, ..)| *target == id);
            row["sCode"] = refusal.map_or("0", |(_, code, _)| code).into();
            row["sMsg"] = refusal.map_or("", |(_, _, msg)| msg).into();
            row
        })
        .collect();
    let msg = if code == "0" {
        ""
    } else {
        "Bulk operation partially successful"
    };
    reply(
        e,
        a,
        &serde_json::json!({"code":code,"msg":msg,"data":data}).to_string(),
        now,
    );
    prepared(e, now)
}

fn terminal(out: &[Action]) -> Option<Result<&str, &safe_cex_engine::VenueError>> {
    out.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone {
            command: OWNER,
            result,
        }) => Some(Ok(result.as_str())),
        Action::Notice(Notice::CommandFailed {
            command: OWNER,
            error,
        }) => Some(Err(error)),
        _ => None,
    })
}

#[test]
fn an_already_gone_row_in_a_partial_batch_still_cancels_the_algo_orders() {
    let (mut e, ids) = round_three();
    let out = cancel(&mut e, ids);
    let sent = writes(&out);
    assert_eq!(sent.len(), 1);
    assert!(url(sent[0]).ends_with("/cancel-batch-orders"));
    let out = answer(
        &mut e,
        sent[0],
        "2",
        &[(
            "4114015971",
            "51400",
            "Order cancellation failed as the order has been filled, canceled or does not exist",
        )],
        6.,
    );
    assert!(
        terminal(&out).is_none(),
        "the algo group still runs: {out:?}"
    );
    let sent = writes(&out);
    assert_eq!(sent.len(), 1, "{out:?}");
    assert!(url(sent[0]).ends_with("/cancel-algos"));
    let algo_ids: Vec<_> = rows(sent[0])
        .iter()
        .map(|r| r["algoId"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(algo_ids, ["4114011563", "4114014928"]);
    let out = answer(&mut e, sent[0], "0", &[], 7.);
    assert!(writes(&out).is_empty());
    assert_eq!(
        terminal(&out),
        Some(Ok("[]")),
        "every target is cancelled or already gone: {out:?}"
    );
    // As on master, completion restores the optimistic status; only the order
    // stream or census publishes Canceled (`replay correctness (okx)` pins this).
    assert!(
        e.store()
            .orders()
            .iter()
            .all(|o| o.status == OrderStatus::Open),
        "{:?}",
        e.store().orders()
    );
}

#[test]
fn a_refused_row_is_reported_by_id_after_every_other_group_is_cancelled() {
    let (mut e, ids) = round_three();
    let out = cancel(&mut e, ids);
    let out = answer(
        &mut e,
        writes(&out)[0],
        "2",
        &[("4114015971", "51000", "Parameter error")],
        6.,
    );
    assert!(terminal(&out).is_none(), "{out:?}");
    let algos = writes(&out)[0].clone();
    assert!(url(&algos).ends_with("/cancel-algos"));
    let out = answer(&mut e, &algos, "0", &[], 7.);
    let Some(Err(error)) = terminal(&out) else {
        panic!("a refused target fails the command: {out:?}")
    };
    assert!(error.message.contains("4114015971"), "{error:?}");
    assert!(
        error.message.contains("Parameter error (code 51000)"),
        "{error:?}"
    );
    assert!(error.message.contains("1 of 5"), "{error:?}");
    assert_ne!(error.outcome, Some(Outcome::Unknown), "{error:?}");
    let elements =
        serde_json::to_value(error.elements.as_ref().expect("per-order outcomes")).unwrap();
    let statuses: Vec<_> = elements
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["index"].as_u64().unwrap(),
                e["status"].as_str().unwrap().to_owned(),
                e["targetOrderId"].as_str().map(str::to_owned),
            )
        })
        .collect();
    assert_eq!(statuses.len(), 5, "{elements}");
    assert!(
        statuses
            .iter()
            .all(|(_, status, _)| status == "applied" || status == "notSent"),
        "{elements}"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|(_, status, _)| status == "notSent")
            .count(),
        1,
        "{elements}"
    );
    let indices: Vec<_> = statuses.iter().map(|(index, ..)| *index).collect();
    assert_eq!(indices, [0, 1, 2, 3, 4]);
}

#[test]
fn a_wholly_refused_cancel_is_a_definitive_rejection() {
    let mut e = trading_engine();
    e.store_mut().upsert_order(row("101", OrderType::Limit));
    let out = cancel(&mut e, vec!["101".into()]);
    let out = answer(
        &mut e,
        writes(&out)[0],
        "1",
        &[("101", "51000", "Parameter error")],
        6.,
    );
    assert!(writes(&out).is_empty());
    let Some(Err(error)) = terminal(&out) else {
        panic!("{out:?}")
    };
    assert_eq!(error.outcome, Some(Outcome::Rejected), "{error:?}");
    assert_eq!(error.venue_code.as_deref(), Some("51000"));
}

#[test]
fn an_unproven_row_still_stops_before_the_algo_group() {
    let (mut e, ids) = round_three();
    let out = cancel(&mut e, ids);
    let out = answer(
        &mut e,
        writes(&out)[0],
        "2",
        &[("4114015971", "51412", "Cancellation timed out")],
        6.,
    );
    assert!(
        writes(&out).is_empty(),
        "uncertain rows never continue: {out:?}"
    );
    let Some(Err(error)) = terminal(&out) else {
        panic!("{out:?}")
    };
    assert_eq!(error.outcome, Some(Outcome::Unknown));
}
