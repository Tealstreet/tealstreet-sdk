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

// Live trading_pairs BTCUSDT: "maxLimitOrderVolume":"1200","maxMarketOrderVolume":"120".
const LIVE_PAIRS: &str = r#"{"code":0,"data":[{"symbol":"BTCUSDT","base":"BTC","quote":"USDT","minTradeVolume":"0.0001","maxLimitOrderVolume":"1200","maxMarketOrderVolume":"120","basePrecision":4,"quotePrecision":1,"symbolStatus":"OPEN","isApiSupported":true}],"msg":"Success"}"#;

fn placed_quantities(kind: &str) -> Vec<String> {
    let (mut e, _) = startup_serving(
        engine(Options {
            key: Some("replay-key".into()),
            secret: Some("replay-secret".into()),
            ..Default::default()
        }),
        LIVE_PAIRS,
        TICKERS,
    );
    prepared_now(&mut e);
    let o: PlaceOrderOpts = serde_json::from_str(&format!(
        r#"{{"symbol":"BTCUSDT","type":"{kind}","side":"buy","price":90000,"amount":250}}"#
    ))
    .unwrap();
    e.handle(
        1.0,
        Event::Command(Command::PlaceOrders {
            command: 3,
            orders: vec![o],
        }),
    );
    let sent = requests(&mut e);
    assert_eq!(
        sent.len(),
        1,
        "{:?}",
        sent.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    let b = body(&sent[0]);
    match b["orderList"].as_array() {
        Some(rows) => rows
            .iter()
            .map(|r| r["qty"].as_str().unwrap().to_owned())
            .collect(),
        None => vec![b["qty"].as_str().unwrap().to_owned()],
    }
}

#[test]
fn a_market_order_splits_at_the_market_maximum_and_a_limit_at_the_limit_maximum() {
    let market = placed_quantities("market");
    assert_eq!(market, ["80", "80", "80", "10"]);
    assert_eq!(
        market
            .iter()
            .map(|q| q.parse::<f64>().unwrap())
            .sum::<f64>(),
        250.0
    );
    let limit = placed_quantities("limit");
    assert_eq!(limit, ["250"]);
    assert_eq!(
        limit.iter().map(|q| q.parse::<f64>().unwrap()).sum::<f64>(),
        250.0
    );
}

#[test]
fn a_cancel_all_failure_list_fails_naming_the_orders_and_keeps_the_successes() {
    let (mut e, _) = startup();
    prepared_now(&mut e);
    let rows = e.store().orders().to_vec();
    e.store_mut().set_orders(Vec::new());
    e.handle(
        1.0,
        Event::Command(Command::CancelSymbolOrders {
            command: 4,
            symbol: "BTCUSDT".into(),
        }),
    );
    let sweep = requests(&mut e).remove(0);
    assert!(
        sweep.url.ends_with("/trade/cancel_all_orders"),
        "{}",
        sweep.url
    );
    // "one" reached the store after the plan; only the sweep can cancel it.
    e.store_mut().set_orders(rows);
    // The documented answer: bitunix.com/api-docs futures "Cancel all orders".
    reply(
        &mut e,
        sweep.id,
        r#"{"code":0,"data":{"successList":[{"orderId":"one","clientId":"web"}],"failureList":[{"orderId":"11112","clientId":"22223","errorMsg":"Order status error","errorCode":10013}]},"msg":"Success"}"#,
    );
    let error = outcome(&prepared_now(&mut e), 4)
        .expect("an outcome")
        .expect_err("a refused order was not cancelled");
    assert!(error.contains("11112"), "{error}");
    assert!(error.contains("Order status error"), "{error}");
    assert_eq!(
        e.store().order("one").unwrap().status,
        OrderStatus::Canceled
    );
}
