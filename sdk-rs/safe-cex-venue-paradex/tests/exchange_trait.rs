//! Constructed Paradex workflows over scripted HTTP and actual shared socket/runtime bindings.
#[path = "support/exchange_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use serde_json::json;
use support::drive;
fn script(wire: &support::Wire) {
    let mut responses = wire.responses.borrow_mut();
    responses.insert("/markets".into(),json!({"results":[{"market":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD","asset_kind":"PERP","status":"ACTIVE","step_size":"0.003","tick_size":"0.25","min_order_size":"0.003","max_order_size":"100","initial_margin_fraction":"0.05","maker_fee":"0.0001","taker_fee":"0.0002"}]}));
    responses.insert("/markets/summary".into(),json!({"results":[{"symbol":"BTC-USD-PERP","bid":"101","ask":"102","last_traded_price":"101.5","mark_price":"101.6","underlying_price":"101.4","price_change_rate_24h":"0.01","funding_rate":"0.0001","volume_24h":"12","open_interest":"5"}]}));
}
#[test]
fn public_constructor_and_start_execute_existing_market_workflows() {
    let (venue, rt, store, wire) =
        support::fixture_with(json!({"key":"","secret":"","publicDataOnly":true}), script);
    assert!(venue.is_public_data_only());
    drive(&rt, venue.start()).unwrap();
    assert!(store.loaded().markets && store.loaded().tickers);
    assert_eq!(store.markets().len(), 1);
    assert_eq!(store.markets()[0].symbol, "BTC-USD-PERP");
    assert_eq!(store.markets()[0].precision.amount, 0.003);
    assert_eq!(store.markets()[0].precision.price, 0.25);
    assert_eq!(store.tickers().len(), 1);
    assert_eq!(store.tickers()[0].bid, 101.0);
    assert!(wire.requests.borrow().iter().any(|r| r.url == "/markets"));
    venue.dispose().unwrap();
    assert!(venue.base().is_disposed());
}

#[test]
fn public_socket_subscribes_and_delivers_throttled_source_tickers() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let (venue, runtime, store, _) =
        support::fixture_with(json!({"key":"","secret":"","publicDataOnly":true}), script);
    drive(&runtime, venue.start()).unwrap();
    for _ in 0..8 {
        runtime.pump();
    }
    assert!(
        runtime.errors.borrow().is_empty(),
        "public callback errors: {:?}",
        runtime.errors.borrow()
    );
    let frames: Vec<serde_json::Value> = runtime
        .sent
        .borrow()
        .iter()
        .map(|frame| serde_json::from_str(frame).unwrap())
        .collect();
    let channels: Vec<_> = frames
        .iter()
        .filter(|frame| frame["method"] == "subscribe")
        .map(|frame| frame["params"]["channel"].as_str().unwrap())
        .collect();
    assert_eq!(
        channels,
        vec!["bbo.BTC-USD-PERP", "trades.BTC-USD-PERP", "markets_summary"]
    );
    let socket = runtime.sockets.borrow()[0].1.clone();
    let push = |bid: &str| {
        socket.events.borrow_mut().push_back(WsEvent::Message(SocketFrame::Text(json!({
        "jsonrpc":"2.0", "method":"subscription", "params":{"channel":"bbo.BTC-USD-PERP",
        "data":{"market":"BTC-USD-PERP", "bid_price":bid, "ask_price":"123.75", "index_price":"121.125"}}
    }).to_string())))
    };
    push("120.25");
    for _ in 0..8 {
        runtime.pump();
    }
    assert_eq!(store.tickers()[0].bid, 120.25);
    push("122.5");
    for _ in 0..8 {
        runtime.pump();
    }
    assert_eq!(
        store.tickers()[0].bid,
        120.25,
        "second source update waits for throttle"
    );
    runtime.now.set(runtime.now.get() + 100.);
    for _ in 0..8 {
        runtime.pump();
    }
    assert_eq!(
        store.tickers()[0].bid,
        122.5,
        "latest source update delivered at trailing edge"
    );
    assert_eq!(store.tickers()[0].ask, 123.75);
    assert_eq!(store.tickers()[0].index, 121.125);
    assert!(
        runtime.errors.borrow().is_empty(),
        "feed callback errors: {:?}",
        runtime.errors.borrow()
    );
    venue.dispose().unwrap();
}

fn private_options() -> serde_json::Value {
    json!({"key":format!("0x{:064x}",1),"secret":"","paradexAddress":"0x123","subkeyAddress":"0x456"})
}
fn private_script(wire: &support::Wire) {
    script(wire);
    let mut responses = wire.responses.borrow_mut();
    responses.insert("/v1/system/config".into(),json!({"starknet_fullnode_rpc_url":"https://fixture.invalid/rpc", "starknet_chain_id":"PRIVATE_SN_POTC_MAINNET", "l1_chain_id":"1", "paraclear_account_hash":"0x1", "paraclear_account_proxy_hash":"0x2", "paraclear_address":"0x3", "paraclear_decimals":8, "bridged_tokens":[]}));
    responses.insert(
        "/v1/auth/0x456".into(),
        json!({"jwt_token":"fixture.jwt.token"}),
    );
    responses.insert("/account".into(),json!({"account":"fixture","account_number":"42","address":"0x123","public_key":"0x456","initial_margin_requirement":"90","maintenance_margin_requirement":"45","account_value":"500","total_collateral":"500","free_collateral":"410","margin_cushion":"400","settlement_asset":"USDC","updated_at":1700000000000.0,"status":"ACTIVE","seq_no":7}));
    responses.insert(
        "/account/margin".into(),
        json!({"account":"0x123","margin_methodology":"portfolio","configs":[]}),
    );
    responses.insert("/positions".into(), json!({"results":[]}));
    responses.insert("/orders".into(), json!({"results":[]}));
}
#[test]
fn private_start_signs_jwt_and_subscribes_only_after_the_matching_auth_reply() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let (venue, runtime, store, wire) = support::fixture_with(private_options(), private_script);
    drive(&runtime, venue.start()).unwrap();
    for _ in 0..12 {
        runtime.pump();
    }
    assert!(
        runtime.errors.borrow().is_empty(),
        "private callback errors: {:?}; logs {:?}",
        runtime.errors.borrow(),
        runtime.console.borrow()
    );
    assert!(store.loaded().balance && store.loaded().positions && store.loaded().orders);
    assert_eq!(store.balance().total, 500.);
    let frames: Vec<serde_json::Value> = runtime
        .sent
        .borrow()
        .iter()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect();
    let auth = frames
        .iter()
        .find(|frame| frame["method"] == "auth")
        .expect("signed JWT auth frame");
    assert_eq!(auth["params"]["bearer"], "fixture.jwt.token");
    assert!(
        !frames
            .iter()
            .any(|frame| frame["params"]["channel"] == "orders.ALL")
    );
    let private_socket = runtime.sockets.borrow()[1].1.clone();
    private_socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(
            json!({"jsonrpc":"2.0","id":auth["id"],"result":{}}).to_string(),
        )));
    for _ in 0..8 {
        runtime.pump();
    }
    let frames: Vec<serde_json::Value> = runtime
        .sent
        .borrow()
        .iter()
        .map(|v| serde_json::from_str(v).unwrap())
        .collect();
    for channel in ["orders.ALL", "positions", "balance_events"] {
        assert!(
            frames.iter().any(
                |frame| frame["method"] == "subscribe" && frame["params"]["channel"] == channel
            ),
            "private channel {channel}"
        );
    }
    let requests = wire.requests.borrow();
    let auth_request = requests
        .iter()
        .find(|request| request.url.ends_with("/auth/0x456"))
        .expect("real signed auth HTTP request");
    assert!(
        auth_request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("PARADEX-STARKNET-SIGNATURE"))
    );
    assert!(requests.iter().filter(|r| r.url == "/account").all(|r| {
        r.headers.iter().any(|(name, value)| {
            name.eq_ignore_ascii_case("Authorization") && value == "Bearer fixture.jwt.token"
        })
    }));
    drop(requests);
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let seen = calls.clone();
    venue.base().on(
        "fill",
        std::rc::Rc::new(move |_, _| {
            seen.set(seen.get() + 1);
            Ok(())
        }),
    );
    assert!(!venue.base().is_disposed());
    venue.dispose().unwrap();
    assert!(
        venue.base().is_disposed(),
        "PARADEX DISPOSAL source writes the shared flag"
    );
    assert_eq!(
        runtime.sockets.borrow().len(),
        2,
        "PARADEX DISPOSAL socket census"
    );
    assert!(
        runtime
            .sockets
            .borrow()
            .iter()
            .all(|(_, socket)| socket.closed.get()),
        "PARADEX DISPOSAL source closes both sockets"
    );
    venue
        .base()
        .emit("fill", json!({"source":"disposal witness"}))
        .unwrap();
    assert_eq!(
        calls.get(),
        1,
        "PARADEX DISPOSAL source retains exchange listeners"
    );
}

