//! DX REST section 6 and 9.1 native write contracts, using synthetic sessions.
mod support;
use safe_cex_engine::model::*;
use safe_cex_engine::{Command, HttpRequest, Method, Notice};
use safe_cex_venue_breakoutprop::Options;
use serde_json::json;
use support::*;

const EMPTY_PORTFOLIO: &str = r#"{"portfolios":[{"account":"default:00002","version":1,"balances":[],"positions":[],"orders":[]}]}"#;

fn private() -> Rig {
    private_with_interval(1_000_000.0)
}
fn private_with_interval(tick_interval: f64) -> Rig {
    let mut rig = Rig::new(Options {
        key: "example-user".into(),
        secret: "example-password".into(),
        tick_interval,
        ..Options::default()
    });
    rig.boot();
    for _ in 0..2 {
        let portfolio = rig.next_request();
        assert!(portfolio.url.ends_with("/portfolio"), "{}", portfolio.url);
        rig.reply(&portfolio, 200, EMPTY_PORTFOLIO);
    }
    let quotes = rig.next_request();
    assert!(quotes.url.ends_with("/marketdata"));
    rig.reply(&quotes, 200, QUOTES);
    rig
}

fn order(value: serde_json::Value) -> PlaceOrderOpts {
    serde_json::from_value(value).unwrap()
}
fn limit() -> PlaceOrderOpts {
    order(json!({"symbol":"ETHUSD","type":"limit","side":"buy","amount":1.235,"price":3656.015}))
}
fn place(rig: &mut Rig, command: u32, orders: Vec<PlaceOrderOpts>) {
    rig.command(Command::PlaceOrders { command, orders });
}
fn open() -> Order {
    Order {
        id: "816099:5".into(),
        parent_id: None,
        status: OrderStatus::Open,
        symbol: "ETHUSD".into(),
        kind: OrderType::Limit,
        side: OrderSide::Buy,
        price: 3656.01,
        amount: 1.0,
        filled: 0.0,
        remaining: 1.0,
        reduce_only: false,
        close: None,
        client_order_id: None,
        time_in_force: Some(TimeInForce::GoodTillCancel),
        last_update: 1_000.0,
        trigger_type: None,
        take_profit: None,
        stop_loss: None,
        extra: None,
    }
}
fn amend(rig: &mut Rig, command: u32) {
    rig.command(Command::AmendOrder {
        command,
        id: "816099:5".into(),
        price: Some(3660.0),
        amount: None,
    });
}
fn body(request: &HttpRequest) -> serde_json::Value {
    serde_json::from_str(request.body.as_deref().unwrap()).unwrap()
}
fn done_count(rig: &Rig, id: u32) -> usize {
    rig.notices
        .iter()
        .filter(|notice| matches!(notice, Notice::CommandDone { command, .. } | Notice::CommandFailed { command, .. } if *command == id))
        .count()
}

fn portfolio_with_order(code: &str, client: &str, version: u64) -> String {
    json!({"portfolios":[{"account":"default:00002","version":version,
        "balances":[{"account":"default:00002","currency":"USD","value":500}],
        "positions":[],"orders":[{"account":"default:00002","orderCode":code,
            "clientOrderId":client,"instrument":"ETHUSD","type":"LIMIT","status":"WORKING",
            "finalStatus":false,"transactionTime":"2024-12-11T07:55:11Z",
            "legs":[{"instrument":"ETHUSD","side":"BUY","positionEffect":"OPEN",
                "price":3656.01,"quantity":1,"filledQuantity":0,"remainingQuantity":1}]}]}]})
    .to_string()
}

fn cancellation_history(filled: f64) -> serde_json::Value {
    json!({"orders":[{"account":"default:00002","orderId":46073148,"orderCode":"816099:5",
        "instrument":"ETHUSD","status":"CANCELED","finalStatus":true,
        "legs":[{"instrument":"ETHUSD","side":"BUY","positionEffect":"OPEN","quantity":1,
            "filledQuantity":filled,"remainingQuantity":0}]}]})
}

fn zero_close_order(kind: OrderType) -> Order {
    Order {
        kind,
        amount: 0.0,
        filled: 0.0,
        remaining: 0.0,
        reduce_only: true,
        side: OrderSide::Sell,
        ..open()
    }
}

fn zero_close_history() -> serde_json::Value {
    let mut history = cancellation_history(0.0);
    history["orders"][0]["legs"][0]["quantity"] = json!(0);
    history["orders"][0]["legs"][0]["positionEffect"] = json!("CLOSE");
    history["orders"][0]["legs"][0]["side"] = json!("SELL");
    history
}

#[test]
fn native_limit_payload_rounds_steps_generates_attributed_code_and_uses_close() {
    let mut rig = private();
    let mut opts = limit();
    opts.reduce_only = Some(true);
    place(&mut rig, 10, vec![opts]);
    let request = rig.next_request();
    assert_eq!(request.method, Method::Post);
    assert_eq!(
        request.url,
        "https://breakout.prosp.devexperts.com/dxsca-web/accounts/default%3A00002/orders"
    );
    let mut native = body(&request);
    let code = native["orderCode"].as_str().unwrap();
    assert!(code.starts_with("tealstreet-"));
    assert!(
        code.strip_prefix("tealstreet-")
            .unwrap()
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric())
    );
    native.as_object_mut().unwrap().remove("orderCode");
    assert_eq!(
        native,
        json!({"type":"LIMIT","instrument":"ETHUSD","quantity":1.24,"side":"BUY","limitPrice":3656.02,"positionEffect":"CLOSE","tif":"GTC"})
    );
    rig.reply(
        &request,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073148}"#,
    );
    assert_eq!(rig.result(10), Ok("[\"46073148\"]".into()));
}

