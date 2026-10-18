#[path = "support/binding_harness.rs"]
mod support;

use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, PlaceOrderOpts};
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

fn check_zero_max(name: ExchangeName, test: &str) {
    if std::env::var_os("BYBIT_ZERO_MAX_CHILD").is_some() {
        let h = support::harness_as(name, support::options(), |wire| {
            wire.ok("GET", "/v5/position/list", json!({"list": []}));
        });
        let market = serde_json::from_value(json!({
            "id":"BTCUSDT", "symbol":"BTCUSDT", "base":"BTC", "quote":"USDT", "active":true,
            "precision":{"amount":0.001,"price":0.1},
            "limits":{"amount":{"min":0.001,"max":0},"leverage":{"min":1,"max":100}}
        }))
        .unwrap();
        Store::update(
            h.store.as_ref(),
            StoreUpdate {
                markets: Some(vec![market]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
        let opts: PlaceOrderOpts = serde_json::from_value(json!({
            "symbol":"BTCUSDT", "type":"limit", "side":"buy", "amount":1, "price":25000
        }))
        .unwrap();
        println!("BYBIT_PLACEMENT_READY");
        std::io::stdout().flush().unwrap();
        let venue = h.venue();
        let ids = h
            .drive(Box::pin(venue.base().place_order(venue, opts)))
            .unwrap();
        assert!(ids.is_empty(), "lodash times(Infinity) produces no lots");
        assert!(h.wire.calls("/v5/order/create").is_empty());
        venue.dispose().unwrap();
        return;
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test, "--nocapture"])
        .env("BYBIT_ZERO_MAX_CHILD", "1")
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (ready, received) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if line.unwrap().ends_with("BYBIT_PLACEMENT_READY") {
                ready.send(()).unwrap();
            }
        }
    });
    if received.recv_timeout(Duration::from_secs(90)).is_err() {
        if child.try_wait().unwrap().is_none() {
            child.kill().unwrap();
        }
        child.wait().unwrap();
        reader.join().unwrap();
        panic!("Bybit fixture did not reach placement");
    }
    let deadline = Instant::now() + Duration::from_millis(400);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            reader.join().unwrap();
            assert!(status.success(), "actual Bybit placement failed: {status}");
            return;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            reader.join().unwrap();
            panic!("actual Bybit placement did not return for maxOrderQty=0");
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn primary_zero_max_returns_no_orders() {
    check_zero_max(ExchangeName::Bybit, "primary_zero_max_returns_no_orders");
}

#[test]
fn demo_zero_max_returns_no_orders() {
    check_zero_max(ExchangeName::Bybitdemo, "demo_zero_max_returns_no_orders");
}