#[test]
fn private_signing_failure_sends_no_auth_or_subscribe_and_recovers() {
    let mut options = private_options();
    // Starknet's Account constructor admits this key; signing rejects it.
    // The actual-source SDK oracle independently requires that rejection.
    options["key"] = json!(format!("0x{}", "f".repeat(64)));
    let (venue, runtime, _, wire) = support::fixture_with(options, private_script);
    assert!(drive(&runtime, venue.start()).is_err());
    for _ in 0..16 {
        runtime.pump();
    }
    let frames: Vec<serde_json::Value> = runtime
        .sent
        .borrow()
        .iter()
        .map(|frame| serde_json::from_str(frame).unwrap())
        .collect();
    assert!(
        frames.iter().all(|frame| frame["method"] != "auth"),
        "PARADEX AUTH FAILURE must send no authentication frame"
    );
    for channel in ["orders.ALL", "positions", "balance_events"] {
        assert!(
            frames
                .iter()
                .all(|frame| frame["params"]["channel"] != channel),
            "PARADEX AUTH FAILURE must not subscribe to {channel}"
        );
    }
    assert!(
        !wire
            .requests
            .borrow()
            .iter()
            .any(|request| request.url.contains("/auth/")),
        "PARADEX AUTH FAILURE signing must fail before HTTP token submission"
    );
    let logs = runtime.console.borrow();
    assert!(
        logs.iter()
            .any(|message| message.contains("Failed to authenticate WebSocket:")),
        "PARADEX AUTH FAILURE source catch must still log: {logs:?}"
    );
    assert!(
        logs.iter().any(|message| message
            .contains("Private WebSocket signing could not complete; reconnecting")),
        "PARADEX AUTH FAILURE shared recovery must execute: {logs:?}"
    );
    drop(logs);
    assert!(
        runtime
            .sockets
            .borrow()
            .iter()
            .skip(1)
            .any(|(_, socket)| socket.closed.get()),
        "PARADEX AUTH FAILURE shared reconnect must close the failed socket"
    );
    venue.dispose().unwrap();
}

