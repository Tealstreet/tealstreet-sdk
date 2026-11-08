use super::*;
use safe_cex_engine::write_plan::ElementResult;

#[derive(Clone, Copy)]
enum Path {
    Lots,
    MissingReference,
    Batch,
    Children,
    CombinedCount,
    Normalized,
}

fn placement(path: Path) -> Harness {
    let mut h = Harness::new(None);
    let rows = match path {
        Path::Lots => vec![PlaceOrderOpts {
            amount: 21.,
            price: Some(2.),
            ..opts()
        }],
        Path::MissingReference => {
            h.quote(Some(0.), None);
            vec![
                opts(),
                PlaceOrderOpts {
                    amount: 11.,
                    kind: OrderType::Market,
                    ..opts()
                },
            ]
        }
        Path::Batch => [91., 92., 93.]
            .into_iter()
            .map(|price| PlaceOrderOpts {
                price: Some(price),
                ..opts()
            })
            .collect(),
        Path::Children => vec![PlaceOrderOpts {
            stop_loss: Some(0.1),
            take_profit: Some(110.),
            ..opts()
        }],
        Path::CombinedCount => [50_000., 50_010.]
            .into_iter()
            .map(|amount| PlaceOrderOpts { amount, ..opts() })
            .collect(),
        Path::Normalized => vec![PlaceOrderOpts {
            amount: 21.0004,
            price: Some(2.),
            ..opts()
        }],
    };
    h.place(rows);
    accept_parity_payload(&mut h, "prefix");
    h
}

fn wire(h: &Harness) -> Vec<(String, f64, Option<f64>)> {
    h.sent
        .iter()
        .map(|(method, url, body)| {
            assert_eq!(*method, safe_cex_engine::Method::Post);
            assert!(url.ends_with("/trade/order"));
            let body: serde_json::Value = serde_json::from_str(body.as_deref().unwrap()).unwrap();
            (
                body["type"].as_str().unwrap().into(),
                body["quantity"].as_f64().unwrap(),
                body["price"]
                    .as_f64()
                    .or_else(|| body["stopPrice"].as_f64()),
            )
        })
        .collect()
}

