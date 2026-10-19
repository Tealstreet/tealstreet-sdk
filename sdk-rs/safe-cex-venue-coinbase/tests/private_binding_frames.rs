//! Actual Coinbase private socket frames, production mapping, Store and emitter.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod harness;
use harness::{Harness, Http, Socket, harness, options};
use safe_cex_exchange::{exchange::Exchange, websocket::WsEvent};
use safe_cex_transport::socket::SocketFrame;
use safe_cex_types::*;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn script(http: &Http) {
    http.ok("GET", "/api/v3/brokerage/portfolios", json!({"portfolios":[{"name":"Synthetic Spot", "uuid":"portfolio-synthetic", "type":"DEFAULT", "deleted":false}]}));
    http.ok(
        "GET",
        "/api/v3/brokerage/portfolios/portfolio-synthetic",
        json!({"breakdown":{"spot_positions":[{"asset":"USDC","available_to_trade_crypto":13.25,"total_balance_crypto":17.5}]}}),
    );
    http.route(
        "GET",
        "/api/v3/brokerage/cfm/balance_summary",
        403.0,
        json!({"message":"No CFM permissions"}),
    );
    http.ok(
        "GET",
        "/api/v3/brokerage/products",
        json!({"products":[product()]}),
    );
    http.ok(
        "GET",
        "/api/v3/brokerage/orders/historical/batch",
        json!({"orders":[],"has_next":false}),
    );
}
fn product() -> Value {
    json!({"product_id":"BTC-USDC","base_currency_id":"BTC","quote_currency_id":"USDC","price_increment":"0.01","base_increment":"0.001","base_min_size":"0.001","base_max_size":"100", "trading_disabled":false,"view_only":false,"price":"123.5","price_percentage_change_24h":"2.5","volume_24h":"1000"})
}

fn connected(name: ExchangeName) -> (Harness, Rc<Socket>, Rc<RefCell<Vec<Value>>>) {
    let h = harness(name, options(), script);
    h.drive(h.venue().start()).unwrap();
    h.runtime.pump();
    let socket = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url == "wss://advanced-trade-ws-user.coinbase.com")
        .unwrap()
        .1
        .clone();
    let fills = Rc::new(RefCell::new(Vec::new()));
    let observed = fills.clone();
    h.venue().base().on(
        "fill",
        Rc::new(move |_, value| {
            let value = safe_cex_exchange::js_runtime::value::to_json(value).unwrap();
            observed.borrow_mut().push(value.clone());
            Ok(())
        }),
    );
    (h, socket, fills)
}
fn order(status: &str) -> Value {
    json!({"order_id":"ws-native-order","client_order_id":"ws-native-client","product_id":"BTC-USDC",
        "order_type":"LIMIT","order_side":"BUY","status":status,"cumulative_quantity":if status=="FILLED" {"2.5"} else {"0"},
        "leaves_quantity":if status=="FILLED" {"0"} else {"2.5"},"limit_price":"123.45","avg_price":"123.55"})
}
fn send(h: &Harness, socket: &Socket, frame: Value) {
    socket
        .events
        .borrow_mut()
        .push_back(WsEvent::Message(SocketFrame::Text(frame.to_string())));
    h.runtime.pump();
}
fn snapshot(h: &Harness, socket: &Socket) {
    send(
        h,
        socket,
        json!({"channel":"user","events":[{"type":"snapshot","orders":[order("OPEN")]}]}),
    );
}
#[test]
fn real_private_order_snapshot_populates_store() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let (h, socket, fills) = connected(name);
        assert!(h.store.orders().unwrap().is_empty());
        snapshot(&h, &socket);
        let orders = h.store.orders().unwrap();
        assert_eq!(orders.len(), 1, "real private snapshot must reach Store");
        assert_eq!(orders[0].id, "ws-native-order");
        assert_eq!(orders[0].parent_id.as_deref(), Some("ws-native-client"));
        assert_eq!(orders[0].symbol, "BTC-USDC");
        assert_eq!(orders[0].amount, 2.5);
        assert_eq!(orders[0].filled, 0.0);
        assert_eq!(orders[0].remaining, 2.5);
        assert!(h.store.loaded().unwrap().orders);
        assert!(fills.borrow().is_empty(), "snapshot does not emit fill");
        drop(orders);
        h.venue().dispose().unwrap();
    }
}
#[test]
fn real_private_filled_update_closes_order_and_emits_fill() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let (h, socket, fills) = connected(name);
        snapshot(&h, &socket);
        assert_eq!(
            h.store.orders().unwrap().len(),
            1,
            "fill witness must begin with an open order"
        );
        send(
            &h,
            &socket,
            json!({"channel":"user","events":[{"type":"update","orders":[order("FILLED")]}]}),
        );
        let orders = h.store.orders().unwrap();
        assert_eq!(
            orders.len(),
            1,
            "filled order remains in source Store history"
        );
        assert_eq!(
            orders[0].status,
            OrderStatus::Closed,
            "real filled private update must close order"
        );
        assert_eq!(orders[0].filled, 2.5);
        assert_eq!(orders[0].remaining, 0.0);
        drop(orders);
        assert_eq!(
            fills.borrow().as_slice(),
            [
                json!({"id":"ws-native-order","side":"buy","symbol":"BTC-USDC","price":"123.55","amount":2.5})
            ],
            "real filled private update must emit exact fill payload"
        );
        h.venue().dispose().unwrap();
    }
}
fn ignored_topic(topic: &str) {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let (h, socket, fills) = connected(name);
        snapshot(&h, &socket);
        let before_orders = h.store.orders().unwrap().to_vec();
        let before_positions = h.store.positions().unwrap().to_vec();
        let before_balance = h.store.balance().unwrap().clone();
        send(
            &h,
            &socket,
            json!({"channel":topic,"events":[{"type":"update","positions":[{"product_id":"BTC-USDC","number_of_contracts":"999"}],"balances":[{"asset":"USDC","available":"999"}]}]}),
        );
        assert_eq!(
            h.store.orders().unwrap().to_vec(),
            before_orders,
            "unsupported private topic must preserve orders"
        );
        assert_eq!(
            h.store.positions().unwrap().to_vec(),
            before_positions,
            "source has no private position dispatcher"
        );
        assert_eq!(
            h.store.balance().unwrap(),
            before_balance,
            "source has no private balance dispatcher"
        );
        assert!(
            fills.borrow().is_empty(),
            "unsupported private topic must not invent fills"
        );
        h.venue().dispose().unwrap();
    }
}
#[test]
fn real_private_position_frame_preserves_source_noop() {
    ignored_topic("positions");
}
#[test]
fn real_private_balance_frame_preserves_source_noop() {
    ignored_topic("balances");
}