#[test]
fn placement_batch_is_sequential_and_failure_never_starts_next_order() {
    let mut rig = private();
    place(&mut rig, 10, vec![limit(), limit()]);
    let first = rig.next_request();
    assert!(rig.requests.is_empty());
    rig.reply(&first, 409, r#"{"errorCode":33,"description":"Rejected"}"#);
    assert!(rig.result(10).is_err());
    assert!(rig.requests.is_empty());
    assert_eq!(done_count(&rig, 10), 1);
}

#[test]
fn whole_batch_preflight_rejects_late_caller_ids_and_missing_prices_before_write() {
    for invalid in [
        json!({"symbol":"ETHUSD","type":"market","side":"buy","amount":1,"clientOrderId":"external"}),
        json!({"symbol":"ETHUSD","type":"limit","side":"buy","amount":1}),
        json!({"symbol":"ETHUSD","type":"stop_market","side":"buy","amount":1}),
        json!({"symbol":"ETHUSD","type":"trailing_stop_market","side":"buy","amount":1}),
        json!({"symbol":"ETHUSD","type":"market","side":"buy","amount":1,"stopLoss":3500}),
        json!({"symbol":"ETHUSD","type":"market","side":"buy","amount":1,"triggerPrice":3500}),
        json!({"symbol":"ETHUSD","type":"market","side":"buy","amount":1,"close":true}),
        json!({"symbol":"ETHUSD","type":"stop_market","side":"buy","amount":1,"triggerPrice":3500,"triggerType":"mark_price"}),
    ] {
        let mut rig = private();
        place(&mut rig, 10, vec![limit(), order(invalid)]);
        assert!(rig.result(10).is_err());
        assert!(rig.requests.is_empty());
    }
}

#[test]
fn native_cancel_accepts_external_order_codes_without_store_and_is_conditional() {
    let mut rig = private();
    rig.command(Command::CancelOrders {
        command: 10,
        ids: vec!["816099:5/unsafe?".into()],
    });
    let request = rig.next_request();
    assert_eq!(request.method, Method::Delete);
    assert!(request.url.ends_with("/orders/816099%3A5%2Funsafe%3F"));
    assert!(request.headers.contains(&("If-Match", "*".into())));
    assert!(request.body.is_none());
    rig.reply(
        &request,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    assert!(rig.result(10).is_ok());
}

#[test]
fn ack_returned_numeric_id_cancels_through_generated_client_code_before_store() {
    let mut rig = private();
    place(&mut rig, 10, vec![limit()]);
    let placement = rig.next_request();
    let code = body(&placement)["orderCode"].as_str().unwrap().to_owned();
    rig.reply(&placement, 200, r#"{"orderId":42}"#);
    rig.command(Command::CancelOrders {
        command: 11,
        ids: vec!["42".into()],
    });
    let cancellation = rig.next_request();
    assert!(cancellation.url.ends_with(&format!("/orders/{code}")));
    rig.reply(&cancellation, 200, r#"{"orderId":42,"updateOrderId":43}"#);
    assert!(rig.result(11).is_ok());
}

#[test]
fn placement_ack_fences_an_older_inflight_portfolio_transaction() {
    let mut rig = private_with_interval(2_000.0);
    rig.engine.store_mut().set_orders(vec![open()]);
    let stale = rig.next_request();
    assert!(stale.url.ends_with("/portfolio"));
    place(&mut rig, 10, vec![limit()]);
    let placement = rig.next_request();
    assert_eq!(placement.method, Method::Post);
    rig.reply(&placement, 200, r#"{"orderId":42}"#);
    rig.reply(
        &stale,
        200,
        &EMPTY_PORTFOLIO.replace(
            "\"balances\":[]",
            "\"balances\":[{\"account\":\"default:00002\",\"currency\":\"USD\",\"value\":500}]",
        ),
    );
    assert_eq!(rig.engine.store().balance().total, 0.0);
    assert_eq!(rig.engine.store().orders().len(), 1);
    assert_eq!(rig.engine.store().orders()[0].id, "816099:5");
}

#[test]
fn confirmed_cancel_fences_pending_portfolios_and_filters_fresh_native_or_client_aliases() {
    for snapshot_code in ["816099:5", "another-native-code"] {
        let mut rig = private_with_interval(2_000.0);
        let mut existing = open();
        existing.client_order_id = Some("tealstreet-alias".into());
        rig.engine.store_mut().set_orders(vec![existing]);
        let stale = rig.next_request();
        assert!(stale.url.ends_with("/portfolio"));
        rig.command(Command::CancelOrders {
            command: 10,
            ids: vec!["816099:5".into()],
        });
        let cancellation = rig.next_request();
        assert_eq!(cancellation.method, Method::Delete);
        rig.reply(&cancellation, 200, r#"{"orderId":42,"updateOrderId":43}"#);
        rig.reply(
            &stale,
            200,
            &portfolio_with_order("816099:5", "tealstreet-alias", 999),
        );
        assert_eq!(rig.engine.store().orders()[0].status, OrderStatus::Canceled);
        assert_eq!(rig.engine.store().balance().total, 0.0);
        let quotes = rig.next_request();
        assert!(quotes.url.ends_with("/marketdata"));
        rig.reply(&quotes, 200, QUOTES);
        let fresh = rig.next_request();
        assert!(fresh.url.ends_with("/portfolio"));
        rig.reply(
            &fresh,
            200,
            &portfolio_with_order(snapshot_code, "tealstreet-alias", 1),
        );
        assert_eq!(rig.engine.store().balance().total, 500.0);
        assert!(rig.engine.store().orders().is_empty());
    }
}

#[test]
fn cancel_symbol_includes_acknowledged_orders_before_store_snapshot() {
    let mut rig = private();
    place(&mut rig, 10, vec![limit()]);
    let placement = rig.next_request();
    let code = body(&placement)["orderCode"].as_str().unwrap().to_owned();
    rig.reply(&placement, 200, r#"{"orderId":46073148}"#);
    rig.command(Command::CancelSymbolOrders {
        command: 11,
        symbol: "ETHUSD".into(),
    });
    let cancellation = rig.next_request();
    assert!(cancellation.url.ends_with(&format!("/orders/{code}")));
    rig.reply(&cancellation, 200, r#"{"orderId":46073148}"#);
    assert!(rig.result(11).is_ok());
}

#[test]
fn cancellation_batches_wait_for_every_current_result_and_stop_after_failed_batch() {
    let mut rig = private();
    rig.command(Command::CancelOrders {
        command: 10,
        ids: (0..11).map(|i| format!("external-{i}")).collect(),
    });
    let requests: Vec<_> = (0..10).map(|_| rig.next_request()).collect();
    rig.reply(&requests[0], 409, r#"{"errorCode":33}"#);
    assert_eq!(done_count(&rig, 10), 0);
    for request in &requests[1..9] {
        rig.reply(request, 200, "{}");
    }
    assert_eq!(done_count(&rig, 10), 0);
    rig.reply(&requests[9], 200, "{}");
    assert!(rig.result(10).is_err());
    assert_eq!(done_count(&rig, 10), 1);
    assert!(rig.requests.is_empty());
}

#[test]
fn cancellation_next_batch_begins_after_all_first_ten_complete() {
    let mut rig = private();
    rig.command(Command::CancelOrders {
        command: 10,
        ids: (0..11).map(|i| format!("external-{i}")).collect(),
    });
    let requests: Vec<_> = (0..10).map(|_| rig.next_request()).collect();
    for request in &requests[..9] {
        rig.reply(request, 200, "{}");
    }
    assert!(rig.requests.is_empty());
    rig.reply(&requests[9], 200, "{}");
    let last = rig.next_request();
    assert!(last.url.ends_with("/external-10"));
    rig.reply(&last, 200, "{}");
    assert!(rig.result(10).is_ok());
}

#[test]
fn amendment_cancels_before_replacement_and_rejects_concurrent_same_order_mutations() {
    let mut rig = private();
    rig.engine.store_mut().set_orders(vec![open()]);
    amend(&mut rig, 10);
    let cancel = rig.next_request();
    assert_eq!(cancel.method, Method::Delete);
    assert!(
        !rig.notices
            .iter()
            .any(|n| matches!(n, Notice::CommandProgress { command: 10, .. }))
    );
    amend(&mut rig, 11);
    assert!(rig.result(11).is_err());
    assert!(rig.requests.is_empty());
    rig.reply(
        &cancel,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    let confirmation = rig.next_request();
    assert_eq!(confirmation.method, Method::Get);
    assert!(
        confirmation
            .url
            .ends_with("/orders/history?with-order-id=46073148&limit=2")
    );
    amend(&mut rig, 13);
    assert!(rig.result(13).is_err());
    rig.reply(&confirmation, 200, &cancellation_history(0.0).to_string());
    let replacement = rig.next_request();
    assert_eq!(replacement.method, Method::Post);
    assert_order_code_receipt(&rig, &replacement, 10);
    assert_eq!(body(&replacement)["limitPrice"], 3660.0);
    rig.reply(&replacement, 200, r#"{"orderId":46073150}"#);
    assert_eq!(rig.result(10), Ok("[\"46073150\"]".into()));
    amend(&mut rig, 12);
    assert!(rig.result(12).is_err());
    assert!(rig.requests.is_empty());
}

#[test]
fn native_fill_after_last_portfolio_blocks_replacement_even_when_store_still_says_unfilled() {
    let mut rig = private();
    rig.engine.store_mut().set_orders(vec![open()]);
    amend(&mut rig, 10);
    let cancellation = rig.next_request();
    rig.reply(
        &cancellation,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    let confirmation = rig.next_request();
    assert_eq!(confirmation.method, Method::Get);
    rig.reply(&confirmation, 200, &cancellation_history(0.25).to_string());
    assert!(
        !rig.notices
            .iter()
            .any(|n| matches!(n, Notice::CommandProgress { command: 10, .. }))
    );
    assert!(rig.result(10).is_err());
    assert!(rig.requests.is_empty());
}

#[test]
fn missing_ambiguous_unmatched_or_nonterminal_native_history_never_authorizes_replacement() {
    let mut cases = vec![json!({"orders":[]})];
    let mut duplicate = cancellation_history(0.0);
    let duplicate_order = duplicate["orders"][0].clone();
    duplicate["orders"]
        .as_array_mut()
        .unwrap()
        .push(duplicate_order);
    cases.push(duplicate);
    for (path, value) in [
        ("account", json!("default:other")),
        ("orderId", json!(42)),
        ("orderCode", json!("other-code")),
        ("instrument", json!("BTCUSD")),
        ("status", json!("WORKING")),
        ("status", json!("COMPLETED")),
        ("finalStatus", json!(false)),
    ] {
        let mut case = cancellation_history(0.0);
        case["orders"][0][path] = value;
        cases.push(case);
    }
    let mut quantity = cancellation_history(0.0);
    quantity["orders"][0]["legs"][0]["quantity"] = json!(2);
    cases.push(quantity);
    let mut paged = cancellation_history(0.0);
    paged["nextPageTransactionTime"] = json!("2024-12-11T07:55:11Z");
    cases.push(paged);
    for case in cases {
        let mut rig = private();
        rig.engine.store_mut().set_orders(vec![open()]);
        amend(&mut rig, 10);
        let cancellation = rig.next_request();
        rig.reply(
            &cancellation,
            200,
            r#"{"orderId":46073148,"updateOrderId":46073149}"#,
        );
        let confirmation = rig.next_request();
        rig.reply(&confirmation, 200, &case.to_string());
        assert!(rig.result(10).is_err(), "{case}");
        assert!(rig.requests.is_empty(), "{case}");
    }
}

#[test]
fn failed_native_cancellation_history_read_does_not_place_replacement() {
    let mut rig = private();
    rig.engine.store_mut().set_orders(vec![open()]);
    amend(&mut rig, 10);
    let cancellation = rig.next_request();
    rig.reply(
        &cancellation,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    let confirmation = rig.next_request();
    rig.reply(&confirmation, 503, "{}");
    assert!(rig.result(10).is_err());
    assert!(rig.requests.is_empty());
    // The cancel took effect and the replacement was never sent.
    assert_eq!(rig.failures[0].outcome, None);
}

#[test]
fn history_confirmation_can_retry_a_read_without_repeating_the_cancel_write() {
    let mut rig = private();
    rig.engine.store_mut().set_orders(vec![open()]);
    amend(&mut rig, 10);
    let cancellation = rig.next_request();
    rig.reply(
        &cancellation,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    let confirmation = rig.next_request();
    rig.failure(&confirmation);
    let retry = rig.next_request();
    assert_eq!(retry.method, Method::Get);
    assert_eq!(retry.url, confirmation.url);
    rig.reply(&retry, 200, &cancellation_history(0.0).to_string());
    let replacement = rig.next_request();
    assert_eq!(replacement.method, Method::Post);
    rig.reply(&replacement, 200, r#"{"orderId":43}"#);
    assert!(rig.result(10).is_ok());
}

#[test]
fn all_order_cancellation_includes_multiple_symbol_acks_before_store_sees_them() {
    let mut rig = private();
    let mut markets = rig.engine.store().markets().to_vec();
    let mut btc = markets[0].clone();
    btc.id = "BTCUSD".into();
    btc.symbol = "BTCUSD".into();
    btc.base = "BTC".into();
    markets.push(btc);
    rig.engine.store_mut().set_markets(markets);
    let eth = limit();
    let mut btc = limit();
    btc.symbol = "BTCUSD".into();
    place(&mut rig, 10, vec![eth, btc]);
    let first = rig.next_request();
    let first_code = body(&first)["orderCode"].as_str().unwrap().to_owned();
    rig.reply(&first, 200, r#"{"orderId":42}"#);
    let second = rig.next_request();
    let second_code = body(&second)["orderCode"].as_str().unwrap().to_owned();
    rig.reply(&second, 200, r#"{"orderId":43}"#);
    assert!(rig.engine.store().orders().is_empty());
    rig.command(Command::CancelSymbolOrders {
        command: 11,
        symbol: String::new(),
    });
    let cancellations = [rig.next_request(), rig.next_request()];
    assert!(
        cancellations
            .iter()
            .any(|request| request.url.ends_with(&first_code))
    );
    assert!(
        cancellations
            .iter()
            .any(|request| request.url.ends_with(&second_code))
    );
    for request in cancellations {
        rig.reply(&request, 200, "{}");
    }
    assert!(rig.result(11).is_ok());
}

#[test]
fn unknown_order_or_network_failure_during_amendment_never_places_replacement() {
    for network_failure in [false, true] {
        let mut rig = private();
        rig.engine.store_mut().set_orders(vec![open()]);
        amend(&mut rig, 10);
        let cancellation = rig.next_request();
        if network_failure {
            rig.failure(&cancellation);
        } else {
            rig.reply(
                &cancellation,
                404,
                r#"{"errorCode":2,"description":"Entity not found at server"}"#,
            );
        }
        assert!(rig.result(10).is_err());
        assert!(rig.requests.is_empty());
    }
}

#[test]
fn malformed_cancel_acknowledgement_does_not_authorize_replacement() {
    for body in ["", "{}", r#"{"errorCode":2}"#, "<html>login</html>"] {
        let mut rig = private();
        rig.engine.store_mut().set_orders(vec![open()]);
        amend(&mut rig, 10);
        let cancellation = rig.next_request();
        rig.reply(&cancellation, 200, body);
        assert!(rig.result(10).is_err());
        assert!(rig.requests.is_empty());
    }
}

#[test]
fn partial_fill_before_amendment_fails_without_cancel() {
    let mut rig = private();
    let mut existing = open();
    existing.filled = 0.25;
    existing.remaining = 0.75;
    rig.engine.store_mut().set_orders(vec![existing]);
    amend(&mut rig, 10);
    assert!(rig.result(10).is_err());
    assert!(rig.requests.is_empty());
}

#[test]
fn known_fill_while_cancel_inflight_blocks_replacement() {
    let mut rig = private();
    rig.engine.store_mut().set_orders(vec![open()]);
    amend(&mut rig, 10);
    let cancellation = rig.next_request();
    let mut existing = open();
    existing.filled = 0.25;
    existing.remaining = 0.75;
    rig.engine.store_mut().set_orders(vec![existing]);
    rig.reply(
        &cancellation,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    assert!(rig.result(10).is_err());
    assert!(rig.requests.is_empty());
}

#[test]
fn removed_store_row_requires_native_terminal_proof_before_replacement() {
    let mut rig = private();
    rig.engine.store_mut().set_orders(vec![open()]);
    amend(&mut rig, 10);
    let cancellation = rig.next_request();
    rig.engine.store_mut().set_orders(vec![]);
    rig.reply(
        &cancellation,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    let confirmation = rig.next_request();
    assert_eq!(confirmation.method, Method::Get);
    rig.reply(&confirmation, 200, &cancellation_history(0.0).to_string());
    let replacement = rig.next_request();
    assert_eq!(replacement.method, Method::Post);
    rig.reply(&replacement, 200, r#"{"orderId":43}"#);
    assert!(rig.result(10).is_ok());
}

#[test]
fn placement_transport_failure_is_terminal_and_does_not_repeat_write() {
    let mut rig = private();
    place(&mut rig, 10, vec![limit()]);
    let request = rig.next_request();
    rig.failure(&request);
    assert!(rig.result(10).is_err());
    assert!(rig.requests.is_empty());
}

#[test]
fn explicit_first_401_retries_same_order_code_once_after_session_refresh() {
    let mut rig = private();
    place(&mut rig, 10, vec![limit()]);
    let request = rig.next_request();
    rig.reply(&request, 401, r#"{"errorCode":1}"#);
    let login = rig.next_request();
    assert!(login.url.ends_with("/login"));
    rig.reply(
        &login,
        200,
        r#"{"sessionToken":"fresh-session","timeout":"00:30:00"}"#,
    );
    let retry = rig.next_request();
    assert_eq!(retry.body, request.body);
    assert_eq!(retry.url, request.url);
    assert!(
        retry
            .headers
            .contains(&("Authorization", "DXAPI fresh-session".into()))
    );
    rig.reply(&retry, 401, r#"{"errorCode":1}"#);
    assert!(rig.result(10).is_err());
    assert!(rig.requests.is_empty());
}

#[test]
fn public_only_and_fixed_account_setting_writes_fail_without_requests() {
    let mut public = Rig::defaults();
    public.boot();
    place(&mut public, 1, vec![limit()]);
    assert!(public.result(1).is_err());
    public.command(Command::CancelOrders {
        command: 2,
        ids: vec!["external".into()],
    });
    assert!(public.result(2).is_err());
    assert!(public.requests.is_empty());
    let mut private = private();
    for command in [
        Command::SetLeverage {
            command: 1,
            symbol: "ETHUSD".into(),
            leverage: 5.0,
            side: None,
        },
        Command::SetMarginType {
            command: 2,
            symbol: "ETHUSD".into(),
            isolated: false,
        },
        Command::SetPositionMode {
            command: 3,
            hedged: false,
            symbol: None,
        },
    ] {
        private.command(command);
    }
    for id in 1..=3 {
        assert!(private.result(id).is_err());
    }
    assert!(private.requests.is_empty());
}

#[test]
fn explicit_zero_close_quantity_is_forwarded_while_zero_open_and_rounded_zero_are_rejected() {
    let mut rig = private();
    let zero_close = order(
        json!({"symbol":"ETHUSD","type":"market","side":"sell","amount":0,"reduceOnly":true}),
    );
    place(&mut rig, 10, vec![zero_close]);
    let request = rig.next_request();
    let payload = body(&request);
    assert_eq!(payload["quantity"], 0.0);
    assert_eq!(payload["positionEffect"], "CLOSE");
    assert!(payload.get("positionCode").is_none());
    rig.reply(&request, 200, r#"{"orderId":42}"#);
    assert!(rig.result(10).is_ok());
    for invalid in [
        json!({"symbol":"ETHUSD","type":"market","side":"buy","amount":0}),
        json!({"symbol":"ETHUSD","type":"market","side":"sell","amount":0.001,"reduceOnly":true}),
        json!({"symbol":"ETHUSD","type":"market","side":"sell","amount":0,"reduceOnly":true,"close":true}),
    ] {
        place(&mut rig, 11, vec![order(invalid)]);
        assert!(rig.result(11).is_err());
        assert!(rig.requests.is_empty());
    }
}

#[test]
fn unsupported_max_slippage_fails_entire_batch_even_for_explicit_zero() {
    for value in [0.0, 1.0] {
        let mut rig = private();
        place(
            &mut rig,
            10,
            vec![
                limit(),
                order(
                    json!({"symbol":"ETHUSD","type":"market","side":"buy","amount":1,"maxSlippagePct":value}),
                ),
            ],
        );
        assert!(rig.result(10).is_err());
        assert!(rig.requests.is_empty());
    }
}

#[test]
fn zero_close_limit_and_stop_price_amendments_require_native_cancellation_proof() {
    for (kind, native_type, price_field, absent_price_field) in [
        (OrderType::Limit, "LIMIT", "limitPrice", "stopPrice"),
        (OrderType::StopLoss, "STOP", "stopPrice", "limitPrice"),
    ] {
        let mut rig = private();
        rig.engine
            .store_mut()
            .set_orders(vec![zero_close_order(kind)]);
        amend(&mut rig, 10);
        let cancel = rig.next_request();
        assert_eq!(cancel.method, Method::Delete);
        rig.reply(
            &cancel,
            200,
            r#"{"orderId":46073148,"updateOrderId":46073149}"#,
        );
        let confirmation = rig.next_request();
        assert_eq!(confirmation.method, Method::Get);
        assert!(
            confirmation
                .url
                .ends_with("/orders/history?with-order-id=46073148&limit=2")
        );
        assert!(rig.requests.is_empty());
        rig.reply(&confirmation, 200, &zero_close_history().to_string());
        let replacement = rig.next_request();
        assert_eq!(replacement.method, Method::Post);
        let payload = body(&replacement);
        assert_eq!(payload["quantity"], 0.0);
        assert_eq!(payload["positionEffect"], "CLOSE");
        assert_eq!(payload["type"], native_type);
        assert_eq!(payload[price_field], 3660.0);
        assert!(payload.get(absent_price_field).is_none());
        assert!(payload.get("positionCode").is_none());
        rig.reply(&replacement, 200, r#"{"orderId":42}"#);
        assert_eq!(rig.result(10), Ok("[\"42\"]".into()));
    }
}

#[test]
fn zero_close_amendment_fails_closed_when_native_cancellation_proof_is_unsafe() {
    let mut cases = vec![json!({"orders":[]})];
    for (field, value) in [
        ("quantity", json!(1)),
        ("filledQuantity", json!(0.01)),
        ("positionEffect", json!("OPEN")),
    ] {
        let mut proof = zero_close_history();
        proof["orders"][0]["legs"][0][field] = value;
        cases.push(proof);
    }
    let mut completed = zero_close_history();
    completed["orders"][0]["status"] = json!("COMPLETED");
    cases.push(completed);
    for kind in [OrderType::Limit, OrderType::StopLoss] {
        for proof in &cases {
            let mut rig = private();
            rig.engine
                .store_mut()
                .set_orders(vec![zero_close_order(kind)]);
            amend(&mut rig, 10);
            let cancel = rig.next_request();
            rig.reply(
                &cancel,
                200,
                r#"{"orderId":46073148,"updateOrderId":46073149}"#,
            );
            let confirmation = rig.next_request();
            assert_eq!(confirmation.method, Method::Get);
            rig.reply(&confirmation, 200, &proof.to_string());
            assert!(rig.result(10).is_err(), "{proof}");
            assert!(rig.requests.is_empty(), "{proof}");
        }
    }
}

#[test]
fn zero_open_or_partially_filled_close_order_cannot_be_amended() {
    for kind in [OrderType::Limit, OrderType::StopLoss] {
        for reduce_only in [false, true] {
            let mut rig = private();
            let mut existing = zero_close_order(kind);
            existing.reduce_only = reduce_only;
            if reduce_only {
                existing.filled = 0.01;
            }
            rig.engine.store_mut().set_orders(vec![existing]);
            amend(&mut rig, 10);
            assert!(rig.result(10).is_err());
            assert!(rig.requests.is_empty());
        }
    }
}

#[test]
fn empty_client_aliases_do_not_drop_native_orders_from_symbol_or_all_cancellation() {
    for symbol in ["ETHUSD", ""] {
        let mut rig = private();
        let mut first = open();
        first.client_order_id = Some(String::new());
        let mut second = first.clone();
        second.id = "816099:6".into();
        rig.engine.store_mut().set_orders(vec![first, second]);
        rig.command(Command::CancelSymbolOrders {
            command: 10,
            symbol: symbol.into(),
        });
        let cancellations = [rig.next_request(), rig.next_request()];
        assert!(
            cancellations
                .iter()
                .any(|request| request.url.ends_with("/orders/816099%3A5"))
        );
        assert!(
            cancellations
                .iter()
                .any(|request| request.url.ends_with("/orders/816099%3A6"))
        );
        for request in cancellations {
            rig.reply(&request, 200, "{}");
        }
        assert!(rig.result(10).is_ok());
    }
}

#[test]
fn native_cancellation_dedup_and_locks_ignore_empty_client_aliases() {
    let mut rig = private();
    let mut rows = Vec::new();
    for id in ["816099:5", "816099:6", "816099:7"] {
        let mut row = open();
        row.id = id.into();
        row.client_order_id = Some(String::new());
        rows.push(row);
    }
    rig.engine.store_mut().set_orders(rows);
    rig.command(Command::CancelOrders {
        command: 10,
        ids: vec!["816099:5".into(), "816099:6".into(), "816099:5".into()],
    });
    let first = rig.next_request();
    let second = rig.next_request();
    rig.command(Command::CancelOrders {
        command: 11,
        ids: vec!["816099:6".into()],
    });
    assert!(rig.result(11).is_err());
    rig.command(Command::CancelOrders {
        command: 12,
        ids: vec!["816099:7".into()],
    });
    let third = rig.next_request();
    assert!(third.url.ends_with("/orders/816099%3A7"));
    rig.reply(&first, 200, "{}");
    rig.reply(&second, 200, "{}");
    rig.reply(&third, 200, "{}");
    assert!(rig.result(10).is_ok());
    assert!(rig.result(12).is_ok());
    assert_eq!(done_count(&rig, 10), 1);
}

#[test]
fn empty_store_client_alias_does_not_shadow_ack_routing_or_duplicate_its_client_code() {
    let mut rig = private();
    place(&mut rig, 10, vec![limit()]);
    let placement = rig.next_request();
    let code = body(&placement)["orderCode"].as_str().unwrap().to_owned();
    rig.reply(&placement, 200, r#"{"orderId":42}"#);
    let mut row = open();
    row.id = "42".into();
    row.client_order_id = Some(String::new());
    rig.engine.store_mut().set_orders(vec![row]);
    rig.command(Command::CancelOrders {
        command: 11,
        ids: vec!["42".into(), code.clone()],
    });
    let cancellation = rig.next_request();
    assert!(cancellation.url.ends_with(&format!("/orders/{code}")));
    rig.command(Command::CancelOrders {
        command: 12,
        ids: vec![code],
    });
    assert!(rig.result(12).is_err());
    rig.reply(&cancellation, 200, "{}");
    assert!(rig.result(11).is_ok());
    assert!(rig.requests.is_empty());
}

#[test]
fn empty_client_alias_cannot_authorize_history_for_a_different_native_order_code() {
    let mut rig = private();
    let mut existing = open();
    existing.client_order_id = Some(String::new());
    rig.engine.store_mut().set_orders(vec![existing]);
    amend(&mut rig, 10);
    let cancel = rig.next_request();
    rig.reply(
        &cancel,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    let confirmation = rig.next_request();
    let mut proof = cancellation_history(0.0);
    proof["orders"][0]["orderCode"] = json!("different-native-order");
    proof["orders"][0]["clientOrderId"] = json!("");
    rig.reply(&confirmation, 200, &proof.to_string());
    assert!(rig.result(10).is_err());
    assert!(rig.requests.is_empty());
}

fn assert_order_code_receipt(rig: &Rig, request: &HttpRequest, command: u32) -> String {
    let code = body(request)["orderCode"].as_str().unwrap().to_owned();
    let index = rig
        .receipt_trace
        .iter()
        .position(|a| matches!(a, ReceiptTrace::Http(id) if *id == request.id))
        .unwrap();
    match &rig.receipt_trace[index - 1] {
        ReceiptTrace::Progress(owner, receipts) => {
            assert_eq!(*owner, command);
            assert_eq!(
                receipts,
                &vec![OrderIdentityReceipt {
                    symbol: "ETHUSD".into(),
                    kind: OrderIdentityKind::OrderCode,
                    identity: code.clone()
                }]
            );
        }
        _ => panic!("orderCode receipt must precede actual write"),
    }
    code
}

#[test]
fn actual_dispatch_reports_order_codes_sequentially_without_promoting_ack_ids() {
    let mut rig = private();
    place(&mut rig, 77, vec![limit(), limit()]);
    let first = rig.next_request();
    let first_code = assert_order_code_receipt(&rig, &first, 77);
    assert_eq!(
        rig.notices
            .iter()
            .filter(|n| matches!(n, Notice::CommandProgress { command: 77, .. }))
            .count(),
        1
    );
    rig.reply(&first, 200, r#"{"orderId":46073148}"#);
    let next = rig.next_request();
    let second_code = assert_order_code_receipt(&rig, &next, 77);
    assert_ne!(first_code, second_code);
    assert_ne!(first_code, "46073148");
    assert_ne!(second_code, "46073148");
}

#[test]
fn write_failures_are_classified_without_changing_their_text() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let rejected = |kind| (Some(kind), Some(Outcome::Rejected));
    for (status, classified) in [
        (500, (None, Some(Outcome::Unknown))),
        (408, (None, Some(Outcome::Unknown))),
        (403, rejected(ErrorKind::AuthPermission)),
        (409, rejected(ErrorKind::Rejected)),
        (429, rejected(ErrorKind::RateLimited)),
    ] {
        let mut rig = private();
        place(&mut rig, 10, vec![limit()]);
        let request = rig.next_request();
        rig.reply(&request, status, r#"{"errorCode":33}"#);
        assert_eq!(rig.result(10), Err(format!("DXtrade HTTP {status}")));
        let error = &rig.failures[0];
        assert_eq!((error.kind, error.outcome), classified, "{status}");
    }
    // A malformed 2xx acknowledgement does not prove what happened.
    let mut rig = private();
    place(&mut rig, 10, vec![limit()]);
    let request = rig.next_request();
    rig.reply(&request, 200, "{}");
    assert_eq!(rig.failures[0].outcome, Some(Outcome::Unknown));
    // Transport loss on a write is Unknown.
    let mut rig = private();
    place(&mut rig, 10, vec![limit()]);
    let request = rig.next_request();
    rig.failure(&request);
    assert_eq!(
        (rig.failures[0].kind, rig.failures[0].outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
    // Preflight refusals were never sent.
    let mut rig = private();
    place(
        &mut rig,
        10,
        vec![order(
            json!({"symbol":"ETHUSD","type":"limit","side":"buy","amount":1}),
        )],
    );
    assert!(rig.requests.is_empty());
    assert_eq!(rig.failures[0].outcome, Some(Outcome::NotSent));
}

#[test]
fn a_refusal_after_an_accepted_placement_is_never_rejected() {
    let mut rig = private();
    place(&mut rig, 10, vec![limit(), limit()]);
    let first = rig.next_request();
    rig.reply(
        &first,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    let second = rig.next_request();
    rig.reply(&second, 409, r#"{"errorCode":33}"#);
    assert_eq!(rig.result(10), Err("DXtrade HTTP 409".to_owned()));
    assert_eq!(rig.failures[0].outcome, None);
}

#[test]
fn an_unknown_cancel_leg_makes_the_batch_unknown_with_its_kind() {
    use safe_cex_engine::{ErrorKind, Outcome};
    let mut rig = private();
    rig.command(Command::CancelOrders {
        command: 10,
        ids: vec!["first".into(), "second".into()],
    });
    let first = rig.next_request();
    let second = rig.next_request();
    rig.reply(&first, 429, "{}");
    rig.failure(&second);
    assert_eq!(rig.result(10), Err("DXtrade HTTP 429".to_owned()));
    assert_eq!(
        (rig.failures[0].kind, rig.failures[0].outcome),
        (Some(ErrorKind::Network), Some(Outcome::Unknown))
    );
}

#[test]
#[ignore = "original RED: P0 write adoption waits for session/signer and Plan seams"]
fn p0_write_accepted_id_survives_later_refusal() {
    p0_write_partial(false);
}

#[test]
#[ignore = "original RED: P0 write adoption waits for session/signer and Plan seams"]
fn p0_write_accepted_id_survives_later_transport_loss() {
    p0_write_partial(true);
}

fn p0_write_partial(transport: bool) {
    let mut rig = private();
    place(&mut rig, 751, vec![limit(), limit()]);
    let first = rig.next_request();
    assert_eq!(first.method, Method::Post);
    rig.reply(
        &first,
        200,
        r#"{"orderId":46073148,"updateOrderId":46073149}"#,
    );
    let second = rig.next_request();
    assert_eq!(second.method, Method::Post);
    if transport {
        rig.failure(&second);
    } else {
        rig.reply(&second, 409, r#"{"errorCode":33}"#);
    }
    assert_eq!(done_count(&rig, 751), 1);
    let [error] = rig.failures.as_slice() else {
        panic!("one failed terminal: {:?}", rig.failures);
    };
    if transport {
        assert_eq!(error.outcome, Some(safe_cex_engine::Outcome::Unknown));
    }
    assert!(!matches!(
        error.outcome,
        Some(safe_cex_engine::Outcome::Rejected | safe_cex_engine::Outcome::NotSent)
    ));
    assert_eq!(
        error
            .partial
            .as_ref()
            .map(|partial| partial.order_ids.as_slice()),
        Some(["46073148".to_owned()].as_slice())
    );
    assert!(
        !rig.requests
            .iter()
            .any(|request| request.method == Method::Post && request.url.contains("/orders"))
    );
}

#[test]
fn p0_write_login_401_preserves_auth_invalid_for_waiter() {
    p0_write_login(401, safe_cex_engine::ErrorKind::AuthInvalid);
}

#[test]
fn p0_write_login_403_preserves_auth_permission_for_waiter() {
    p0_write_login(403, safe_cex_engine::ErrorKind::AuthPermission);
}

fn p0_write_login(status: u16, kind: safe_cex_engine::ErrorKind) {
    let mut rig = Rig::defaults();
    rig.command(Command::FetchCandles {
        command: 752,
        symbol: "ETHUSD".into(),
        interval: Interval::H1,
        from_ms: None,
        to_ms: None,
        limit: None,
    });
    let login = rig.next_request();
    assert!(login.url.ends_with("/login"));
    rig.reply(
        &login,
        status,
        r#"{"message":"example-password secret-token"}"#,
    );
    assert_eq!(done_count(&rig, 752), 1);
    let [error] = rig.failures.as_slice() else {
        panic!("one failed waiter: {:?}", rig.failures);
    };
    assert_eq!(error.kind, Some(kind));
    assert!(
        rig.notices
            .iter()
            .any(|notice| matches!(notice, Notice::Event(event)
        if event.code == safe_cex_engine::log::code::AUTH_REJECTED))
    );
    assert!(
        rig.notices
            .iter()
            .any(|notice| matches!(notice, Notice::Deactivate))
    );
    assert!(rig.requests.is_empty());
    let notices = format!("{:?}", rig.notices);
    assert!(!notices.contains("example-password"));
    assert!(!notices.contains("secret-token"));
}
