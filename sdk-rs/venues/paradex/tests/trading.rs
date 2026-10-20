//! Trading contracts against documented request/response wire shapes.
mod support;
use safe_cex_engine::model::{OrderSide, OrderType, PlaceOrderOpts, TimeInForce};
use safe_cex_engine::{Command, Event, Method};
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
    assert!(quote.url.ends_with("/orderbook/BTC-USD-PERP?depth=5"));
    assert!(!quote.headers.iter().any(|(n, _)| *n == "Authorization"));
    h.respond(quote, 200, r#"{"bids":[["102","1"]],"asks":[["103","2"]]}"#);
    let request = h.take("/orders");
    let payload: Payload = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "LIMIT");
    assert_eq!(payload.price.as_deref(), Some("103"));
    assert_eq!(payload.instruction.as_deref(), Some("IOC"));
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
    assert!(h.pending.is_empty());
    assert!(h.result(8).is_err());
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
    h.respond(tp, 200, r#"{"id":"tp","client_id":""}"#);
    let sl = h.take("/orders");
    let payload: Payload = serde_json::from_str(sl.body.as_deref().unwrap()).unwrap();
    assert_eq!(payload.kind, "STOP_LOSS_MARKET");
    h.respond(sl, 200, r#"{"id":"sl"}"#);
    assert_eq!(h.result(9), Ok("[\"entry\",\"tp\",\"sl\"]"));
}
#[test]
fn partial_bracket_failure_returns_the_accepted_entry_and_does_not_replay_it() {
    let mut h = private();
    let mut order = opts(OrderType::Limit);
    order.client_order_id = None;
    order.take_profit = Some(110.);
    order.stop_loss = Some(90.);
    place(&mut h, 10, vec![order]);
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
    let tp = h.take("/orders");
    h.respond(tp, 400, r#"{"error":"REJECTED"}"#);
    assert!(h.pending.is_empty());
    assert_eq!(h.result(10), Ok("[\"entry\"]"));
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
