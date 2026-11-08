//! Continue/stop decisions belong to the venue's legacy TypeScript policy.
mod support;
use safe_cex_engine::model::{Order, OrderSide, OrderStatus, OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, Method};
use safe_cex_venue_binance::Variant;
use support::{Harness, Options};

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
fn split_lots_continue_after_a_definite_batch_item_refusal() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        place(&mut h, vec![order(variant, 26.0)]);
        refuse_first_batch_row(&mut h, variant);
        let next = h.take(&format!("{}/order", root(variant)));
        assert_eq!(next.method, Method::Post);
        assert!(next.url.contains("quantity=1&"));
        h.reply(next.id, 200, "", r#"{"orderId":6}"#);
        assert!(h.requests.is_empty());
        assert_accepted_ids_survive_the_failure(&h);
    }
}

#[test]
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
fn a_refused_stop_replacement_ends_without_a_ts_absent_put_back() {
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
    assert!(
        h.requests
            .iter()
            .all(|request| request.method == Method::Get)
    );
    assert!(h.result(91).unwrap().is_err());
}

#[test]
fn a_native_item_timeout_does_not_suppress_the_next_batch_chunk() {
    for variant in [Variant::UsdM, Variant::CoinM] {
        let mut h = ready(variant);
        place(&mut h, (0..6).map(|_| order(variant, 3.0)).collect());
        let first = h.take(&format!("{}/batchOrders", root(variant)));
        h.reply(first.id, 200, "", r#"[{"code":-1007,"msg":"Timeout waiting for response"},{"orderId":2},{"orderId":3},{"orderId":4},{"orderId":5}]"#);
        let next = h.take(&format!("{}/order", root(variant)));
        h.reply(next.id, 200, "", r#"{"orderId":6}"#);
        assert!(h.requests.is_empty());
        assert_accepted_ids_survive_the_failure(&h);
        assert_eq!(
            h.failures.last().unwrap().1.outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
    }
}

#[test]
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
