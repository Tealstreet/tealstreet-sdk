//! Legacy TS sends one normalized native lot payload, including above-max sizes.
use super::*;

fn harness(maximum: f64, step: f64) -> Harness {
    let mut h = Harness::new();
    let mut market = h.engine.store().market("BTCUSDT").unwrap().clone();
    market.precision.amount = step;
    market.limits.amount.min = step;
    market.limits.amount.max = maximum;
    h.engine.store_mut().set_markets(vec![market]);
    h
}

fn request(
    h: &mut Harness,
    amount: f64,
    kind: OrderType,
    expected_size: &str,
    expected_sum: f64,
) -> HttpRequest {
    let mut opts = order();
    opts.amount = amount;
    opts.kind = kind;
    place(h, vec![opts]);
    h.metadata(false, "CROSS");
    let request = h.take("/api/v1/orders");
    assert_eq!(request.method, Method::Post);
    assert!(request.url.ends_with("/api/v1/orders"));
    let actual: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    let mut expected = serde_json::json!({
        "symbol":"XBTUSDTM", "type":"limit", "side":"buy", "size":expected_size,
        "price":"100", "leverage":5.0, "reduceOnly":false, "marginMode":"CROSS",
        "timeInForce":"GTC", "clientOid":"caller_ABC-123"
    });
    if kind == OrderType::Market {
        expected["type"] = "market".into();
        expected.as_object_mut().unwrap().remove("price");
        expected.as_object_mut().unwrap().remove("timeInForce");
    }
    assert_eq!(actual, expected);
    let sizes = [actual["size"].as_str().unwrap().parse::<f64>().unwrap()];
    assert_eq!(
        sizes.iter().sum::<f64>(),
        expected_sum,
        "sum of the single TS payload"
    );
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    request
}

// TS: packages/safe-cex/src/__tests__/reference/kucoin/kucoin.exchange.ts:881-925;
// venue refusal is emitted and thrown by placeOrderBatch at :983-1005.
// KNOWN-ISSUE: TS sends above-max; venue refuses
fn assert_above_max(amount: f64, maximum: f64, step: f64, kind: OrderType, size: &str, sum: f64) {
    let mut h = harness(maximum, step);
    let request = request(&mut h, amount, kind, size, sum);
    h.response(
        &request,
        200,
        r#"{"code":"400100","msg":"Order size exceeds market maximum"}"#,
    );
    h.pace();
    assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    assert_eq!(h.failures.len(), 1);
    assert_eq!(h.failures[0].outcome, Some(Outcome::Rejected));
    assert_eq!(h.failures[0].venue_code.as_deref(), Some("400100"));
    assert_eq!(
        h.done(1),
        Err("KuCoin API Error 400100: Order size exceeds market maximum (HTTP 200)".into())
    );
}

#[test]
fn twenty_six_stays_one_native_payload() {
    // KNOWN-ISSUE: TS sends above-max; venue refuses
    assert_above_max(26., 5., 1., OrderType::Limit, "26", 26.);
}

#[test]
fn eleven_stays_one_native_payload() {
    // KNOWN-ISSUE: TS sends above-max; venue refuses
    assert_above_max(11., 5., 1., OrderType::Limit, "11", 11.);
}

#[test]
fn twelve_stays_one_native_payload() {
    // KNOWN-ISSUE: TS sends above-max; venue refuses
    assert_above_max(12., 5., 1., OrderType::Limit, "12", 12.);
}

#[test]
fn exact_multiple_still_sends_one_oversized_payload() {
    // KNOWN-ISSUE: TS sends above-max; venue refuses
    assert_above_max(10., 5., 1., OrderType::Limit, "10", 10.);
}

#[test]
fn half_step_stays_one_native_payload() {
    // KNOWN-ISSUE: TS sends above-max; venue refuses
    assert_above_max(7., 3., 0.5, OrderType::Limit, "7", 7.);
}

#[test]
fn thousandth_step_rounds_before_sending_one_payload() {
    // KNOWN-ISSUE: TS sends above-max; venue refuses
    assert_above_max(1.0005, 0.3, 0.001, OrderType::Limit, "1.001", 1.001);
}

#[test]
fn market_order_keeps_its_single_oversized_payload() {
    // KNOWN-ISSUE: TS sends above-max; venue refuses
    assert_above_max(26., 5., 1., OrderType::Market, "26", 26.);
}

#[test]
fn rounding_above_the_maximum_does_not_add_a_local_refusal() {
    // KNOWN-ISSUE: TS sends above-max; venue refuses
    assert_above_max(4.6, 4.8, 1., OrderType::Limit, "5", 5.);
}

#[test]
fn below_and_at_maximum_still_place_one_native_order() {
    for (amount, size) in [(4., "4"), (5., "5")] {
        let mut h = harness(5., 1.);
        let request = request(&mut h, amount, OrderType::Limit, size, amount);
        h.response(
            &request,
            200,
            r#"{"code":"200000","data":{"orderId":"accepted"}}"#,
        );
        h.pace();
        assert_eq!(h.done(1), Ok(r#"["accepted"]"#.into()));
        assert!(h.failures.is_empty());
        assert!(!h.actions.iter().any(|a| matches!(a, Action::Http(_))));
    }
}
