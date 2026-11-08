use super::*;
use safe_cex_engine::{ErrorKind, Outcome};

fn configured(minimum: f64, maximum: f64, step: f64, notional: f64, algos: u32) -> Harness {
    let info = json!({"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":1,"quantityPrecision":3,"filters":[
        {"filterType":"PRICE_FILTER","tickSize":"0.1"},
        {"filterType":"LOT_SIZE","stepSize":step.to_string(),"minQty":minimum.to_string(),"maxQty":maximum.to_string()},
        {"filterType":"MARKET_LOT_SIZE","stepSize":step.to_string(),"minQty":minimum.to_string(),"maxQty":maximum.to_string()},
        {"filterType":"MIN_NOTIONAL","notional":notional.to_string()},
        {"filterType":"MAX_NUM_ALGO_ORDERS","limit":algos}
    ]}]}).to_string();
    let mut h = Harness::new();
    h.load(&info);
    h.requests.clear();
    h.sent_requests = 0;
    h.sent_writes = 0;
    h.engine.store_mut().set_tickers(vec![Ticker {
        symbol: "BTCUSDT".into(),
        last: 1.0,
        mark: 1.0,
        ..Ticker::default()
    }]);
    h
}

fn ordinary(amount: f64, price: f64) -> serde_json::Value {
    json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":amount,"price":price})
}

fn batch(h: &mut Harness, orders: Vec<serde_json::Value>) {
    h.command(Command::PlaceOrders {
        command: 1,
        orders: orders
            .into_iter()
            .map(|o| serde_json::from_value(o).expect("fixture order"))
            .collect(),
    });
}

fn refused(h: &Harness) {
    assert_eq!(
        h.sent_requests, 0,
        "no transport, including the write clock"
    );
    assert_eq!(
        h.requests.len(),
        0,
        "invalid plan sent requests: {:?}",
        h.requests
    );
    assert_eq!(h.failures.len(), 1, "one terminal refusal: {:?}", h.done);
    let error = &h.failures[0].1;
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
    assert_eq!(error.outcome, Some(Outcome::NotSent));
    assert!(error.partial.is_none());
}

#[test]
fn invalid_second_limit_zero_writes() {
    let mut h = Harness::new();
    batch(&mut h, vec![limit(), ordinary(1.0, 0.0)]);
    refused(&h);
}

#[test]
fn invalid_attached_sl_zero_writes() {
    for trigger in [-1.0, 0.04] {
        let mut h = Harness::new();
        let mut bracket = limit();
        bracket["stopLoss"] = json!(trigger);
        batch(&mut h, vec![limit(), bracket]);
        refused(&h);
    }
}

#[test]
fn limit_split_tail_rebalances_to_native_minimum() {
    let mut h = configured(3.0, 5.0, 1.0, 0.0, 10);
    h.place(ordinary(7.0, 1.0));
    assert_eq!(quantities(&mut h), ["4", "3"]);
    assert!(h.result(1).is_ok());
}

#[test]
fn limit_split_tail_below_min_qty_zero_writes() {
    let mut h = configured(4.0, 5.0, 1.0, 0.0, 10);
    h.place(ordinary(6.0, 1.0));
    refused(&h);
}

#[test]
fn split_tail_below_min_notional_zero_writes() {
    let mut h = configured(1.0, 5.0, 1.0, 4.1, 10);
    h.place(ordinary(8.0, 1.04));
    refused(&h);
}

#[test]
fn borrowed_market_donor_below_min_zero_writes() {
    let mut h = configured(4.0, 5.0, 1.0, 0.0, 10);
    h.place(json!({"symbol":"BTCUSDT","type":"market","side":"buy","amount":6}));
    refused(&h);
}

