mod support;

use safe_cex_engine::{Command, ErrorKind, Method, Outcome};
use serde_json::{Value, json};
use support::*;

fn harness(uta: bool, min: f64, max: f64, step: f64, value: f64) -> Harness {
    let mut h = Harness::started(FixtureOptions {
        key: Some("fixture-key".into()),
        secret: Some("fixture-secret".into()),
        passphrase: Some("fixture-passphrase".into()),
        ..FixtureOptions::default()
    });
    let mut contracts: Value = serde_json::from_str(CONTRACTS).expect("contracts fixture");
    let row = &mut contracts["data"][0];
    row["minTradeNum"] = json!(min.to_string());
    row["maxOrderQty"] = json!(max.to_string());
    row["sizeMultiplier"] = json!(step.to_string());
    row["minTradeUSDT"] = json!(value.to_string());
    h.respond("/contracts", 200, &contracts.to_string());
    h.respond("/tickers", 200, TICKERS);
    h.respond(
        "/account/settings",
        200,
        if uta {
            r#"{"code":"00000","data":{"holdMode":"one_way_mode"}}"#
        } else {
            r#"{"code":"40084","msg":"Classic Account mode","data":null}"#
        },
    );
    if uta {
        h.respond(
            "/current-position",
            200,
            &POSITION
                .replace("\"data\":[", "\"data\":{\"list\":[")
                .replace("]}", "]}}"),
        );
        h.respond("/account/assets", 200, r#"{"code":"00000","data":{"accountEquity":"1000","effEquity":"900","unrealisedPnl":"0"}}"#);
        h.respond(
            "/unfilled-orders",
            200,
            r#"{"code":"00000","data":{"list":[]}}"#,
        );
        while h
            .requests
            .iter()
            .any(|r| r.url.contains("/unfilled-strategy-orders"))
        {
            h.respond(
                "/unfilled-strategy-orders",
                200,
                r#"{"code":"00000","data":[]}"#,
            );
        }
    } else {
        h.respond("/all-position", 200, POSITION);
        h.respond("/account/accounts", 200, ACCOUNT);
        h.orders(OPEN_ORDER);
    }
    h.login();
    h.requests.clear();
    h
}

fn order(amount: f64) -> Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":100.0})
}

fn place(h: &mut Harness, orders: Vec<Value>) {
    h.command(Command::PlaceOrders {
        command: 1,
        orders: orders
            .into_iter()
            .map(|v| serde_json::from_value(v).expect("order fixture"))
            .collect(),
    });
}

fn refused(h: &Harness) {
    assert_eq!(
        h.requests.len(),
        0,
        "planning refusal must emit zero transport requests"
    );
    assert_eq!(h.failures.len(), 1, "one typed terminal refusal");
    assert_eq!(h.failures[0].kind, Some(ErrorKind::InvalidInput));
    assert_eq!(h.failures[0].outcome, Some(Outcome::NotSent));
}

fn settle(h: &mut Harness, uta: bool) -> Vec<Value> {
    let mut rows = vec![];
    let mut calls = 0;
    while let Some(index) = h.requests.iter().position(|r| r.method != Method::Get) {
        assert!(calls < 100, "unexpected resend");
        let request = h.requests.remove(index);
        assert_eq!(request.method, Method::Post);
        let payload: Value =
            serde_json::from_str(request.body.as_deref().expect("body")).expect("JSON body");
        if uta {
            assert!(request.url.ends_with("/place-order"));
            rows.push(payload);
            h.reply(
                request.id,
                200,
                &json!({"code":"00000","data":{"orderId":format!("id-{calls}")}}).to_string(),
            );
        } else {
            assert!(request.url.ends_with("/batch-place-order"));
            let batch = payload["orderList"].as_array().expect("batch rows");
            assert!(batch.len() <= 50);
            let replies: Vec<_> = batch
                .iter()
                .enumerate()
                .map(|(index, _)| json!({"orderId":format!("id-{calls}-{index}")}))
                .collect();
            rows.extend(batch.iter().cloned());
            h.reply(
                request.id,
                200,
                &json!({"code":"00000","data":{"successList":replies,"failureList":[]}})
                    .to_string(),
            );
        }
        calls += 1;
    }
    assert_eq!(
        calls,
        if uta {
            rows.len()
        } else {
            rows.len().div_ceil(50)
        }
    );
    assert!(
        h.done.iter().any(|(id, result)| *id == 1 && result.is_ok()),
        "{:?}",
        h.done
    );
    let mut ids: Vec<_> = rows
        .iter()
        .map(|row| row["clientOid"].as_str().expect("native client ID"))
        .collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), rows.len(), "each child is placed exactly once");
    rows
}