#[test]
fn real_private_write_timeout_emits_forced_toast_once_until_recovery() {
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let (h, socket, _) = connected(name);
        h.http.ok("POST", "/api/v3/brokerage/orders", json!({"success":true,"success_response":{"order_id":"posted-order","product_id":"BTC-USDC","side":"BUY","client_order_id":"toast-check"}}));
        let warnings = Rc::new(RefCell::new(Vec::new()));
        let seen = warnings.clone();
        h.venue().base().on_args(
            "warning",
            Rc::new(move |_, args| {
                let args: Vec<serde_json::Value> = args
                    .iter()
                    .map(|value| safe_cex_exchange::js_runtime::value::to_json(value).unwrap())
                    .collect();
                seen.borrow_mut().push(args.to_vec());
                Ok(())
            }),
        );
        let place = || {
            let opts: PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":2.5,"price":123.45,"clientOrderId":"toast-check"})).unwrap();
            h.drive(Box::pin(
                h.exchange.base().place_order(h.exchange.as_ref(), opts),
            ))
            .unwrap();
            h.runtime.advance(8000.0);
        };
        place();
        let reason = h
            .store
            .socket_health()
            .unwrap()
            .unwrap()
            .into_value()
            .unwrap()["privateDegradedReason"]
            .as_str()
            .unwrap()
            .to_owned();
        assert_eq!(
            warnings.borrow().as_slice(),
            [vec![json!(reason), json!({"forceToast":true})]],
            "production warning must retain the second emitter argument"
        );
        place();
        assert_eq!(
            warnings.borrow().len(),
            1,
            "repeat quiet writes warn only once"
        );
        send(&h, &socket, json!({"channel":"user","events":[]}));
        place();
        assert_eq!(
            warnings.borrow().len(),
            2,
            "a received user frame resets the warning latch"
        );
        assert_eq!(
            warnings.borrow()[1],
            vec![json!(reason), json!({"forceToast":true})]
        );
        h.venue().dispose().unwrap();
    }
}

