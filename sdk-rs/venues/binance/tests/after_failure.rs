//! Continue/stop decisions belong to the venue's legacy TypeScript policy.
mod support;
use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

const ENGINE_UNKNOWN_TEXT: &str = "The outcome is unknown. Check Open Orders";

fn root(variant: Variant) -> &'static str {
    if variant == Variant::CoinM {
        "/dapi/v1"
    } else {
        "/fapi/v1"
    }
}
fn symbol(variant: Variant) -> &'static str {
    if variant == Variant::CoinM {
        "BTCUSD_PERP"
    } else {
        "BTCUSDT"
    }
}
fn ready(variant: Variant) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("fixture-key".into()),
        secret: Some("fixture-seed".into()),
        ..Options::default()
    });
    let info = serde_json::json!({"symbols":[{"symbol":symbol(variant),"status":"TRADING","baseAsset":"BTC","quoteAsset":if variant == Variant::CoinM {"USD"} else {"USDT"},"marginAsset":if variant == Variant::CoinM {"BTC"} else {"USDT"},"contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"5"},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":"1","maxQty":"5"}]}]});
    h.respond(
        &format!("{}/exchangeInfo", root(variant)),
        200,
        &info.to_string(),
    );
    for (path, body) in [
        ("ticker/24hr", support::DAILY),
        ("ticker/bookTicker", support::BOOK_TICKERS),
        ("premiumIndex", support::PREMIUM),
    ] {
        h.respond(
            &format!("{}/{path}", root(variant)),
            200,
            &body.replace("BTCUSDT", symbol(variant)),
        );
    }
    assert!(h.engine.store().market(symbol(variant)).is_some());
    h.requests.clear();
    h
}
fn order(variant: Variant, amount: f64) -> PlaceOrderOpts {
    serde_json::from_value(serde_json::json!({"symbol":symbol(variant),"type":"limit","side":"buy","amount":amount,"price":100})).unwrap()
}
fn place(h: &mut Harness, orders: Vec<PlaceOrderOpts>) {
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders,
    }));
    assert!(!h.requests.is_empty(), "{:?}", h.done);
}
fn refuse_first_batch_row(h: &mut Harness, variant: Variant) {
    let batch = h.take(&format!("{}/batchOrders", root(variant)));
    h.reply(batch.id, 200, "", r#"[{"code":-2019,"msg":"Margin is insufficient."},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#);
}

fn assert_accepted_ids_survive_the_failure(h: &Harness) {
    let failure = &h.failures.iter().find(|(id, _)| *id == 77).unwrap().1;
    assert_eq!(
        failure.partial.as_ref().unwrap().order_ids,
        ["2", "3", "4", "5", "6"]
    );
    assert_ne!(failure.outcome, Some(safe_cex_engine::Outcome::NotSent));
    assert_ne!(failure.outcome, Some(safe_cex_engine::Outcome::Rejected));
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | max-qty-splitting/audit trading.rs:2831
// TS: packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1830-1844,1997-2011;
// binanceinverse.exchange.ts:915-924,937-941. Six 5-unit lots, then continue after a definite refusal.
fn split_lots_continue_after_a_definite_batch_item_refusal() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        place(&mut h, vec![order(variant, 30.0)]);
        let mut sizes = support::quantities(&h.requests[0]);
        refuse_first_batch_row(&mut h, variant);
        let next = h.take(&format!("{}/order", root(variant)));
        assert_eq!(next.method, Method::Post);
        assert!(next.url.contains("quantity=5&"));
        sizes.extend(support::quantities(&next));
        assert_eq!(sizes, [5.0; 6]);
        assert_eq!(sizes.iter().sum::<f64>(), 30.0);
        h.reply(next.id, 200, "", r#"{"orderId":6}"#);
        assert!(h.requests.is_empty());
        assert_accepted_ids_survive_the_failure(&h);
    }
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | batch-chunking/audit trading.rs:1468
// TS: packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1970-2021;
// binanceinverse.exchange.ts:928-943. Send five rows, then the sixth after definite row refusal.
fn later_batch_chunks_continue_after_a_definite_item_refusal() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        place(&mut h, (0..6).map(|_| order(variant, 3.0)).collect());
        refuse_first_batch_row(&mut h, variant);
        let next = h.take(&format!("{}/order", root(variant)));
        assert!(next.url.contains("quantity=3&"));
        h.reply(next.id, 200, "", r#"{"orderId":6}"#);
        assert!(h.requests.is_empty());
        assert_accepted_ids_survive_the_failure(&h);
    }
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | separate-TP-SL-children/audit trading.rs:2452
// TS: packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1685-1714,2025-2040;
// binanceinverse.exchange.ts:937-941. Definite parent refusal continues; child refusal throws.
fn children_follow_a_parent_batch_item_refusal_but_stop_after_a_child_refusal() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        let mut parent = order(variant, 10.0);
        parent.stop_loss = Some(90.0);
        parent.take_profit = Some(110.0);
        place(&mut h, vec![parent]);
        let batch = h.take(&format!("{}/batchOrders", root(variant)));
        h.reply(
            batch.id,
            200,
            "",
            r#"[{"code":-2019,"msg":"Margin is insufficient."},{"orderId":2}]"#,
        );
        let child = h.take(&format!("{}/algoOrder", root(variant)));
        h.reply(
            child.id,
            400,
            "",
            r#"{"code":-2021,"msg":"Order would immediately trigger."}"#,
        );
        assert!(
            h.requests
                .iter()
                .all(|request| request.method == Method::Get)
        );
        assert!(h.result(77).unwrap().is_err());
    }
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | cancel-replace/audit trading.rs:2093
// TS: packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1597-1618.
// TS cancels then places; it has no put-back. The putback-3356 brief explicitly
// preserves master's Rust protection policy (6890877e6 trading.rs:1388-1438).
fn a_refused_stop_replacement_preserves_masters_protective_put_back() {
    let mut h = ready(Variant::UsdM);
    h.engine.store_mut().upsert_order(Order {
        id: "12".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::StopLoss,
        side: OrderSide::Sell,
        price: 90.0,
        amount: 3.0,
        filled: 0.0,
        remaining: 3.0,
        reduce_only: true,
        close: None,
        client_order_id: None,
        time_in_force: None,
        last_update: 1.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    });
    h.event(Event::Command(Command::AmendOrder {
        command: 91,
        id: "12".into(),
        price: Some(91.0),
        amount: None,
    }));
    let cancel = h.take("/fapi/v1/algoOrder");
    assert_eq!(cancel.method, Method::Delete);
    h.reply(
        cancel.id,
        200,
        "",
        r#"{"algoId":12,"code":"200","msg":"success"}"#,
    );
    let replacement = h.take("/fapi/v1/algoOrder");
    assert_eq!(replacement.method, Method::Post);
    h.reply(
        replacement.id,
        400,
        "",
        r#"{"code":-2021,"msg":"Order would immediately trigger."}"#,
    );
    let put_back = h.take("/fapi/v1/algoOrder");
    assert_eq!(put_back.method, Method::Post);
    assert!(put_back.url.contains("triggerPrice=90&"));
    assert!(put_back.url.contains("quantity=3&"));
    h.reply(put_back.id, 200, "", r#"{"algoId":13}"#);
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == Method::Get)
    );
    assert!(h.result(91).unwrap().is_err());
    assert_eq!(
        h.failures
            .iter()
            .find(|(id, _)| *id == 91)
            .unwrap()
            .1
            .protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1997-2011
// and binanceinverse.exchange.ts:937-941.
fn a_native_item_timeout_stops_the_next_batch_chunk() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        place(&mut h, (0..6).map(|_| order(variant, 3.0)).collect());
        let first = h.take(&format!("{}/batchOrders", root(variant)));
        h.reply(first.id, 200, "", r#"[{"code":-1007,"msg":"Timeout waiting for response"},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#);
        assert_unknown_stop(&h);
    }
}

fn assert_unknown_stop(h: &Harness) {
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == Method::Get),
        "{:?}",
        h.paths()
    );
    let failures: Vec<_> = h.failures.iter().filter(|(id, _)| *id == 77).collect();
    assert_eq!(failures.len(), 1);
    let error = &failures[0].1;
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(
        error.partial.as_ref().unwrap().order_ids,
        ["2", "3", "4", "5"]
    );
    assert_eq!(error.venue_code.as_deref(), Some("-1007"));
    let rows = error
        .elements
        .as_ref()
        .expect("logical outcomes reach the caller");
    assert!(
        matches!(&rows[0].result, safe_cex_engine::write_plan::ElementResult::Unknown { error } if error.venue_code.as_deref() == Some("-1007"))
    );
    for (index, row) in rows.iter().enumerate().take(5).skip(1) {
        assert!(
            matches!(&row.result, safe_cex_engine::write_plan::ElementResult::Placed { order_ids } if order_ids == &[format!("{}", index + 1)])
        );
    }
    assert!(rows.iter().skip(5).all(|row| matches!(
        &row.result,
        safe_cex_engine::write_plan::ElementResult::NotSent { error: None }
    )));
    assert!(
        error.message.contains(ENGINE_UNKNOWN_TEXT),
        "{}",
        error.message
    );
    assert!(
        h.command_errors
            .iter()
            .any(|(id, message)| *id == 77 && message.contains(ENGINE_UNKNOWN_TEXT))
    );
    assert_eq!(
        h.command_notices
            .iter()
            .filter(|(id, event)| *id == 77 && *event == "failed")
            .count(),
        1
    );
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1997-2011
// and binanceinverse.exchange.ts:937-941.
fn a_native_item_timeout_stops_later_split_lots() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        place(&mut h, vec![order(variant, 30.0)]);
        let first = h.take(&format!("{}/batchOrders", root(variant)));
        h.reply(first.id, 200, "", r#"[{"code":-1007,"msg":"Timeout waiting for response"},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#);
        assert_unknown_stop(&h);
    }
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1685-1714
// and binanceinverse.exchange.ts:937-941.
fn a_native_item_timeout_stops_attached_children() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        let mut parent = order(variant, 25.0);
        parent.stop_loss = Some(90.0);
        parent.take_profit = Some(110.0);
        place(&mut h, vec![parent]);
        let first = h.take(&format!("{}/batchOrders", root(variant)));
        h.reply(first.id, 200, "", r#"[{"code":-1007,"msg":"Timeout waiting for response"},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#);
        assert_unknown_stop(&h);
    }
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | batch-chunking/audit trading.rs:1468
// TS: packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:2012-2016;
// binanceinverse.exchange.ts:930-942. Thrown HTTP failure skips later chunks.
fn an_http_batch_failure_stops_before_the_next_chunk() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        for status in [400, 429, 503] {
            let mut h = ready(variant);
            place(&mut h, (0..6).map(|_| order(variant, 3.0)).collect());
            let first = h.take(&format!("{}/batchOrders", root(variant)));
            h.reply(
                first.id,
                status,
                "",
                r#"{"code":-2019,"msg":"Batch request failed"}"#,
            );
            assert!(
                h.requests
                    .iter()
                    .all(|request| request.method == Method::Get)
            );
            assert!(h.result(77).unwrap().is_err());
        }
    }
}