#[test]
fn private_source_channels_update_native_order_position_and_balance_fields() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    let (venue, runtime, store, _) = support::fixture_with(private_options(), private_script);
    drive(&runtime, venue.start()).unwrap();
    for _ in 0..8 {
        runtime.pump();
    }
    let socket = runtime.sockets.borrow()[1].1.clone();
    let fills = std::rc::Rc::new(std::cell::Cell::new(0usize));
    let observed = fills.clone();
    venue.base().on(
        "fill",
        std::rc::Rc::new(move |_, _| {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    );
    let position_writes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let capture = position_writes.clone();
    let unsubscribe = store.subscribe(std::sync::Arc::new(move |data, _| {
        capture.lock().unwrap().push(data.positions.clone());
        Ok(())
    }));
    let push = |channel: &str, data: serde_json::Value| {
        socket.events.borrow_mut().push_back(WsEvent::Message(SocketFrame::Text(
            json!({"jsonrpc":"2.0","method":"subscription","params":{"channel":channel,"data":data}}).to_string(),
        )));
        runtime.pump();
    };
    push(
        "orders.ALL",
        json!({"id":"ws-order-7","market":"BTC-USD-PERP","side":"SELL","type":"LIMIT","status":"OPEN","price":"109.25","size":"0.012","remaining_size":"0.009","reduce_only":false,"time_in_force":"GoodTillCancel"}),
    );
    let orders = store.orders();
    assert_eq!(orders.len(), 1, "PARADEX private order census");
    let order = &orders[0];
    assert_eq!(order.id, "ws-order-7");
    assert_eq!(order.side, safe_cex_types::OrderSide::Sell);
    assert_eq!(order.price, 109.25);
    assert_eq!(order.amount, 0.012);
    assert_eq!(order.remaining, 0.009);
    assert_eq!(order.filled.to_bits(), (0.012_f64 - 0.009).to_bits());
    assert_eq!(order.info.as_ref().unwrap()["market"], "BTC-USD-PERP");
    drop(orders);
    push(
        "orders.ALL",
        json!({"id":"ws-order-7","market":"BTC-USD-PERP","side":"SELL","type":"LIMIT","status":"FILLED","price":"109.25","size":"0.012","remaining_size":"0","reduce_only":false,"time_in_force":"GoodTillCancel"}),
    );
    let orders = store.orders();
    let terminal = orders
        .iter()
        .find(|order| order.id == "ws-order-7")
        .unwrap();
    assert_eq!(
        terminal.status.value(),
        Some(&safe_cex_types::OrderStatus::Closed)
    );
    assert_eq!(terminal.filled, 0.012);
    assert_eq!(terminal.remaining, 0.0);
    drop(orders);
    // The source immediately starts a REST refresh too. Observe the feed's
    // native notification independently of the subsequent snapshot replacement.
    push(
        "positions",
        json!({"market":"BTC-USD-PERP","status":"OPEN","side":"SHORT","size":"0.021","average_entry_price":"117.5","unrealized_pnl":"-3.25","realized_positional_pnl":"1.125","leverage":"7","liquidation_price":"131.75","last_updated_at":1700000000123.0}),
    );
    let positions = position_writes.lock().unwrap();
    let position = positions
        .iter()
        .flatten()
        .find(|position| position.contracts == 0.021)
        .unwrap_or_else(|| {
            panic!(
                "PARADEX synchronous native position write; positions {positions:?}; logs {:?}",
                runtime.console.borrow()
            )
        });
    assert_eq!(position.side, safe_cex_types::PositionSide::Short);
    assert_eq!(position.entry_price, 117.5);
    assert_eq!(position.notional, 0.021 * 117.5);
    assert_eq!(position.unrealized_pnl, -3.25);
    assert_eq!(position.realized_pnl, Some(1.125));
    assert_eq!(position.leverage, 7.);
    assert_eq!(position.liquidation_price, 131.75);
    drop(positions);
    push(
        "balance_events",
        json!({"total_collateral":"777.25","free_collateral":"601.5","initial_margin_requirement":"171.125"}),
    );
    let balance = store.balance();
    assert_eq!(
        (balance.total, balance.free, balance.used, balance.upnl),
        (777.25, 601.5, 171.125, 0.)
    );
    drop(balance);
    assert!(runtime.errors.borrow().is_empty());
    assert!(
        !runtime
            .console
            .borrow()
            .iter()
            .any(|message| message.contains("Error handling")),
        "PARADEX source ingress must not swallow a binding error"
    );
    assert_eq!(
        fills.get(),
        0,
        "source private orders update Store without emitting fills"
    );
    unsubscribe();
    venue.dispose().unwrap();
}

fn typed<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
    serde_json::from_value(value).unwrap()
}
fn limit_order(id: &str) -> safe_cex_types::PlaceOrderOpts {
    typed(
        json!({"symbol":"BTC-USD-PERP","type":"limit","side":"buy","amount":0.006,"price":101.5,"clientOrderId":id}),
    )
}
fn reconciled_order() -> safe_cex_types::Order {
    typed(
        json!({"id":"native-one","symbol":"BTC-USD-PERP","type":"limit","side":"buy","status":"open","price":101.5,"amount":0.006,"filled":0.0,"remaining":0.006,"reduceOnly":false,"timeInForce":"GoodTillCancel","lastUpdate":1700000000000.0,"info":{"id":"native-one","market":"BTC-USD-PERP","type":"LIMIT","side":"BUY","status":"OPEN","price":"101.5","size":"0.006","remaining_size":"0.006","reduce_only":false,"instruction":"GTC"}}),
    )
}
#[test]
fn all_sixteen_exchange_entrypoints_execute_source_workflows() {
    let (venue, runtime, store, wire) = support::fixture_with(private_options(), |wire| {
        private_script(wire);
        let mut responses = wire.responses.borrow_mut();
        responses.insert(
            "/markets/klines".into(),
            json!({"results":[[1700000000000.0,101,103,100,102,7]]}),
        );
        responses.insert("/orders/native-one".into(), json!({"id":"native-one"}));
        responses.insert("/orders/batch".into(),json!({"orders":[{"id":"batch-two","client_id":"caller-two"},{"id":"batch-three","client_id":"caller-three"}],"errors":[]}));
    });
    drive(&runtime, venue.start()).unwrap();
    assert_eq!(drive(&runtime, venue.get_account()).unwrap().user_id, "42");
    assert_eq!(
        drive(&runtime, venue.validate_account()).unwrap().error,
        "".into()
    );
    assert_eq!(
        drive(&runtime, venue.fetch_positions(None)).unwrap().len(),
        1,
        "source fake position for loaded market"
    );
    assert!(
        drive(&runtime, venue.fetch_orders(None))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        drive(
            &runtime,
            venue.fetch_symbol_max_leverage("BTC-USD-PERP".into())
        )
        .unwrap_err()
        .message,
        "Not implemented"
    );
    drive(
        &runtime,
        venue.set_leverage("BTC-USD-PERP".into(), 7., None),
    )
    .unwrap();
    assert_eq!(
        drive(&runtime, venue.change_position_mode(true, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(
        drive(&runtime, venue.change_margin_type(true, None))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    wire.sequence
        .borrow_mut()
        .entry("/orders".into())
        .or_default()
        .push_back((200, json!({"id":"native-one","client_id":"caller-one"})));
    assert_eq!(
        drive(
            &runtime,
            venue
                .base()
                .place_order(venue.as_ref(), limit_order("caller-one"))
        )
        .unwrap(),
        vec!["native-one"]
    );
    assert_eq!(
        drive(
            &runtime,
            venue.base().place_orders(
                venue.as_ref(),
                vec![limit_order("caller-two"), limit_order("caller-three")]
            )
        )
        .unwrap(),
        vec!["batch-two", "batch-three"]
    );
    assert_eq!(
        drive(
            &runtime,
            venue.update_order(typed(
                json!({"order":reconciled_order(),"update":{"amount":0.0089,"price":102.13}})
            ))
        )
        .unwrap(),
        vec!["native-one"]
    );
    drive(&runtime, venue.cancel_orders(vec![reconciled_order()])).unwrap();
    assert_eq!(
        drive(&runtime, venue.schedule_cancel(Some(Some(1500.))))
            .unwrap_err()
            .message,
        "Not implemented"
    );
    assert_eq!(
        drive(
            &runtime,
            venue.fetch_public_trades("BTC-USD-PERP".into(), Some(1700000000000.))
        )
        .unwrap_err()
        .message,
        "Not implemented"
    );
    let candles=drive(&runtime,venue.fetch_ohlcv(typed(json!({"symbol":"BTC-USD-PERP","interval":"1m","from":1699999980000.0,"to":1700000040000.0})))).unwrap();
    assert_eq!(candles.len(), 1);
    assert_eq!(candles[0].close, 102.);
    assert!(store.loaded().markets && store.loaded().positions && store.loaded().orders);
    let requests = wire.requests.borrow();
    let post = requests
        .iter()
        .find(|request| request.method.eq_ignore_ascii_case("POST") && request.url == "/orders")
        .unwrap();
    let body: serde_json::Value =
        serde_json::from_slice(&post.body.as_ref().unwrap().wire_bytes().unwrap()).unwrap();
    assert_eq!(body["client_id"], "caller-one");
    assert_eq!(body["size"], "0.006");
    assert_eq!(body["price"], "101.5");
    assert!(
        body["signature"]
            .as_str()
            .is_some_and(|value| value.starts_with('['))
    );
    let put = requests
        .iter()
        .find(|request| {
            request.method.eq_ignore_ascii_case("PUT") && request.url == "/orders/native-one"
        })
        .unwrap();
    let modified: serde_json::Value =
        serde_json::from_slice(&put.body.as_ref().unwrap().wire_bytes().unwrap()).unwrap();
    assert_eq!(
        modified["size"], "0.009",
        "PARADEX PRECISION amendment amount axis"
    );
    assert_eq!(
        modified["price"], "102.25",
        "PARADEX PRECISION amendment price axis"
    );
    assert_eq!(
        modified["id"], "native-one",
        "PARADEX amendment retains venue identity"
    );
    let auth = requests
        .iter()
        .find(|request| request.url.ends_with("/auth/0x456"))
        .unwrap();
    compare_actual_sdk_signatures(&body, &modified, auth);

    assert!(
        requests
            .iter()
            .any(|request| request.method.eq_ignore_ascii_case("DELETE")
                && request.url == "/orders/native-one")
    );
    drop(requests);
    venue.dispose().unwrap();
}

fn compare_actual_sdk_signatures(
    order: &serde_json::Value,
    modified: &serde_json::Value,
    auth: &safe_cex_transport::RestRequest,
) {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let header = |key: &str| {
        auth.headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(key))
            .unwrap()
            .1
            .clone()
    };
    let cases = json!([
        {"kind":"Order","timestamp":order["signature_timestamp"],"params":order},
        {"kind":"ModifyOrder","timestamp":modified["signature_timestamp"],"params":modified},
        {"kind":"JWT","timestamp":header("PARADEX-TIMESTAMP").parse::<f64>().unwrap()*1000.}
    ]);
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/binding_signatures_source.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(cases.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "actual SDK signing: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!(
        "Paradex SDK source: Node {} (major {}) / {} / V8 {}",
        capture["node"], capture["major"], capture["arch"], capture["v8"]
    );
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "source Order/ModifyOrder/JWT signatures");
    assert_eq!(
        rows[0]["signature"], order["signature"],
        "native Order signature equals actual TS SDK"
    );
    assert_eq!(
        rows[1]["signature"], modified["signature"],
        "native ModifyOrder signature equals actual TS SDK"
    );
    for key in [
        "PARADEX-STARKNET-SIGNATURE",
        "PARADEX-STARKNET-ACCOUNT",
        "PARADEX-TIMESTAMP",
        "PARADEX-SIGNATURE-EXPIRATION",
    ] {
        assert_eq!(
            rows[2]["headers"][key].as_str().unwrap(),
            header(key),
            "native JWT header {key} equals actual TS SDK"
        );
    }
}

#[test]
fn amendment_precision_keeps_amount_execution_and_trigger_axes_distinct() {
    let (venue, runtime, _, wire) = support::fixture_with(private_options(), |wire| {
        private_script(wire);
        let mut responses = wire.responses.borrow_mut();
        responses.insert("/orders/native-one".into(), json!({"id":"native-one"}));
        responses.insert("/orders/native-stop".into(), json!({"id":"native-stop"}));
    });
    drive(&runtime, venue.start()).unwrap();
    drive(
        &runtime,
        venue.update_order(typed(
            json!({"order":reconciled_order(),"update":{"amount":0.7589,"price":102.13}}),
        )),
    )
    .unwrap();
    let mut stop = reconciled_order();
    stop.id = "native-stop".into();
    stop.r#type = (safe_cex_types::OrderType::StopLoss).into();
    stop.reduce_only = true.into();
    stop.price = (99.37).into();
    stop.info = (Some(
        json!({"id":"native-stop","market":"BTC-USD-PERP","side":"BUY","type":"STOP_MARKET","status":"UNTRIGGERED","price":"0","trigger_price":"99.37","size":"0.006","remaining_size":"0.006","reduce_only":true,"instruction":"GTC"}),
    )).into();
    drive(
        &runtime,
        venue.update_order(typed(
            json!({"order":stop,"update":{"amount":0.7589,"price":103.13}}),
        )),
    )
    .unwrap();
    let requests = wire.requests.borrow();
    let puts: Vec<_> = requests
        .iter()
        .filter(|request| request.method.eq_ignore_ascii_case("PUT"))
        .collect();
    assert_eq!(
        puts.len(),
        2,
        "PARADEX PRECISION independent limit/stop census"
    );
    let limit: serde_json::Value =
        serde_json::from_slice(&puts[0].body.as_ref().unwrap().wire_bytes().unwrap()).unwrap();
    let stop: serde_json::Value =
        serde_json::from_slice(&puts[1].body.as_ref().unwrap().wire_bytes().unwrap()).unwrap();
    assert_eq!(
        limit["size"], "0.759",
        "PARADEX PRECISION limit amount axis"
    );
    assert_eq!(
        limit["price"], "102.25",
        "PARADEX PRECISION limit execution axis"
    );
    assert!(
        limit.get("trigger_price").is_none(),
        "PARADEX PRECISION limit omits trigger"
    );
    assert_eq!(stop["size"], "0.759", "PARADEX PRECISION stop amount axis");
    assert_eq!(
        stop["price"], "0",
        "PARADEX PRECISION stop retains execution price"
    );
    assert_eq!(
        stop["trigger_price"], "103.25",
        "PARADEX PRECISION stop trigger axis"
    );
    assert_eq!(puts[0].url, "/orders/native-one");
    assert_eq!(puts[1].url, "/orders/native-stop");
    drop(requests);
    venue.dispose().unwrap();
}

#[test]
fn ethereum_sdk_accounts_and_jwt_headers_match_actual_source_sdk() {
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut inputs = vec![];
    let mut native_headers = vec![];
    for (index, chain, account_hash, proxy_hash) in
        [(1, "1", "0x1", "0x2"), (2, "11155111", "0x3", "0x5")]
    {
        let options = json!({"key":format!("{index:064x}"),"secret":"","paradexAddress":"0x123","subkeyAddress":"0x456","testnet":index == 2});
        let (venue, runtime, _, wire) = support::fixture_with(options.clone(), |wire| {
            private_script(wire);
            let mut responses = wire.responses.borrow_mut();
            let config = responses.get_mut("/v1/system/config").unwrap();
            config["l1_chain_id"] = json!(chain);
            config["paraclear_account_hash"] = json!(account_hash);
            config["paraclear_account_proxy_hash"] = json!(proxy_hash);
        });
        drive(&runtime, venue.start()).unwrap();
        for _ in 0..8 {
            runtime.pump();
        }
        let requests = wire.requests.borrow();
        let auth = requests
            .iter()
            .find(|request| request.url.ends_with("/auth/0x456"))
            .expect("Ethereum native signed JWT");
        let headers: serde_json::Map<String, serde_json::Value> = auth
            .headers
            .iter()
            .filter(|(key, _)| key.starts_with("PARADEX-"))
            .map(|(key, value)| (key.clone(), json!(value)))
            .collect();
        let timestamp = headers["PARADEX-TIMESTAMP"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            * 1000.;
        inputs.push(json!({"options":options,"timestamp":timestamp,"config":{"paradexFullNodeRpcUrl":"https://fixture.invalid/rpc","paradexChainId":"PRIVATE_SN_POTC_MAINNET","ethereumChainId":chain,"paraclearAccountHash":account_hash,"paraclearAccountProxyHash":proxy_hash}}));
        native_headers.push(headers);
        drop(requests);
        venue.dispose().unwrap();
    }
    assert_eq!(native_headers.len(), 2, "Ethereum native case floor");
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/binding_ethereum_source.cjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&inputs).unwrap().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "actual Ethereum SDK: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    eprintln!(
        "Paradex Ethereum source: Node {} / {} / V8 {}",
        capture["node"], capture["arch"], capture["v8"]
    );
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 2, "Ethereum source case floor");
    for index in 0..2 {
        for key in [
            "PARADEX-STARKNET-SIGNATURE",
            "PARADEX-STARKNET-ACCOUNT",
            "PARADEX-TIMESTAMP",
            "PARADEX-SIGNATURE-EXPIRATION",
        ] {
            assert_eq!(
                native_headers[index][key], rows[index]["headers"][key],
                "PARADEX ETHEREUM native JWT {index}/{key}"
            );
        }
    }
}

