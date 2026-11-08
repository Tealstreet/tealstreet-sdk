//! Whole-command preflight uses the native quantities and prices sent to Binance.
mod support;
use safe_cex_engine::model::{OrderType, PlaceOrderOpts};
use safe_cex_engine::{Command, Event, Method, Outcome};
use safe_cex_venue_binance::Variant;
use serde_json::{Value, json};
use support::{Harness, Options};

fn ready(variant: Variant, minimum: &str, maximum: &str, notional: &str) -> Harness {
    ready_with(variant, minimum, maximum, notional, |_| {})
}
fn ready_with(
    variant: Variant,
    minimum: &str,
    maximum: &str,
    notional: &str,
    change: impl FnOnce(&mut Value),
) -> Harness {
    let mut h = Harness::started(Options {
        variant,
        key: Some("fixture-key".into()),
        secret: Some("fixture-seed".into()),
        ..Options::default()
    });
    let prefix = match variant {
        Variant::Spot => "/api/v3",
        Variant::CoinM => "/dapi/v1",
        _ => "/fapi/v1",
    };
    let mut info = json!({"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","contractSize":100,"filters":[{"filterType":"PRICE_FILTER","tickSize":"0.1"},{"filterType":"LOT_SIZE","stepSize":"1","minQty":minimum,"maxQty":maximum},{"filterType":"MARKET_LOT_SIZE","stepSize":"1","minQty":minimum,"maxQty":maximum},{"filterType":"MIN_NOTIONAL","notional":notional,"applyToMarket":true}] }]} );
    if variant == Variant::CoinM {
        info["symbols"][0].as_object_mut().unwrap().remove("status");
        info["symbols"][0]["contractStatus"] = json!("TRADING");
    }
    change(&mut info);
    h.respond(&format!("{prefix}/exchangeInfo"), 200, &info.to_string());
    h.respond(&format!("{prefix}/ticker/24hr"), 200, support::DAILY);
    if variant == Variant::Spot {
        h.engine
            .store_mut()
            .update_ticker("BTCUSDT", |t| t.last = 100.0);
    } else {
        h.respond(
            &format!("{prefix}/ticker/bookTicker"),
            200,
            support::BOOK_TICKERS,
        );
        h.respond(&format!("{prefix}/premiumIndex"), 200, support::PREMIUM);
    }
    h.requests.clear();
    h
}
fn order(amount: f64, price: f64) -> PlaceOrderOpts {
    serde_json::from_value(
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":price}),
    )
    .unwrap()
}
fn place(h: &mut Harness, orders: Vec<PlaceOrderOpts>) {
    h.event(Event::Command(Command::PlaceOrders {
        command: 77,
        orders,
    }));
}
fn zero_writes(h: &mut Harness) {
    assert_eq!(
        h.requests.len(),
        0,
        "no transport before a whole-plan refusal: {:?}",
        h.paths()
    );
    let (_, error) = h
        .failures
        .iter()
        .find(|(id, _)| *id == 77)
        .expect("typed command refusal");
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert_eq!(
        h.progress.len(),
        0,
        "unreleased lots have no progress receipt"
    );
}
fn param(url: &str, name: &str) -> Option<String> {
    let value = url.split_once('?')?.1.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then_some(value)
    })?;
    let bytes = value.as_bytes();
    let mut decoded = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            decoded.push(u8::from_str_radix(&value[index + 1..index + 3], 16).unwrap());
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}
fn rows(url: &str) -> Vec<Value> {
    match param(url, "batchOrders") {
        Some(batch) => serde_json::from_str(&batch).unwrap(),
        None => vec![
            json!({"quantity":param(url,"quantity"),"type":param(url,"type"),"price":param(url,"price")}),
        ],
    }
}
fn settle(h: &mut Harness) -> (usize, Vec<Value>) {
    let mut calls = 0;
    let mut sent = Vec::new();
    while let Some(index) = h.requests.iter().position(|r| r.method == Method::Post) {
        let request = h.requests.remove(index);
        let batch = rows(&request.url);
        calls += 1;
        let replies: Vec<_> = batch
            .iter()
            .enumerate()
            .map(|(index, _)| json!({"orderId":calls*100+index+1,"algoId":calls*100+index+1}))
            .collect();
        let reply = if batch.len() == 1 {
            replies[0].to_string()
        } else {
            serde_json::to_string(&replies).unwrap()
        };
        sent.extend(batch);
        h.reply(request.id, 200, "", &reply);
    }
    (calls, sent)
}

