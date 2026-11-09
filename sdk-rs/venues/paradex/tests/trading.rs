//! Trading contracts against documented request/response wire shapes.
mod support;
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, TimeInForce};
use safe_cex_engine::{Action, Command, Event, Method, OrderIdentityKind, OrderIdentityReceipt};
use safe_cex_venue_paradex::Options;
use serde::Deserialize;
use support::*;
fn private() -> Harness {
    let mut h = Harness::keyed(
        Options {
            paradex_address: Some("0x0123".into()),
            subkey_address: Some("0x456".into()),
            ..Options::default()
        },
        STARK_KEY,
    );
    h.reply(
        "/markets",
        &MARKETS.replace(
            "\"min_notional\":\"10\"",
            "\"price_bands_width\":\"0.05\",\"min_notional\":\"10\"",
        ),
    );
    h.reply("summary", TICKERS);
    h.reply(
        "/system/config",
        r#"{"starknet_chain_id":"PRIVATE_SN_PARACLEAR_MAINNET"}"#,
    );
    h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    h.reply("/orders", r#"{"results":[]}"#);
    h.reply("/account/margin", r#"{"configs":[]}"#);
    h
}

// Paradex declares no native rate-code table; RATE_LIMIT is diagnostic on 429.
const AMBIGUOUS_WRITES: &[(u16, &str)] = &[
    (429, "{}"),
    (429, r#"{"error":"RATE_LIMIT","message":"slow down"}"#),
    (429, r#"{"error":"INVALID_SIGNATURE","message":"refused"}"#),
    (408, r#"{"error":"INVALID_PRICE","message":"refused"}"#),
    (500, r#"{"error":"INVALID_SIGNATURE","message":"refused"}"#),
    (502, "bad gateway"),
    (503, r#"{"error":"INVALID_PRICE","message":"refused"}"#),
    (504, "gateway timeout"),
];

fn assert_dispatched_unknown(h: &Harness) {
    let error = failed(h, 91);
    assert_eq!(
        error.outcome,
        Some(safe_cex_engine::Outcome::Unknown),
        "{error:?}"
    );
    let text = error.legacy_text().to_ascii_lowercase();
    assert!(
        text.contains("unknown") && text.contains("check open orders"),
        "{text}"
    );
    assert!(
        !text.contains("refused") && !text.replace("may have no stop", "").contains("no stop"),
        "{text}"
    );
    assert_eq!(error.protection, None);
    assert!(
        !h.pending.iter().any(|r| r.method != Method::Get),
        "no resend or put-back: {:?}",
        h.pending
    );
}

#[test]
fn dispatched_ambiguous_ordinary_writes_are_unknown_without_resend() {
    for &(status, body) in AMBIGUOUS_WRITES {
        for operation in ["create", "amend", "cancel", "leverage"] {
            let mut h = if operation == "amend" {
                amendment_account(AMEND_ROW).0
            } else {
                private()
            };
            match operation {
                "create" => place(&mut h, 91, vec![opts(OrderType::Limit)]),
                "amend" => amend(&mut h, 91, Some(101.), Some(3.)),
                "cancel" => h.command(Command::CancelOrders {
                    command: 91,
                    ids: vec!["123".into()],
                }),
                "leverage" => h.command(Command::SetLeverage {
                    command: 91,
                    symbol: "BTC-USD-PERP".into(),
                    leverage: 20.,
                    side: None,
                }),
                _ => unreachable!(),
            }
            let writes: Vec<_> = h
                .pending
                .iter()
                .filter(|r| r.method != Method::Get)
                .cloned()
                .collect();
            assert_eq!(writes.len(), 1, "{operation}: {:?}", h.pending);
            let request = h.take(&writes[0].url);
            h.respond(request, status, body);
            assert_dispatched_unknown(&h);
        }
    }
}

#[test]
fn dispatched_ambiguous_protective_writes_are_unknown_without_put_back() {
    for &(status, body) in AMBIGUOUS_WRITES {
        for leg in ["create", "cancel", "amend-cancel", "replacement", "restore"] {
            let mut h = if leg == "create" {
                private()
            } else {
                with_untriggered_stop()
            };
            if leg == "create" {
                // An accepted entry precedes the dispatched protective child.
                let mut order = opts(OrderType::Limit);
                order.client_order_id = None;
                order.stop_loss = Some(90.);
                place(&mut h, 91, vec![order]);
                let entry = h.take("/orders");
                let payload: Payload =
                    serde_json::from_str(entry.body.as_deref().unwrap()).unwrap();
                h.respond(
                    entry,
                    200,
                    &serde_json::json!({"id":"entry", "client_id":payload.client_id}).to_string(),
                );
            } else if leg == "cancel" {
                h.command(Command::CancelOrders {
                    command: 91,
                    ids: vec!["123".into()],
                });
            } else {
                amend_stop(&mut h, 91);
                if leg != "amend-cancel" {
                    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
                    h.respond(cancel, 204, "");
                    confirm_stop_cancel(&mut h);
                    if leg == "restore" {
                        let replacement = h.take("/orders");
                        h.respond(
                            replacement,
                            400,
                            r#"{"error":"VALIDATION","message":"refused"}"#,
                        );
                    }
                }
            }
            let writes: Vec<_> = h
                .pending
                .iter()
                .filter(|r| r.method != Method::Get)
                .cloned()
                .collect();
            assert_eq!(writes.len(), 1, "{leg}: {:?}", h.pending);
            let request = h.take(&writes[0].url);
            h.respond(request, status, body);
            assert_dispatched_unknown(&h);
        }
    }
}
fn opts(kind: OrderType) -> PlaceOrderOpts {
    serde_json::from_str(&format!(r#"{{"symbol":"BTC-USD-PERP","type":{},"side":"buy","amount":1,"price":100,"clientOrderId":"caller"}}"#,serde_json::to_string(&kind).unwrap())).unwrap()
}
#[derive(Deserialize)]
struct Payload {
    #[serde(rename = "type")]
    kind: String,
    price: Option<String>,
    trigger_price: Option<String>,
    instruction: Option<String>,
    client_id: Option<String>,
    signature: Option<String>,
    signature_timestamp: Option<u64>,
    size: Option<String>,
    flags: Option<Vec<String>>,
}
fn place(h: &mut Harness, command: u32, orders: Vec<PlaceOrderOpts>) {
    h.command(Command::PlaceOrders { command, orders });
}
#[test]
fn capped_buy_uses_a_fresh_public_book_then_a_signed_limit_ioc_with_exact_caller_id() {
    let mut h = private();
    let mut order = opts(OrderType::Market);
    order.max_slippage_pct = Some(1.);
    order.price = None;
    place(&mut h, 1, vec![order]);
    assert_eq!(h.pending.len(), 1);
    let quote = h.take("/orderbook/");
    assert_eq!(quote.method, Method::Get);
    assert!(
        !h.receipt_trace
            .iter()
            .any(|a| matches!(a, ReceiptTrace::Progress(1, _)))
    );
    assert!(quote.url.ends_with("/orderbook/BTC-USD-PERP?depth=5"));
    assert!(!quote.headers.iter().any(|(n, _)| *n == "Authorization"));
    h.respond(quote, 200, r#"{"bids":[["102","1"]],"asks":[["103","2"]]}"#);
    let request = h.take("/orders");
    let payload: Payload = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "LIMIT");
    assert_eq!(payload.price.as_deref(), Some("103"));
    assert_eq!(payload.instruction.as_deref(), Some("IOC"));
    assert_receipts(
        &h,
        &request,
        1,
        vec![OrderIdentityReceipt {
            symbol: "BTC-USD-PERP".into(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: "caller".into(),
        }],
    );
    assert_eq!(payload.client_id.as_deref(), Some("caller"));
    assert!(payload.signature.unwrap().starts_with("[\""));
    assert_eq!(payload.signature_timestamp, Some(h.now as u64));
    assert!(
        request
            .headers
            .iter()
            .any(|(n, v)| *n == "PARADEX-PARTNER" && v == "TEALSTREET")
    );
    h.respond(request, 200, r#"{"id":"venue-1","client_id":"caller"}"#);
    assert_eq!(h.result(1), Ok("[\"venue-1\"]"));
}
#[test]
fn capped_prices_clamp_to_venue_bands_and_round_inward_when_the_tick_would_cross() {
    for (side, expected) in [(OrderSide::Buy, "105.5"), (OrderSide::Sell, "95.5")] {
        let mut h = private();
        let mut order = opts(OrderType::Market);
        order.side = side;
        order.max_slippage_pct = Some(20.);
        order.price = None;
        place(&mut h, 2, vec![order]);
        h.reply(
            "/orderbook/",
            r#"{"bids":[["99","1"]],"asks":[["101","1"]]}"#,
        );
        let request = h.take("/orders");
        let payload: Payload = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.price.as_deref(), Some(expected));
    }
}
#[test]
fn any_invalid_order_or_missing_touch_prevents_every_write_in_a_planned_batch() {
    let mut h = private();
    let mut cap = opts(OrderType::Market);
    cap.max_slippage_pct = Some(2.);
    let mut bad = opts(OrderType::Limit);
    bad.client_order_id = Some("other".into());
    bad.amount = f64::NAN;
    place(&mut h, 3, vec![cap.clone(), bad]);
    assert!(h.pending.is_empty());
    assert!(h.result(3).is_err());
    let mut limit = opts(OrderType::Limit);
    limit.client_order_id = Some("other".into());
    place(&mut h, 4, vec![limit, cap]);
    let quote = h.take("/orderbook/");
    h.respond(quote, 200, r#"{"bids":[["99","1"]],"asks":[]}"#);
    assert!(h.pending.is_empty());
    assert!(h.result(4).is_err());
}
#[test]
fn capped_market_rejects_conflicting_time_in_force_before_requesting_quotes() {
    let mut h = private();
    let mut order = opts(OrderType::Market);
    order.max_slippage_pct = Some(1.);
    order.time_in_force = Some(TimeInForce::GoodTillCancel);
    place(&mut h, 5, vec![order]);
    assert!(h.pending.is_empty());
    assert!(h.result(5).is_err());
}
#[test]
fn native_limit_batch_correlates_returned_ids_by_caller_identity_and_preflights_all_ids() {
    let mut h = private();
    let first = opts(OrderType::Limit);
    let mut second = first.clone();
    second.client_order_id = Some("second".into());
    place(&mut h, 6, vec![first.clone(), second]);
    let request = h.take("/orders/batch");
    assert!(request.body.as_deref().unwrap().starts_with('['));
    h.respond(request,200,r#"{"orders":[{"id":"id-second","client_id":"second"},{"id":"id-first","client_id":"caller"}]}"#);
    assert_eq!(h.result(6), Ok("[\"id-first\",\"id-second\"]"));
    place(&mut h, 7, vec![first.clone(), first]);
    assert!(h.pending.is_empty());
    assert!(h.result(7).is_err());
}
#[test]
fn a_lost_write_response_is_never_retried() {
    let mut h = private();
    place(&mut h, 8, vec![opts(OrderType::Limit)]);
    let request = h.take("/orders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    assert!(h.result(8).is_err());
    // Nothing is resent; the only request is the fence's reconcile read.
    let reconcile = h.pending.iter().map(|r| r.method).collect::<Vec<_>>();
    assert_eq!(reconcile, vec![Method::Get]);
    h.take("/orders");
}
#[test]
fn brackets_have_individual_signatures_and_accept_child_responses_without_a_client_identity() {
    let mut h = private();
    let mut order = opts(OrderType::Limit);
    order.client_order_id = None;
    order.take_profit = Some(110.);
    order.stop_loss = Some(90.);
    place(&mut h, 9, vec![order]);
    let main = h.take("/orders");
    let payload: Payload = serde_json::from_str(main.body.as_deref().unwrap()).unwrap();
    let client = payload.client_id.unwrap();
    assert_receipts(
        &h,
        &main,
        9,
        vec![OrderIdentityReceipt {
            symbol: "BTC-USD-PERP".into(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: client.clone(),
        }],
    );
    h.respond(
        main,
        200,
        &format!("{{\"id\":\"entry\",\"client_id\":\"{client}\"}}"),
    );
    let tp = h.take("/orders");
    let payload: Payload = serde_json::from_str(tp.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "TAKE_PROFIT_MARKET");
    assert_eq!(payload.trigger_price.as_deref(), Some("110"));
    assert_eq!(payload.flags, Some(vec!["REDUCE_ONLY".into()]));
    assert!(payload.client_id.is_none());
    assert_eq!(
        h.receipt_trace
            .iter()
            .filter(|a| matches!(a, ReceiptTrace::Progress(9, _)))
            .count(),
        1
    );
    h.respond(tp, 200, r#"{"id":"tp","client_id":""}"#);
    let sl = h.take("/orders");
    let payload: Payload = serde_json::from_str(sl.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "STOP_LOSS_MARKET");
    assert_eq!(
        h.receipt_trace
            .iter()
            .filter(|a| matches!(a, ReceiptTrace::Progress(9, _)))
            .count(),
        1
    );
    h.respond(sl, 200, r#"{"id":"sl"}"#);
    assert_eq!(h.result(9), Ok("[\"entry\",\"tp\",\"sl\"]"));
}
fn bracket_entry(h: &mut Harness, command: u32, next: Option<PlaceOrderOpts>) {
    let mut order = opts(OrderType::Limit);
    order.client_order_id = None;
    order.take_profit = Some(110.);
    order.stop_loss = Some(90.);
    place(
        h,
        command,
        [Some(order), next].into_iter().flatten().collect(),
    );
    let main = h.take("/orders");
    let payload: Payload = serde_json::from_str(main.body.as_deref().unwrap()).unwrap();
    h.respond(
        main,
        200,
        &format!(
            "{{\"id\":\"entry\",\"client_id\":\"{}\"}}",
            payload.client_id.unwrap()
        ),
    );
}
fn logged_child_rejection(h: &Harness, expected: &str) -> bool {
    h.actions.iter().any(|action| {
        matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))
        if message.contains(expected))
    })
}
#[test]
fn partial_bracket_failure_skips_the_remaining_child_and_does_not_replay_entry() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:1817-1896.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:532/separate-TP-SL
    let mut h = private();
    bracket_entry(&mut h, 10, None);
    let tp = h.take("/orders");
    h.respond(tp, 400, r#"{"error":"REJECTED"}"#);
    assert!(h.pending.is_empty());
    // The entry stands, but a refused leg leaves it unprotected: the command fails.
    assert!(h.result(10).is_err());
    assert!(logged_child_rejection(
        &h,
        "TP/SL take-profit placement failed: Paradex HTTP 400: REJECTED"
    ));
}
#[test]
fn a_rejected_stop_loss_keeps_the_take_profit_and_the_next_order() {
    // Legacy TS: paradex.exchange.ts:1817-1896,1949-1965 keeps TP and continues entries.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:532/separate-TP-SL
    let mut h = private();
    let mut next = opts(OrderType::Market);
    next.client_order_id = Some("next".into());
    bracket_entry(&mut h, 11, Some(next));
    let tp = h.take("/orders");
    let payload: Payload = serde_json::from_str(tp.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "TAKE_PROFIT_MARKET");
    h.respond(tp, 200, r#"{"id":"tp"}"#);
    let sl = h.take("/orders");
    h.respond(sl, 400, r#"{"error":"REJECTED"}"#);
    let after = h.take("/orders");
    let payload: Payload = serde_json::from_str(after.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.client_id.as_deref(), Some("next"));
    h.respond(after, 200, r#"{"id":"next-id","client_id":"next"}"#);
    assert!(h.pending.is_empty());
    assert!(h.result(11).is_err());
    assert!(logged_child_rejection(
        &h,
        "TP/SL stop-loss placement failed: Paradex HTTP 400: REJECTED"
    ));
}
#[test]
fn a_wrong_side_bracket_rejects_the_whole_command_before_any_write() {
    let mut h = private();
    for (command, side, take_profit, stop_loss, leg) in [
        (
            12,
            OrderSide::Buy,
            95.,
            90.,
            "take-profit 95 must be above the buy entry price 100",
        ),
        (
            13,
            OrderSide::Buy,
            110.,
            100.,
            "stop-loss 100 must be below the buy entry price 100",
        ),
        (
            14,
            OrderSide::Sell,
            105.,
            110.,
            "take-profit 105 must be below the sell entry price 100",
        ),
        (
            15,
            OrderSide::Sell,
            90.,
            95.,
            "stop-loss 95 must be above the sell entry price 100",
        ),
    ] {
        let mut first = opts(OrderType::Market);
        first.client_order_id = Some("first".into());
        let mut order = opts(OrderType::Limit);
        order.client_order_id = None;
        order.side = side;
        order.take_profit = Some(take_profit);
        order.stop_loss = Some(stop_loss);
        place(&mut h, command, vec![first, order]);
        assert!(h.pending.is_empty());
        let error = h.result(command).unwrap_err();
        assert!(error.contains(leg), "{error}");
    }
}

#[test]
fn placement_continues_after_a_rejection_and_retains_earlier_and_later_ids() {
    let mut h = private();
    let orders = ["first", "second", "third"].map(|id| {
        let mut order = opts(OrderType::Market);
        order.client_order_id = Some(id.into());
        order
    });
    place(&mut h, 70, orders.into());
    h.reply("/orders", r#"{"id":"one","client_id":"first"}"#);
    let rejected = h.take("/orders");
    h.respond(
        rejected,
        400,
        r#"{"error":"INSUFFICIENT_MARGIN","message":"Not enough collateral"}"#,
    );
    h.reply("/orders", r#"{"id":"three","client_id":"third"}"#);
    assert_eq!(h.result(70), Ok("[\"one\",\"three\"]"));
    assert!(h.pending.is_empty());
    assert!(h.actions.iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))
        if message.contains("second") && message.contains("Not enough collateral"))));
}

#[test]
fn a_refused_entry_skips_its_children_and_an_unknown_entry_stops_the_action() {
    for ambiguous in [false, true] {
        let mut h = private();
        let mut bracket = opts(OrderType::Market);
        bracket.client_order_id = None;
        bracket.take_profit = Some(110.);
        bracket.stop_loss = Some(90.);
        let mut next = opts(OrderType::Market);
        next.client_order_id = Some("next".into());
        place(&mut h, 71, vec![bracket, next]);
        let first = h.take("/orders");
        if ambiguous {
            h.e.handle(
                h.now,
                Event::HttpFailed {
                    request: first.id,
                    error: "response lost after possible acceptance",
                },
            );
            h.collect();
        } else {
            h.respond(
                first,
                400,
                r#"{"error":"REJECTED","message":"entry refused"}"#,
            );
        }
        if ambiguous {
            after_failure_no_more_writes(&h);
            reconcile_read_only(&mut h);
            let error = failed(&h, 71);
            assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
            assert!(error.partial.is_none());
        } else {
            let next = take_method(&mut h, Method::Post, "/orders");
            let payload: Payload = serde_json::from_str(next.body.as_deref().unwrap()).unwrap();
            assert_eq!(payload.kind, "MARKET");
            assert_eq!(payload.client_id.as_deref(), Some("next"));
            h.respond(next, 200, r#"{"id":"accepted-next","client_id":"next"}"#);
            assert_eq!(h.result(71), Ok("[\"accepted-next\"]"));
        }
        assert!(h.pending.is_empty());
    }
}

#[test]
fn a_child_leg_lost_in_transport_makes_the_command_unknown_with_accepted_ids() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:1817-1896.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:532/separate-TP-SL
    let mut h = private();
    bracket_entry(&mut h, 12, None);
    let tp = h.take("/orders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: tp.id,
            error: "response lost after possible acceptance",
        },
    );
    h.collect();
    // TS paradex.exchange.ts:1817-1896 skips SL after TP transport loss.
    reconcile_read_only(&mut h);
    let error = failed(&h, 12);
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    assert_eq!(error.partial.unwrap().order_ids, ["entry"]);
}

/// After an Unknown write the only pending request is the fence's order read.
fn reconcile_read_only(h: &mut Harness) {
    let read = h.take("/orders");
    assert_eq!(read.method, Method::Get);
    assert!(h.pending.is_empty(), "{:?}", h.pending);
}

fn failed(h: &Harness, id: u32) -> safe_cex_engine::VenueError {
    let error = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::Notice(safe_cex_engine::Notice::CommandFailed { command, error })
                if *command == id =>
            {
                Some(error.clone())
            }
            _ => None,
        })
        .expect("command failed");
    if error.outcome == Some(safe_cex_engine::Outcome::Unknown) {
        assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
    }
    error
}

#[test]
fn a_mixed_native_batch_fails_with_correlated_successes_and_missing_clients() {
    let mut h = private();
    let orders = ["first", "second", "third"].map(|id| {
        let mut order = opts(OrderType::Limit);
        order.client_order_id = Some(id.into());
        order
    });
    place(&mut h, 72, orders.into());
    h.reply("/orders/batch", r#"{"orders":[{"id":"three","client_id":"third"},{"id":"one","client_id":"first"}],"errors":[{"error":"INVALID_PRICE","message":"outside price bands"}]}"#);
    assert_batch_refusal(
        &h,
        72,
        vec!["one".into(), "three".into()],
        "outside price bands",
    );
    assert!(h.actions.iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))
        if message.contains("second") && message.contains("outside price bands"))));
}

fn rejection_level(h: &Harness) -> safe_cex_engine::Level {
    h.actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.message.contains("placement failed for client IDs") =>
            {
                Some(event.level)
            }
            _ => None,
        })
        .expect("the rejection is logged")
}

#[test]
fn a_lone_rejected_entry_leaves_its_toast_to_the_failed_command() {
    let mut h = private();
    place(&mut h, 74, vec![opts(OrderType::Market)]);
    let only = h.take("/orders");
    h.respond(
        only,
        400,
        r#"{"error":"REJECTED","message":"entry refused"}"#,
    );
    assert!(h.result(74).unwrap_err().contains("entry refused"));
    assert_eq!(rejection_level(&h), safe_cex_engine::Level::Warn);
}

#[test]
fn a_rejected_last_entry_beside_an_accepted_one_still_reports_itself() {
    let mut h = private();
    let mut second = opts(OrderType::Market);
    second.client_order_id = Some("second".into());
    place(&mut h, 75, vec![opts(OrderType::Market), second]);
    let first = h.take("/orders");
    let accepted: Payload = serde_json::from_str(first.body.as_deref().unwrap()).unwrap();
    let body = format!(
        r#"{{"id":"one","client_id":"{}"}}"#,
        accepted.client_id.unwrap()
    );
    h.respond(first, 200, &body);
    let second = h.take("/orders");
    h.respond(
        second,
        400,
        r#"{"error":"REJECTED","message":"second refused"}"#,
    );
    assert_eq!(h.result(75), Ok("[\"one\"]"));
    assert_eq!(rejection_level(&h), safe_cex_engine::Level::Error);
}

#[test]
fn all_failed_placements_still_attempt_every_independent_order_and_return_an_error() {
    let mut h = private();
    let mut second = opts(OrderType::Market);
    second.client_order_id = Some("second".into());
    place(&mut h, 73, vec![opts(OrderType::Market), second]);
    let first = h.take("/orders");
    h.respond(
        first,
        200,
        r#"{"error":"FIRST_REJECTED","message":"first reason"}"#,
    );
    let second = h.take("/orders");
    h.respond(
        second,
        400,
        r#"{"error":"SECOND_REJECTED","message":"second reason"}"#,
    );
    let error = h.result(73).unwrap_err();
    assert!(error.contains("first reason") && error.contains("second reason"));
    assert!(h.pending.is_empty());
}

#[test]
fn leverage_is_serialized_as_a_json_integer_after_flooring_and_clamping() {
    #[derive(Deserialize)]
    struct IntegerLeverage {
        leverage: u32,
    }
    for (input, expected) in [(20., 20), (20.9, 20), (100., 50), (0., 1)] {
        let mut h = private();
        h.command(Command::SetLeverage {
            command: 74,
            symbol: "BTC-USD-PERP".into(),
            leverage: input,
            side: None,
        });
        let request = h.take("/account/margin/");
        let body: IntegerLeverage = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(body.leverage, expected);
        h.respond(request, 200, "{}");
        assert_eq!(
            h.e.store().setting("BTC-USD-PERP").unwrap().leverage,
            f64::from(expected)
        );
    }
}

#[test]
fn a_rejected_leverage_write_with_mistyped_message_never_commits_requested_settings() {
    let mut h = private();
    let before = h.e.store().setting("BTC-USD-PERP").unwrap().leverage;
    h.command(Command::SetLeverage {
        command: 75,
        symbol: "BTC-USD-PERP".into(),
        leverage: 20.,
        side: None,
    });
    h.reply(
        "/account/margin/",
        r#"{"error":"REJECTED","message":{"detail":"denied"}}"#,
    );
    assert!(h.result(75).unwrap_err().contains("REJECTED"));
    assert_eq!(
        h.e.store().setting("BTC-USD-PERP").unwrap().leverage,
        before
    );
}
#[test]
fn fetch_leverage_returns_two_fetched_leverage_rails_and_updates_settings() {
    let mut h = private();
    h.command(Command::FetchLeverage {
        command: 11,
        symbol: "BTC-USD-PERP".into(),
    });
    h.reply(
        "/account/margin",
        r#"{"configs":[{"market":"BTC-USD-PERP","leverage":25}]}"#,
    );
    assert_eq!(
        h.result(11),
        Ok("{\"buyLeverage\":25.0,\"sellLeverage\":25.0}")
    );
    assert_eq!(h.e.store().setting("BTC-USD-PERP").unwrap().leverage, 25.);
}
#[test]
fn cancel_pair_errors_are_correlated_and_attempt_all_batches_without_resending() {
    let mut h = private();
    h.command(Command::CancelOrders {
        command: 12,
        ids: vec!["a".into(), "b".into(), "c".into()],
    });
    let pair = h.take("/orders/batch");
    assert_eq!(pair.method, Method::Delete);
    h.respond(
        pair,
        200,
        r#"{"results":[{"id":"a","status":"NOT_FOUND"},{"id":"b","status":"ALREADY_CLOSED"}]}"#,
    );
    let single = h.take("/orders/c");
    h.respond(single, 204, "");
    assert!(h.result(12).is_err());
    assert!(h.pending.is_empty());
}
#[test]
fn cancel_all_also_deletes_untriggered_stops_by_id() {
    let mut h = private();
    let private = h
        .actions
        .iter()
        .find_map(|a| match a {
            safe_cex_engine::Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: private });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket: private,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    h.e.handle(h.now,Event::WsMessage{socket:private,text:r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"STOP_LOSS_MARKET","status":"UNTRIGGERED","side":"SELL","size":"2","remaining_size":"2","price":"95","trigger_price":"90","flags":["REDUCE_ONLY"],"instruction":"GTC","created_at":1700000000000}}}"#});
    h.collect();
    // Live: DELETE /orders?market= queued, the stop still there 8 minutes later.
    h.command(Command::CancelSymbolOrders {
        command: 12,
        symbol: "BTC-USD-PERP".into(),
    });
    let stop = take_method(&mut h, Method::Delete, "/orders/123");
    // Live: a stop already gone answers this, and the nuke read as failed.
    h.respond(
        stop,
        400,
        r#"{"error":"ORDER_ID_NOT_FOUND","message":"order not found"}"#,
    );
    let all = take_method(&mut h, Method::Delete, "/orders?market=");
    h.respond(all, 200, r#"{"message":"cancel requests queued"}"#);
    assert!(h.result(12).is_ok());
}

/// The untriggered SELL stop of `with_untriggered_stop`, edited to trigger at 91.
fn amend_stop(h: &mut Harness, command: u32) {
    h.command(Command::AmendOrder {
        command,
        id: "123".into(),
        price: Some(91.),
        amount: None,
    });
}
fn order_posts(h: &Harness) -> usize {
    h.pending
        .iter()
        .filter(|r| r.method == Method::Post && r.url.ends_with("/orders"))
        .count()
}

#[test]
fn a_refused_stop_replacement_puts_the_exact_original_back_once() {
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let place = h.take("/orders");
    h.respond(place, 400, r#"{"error":"VALIDATION","message":"refused"}"#);
    let restore = h.take("/orders");
    let payload: Payload = serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "STOP_LOSS_MARKET");
    assert_eq!(payload.trigger_price.as_deref(), Some("90"));
    assert_eq!(payload.size.as_deref(), Some("2"));
    assert_eq!(payload.price, None, "a market stop carries no price");
    assert_eq!(payload.instruction.as_deref(), Some("GTC"));
    assert_eq!(payload.flags, Some(vec!["REDUCE_ONLY".to_owned()]));
    assert!(payload.signature.is_some_and(|s| !s.is_empty()));
    let body = format!(
        r#"{{"id":"124","client_id":"{}"}}"#,
        payload.client_id.unwrap()
    );
    h.respond(restore, 201, &body);
    let error = h.result(13).unwrap_err();
    assert!(
        error.contains("stop is unchanged") && error.contains("put back"),
        "{error}"
    );
    assert_eq!(
        h.failures.last().unwrap().protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
    assert_eq!(order_posts(&h), 0);
}

#[test]
fn a_refused_restore_is_visibly_unprotected_and_never_retried() {
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    for _ in 0..2 {
        let post = h.take("/orders");
        h.respond(post, 400, r#"{"error":"VALIDATION","message":"refused"}"#);
    }
    let error = h.result(13).unwrap_err();
    assert!(error.contains("no stop"), "{error}");
    assert_eq!(
        h.failures.last().unwrap().protection,
        Some(safe_cex_engine::error::Protection::Unprotected)
    );
    assert_eq!(order_posts(&h), 0, "one restore, never a retry");
}

/// The ambiguous leg may have placed protection; an earlier refusal remains true.
fn assert_unknown_stop(h: &Harness, command: u32) {
    let error = h.result(command).unwrap_err();
    assert!(
        error.contains("outcome is unknown. Check Open Orders"),
        "{error}"
    );
    let words = error.to_ascii_lowercase();
    assert!(!words.contains("unprotected"), "{error}");
    assert_eq!(h.failures.last().unwrap().protection, None);
}

fn assert_unknown_replacement(h: &Harness, command: u32) {
    assert_unknown_stop(h, command);
    let error = h.result(command).unwrap_err();
    assert!(!error.contains("refused"), "{error}");
}

fn assert_unknown_put_back(h: &Harness, command: u32) {
    assert_unknown_stop(h, command);
    let error = h.result(command).unwrap_err();
    let words = error.to_ascii_lowercase();
    assert!(
        !words.contains("no stop") || words.contains("may have no stop"),
        "{error}"
    );
}

#[test]
fn a_rate_limited_restore_is_unknown_not_unprotected() {
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let place = h.take("/orders");
    h.respond(place, 400, r#"{"error":"VALIDATION","message":"refused"}"#);
    let restore = h.take("/orders");
    h.respond(
        restore,
        429,
        r#"{"error":"RATE_LIMIT","message":"slow down"}"#,
    );
    assert_unknown_put_back(&h, 13);
    assert_eq!(order_posts(&h), 0, "one restore, never a retry");
}

#[test]
fn a_rate_limited_replacement_is_unknown_and_never_put_back() {
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let place = h.take("/orders");
    h.respond(
        place,
        429,
        r#"{"error":"RATE_LIMIT","message":"slow down"}"#,
    );
    assert_eq!(
        order_posts(&h),
        0,
        "a maybe-placed replacement is never doubled"
    );
    assert_unknown_replacement(&h, 13);
}

#[test]
fn a_stale_crossed_or_changed_stop_at_the_final_send_cancels_nothing() {
    for case in ["stale", "crossed", "changed"] {
        let mut h = with_untriggered_stop();
        // The JWT is due: the cancel waits on its refresh before it can leave.
        h.now += 181_000.;
        // The prices move (so their freshness is stamped) at the new time.
        h.e.handle(h.now, Event::WsOpened { socket: h.public });
        h.collect();
        h.e.store_mut().update_ticker("BTC-USD-PERP", |t| {
            t.mark = 100.6;
            t.last = 100.1;
        });
        amend_stop(&mut h, 13);
        assert!(!h.pending.iter().any(|r| r.method == Method::Delete));
        match case {
            "crossed" => h.e.store_mut().update_ticker("BTC-USD-PERP", |t| {
                t.mark = 90.5;
                t.last = 90.5;
            }),
            "changed" => {
                // Edited elsewhere while the cancel waited.
                let private = h
                    .actions
                    .iter()
                    .find_map(|a| match a {
                        safe_cex_engine::Action::WsConnect {
                            socket,
                            channel: Some(safe_cex_engine::SocketChannel::Private),
                            ..
                        } => Some(*socket),
                        _ => None,
                    })
                    .unwrap();
                h.e.handle(h.now,Event::WsMessage{socket:private,text:r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"STOP_LOSS_MARKET","status":"UNTRIGGERED","side":"SELL","size":"2","remaining_size":"2","price":"95","trigger_price":"88","flags":["REDUCE_ONLY"],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000999000}}}"#});
                h.collect();
            }
            _ => h.now += 6_000.,
        }
        h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
        assert!(
            !h.pending.iter().any(|r| r.method == Method::Delete),
            "{case}: {:?}",
            h.pending.iter().map(|r| &r.url).collect::<Vec<_>>()
        );
        let error = h.result(13).unwrap_err();
        assert!(error.contains("stop was not changed"), "{error}");
        assert_eq!(
            h.failures.last().unwrap().outcome,
            Some(safe_cex_engine::Outcome::NotSent)
        );
        assert_eq!(
            h.e.store().order("123").unwrap().status,
            safe_cex_engine::model::OrderStatus::Open
        );
    }
}

/// An untriggered stop ENTRY (not reduce-only): Paradex maps it to Conditional.
fn with_stop_entry(side: &str, kind: &str, trigger: &str, price: &str) -> Harness {
    let mut h = private();
    let private = h
        .actions
        .iter()
        .find_map(|a| match a {
            safe_cex_engine::Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: private });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket: private,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    let text = format!(
        r#"{{"jsonrpc":"2.0","method":"subscription","params":{{"channel":"orders.ALL","data":{{"id":"321","market":"BTC-USD-PERP","type":"{kind}","status":"UNTRIGGERED","side":"{side}","size":"1","remaining_size":"1","price":"{price}","trigger_price":"{trigger}","flags":[],"instruction":"GTC","stp":"EXPIRE_MAKER","created_at":1700000000000}}}}}}"#
    );
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket: private,
            text: &text,
        },
    );
    h.collect();
    assert_eq!(
        h.e.store().order("321").unwrap().kind,
        OrderType::Conditional
    );
    h
}

#[test]
fn a_stop_its_signer_could_not_put_back_is_refused_before_its_cancel() {
    // A reduce-only stop limit is a shape the Stark signer does not admit.
    let mut h = with_untriggered_stop();
    let private = h
        .actions
        .iter()
        .find_map(|a| match a {
            safe_cex_engine::Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now,Event::WsMessage{socket:private,text:r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"STOP_LOSS_LIMIT","status":"UNTRIGGERED","side":"SELL","size":"2","remaining_size":"2","price":"89","trigger_price":"90","flags":["REDUCE_ONLY"],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000001000}}}"#});
    h.collect();
    amend_stop(&mut h, 13);
    assert!(!h.pending.iter().any(|r| r.method == Method::Delete));
    assert!(h.result(13).unwrap_err().contains("not changed"));
}

#[test]
fn a_stop_entry_edit_checks_the_entry_direction() {
    // mark 100.5, last 100: a buy entry fires above the market, a sell below it.
    for (side, from, to, safe) in [
        ("BUY", "110", 112., true),
        ("SELL", "90", 88., true),
        ("BUY", "110", 99., false),
        ("SELL", "90", 101., false),
    ] {
        let price = if side == "BUY" { "111" } else { "89" };
        let mut h = with_stop_entry(side, "STOP_LIMIT", from, price);
        h.command(Command::AmendOrder {
            command: 13,
            id: "321".into(),
            price: Some(to),
            amount: None,
        });
        let cancelled = h.pending.iter().any(|r| r.method == Method::Delete);
        assert_eq!(cancelled, safe, "{side} {from}->{to}");
        if !safe {
            let error = h.result(13).unwrap_err().to_owned();
            assert!(
                error.contains("not changed") || error.contains("at once"),
                "{error}"
            );
        }
    }
}

#[test]
fn a_refused_stop_entry_replacement_puts_back_the_entry_exactly() {
    let mut h = with_stop_entry("BUY", "STOP_LIMIT", "110", "111");
    h.command(Command::AmendOrder {
        command: 13,
        id: "321".into(),
        price: Some(112.),
        amount: None,
    });
    let cancel = take_method(&mut h, Method::Delete, "/orders/321");
    h.respond(cancel, 204, "");
    confirm_cancel_row(
        &mut h,
        r#"{"id":"321","market":"BTC-USD-PERP","type":"STOP_LIMIT","status":"CLOSED","cancel_reason":"USER_CANCELED","side":"BUY","size":"1","remaining_size":"1","price":"111","trigger_price":"110","flags":[],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000000100}"#,
    );
    let place = h.take("/orders");
    h.respond(place, 400, r#"{"error":"VALIDATION","message":"refused"}"#);
    let restore = h.take("/orders");
    let body: serde_json::Value = serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
    assert_eq!(body["type"], "STOP_LIMIT");
    assert_eq!(body["trigger_price"], "110");
    assert_eq!(body["price"], "111");
    assert_eq!(body["stp"], "EXPIRE_TAKER", "the one mode the signer signs");
    assert!(body.get("flags").is_none(), "an entry is not reduce-only");
}

#[test]
fn a_stale_crossed_or_changed_stop_entry_at_the_final_send_cancels_nothing() {
    for case in ["stale", "crossed", "changed"] {
        // A BUY stop entry at 110 (limit 111), moved to 112.
        let mut h = with_stop_entry("BUY", "STOP_LIMIT", "110", "111");
        h.now += 181_000.;
        h.e.handle(h.now, Event::WsOpened { socket: h.public });
        h.collect();
        h.e.store_mut().update_ticker("BTC-USD-PERP", |t| {
            t.mark = 100.6;
            t.last = 100.1;
        });
        h.command(Command::AmendOrder {
            command: 13,
            id: "321".into(),
            price: Some(112.),
            amount: None,
        });
        assert!(
            !h.pending.iter().any(|r| r.method == Method::Delete),
            "{case}"
        );
        match case {
            // The market rose through 112 while the cancel waited: it would fire at once.
            "crossed" => h.e.store_mut().update_ticker("BTC-USD-PERP", |t| {
                t.mark = 112.5;
                t.last = 112.5;
            }),
            "changed" => {
                let private = h
                    .actions
                    .iter()
                    .find_map(|a| match a {
                        safe_cex_engine::Action::WsConnect {
                            socket,
                            channel: Some(safe_cex_engine::SocketChannel::Private),
                            ..
                        } => Some(*socket),
                        _ => None,
                    })
                    .unwrap();
                h.e.handle(h.now,Event::WsMessage{socket:private,text:r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"321","market":"BTC-USD-PERP","type":"STOP_LIMIT","status":"UNTRIGGERED","side":"BUY","size":"1","remaining_size":"1","price":"111","trigger_price":"109","flags":[],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000999000}}}"#});
                h.collect();
            }
            _ => h.now += 6_000.,
        }
        h.reply("/auth/", r#"{"jwt_token":"jwt"}"#);
        assert!(
            !h.pending.iter().any(|r| r.method == Method::Delete),
            "{case}: {:?}",
            h.pending.iter().map(|r| &r.url).collect::<Vec<_>>()
        );
        assert!(h.result(13).unwrap_err().contains("not changed"), "{case}");
    }
}

#[test]
fn a_crossed_reference_at_planning_or_an_already_gone_stop_never_restores() {
    let mut h = with_untriggered_stop();
    // A SELL stop at 101 is above the 100 last price: it would fire at once.
    h.command(Command::AmendOrder {
        command: 13,
        id: "123".into(),
        price: Some(101.),
        amount: None,
    });
    assert!(!h.pending.iter().any(|r| r.method == Method::Delete));
    let planning = h.result(13).unwrap_err().to_owned();
    assert!(planning.contains("would trigger at once"), "{planning}");
    // The venue no longer holds the stop (it may have fired): never put it back.
    amend_stop(&mut h, 14);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(
        cancel,
        400,
        r#"{"error":"ORDER_ID_NOT_FOUND","message":"order not found"}"#,
    );
    assert_eq!(order_posts(&h), 0);
    expire_cancel_proof(&mut h);
    assert!(
        h.result(14)
            .unwrap_err()
            .contains("outcome of your BTC-USD-PERP protection update is unknown")
    );
}

#[test]
fn a_cancel_during_the_replacement_is_never_undone_by_a_put_back() {
    // Cancelled before the refusal: the put-back is dropped.
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let place = h.take("/orders");
    h.command(Command::CancelOrders {
        command: 14,
        ids: vec!["123".into()],
    });
    let user = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(
        user,
        400,
        r#"{"error":"ORDER_ID_NOT_FOUND","message":"order not found"}"#,
    );
    h.respond(place, 400, r#"{"error":"VALIDATION","message":"refused"}"#);
    assert_eq!(order_posts(&h), 0, "the cancelled stop is not put back");
    assert!(h.result(13).unwrap_err().contains("no stop"));

    // Cancelled after the put-back: the cancel follows it to its new id.
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let place = h.take("/orders");
    h.respond(place, 400, r#"{"error":"VALIDATION","message":"refused"}"#);
    let restore = h.take("/orders");
    let payload: Payload = serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
    let body = format!(
        r#"{{"id":"124","client_id":"{}"}}"#,
        payload.client_id.unwrap()
    );
    h.respond(restore, 201, &body);
    h.command(Command::CancelOrders {
        command: 15,
        ids: vec!["123".into()],
    });
    let cancel = h.take("/orders/batch");
    let body = cancel.body.as_deref().unwrap();
    assert!(
        body.contains("\"123\"") && body.contains("\"124\""),
        "{body}"
    );
}

#[test]
fn a_second_amend_of_the_replaced_stop_is_refused() {
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let place = h.take("/orders");
    h.command(Command::AmendOrder {
        command: 14,
        id: "123".into(),
        price: Some(92.),
        amount: None,
    });
    assert!(h.result(14).is_err());
    h.respond(place, 400, r#"{"error":"VALIDATION","message":"refused"}"#);
    assert_eq!(order_posts(&h), 1, "one put-back only");
}

#[test]
fn an_amount_only_edit_below_the_minimum_sends_no_cancel() {
    let mut h = with_untriggered_stop();
    h.command(Command::AmendOrder {
        command: 13,
        id: "123".into(),
        price: None,
        // Below the 0.00001 size increment: nothing valid is left to place.
        amount: Some(0.000001),
    });
    assert!(!h.pending.iter().any(|r| r.method == Method::Delete));
    assert!(h.result(13).is_err());
}

#[test]
fn an_untriggered_stop_edit_cancels_and_replaces() {
    let mut h = private();
    let private = h
        .actions
        .iter()
        .find_map(|a| match a {
            safe_cex_engine::Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: private });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket: private,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    h.e.handle(h.now,Event::WsMessage{socket:private,text:r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"STOP_LOSS_MARKET","status":"UNTRIGGERED","side":"SELL","size":"2","remaining_size":"2","price":"","trigger_price":"90","flags":["REDUCE_ONLY"],"instruction":"GTC","created_at":1700000000000}}}"#});
    h.collect();
    h.command(Command::AmendOrder {
        command: 13,
        id: "123".into(),
        price: Some(91.),
        amount: Some(3.),
    });
    // Live: editing an untriggered stop answers ORDER_IS_NOT_OPEN.
    assert!(!h.pending.iter().any(|r| r.method == Method::Put));
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let place = h.take("/orders");
    let payload: Payload = serde_json::from_str(place.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.trigger_price.as_deref(), Some("91"));
    assert_eq!(payload.size.as_deref(), Some("3"));
}

#[test]
fn a_single_native_eligible_limit_uses_the_batch_endpoint_once() {
    let mut h = private();
    place(&mut h, 14, vec![opts(OrderType::Limit)]);
    let request = h.take("/orders/batch");
    let payload: Vec<Payload> = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.len(), 1);
    assert_eq!(payload[0].client_id.as_deref(), Some("caller"));
    h.respond(
        request,
        200,
        r#"{"orders":[{"id":"one","client_id":"caller"}]}"#,
    );
    assert_eq!(h.result(14), Ok("[\"one\"]"));
}

#[test]
fn cancel_all_rejects_an_empty_or_unknown_market_before_sending_a_delete() {
    for symbol in ["", "UNKNOWN-USD-PERP"] {
        let mut h = private();
        h.command(Command::CancelSymbolOrders {
            command: 15,
            symbol: symbol.into(),
        });
        assert!(h.pending.is_empty());
        assert!(h.result(15).is_err());
    }
}
#[test]
fn accepted_amend_fences_reads_started_in_flight_and_requires_fresh_reconciliation() {
    let mut h = private();
    let socket = h
        .actions
        .iter()
        .find_map(|a| match a {
            safe_cex_engine::Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    const ROW: &str = r#"{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"BUY","size":"2","remaining_size":"2","price":"100","flags":[],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000000000}"#;
    let message = format!(
        "{{\"jsonrpc\":\"2.0\",\"method\":\"subscription\",\"params\":{{\"channel\":\"orders.ALL\",\"data\":{ROW}}}}}"
    );
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: &message,
        },
    );
    h.collect();
    h.command(Command::AmendOrder {
        command: 16,
        id: "123".into(),
        price: Some(101.),
        amount: None,
    });
    let amend = h.take("/orders/123");
    h.command(Command::AmendOrder {
        command: 17,
        id: "123".into(),
        price: None,
        amount: Some(3.),
    });
    assert!(h.result(17).is_err());
    assert!(h.pending.is_empty());
    h.timer(h.now + 1_000.);
    let snapshot = h.take("/orders");
    h.respond(amend, 200, r#"{"id":"123"}"#);
    h.respond(snapshot, 200, &format!("{{\"results\":[{ROW}]}}"));
    h.command(Command::AmendOrder {
        command: 18,
        id: "123".into(),
        price: None,
        amount: Some(3.),
    });
    assert!(h.result(18).is_err());
    assert!(h.pending.is_empty());
    let updated = message
        .replace("\"price\":\"100\"", "\"price\":\"101\"")
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000001000",
        );
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: &updated,
        },
    );
    h.collect();
    h.command(Command::AmendOrder {
        command: 19,
        id: "123".into(),
        price: None,
        amount: Some(3.),
    });
    let request = h.take("/orders/123");
    let payload: Payload = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.price.as_deref(), Some("101"));
    assert_eq!(payload.size.as_deref(), Some("3"));
}

const AMEND_ROW: &str = r#"{"id":"123","market":"BTC-USD-PERP","type":"LIMIT","status":"OPEN","side":"BUY","size":"2","remaining_size":"2","price":"99","flags":[],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000000000}"#;
fn amendment_account(row: &str) -> (Harness, safe_cex_engine::SocketId) {
    let mut h = private();
    let socket = h
        .actions
        .iter()
        .find_map(|a| match a {
            safe_cex_engine::Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    amendment_update(&mut h, socket, row);
    (h, socket)
}
fn amendment_update(h: &mut Harness, socket: safe_cex_engine::SocketId, row: &str) {
    let text = format!(
        r#"{{"jsonrpc":"2.0","method":"subscription","params":{{"channel":"orders.ALL","data":{row}}}}}"#
    );
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    h.collect();
}
fn amend(h: &mut Harness, command: u32, price: Option<f64>, amount: Option<f64>) {
    h.command(Command::AmendOrder {
        command,
        id: "123".into(),
        price,
        amount,
    });
}

#[test]
fn post_ack_repair_cannot_restore_old_amendment_attributes_even_after_a_same_stamp_match() {
    let (mut h, socket) = amendment_account(AMEND_ROW);
    h.now += 500.; // Target matches remain valid even behind host completion time.
    amend(&mut h, 101, Some(101.), None);
    let request = h.take("/orders/123");
    h.respond(request, 200, r#"{"id":"123"}"#);
    assert_eq!(h.result(101), Ok("[\"123\"]"));
    // Begin the GET strictly after the successful write response.
    h.timer(h.now + 500.);
    h.reply("/orders", &format!(r#"{{"results":[{AMEND_ROW}]}}"#));
    amend(&mut h, 102, None, Some(3.));
    assert!(h.result(102).is_err());
    assert!(h.pending.is_empty());
    // The venue can echo changed attributes with the same server timestamp.
    let matching = AMEND_ROW.replace("\"price\":\"99\"", "\"price\":\"101\"");
    amendment_update(&mut h, socket, &matching);
    amendment_update(&mut h, socket, AMEND_ROW);
    assert_eq!(h.e.store().order("123").unwrap().price, 101.);
    amend(&mut h, 103, None, Some(3.));
    let next = h.take("/orders/123");
    let payload: Payload = serde_json::from_str(next.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.price.as_deref(), Some("101"));
    assert_eq!(payload.size.as_deref(), Some("3"));
}

#[test]
fn post_ack_size_changes_require_all_exact_mutable_attributes() {
    {
        let trigger = false;
        let row = if trigger {
            AMEND_ROW
                .replace("\"type\":\"LIMIT\"", "\"type\":\"STOP_LIMIT\"")
                .replace("\"status\":\"OPEN\"", "\"status\":\"UNTRIGGERED\"")
                .replace(
                    "\"price\":\"99\"",
                    "\"price\":\"99\",\"trigger_price\":\"110\"",
                )
        } else {
            AMEND_ROW.into()
        };
        let (mut h, socket) = amendment_account(&row);
        amend(&mut h, 110, trigger.then_some(120.), Some(3.));
        let request = h.take("/orders/123");
        h.respond(request, 200, r#"{"id":"123"}"#);
        amendment_update(&mut h, socket, &row);
        amend(&mut h, 111, None, Some(4.));
        assert!(h.result(111).is_err());
        let changed = row
            .replace("\"size\":\"2\"", "\"size\":\"3\"")
            .replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"3\"");
        if trigger {
            amendment_update(&mut h, socket, &changed);
            amend(&mut h, 112, None, Some(4.));
            assert!(h.result(112).is_err());
        }
        let changed = if trigger {
            changed.replace("\"trigger_price\":\"110\"", "\"trigger_price\":\"120\"")
        } else {
            changed
        };
        amendment_update(&mut h, socket, &changed);
        amend(&mut h, 113, None, Some(4.));
        let request = h.take("/orders/123");
        let payload: Payload = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.price.as_deref(), Some("99"));
        assert_eq!(payload.trigger_price.as_deref(), trigger.then_some("120"));
    }
}

#[test]
fn fills_while_amending_do_not_unlock_old_attributes_and_matching_smaller_sizes_remain_valid() {
    let (mut h, socket) = amendment_account(AMEND_ROW);
    h.now += 5_000.;
    amend(&mut h, 120, Some(101.), Some(1.5));
    let request = h.take("/orders/123");
    let fill = AMEND_ROW
        .replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"1\"")
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000006000",
        );
    amendment_update(&mut h, socket, &fill);
    assert_eq!(h.e.store().order("123").unwrap().filled, 1.);
    h.respond(request, 200, r#"{"id":"123"}"#);
    amendment_update(&mut h, socket, &fill);
    amend(&mut h, 121, None, Some(3.));
    assert!(h.result(121).is_err());
    let matching = fill
        .replace("\"size\":\"2\"", "\"size\":\"1.5\"")
        .replace("\"remaining_size\":\"1\"", "\"remaining_size\":\"0.5\"")
        .replace("\"price\":\"99\"", "\"price\":\"101\"");
    amendment_update(&mut h, socket, &matching);
    amend(&mut h, 122, None, Some(3.));
    let request = h.take("/orders/123");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains("\"price\":\"101\"")
    );
}

#[test]
fn accepted_amendments_can_reconcile_a_strictly_newer_coherent_external_edit() {
    let (mut h, socket) = amendment_account(AMEND_ROW);
    h.now += 5_000.;
    amend(&mut h, 130, Some(101.), None);
    let request = h.take("/orders/123");
    h.respond(request, 200, r#"{"id":"123"}"#);
    let external = AMEND_ROW
        .replace("\"price\":\"99\"", "\"price\":\"105\"")
        .replace(
            "\"last_updated_at\":1700000000000",
            "\"last_updated_at\":1700000006000",
        );
    amendment_update(&mut h, socket, &external);
    amendment_update(&mut h, socket, AMEND_ROW);
    amend(&mut h, 131, None, Some(3.));
    let request = h.take("/orders/123");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains("\"price\":\"105\"")
    );
}

#[test]
fn definitive_amend_rejection_allows_unchanged_reconciliation_but_transport_failure_stays_guarded()
{
    for outcome in ["400", "business", "lost", "500", "bad-id", "408", "429"] {
        let ambiguous = !matches!(outcome, "400" | "business");
        let (mut h, socket) = amendment_account(AMEND_ROW);
        amend(&mut h, 140, Some(101.), None);
        let request = h.take("/orders/123");
        match outcome {
            "lost" => {
                h.e.handle(
                    h.now,
                    Event::HttpFailed {
                        request: request.id,
                        error: "response lost",
                    },
                );
                h.collect();
            }
            "business" => h.respond(request, 200, r#"{"error":"INVALID_ORDER"}"#),
            "bad-id" => h.respond(request, 200, r#"{"id":"different-order"}"#),
            _ => h.respond(
                request,
                outcome.parse().unwrap(),
                r#"{"error":"INVALID_ORDER"}"#,
            ),
        }
        amendment_update(&mut h, socket, AMEND_ROW);
        amend(&mut h, 141, None, Some(3.));
        if ambiguous {
            assert!(h.result(141).is_err());
            assert!(h.pending.iter().all(|r| r.method == Method::Get));
            // A complete read of the unchanged row releases the engine's
            // placement fence; the venue's amend fence still needs evidence.
            if let Some(i) = h.pending.iter().position(|r| r.url.contains("/orders")) {
                let read = h.pending.remove(i);
                h.respond(read, 200, &format!(r#"{{"results":[{AMEND_ROW}]}}"#));
            }
            assert!(h.pending.is_empty());
            amend(&mut h, 143, None, Some(3.));
            assert!(h.result(143).is_err());
            assert!(h.pending.is_empty());
            let matching = AMEND_ROW.replace("\"price\":\"99\"", "\"price\":\"101\"");
            amendment_update(&mut h, socket, &matching);
            amend(&mut h, 142, None, Some(3.));
        }
        let request = h.take("/orders/123");
        let payload: Payload = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
        assert_eq!(
            payload.price.as_deref(),
            Some(if ambiguous { "101" } else { "99" })
        );
    }
}

#[test]
fn a_rejected_followup_amend_restores_the_previous_settled_guard() {
    let (mut h, socket) = amendment_account(AMEND_ROW);
    amend(&mut h, 150, Some(101.), None);
    let request = h.take("/orders/123");
    h.respond(request, 200, r#"{"id":"123"}"#);
    let matching = AMEND_ROW.replace("\"price\":\"99\"", "\"price\":\"101\"");
    amendment_update(&mut h, socket, &matching);
    amend(&mut h, 151, Some(102.), None);
    let request = h.take("/orders/123");
    h.respond(request, 400, r#"{"error":"INVALID_ORDER"}"#);
    amendment_update(&mut h, socket, AMEND_ROW);
    amend(&mut h, 152, None, Some(3.));
    assert!(h.result(152).is_err());
    amendment_update(&mut h, socket, &matching);
    amend(&mut h, 153, None, Some(3.));
    let request = h.take("/orders/123");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains("\"price\":\"101\"")
    );
}

#[test]
fn a_full_matching_modify_response_is_reconciled_but_terminal_events_close_the_order() {
    let (mut h, socket) = amendment_account(AMEND_ROW);
    amend(&mut h, 160, Some(101.), None);
    let request = h.take("/orders/123");
    let matching = AMEND_ROW.replace("\"price\":\"99\"", "\"price\":\"101\"");
    h.respond(request, 200, &matching);
    assert_eq!(h.e.store().order("123").unwrap().price, 101.);
    amend(&mut h, 161, None, Some(3.));
    let request = h.take("/orders/123");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains("\"price\":\"101\"")
    );
    h.respond(request, 200, r#"{"id":"123"}"#);
    // The terminal event can still describe the last executable price.
    let terminal = matching.replace("\"status\":\"OPEN\"", "\"status\":\"CLOSED\"");
    amendment_update(&mut h, socket, &terminal);
    assert_eq!(
        h.e.store().order("123").unwrap().status,
        safe_cex_engine::model::OrderStatus::Canceled
    );
    amendment_update(&mut h, socket, &matching);
    amend(&mut h, 162, None, Some(4.));
    assert!(h.result(162).is_err());
    assert!(h.pending.is_empty());
}

#[test]
fn an_amend_that_never_left_the_jwt_queue_does_not_keep_an_uncertain_target() {
    let (mut h, socket) = amendment_account(AMEND_ROW);
    h.now += 181_000.;
    amend(&mut h, 170, Some(101.), None);
    let auth = h.take("/auth/");
    assert!(!h.pending.iter().any(|r| r.method == Method::Put));
    h.respond(auth, 503, r#"{"error":"UNAVAILABLE"}"#);
    assert!(h.result(170).is_err());
    amendment_update(&mut h, socket, AMEND_ROW);
    amend(&mut h, 171, None, Some(3.));
    h.reply("/auth/", r#"{"jwt_token":"fresh"}"#);
    let request = h.take("/orders/123");
    assert!(
        request
            .body
            .as_deref()
            .unwrap()
            .contains("\"price\":\"99\"")
    );
}

#[test]
fn unsupported_bybit_controls_reject_the_whole_batch_before_quotes_or_writes() {
    for bbo in [false, true] {
        for level in [1., f64::NAN, f64::INFINITY] {
            let mut h = private();
            let first = opts(OrderType::Limit);
            let mut unsupported = opts(OrderType::Market);
            unsupported.client_order_id = Some("unsupported-control".into());
            unsupported.max_slippage_pct = Some(0.1);
            if bbo {
                unsupported.bybit_bbo = Some(safe_cex_engine::model::BybitBbo {
                    side_type: "Counterparty".into(),
                    level,
                });
            } else {
                unsupported.bybit_slippage_tolerance_ticks = Some(level);
            }
            place(&mut h, 180, vec![first, unsupported]);
            assert!(
                h.result(180)
                    .unwrap_err()
                    .contains("not supported by Paradex")
            );
            assert!(
                h.pending.is_empty(),
                "unsupported controls must not fetch quotes or send writes"
            );
        }
    }
}

#[test]
fn unsupported_stp_controls_reject_the_whole_batch_before_quotes_or_writes() {
    use safe_cex_engine::model::StpMode;
    for mode in [
        StpMode::ExpireTaker,
        StpMode::ExpireMaker,
        StpMode::ExpireBoth,
    ] {
        let mut h = private();
        let mut unsupported = opts(OrderType::Market);
        unsupported.client_order_id = Some("unsupported-stp".into());
        unsupported.max_slippage_pct = Some(0.1);
        unsupported.stp_mode = Some(mode);
        place(&mut h, 190, vec![opts(OrderType::Limit), unsupported]);
        assert!(
            h.result(190)
                .unwrap_err()
                .contains("not supported by Paradex")
        );
        assert!(h.pending.is_empty());
    }
    for mode in ["", "UNKNOWN"] {
        let json = format!(
            r#"{{"symbol":"BTC-USD-PERP","type":"limit","side":"buy","amount":1,"stpMode":"{mode}"}}"#
        );
        assert!(serde_json::from_str::<PlaceOrderOpts>(&json).is_err());
    }
}

#[test]
fn unsupported_aster_commands_complete_with_an_error_instead_of_waiting_forever() {
    use safe_cex_engine::model::StpMode;
    for command in [
        Command::FetchStpMode { command: 191 },
        Command::SetStpMode {
            command: 191,
            mode: StpMode::ExpireTaker,
        },
        Command::CancelOrdersWithSymbols {
            command: 191,
            orders: vec![],
        },
    ] {
        let mut h = private();
        h.command(command);
        assert!(
            h.result(191)
                .unwrap_err()
                .contains("not supported by Paradex")
        );
        assert!(h.pending.is_empty());
    }
}

fn assert_receipts(
    h: &Harness,
    request: &safe_cex_engine::HttpRequest,
    command: u32,
    expected: Vec<OrderIdentityReceipt>,
) {
    let index = h
        .receipt_trace
        .iter()
        .position(|a| matches!(a, ReceiptTrace::Http(id) if *id == request.id))
        .unwrap();
    match &h.receipt_trace[index - 1] {
        ReceiptTrace::Progress(owner, receipts) => {
            assert_eq!(*owner, command);
            assert_eq!(receipts, &expected);
        }
        _ => panic!("identities must precede the authenticated write"),
    }
}

#[test]
fn batch_receipts_bind_exact_native_generated_and_requested_ids_before_http() {
    let mut h = private();
    let first = opts(OrderType::Limit);
    let mut second = first.clone();
    second.client_order_id = None;
    place(&mut h, 77, vec![first, second]);
    let request = h.take("/orders");
    let rows: serde_json::Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    let expected: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|row| OrderIdentityReceipt {
            symbol: "BTC-USD-PERP".into(),
            kind: OrderIdentityKind::ClientOrderId,
            identity: row["client_id"].as_str().unwrap().into(),
        })
        .collect();
    assert_eq!(expected.len(), 2);
    assert_eq!(expected[0].identity, "caller");
    assert_ne!(expected[1].identity, "caller");
    assert_receipts(&h, &request, 77, expected);
}

#[test]
fn write_failures_preserve_definitive_text_and_separate_ambiguous_diagnostics() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let rejected = |kind| (Some(kind), Some(Outcome::Rejected));
    for (status, body, classified, text) in [
        (503, "", (None, Some(Outcome::Unknown)), "Paradex HTTP 503"),
        (408, "", (None, Some(Outcome::Unknown)), "Paradex HTTP 408"),
        (
            403,
            "",
            rejected(ErrorKind::AuthPermission),
            "Paradex HTTP 403",
        ),
        (
            429,
            "",
            (Some(ErrorKind::RateLimited), Some(Outcome::Unknown)),
            "Paradex HTTP 429",
        ),
        (
            400,
            r#"{"error":"INVALID_PRICE","message":"outside price bands"}"#,
            rejected(ErrorKind::Rejected),
            "Paradex HTTP 400: INVALID_PRICE: outside price bands",
        ),
        (
            200,
            r#"{"error":"INSUFFICIENT_MARGIN","message":"no margin"}"#,
            (None, None),
            "Paradex request rejected: INSUFFICIENT_MARGIN: no margin",
        ),
    ] {
        let mut h = private();
        place(&mut h, 8, vec![opts(OrderType::Limit)]);
        let request = h.take("/orders");
        h.respond(request, status, body);
        if matches!(status, 408 | 429 | 500..) {
            assert!(h.result(8).unwrap_err().contains(ENGINE_UNKNOWN_TEXT));
            assert_eq!(h.failures[0].venue_message.as_deref(), Some(text));
        } else {
            assert_eq!(h.result(8), Err(text), "{status} {body}");
        }
        let error = &h.failures[0];
        assert_eq!((error.kind, error.outcome), classified, "{status} {body}");
    }
    // A malformed 2xx acknowledgement does not prove what happened.
    let mut h = private();
    place(&mut h, 8, vec![opts(OrderType::Limit)]);
    let request = h.take("/orders");
    h.respond(request, 200, "{}");
    assert_eq!(h.failures[0].outcome, Some(Outcome::Unknown));
    assert!(h.result(8).unwrap_err().contains(ENGINE_UNKNOWN_TEXT));
    assert!(h.failures[0].venue_message.is_some());
    // Transport loss on a write is Unknown.
    let mut h = private();
    place(&mut h, 8, vec![opts(OrderType::Limit)]);
    let request = h.take("/orders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: request.id,
            error: "reset",
        },
    );
    h.collect();
    assert_eq!(
        (h.failures[0].kind, h.failures[0].outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
    assert!(h.result(8).unwrap_err().contains(ENGINE_UNKNOWN_TEXT));
    assert_eq!(h.failures[0].venue_message.as_deref(), Some("reset"));
    // Unsupported controls were never sent.
    for command in [
        Command::FetchStpMode { command: 9 },
        Command::SetPositionMode {
            command: 9,
            hedged: true,
            symbol: None,
        },
    ] {
        let mut h = private();
        h.command(command);
        assert_eq!(
            (h.failures[0].kind, h.failures[0].outcome),
            (Some(ErrorKind::Unsupported), Some(Outcome::NotSent))
        );
    }
}

#[test]
fn several_failed_placements_share_an_outcome_only_when_they_agree() {
    use safe_cex_engine::Outcome;
    for (second, outcome) in [(503, Some(Outcome::Unknown)), (400, None)] {
        let mut h = private();
        let mut other = opts(OrderType::Market);
        other.client_order_id = Some("second".into());
        place(&mut h, 73, vec![opts(OrderType::Market), other]);
        let first = h.take("/orders");
        h.respond(
            first,
            200,
            r#"{"error":"FIRST_REJECTED","message":"first reason"}"#,
        );
        let request = h.take("/orders");
        h.respond(request, second, "");
        assert_eq!(h.failures.len(), 1);
        assert_eq!(h.failures[0].outcome, outcome, "{second}");
    }
}

#[test]
fn aggregation_preserves_transport_diagnostic_before_refusal() {
    assert_aggregated_unknown_diagnostic(true, false);
}

#[test]
fn aggregation_preserves_transport_diagnostic_after_refusal() {
    assert_aggregated_unknown_diagnostic(false, false);
}

#[test]
fn aggregation_preserves_malformed_ack_diagnostic_before_refusal() {
    assert_aggregated_unknown_diagnostic(true, true);
}

#[test]
fn aggregation_preserves_malformed_ack_diagnostic_after_refusal() {
    assert_aggregated_unknown_diagnostic(false, true);
}

fn assert_aggregated_unknown_diagnostic(unknown_first: bool, malformed: bool) {
    let mut h = private();
    let mut second = opts(OrderType::Market);
    second.client_order_id = Some("second".into());
    place(&mut h, 73, vec![opts(OrderType::Market), second]);
    for unknown in [unknown_first, !unknown_first]
        .into_iter()
        .take(if unknown_first { 1 } else { 2 })
    {
        let request = take_method(&mut h, Method::Post, "/orders");
        if unknown && malformed {
            h.respond(request, 200, "{}");
        } else if unknown {
            h.e.handle(
                h.now,
                Event::HttpFailed {
                    request: request.id,
                    error: "review transport diagnostic",
                },
            );
            h.collect();
        } else {
            h.respond(
                request,
                400,
                r#"{"error":"INVALID_PRICE","message":"definitive refusal"}"#,
            );
        }
    }
    let [error] = h.failures.as_slice() else {
        panic!("{:?}", h.failures)
    };
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    let diagnostic = if malformed {
        "invalid Paradex placement response"
    } else {
        "review transport diagnostic"
    };
    assert_eq!(
        error.venue_message.as_deref(),
        Some(diagnostic),
        "{error:?}"
    );
    let kind = (!malformed).then_some(safe_cex_engine::ErrorKind::Network);
    assert_eq!(error.kind, kind);
    let text = error.legacy_text().to_ascii_lowercase();
    assert!(
        text.contains("unknown") && text.contains("check open orders"),
        "{error:?}"
    );
    assert_eq!(
        text.contains("definitive refusal"),
        !unknown_first,
        "{text}"
    );
    assert!(!error.message.contains(diagnostic), "{error:?}");
    assert!(
        !h.pending.iter().any(|r| r.method != Method::Get),
        "no resend or put-back: {:?}",
        h.pending
    );
}

#[test]
fn the_first_unknown_diagnostic_is_retained_and_the_next_leg_is_withheld() {
    use safe_cex_engine::{ErrorKind, Outcome};
    for transport_first in [false, true] {
        let mut h = private();
        let mut second = opts(OrderType::Market);
        second.client_order_id = Some("second".into());
        place(&mut h, 73, vec![opts(OrderType::Market), second]);
        for transport in [transport_first, !transport_first].into_iter().take(1) {
            let request = take_method(&mut h, Method::Post, "/orders");
            if transport {
                h.e.handle(
                    h.now,
                    Event::HttpFailed {
                        request: request.id,
                        error: "first transport diagnostic",
                    },
                );
                h.collect();
            } else {
                h.respond(request, 429, "rate diagnostic");
            }
        }
        let [error] = h.failures.as_slice() else {
            panic!("{:?}", h.failures)
        };
        assert_eq!(error.outcome, Some(Outcome::Unknown));
        let (kind, diagnostic) = if transport_first {
            (ErrorKind::Network, "first transport diagnostic")
        } else {
            (ErrorKind::RateLimited, "Paradex HTTP 429")
        };
        assert_eq!(error.kind, Some(kind));
        assert_eq!(error.venue_message.as_deref(), Some(diagnostic));
        assert!(error.message.contains("unknown") && error.message.contains("Check Open Orders"));
        assert!(!h.pending.iter().any(|r| r.method != Method::Get));
    }
}

#[test]
fn aggregation_normalizes_transport_unknown_warning() {
    for lost_first in [false, true] {
        let mut h = private();
        let mut second = opts(OrderType::Market);
        second.client_order_id = Some("second".into());
        place(&mut h, 73, vec![opts(OrderType::Market), second]);
        for lost in [lost_first, !lost_first]
            .into_iter()
            .take(if lost_first { 1 } else { 2 })
        {
            let request = take_method(&mut h, Method::Post, "/orders");
            if lost {
                h.e.handle(
                    h.now,
                    Event::HttpFailed {
                        request: request.id,
                        error: "fixture connection reset",
                    },
                );
                h.collect();
            } else {
                h.respond(
                    request,
                    400,
                    r#"{"error":"INVALID_PRICE","message":"definitive refusal"}"#,
                );
            }
        }
        let [error] = h.failures.as_slice() else {
            panic!("{:?}", h.failures)
        };
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        let text = error.legacy_text().to_ascii_lowercase();
        assert!(
            text.contains("unknown") && text.contains("check open orders"),
            "{error:?}"
        );
        assert_eq!(text.contains("definitive refusal"), !lost_first, "{text}");
        assert!(!text.contains("connection reset"), "{text}");
        assert!(
            !h.pending.iter().any(|r| r.method != Method::Get),
            "no resend or put-back: {:?}",
            h.pending
        );
    }
}

#[test]
fn aggregation_keeps_unknown_warning_beside_a_definitive_refusal() {
    for ambiguous_first in [false, true] {
        for &(status, body) in AMBIGUOUS_WRITES {
            let mut h = private();
            let mut second = opts(OrderType::Market);
            second.client_order_id = Some("second".into());
            place(&mut h, 73, vec![opts(OrderType::Market), second]);
            for ambiguous in [ambiguous_first, !ambiguous_first]
                .into_iter()
                .take(if ambiguous_first { 1 } else { 2 })
            {
                let request = take_method(&mut h, Method::Post, "/orders");
                if ambiguous {
                    h.respond(request, status, body);
                } else {
                    h.respond(
                        request,
                        400,
                        r#"{"error":"INVALID_PRICE","message":"definitive refusal"}"#,
                    );
                }
            }
            assert_eq!(h.failures.len(), 1);
            let error = &h.failures[0];
            assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
            let text = error.legacy_text().to_ascii_lowercase();
            assert!(
                text.contains("unknown") && text.contains("check open orders"),
                "{error:?}"
            );
            assert_eq!(
                text.contains("definitive refusal"),
                !ambiguous_first,
                "only an already-attempted refusal stays visible: {text}"
            );
            assert!(
                !h.pending.iter().any(|r| r.method != Method::Get),
                "no resend or put-back: {:?}",
                h.pending
            );
        }
    }
}

#[test]
fn public_rest_book_refusal_does_not_invalidate_or_refresh_existing_jwt() {
    let mut h = private();
    let before = (
        h.e.store().balance().clone(),
        h.e.store().positions().to_vec(),
        h.e.store().orders().to_vec(),
    );
    h.actions.clear();
    h.e.handle(
        h.now,
        Event::Command(Command::FetchOrderBook {
            command: 70,
            symbol: "BTC-USD-PERP".into(),
            limit: None,
        }),
    );
    for action in safe_cex_engine::test_support::prepared_actions(&mut h.e, h.now) {
        match action {
            Action::Http(request) => {
                h.receipt_trace.push(ReceiptTrace::Http(request.id));
                h.pending.push(request);
            }
            action => h.actions.push(action),
        }
    }
    assert_eq!(h.pending.len(), 1);
    let request = h.take("/orderbook/");
    assert!(request.headers.is_empty());
    h.respond(
        request,
        401,
        r#"{"error":"INVALID_API_KEY","message":"invalid api key"}"#,
    );
    assert!(h.result(70).is_err());
    assert!(h.pending.is_empty());
    assert!(!h.actions.iter().any(|a| matches!(
        a,
        Action::Notice(safe_cex_engine::Notice::AccountAuth { .. })
    )));
    assert_eq!(
        (
            h.e.store().balance().clone(),
            h.e.store().positions().to_vec(),
            h.e.store().orders().to_vec()
        ),
        before
    );
    h.command(Command::FetchAccount { command: 71 });
    assert_eq!(h.pending.len(), 1);
    let request = h.take("/account");
    assert!(
        request
            .headers
            .iter()
            .any(|(name, value)| *name == "Authorization" && value == "Bearer jwt")
    );
    h.respond(request, 200, r#"{"account_number":"9007199254740993"}"#);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(h.result(71).unwrap()).unwrap()["userId"],
        "9007199254740993"
    );
}
fn take_method(h: &mut Harness, method: Method, path: &str) -> safe_cex_engine::HttpRequest {
    let i = h
        .pending
        .iter()
        .position(|r| r.method == method && r.url.contains(path))
        .unwrap_or_else(|| panic!("missing {method:?} {path}: {:?}", h.pending));
    h.pending.remove(i)
}
#[test]
fn an_unknown_placement_refuses_a_retry_until_a_fresh_order_read_reconciles_it() {
    let mut h = private();
    // An order read dispatched before the write cannot clear its fence.
    h.timer(h.now + 10_000.);
    h.reply("/positions", r#"{"results":[]}"#);
    h.reply(
        "/account",
        r#"{"total_collateral":"1000","free_collateral":"1000"}"#,
    );
    let stale = take_method(&mut h, Method::Get, "/orders");
    place(&mut h, 8, vec![opts(OrderType::Limit)]);
    let write = take_method(&mut h, Method::Post, "/orders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: write.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    assert!(h.result(8).is_err());

    // 1. The retry is refused NotSent and never written.
    place(&mut h, 9, vec![opts(OrderType::Limit)]);
    assert!(
        !h.pending.iter().any(|r| r.method == Method::Post),
        "the retry was written while the first placement is uncertain"
    );
    assert!(h.result(9).unwrap_err().contains("uncertain"));
    assert_eq!(
        h.failures.last().and_then(|e| e.outcome),
        Some(safe_cex_engine::Outcome::NotSent)
    );

    // 2. The read in flight before the write completes; the fence holds.
    h.respond(stale, 200, r#"{"results":[]}"#);
    place(&mut h, 10, vec![opts(OrderType::Limit)]);
    assert!(h.result(10).unwrap_err().contains("uncertain"));
    assert!(!h.pending.iter().any(|r| r.method == Method::Post));

    // 3. The hook's owed read, dispatched after the write, releases it.
    let fresh = take_method(&mut h, Method::Get, "/orders");
    h.respond(fresh, 200, r#"{"results":[]}"#);

    // 4. The retry now places.
    place(&mut h, 11, vec![opts(OrderType::Limit)]);
    take_method(&mut h, Method::Post, "/orders");
}
#[test]
fn a_rejected_row_on_another_market_releases_the_fence() {
    let mut h = private();
    place(&mut h, 8, vec![opts(OrderType::Limit)]);
    let write = take_method(&mut h, Method::Post, "/orders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: write.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    let fresh = take_method(&mut h, Method::Get, "/orders");
    h.respond(
        fresh,
        200,
        r#"{"results":[{"id":"77","market":"SOL-USD-PERP"}]}"#,
    );
    place(&mut h, 9, vec![opts(OrderType::Limit)]);
    take_method(&mut h, Method::Post, "/orders");
}

#[test]
fn engine_reasks_join_a_slow_post_write_read_that_then_releases_the_fence() {
    let mut h = private();
    place(&mut h, 8, vec![opts(OrderType::Limit)]);
    let write = take_method(&mut h, Method::Post, "/orders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: write.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    // The hook's read, dispatched after the write, stays unanswered.
    let slow = take_method(&mut h, Method::Get, "/orders");
    let start = h.now;
    let mut reasks = vec![];
    for _ in 0..3 {
        let (timer, at) = h
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms > h.now => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("engine re-ask");
        reasks.push(at - start);
        h.now = at;
        h.e.handle(h.now, Event::TimerFired { timer });
        h.collect();
        // Each re-ask joins the slow read: no restart, no second read.
        assert!(h.pending.is_empty(), "{:?}", h.pending);
        place(&mut h, 9, vec![opts(OrderType::Limit)]);
        assert!(h.result(9).unwrap_err().contains("uncertain"));
    }
    assert_eq!(reasks, vec![1_000., 3_000., 7_000.]);
    h.respond(slow, 200, r#"{"results":[]}"#);
    // The slow read releases on completion; at most the one owed read follows.
    let reads = h.pending.iter().filter(|r| r.method == Method::Get).count();
    assert!(reads <= 1, "{:?}", h.pending);
    place(&mut h, 10, vec![opts(OrderType::Limit)]);
    take_method(&mut h, Method::Post, "/orders");
}
#[test]
fn cancel_all_waits_for_reconciliation_after_an_unknown_placement_but_cancel_by_id_does_not() {
    let mut h = private();
    place(&mut h, 8, vec![opts(OrderType::Limit)]);
    let write = take_method(&mut h, Method::Post, "/orders");
    h.e.handle(
        h.now,
        Event::HttpFailed {
            request: write.id,
            error: "response lost after acceptance",
        },
    );
    h.collect();
    let fresh = take_method(&mut h, Method::Get, "/orders");
    // A late-landing order could survive a venue-side cancel-all, so the
    // engine default refuses it NotSent before any DELETE.
    h.command(Command::CancelSymbolOrders {
        command: 9,
        symbol: "BTC-USD-PERP".into(),
    });
    assert!(
        !h.pending.iter().any(|r| r.method == Method::Delete),
        "cancel-all was sent while the placement is uncertain"
    );
    assert!(h.result(9).unwrap_err().contains("uncertain"));
    assert_eq!(
        h.failures.last().and_then(|e| e.outcome),
        Some(safe_cex_engine::Outcome::NotSent)
    );
    // Cancel by ID cannot place, so it stays admitted.
    h.command(Command::CancelOrders {
        command: 10,
        ids: vec!["resting".into()],
    });
    let cancel = take_method(&mut h, Method::Delete, "/orders/resting");
    h.respond(cancel, 200, "{}");
    assert!(!h.result(10).is_err_and(|e| e.contains("uncertain")));
    // The cancel changed the order store after that read began, so its
    // snapshot cannot commit and cannot release the fence.
    h.respond(fresh, 200, r#"{"results":[]}"#);
    h.command(Command::CancelSymbolOrders {
        command: 11,
        symbol: "BTC-USD-PERP".into(),
    });
    assert!(h.result(11).unwrap_err().contains("uncertain"));
    // The engine's re-ask starts a new read, whose clean snapshot releases it.
    let (timer, at) = h
        .actions
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms > h.now => Some((*timer, *at_ms)),
            _ => None,
        })
        .expect("engine re-ask");
    h.now = at;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.collect();
    let reread = take_method(&mut h, Method::Get, "/orders");
    h.respond(reread, 200, r#"{"results":[]}"#);
    h.command(Command::CancelSymbolOrders {
        command: 12,
        symbol: "BTC-USD-PERP".into(),
    });
    take_method(&mut h, Method::Delete, "/orders?market=");
}

#[test]
fn a_refused_auth_is_a_typed_terminal_failure_without_retries() {
    let mut h = private();
    // Past the JWT refresh point, the next private request re-authenticates.
    h.now += 180_000.;
    place(&mut h, 80, vec![opts(OrderType::Limit)]);
    let auth = h.take("/auth/");
    h.respond(
        auth,
        401,
        r#"{"error":"INVALID_SIGNATURE","message":"unknown subkey"}"#,
    );
    let error = failed(&h, 80);
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::AuthInvalid));
    assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::NotSent));
    // No 2 s retry is armed, and nothing re-authenticates later.
    let at = h.now + 2_000.;
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a, Action::SetTimer { at_ms, .. } if (*at_ms - at).abs() < 1.))
    );
    place(&mut h, 81, vec![opts(OrderType::Limit)]);
    assert!(!h.pending.iter().any(|r| r.url.contains("/auth/")));
    let error = failed(&h, 81);
    assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::AuthInvalid));
}

#[test]
fn a_real_batch_answer_with_a_null_error_slot_is_accepted() {
    let mut h = private();
    place(&mut h, 9, vec![opts(OrderType::Limit)]);
    let r = h.take("/orders");
    let batch = r.url.contains("batch");
    let client = opts(OrderType::Limit).client_order_id.unwrap();
    let body = format!(
        r#"{{"orders":[{{"id":"1793547697690201709231680000","market":"BTC-USD-PERP","side":"BUY","type":"LIMIT","size":"12","remaining_size":"12","price":"1.1117","status":"NEW","cancel_reason":"","client_id":"{client}","instruction":"GTC","flags":[],"trigger_price":"0"}}],"errors":[null]}}"#
    );
    h.respond(r, 200, &body);
    assert!(batch, "this shape is the batch endpoint's");
    assert!(h.result(9).is_ok(), "{:?}", h.result(9));
}

#[test]
fn a_refused_bracket_leg_is_reported_with_the_entry_as_partial() {
    let mut h = private();
    let mut order = opts(OrderType::Limit);
    order.client_order_id = None;
    order.stop_loss = Some(90.);
    place(&mut h, 9, vec![order]);
    let main = h.take("/orders");
    let payload: Payload = serde_json::from_str(main.body.as_deref().unwrap()).unwrap();
    let client = payload.client_id.unwrap();
    h.respond(
        main,
        200,
        &format!("{{\"id\":\"entry\",\"client_id\":\"{client}\"}}"),
    );
    let sl = h.take("/orders");
    h.respond(
        sl,
        400,
        r#"{"error":"REDUCE_ONLY_NO_POSITION","message":"no position"}"#,
    );
    assert!(h.result(9).is_err());
}

fn with_untriggered_stop() -> Harness {
    let mut h = private();
    let private = h
        .actions
        .iter()
        .find_map(|a| match a {
            safe_cex_engine::Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    h.e.handle(h.now, Event::WsOpened { socket: private });
    h.collect();
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket: private,
            text: r#"{"jsonrpc":"2.0","id":0,"result":{}}"#,
        },
    );
    h.collect();
    h.e.handle(h.now,Event::WsMessage{socket:private,text:r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"orders.ALL","data":{"id":"123","market":"BTC-USD-PERP","type":"STOP_LOSS_MARKET","status":"UNTRIGGERED","side":"SELL","size":"2","remaining_size":"2","price":"95","trigger_price":"90","flags":["REDUCE_ONLY"],"instruction":"GTC","created_at":1700000000000}}}"#});
    h.collect();
    h
}

#[test]
fn a_stop_replacement_with_unknown_outcome_says_the_stop_may_be_gone() {
    let mut h = with_untriggered_stop();
    h.command(Command::AmendOrder {
        command: 13,
        id: "123".into(),
        price: Some(91.),
        amount: None,
    });
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let place = h.take("/orders");
    h.respond(place, 503, "");
    assert_eq!(
        order_posts(&h),
        0,
        "an unknown replacement is never doubled"
    );
    let error = h.result(13).unwrap_err();
    assert!(error.contains("may have no stop"), "{error}");
    assert!(error.contains("Check Open Orders"), "{error}");
}

#[test]
fn cancel_all_still_sweeps_the_market_when_a_stop_delete_fails() {
    let mut h = with_untriggered_stop();
    h.command(Command::CancelSymbolOrders {
        command: 12,
        symbol: "BTC-USD-PERP".into(),
    });
    let stop = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(
        stop,
        429,
        r#"{"error":"RATE_LIMIT_EXCEEDED","message":"rate limit exceeded"}"#,
    );
    let all = take_method(&mut h, Method::Delete, "/orders?market=");
    h.respond(all, 200, r#"{"message":"cancel requests queued"}"#);
    assert!(h.result(12).is_err(), "the stop may still be live");
}

fn canceled_stop_row() -> &'static str {
    r#"{"id":"123","market":"BTC-USD-PERP","type":"STOP_LOSS_MARKET","status":"CLOSED","cancel_reason":"USER_CANCELED","side":"SELL","size":"2","remaining_size":"2","price":"95","trigger_price":"90","flags":["REDUCE_ONLY"],"instruction":"GTC","created_at":1700000000000,"last_updated_at":1700000000100}"#
}
fn confirm_cancel_row(h: &mut Harness, row: &str) {
    assert_eq!(order_posts(h), 0, "no create before exact target proof");
    let read = take_method(h, Method::Get, "/orders-history?");
    assert!(
        read.url
            .contains("market=BTC-USD-PERP&status=CLOSED&page_size=100")
    );
    h.respond(read, 200, &format!(r#"{{"results":[{row}]}}"#));
}
fn confirm_stop_cancel(h: &mut Harness) {
    confirm_cancel_row(h, canceled_stop_row());
}
fn expire_cancel_proof(h: &mut Harness) {
    let timer = h.timer_at(h.now + 5_000.);
    h.now += 5_000.;
    h.e.handle(h.now, Event::TimerFired { timer });
    h.collect();
    assert_eq!(order_posts(h), 0);
    assert_eq!(
        h.failures.last().unwrap().outcome,
        Some(safe_cex_engine::Outcome::Unknown)
    );
    assert_eq!(h.failures.last().unwrap().protection, None);
    assert!(
        h.failures
            .last()
            .unwrap()
            .message
            .contains("Check Open Orders and your position before replacing protection")
    );
}
#[test]
fn guarded_cancel_not_found_never_authorizes_a_replacement_or_put_back() {
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(
        cancel,
        400,
        r#"{"error":"ORDER_ID_NOT_FOUND","message":"order not found"}"#,
    );
    assert_eq!(
        order_posts(&h),
        0,
        "ORDER_ID_NOT_FOUND is not unfilled target-terminal cancellation proof"
    );
}

#[test]
fn queued_guarded_cancel_waits_for_target_terminal_proof_before_create() {
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    assert_eq!(
        order_posts(&h),
        0,
        "204 queues cancellation; it does not prove the stop canceled unfilled"
    );
}

#[test]
fn a_targeted_unfilled_cancel_proof_releases_exactly_one_replacement_even_after_not_found() {
    for status in [204, 400] {
        let mut h = with_untriggered_stop();
        amend_stop(&mut h, 13);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        h.respond(
            cancel,
            status,
            if status == 400 {
                r#"{"error":"ORDER_ID_NOT_FOUND","message":"order not found"}"#
            } else {
                ""
            },
        );
        confirm_stop_cancel(&mut h);
        assert_eq!(order_posts(&h), 1);
        let place = take_method(&mut h, Method::Post, "/orders");
        let payload: Payload = serde_json::from_str(place.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.trigger_price.as_deref(), Some("91"));
        assert_eq!(payload.size.as_deref(), Some("2"));
        h.respond(
            place,
            201,
            &format!(
                r#"{{"id":"replacement","client_id":"{}"}}"#,
                payload.client_id.unwrap()
            ),
        );
        assert!(h.result(13).is_ok());
        assert_eq!(order_posts(&h), 0);
    }
}

fn observe_proof_stop(h: &mut Harness, client_id: Option<&str>) {
    let socket = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect {
                socket,
                channel: Some(safe_cex_engine::SocketChannel::Private),
                ..
            } => Some(*socket),
            _ => None,
        })
        .unwrap();
    let mut row: serde_json::Value = serde_json::from_str(canceled_stop_row()).unwrap();
    row["status"] = "UNTRIGGERED".into();
    row["cancel_reason"] = "".into();
    if let Some(client_id) = client_id {
        row["client_id"] = client_id.into();
    }
    h.e.handle(
        h.now,
        Event::WsMessage {
            socket,
            text: &serde_json::json!({
                "jsonrpc": "2.0", "method": "subscription",
                "params": {"channel": "orders.ALL", "data": row}
            })
            .to_string(),
        },
    );
    h.collect();
}

#[test]
fn cancel_proof_history_window_adds_margins_to_target_creation_and_each_reads_own_time() {
    for client_id in [None, Some("ts_original")] {
        let mut h = with_untriggered_stop();
        observe_proof_stop(&mut h, client_id);
        h.now += 200.75;
        amend_stop(&mut h, 13);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        h.now += 100.;
        h.respond(cancel, 204, "");
        for poll in 0..3 {
            let read = take_method(&mut h, Method::Get, "/orders-history?");
            assert!(
                read.url.contains(&format!(
                    "&start_at=1699999940000&end_at={:.0}",
                    h.now.floor() + 60_000.
                )),
                "{}",
                read.url
            );
            assert_eq!(
                read.url.contains("&client_id=ts_original"),
                client_id.is_some()
            );
            h.respond(read, 200, r#"{"results":[]}"#);
            assert_eq!(order_posts(&h), 0);
            if poll < 2 {
                h.timer(h.now + 1_000.);
            }
        }
        assert_eq!(
            h.failures.last().unwrap().outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert_eq!(h.failures.last().unwrap().protection, None);
    }
}

#[test]
fn a_clientless_bracket_leg_can_prove_its_cancel_in_the_bounded_history_window() {
    let mut h = with_untriggered_stop();
    let mut entry = opts(OrderType::Limit);
    entry.amount = 2.;
    entry.client_order_id = None;
    entry.stop_loss = Some(90.);
    place(&mut h, 9, vec![entry]);
    let parent = take_method(&mut h, Method::Post, "/orders");
    let parent_payload: Payload = serde_json::from_str(parent.body.as_deref().unwrap()).unwrap();
    h.respond(
        parent,
        200,
        &format!(
            r#"{{"id":"entry","client_id":"{}"}}"#,
            parent_payload.client_id.unwrap()
        ),
    );
    let child = take_method(&mut h, Method::Post, "/orders");
    let payload: Payload = serde_json::from_str(child.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "STOP_LOSS_MARKET");
    assert_eq!(payload.size.as_deref(), Some("2"));
    assert_eq!(payload.trigger_price.as_deref(), Some("90"));
    assert_eq!(
        payload.client_id, None,
        "native bracket children omit client_id"
    );
    h.respond(child, 201, r#"{"id":"123"}"#);
    assert!(h.result(9).is_ok());
    observe_proof_stop(&mut h, None);
    h.now += 200.;
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    let read = take_method(&mut h, Method::Get, "/orders-history?");
    assert!(
        read.url
            .contains("&start_at=1699999940000&end_at=1700000060200"),
        "{}",
        read.url
    );
    assert!(!read.url.contains("client_id="));
    h.respond(
        read,
        200,
        &format!(r#"{{"results":[{}]}}"#, canceled_stop_row()),
    );
    assert_eq!(order_posts(&h), 1);
}

fn prove_cancel_through_an_exclusive_history_window(filter_updated: bool) {
    let mut h = with_untriggered_stop();
    h.now += 200.;
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    let read = take_method(&mut h, Method::Get, "/orders-history?");
    let bound = |name: &str| -> f64 {
        read.url
            .split_once('?')
            .unwrap()
            .1
            .split('&')
            .find_map(|pair| {
                let (key, value) = pair.split_once('=')?;
                (key == name).then(|| value.parse().unwrap())
            })
            .unwrap()
    };
    let mut row: serde_json::Value = serde_json::from_str(canceled_stop_row()).unwrap();
    if filter_updated {
        row["last_updated_at"] = 1_700_000_005_000_u64.into();
        assert!(row["last_updated_at"].as_f64().unwrap() > h.now);
    }
    let timestamp = row[if filter_updated {
        "last_updated_at"
    } else {
        "created_at"
    }]
    .as_f64()
    .unwrap();
    let body = if bound("start_at") < timestamp && timestamp < bound("end_at") {
        serde_json::json!({"results": [row]}).to_string()
    } else {
        r#"{"results":[]}"#.into()
    };
    h.respond(read, 200, &body);
    assert_eq!(
        order_posts(&h),
        1,
        "the exact target must remain inside exclusive history bounds"
    );
    let place = take_method(&mut h, Method::Post, "/orders");
    let payload: Payload = serde_json::from_str(place.body.as_deref().unwrap()).unwrap();
    h.respond(
        place,
        201,
        &format!(
            r#"{{"id":"replacement","client_id":"{}"}}"#,
            payload.client_id.unwrap()
        ),
    );
    assert!(h.result(13).is_ok());
}

#[test]
fn cancel_proof_window_includes_the_exact_target_creation_boundary() {
    prove_cancel_through_an_exclusive_history_window(false);
}

#[test]
fn cancel_proof_window_includes_a_venue_update_ahead_of_the_device_clock() {
    prove_cancel_through_an_exclusive_history_window(true);
}

#[test]
fn a_full_cancel_proof_page_requires_the_exact_target_even_with_a_cursor() {
    for target_present in [false, true] {
        let mut h = with_untriggered_stop();
        amend_stop(&mut h, 13);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        h.respond(cancel, 204, "");
        let mut rows = (0..100)
            .map(|i| canceled_stop_row().replace("\"123\"", &format!("\"peer-{i}\"")))
            .collect::<Vec<_>>();
        if target_present {
            rows[99] = canceled_stop_row().into();
        }
        let body = format!(r#"{{"results":[{}],"next":"more"}}"#, rows.join(","));
        let read = take_method(&mut h, Method::Get, "/orders-history?");
        h.respond(read, 200, &body);
        if target_present {
            assert_eq!(order_posts(&h), 1);
        } else {
            assert_eq!(order_posts(&h), 0);
            expire_cancel_proof(&mut h);
        }
    }
}

#[test]
fn cancel_proof_never_infers_cancellation_or_zero_fill_from_incomplete_rows() {
    for row in [
        canceled_stop_row().replace("\"id\":\"123\"", "\"id\":\"other\""),
        canceled_stop_row().replace("CLOSED", "OPEN"),
        canceled_stop_row().replace("USER_CANCELED", "FILLED"),
        canceled_stop_row().replace("USER_CANCELED", ""),
        canceled_stop_row().replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"1\""),
        canceled_stop_row().replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"0\""),
        canceled_stop_row().replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"NaN\""),
        canceled_stop_row().replace("\"size\":\"2\"", "\"size\":\"3\""),
        canceled_stop_row().replace(
            "\"remaining_size\":\"2\"",
            "\"remaining_size\":\"2.000000001\"",
        ),
        canceled_stop_row().replace(
            "\"remaining_size\":\"2\"",
            "\"remaining_size\":\"1.99999999999999999999\"",
        ),
        canceled_stop_row().replace("1700000000100", "1690000000000"),
        format!("{},{}", canceled_stop_row(), canceled_stop_row()),
        canceled_stop_row().replace("BTC-USD-PERP", "ETH-USD-PERP"),
        canceled_stop_row().replace("\"trigger_price\":\"90\"", "\"trigger_price\":\"89\""),
        canceled_stop_row().replace("1700000000000", "1690000000000"),
        "{}".into(),
    ] {
        let mut h = with_untriggered_stop();
        amend_stop(&mut h, 13);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        h.respond(cancel, 204, "");
        confirm_cancel_row(&mut h, &row);
        assert_eq!(order_posts(&h), 0, "{row}");
        expire_cancel_proof(&mut h);
    }
    for body in ["{}", "[]", r#"{"results":null}"#, r#"{"results":[]}"#] {
        let mut h = with_untriggered_stop();
        amend_stop(&mut h, 13);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        h.respond(cancel, 204, "");
        let read = take_method(&mut h, Method::Get, "/orders-history?");
        h.respond(read, 200, body);
        expire_cancel_proof(&mut h);
    }
}

#[test]
fn cancel_proof_polls_are_bounded_and_share_one_nonrenewing_deadline() {
    let mut h = with_untriggered_stop();
    amend_stop(&mut h, 13);
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    for n in 0..3 {
        let read = take_method(&mut h, Method::Get, "/orders-history?");
        h.respond(read, 200, r#"{"results":[],"next":"more"}"#);
        assert_eq!(order_posts(&h), 0);
        if n < 2 {
            let timer = h.timer_at(h.now + 1_000.);
            h.now += 1_000.;
            h.e.handle(h.now, Event::TimerFired { timer });
            h.collect();
        }
    }
    assert!(
        h.result(13)
            .unwrap_err()
            .contains("outcome of your BTC-USD-PERP protection update is unknown")
    );
    assert!(!h.pending.iter().any(|r| r.url.contains("/orders-history")));
}

#[test]
fn a_late_terminal_proof_at_the_deadline_never_creates_or_finishes_twice() {
    for deliver_first in [false, true] {
        let mut h = with_untriggered_stop();
        amend_stop(&mut h, 13);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        h.respond(cancel, 204, "");
        let read = take_method(&mut h, Method::Get, "/orders-history?");
        let timer = h.timer_at(h.now + 5_000.);
        h.now += 5_000.;
        if !deliver_first {
            h.e.handle(h.now, Event::TimerFired { timer });
            h.collect();
        }
        h.respond(
            read,
            200,
            &format!(r#"{{"results":[{}]}}"#, canceled_stop_row()),
        );
        if deliver_first {
            h.e.handle(h.now, Event::TimerFired { timer });
            h.collect();
        }
        assert_eq!(order_posts(&h), 0);
        assert_eq!(h.failures.len(), 1);
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        h.e.handle(h.now, Event::TimerFired { timer });
        h.collect();
        assert_eq!(h.failures.len(), 1, "retired timer is inert");
        assert_eq!(order_posts(&h), 0);
    }
}

#[test]
fn ordinary_paradex_place_amend_and_cancel_keep_the_master_wire_and_outcomes() {
    fn wire(request: &safe_cex_engine::HttpRequest) -> serde_json::Value {
        serde_json::json!({
            "method": format!("{:?}", request.method), "url": request.url,
            "headers": request.headers, "body": request.body, "route": format!("{:?}", request.route),
        })
    }
    let mut records = Vec::new();
    let mut h = private();
    place(&mut h, 31, vec![opts(OrderType::Limit)]);
    let request = h.take("/orders/batch");
    records.push(wire(&request));
    h.respond(
        request,
        201,
        r#"{"orders":[{"id":"ordinary","client_id":"caller"}]}"#,
    );
    assert_eq!(h.result(31), Ok("[\"ordinary\"]"));
    h.command(Command::CancelOrders {
        command: 32,
        ids: vec!["ordinary".into()],
    });
    let request = take_method(&mut h, Method::Delete, "/orders/ordinary");
    records.push(wire(&request));
    h.respond(
        request,
        400,
        r#"{"error":"ORDER_ID_NOT_FOUND","message":"order not found"}"#,
    );
    assert_eq!(h.result(32), Ok("[]"));
    assert!(
        !h.pending
            .iter()
            .any(|request| request.url.contains("/orders-history"))
    );
    let (mut h, _) = amendment_account(AMEND_ROW);
    amend(&mut h, 33, Some(101.), Some(3.));
    let request = take_method(&mut h, Method::Put, "/orders/123");
    records.push(wire(&request));
    h.respond(request, 200, r#"{"id":"123"}"#);
    assert!(h.result(33).is_ok());
    assert!(
        !h.pending
            .iter()
            .any(|request| request.url.contains("/orders-history"))
    );
    let wire = serde_json::to_vec(&records).unwrap();
    assert_eq!(
        safe_cex_crypto::hex(&safe_cex_crypto::sha256(&wire)),
        "b3d210149a73d9320751a3768265a91c057ddc76e3b48baf11cd962aa65c5675"
    );
}

#[test]
fn a_failed_cancel_proof_read_never_restores_or_retries_a_write() {
    for status in [400, 401, 403, 408, 429, 500, 503] {
        let mut h = with_untriggered_stop();
        amend_stop(&mut h, 13);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        h.respond(cancel, 204, "");
        let read = take_method(&mut h, Method::Get, "/orders-history?");
        h.respond(
            read,
            status,
            r#"{"error":"READ_FAILED","message":"read failed"}"#,
        );
        assert_eq!(order_posts(&h), 0);
        assert_eq!(h.failures.len(), 1);
        assert_eq!(
            h.failures[0].outcome,
            Some(safe_cex_engine::Outcome::Unknown)
        );
        assert_eq!(h.failures[0].protection, None);
        assert!(
            h.failures[0]
                .message
                .contains("Check Open Orders and your position before replacing protection")
        );
        assert!(
            !h.pending
                .iter()
                .any(|r| r.method == Method::Delete || r.url.contains("/orders-history"))
        );
    }
}

#[test]
fn a_terminal_proof_cannot_override_an_observed_fill_or_a_new_cancel() {
    for observed in ["fill", "cancel", "symbol-cancel", "crossed"] {
        let mut h = with_untriggered_stop();
        amend_stop(&mut h, 13);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        h.respond(cancel, 204, "");
        let read = take_method(&mut h, Method::Get, "/orders-history?");
        match observed {
            "fill" => {
                let socket = h.actions.iter().find_map(|a| match a {
                    Action::WsConnect { socket, channel: Some(safe_cex_engine::SocketChannel::Private), .. } => Some(*socket),
                    _ => None,
                }).unwrap();
                amendment_update(&mut h, socket, &canceled_stop_row().replace("\"remaining_size\":\"2\"", "\"remaining_size\":\"1\""));
                assert_eq!(h.e.store().order("123").unwrap().filled, 1.);
            }
            "cancel" => {
                h.command(Command::CancelOrders { command: 14, ids: vec!["123".into()] });
                let user_cancel = take_method(&mut h, Method::Delete, "/orders/123");
                h.respond(user_cancel, 204, "");
                assert_eq!(h.result(14), Ok("[]"));
            }
            "symbol-cancel" => {
                let socket = h.actions.iter().find_map(|a| match a {
                    Action::WsConnect { socket, channel: Some(safe_cex_engine::SocketChannel::Private), .. } => Some(*socket),
                    _ => None,
                }).unwrap();
                amendment_update(&mut h, socket, canceled_stop_row());
                assert_eq!(h.e.store().order("123").unwrap().status, safe_cex_engine::model::OrderStatus::Canceled);
                h.command(Command::CancelSymbolOrders { command: 14, symbol: "BTC-USD-PERP".into() });
                let stop_cancel = take_method(&mut h, Method::Delete, "/orders/123");
                h.respond(stop_cancel, 204, "");
                let user_cancel = take_method(&mut h, Method::Delete, "/orders?market=");
                h.respond(user_cancel, 200, r#"{"message":"cancel requests queued"}"#);
                assert_eq!(h.result(14), Ok("[]"));
            }
            _ => h.ws(r#"{"jsonrpc":"2.0","method":"subscription","params":{"channel":"markets_summary","data":{"symbol":"BTC-USD-PERP","last_traded_price":"90","mark_price":"90"}}}"#),
        }
        if observed == "crossed" {
            // Use the Store-facing quote receipt also used by the existing final-send controls.
            h.e.store_mut().update_ticker("BTC-USD-PERP", |ticker| {
                ticker.last = 90.;
                ticker.mark = 90.;
            });
        }
        h.respond(
            read,
            200,
            &format!(r#"{{"results":[{}]}}"#, canceled_stop_row()),
        );
        assert_eq!(order_posts(&h), 0, "{observed}");
        expire_cancel_proof(&mut h);
    }
}

fn parity_market_limits(h: &mut Harness, min: f64) {
    let mut markets = h.e.store().markets().to_vec();
    let market = markets
        .iter_mut()
        .find(|m| m.symbol == "BTC-USD-PERP")
        .unwrap();
    market.limits.amount.min = min;
    market.limits.amount.max = 5.0;
    market.precision.amount = 0.1;
    market.precision.price = 0.1;
    h.e.store_mut().set_markets(markets);
}

fn parity_ack_batch(h: &mut Harness, request: safe_cex_engine::HttpRequest, prefix: &str) {
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    let response: Vec<_> = rows.iter().enumerate().map(|(index, row)| {
        serde_json::json!({"id":format!("{prefix}-{index}"),"client_id":row["client_id"]})
    }).collect();
    h.respond(
        request,
        200,
        &serde_json::json!({"orders":response}).to_string(),
    );
}

fn assert_batch_refusal(h: &Harness, command: u32, accepted: Vec<String>, message: &str) {
    use safe_cex_engine::{Notice, Outcome};
    let failures: Vec<_> = h
        .actions
        .iter()
        .filter_map(|action| match action {
            Action::Notice(Notice::CommandFailed { command: id, error }) if *id == command => {
                Some(error)
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        failures.len(),
        1,
        "batch refusal must fail the command: {:?}",
        h.result(command)
    );
    let error = failures[0];
    assert!(error.message.contains(message), "{error:?}");
    assert_eq!(
        error.partial.as_ref().map(|partial| &partial.order_ids),
        Some(&accepted)
    );
    assert_eq!(
        error.outcome, None,
        "accepted prefix plus definite refusal is a known partial failure"
    );
    assert!(!h.actions.iter().any(|action| matches!(
        action,
        Action::Notice(Notice::CommandDone { command: id, .. }) if *id == command
    )));
    assert!(!h.actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::CommandFailed { error, .. }) if error.outcome == Some(Outcome::Unknown)
    )));
    assert!(!h.actions.iter().any(|action| matches!(action,
        Action::Notice(Notice::Event(event)) if event.code == "order.outcome_unknown"
    )));
}

#[test]
fn parity_later_native_rule_violations_reach_the_venue_after_the_first_batch() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:1574-1593,1683-1713,1949-1965.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:423/ten-row-batch-and-serial-placement
    for (amount, price) in [
        (0.5, 100.0),
        (1.05, 100.0),
        (6.0, 100.0),
        (1.0, 100.05),
        (0.01, 100.0),
    ] {
        let mut h = private();
        parity_market_limits(&mut h, 1.0);
        h.receipt_trace.clear();
        let mut good = opts(OrderType::Limit);
        good.client_order_id = None;
        let mut orders = vec![good.clone(); 10];
        let mut last = good;
        last.amount = amount;
        last.price = Some(price);
        orders.push(last);
        place(&mut h, 91, orders);
        let first = h.take("/orders/batch");
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(first.body.as_deref().unwrap()).unwrap();
        assert_eq!(rows.len(), 10);
        parity_ack_batch(&mut h, first, "accepted");
        let last = h.take("/orders/batch");
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(last.body.as_deref().unwrap()).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["size"], amount.to_string());
        assert_eq!(rows[0]["price"], price.to_string());
        h.respond(
            last,
            400,
            r#"{"error":"VALIDATION","message":"venue refused final row"}"#,
        );
        assert_batch_refusal(
            &h,
            91,
            (0..10).map(|index| format!("accepted-{index}")).collect(),
            "venue refused final row",
        );
        assert!(h.actions.iter().any(|action| matches!(action,
            Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                level: safe_cex_engine::Level::Error, message, ..
            })) if message.contains("venue refused final row"))));
        assert_eq!(
            h.receipt_trace
                .iter()
                .filter(|entry| matches!(entry, ReceiptTrace::Http(_)))
                .count(),
            2
        );
        assert!(
            h.pending
                .iter()
                .all(|request| request.method == Method::Get)
        );
    }
}

#[test]
fn parity_an_off_tick_child_is_submitted_after_its_parent() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:1414-1422,1856-1875.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:532/separate-TP-SL
    let mut h = private();
    parity_market_limits(&mut h, 1.0);
    h.receipt_trace.clear();
    let mut order = opts(OrderType::Limit);
    order.client_order_id = None;
    order.stop_loss = Some(90.05);
    place(&mut h, 91, vec![order]);
    let parent = h.take("/orders");
    let payload: Payload = serde_json::from_str(parent.body.as_deref().unwrap()).unwrap();
    h.respond(
        parent,
        200,
        &serde_json::json!({"id":"parent","client_id":payload.client_id}).to_string(),
    );
    let child = h.take("/orders");
    let payload: Payload = serde_json::from_str(child.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.trigger_price.as_deref(), Some("90.05"));
    h.respond(child, 400, r#"{"error":"VALIDATION","message":"off tick"}"#);
    assert!(h.result(91).is_err());
    assert_eq!(
        h.receipt_trace
            .iter()
            .filter(|entry| matches!(entry, ReceiptTrace::Http(_)))
            .count(),
        2
    );
    assert!(
        h.pending
            .iter()
            .all(|request| request.method == Method::Get)
    );
}

#[test]
fn parity_replacement_and_exact_put_back_have_no_new_native_minimum_gate() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:2067-2106; historical 31d7fda4:1394-1438.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:1785/protective-amend-and-put-back-gating
    let mut h = with_untriggered_stop();
    parity_market_limits(&mut h, 3.0);
    h.command(Command::AmendOrder {
        command: 91,
        id: "123".into(),
        price: Some(91.0),
        amount: Some(0.5),
    });
    let cancel = take_method(&mut h, Method::Delete, "/orders/123");
    h.respond(cancel, 204, "");
    confirm_stop_cancel(&mut h);
    let replacement = h.take("/orders");
    let payload: Payload = serde_json::from_str(replacement.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.size.as_deref(), Some("0.5"));
    h.respond(
        replacement,
        400,
        r#"{"error":"VALIDATION","message":"refused"}"#,
    );
    let restore = h.take("/orders");
    let payload: Payload = serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.size.as_deref(), Some("2"));
    assert_eq!(payload.trigger_price.as_deref(), Some("90"));
    h.respond(
        restore,
        201,
        &serde_json::json!({"id":"restored","client_id":payload.client_id}).to_string(),
    );
    assert_eq!(
        failed(&h, 91).protection,
        Some(safe_cex_engine::error::Protection::Restored)
    );
    assert_eq!(order_posts(&h), 0);
}

#[test]
fn parity_all_valid_batches_still_submit_each_row_once() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:1949-1965.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:423/ten-row-batch-and-serial-placement
    let mut h = private();
    h.receipt_trace.clear();
    let mut order = opts(OrderType::Limit);
    order.client_order_id = None;
    place(&mut h, 91, vec![order; 11]);
    let first = h.take("/orders/batch");
    parity_ack_batch(&mut h, first, "first");
    let last = h.take("/orders/batch");
    parity_ack_batch(&mut h, last, "last");
    let ids: Vec<String> = serde_json::from_str(h.result(91).unwrap()).unwrap();
    assert_eq!(ids.len(), 11);
    assert_eq!(
        ids.iter().collect::<std::collections::HashSet<_>>().len(),
        11
    );
    assert_eq!(
        h.receipt_trace
            .iter()
            .filter(|entry| matches!(entry, ReceiptTrace::Http(_)))
            .count(),
        2
    );
    assert!(h.pending.is_empty());
}

fn after_failure_no_more_writes(h: &Harness) {
    assert!(
        h.pending.iter().all(|r| r.method == Method::Get),
        "{:?}",
        h.pending
    );
}

fn after_failure_refuse(h: &mut Harness, request: safe_cex_engine::HttpRequest, lost: bool) {
    if lost {
        h.e.handle(
            h.now,
            Event::HttpFailed {
                request: request.id,
                error: "fixture transport loss",
            },
        );
        h.collect();
    } else {
        h.respond(
            request,
            400,
            r#"{"error":"VALIDATION","message":"fixture refusal"}"#,
        );
    }
}

#[test]
fn after_failure_native_batch_stops_later_chunks_and_serial_entries() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:1942-1958.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:423/ten-row-batch-and-serial-placement
    // TS paradex.exchange.ts:1942-1958 awaits flush outside the per-entry catch.
    for lost in [false, true] {
        let mut h = private();
        h.receipt_trace.clear();
        let mut order = opts(OrderType::Limit);
        order.client_order_id = None;
        let mut orders = vec![order; 21];
        let mut serial = opts(OrderType::Market);
        serial.client_order_id = Some("unsent-serial".into());
        orders.push(serial);
        place(&mut h, 91, orders);
        let first = h.take("/orders/batch");
        parity_ack_batch(&mut h, first, "first");
        let second = h.take("/orders/batch");
        after_failure_refuse(&mut h, second, lost);
        after_failure_no_more_writes(&h);
        assert_eq!(
            h.receipt_trace
                .iter()
                .filter(|r| matches!(r, ReceiptTrace::Http(_)))
                .count(),
            if lost { 3 } else { 2 }
        );
    }
}

#[test]
fn after_failure_serial_entry_continues_only_after_definite_refusal() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:1958-1965.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:423/ten-row-batch-and-serial-placement
    // TS paradex.exchange.ts:1414-1431,1958-1965 catches one entry, then continues the loop.
    for lost in [false, true] {
        let mut h = private();
        let mut entry = opts(OrderType::Market);
        entry.client_order_id = None;
        entry.take_profit = Some(110.);
        entry.stop_loss = Some(90.);
        let mut next = opts(OrderType::Market);
        next.client_order_id = Some("next".into());
        place(&mut h, 91, vec![entry, next]);
        let first = h.take("/orders");
        after_failure_refuse(&mut h, first, lost);
        if lost {
            after_failure_no_more_writes(&h);
            assert_eq!(
                failed(&h, 91).outcome,
                Some(safe_cex_engine::Outcome::Unknown)
            );
            continue;
        }
        let next = take_method(&mut h, Method::Post, "/orders");
        let payload: Payload = serde_json::from_str(next.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.kind, "MARKET");
        assert_eq!(payload.client_id.as_deref(), Some("next"));
        h.respond(next, 200, r#"{"id":"next","client_id":"next"}"#);
        after_failure_no_more_writes(&h);
    }
}

#[test]
fn after_failure_take_profit_skips_stop_loss_and_continues_only_after_refusal() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:1817-1896.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:532/separate-TP-SL
    // TS paradex.exchange.ts:1817-1896 has one try for TP+SL; 1949-1965 continues entries.
    for lost in [false, true] {
        let mut h = private();
        let mut next = opts(OrderType::Market);
        next.client_order_id = Some("next".into());
        bracket_entry(&mut h, 91, Some(next));
        let tp = h.take("/orders");
        let payload: Payload = serde_json::from_str(tp.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.kind, "TAKE_PROFIT_MARKET");
        after_failure_refuse(&mut h, tp, lost);
        if lost {
            after_failure_no_more_writes(&h);
            assert_eq!(
                failed(&h, 91).outcome,
                Some(safe_cex_engine::Outcome::Unknown)
            );
            continue;
        }
        let next = take_method(&mut h, Method::Post, "/orders");
        let payload: Payload = serde_json::from_str(next.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.kind, "MARKET");
        assert_eq!(payload.client_id.as_deref(), Some("next"));
        h.respond(next, 200, r#"{"id":"next","client_id":"next"}"#);
        after_failure_no_more_writes(&h);
    }
}

#[test]
fn after_failure_protective_cancel_or_exact_restore_is_terminal() {
    // Legacy TS: packages/safe-cex/src/__tests__/reference/paradex/paradex.exchange.ts:2105-2113; historical 31d7fda4:1427-1438.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:1785/protective-amend-and-put-back-gating
    // Keep the existing Rust exact put-back per the lead's explicit no-removal instruction.
    for cancel_failure in [true, false] {
        let mut h = with_untriggered_stop();
        amend_stop(&mut h, 91);
        let cancel = take_method(&mut h, Method::Delete, "/orders/123");
        if cancel_failure {
            after_failure_refuse(&mut h, cancel, false);
        } else {
            h.respond(cancel, 204, "");
            confirm_stop_cancel(&mut h);
            let replacement = h.take("/orders");
            after_failure_refuse(&mut h, replacement, false);
            let restore = h.take("/orders");
            let payload: Payload = serde_json::from_str(restore.body.as_deref().unwrap()).unwrap();
            assert_eq!(payload.size.as_deref(), Some("2"));
            assert_eq!(payload.trigger_price.as_deref(), Some("90"));
            after_failure_refuse(&mut h, restore, false);
        }
        assert!(h.result(91).is_err());
        after_failure_no_more_writes(&h);
    }
}

#[test]
fn pin_native_batch_row_refusal_keeps_accepted_rows_but_stops_the_next_chunk() {
    // Legacy TS: paradex.exchange.ts:1772-1780,1942-1958 throws on a per-row batch error.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:423/ten-row-batch-and-serial-placement
    let mut h = private();
    h.receipt_trace.clear();
    let mut order = opts(OrderType::Limit);
    order.client_order_id = None;
    place(&mut h, 91, vec![order; 11]);
    let batch = h.take("/orders/batch");
    let rows: Vec<serde_json::Value> =
        serde_json::from_str(batch.body.as_deref().unwrap()).unwrap();
    assert_eq!(rows.len(), 10);
    let accepted: Vec<_> = rows[..9].iter().enumerate().map(|(i,row)| serde_json::json!({"id":format!("accepted-{i}"),"client_id":row["client_id"]})).collect();
    h.respond(batch, 200, &serde_json::json!({"orders":accepted,"errors":[{"error":"INVALID_PRICE","message":"last row refused"}]}).to_string());
    assert_batch_refusal(
        &h,
        91,
        (0..9).map(|index| format!("accepted-{index}")).collect(),
        "INVALID_PRICE",
    );
    after_failure_no_more_writes(&h);
    assert_eq!(
        h.receipt_trace
            .iter()
            .filter(|r| matches!(r, ReceiptTrace::Http(_)))
            .count(),
        1
    );
}

#[test]
fn pin_native_amend_refusal_stops_without_replacing_or_restoring() {
    // Legacy TS: paradex.exchange.ts:2099-2113 PUT failure is rethrown without a fallback.
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:1785/protective-amend-and-put-back-gating
    let (mut h, _) = amendment_account(AMEND_ROW);
    h.receipt_trace.clear();
    amend(&mut h, 91, Some(101.), Some(3.));
    let request = take_method(&mut h, Method::Put, "/orders/123");
    after_failure_refuse(&mut h, request, false);
    assert!(h.result(91).is_err());
    after_failure_no_more_writes(&h);
    assert_eq!(
        h.receipt_trace
            .iter()
            .filter(|r| matches!(r, ReceiptTrace::Http(_)))
            .count(),
        1
    );
}

#[test]
fn unknown_exception_serial_entry_halts_the_action_with_accepted_ids() {
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:423/ten-row-batch-and-serial-placement
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at paradex.exchange.ts:1958-1965.
    for lost in [false, true] {
        let mut h = private();
        let orders = ["accepted", "uncertain", "unsent"].map(|id| {
            let mut order = opts(OrderType::Market);
            order.client_order_id = Some(id.into());
            order
        });
        place(&mut h, 91, orders.into());
        h.reply("/orders", r#"{"id":"accepted-id","client_id":"accepted"}"#);
        let second = take_method(&mut h, Method::Post, "/orders");
        if lost {
            after_failure_refuse(&mut h, second, true);
        } else {
            h.respond(second, 200, "{}");
        }
        after_failure_no_more_writes(&h);
        let error = failed(&h, 91);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["accepted-id"]);
        assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
        unknown_exception_journal_and_telemetry(&h);
    }
}

#[test]
fn unknown_exception_child_halts_later_children_and_independent_entries() {
    // KNOWN-ISSUE: see ~/agent-reports/known-issues-for-sam.txt row: paradex | trading.rs:532/separate-TP-SL
    // PARITY-EXCEPTION: stop-after-UNKNOWN (Sean/daxx, 2026-11-08); TS continues at paradex.exchange.ts:1844-1850,1879-1898,1949-1965.
    for lost in [false, true] {
        let mut h = private();
        let mut next = opts(OrderType::Market);
        next.client_order_id = Some("unsent".into());
        bracket_entry(&mut h, 91, Some(next));
        let tp = take_method(&mut h, Method::Post, "/orders");
        if lost {
            after_failure_refuse(&mut h, tp, true);
        } else {
            h.respond(tp, 200, "{}");
        }
        after_failure_no_more_writes(&h);
        let error = failed(&h, 91);
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
        assert_eq!(error.partial.as_ref().unwrap().order_ids, ["entry"]);
        assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
        unknown_exception_journal_and_telemetry(&h);
    }
}

fn unknown_exception_journal_and_telemetry(h: &Harness) {
    let error = failed(h, 91);
    assert!(error.message.contains(ENGINE_UNKNOWN_TEXT), "{error:?}");
    let event = h
        .actions
        .iter()
        .find_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == "order.outcome_unknown" =>
            {
                Some(event)
            }
            _ => None,
        })
        .expect("Unknown journal telemetry");
    assert!(event.message.contains(ENGINE_UNKNOWN_TEXT));
    assert!(
        event
            .fields
            .iter()
            .any(|(key, value)| *key == "outcome" && value.as_str() == Some("unknown"))
    );
    let report = event
        .fields
        .iter()
        .find_map(|(key, value)| (*key == "journal").then(|| value.as_str()).flatten())
        .expect("journal snapshot");
    let report: serde_json::Value = serde_json::from_str(report).unwrap();
    assert_eq!(
        report["placed"],
        serde_json::json!(error.partial.as_ref().unwrap().order_ids)
    );
    let unknown = report["unknown"].as_array().unwrap();
    assert_eq!(unknown.len(), 1);
    assert!(error.message.contains(unknown[0].as_str().unwrap()));
    assert!(!report["notSent"].as_array().unwrap().is_empty());
}