#[test]
fn numeric_created_at_survives_constructed_fetch_orders() {
    let (venue, runtime, store, wire) = support::fixture_with(private_options(), |wire| {
        private_script(wire);
        wire.responses.borrow_mut().insert(
            "/orders".into(),
            json!({"results":[{
                "id":"epoch-order", "market":"BTC-USD-PERP", "side":"BUY", "type":"LIMIT",
                "status":"OPEN", "price":"101.5", "size":"0.006", "remaining_size":"0.003",
                "created_at":1784505600000i64, "reduce_only":false
            }]}),
        );
    });
    drive(&runtime, venue.start())
        .expect("numeric created_at must not abort initial order loading");
    let orders = drive(&runtime, venue.fetch_orders(None)).expect("numeric created_at REST read");
    assert_eq!(orders.len(), 1, "retain open row from REST");
    assert_eq!(orders[0].id, "epoch-order");
    assert_eq!(
        orders[0].last_update.to_bits(),
        1784505600000.0f64.to_bits()
    );
    assert_eq!(orders[0].filled.to_bits(), (0.006f64 - 0.003f64).to_bits());
    assert_eq!(
        orders[0].info.as_ref().unwrap()["created_at"]
            .as_f64()
            .unwrap()
            .to_bits(),
        1784505600000.0f64.to_bits()
    );
    assert!(
        store.loaded().orders,
        "startup commits the native order snapshot"
    );
    assert_eq!(store.orders().len(), 1);
    assert_eq!(store.orders()[0].id, "epoch-order");
    assert!(
        wire.requests
            .borrow()
            .iter()
            .any(|request| request.url.ends_with("/orders"))
    );
    venue.dispose().unwrap();
}