fn refused(path: Path) {
    let mut h = placement(path);
    h.reply(r#"{"code":100400,"msg":"scripted definite refusal"}"#);
    assert_eq!(h.requests.len(), 1, "TS continues the untouched suffix");
    accept_parity_payload(&mut h, "later");
    let has_tail = matches!(path, Path::Lots | Path::Normalized | Path::MissingReference);
    if has_tail {
        accept_parity_payload(&mut h, "tail");
    }
    let expected = match path {
        Path::Lots | Path::Normalized => vec![
            ("LIMIT", 6.667, Some(2.)),
            ("LIMIT", 6.667, Some(2.)),
            ("LIMIT", 6.667, Some(2.)),
            ("LIMIT", 1., Some(2.)),
        ],
        Path::MissingReference => vec![
            ("LIMIT", 1., Some(95.)),
            ("MARKET", 5., None),
            ("MARKET", 5., None),
            ("MARKET", 1., None),
        ],
        Path::Batch => vec![
            ("LIMIT", 1., Some(91.)),
            ("LIMIT", 1., Some(92.)),
            ("LIMIT", 1., Some(93.)),
        ],
        Path::Children => vec![
            ("LIMIT", 1., Some(95.)),
            ("TRIGGER_MARKET", 1., Some(0.1)),
            ("TRIGGER_MARKET", 1., Some(110.)),
        ],
        Path::CombinedCount => {
            assert_eq!(h.requests.len(), 1);
            h.failed();
            vec![("LIMIT", 10., Some(95.)); 4]
        }
    };
    assert_eq!(
        wire(&h),
        expected
            .into_iter()
            .map(|(kind, qty, price)| (kind.into(), qty, price))
            .collect::<Vec<_>>()
    );
    assert!(h.requests.is_empty());
    let rows = h.failures[0].elements.as_ref().expect("complete report");
    assert_eq!(
        rows.len(),
        if matches!(path, Path::CombinedCount) {
            10_001
        } else if has_tail {
            4
        } else {
            3
        }
    );
    assert!(
        matches!(&rows[0].result, ElementResult::Placed { order_ids } if order_ids == &["prefix"])
    );
    assert!(
        matches!(&rows[1].result, ElementResult::NotSent { error: Some(e) } if e.outcome == Some(safe_cex_engine::Outcome::Rejected))
    );
    assert!(
        matches!(&rows[2].result, ElementResult::Placed { order_ids } if order_ids == &["later"])
    );
    if has_tail {
        assert!(
            matches!(&rows[3].result, ElementResult::Placed { order_ids } if order_ids == &["tail"])
        );
    }
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1544-1564,1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | perp-max-qty-splitting trading.rs:1425
#[test]
fn lots_continue_after_definite_refusal() {
    refused(Path::Lots);
}
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1485-1542,1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | missing/nonpositive notional reference trading.rs:1387
#[test]
fn missing_reference_continues_after_definite_refusal() {
    refused(Path::MissingReference);
}
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1447-1452,1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | perp-serial-batch trading.rs:1233
#[test]
fn batch_continues_after_definite_refusal() {
    refused(Path::Batch);
}
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1567-1595,1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | attached-TP-SL children trading.rs:1450
#[test]
fn children_continue_after_definite_refusal() {
    refused(Path::Children);
}
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1451-1452,1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | combined-native-write-count trading.rs:1233
#[test]
fn combined_count_continues_after_definite_refusal() {
    refused(Path::CombinedCount);
}
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1544-1564,1854-1889
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | exact-quantity-conservation and explicit-dust-close minimum policy trading.rs:1387/1425
#[test]
fn normalized_lots_continue_after_definite_refusal() {
    refused(Path::Normalized);
}

fn amend(amount: bool, unknown: bool) {
    let mut h = Harness::new(Some((order(), "LIMIT")));
    h.command(Command::AmendOrder {
        command: 1,
        id: order().id,
        amount: amount.then_some(0.01),
        price: (!amount).then_some(1.04),
    });
    assert_eq!(h.sent.len(), 1);
    assert!(h.sent[0].1.ends_with(if amount {
        "/trade/amend"
    } else {
        "/trade/cancelReplace"
    }));
    if unknown {
        h.failed();
    } else {
        h.reply(r#"{"code":100400,"msg":"definite refusal"}"#);
    }
    assert!(h.requests.is_empty());
    assert_eq!(h.sent.len(), 1, "single native call never repeats");
    if unknown {
        assert_unknown(&h, 0, if amount { 1 } else { 2 });
    }
}

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1136-1157
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | ordinary-quantity-amend trading.rs:1540
#[test]
fn quantity_amend_stops_after_refusal() {
    amend(true, false);
}
// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1398-1440
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: bingx | ordinary-atomic-cancel-replace trading.rs:1540
#[test]
fn atomic_replace_stops_after_refusal() {
    amend(false, false);
}

fn assert_unknown(h: &Harness, current: usize, count: usize) {
    assert!(h.requests.is_empty());
    let e = &h.failures[0];
    assert_eq!(e.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert!(e.message.contains(ENGINE_UNKNOWN_TEXT), "{}", e.message);
    let rows = e.elements.as_ref().expect("unknown journal");
    for row in &rows[current..current + count] {
        assert!(matches!(&row.result, ElementResult::Unknown { error }
            if error.outcome == Some(safe_cex_engine::Outcome::Unknown)));
    }
    for row in &rows[current + count..] {
        assert!(matches!(
            &row.result,
            ElementResult::NotSent { error: None }
        ));
    }
    assert_eq!(
        h.failures.len(),
        1,
        "one structured CommandFailed telemetry event"
    );
    assert_eq!(h.done.len(), 1);
    assert!(
        h.done[0].is_err(),
        "terminal event carries the complete journal"
    );
}

fn unknown(path: Path) {
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
    // packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1871-1889.
    for gateway in [false, true] {
        let mut h = placement(path);
        if gateway {
            h.reply_status(503, r#"{"msg":"unavailable"}"#);
        } else {
            h.failed();
        }
        assert_eq!(h.sent.len(), 2);
        assert_unknown(&h, 1, 1);
        let rows = h.failures[0].elements.as_ref().unwrap();
        assert!(
            matches!(&rows[0].result, ElementResult::Placed { order_ids } if order_ids == &["prefix"])
        );
    }
}

#[test]
fn lots_unknown_stops() {
    unknown(Path::Lots);
}
#[test]
fn missing_reference_unknown_stops() {
    unknown(Path::MissingReference);
}
#[test]
fn batch_unknown_stops() {
    unknown(Path::Batch);
}
#[test]
fn children_unknown_stops() {
    unknown(Path::Children);
}
#[test]
fn combined_count_unknown_stops() {
    unknown(Path::CombinedCount);
}
#[test]
fn normalized_unknown_stops() {
    unknown(Path::Normalized);
}
#[test]
fn quantity_amend_unknown_stops() {
    amend(true, true);
}
#[test]
fn atomic_replace_unknown_stops() {
    amend(false, true);
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:940-962.
#[test]
fn independent_cancel_unknown_stops() {
    let mut second = order();
    second.id = "second".into();
    let mut h = Harness::new(None);
    h.engine.store_mut().set_orders(vec![order(), second]);
    h.command(Command::CancelOrders {
        command: 1,
        ids: vec![order().id, "second".into()],
    });
    h.failed();
    assert_eq!(h.sent.len(), 1);
    assert_unknown(&h, 0, 1);
}

// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1871-1889.
#[test]
fn definite_refusal_then_unknown_keeps_every_fact() {
    for gateway in [false, true] {
        let mut h = Harness::new(None);
        h.place(vec![PlaceOrderOpts {
            amount: 31.,
            price: Some(2.),
            ..opts()
        }]);
        accept_parity_payload(&mut h, "prefix");
        h.reply(r#"{"code":100400,"msg":"definite refusal"}"#);
        assert_eq!(h.requests.len(), 1);
        if gateway {
            h.reply_status(503, r#"{"msg":"unavailable"}"#);
        } else {
            h.failed();
        }
        assert_eq!(h.sent.len(), 3);
        assert_unknown(&h, 2, 1);
        let rows = h.failures[0].elements.as_ref().unwrap();
        assert_eq!(rows.len(), 5);
        assert!(
            matches!(&rows[0].result, ElementResult::Placed { order_ids } if order_ids == &["prefix"])
        );
        assert!(
            matches!(&rows[1].result, ElementResult::NotSent { error: Some(error) } if error.venue_code.as_deref() == Some("100400"))
        );
    }
}
