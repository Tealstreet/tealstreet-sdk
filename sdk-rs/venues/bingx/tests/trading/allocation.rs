//! Exact legacy allocation, observed through signed native requests.
use super::*;

// TS: packages/safe-cex/src/__tests__/reference/bingx/bingx.exchange.ts:1544-1564.
// In particular, TS rounds equal-sized lots independently. Do not redistribute
// the rounding difference to conserve the original requested quantity.
fn assert_lots(amount: f64, maximum: f64, step: f64, expected: &[f64], expected_sum: f64) {
    let mut h = Harness::new(None);
    let mut m = market();
    m.precision.amount = step;
    m.limits.amount.max = maximum;
    m.limits.amount.min = step;
    h.engine.store_mut().set_markets(vec![m]);
    h.place(vec![PlaceOrderOpts { amount, ..opts() }]);
    let mut clients = std::collections::HashSet::new();
    let mut ids = Vec::new();
    let mut actual_sum = 0.;
    for (index, quantity) in expected.iter().enumerate() {
        assert_eq!(h.requests.len(), 1, "one serial lot at index {index}");
        assert_eq!(h.requests[0].method, safe_cex_engine::Method::Post);
        let body: serde_json::Value = serde_json::from_str(h.body()).unwrap();
        assert_eq!(
            body["quantity"].as_f64(),
            Some(*quantity),
            "lot {index}: {amount}/{maximum}/{step}"
        );
        actual_sum += body["quantity"].as_f64().unwrap();
        assert!(clients.insert(body["clientOrderId"].as_str().unwrap().to_owned()));
        let id = format!("allocation-{index}");
        h.reply(&serde_json::json!({"code":0,"data":{"orderId":id}}).to_string());
        ids.push(id);
    }
    assert!(
        (actual_sum - expected_sum).abs() < 1e-12,
        "TS lot sum {actual_sum}, expected {expected_sum}: {amount}/{maximum}/{step}"
    );
    assert!(h.requests.is_empty(), "no additional tail");
    assert!(h.failures.is_empty());
    assert_eq!(h.done.len(), 1);
    let placed: Vec<String> = serde_json::from_str(h.done[0].as_ref().unwrap()).unwrap();
    assert_eq!(placed, ids, "every physical lot completes exactly once");
}

#[test]
fn twenty_six_uses_six_equal_lots_and_the_native_remainder() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
    assert_lots(26., 5., 1., &[4., 4., 4., 4., 4., 4., 1.], 25.);
}

#[test]
fn eleven_uses_three_equal_lots_and_the_native_remainder() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 10 of requested 11); kept for exact TS parity per Sam's decision
    assert_lots(11., 5., 1., &[3., 3., 3., 1.], 10.);
}

#[test]
fn twelve_keeps_the_ts_rounded_total() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 11 of requested 12); kept for exact TS parity per Sam's decision
    assert_lots(12., 5., 1., &[3., 3., 3., 2.], 11.);
}

#[test]
fn exact_multiple_has_no_extra_remainder() {
    assert_lots(10., 5., 1., &[5., 5.], 10.);
}

#[test]
fn half_step_uses_equal_lots_before_the_remainder() {
    assert_lots(7., 3., 0.5, &[2., 2., 2., 1.], 7.);
}

#[test]
fn thousandth_step_normalizes_before_allocating_equal_lots() {
    assert_lots(
        1.0005,
        0.3,
        0.001,
        &[0.225, 0.225, 0.225, 0.225, 0.101],
        1.001,
    );
}