#[test]
fn raw_order_dates_and_numeric_sizes_survive_constructed_fetch_orders() {
    let (venue, runtime, store, _) = support::fixture_with(private_options(), |wire| {
        private_script(wire);
        let mut rows = Vec::new();
        for (id, date) in [
            ("null-date", Some(json!(null))),
            ("false-date", Some(json!(false))),
            ("true-date", Some(json!(true))),
            ("missing-date", None),
            ("valid-sibling", Some(json!("2026-07-20T00:00:00Z"))),
        ] {
            let mut row = json!({
                "id": id, "market": "BTC-USD-PERP", "side": "BUY", "type": "LIMIT",
                "status": "OPEN", "price": "101.5", "size": 0.006,
                "remaining_size": 0.003, "reduce_only": false
            });
            if let Some(date) = date {
                row["created_at"] = date;
            }
            rows.push(row);
        }
        wire.responses
            .borrow_mut()
            .insert("/orders".into(), json!({"results": rows}));
    });
    let venue: &dyn Exchange = venue.as_ref();
    drive(&runtime, venue.start()).expect("raw order coercion must preserve startup batch");
    let orders = drive(&runtime, venue.fetch_orders(None)).expect("raw order REST batch");
    assert_eq!(
        orders.len(),
        5,
        "retain every odd row and its valid sibling"
    );
    assert!(store.loaded().orders);
    assert_eq!(store.orders().len(), 5);
    for (order, id) in orders.iter().zip([
        "null-date",
        "false-date",
        "true-date",
        "missing-date",
        "valid-sibling",
    ]) {
        assert_eq!(order.id, id);
        assert_eq!(order.amount.to_bits(), 0.006_f64.to_bits());
        assert_eq!(order.remaining.to_bits(), 0.003_f64.to_bits());
        assert_eq!(order.filled.to_bits(), (0.006_f64 - 0.003).to_bits());
        let stored = store
            .orders()
            .iter()
            .find(|row| row.id == id)
            .unwrap()
            .clone();
        assert_eq!(stored.amount.to_bits(), order.amount.to_bits());
        assert_eq!(stored.last_update.to_bits(), order.last_update.to_bits());
    }
    for (index, expected) in [(0, 0.0_f64), (1, 0.0), (2, 1.0), (4, 1784505600000.0)] {
        assert_eq!(orders[index].last_update.to_bits(), expected.to_bits());
    }
    assert!(orders[3].last_update.is_nan());
    assert!(orders[0].info.as_ref().unwrap()["created_at"].is_null());
    assert_eq!(orders[1].info.as_ref().unwrap()["created_at"], false);
    assert_eq!(orders[2].info.as_ref().unwrap()["created_at"], true);
    assert!(orders[3].info.as_ref().unwrap().get("created_at").is_none());
}

#[test]
fn constructed_rest_orders_use_host_local_timezone() {
    constructed_orders_use_host_local_timezone(false);
}

#[test]
fn constructed_private_order_timestamps_keep_raw_clock() {
    constructed_orders_use_host_local_timezone(true);
}

