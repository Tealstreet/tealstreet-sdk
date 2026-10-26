//! Trading contracts against documented request/response wire shapes.
mod support;
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, TimeInForce};
use safe_cex_engine::{Action, Command, Event, Method, OrderIdentityKind, OrderIdentityReceipt};
use safe_cex_venue_paradex::Options;
use serde::Deserialize;
use support::*;
fn private() -> Harness {
    let mut h = Harness::empty(Options {
        key: Some(format!("0x{}1", "0".repeat(63))),
        paradex_address: Some("0x0123".into()),
        subkey_address: Some("0x456".into()),
        ..Options::default()
    });
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
fn partial_bracket_failure_returns_the_accepted_entry_and_does_not_replay_it() {
    let mut h = private();
    bracket_entry(&mut h, 10, None);
    let tp = h.take("/orders");
    h.respond(tp, 400, r#"{"error":"REJECTED"}"#);
    let sl = h.take("/orders");
    let payload: Payload = serde_json::from_str(sl.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "STOP_LOSS_MARKET");
    assert_eq!(payload.flags, Some(vec!["REDUCE_ONLY".into()]));
    h.respond(sl, 200, r#"{"id":"sl"}"#);
    assert!(h.pending.is_empty());
    assert_eq!(h.result(10), Ok("[\"entry\",\"sl\"]"));
    assert!(logged_child_rejection(
        &h,
        "TP/SL take-profit placement failed: Paradex HTTP 400: REJECTED; still placing the stop-loss"
    ));
}
#[test]
fn a_rejected_stop_loss_keeps_the_take_profit_and_the_next_order() {
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
    assert_eq!(h.result(11), Ok("[\"entry\",\"tp\",\"next-id\"]"));
    assert!(logged_child_rejection(
        &h,
        "TP/SL stop-loss placement failed: Paradex HTTP 400: REJECTED; no other leg remains to send"
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
fn a_failed_entry_skips_only_its_children_then_attempts_the_next_entry() {
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
        let next = h.take("/orders");
        let payload: Payload = serde_json::from_str(next.body.as_deref().unwrap()).unwrap();
        assert_eq!(payload.kind, "MARKET");
        assert_eq!(payload.client_id.as_deref(), Some("next"));
        h.respond(next, 200, r#"{"id":"accepted-next","client_id":"next"}"#);
        assert_eq!(h.result(71), Ok("[\"accepted-next\"]"));
        assert!(h.pending.is_empty());
    }
}

#[test]
fn a_mixed_native_batch_returns_correlated_successes_and_reports_missing_clients() {
    let mut h = private();
    let orders = ["first", "second", "third"].map(|id| {
        let mut order = opts(OrderType::Limit);
        order.client_order_id = Some(id.into());
        order
    });
    place(&mut h, 72, orders.into());
    h.reply("/orders/batch", r#"{"orders":[{"id":"three","client_id":"third"},{"id":"one","client_id":"first"}],"errors":[{"error":"INVALID_PRICE","message":"outside price bands"}]}"#);
    assert_eq!(h.result(72), Ok("[\"one\",\"three\"]"));
    assert!(h.actions.iter().any(|action| matches!(action,
        Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent { level: safe_cex_engine::Level::Error, message, .. }))
        if message.contains("second") && message.contains("outside price bands"))));
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
fn untriggered_stop_with_null_execution_price_can_be_amended_without_cancel_replace() {
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
    let request = h.take("/orders/123");
    assert_eq!(request.method, Method::Put);
    let payload: Payload = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.price.as_deref(), Some("0"));
    assert_eq!(payload.trigger_price.as_deref(), Some("91"));
    assert_eq!(payload.size.as_deref(), Some("3"));
    assert!(payload.signature.unwrap().starts_with("[\""));
    h.respond(request, 200, r#"{"id":"123"}"#);
    assert_eq!(h.result(13), Ok("[\"123\"]"));
    h.command(Command::AmendOrder {
        command: 130,
        id: "123".into(),
        price: None,
        amount: Some(4.),
    });
    assert!(h.result(130).is_err());
    assert!(h.pending.is_empty());
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
fn post_ack_size_and_trigger_changes_require_all_exact_mutable_attributes() {
    for trigger in [false, true] {
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
fn write_failures_are_classified_without_changing_their_text() {
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
            (Some(ErrorKind::RateLimited), None),
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
        assert_eq!(h.result(8), Err(text), "{status} {body}");
        let error = &h.failures[0];
        assert_eq!((error.kind, error.outcome), classified, "{status} {body}");
    }
    // A malformed 2xx acknowledgement does not prove what happened.
    let mut h = private();
    place(&mut h, 8, vec![opts(OrderType::Limit)]);
    let request = h.take("/orders");
    h.respond(request, 200, "{}");
    assert_eq!(h.failures[0].outcome, Some(Outcome::Unknown));
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
        Action::SignPayload { .. } | Action::Notice(safe_cex_engine::Notice::AccountAuth { .. })
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
    assert!(!h.result(10).is_err_and(|e| e.contains("uncertain")));
    h.respond(fresh, 200, r#"{"results":[]}"#);
    h.command(Command::CancelSymbolOrders {
        command: 11,
        symbol: "BTC-USD-PERP".into(),
    });
    take_method(&mut h, Method::Delete, "/orders?market=");
}
