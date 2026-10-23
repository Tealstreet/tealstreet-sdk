//! Original REDs for the S5 write cutover: mixed placement outcomes and native
//! refusal codes. Each fails on an assertion against today's source.
#[allow(dead_code)]
mod support;
use safe_cex_engine::model::PlaceOrderOpts;
use safe_cex_engine::{Command, Event};
use support::*;

fn place(h: &mut Harness, count: usize) {
    let orders: Vec<PlaceOrderOpts> = serde_json::from_value(serde_json::Value::Array(
        (0..count)
            .map(|index| serde_json::json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":60000,"clientOrderId":format!("Caller{index}")}))
            .collect(),
    ))
    .unwrap();
    h.e.handle(
        1.0,
        Event::Command(Command::PlaceOrders { command: 7, orders }),
    );
    h.collect();
}

fn failure(h: &Harness) -> &safe_cex_engine::VenueError {
    let [error] = h.failures.as_slice() else {
        panic!("expected one command failure: {:?}", h.failures);
    };
    error
}

#[test]
#[ignore = "original RED: a mixed placement failure drops the accepted IDs (#12); enabled by the S3-S6 cutover"]
fn mixed_batch_failure_reports_the_accepted_ids_as_partial() {
    let mut h = Harness::new();
    h.seed();
    place(&mut h, 2);
    h.reply(
        "batch-orders",
        r#"{"code":"1","msg":"","data":[{"orderId":"Accepted0","code":"0","msg":""},{"orderId":"","code":"102002","msg":"Insufficient balance"}]}"#,
    );
    let error = failure(&h);
    let partial = error
        .partial
        .as_deref()
        .unwrap_or_else(|| panic!("accepted IDs dropped: {error:?}"));
    assert_eq!(partial.order_ids, ["Accepted0"]);
}

#[test]
#[ignore = "original RED: a later batch failure drops earlier accepted IDs (#12); enabled by the S3-S6 cutover"]
fn later_batch_refusal_reports_the_earlier_accepted_ids_as_partial() {
    let mut h = Harness::new();
    h.seed();
    place(&mut h, 21);
    let ack: Vec<_> = (0..20)
        .map(|index| serde_json::json!({"orderId":format!("Accepted{index}"),"code":"0"}))
        .collect();
    h.reply(
        "batch-orders",
        &serde_json::json!({"code":"0","data":ack}).to_string(),
    );
    let pause = h
        .actions
        .iter()
        .rev()
        .find_map(|action| match action {
            safe_cex_engine::Action::SetTimer { timer, at_ms } if *at_ms == 350. => Some(*timer),
            _ => None,
        })
        .expect("batch pause");
    h.e.handle(350., Event::TimerFired { timer: pause });
    h.collect();
    h.reply(
        "batch-orders",
        r#"{"code":"1","msg":"","data":[{"orderId":"","code":"102002","msg":"Insufficient balance"}]}"#,
    );
    let error = failure(&h);
    let partial = error
        .partial
        .as_deref()
        .unwrap_or_else(|| panic!("accepted IDs dropped: {error:?}"));
    let expected: Vec<_> = (0..20).map(|index| format!("Accepted{index}")).collect();
    assert_eq!(partial.order_ids, expected);
}

#[test]
#[ignore = "original RED: explicit native refusal codes stay unclassified (#35); enabled by the S3-S6 cutover"]
fn explicit_row_refusal_carries_its_native_code_and_a_table_verdict() {
    // 102002 is a documented refusal; 999999 exercises the table's unlisted default.
    for code in ["102002", "999999"] {
        let mut h = Harness::new();
        h.seed();
        place(&mut h, 1);
        h.reply(
            "batch-orders",
            &format!(
                r#"{{"code":"1","msg":"","data":[{{"orderId":"","code":"{code}","msg":"refused"}}]}}"#
            ),
        );
        let error = failure(&h);
        assert_eq!(error.venue_code.as_deref(), Some(code), "{error:?}");
        assert!(error.outcome.is_some(), "{code} has no verdict: {error:?}");
    }
}
