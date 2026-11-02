//! Bitunix audit findings: a two-leg position TP/SL and full-position merges.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::test_support::prepared_now;
use safe_cex_engine::{Action, Command, Event, HttpRequest, Method, Notice};
use support::*;

fn writes(actions: &[Action]) -> Vec<HttpRequest> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::Http(r) if r.method != Method::Get => Some(r.clone()),
            _ => None,
        })
        .collect()
}
fn outcome(actions: &[Action], command: u32) -> Option<Result<String, String>> {
    actions.iter().find_map(|a| match a {
        Action::Notice(Notice::CommandDone { command: c, result }) if *c == command => {
            Some(Ok(result.clone()))
        }
        Action::Notice(Notice::CommandFailed { command: c, error }) if *c == command => {
            Some(Err(error.legacy_text()))
        }
        _ => None,
    })
}
fn body(r: &HttpRequest) -> serde_json::Value {
    serde_json::from_str(r.body.as_deref().unwrap()).unwrap()
}

/// Cancels the TP leg, acks the cancel, and returns the follow-up writes and actions.
fn cancel_tp_leg() -> (TestEngine, Vec<HttpRequest>, Vec<Action>) {
    let (mut e, _) = startup();
    prepared_now(&mut e);
    e.handle(
        1.0,
        Event::Command(Command::CancelOrders {
            command: 1,
            ids: vec!["protect__position-1__tp".into()],
        }),
    );
    let sent = writes(&prepared_now(&mut e));
    assert!(
        sent[0].url.contains("/tpsl/cancel_order"),
        "{}",
        sent[0].url
    );
    reply(
        &mut e,
        sent[0].id,
        r#"{"code":0,"data":{"orderId":"protect"}}"#,
    );
    let after = prepared_now(&mut e);
    (e, writes(&after), after)
}

#[test]
fn cancelling_one_leg_of_a_position_tpsl_replaces_the_other() {
    let (mut e, follow, after) = cancel_tp_leg();
    let place = follow
        .iter()
        .find(|r| r.url.ends_with("/tpsl/place_order"))
        .unwrap_or_else(|| {
            panic!(
                "the kept stop goes back: {:?} {:?}",
                follow.iter().map(|r| &r.url).collect::<Vec<_>>(),
                outcome(&after, 1)
            )
        });
    let p = body(place);
    assert_eq!(p["slPrice"], "100000");
    assert_eq!(p["slStopType"], "MARK_PRICE");
    assert_eq!(p["slQty"], "0.01");
    assert!(p.get("tpPrice").is_none_or(|v| v.is_null()), "{p}");
    reply(&mut e, place.id, r#"{"code":0,"data":{"orderId":"kept"}}"#);
    assert!(matches!(outcome(&prepared_now(&mut e), 1), Some(Ok(_))));
}

#[test]
fn a_refused_kept_leg_says_the_position_has_no_stop() {
    let (mut e, follow, _) = cancel_tp_leg();
    let place = follow
        .iter()
        .find(|r| r.url.ends_with("/tpsl/place_order"))
        .expect("the kept stop goes back");
    reply(&mut e, place.id, r#"{"code":30001,"msg":"refused"}"#);
    let result = outcome(&prepared_now(&mut e), 1).expect("an outcome");
    let error = result.expect_err("never success with both legs gone");
    assert!(error.contains("no stop"), "{error}");
}

const HALF: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","id":"half","positionId":"position-1","tpPrice":null,"slPrice":"99000","tpQty":null,"slQty":"0.005","slStopType":"MARK_PRICE","slOrderType":"MARKET"}]}"#;

#[test]
fn a_full_position_tp_does_not_copy_a_partial_stop() {
    let (mut e, _) = startup();
    prepared_now(&mut e);
    let mut o: PlaceOrderOpts = serde_json::from_str(
        r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","price":90000,"amount":0}"#,
    )
    .unwrap();
    o.kind = OrderType::TakeProfit;
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 2,
            orders: vec![o],
        }),
    );
    // The placement re-reads the account first; the venue holds only a half-size stop.
    let mut actions = prepared_now(&mut e);
    let mut sent = Vec::new();
    for _ in 0..50 {
        sent.extend(writes(&actions));
        let reads: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::Http(r) if r.method == Method::Get => Some(r.clone()),
                _ => None,
            })
            .collect();
        if reads.is_empty() {
            break;
        }
        actions = Vec::new();
        for r in reads {
            let answer = if r.url.contains("/tpsl/") {
                HALF
            } else if r.url.contains("get_pending_positions") {
                POSITIONS
            } else if r.url.contains("get_pending_orders") {
                ORDERS
            } else {
                ACCOUNT
            };
            reply(&mut e, r.id, answer);
            actions.extend(prepared_now(&mut e));
        }
    }
    let place = sent
        .iter()
        .find(|r| r.url.contains("/tpsl/position/place_order"))
        .expect("a full-position TP");
    let p = body(place);
    assert_eq!(p["tpPrice"], "90000");
    assert!(p.get("slPrice").is_none_or(|v| v.is_null()), "{p}");
}