fn constructed_orders_use_host_local_timezone(private: bool) {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::process::Command;

    let name = if private {
        "constructed_private_order_timestamps_keep_raw_clock"
    } else {
        "constructed_rest_orders_use_host_local_timezone"
    };
    // Isolate the process timezone, without changing the parallel test runner's
    // environment. The source's new Date(created_at).getTime() runs in Node
    // under the same fixed UTC-7 host profile as the constructed exchange.
    if std::env::var("PARADEX_LOCAL_DATE_CHILD").as_deref() != Ok(name) {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture"])
            .env("TZ", "Etc/GMT+7")
            .env("PARADEX_LOCAL_DATE_CHILD", name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "non-UTC constructed exchange child failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let dates = ["2026-07-20T00:00:00", "2026-07-20T00:00:00Z", "2026-07-20"];
    let output = Command::new("node")
        .args([
            "-e",
            "console.log(JSON.stringify(JSON.parse(process.argv[1]).map(v => new Date(v).getTime())))",
            &serde_json::to_string(&dates).unwrap(),
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut expected: Vec<f64> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(expected.len(), dates.len());
    assert_eq!(expected[0] - expected[1], 7.0 * 60.0 * 60.0 * 1000.0);
    assert_eq!(expected[1], expected[2], "ISO date-only remains UTC");
    let rows: Vec<_> = dates
        .iter()
        .enumerate()
        .map(|(i, date)| {
            json!({
                "id":format!("local-date-{i}"), "market":"BTC-USD-PERP",
                "side":"BUY", "type":"LIMIT", "status":"OPEN", "price":"101.5",
                "size":"0.006", "remaining_size":"0.003", "created_at":date,
                "reduce_only":false
            })
        })
        .collect();
    let conversions = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let captured = conversions.clone();
    let (venue, runtime, store, _) = support::fixture_with_timezone(
        private_options(),
        |wire| {
            private_script(wire);
            if !private {
                wire.responses
                    .borrow_mut()
                    .insert("/orders".into(), json!({"results":rows}));
            }
        },
        // The embedding host supplies its local-to-UTC policy, matching the
        // fixed UTC-7 profile selected for the independent Node witness.
        std::rc::Rc::new(move |wall_time| {
            captured.borrow_mut().push(wall_time);
            wall_time + 7.0 * 60.0 * 60.0 * 1000.0
        }),
    );
    drive(&runtime, venue.start()).unwrap();
    let startup_conversions = conversions.borrow().len();
    if private {
        for _ in 0..8 {
            runtime.pump();
        }
        let socket = runtime.sockets.borrow()[1].1.clone();
        // Private updates use Date.now(), independently of created_at.
        expected.fill(runtime.now.get());
        for row in &rows {
            socket
                .events
                .borrow_mut()
                .push_back(WsEvent::Message(SocketFrame::Text(
                    json!({"jsonrpc":"2.0", "method":"subscription",
                    "params":{"channel":"orders.ALL","data":row}})
                    .to_string(),
                )));
            runtime.pump();
        }
    } else {
        let orders = drive(&runtime, venue.fetch_orders(None)).unwrap();
        assert_eq!(orders.len(), dates.len());
        for (order, expected) in orders.iter().zip(&expected) {
            assert_eq!(
                order.last_update.to_bits(),
                expected.to_bits(),
                "REST {}",
                order.id
            );
        }
    }
    if private {
        assert!(
            conversions.borrow().is_empty(),
            "Date.now bypasses timezone conversion"
        );
    } else {
        assert!(startup_conversions > 0, "startup maps the local date");
        assert_eq!(
            &*conversions.borrow(),
            &vec![expected[1]; startup_conversions + 1],
            "only the local date converts, one more time during the explicit REST read"
        );
    }
    let stored = store.orders();
    assert_eq!(stored.len(), dates.len());
    for (index, expected) in expected.iter().enumerate() {
        let order = stored
            .iter()
            .find(|o| o.id == format!("local-date-{index}"))
            .unwrap();
        assert_eq!(
            order.last_update.to_bits(),
            expected.to_bits(),
            "Store {}",
            order.id
        );
    }
    assert!(
        runtime.errors.borrow().is_empty(),
        "{:?}",
        runtime.errors.borrow()
    );
    venue.dispose().unwrap();
}

#[test]
fn cancel_all_orders_reaches_venue_even_when_store_is_empty() {
    let (venue, runtime, store, wire) = support::fixture_with(private_options(), private_script);
    drive(&runtime, venue.start()).unwrap();
    assert!(store.orders().is_empty());
    let venue: &dyn Exchange = venue.as_ref();
    drive(&runtime, venue.cancel_all_orders()).unwrap();
    let requests = wire.requests.borrow();
    let deletes: Vec<_> = requests.iter().filter(|r| r.method == "delete").collect();
    assert_eq!(
        deletes.len(),
        1,
        "cancel-all must reach venue with an empty Store"
    );
    assert_eq!(deletes[0].url, "/orders");
    assert!(deletes[0].params.is_none());
    assert!(store.orders().is_empty());
}

#[test]
fn cancel_symbol_orders_reaches_venue_even_when_store_is_empty() {
    let (venue, runtime, store, wire) = support::fixture_with(private_options(), private_script);
    drive(&runtime, venue.start()).unwrap();
    assert!(store.orders().is_empty());
    let venue: &dyn Exchange = venue.as_ref();
    drive(&runtime, venue.cancel_symbol_orders("BTC-USD-PERP".into())).unwrap();
    let requests = wire.requests.borrow();
    let deletes: Vec<_> = requests.iter().filter(|r| r.method == "delete").collect();
    assert_eq!(
        deletes.len(),
        1,
        "symbol cancellation must reach venue with an empty Store"
    );
    assert_eq!(deletes[0].url, "/orders?market=BTC-USD-PERP");
    assert!(deletes[0].params.is_none());
    assert!(store.orders().is_empty());
}

#[test]
fn venue_wide_cancellations_propagate_http_errors() {
    for symbol_scoped in [false, true] {
        let (venue, runtime, store, wire) =
            support::fixture_with(private_options(), private_script);
        drive(&runtime, venue.start()).unwrap();
        assert!(store.orders().is_empty());
        wire.statuses.borrow_mut().insert("/orders".into(), 400);
        let venue: &dyn Exchange = venue.as_ref();
        let result = if symbol_scoped {
            drive(&runtime, venue.cancel_symbol_orders("BTC-USD-PERP".into()))
        } else {
            drive(&runtime, venue.cancel_all_orders())
        };
        assert!(
            result.is_err(),
            "venue cancellation must propagate a rejected DELETE"
        );
        assert_eq!(
            wire.requests
                .borrow()
                .iter()
                .filter(|r| r.method == "delete")
                .count(),
            1
        );
        assert!(store.orders().is_empty());
    }
}

fn trait_history_script(wire: &support::Wire) {
    private_script(wire);
    let mut responses = wire.responses.borrow_mut();
    responses.insert(
        "/fills".into(),
        json!({"results":[
            {"id":"open-fill","order_id":"open-order","market":"BTC-USD-PERP","side":"BUY",
             "price":"100","size":"2","fill_size":"2","created_at":1699999900000.0,
             "fee":"0.1","fee_currency":"USD","realized_pnl":"0","liquidity":"MAKER"},
            {"id":"close-fill","order_id":"close-order","market":"BTC-USD-PERP","side":"SELL",
             "price":"110","size":"2","fill_size":"2","created_at":1700000000000.0,
             "fee":"0.1","fee_currency":"USD","realized_pnl":"20","liquidity":"TAKER"}
        ]}),
    );
    responses.insert(
        "/orders-history".into(),
        json!({"results":[
            {"id":"closed-history","market":"BTC-USD-PERP","side":"BUY","type":"LIMIT",
             "status":"CLOSED","size":"2","remaining_size":"0","price":"99",
             "avg_fill_price":"100","created_at":1699999900000.0,"last_updated_at":1700000000000.0}
        ]}),
    );
}

#[test]
fn real_trait_history_fills_reach_authenticated_rows() {
    let (venue, runtime, _, wire) = support::fixture_with(private_options(), trait_history_script);
    drive(&runtime, venue.start()).unwrap();
    let rows = drive(
        &runtime,
        venue.fetch_historical_fills(typed(json!({
            "symbol":"BTC-USD-PERP","from":1699999800000.0,"to":1700000100000.0,"limit":25
        }))),
    )
    .unwrap();
    assert_eq!(rows.len(), 2, "real historical fill row cardinality");
    assert_eq!(rows[0].id.as_deref(), Some("close-fill"));
    assert_eq!(rows[0].timestamp, 1700000000.0);
    assert_eq!(rows[0].realized_pnl, Some(20.0));
    assert_eq!(rows[1].price, 100.0);
    let requests = wire.requests.borrow();
    let read = requests
        .iter()
        .find(|r| r.url.split('?').next() == Some("/fills"))
        .unwrap();
    assert_eq!(trait_request_query(read, "market"), "BTC-USD-PERP");
    assert_eq!(trait_request_query(read, "end_at"), "1700000100000");
    drop(requests);
    venue.dispose().unwrap();
}

#[test]
fn real_trait_history_income_reaches_fill_ledger() {
    let (venue, runtime, _, _) = support::fixture_with(private_options(), trait_history_script);
    drive(&runtime, venue.start()).unwrap();
    let rows = drive(
        &runtime,
        venue.fetch_income_history(Some(typed(json!({
            "startTime":1699999800000.0,"endTime":1700000100000.0
        })))),
    )
    .unwrap();
    assert_eq!(rows.len(), 3, "real fill ledger row cardinality");
    assert_eq!(
        rows.iter()
            .filter(|r| r.income_type == "REALIZED_PNL")
            .count(),
        1
    );
    assert_eq!(
        rows.iter()
            .find(|r| r.income_type == "REALIZED_PNL")
            .unwrap()
            .income,
        20.0
    );
    assert_eq!(
        rows.iter()
            .filter(|r| r.income_type == "COMMISSION")
            .map(|r| r.income)
            .sum::<f64>(),
        -0.2
    );
    venue.dispose().unwrap();
}

#[test]
fn real_trait_history_income_range_keeps_source_filter() {
    let (venue, runtime, _, _) = support::fixture_with(private_options(), trait_history_script);
    drive(&runtime, venue.start()).unwrap();
    let rows = drive(
        &runtime,
        venue.fetch_income_history_range(
            1699999800000.0,
            1700000100000.0,
            Some("REALIZED_PNL".into()),
        ),
    )
    .unwrap();
    assert_eq!(rows.len(), 1, "real filtered ledger row cardinality");
    assert_eq!(rows[0].income_type, "REALIZED_PNL");
    assert_eq!(rows[0].income, 20.0);
    assert_eq!(rows[0].timestamp, 1700000000000.0);
    venue.dispose().unwrap();
}

#[test]
fn real_trait_history_orders_reach_authenticated_rows() {
    let (venue, runtime, _, wire) = support::fixture_with(private_options(), trait_history_script);
    drive(&runtime, venue.start()).unwrap();
    let rows = drive(
        &runtime,
        venue.fetch_historical_orders(Some(typed(json!({
            "symbol":"BTC-USD-PERP","from":1699999800000.0,"to":1700000100000.0,"limit":5
        })))),
    )
    .unwrap();
    assert_eq!(rows.len(), 1, "real historical order row cardinality");
    assert_eq!(rows[0].id, "closed-history");
    assert_eq!(rows[0].filled, 2.0);
    assert_eq!(rows[0].average_price, Some(100.0));
    let requests = wire.requests.borrow();
    let read = requests
        .iter()
        .find(|r| r.url.split('?').next() == Some("/orders-history"))
        .unwrap();
    assert_eq!(trait_request_query(read, "start_at"), "1699999800000");
    drop(requests);
    venue.dispose().unwrap();
}

#[test]
fn real_trait_history_journal_uses_symbol_scoped_fills() {
    let (venue, runtime, _, wire) = support::fixture_with(private_options(), trait_history_script);
    drive(&runtime, venue.start()).unwrap();
    let rows = drive(
        &runtime,
        venue.fetch_historical_positions(safe_cex_exchange::journal::HistoricalPositionOptions {
            account: "paradex-journal".into(),
            from: Some(1699999800000.0),
            to: Some(1700000100000.0),
        }),
    )
    .unwrap();
    assert_eq!(rows.len(), 1, "real journal round-trip cardinality");
    assert_eq!(rows[0]["account"], "paradex-journal");
    assert_eq!(rows[0]["symbol"], "BTC-USD-PERP");
    assert_eq!(rows[0]["settleCurrency"], "USD");
    assert_eq!(rows[0]["realizedPnl"], 20.0);
    assert_eq!(rows[0]["fees"], 0.2);
    assert!(wire.requests.borrow().iter().any(|r| {
        r.url.split('?').next() == Some("/fills")
            && r.url.split_once('?').is_some_and(|(_, query)| {
                query.split('&').any(|pair| pair == "market=BTC-USD-PERP")
            })
    }));
    venue.dispose().unwrap();
}

#[test]
fn real_trait_stream_book_delivers_and_cleans_up_socket_listener() {
    use safe_cex_exchange::websocket::WsEvent;
    use safe_cex_transport::socket::SocketFrame;
    use std::{cell::RefCell, rc::Rc};
    let (venue, runtime, _, _) =
        support::fixture_with(json!({"key":"","secret":"","publicDataOnly":true}), script);
    drive(&runtime, venue.start()).unwrap();
    for _ in 0..16 {
        runtime.pump();
    }
    let seen = Rc::new(RefCell::new(Vec::<safe_cex_types::OrderBook>::new()));
    let sink = seen.clone();
    let cleanup = venue
        .listen_order_book(
            "BTC-USD-PERP".into(),
            Rc::new(move |row| sink.borrow_mut().push(row)),
        )
        .unwrap();
    let channel = "order_book.BTC-USD-PERP.snapshot@15@100ms";
    assert!(runtime.sent.borrow().iter().any(|s| {
        let frame: serde_json::Value = serde_json::from_str(s).unwrap();
        frame["method"] == "subscribe" && frame["params"]["channel"] == channel
    }));
    let socket = runtime.sockets.borrow()[0].1.clone();
    let frame = json!({"jsonrpc":"2.0","method":"subscription","params":{
        "channel":channel,"data":{"market":"BTC-USD-PERP","bids":[["101.25","2"]],"asks":[["102.5","3"]]}
    }}).to_string();
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.clone())));
    for _ in 0..16 {
        runtime.pump();
    }
    assert_eq!(
        seen.borrow().len(),
        1,
        "runtime errors: {:?}; console: {:?}",
        runtime.errors.borrow(),
        runtime.console.borrow()
    );
    assert_eq!(seen.borrow()[0].bids[0].price, 101.25);
    assert_eq!(seen.borrow()[0].asks[0].amount, 3.0);
    cleanup().unwrap();
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame)));
    runtime.now.set(runtime.now.get() + 1000.0);
    for _ in 0..16 {
        runtime.pump();
    }
    assert_eq!(seen.borrow().len(), 1, "book cleanup removes callback");
    assert!(runtime.sent.borrow().iter().any(|s| {
        let frame: serde_json::Value = serde_json::from_str(s).unwrap();
        frame["method"] == "unsubscribe" && frame["params"]["channel"] == channel
    }));
    venue.dispose().unwrap();
}

