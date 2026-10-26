mod support;

use safe_cex_engine::model::{OrderSide, OrderStatus, OrderType};
use safe_cex_engine::{Command, Method};
use serde_json::{Value, json};
use support::Harness;

#[test]
fn old_id_cancellation_does_not_replace_acknowledged_amend_price_or_size() {
    for resolve in [false, true] {
        let mut h = Harness::classic();
        h.command(Command::AmendOrder {
            command: 1,
            id: "1".into(),
            price: Some(95.0),
            amount: Some(3.0),
        });
        let request = h.take("/modify-order");
        let acknowledgement = if resolve {
            h.reply(
                request.id,
                200,
                r#"{"code":"00000","data":{"orderId":"1"}}"#,
            );
            h.take("/order/detail").id
        } else {
            request.id
        };
        h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"canceled"}]}"#);
        h.reply(
            acknowledgement,
            200,
            r#"{"code":"00000","data":{"orderId":"2","status":"live","baseVolume":"0"}}"#,
        );
        let replacement = h.engine.store().order("2").unwrap();
        assert_eq!(replacement.price, 95.0);
        assert_eq!(replacement.amount, 3.0);
        assert_eq!(replacement.remaining, 3.0);

        h.command(Command::AmendOrder {
            command: 2,
            id: "2".into(),
            price: Some(96.0),
            amount: None,
        });
        let request = h.take("/modify-order");
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body["newSize"], "3");
    }
}

#[test]
fn replacement_id_stream_values_survive_an_older_amend_acknowledgement() {
    let mut h = Harness::classic();
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(95.0),
        amount: Some(3.0),
    });
    let request = h.take("/modify-order");
    h.private_message(r#"{"arg":{"channel":"orders"},"data":[{"orderId":"2","symbol":"BTCUSDT","size":"3","baseVolume":"0.5","price":"95","side":"buy","orderType":"limit","status":"partially_filled"}]}"#);
    h.reply(
        request.id,
        200,
        r#"{"code":"00000","data":{"orderId":"2"}}"#,
    );
    let replacement = h.engine.store().order("2").unwrap();
    assert_eq!(replacement.price, 95.0);
    assert_eq!(replacement.amount, 3.0);
    assert_eq!(replacement.filled, 0.5);
    assert_eq!(replacement.remaining, 2.5);
}

#[test]
fn terminal_replacement_retires_old_identity_without_reopening_the_replacement() {
    for resolve in [false, true] {
        for (status, expected_status) in [
            ("filled", OrderStatus::Closed),
            ("canceled", OrderStatus::Canceled),
        ] {
            for previously_visible in [false, true] {
                let mut h = Harness::classic();
                h.command(Command::AmendOrder {
                    command: 1,
                    id: "1".into(),
                    price: Some(95.0),
                    amount: Some(3.0),
                });
                let request = h.take("/modify-order");
                let acknowledgement = if resolve {
                    h.reply(
                        request.id,
                        200,
                        r#"{"code":"00000","data":{"orderId":"1"}}"#,
                    );
                    h.take("/order/detail").id
                } else {
                    request.id
                };
                let mut replacement = json!({
                    "orderId":"2", "symbol":"BTCUSDT", "size":"3", "baseVolume":"0",
                    "price":"95", "side":"buy", "orderType":"limit", "status":"live"
                });
                if previously_visible {
                    h.private_message(
                        &json!({"arg":{"channel":"orders"},"data":[replacement.clone()]})
                            .to_string(),
                    );
                }
                replacement["status"] = json!(status);
                if status == "filled" {
                    replacement["baseVolume"] = json!("3");
                }
                h.private_message(
                    &json!({"arg":{"channel":"orders"},"data":[replacement]}).to_string(),
                );
                h.reply(
                    acknowledgement,
                    200,
                    r#"{"code":"00000","data":{"orderId":"2"}}"#,
                );
                assert_eq!(
                    h.engine.store().order("1").unwrap().status,
                    OrderStatus::Canceled
                );
                if previously_visible {
                    assert_eq!(h.engine.store().order("2").unwrap().status, expected_status);
                } else {
                    assert!(h.engine.store().order("2").is_none());
                }
                assert_eq!(h.done.last().unwrap().1.as_ref().unwrap(), "[\"2\"]");
            }
        }
    }
}