#[test]
fn split_preflight_rebalances_the_audited_tail_and_preserves_valid_lots() {
    for uta in [false, true] {
        for reduce in [false, true] {
            for (total, expected) in [(4.0, vec!["2", "2"]), (6.0, vec!["3", "3"])] {
                let mut h = harness(uta, 2.0, 3.0, 1.0, 5.0);
                let mut o = order(total);
                o["reduceOnly"] = json!(reduce);
                o["takeProfit"] = json!(120.0);
                o["stopLoss"] = json!(80.0);
                place(&mut h, vec![o]);
                let rows = settle(&mut h, uta);
                let field = if uta { "qty" } else { "size" };
                assert_eq!(
                    rows.iter()
                        .map(|r| r[field].as_str().expect("quantity"))
                        .collect::<Vec<_>>(),
                    expected
                );
                for row in rows {
                    assert_eq!(
                        row[if uta {
                            "takeProfit"
                        } else {
                            "presetStopSurplusPrice"
                        }],
                        "120"
                    );
                    assert_eq!(
                        row[if uta {
                            "stopLoss"
                        } else {
                            "presetStopLossPrice"
                        }],
                        "80"
                    );
                    assert_eq!(
                        row["reduceOnly"],
                        if uta {
                            if reduce { "yes" } else { "no" }
                        } else if reduce {
                            "YES"
                        } else {
                            "NO"
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn split_preflight_impossible_tail_is_not_sent() {
    for uta in [false, true] {
        let mut h = harness(uta, 3.0, 3.0, 1.0, 5.0);
        place(&mut h, vec![order(4.0)]);
        refused(&h);
    }
}

#[test]
fn split_preflight_checks_every_row_before_the_first_chunk_or_uta_write() {
    for uta in [false, true] {
        for last in [1.0, 0.1] {
            let mut h = harness(uta, 2.0, 3.0, 1.0, 5.0);
            let mut orders = vec![order(2.0); 50];
            orders.push(order(last));
            place(&mut h, orders);
            refused(&h);
        }
        let mut h = harness(uta, 2.0, 3.0, 1.0, 5.0);
        place(&mut h, vec![order(2.0); 51]);
        assert_eq!(settle(&mut h, uta).len(), 51);
    }
}

#[test]
fn split_preflight_checks_each_lots_notional_at_the_wire_price() {
    for uta in [false, true] {
        let mut h = harness(uta, 1.0, 3.0, 1.0, 201.0);
        place(&mut h, vec![order(4.0)]);
        refused(&h);
        let mut h = harness(uta, 0.1, 3.0, 0.1, 5.002);
        let mut o = order(0.1);
        o["price"] = json!(50.04);
        place(&mut h, vec![o]);
        refused(&h);
        let mut h = harness(uta, 1.0, 3.0, 1.0, 201.0);
        place(&mut h, vec![order(6.0)]);
        assert_eq!(settle(&mut h, uta).len(), 2);
    }
}

#[test]
fn split_preflight_rejects_attached_prices_that_round_to_zero() {
    for uta in [false, true] {
        for field in ["takeProfit", "stopLoss"] {
            let mut h = harness(uta, 1.0, 3.0, 1.0, 5.0);
            let mut bad = order(2.0);
            bad[field] = json!(0.04);
            place(&mut h, vec![order(2.0), bad]);
            refused(&h);
        }
    }
}

#[test]
fn split_preflight_invalid_amend_plan_never_modifies_or_cancels() {
    let mut h = harness(false, 3.0, 3.0, 1.0, 5.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(100.0),
        amount: Some(4.0),
    });
    refused(&h);
    assert_eq!(
        h.engine.store().order("1").expect("original order").status,
        safe_cex_engine::model::OrderStatus::Open
    );
}

#[test]
fn split_preflight_floors_off_step_caps_and_never_loses_a_finite_maximum() {
    for uta in [false, true] {
        let mut h = harness(uta, 0.1, 0.95, 0.1, 0.0);
        place(&mut h, vec![order(1.8)]);
        let rows = settle(&mut h, uta);
        assert_eq!(rows.len(), 2);
        assert!(
            rows.iter()
                .all(|row| row[if uta { "qty" } else { "size" }] == "0.9")
        );
        let mut h = harness(uta, 0.0, 1e300, 1e292, 0.0);
        place(&mut h, vec![order(1e306)]);
        refused(&h);
    }
}

#[test]
fn split_preflight_existing_valid_batches_place_every_element_once() {
    for uta in [false, true] {
        let mut h = harness(uta, 2.0, 3.0, 1.0, 5.0);
        place(&mut h, vec![order(6.0)]);
        let rows = settle(&mut h, uta);
        assert_eq!(rows.len(), 2);
        for row in rows {
            assert_eq!(row[if uta { "qty" } else { "size" }], "3");
        }
        let mut h = harness(uta, 2.0, 3.0, 1.0, 5.0);
        place(&mut h, vec![order(2.0); 51]);
        assert_eq!(settle(&mut h, uta).len(), 51);
    }
}

#[test]
fn split_preflight_valid_split_amend_cancels_once_then_places_each_lot_once() {
    let mut h = harness(false, 2.0, 3.0, 1.0, 5.0);
    h.command(Command::AmendOrder {
        command: 1,
        id: "1".into(),
        price: Some(100.0),
        amount: Some(6.0),
    });
    assert_eq!(h.requests.len(), 1);
    let cancel = h.requests.remove(0);
    assert!(
        cancel.url.ends_with("/batch-cancel-orders"),
        "oversized native amend: {}",
        cancel.url
    );
    h.reply(
        cancel.id,
        200,
        r#"{"code":"00000","data":{"successList":[{"orderId":"1"}],"failureList":[]}}"#,
    );
    let rows = settle(&mut h, false);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row["size"] == "3"));
}