fn callback_failure(fault: &str) -> safe_cex_exchange::runtime::ExchangeError {
    let mut error =
        safe_cex_exchange::runtime::ExchangeError::new(format!("{fault}-consumer-failed"));
    error.origin = Some(safe_cex_exchange::runtime::ExchangeErrorOrigin::new(
        std::io::Error::other("opaque callback origin"),
    ));
    error
}
fn source_health_callback_profiles() -> Vec<Value> {
    let source = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/health-callback-binding.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        source.status.success(),
        "{}",
        String::from_utf8_lossy(&source.stderr)
    );
    let expected: Vec<Value> = serde_json::from_slice(&source.stdout).unwrap();
    assert_eq!(expected.len(), 2);
    expected
}
fn health_check_write(h: &Harness) {
    let opts: PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTC-USDC","type":"limit","side":"buy","amount":2.5,"price":123.45,"clientOrderId":"health-check"})).unwrap();
    h.drive(Box::pin(
        h.exchange.base().place_order(h.exchange.as_ref(), opts),
    ))
    .unwrap();
    h.runtime.advance(8000.0);
}
fn assert_callback_origin(h: &Harness, original: &safe_cex_exchange::runtime::ExchangeError) {
    let errors = h.runtime.reported_errors.borrow();
    let actual = errors
        .iter()
        .filter(|error| error.message == original.message)
        .collect::<Vec<_>>();
    assert_eq!(
        actual.len(),
        1,
        "host must receive exactly the actual callback rejection: {:?}",
        h.runtime.reports.borrow()
    );
    assert_eq!(
        actual[0].origin, original.origin,
        "original opaque callback cause survives the venue and runtime"
    );
}
fn health_callbacks_preserve_source_throws_and_effect_order(fault: &'static str) {
    let expected = source_health_callback_profiles();
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let index = usize::from(fault == "warning");
        assert_eq!(expected[index]["fault"], fault);
        assert_eq!(expected[index]["sameError"], true);
        let (h, _, _) = connected(name);
        h.http.ok("POST","/api/v3/brokerage/orders",json!({"success":true,"success_response":{"order_id":"health-order","product_id":"BTC-USDC","side":"BUY","client_order_id":"health-check"}}));
        let original = callback_failure(fault);
        let warning_error = original.clone();
        let log_error = original.clone();
        let warnings = Rc::new(RefCell::new(0_u64));
        let observed = warnings.clone();
        h.venue().base().on(
            "warning",
            Rc::new(move |_, _| {
                *observed.borrow_mut() += 1;
                if fault == "warning" {
                    Err(warning_error.clone())
                } else {
                    Ok(())
                }
            }),
        );
        let log_id = h.venue().base().on(
            "log",
            Rc::new(move |_, message| {
                if fault == "log"
                    && message.as_str().is_some_and(|m| {
                        String::from_utf16_lossy(m).contains("Coinbase private WS is not receiving")
                    })
                {
                    Err(log_error.clone())
                } else {
                    Ok(())
                }
            }),
        );
        health_check_write(&h);
        assert!(
            h.store
                .socket_health()
                .unwrap()
                .unwrap()
                .into_value()
                .unwrap()["privateDegradedReason"]
                .as_str()
                .unwrap()
                .contains("Coinbase private WS"),
            "health commits before callback failure"
        );
        assert_eq!(
            *warnings.borrow(),
            expected[index]["warnings"].as_u64().unwrap(),
            "{fault}: source stops subsequent effects on callback failure"
        );
        assert_callback_origin(&h, &original);
        h.venue().base().off("log", Some(&log_id));
        health_check_write(&h);
        assert_eq!(
            *warnings.borrow(),
            expected[index]["recoveryWarnings"].as_u64().unwrap(),
            "{fault}: latch retains source state after the original throw"
        );
        assert_eq!(expected[index]["recoveryWarned"], true);
        assert_callback_origin(&h, &original);
        h.venue().dispose().unwrap();
    }
}
#[test]
fn real_health_log_callback_failure_stops_warning_and_reaches_timer_host() {
    health_callbacks_preserve_source_throws_and_effect_order("log");
}
#[test]
fn real_health_warning_callback_failure_reaches_timer_host() {
    health_callbacks_preserve_source_throws_and_effect_order("warning");
}
#[test]
fn real_auth_failure_log_callback_reaches_socket_host() {
    let expected = source_health_callback_profiles();
    assert_eq!(expected[0]["sameError"], true);
    for name in [ExchangeName::Coinbase, ExchangeName::Coinbasespot] {
        let (h, socket, _) = connected(name);
        let original = callback_failure("auth-log");
        let injected = original.clone();
        h.venue().base().on(
            "log",
            Rc::new(move |_, message| {
                if message.as_str().is_some_and(|m| {
                    String::from_utf16_lossy(m)
                        .contains("Coinbase private WS authentication failed")
                }) {
                    Err(injected.clone())
                } else {
                    Ok(())
                }
            }),
        );
        send(
            &h,
            &socket,
            json!({"type":"error","message":"authentication failed"}),
        );
        assert!(
            h.store
                .socket_health()
                .unwrap()
                .unwrap()
                .into_value()
                .unwrap()["privateDegradedReason"]
                .as_str()
                .unwrap()
                .contains("Coinbase private WS authentication failed")
        );
        assert_callback_origin(&h, &original);
        h.venue().dispose().unwrap();
    }
}