fn uta_trailing(
    side: &str,
    position: Option<&str>,
    reduce_only: Option<&str>,
    hedged: bool,
) -> Harness {
    let mut h = Harness::private_started();
    h.respond(
        "/account/settings",
        200,
        &json!({"code":"00000","data":{"holdMode":if hedged {"hedge_mode"}else{"one_way_mode"}}})
            .to_string(),
    );
    h.respond(
        "/current-position",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("/account/assets", 200, r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"1000","unrealisedPnl":"0"}}"#);
    h.respond(
        "/unfilled-orders",
        200,
        r#"{"code":"00000","data":{"list":[]}}"#,
    );
    h.respond("type=tpsl", 200, r#"{"code":"00000","data":[]}"#);
    // Official unfilled-strategy-orders schema: trailing params are an object;
    // side/posSide establish opening intent without a reduceOnly response field.
    // https://www.bitget.com/docs/catalog/trading/strategy-trading
    let mut row = json!({
        "category":"USDT-FUTURES", "symbol":"BTCUSDT", "type":"trailing_stop",
        "orderId":"external-opening", "qty":"0.01", "side":side,
        "status":"pending", "trailingStopParams":{
            "activationPrice":"95", "activationType":"market", "trailType":"spread",
            "trailVariance":"5", "preOrderType":"market"
        }
    });
    if let Some(position) = position {
        row["posSide"] = json!(position);
    }
    if let Some(reduce_only) = reduce_only {
        row["reduceOnly"] = json!(reduce_only);
    }
    h.respond(
        "type=trailing_stop",
        200,
        &json!({"code":"00000","data":[row]}).to_string(),
    );
    h.login();
    h
}

fn metadata(h: &mut Harness) {
    while h
        .requests
        .iter()
        .any(|r| r.url.contains("/account/settings"))
    {
        let mode = if h.engine.store().hedged() {
            "hedge_mode"
        } else {
            "one_way_mode"
        };
        h.respond(
            "/account/settings",
            200,
            &json!({"code":"00000","data":{"holdMode":mode}}).to_string(),
        );
    }
}

fn rejects_edits_and_allows_cancel(h: &mut Harness) {
    for (command, price, amount) in [(1, None, Some(0.02)), (2, Some(96.0), None)] {
        h.command(Command::AmendOrder {
            command,
            id: "external-opening".into(),
            price,
            amount,
        });
        metadata(h);
        assert!(
            h.done
                .last()
                .unwrap()
                .1
                .as_ref()
                .unwrap_err()
                .contains("native trailing parameters")
        );
        assert!(h.requests.iter().all(|r| r.method == Method::Get));
        assert_eq!(
            h.engine.store().order("external-opening").unwrap().status,
            OrderStatus::Open
        );
    }
    h.command(Command::CancelOrders {
        command: 3,
        ids: vec!["external-opening".into()],
    });
    let request = h.take("/cancel-strategy-order");
    let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(body, json!({"orderId":"external-opening"}));
    h.reply(request.id, 200, r#"{"code":"00000","data":null}"#);
    assert!(h.done.last().unwrap().1.is_ok());
}

#[test]
fn external_opening_uta_trailing_stays_visible_and_cancelable_but_cannot_be_edited() {
    for (side, position, expected_side) in [
        ("buy", "long", OrderSide::Buy),
        ("sell", "short", OrderSide::Sell),
    ] {
        let mut h = uta_trailing(side, Some(position), None, true);
        let order = h.engine.store().order("external-opening").unwrap();
        assert_eq!(order.kind, OrderType::TrailingStopLoss);
        assert_eq!(order.side, expected_side);
        assert_eq!(order.status, OrderStatus::Open);
        assert!(!order.reduce_only);
        rejects_edits_and_allows_cancel(&mut h);
    }
}

#[test]
fn ambiguous_one_way_uta_trailing_edits_fail_before_canceling() {
    // Defensive read shapes: the placement schema permits side-only one-way
    // openings, but REST does not specify whether their posSide is absent/net.
    for side in ["buy", "sell"] {
        for position in [None, Some("net")] {
            for reduce_only in [None, Some("no")] {
                let mut h = uta_trailing(side, position, reduce_only, false);
                let order = h.engine.store().order("external-opening").unwrap();
                assert_eq!(order.status, OrderStatus::Open);
                assert!(!order.reduce_only);
                rejects_edits_and_allows_cancel(&mut h);
            }
        }
    }
}

#[test]
fn explicit_one_way_reduce_only_trailing_stays_editable() {
    for side in ["buy", "sell"] {
        for position in [None, Some("net")] {
            let mut h = uta_trailing(side, position, Some("yes"), false);
            assert!(
                h.engine
                    .store()
                    .order("external-opening")
                    .unwrap()
                    .reduce_only
            );
            h.command(Command::AmendOrder {
                command: 1,
                id: "external-opening".into(),
                price: None,
                amount: Some(0.02),
            });
            metadata(&mut h);
            h.respond(
                "/cancel-strategy-order",
                200,
                r#"{"code":"00000","data":null}"#,
            );
            let request = h.take("/place-strategy-order");
            let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            assert_eq!(body["side"], side);
            assert_eq!(body["reduceOnly"], "yes");
            assert_eq!(body["qty"], "0.02");
            assert!(body.get("posSide").is_none());
        }
    }
}