#[test]
fn real_trait_stream_ohlcv_polls_then_stops_after_cleanup() {
    use std::{cell::RefCell, rc::Rc};
    let (venue, runtime, _, wire) = support::fixture_with(
        json!({"key":"","secret":"","publicDataOnly":true}),
        |wire| {
            script(wire);
            wire.responses.borrow_mut().insert(
                "/markets/klines".into(),
                json!({"results":[[1700000000000.0,101,103,100,102,7]]}),
            );
        },
    );
    drive(&runtime, venue.start()).unwrap();
    let seen = Rc::new(RefCell::new(Vec::<safe_cex_types::Candle>::new()));
    let sink = seen.clone();
    let cleanup = venue
        .listen_ohlcv(
            typed(json!({"symbol":"BTC-USD-PERP","interval":"1m"})),
            Rc::new(move |row| sink.borrow_mut().push(row)),
        )
        .unwrap();
    for _ in 0..32 {
        runtime.pump();
    }
    assert_eq!(seen.borrow().len(), 1, "initial real REST candle poll");
    assert_eq!(seen.borrow()[0].close, 102.0);
    wire.responses.borrow_mut().insert(
        "/markets/klines".into(),
        json!({"results":[[1700000000000.0,101,105,100,104,9]]}),
    );
    runtime.now.set(runtime.now.get() + 3000.0);
    for _ in 0..32 {
        runtime.pump();
    }
    assert_eq!(seen.borrow().last().unwrap().close, 104.0);
    cleanup().unwrap();
    let reads = wire
        .requests
        .borrow()
        .iter()
        .filter(|r| r.url.split('?').next() == Some("/markets/klines"))
        .count();
    assert!(
        reads >= 2,
        "actual initial and follow-up serialized REST reads"
    );
    let delivered = seen.borrow().len();
    wire.responses.borrow_mut().insert(
        "/markets/klines".into(),
        json!({"results":[[1700000000000.0,101,107,100,106,11]]}),
    );
    runtime.now.set(runtime.now.get() + 3000.0);
    for _ in 0..32 {
        runtime.pump();
    }
    assert_eq!(seen.borrow().len(), delivered, "cleanup stops delivery");
    assert_eq!(
        wire.requests
            .borrow()
            .iter()
            .filter(|r| r.url.split('?').next() == Some("/markets/klines"))
            .count(),
        reads,
        "candle cleanup cancels polling"
    );
    assert!(
        runtime.errors.borrow().is_empty(),
        "{:?}",
        runtime.errors.borrow()
    );
    venue.dispose().unwrap();
}

