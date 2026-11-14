mod support;
use safe_cex_engine::{Method, Notice};
use serde_json::json;
use support::trading::*;

// TS: packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts:1114,1180-1196.
// Use reduce-only market orders for base-unit wire volume; limit coverage follows too.
fn assert_ts_split(kind: &str, amount: f64, max: f64, step: f64, expected: &[f64], sum: f64) {
    let mut h = Harness::new();
    h.e.store_mut().update_markets(|market| {
        market.precision.amount = step;
        market.limits.amount.min = step;
        market.limits.amount.max = max;
    });
    h.place(
        &json!([{
            "symbol":"E-BTC-USDT", "type":kind, "side":"buy", "reduceOnly":true,
            "amount":amount, "price":2000
        }])
        .to_string(),
    );
    let mut quantities = vec![];
    let mut clients = std::collections::HashSet::new();
    while let Some(index) = h
        .requests
        .iter()
        .position(|r| r.method == Method::Post && r.url.ends_with("/fapi/v2/order"))
    {
        assert!(
            quantities.len() < expected.len() + 1,
            "unexpected extra split requests"
        );
        let request = h.requests.remove(index);
        let row = body(&request);
        quantities.push(row["volume"].as_str().unwrap().parse::<f64>().unwrap());
        assert!(clients.insert(row["clientOrderId"].as_str().unwrap().to_owned()));
        h.reply(
            request,
            &json!({"code":"0","data":{"orderId":1000 + quantities.len()}}).to_string(),
        );
    }
    assert!(
        h.failures.is_empty(),
        "unexpected local/venue refusal: {:?}",
        h.failures
    );
    assert_eq!(
        quantities, expected,
        "requested {amount}, max {max}, step {step}"
    );
    assert!(
        (quantities.iter().sum::<f64>() - sum).abs() < 1e-10,
        "TS sum {sum}: {quantities:?}"
    );
    let ids: Vec<String> = serde_json::from_str(h.result().expect("command completion")).unwrap();
    assert_eq!(ids.len(), expected.len());
    assert_eq!(
        h.notices
            .iter()
            .filter(|n| matches!(n, Notice::CommandDone { command: 7, .. }))
            .count(),
        1
    );
}

#[test]
fn ts_split_26_5_1_preserves_legacy_underallocation() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
    assert_ts_split("market", 26., 5., 1., &[4., 4., 4., 4., 4., 4., 1.], 25.);
}
#[test]
fn ts_split_11_5_1_preserves_legacy_underallocation() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 10 of requested 11); kept for exact TS parity per Sam's decision
    assert_ts_split("market", 11., 5., 1., &[3., 3., 3., 1.], 10.);
}
#[test]
fn ts_split_12_5_1_preserves_legacy_underallocation() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 11 of requested 12); kept for exact TS parity per Sam's decision
    assert_ts_split("market", 12., 5., 1., &[3., 3., 3., 2.], 11.);
}
#[test]
fn ts_split_exact_multiple_control() {
    assert_ts_split("market", 10., 5., 1., &[5., 5.], 10.);
}
#[test]
fn ts_split_half_step() {
    assert_ts_split("market", 7., 3., 0.5, &[2., 2., 2., 1.], 7.);
}
#[test]
fn ts_split_fractional_rounding() {
    assert_ts_split(
        "market",
        1.0005,
        0.3,
        0.001,
        &[0.225, 0.225, 0.225, 0.225, 0.101],
        1.001,
    );
}
#[test]
fn ts_split_under_limit_control() {
    assert_ts_split("market", 1., 5., 1., &[1.], 1.);
}
#[test]
fn ts_split_limit_wire_volume_uses_the_same_legacy_rounding() {
    // KNOWN-ISSUE: legacy TS over-allocates (sum 0.026 of requested 0.025); kept for parity per Sam.
    assert_ts_split(
        "limit",
        0.025,
        0.01,
        0.001,
        &[0.007, 0.007, 0.007, 0.005],
        0.026,
    );
}
#[test]
fn ts_split_does_not_add_a_thousand_lot_limit_before_first_request() {
    let mut h = Harness::new();
    h.e.store_mut().update_markets(|market| {
        market.precision.amount = 1.;
        market.limits.amount.min = 1.;
        market.limits.amount.max = 1.;
    });
    h.place(
        r#"[{"symbol":"E-BTC-USDT","type":"market","side":"buy","reduceOnly":true,"amount":1001}]"#,
    );
    let request = h.take(Method::Post, "/fapi/v2/order");
    assert_eq!(body(&request)["volume"], "1");
    assert!(!h.failed());
}
#[test]
fn ts_split_sends_a_below_minimum_tail_and_surfaces_native_refusal() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
    // TS :1180-1196 formats the tail; :1283-1304 propagates the native refusal.
    let mut h = Harness::new();
    h.e.store_mut().update_markets(|market| {
        market.precision.amount = 1.;
        market.limits.amount.min = 2.;
        market.limits.amount.max = 5.;
    });
    h.place(
        r#"[{"symbol":"E-BTC-USDT","type":"market","side":"buy","reduceOnly":true,"amount":26}]"#,
    );
    let mut quantities = vec![];
    for index in 0..7 {
        let request = h.take(Method::Post, "/fapi/v2/order");
        quantities.push(
            body(&request)["volume"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap(),
        );
        if index == 6 {
            h.reply(request, r#"{"code":"-1102","msg":"below native minimum"}"#);
        } else {
            h.reply(
                request,
                &json!({"code":"0","data":{"orderId":1000 + index}}).to_string(),
            );
        }
    }
    assert_eq!(quantities, [4., 4., 4., 4., 4., 4., 1.]);
    assert_eq!(quantities.iter().sum::<f64>(), 25.);
    assert_eq!(h.writes(), 0);
    assert_eq!(h.failures.len(), 1);
    assert!(h.failures[0].message.contains("below native minimum"));
    assert_eq!(h.failures[0].partial.as_ref().unwrap().order_ids.len(), 6);
}