#[test]
// KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: binance | batch-chunking/audit trading.rs:1468
// TS: packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1997-2011;
// binanceinverse.exchange.ts:937-941. Returned signature errors are logged, never resent.
fn returned_signature_error_rows_continue_without_resending_the_batch() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        place(&mut h, (0..6).map(|_| order(variant, 3.0)).collect());
        let first = h.take(&format!("{}/batchOrders", root(variant)));
        let rows = vec![serde_json::json!({"code":-1022,"msg":"Signature is not valid"}); 5];
        h.reply(first.id, 200, "", &serde_json::to_string(&rows).unwrap());
        let next = h.take(&format!("{}/order", root(variant)));
        h.reply(next.id, 200, "", r#"{"orderId":6}"#);
        assert!(h.requests.is_empty());
        let error = &h.failures.iter().find(|(id, _)| *id == 77).unwrap().1;
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["6"]);
    }
}

#[test]
// PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at
// packages/safe-cex/src/__tests__/reference/binance/binance.exchange.ts:1997-2011
// and binanceinverse.exchange.ts:937-941. Engine-owned HTTP sealing keeps the latest journal.
fn a_later_gateway_failure_keeps_accepted_refused_unknown_and_unissued_rows() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        place(&mut h, (0..16).map(|_| order(variant, 3.0)).collect());
        let first = h.take(&format!("{}/batchOrders", root(variant)));
        h.reply(
            first.id,
            200,
            "",
            r#"[{"orderId":1},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#,
        );
        let second = h.take(&format!("{}/batchOrders", root(variant)));
        h.reply(second.id, 200, "", r#"[{"code":-2019,"msg":"Margin is insufficient."},{"orderId":7},{"orderId":8},{"orderId":9},{"orderId":10}]"#);
        let third = h.take(&format!("{}/batchOrders", root(variant)));
        h.reply(third.id, 503, "", r#"{"msg":"upstream unavailable"}"#);
        assert!(
            h.requests
                .iter()
                .all(|request| request.method == Method::Get)
        );
        let error = &h.failures.iter().find(|(id, _)| *id == 77).unwrap().1;
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert_eq!(error.venue_code.as_deref(), Some("-2019"));
        assert_eq!(
            error.partial.as_ref().unwrap().order_ids,
            ["7", "8", "9", "10", "1", "2", "3", "4", "5"]
        );
        assert!(error.message.contains("outcome of this order change is unknown: a later request was not confirmed. Check Open Orders"));
        let rows = error.elements.as_ref().unwrap();
        assert_eq!(rows.len(), 16);
        assert!(
            matches!(&rows[5].result, safe_cex_engine::write_plan::ElementResult::NotSent { error: Some(error) } if error.venue_code.as_deref() == Some("-2019"))
        );
        assert!(rows[10..15].iter().all(|row| matches!(&row.result, safe_cex_engine::write_plan::ElementResult::Unknown { error } if error.outcome == Some(safe_cex_engine::Outcome::Unknown))));
        assert!(matches!(
            &rows[15].result,
            safe_cex_engine::write_plan::ElementResult::NotSent { error: None }
        ));
        assert_eq!(
            h.command_notices
                .iter()
                .filter(|(id, event)| *id == 77 && *event == "failed")
                .count(),
            1
        );
    }
}