#[test]
fn batch_cumulative_algo_count_zero_writes() {
    let mut h = configured(1.0, 5.0, 1.0, 0.0, 1);
    let mut bracket = ordinary(3.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    batch(&mut h, vec![bracket.clone(), bracket]);
    refused(&h);
}

#[test]
fn split_attached_children_count_toward_algo_limit_before_entry() {
    let mut h = configured(3.0, 5.0, 1.0, 0.0, 1);
    let mut bracket = ordinary(7.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    h.place(bracket);
    refused(&h);
}

#[test]
fn invalid_replacement_split_never_cancels() {
    let mut h = configured(4.0, 5.0, 1.0, 0.0, 10);
    h.engine
        .store_mut()
        .set_orders(vec![order("10", OrderType::Limit)]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(1.0),
        amount: Some(6.0),
    });
    refused(&h);
    assert_eq!(
        h.engine.store().order("10").expect("live order").status,
        OrderStatus::Open
    );
}

#[test]
fn invalid_replacement_lot_notional_never_cancels() {
    let mut h = configured(1.0, 5.0, 1.0, 4.1, 10);
    let mut live = order("10", OrderType::Limit);
    live.reduce_only = false;
    h.engine.store_mut().set_orders(vec![live]);
    h.command(Command::AmendOrder {
        command: 1,
        id: "10".into(),
        price: Some(1.04),
        amount: Some(8.0),
    });
    refused(&h);
}

#[test]
fn off_step_maximum_is_floored_for_every_wire_lot() {
    let mut h = configured(0.1, 0.95, 0.1, 0.0, 10);
    h.place(ordinary(1.8, 1.0));
    assert_eq!(quantities(&mut h), ["0.9", "0.9"]);
    assert!(h.result(1).is_ok());
}

#[test]
fn valid_plan_places_every_entry_and_bracket_lot_once_in_order() {
    let mut h = configured(3.0, 5.0, 1.0, 3.0, 4);
    let mut bracket = ordinary(7.0, 1.0);
    bracket["stopLoss"] = json!(0.5);
    bracket["takeProfit"] = json!(2.0);
    batch(&mut h, vec![bracket, ordinary(3.0, 1.0)]);
    let mut sent = Vec::new();
    while h.requests.iter().any(|r| r.method == Method::Post) {
        let request = h.take(Method::Post);
        let kind = request
            .url
            .split('&')
            .find_map(|p| p.strip_prefix("type="))
            .expect("wire type")
            .to_owned();
        let amount = request
            .url
            .split('&')
            .find_map(|p| p.strip_prefix("quantity="))
            .expect("wire quantity")
            .to_owned();
        if kind != "LIMIT" {
            assert!(request.url.contains("reduceOnly=true"));
        }
        sent.push((kind, amount));
        h.reply(request, &format!(r#"{{"orderId":{}}}"#, 900 + sent.len()));
    }
    let expected: Vec<_> = [
        ("LIMIT", "4"),
        ("LIMIT", "3"),
        ("STOP_MARKET", "4"),
        ("STOP_MARKET", "3"),
        ("TAKE_PROFIT_MARKET", "4"),
        ("TAKE_PROFIT_MARKET", "3"),
        ("LIMIT", "3"),
    ]
    .into_iter()
    .map(|(kind, amount)| (kind.to_owned(), amount.to_owned()))
    .collect();
    assert_eq!(sent, expected);
    assert_eq!(
        h.sent_writes, 7,
        "each planned element was written exactly once"
    );
    assert_eq!(
        h.result(1).as_ref().expect("valid plan"),
        "[\"901\",\"902\",\"903\",\"904\",\"905\",\"906\",\"907\"]"
    );
    assert_eq!(h.failures.len(), 0);
}

#[test]
fn rebalancing_uses_multiple_donors_without_breaking_their_minima() {
    let mut h = configured(4.0, 5.0, 1.0, 0.0, 10);
    h.place(ordinary(16.0, 1.0));
    assert_eq!(quantities(&mut h), ["4", "4", "4", "4"]);
    assert!(h.result(1).is_ok());
}

#[test]
fn exact_decimal_caps_and_existing_half_up_rounding_are_preserved() {
    let mut h = configured(0.1, 0.3, 0.1, 0.0, 10);
    h.place(ordinary(0.6, 1.0));
    assert_eq!(quantities(&mut h), ["0.3", "0.3"]);
    h.place(ordinary(0.15, 1.0));
    assert_eq!(quantities(&mut h), ["0.2"]);
}

#[test]
fn market_open_checks_each_lot_at_the_mark_and_requires_that_reference() {
    for mark in [None, Some(1.0)] {
        let mut h = configured(1.0, 5.0, 1.0, 4.0, 10);
        h.engine.store_mut().set_tickers(
            mark.map(|mark| Ticker {
                symbol: "BTCUSDT".into(),
                mark,
                last: 100.0,
                ..Ticker::default()
            })
            .into_iter()
            .collect(),
        );
        h.place(json!({"symbol":"BTCUSDT","type":"market","side":"buy","amount":6}));
        refused(&h);
    }
}

#[test]
fn reduce_only_and_full_position_closes_keep_the_notional_exemption() {
    let mut h = configured(1.0, 5.0, 1.0, 100.0, 10);
    h.engine.store_mut().set_tickers(Vec::new());
    h.place(json!({"symbol":"BTCUSDT","type":"market","side":"sell","amount":1,"reduceOnly":true}));
    assert_eq!(quantities(&mut h), ["1"]);
    assert!(h.result(1).is_ok());
    h.place(json!({"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0,"triggerPrice":1.0}));
    let close = h.take(Method::Post);
    assert!(close.url.contains("closePosition=true"));
    assert!(!close.url.contains("quantity="));
    h.reply(close, r#"{"orderId":901}"#);
    assert!(h.failures.is_empty());
}

#[test]
fn invalid_last_v3_row_refuses_before_clock_or_writes() {
    let mut h = Harness::options(v3_options());
    let mut rows = vec![limit(); 8];
    rows.push(ordinary(1.0, 0.0));
    batch(&mut h, rows);
    refused(&h);
}

#[test]
fn unsupported_later_row_refuses_the_whole_plan() {
    for unsupported in [
        json!({"symbol":"BTCUSDT","type":"trailing_stop_market","side":"sell","amount":1,"price":1}),
        json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":1,"stpMode":"EXPIRE_TAKER"}),
    ] {
        let mut h = Harness::new();
        batch(&mut h, vec![limit(), unsupported]);
        assert!(h.requests.is_empty());
        assert_eq!(h.failures[0].1.outcome, Some(Outcome::NotSent));
    }
}

#[test]
fn ambiguous_first_lot_never_releases_or_replays_later_lots() {
    for (status, answer) in [
        (503, r#"{"code":-2010,"msg":"uncertain gateway"}"#),
        (200, r#"{"code":-1007,"msg":"unknown execution"}"#),
        (200, r#"{"code":200,"msg":"missing order ID"}"#),
    ] {
        let mut h = configured(1.0, 5.0, 1.0, 0.0, 10);
        h.place(ordinary(7.0, 1.0));
        let first = h.take(Method::Post);
        h.reply_status(first, status, answer);
        assert_eq!(h.sent_writes, 1);
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
        assert_eq!(h.failures.len(), 1);
        assert_eq!(h.failures[0].1.outcome, Some(Outcome::Unknown));
        h.collect();
        assert!(!h.requests.iter().any(|r| r.method == Method::Post));
    }
}