fn trait_request_query<'a>(request: &'a safe_cex_transport::RestRequest, key: &str) -> &'a str {
    request
        .url
        .split_once('?')
        .expect("actual serialized query")
        .1
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == key)
        .expect("actual query parameter")
        .1
}

#[test]
fn actual_order_signer_refuses_infinite_price_before_posting() {
    let (venue, runtime, _, wire) = support::fixture_with(private_options(), private_script);
    drive(&runtime, venue.start()).unwrap();
    let before = wire
        .requests
        .borrow()
        .iter()
        .filter(|request| request.method.eq_ignore_ascii_case("POST") && request.url == "/orders")
        .count();
    let mut opts = limit_order("infinite-price");
    opts.price = Some(f64::INFINITY);
    let error = drive(&runtime, venue.base().place_order(venue.as_ref(), opts)).unwrap_err();
    assert_eq!(
        wire.requests
            .borrow()
            .iter()
            .filter(
                |request| request.method.eq_ignore_ascii_case("POST") && request.url == "/orders"
            )
            .count(),
        before
    );
    assert_eq!(error.message, "Invalid price value for signing: Infinity");
    venue.dispose().unwrap();
}
#[test]
fn actual_amendment_preflight_refuses_infinite_size_before_putting() {
    let (venue, runtime, _, wire) = support::fixture_with(private_options(), private_script);
    drive(&runtime, venue.start()).unwrap();
    let mut opts: safe_cex_types::UpdateOrderOpts =
        typed(json!({"order":reconciled_order(),"update":{"amount":1.0}}));
    opts.update = safe_cex_types::UpdateOrderOptsUpdate::Variant0(
        safe_cex_types::UpdateOrderOptsUpdateVariant0 {
            amount: f64::INFINITY,
            price: None,
        },
    );
    let error = drive(&runtime, venue.update_order(opts)).unwrap_err();
    assert_eq!(
        error.message,
        "Paradex amended total base size must be greater than zero"
    );
    assert!(
        !wire
            .requests
            .borrow()
            .iter()
            .any(|request| request.method.eq_ignore_ascii_case("PUT"))
    );
    venue.dispose().unwrap();
}

fn identity_order(id: &str) -> serde_json::Value {
    json!({"id":id,"market":"BTC-USD-PERP","side":"BUY","type":"LIMIT","status":"OPEN","price":"101.5","size":"0.006","remaining_size":"0.003","created_at":1784505600000i64,"reduce_only":false})
}
fn order_refusal_logs(
    venue: &dyn Exchange,
) -> std::rc::Rc<std::cell::RefCell<Vec<serde_json::Value>>> {
    let logs = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let captured = logs.clone();
    venue.base().on_args(
        "log",
        std::rc::Rc::new(move |_, args| {
            if args[1] == "warning" {
                captured.borrow_mut().push(args[0].clone());
            }
            Ok(())
        }),
    );
    logs
}
#[test]
fn order_identity_refusals_keep_good_rows_and_report_fields() {
    for field in ["id", "market"] {
        for bad in [
            None,
            Some(serde_json::Value::Null),
            Some(json!(17)),
            Some(json!(false)),
            Some(json!({})),
            Some(json!([])),
        ] {
            let (venue, runtime, store, wire) =
                support::fixture_with(private_options(), private_script);
            drive(&runtime, venue.start()).unwrap();
            let mut refused = identity_order("refused");
            match &bad {
                Some(value) => {
                    refused[field] = value.clone();
                }
                None => {
                    refused.as_object_mut().unwrap().remove(field);
                }
            }
            wire.responses.borrow_mut().insert(
                "/orders".into(),
                json!({"results":[identity_order("before"),refused,identity_order("after")]}),
            );
            let logs = order_refusal_logs(venue.as_ref());
            let before = wire.requests.borrow().len();
            let orders = drive(&runtime, venue.fetch_orders(None))
                .expect("one strict identity refusal must not reject the complete order snapshot");
            let ids: Vec<_> = orders.iter().map(|o| o.id.as_str()).collect();
            assert_eq!(ids, ["before", "after"]);
            for order in &orders {
                assert_eq!(order.symbol, "BTC-USD-PERP");
                assert_eq!(order.amount.to_bits(), 0.006_f64.to_bits());
                assert_eq!(order.last_update.to_bits(), 1784505600000_f64.to_bits());
            }
            assert!(
                store.orders().is_empty(),
                "fetch returns the snapshot; caller commits it"
            );
            let logs = logs.borrow();
            assert_eq!(logs.len(), 1);
            let message = logs[0].as_str().unwrap();
            let received = bad
                .as_ref()
                .map_or_else(|| "undefined".into(), serde_json::Value::to_string);
            assert!(message.contains(&format!("Order.{field}")), "{message}");
            assert!(
                message.contains(&format!("received: {received}")),
                "{message}"
            );
            assert_eq!(
                wire.requests.borrow()[before..]
                    .iter()
                    .filter(|r| r.url == "/orders")
                    .count(),
                1
            );
            venue.dispose().unwrap();
        }
    }
}
#[test]
fn order_identity_all_refused_rows_return_empty() {
    let (venue, runtime, _, wire) = support::fixture_with(private_options(), private_script);
    drive(&runtime, venue.start()).unwrap();
    let mut first = identity_order("first");
    first["id"] = json!(17);
    let mut second = identity_order("second");
    second["market"] = serde_json::Value::Null;
    wire.responses
        .borrow_mut()
        .insert("/orders".into(), json!({"results":[first,second]}));
    let logs = order_refusal_logs(venue.as_ref());
    assert!(
        drive(&runtime, venue.fetch_orders(None))
            .expect("all refused rows resolve empty")
            .is_empty()
    );
    assert_eq!(logs.borrow().len(), 2);
    venue.dispose().unwrap();
}
#[test]
fn order_identity_refusal_preserves_later_source_date_error() {
    let (venue, runtime, _, wire) = support::fixture_with(private_options(), private_script);
    drive(&runtime, venue.start()).unwrap();
    let mut bad = identity_order("bad");
    bad["id"] = json!(17);
    bad["created_at"] = json!({"toString":null});
    wire.responses.borrow_mut().insert(
        "/orders".into(),
        json!({"results":[identity_order("before"),bad]}),
    );
    let logs = order_refusal_logs(venue.as_ref());
    let error = drive(&runtime, venue.fetch_orders(None))
        .expect_err("a source failure still rejects the read");
    assert!(
        error
            .message
            .contains("Cannot convert object to primitive value"),
        "{}",
        error.message
    );
    assert!(
        logs.borrow().is_empty(),
        "a source failure must not be reported as a strict field refusal"
    );
    venue.dispose().unwrap();
}

#[test]
fn order_identity_source_keeps_raw_fields_and_date_error() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-row-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "13");
}
