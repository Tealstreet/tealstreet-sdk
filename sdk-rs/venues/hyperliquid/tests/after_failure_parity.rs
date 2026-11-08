//! Legacy TS response interceptors and updateOrder determine continuation.
mod support;
use safe_cex_engine::{Action, Command, Event, Notice};
use serde_json::{Value, json};
use support::{advance_packet_timers, message, prepared_now, ready, reply};

fn amend_trace(
    protective: bool,
    cancel_error: Option<&str>,
    replacement_error: Option<&str>,
    expected: &[&str],
) {
    let (mut engine, socket) = ready("disabled");
    let source = if protective {
        json!({"coin":"BTC","oid":9,"side":"A","limitPx":"70","triggerPx":"80",
            "origSz":"0","sz":"0","reduceOnly":true,"orderType":"Stop Market",
            "isTrigger":true,"isPositionTpsl":true,"cloid":"0x11111111111111111111111111111111"})
    } else {
        json!({"coin":"BTC","oid":9,"side":"B","limitPx":"100","origSz":"3","sz":"3",
            "reduceOnly":false,"orderType":"Limit"})
    };
    message(
        &mut engine,
        socket,
        1500.,
        &json!({"channel":"orderUpdates","data":[{"order":source,"status":"open"}]}).to_string(),
    );
    prepared_now(&mut engine);
    engine.handle(
        2000.,
        Event::Command(Command::AmendOrder {
            command: 77,
            id: "9".into(),
            price: Some(if protective { 75. } else { 110. }),
            amount: None,
        }),
    );
    let mut actions = prepared_now(&mut engine);
    let mut writes = Vec::new();
    let mut terminal = false;
    for _ in 0..10 {
        for action in &actions {
            match action {
                Action::Http(request) if request.url.ends_with("/exchange") => {
                    let body: Value =
                        serde_json::from_str(request.body.as_deref().expect("body")).expect("wire");
                    let kind = body["action"]["type"].as_str().expect("action type");
                    writes.push(kind.to_owned());
                    assert!(
                        writes.len() <= expected.len(),
                        "unexpected mutation after failure: {writes:?}"
                    );
                    let status = if kind == "cancel" {
                        cancel_error
                            .map_or_else(|| json!("success"), |error| json!({"error":error}))
                    } else {
                        replacement_error.map_or_else(
                            || json!({"resting":{"oid":91}}),
                            |error| json!({"error":error}),
                        )
                    };
                    reply(&mut engine, request, 200, &json!({"status":"ok","response":{"type":kind,"data":{"statuses":[status]}}}).to_string());
                }
                Action::Http(request) if support::kind(request) == "orderStatus" => {
                    reply(
                        &mut engine,
                        request,
                        200,
                        &json!({"status":"order","order":{"order":source,"status":"open"}})
                            .to_string(),
                    );
                }
                Action::Notice(
                    Notice::CommandDone { command: 77, .. }
                    | Notice::CommandFailed { command: 77, .. },
                ) => {
                    assert!(!terminal, "one terminal notice");
                    terminal = true;
                }
                _ => {}
            }
        }
        let next = prepared_now(&mut engine);
        actions = advance_packet_timers(&mut engine, next, 20_000.);
    }
    assert_eq!(writes, expected);
    assert!(terminal, "one terminal notice");
}

#[test]
fn terminal_cancel_miss_continues_to_replacement_as_ts() {
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | cancel-replace/audit trading.rs:405
    // TS hyperliquid.exchange.ts:4227-4233,4411-4416 swallows this terminal state.
    amend_trace(
        false,
        Some("Order was never placed, already canceled, or filled. asset=0"),
        None,
        &["cancel", "order"],
    );
}

#[test]
fn ordinary_cancel_error_stops_before_replacement_as_ts() {
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | cancel-replace/audit trading.rs:405
    // TS hyperliquid.api.ts:78-95 throws; hyperliquid.exchange.ts:4231-4232 rethrows.
    amend_trace(false, Some("Cannot cancel this order"), None, &["cancel"]);
}

#[test]
fn full_position_replacement_refusal_stops_without_rust_only_restore() {
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | protective-compensation
    // TS hyperliquid.exchange.ts:4411-4416 has no catch/put-back after placement failure.
    amend_trace(
        true,
        None,
        Some("Invalid TP/SL price."),
        &["cancel", "order"],
    );
}

#[test]
fn ordinary_replacement_refusal_pins_cancel_then_failed_place_without_suffix() {
    // TS hyperliquid.exchange.ts:4411-4416 cancels before the rejected placement.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: hyperliquid | cancel-replace/audit trading.rs:405
    amend_trace(
        false,
        None,
        Some("Order has invalid price."),
        &["cancel", "order"],
    );
}