#[test]
fn impossible_split_tail_refuses_every_variant_before_the_first_write() {
    for variant in [
        Variant::Spot,
        Variant::CoinM,
        Variant::UsdM,
        Variant::PortfolioMargin,
    ] {
        let mut h = ready(variant, "4", "5", "0");
        place(&mut h, vec![order(4.0, 100.0), order(6.0, 100.0)]);
        zero_writes(&mut h);
    }
}
#[test]
fn audit_tail_is_rebalanced_without_changing_the_total() {
    for variant in [Variant::Spot, Variant::CoinM] {
        let mut h = ready(variant, "3", "5", "0");
        place(&mut h, vec![order(7.0, 100.0)]);
        let (_, sent) = settle(&mut h);
        let sizes: Vec<f64> = sent
            .iter()
            .map(|r| r["quantity"].as_str().unwrap().parse().unwrap())
            .collect();
        assert_eq!(sizes, vec![4.0, 3.0]);
        assert!(h.result(77).unwrap().is_ok());
    }
}
#[test]
fn invalid_last_row_after_a_batch_boundary_refuses_all_requests() {
    for variant in [
        Variant::Spot,
        Variant::CoinM,
        Variant::UsdM,
        Variant::PortfolioMargin,
    ] {
        let mut h = ready(variant, "4", "5", "0");
        let mut orders = vec![order(4.0, 100.0); 5];
        orders.push(order(1.0, 100.0));
        place(&mut h, orders);
        zero_writes(&mut h);
    }
}
#[test]
fn split_notional_is_checked_per_lot_at_the_rounded_limit_price() {
    for variant in [Variant::Spot, Variant::UsdM, Variant::PortfolioMargin] {
        let mut h = ready(variant, "1", "5", "4.1");
        place(&mut h, vec![order(8.0, 1.04)]);
        zero_writes(&mut h);
    }
}
#[test]
fn invalid_late_spot_bracket_discards_the_parent_and_every_split() {
    let mut h = ready(Variant::Spot, "1", "5", "4.1");
    let mut input = order(8.0, 100.0);
    input.stop_loss = Some(1.04);
    place(&mut h, vec![input]);
    zero_writes(&mut h);
}
#[test]
fn usd_m_and_pm_attached_algos_are_all_sized_before_entry_dispatch() {
    for variant in [Variant::UsdM, Variant::PortfolioMargin] {
        let mut h = ready(variant, "1", "5", "0");
        let mut input = order(11.0, 100.0);
        input.stop_loss = Some(90.0);
        input.take_profit = Some(110.0);
        place(&mut h, vec![input]);
        let (calls, sent) = settle(&mut h);
        assert_eq!(
            calls,
            if variant == Variant::PortfolioMargin {
                9
            } else {
                7
            },
            "entry lots plus six algo writes"
        );
        for kind in ["LIMIT", "STOP_MARKET", "TAKE_PROFIT_MARKET"] {
            let sizes: Vec<f64> = sent
                .iter()
                .filter(|r| r["type"] == kind)
                .map(|r| r["quantity"].as_str().unwrap().parse().unwrap())
                .collect();
            assert_eq!(sizes, vec![5.0, 5.0, 1.0], "{kind}");
        }
        assert!(h.result(77).unwrap().is_ok());
    }
}
#[test]
fn off_step_maximum_never_rounds_a_submitted_lot_above_the_native_cap() {
    let mut h = ready(Variant::Spot, "1", "4.6", "0");
    place(&mut h, vec![order(9.0, 100.0)]);
    let (_, sent) = settle(&mut h);
    let sizes: Vec<f64> = sent
        .iter()
        .map(|r| r["quantity"].as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(sizes, vec![4.0, 4.0, 1.0]);
}
#[test]
fn exact_minimum_and_maximum_batches_place_every_row_once() {
    for variant in [
        Variant::Spot,
        Variant::CoinM,
        Variant::UsdM,
        Variant::PortfolioMargin,
    ] {
        let mut h = ready(variant, "1", "5", "0");
        place(&mut h, vec![order(5.0, 100.0); 6]);
        let (calls, sent) = settle(&mut h);
        assert_eq!(sent.len(), 6);
        assert_eq!(
            calls,
            if matches!(variant, Variant::Spot | Variant::PortfolioMargin) {
                6
            } else {
                2
            }
        );
        assert!(sent.iter().all(|r| r["quantity"] == "5"));
        assert!(h.result(77).unwrap().is_ok());
    }
}
#[test]
fn malformed_final_bracket_is_a_zero_write_refusal() {
    let mut h = ready(Variant::UsdM, "1", "5", "0");
    let mut input = order(2.0, 100.0);
    input.take_profit = Some(0.01);
    place(&mut h, vec![order(2.0, 100.0), input]);
    zero_writes(&mut h);
}
#[test]
fn min_notional_uses_mark_for_unpriced_derivative_orders() {
    let mut h = ready(Variant::UsdM, "1", "5", "150");
    h.engine.store_mut().update_ticker("BTCUSDT", |t| {
        t.last = 200.0;
        t.mark = 100.0;
    });
    let mut input = order(1.0, 200.0);
    input.kind = OrderType::Market;
    place(&mut h, vec![order(2.0, 200.0), input]);
    zero_writes(&mut h);
}

fn seed_amend(h: &mut Harness, amount: f64, filled: f64) {
    use safe_cex_engine::model::{Order, OrderSide, OrderStatus, TimeInForce};
    h.engine.store_mut().upsert_order(Order {
        id: "12".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "BTCUSDT".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 100.0,
        amount,
        filled,
        remaining: amount - filled,
        reduce_only: false,
        close: None,
        client_order_id: Some("maker".into()),
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: h.now,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    });
}
#[test]
fn invalid_replacement_tail_never_cancels_the_existing_order() {
    let mut h = ready(Variant::UsdM, "4", "5", "0");
    seed_amend(&mut h, 4.0, 0.0);
    h.event(Event::Command(Command::AmendOrder {
        command: 77,
        id: "12".into(),
        price: Some(100.0),
        amount: Some(6.0),
    }));
    zero_writes(&mut h);
    assert_eq!(
        h.engine.store().order("12").unwrap().status,
        safe_cex_engine::model::OrderStatus::Open
    );
}
#[test]
fn replacement_notional_uses_rounded_wire_price_before_cancel() {
    let mut h = ready_with(Variant::UsdM, "1", "5", "5", |info| {
        info["symbols"][0]["filters"][1]["stepSize"] = json!("0.1");
        info["symbols"][0]["filters"][2]["stepSize"] = json!("0.1");
    });
    seed_amend(&mut h, 4.9, 0.0);
    h.event(Event::Command(Command::AmendOrder {
        command: 77,
        id: "12".into(),
        price: Some(1.04),
        amount: None,
    }));
    zero_writes(&mut h);
}

#[test]
fn market_lot_size_is_validated_at_its_own_native_step() {
    let mut h = ready_with(Variant::Spot, "1", "5", "0", |info| {
        info["symbols"][0]["filters"][2] =
            json!({"filterType":"MARKET_LOT_SIZE","minQty":"2","maxQty":"4","stepSize":"2"});
    });
    let mut input = order(7.0, 100.0);
    input.kind = OrderType::Market;
    place(&mut h, vec![input]);
    zero_writes(&mut h);
}
#[test]
fn spot_notional_filter_alias_is_applied_to_each_wire_lot() {
    let mut h = ready_with(Variant::Spot, "1", "5", "0", |info| {
        info["symbols"][0]["filters"][3] = json!({"filterType":"NOTIONAL","minNotional":"4.1","maxNotional":"10000","applyMinToMarket":true,"applyMaxToMarket":true});
    });
    place(&mut h, vec![order(8.0, 1.04)]);
    zero_writes(&mut h);
}
#[test]
fn disabled_spot_market_notional_keeps_valid_market_intents() {
    let mut h = ready_with(Variant::Spot, "1", "5", "10000", |info| {
        info["symbols"][0]["filters"][3]["applyToMarket"] = json!(false);
    });
    let mut input = order(1.0, 100.0);
    input.kind = OrderType::Market;
    place(&mut h, vec![input]);
    let (calls, sent) = settle(&mut h);
    assert_eq!((calls, sent.len()), (1, 1));
    assert!(h.result(77).unwrap().is_ok());
}
#[test]
fn malformed_native_minimum_cannot_be_coerced_into_a_disabled_filter() {
    let mut h = ready_with(Variant::Spot, "1", "5", "0", |info| {
        info["symbols"][0]["filters"][3]["notional"] = json!("NaN");
    });
    place(&mut h, vec![order(1.0, 100.0)]);
    zero_writes(&mut h);
}

fn runtime_failure_reports_every_leg(status: u16, body: &str, expected: &str) {
    let mut h = ready(Variant::Spot, "1", "5", "0");
    place(&mut h, vec![order(1.0, 100.0); 3]);
    let first = h.requests.remove(0);
    assert_eq!(first.method, Method::Post);
    h.reply(first.id, 200, "", r#"{"orderId":101}"#);
    let second = h.requests.remove(0);
    assert_eq!(second.method, Method::Post);
    h.reply(second.id, status, "", body);
    assert!(
        h.requests.is_empty(),
        "no suffix, resend, cancel or rollback: {:?}",
        h.paths()
    );
    let (_, error) = h.failures.iter().find(|(id, _)| *id == 77).unwrap();
    let error = serde_json::to_value(error).unwrap();
    let elements = error["elements"]
        .as_array()
        .expect("complete per-element outcomes");
    assert_eq!(elements.len(), 3);
    assert_eq!(elements[0]["status"], "placed");
    assert!(!elements[0]["orderIds"].as_array().unwrap().is_empty());
    assert_eq!(elements[1]["status"], expected);
    assert_eq!(elements[2]["status"], "notSent");
    assert_eq!(
        elements
            .iter()
            .map(|row| row["index"].as_u64().unwrap())
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn rejected_second_write_keeps_placed_prefix_and_unsent_suffix() {
    runtime_failure_reports_every_leg(
        400,
        r#"{"code":-1013,"msg":"scripted rejection"}"#,
        "notSent",
    );
}

#[test]
fn ambiguous_second_write_stays_unknown_without_repair() {
    runtime_failure_reports_every_leg(
        503,
        r#"{"code":-1000,"msg":"execution status unknown"}"#,
        "unknown",
    );
}

#[test]
fn exact_decimal_notional_boundary_remains_a_valid_control() {
    let mut h = ready(Variant::Spot, "1", "10", "2.1");
    place(&mut h, vec![order(7.0, 0.3)]);
    let (count, rows) = settle(&mut h);
    assert_eq!(count, 1);
    assert_eq!(rows[0]["quantity"], "7");
    assert_eq!(rows[0]["price"], "0.3");
}

#[test]
fn opening_conditionals_obey_native_min_notional_before_any_algo_write() {
    for (quantity, accepted) in [(0.012, false), (0.1, true)] {
        let mut h = ready_with(Variant::UsdM, "0.001", "100", "5", |info| {
            info["symbols"][0]["filters"][1]["stepSize"] = json!("0.001");
            info["symbols"][0]["filters"][2]["stepSize"] = json!("0.001");
        });
        let mut conditional = order(quantity, 100.0);
        conditional.kind = OrderType::Conditional;
        conditional.price = None;
        conditional.trigger_price = Some(110.0);
        place(&mut h, vec![conditional]);
        if accepted {
            let (count, rows) = settle(&mut h);
            assert_eq!(count, 1);
            assert_eq!(rows[0]["type"], "STOP_MARKET");
        } else {
            zero_writes(&mut h);
        }
    }
}

#[test]
fn native_batch_rows_keep_attributed_acceptances_and_stop_the_child_suffix() {
    let mut h = ready(Variant::UsdM, "1", "5", "0");
    let mut last = order(1.0, 100.0);
    last.stop_loss = Some(90.0);
    place(&mut h, vec![order(1.0, 100.0), order(1.0, 100.0), last]);
    let batch = h.requests.remove(0);
    assert!(batch.url.contains("batchOrders"));
    h.reply(
        batch.id,
        200,
        "",
        r#"[{"orderId":101},{"orderId":102},{"code":-1013,"msg":"row refused"}]"#,
    );
    assert!(
        h.requests.is_empty(),
        "no child, resend, cancel or rollback"
    );
    let error = &h.failures.iter().find(|(id, _)| *id == 77).unwrap().1;
    let rows = serde_json::to_value(error).unwrap()["elements"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        rows.iter()
            .map(|row| row["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["placed", "placed", "notSent", "notSent"]
    );
    assert_eq!(rows[0]["orderIds"], json!(["101"]));
    assert_eq!(rows[1]["orderIds"], json!(["102"]));
    assert_eq!(rows[2]["error"]["outcome"], "rejected");
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["101", "102"]);
}

#[test]
fn unattributable_short_batch_remains_unknown_and_keeps_accepted_ids() {
    let mut h = ready(Variant::UsdM, "1", "5", "0");
    let mut last = order(1.0, 100.0);
    last.stop_loss = Some(90.0);
    place(&mut h, vec![order(1.0, 100.0), order(1.0, 100.0), last]);
    let batch = h.requests.remove(0);
    h.reply(batch.id, 200, "", r#"[{"orderId":101}]"#);
    assert!(h.requests.is_empty(), "no suffix or repair");
    let error = &h.failures.iter().find(|(id, _)| *id == 77).unwrap().1;
    let rows = serde_json::to_value(error).unwrap()["elements"]
        .as_array()
        .unwrap()
        .clone();
    assert_eq!(
        rows.iter()
            .map(|row| row["status"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["unknown", "unknown", "unknown", "notSent"]
    );
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["101"]);
}

#[test]
fn malformed_ack_does_not_turn_a_planned_client_id_into_acceptance() {
    for variant in [Variant::Spot, Variant::PortfolioMargin] {
        for body in ["{}", r#"{"orderId":0}"#, "[101]", r#"{"algoId":101}"#] {
            let mut h = ready(variant, "1", "5", "0");
            place(&mut h, vec![order(1.0, 100.0); 3]);
            let first = h.requests.remove(0);
            h.reply(first.id, 200, "", body);
            assert!(
                h.requests.is_empty(),
                "malformed acceptance cannot dispatch a suffix"
            );
            let error = &h.failures.iter().find(|(id, _)| *id == 77).unwrap().1;
            assert_eq!(error.outcome, Some(Outcome::Unknown));
            let rows = serde_json::to_value(error).unwrap();
            assert_eq!(rows["elements"][0]["status"], "unknown");
            assert_eq!(rows["elements"][1]["status"], "notSent");
            assert_eq!(rows["elements"][2]["status"], "notSent");
        }
    }
}
