//! Native synchronized-clock proof through the production REST and private socket bindings.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use serde_json::{Value, json};
use support::{Harness, harness, options};
const SYMBOL: &str = "BTCUSD-PERP";

fn ok(result: Value) -> Value {
    json!({"id": 1, "method": "m", "code": 0, "result": result})
}

fn instruments() -> Value {
    ok(json!({"data": [{
        "inst_type": "PERPETUAL_SWAP", "tradable": true, "symbol": SYMBOL, "base_ccy": "BTC",
        "quote_ccy": "USD", "qty_tick_size": "0.0001", "price_tick_size": "0.5",
        "contract_size": "1", "underlying_symbol": "BTCUSD", "quantity_decimals": 4,
        "quote_decimals": 1, "max_leverage": "50"
    }, {
        "inst_type": "CCY_PAIR", "tradable": true, "symbol": "BTC_USD", "base_ccy": "BTC",
        "quote_ccy": "USD", "qty_tick_size": "0.0001", "price_tick_size": "0.5",
        "contract_size": "1", "underlying_symbol": "BTCUSD", "quantity_decimals": 4,
        "quote_decimals": 1, "max_leverage": "1"
    }]}))
}

fn tickers() -> Value {
    ok(json!({"data": [
        {"i": SYMBOL, "b": "30000", "k": "30001", "a": "30000.5", "c": "0.012", "v": "8", "vv": "81", "oi": "6"},
        {"i": "BTC_USD", "b": "1", "k": "2", "a": "1.5", "c": "0", "v": "1", "vv": "1", "oi": "0"}
    ]}))
}

fn open_orders() -> Value {
    ok(json!({"data": [{
        "order_id": "O1", "client_oid": "c-1", "instrument_name": SYMBOL, "quantity": "4",
        "cumulative_quantity": "1.5", "limit_price": "29000", "order_type": "LIMIT",
        "side": "BUY", "status": "ACTIVE", "update_time": 33, "create_time": 11,
        "time_in_force": "GOOD_TILL_CANCEL"
    }]}))
}

fn advanced_orders() -> Value {
    ok(json!({"data": [{
        "order_id": "A2", "instrument_name": SYMBOL, "quantity": "4", "cumulative_quantity": "0",
        "ref_price": "25000", "order_type": "STOP_LOSS", "side": "SELL", "status": "ACTIVE",
        "update_time": 33, "create_time": 11, "exec_inst": ["REDUCE_ONLY"]
    }]}))
}

fn positions() -> Value {
    ok(json!({"data": [{
        "instrument_name": SYMBOL, "quantity": "-3", "cost": "-90000", "open_position_pnl": "2",
        "isolation_type": "CROSS_MARGIN", "update_timestamp_ms": 44
    }]}))
}

fn balance() -> Value {
    ok(json!({"data": [{
        "total_margin_balance": "123", "total_available_balance": "90",
        "total_initial_margin": "33", "total_session_unrealized_pnl": "-4",
        "position_balances": [{"instrument_name": "USD", "quantity": "15", "max_withdrawal_balance": "9"}]
    }]}))
}

fn venue(script: impl FnOnce(&support::Http)) -> Harness {
    harness(options(), |http| {
        http.ok("GET", "/public/get-instruments", instruments());
        http.ok("GET", "/public/get-tickers", tickers());
        script(http);
    })
}

struct ResetClock;
impl Drop for ResetClock {
    fn drop(&mut self) {
        VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)());
    }
}
fn frame(socket: &support::Socket, value: Value) {
    socket
        .events
        .borrow_mut()
        .push_back(safe_cex_exchange::websocket::WsEvent::Message(
            safe_cex_transport::socket::SocketFrame::Text(value.to_string()),
        ));
}
fn check_auth(body: &Value, before: f64, after: f64, label: &str, errors: &mut Vec<String>) {
    for field in ["id", "nonce"] {
        let value = body[field].as_f64().unwrap();
        if !(before..=after).contains(&value) {
            errors.push(format!("{label} raw {field}={value}"));
        }
    }
    assert_eq!(body["params"].as_object().map(|p| p.len()).unwrap_or(0), 0);
    let payload = format!(
        "{}{}fixture-key{}",
        body["method"].as_str().unwrap(),
        body["id"].as_f64().unwrap(),
        body["nonce"].as_f64().unwrap()
    );
    assert_eq!(
        body["sig"],
        safe_cex_crypto::hmac_sha256(
            b"fixture-secret",
            payload.as_bytes(),
            safe_cex_crypto::Encoding::Hex
        ),
        "{label} actual HMAC covers the emitted clock values"
    );
}
#[test]
fn native_synced_clock_reaches_rest_and_private_auth() {
    let _reset = ResetClock;
    VIRTUAL_CLOCK.sync_from_server_time((VIRTUAL_CLOCK.now)() + 40_000.0);
    let h = venue(|http| {
        http.ok(
            "POST",
            "/private/get-accounts",
            ok(json!({"master_account":{},"sub_account_list":[],"this_account":{"uuid":"u-1"}})),
        );
        http.ok("POST", "/private/get-open-orders", open_orders());
        http.ok(
            "POST",
            "/private/advanced/get-open-orders",
            advanced_orders(),
        );
        http.ok("POST", "/private/get-positions", positions());
        http.ok("POST", "/private/user-balance", balance());
    });
    let mut errors = Vec::new();
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    assert_eq!(h.drive(h.venue().get_account()).unwrap().user_id, "u-1");
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let calls = h.http.calls("/private/get-accounts");
    assert_eq!(calls.len(), 1);
    let request: Value = serde_json::from_str(calls[0].data.as_str().unwrap()).unwrap();
    check_auth(&request, before, after, "REST", &mut errors);
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    h.drive(h.venue().start()).unwrap();
    h.runtime.advance(1_000.0);
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let private = h
        .runtime
        .sockets
        .borrow()
        .iter()
        .find(|(url, _)| url.ends_with("/user"))
        .unwrap()
        .1
        .clone();
    let auth: Value = private
        .sent
        .borrow()
        .iter()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .find(|v| v["method"] == "public/auth")
        .unwrap();
    check_auth(&auth, before, after, "private auth", &mut errors);
    frame(&private, json!({"method":"public/auth","code":0}));
    h.runtime.pump();
    assert!(
        private
            .sent
            .borrow()
            .iter()
            .any(|text| serde_json::from_str::<Value>(text).unwrap()["method"] == "subscribe")
    );
    let before = VIRTUAL_CLOCK.get_current_time_ms();
    frame(
        &private,
        json!({"code":0,"result":{"channel":"user.order","data":[{
            "order_id":"synced-order", "instrument_name":SYMBOL,"quantity":"4","cumulative_quantity":"1.5",
            "limit_price":"29001.5","order_type":"LIMIT","side":"BUY","status":"ACTIVE",
            "time_in_force":"GOOD_TILL_CANCEL","exec_inst":["REDUCE_ONLY"]
        }]}}),
    );
    h.runtime.pump();
    let after = VIRTUAL_CLOCK.get_current_time_ms();
    let orders = h.store.orders().unwrap();
    let order = orders.iter().find(|o| o.id == "synced-order").unwrap();
    assert_eq!(order.filled, 1.5);
    assert!(
        (before..=after).contains(&order.last_update),
        "private fallback model timestamp reads the same synchronized clock"
    );
    drop(orders);
    h.venue().dispose().unwrap();
    h.runtime.pump();
    assert!(
        errors.is_empty(),
        "native synchronized signing clock diverges:\n{}",
        errors.join("\n")
    );
}
