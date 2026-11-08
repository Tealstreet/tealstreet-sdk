mod support;
use safe_cex_engine::{Action, Command, Event, Method, Notice};
use serde_json::{Value, json};
use support::*;

// TS: packages/safe-cex/src/__tests__/reference/orderly/orderly.exchange.ts:1044,1073-1082.
// Serial dispatch/refusal: same file:1084-1106. These are actual signed HTTP payloads.
fn assert_ts_split(amount: f64, max: f64, step: f64, expected: &[f64], sum: f64) {
    let (mut e, _) = ready();
    actions(&mut e);
    e.store_mut().update_markets(|market| {
        market.precision.amount = step;
        market.limits.amount.min = step;
        market.limits.amount.max = max;
    });
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 900,
            orders: serde_json::from_value(json!([{
                "symbol":"BTCUSDC", "type":"limit", "side":"buy", "amount":amount, "price":60000
            }]))
            .unwrap(),
        }),
    );
    let mut quantities = vec![];
    let mut completions = 0;
    for _ in 0..256 {
        let mut writes = 0;
        for action in actions(&mut e) {
            match action {
                Action::Http(request)
                    if request.method == Method::Post && request.url.ends_with("/v1/order") =>
                {
                    let body: Value =
                        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
                    quantities.push(body["order_quantity"].as_f64().unwrap());
                    writes += 1;
                    reply(
                        &mut e,
                        &request,
                        200,
                        &json!({"success":true,"data":{"order_id":1000 + quantities.len()}})
                            .to_string(),
                    );
                }
                Action::Notice(Notice::CommandDone {
                    command: 900,
                    result,
                }) => {
                    let ids: Vec<String> = serde_json::from_str(&result).unwrap();
                    assert_eq!(ids.len(), expected.len());
                    completions += 1;
                }
                Action::Notice(Notice::CommandFailed {
                    command: 900,
                    error,
                }) => panic!("unexpected local/venue refusal: {error:?}"),
                _ => {}
            }
        }
        if writes == 0 {
            break;
        }
    }
    assert_eq!(
        quantities, expected,
        "requested {amount}, max {max}, step {step}"
    );
    assert!(
        (quantities.iter().sum::<f64>() - sum).abs() < 1e-10,
        "TS sum {sum}: {quantities:?}"
    );
    assert_eq!(completions, 1);
}

#[test]
fn ts_split_26_5_1_preserves_legacy_underallocation() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
    assert_ts_split(26., 5., 1., &[4., 4., 4., 4., 4., 4., 1.], 25.);
}
#[test]
fn ts_split_11_5_1_preserves_legacy_underallocation() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 10 of requested 11); kept for exact TS parity per Sam's decision
    assert_ts_split(11., 5., 1., &[3., 3., 3., 1.], 10.);
}
#[test]
fn ts_split_12_5_1_preserves_legacy_underallocation() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 11 of requested 12); kept for exact TS parity per Sam's decision
    assert_ts_split(12., 5., 1., &[3., 3., 3., 2.], 11.);
}
#[test]
fn ts_split_41_20_fractional_preserves_complete_legacy_sequence() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 40.999 of requested 41); kept for exact TS parity per Sam's decision
    assert_ts_split(41., 20., 0.001, &[13.333, 13.333, 13.333, 1.], 40.999);
}
#[test]
fn ts_split_above_lodash_count_boundary_sends_no_equal_lots() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 0 of requested 10000000000000000); kept for exact TS parity per Sam's decision
    assert_ts_split(10_000_000_000_000_000., 1., 1., &[], 0.);
}
#[test]
fn ts_split_above_lodash_count_boundary_keeps_separate_remainder() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 0.25 of requested 10000000000000000); kept for exact TS parity per Sam's decision
    assert_ts_split(10_000_000_000_000_000., 0.75, 0.25, &[0.25], 0.25);
}
#[test]
fn ts_split_rounding_overflow_sends_no_orders() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 0 of requested 1e308); kept for exact TS parity per Sam's decision
    assert_ts_split(1., 20., 0.001, &[1.], 1.);
    let (mut e, _) = ready();
    actions(&mut e);
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 902,
            orders: serde_json::from_value(json!([{
                "symbol":"BTCUSDC", "type":"limit", "side":"buy", "amount":1e308, "price":60000
            }]))
            .unwrap(),
        }),
    );
    let out = actions(&mut e);
    let writes: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Http(request) if request.method != Method::Get => Some(request),
            _ => None,
        })
        .collect();
    assert!(writes.is_empty(), "TS emits no overflow orders: {writes:?}");
    let completions: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandDone {
                command: 902,
                result,
            }) => Some(result),
            _ => None,
        })
        .collect();
    assert_eq!(completions.len(), 1);
    assert_eq!(*completions[0], "[]");
}
#[test]
fn ts_split_exact_multiple_control() {
    assert_ts_split(10., 5., 1., &[5., 5.], 10.);
}
#[test]
fn ts_split_half_step() {
    assert_ts_split(7., 3., 0.5, &[2., 2., 2., 1.], 7.);
}
#[test]
fn ts_split_fractional_rounding() {
    assert_ts_split(
        1.0005,
        0.3,
        0.001,
        &[0.225, 0.225, 0.225, 0.225, 0.101],
        1.001,
    );
}
#[test]
fn ts_split_under_limit_control() {
    assert_ts_split(1., 5., 1., &[1.], 1.);
}
#[test]
fn ts_split_does_not_add_a_hundred_lot_limit() {
    assert_ts_split(101., 1., 1., &[1.; 101], 101.);
}

#[test]
fn ts_split_sends_a_below_minimum_tail_and_surfaces_native_refusal() {
    // KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
    // TS :1073-1106 sends the below-minimum tail, then stops on the venue refusal.
    let (mut e, _) = ready();
    actions(&mut e);
    e.store_mut().update_markets(|market| {
        market.precision.amount = 1.;
        market.limits.amount.min = 2.;
        market.limits.amount.max = 5.;
    });
    e.handle(
        2000.,
        Event::Command(Command::PlaceOrders {
            command: 901,
            orders: serde_json::from_value(
                json!([{"symbol":"BTCUSDC","type":"limit","side":"buy","amount":26,"price":60000}]),
            )
            .unwrap(),
        }),
    );
    let mut quantities = vec![];
    for index in 0..7 {
        let out = actions(&mut e);
        let writes: Vec<_> = out
            .into_iter()
            .filter_map(|action| match action {
                Action::Http(request) if request.method == Method::Post => Some(request),
                _ => None,
            })
            .collect();
        assert_eq!(
            writes.len(),
            1,
            "TS sends lot {index} before any native refusal"
        );
        let request = &writes[0];
        let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        quantities.push(body["order_quantity"].as_f64().unwrap());
        if index == 6 {
            reply(
                &mut e,
                request,
                400,
                r#"{"success":false,"code":-1103,"message":"below native minimum"}"#,
            );
        } else {
            reply(
                &mut e,
                request,
                200,
                &json!({"success":true,"data":{"order_id":1000 + index}}).to_string(),
            );
        }
    }
    assert_eq!(quantities, [4., 4., 4., 4., 4., 4., 1.]);
    assert_eq!(quantities.iter().sum::<f64>(), 25.);
    let out = actions(&mut e);
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Http(request) if request.method != Method::Get))
    );
    let failures: Vec<_> = out
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed {
                command: 901,
                error,
            }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].message.contains("below native minimum"));
    assert_eq!(failures[0].partial.as_ref().unwrap().order_ids.len(), 6);
}
