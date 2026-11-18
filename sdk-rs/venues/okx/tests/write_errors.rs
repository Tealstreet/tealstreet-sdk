#![allow(dead_code)]
mod support;

use safe_cex_engine::{Action, Command, ErrorKind, Event, Notice};
use support::*;

// Recorded OKX v5 response shape; the documented per-order code/message is
// authoritative when the enclosing operation fails with an empty msg.
// https://www.okx.com/docs-v5/en/#order-book-trading-trade-post-place-order
const REFUSAL: &str = include_str!("fixtures/place-order-insufficient-balance.json");
const REASON: &str = "Order failed. Insufficient USDT balance in account";

fn refusal(command: Command, response: &str) -> safe_cex_engine::VenueError {
    let mut engine = trading_engine();
    let settings = engine.store().setting("BTCUSDT");
    engine.handle(5., Event::Command(command));
    let out = prepared(&mut engine, 5.);
    let request = out
        .iter()
        .find(|action| matches!(action, Action::Http(request) if request.method != safe_cex_engine::Method::Get))
        .expect("one released write");
    reply(&mut engine, request, response, 6.);
    let out = prepared(&mut engine, 6.);
    assert!(
        !out.iter().any(|action| matches!(action, Action::Http(request) if request.method != safe_cex_engine::Method::Get)),
        "a refused write is not sent again: {out:?}"
    );
    let failures: Vec<_> = out
        .into_iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { error, .. }) => Some(error),
            _ => None,
        })
        .collect();
    assert_eq!(engine.store().setting("BTCUSDT"), settings);
    assert_eq!(failures.len(), 1);
    failures.into_iter().next().unwrap()
}

fn placement() -> Command {
    Command::PlaceOrders {
        command: 1,
        orders: vec![
            serde_json::from_str(
                r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":38000}"#,
            )
            .unwrap(),
        ],
    }
}

fn assert_reason(error: safe_cex_engine::VenueError) {
    assert_eq!(error.venue_code.as_deref(), Some("51008"), "{error:?}");
    assert!(error.message.contains(REASON), "{error:?}");
    assert_eq!(error.venue_message.as_deref(), Some(REASON), "{error:?}");
    assert_eq!(error.kind, Some(ErrorKind::InsufficientBalance));
}

#[test]
fn blank_envelope_message_preserves_placement_refusal() {
    assert_reason(refusal(placement(), REFUSAL));
}

#[test]
fn blank_envelope_message_preserves_escaped_placement_refusal() {
    let response = REFUSAL.replace("USDT", r"\u0055SDT");
    assert_reason(refusal(placement(), &response));
}

#[test]
fn blank_envelope_message_preserves_settings_write_refusal() {
    assert_reason(refusal(
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.,
            side: None,
        },
        REFUSAL,
    ));
}

#[test]
fn nonempty_envelope_message_keeps_the_existing_settings_diagnostic() {
    let mut response: serde_json::Value = serde_json::from_str(REFUSAL).unwrap();
    response["msg"] = "Top-level refusal".into();
    let error = refusal(
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.,
            side: None,
        },
        &response.to_string(),
    );
    assert_eq!(error.venue_code.as_deref(), Some("1"));
    assert_eq!(
        error.venue_message.as_deref(),
        Some("OKX 1: Top-level refusal")
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
}

#[test]
fn mixed_settings_rows_keep_unknown_outcome_and_report_the_failed_row() {
    let mut response: serde_json::Value = serde_json::from_str(REFUSAL).unwrap();
    response["data"]
        .as_array_mut()
        .unwrap()
        .insert(0, serde_json::json!({ "sCode": "0", "sMsg": "" }));
    let error = refusal(
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.,
            side: None,
        },
        &response.to_string(),
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let message = error.venue_message.as_deref().unwrap();
    assert!(message.contains(REASON), "{error:?}");
    assert!(message.contains("51008"), "{error:?}");
}

#[test]
fn unknown_native_settings_error_stays_unknown_without_resending() {
    let mut response: serde_json::Value = serde_json::from_str(REFUSAL).unwrap();
    response["data"][0]["sCode"] = "50013".into();
    response["data"][0]["sMsg"] = "Busy".into();
    let error = refusal(
        Command::SetLeverage {
            command: 1,
            symbol: "BTCUSDT".into(),
            leverage: 20.,
            side: None,
        },
        &response.to_string(),
    );
    assert_eq!(error.venue_code.as_deref(), Some("50013"));
    assert_eq!(error.venue_message.as_deref(), Some("Busy"));
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
}

#[test]
fn escaped_refusal_preserves_the_accepted_sibling() {
    let mut command = placement();
    let Command::PlaceOrders { orders, .. } = &mut command else {
        unreachable!()
    };
    orders.push(orders[0].clone());
    let mut response: serde_json::Value = serde_json::from_str(REFUSAL).unwrap();
    response["code"] = "2".into();
    response["data"].as_array_mut().unwrap().insert(
        0,
        serde_json::json!({ "ordId": "accepted", "sCode": "0", "sMsg": "" }),
    );
    let error = refusal(command, &response.to_string().replace("USDT", r"\u0055SDT"));
    assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted"]);
    assert_eq!(error.outcome, None);
    assert_reason(error);
}

#[test]
fn successful_envelope_keeps_the_existing_ack_decoder() {
    // Recovery is limited to failed envelopes; do not broaden success decoding.
    let error = refusal(
        placement(),
        r#"{"code":"0","msg":"","data":[{"ordId":"accepted","sCode":"0","sMsg":"\u006f\u006b"}]}"#,
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
}

#[test]
fn nonempty_envelope_message_keeps_the_existing_placement_diagnostic() {
    let mut response: serde_json::Value = serde_json::from_str(REFUSAL).unwrap();
    response["msg"] = "Top-level refusal".into();
    let response = response.to_string().replace("USDT", r"\u0055SDT");
    let error = refusal(placement(), &response);
    assert_eq!(error.venue_code.as_deref(), Some("1"));
    assert_eq!(
        error.venue_message.as_deref(),
        Some("OKX 1: Top-level refusal")
    );
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
}

#[test]
fn successful_ack_without_message_keeps_the_existing_default() {
    let mut engine = trading_engine();
    engine.handle(5., Event::Command(placement()));
    let out = prepared(&mut engine, 5.);
    let request = out
        .iter()
        .find(|action| {
            matches!(action,
                Action::Http(request) if request.method != safe_cex_engine::Method::Get
            )
        })
        .expect("one released write");
    reply(
        &mut engine,
        request,
        r#"{"code":"0","msg":"","data":[{"ordId":"accepted","sCode":"0"}]}"#,
        6.,
    );
    let out = prepared(&mut engine, 6.);
    assert!(out.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandDone { command: 1, result }) if result == r#"["accepted"]"#
    )), "an omitted sMsg retains its empty borrowed default: {out:?}");
    assert!(
        !out.iter()
            .any(|action| matches!(action, Action::Notice(Notice::CommandFailed { .. })))
    );
}
