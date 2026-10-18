//! The production Blofin market refresh keeps every row when one has a bad contract value.
#[allow(dead_code)]
#[path = "native_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use serde_json::{Value, json};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};
fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return v;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("market refresh did not settle");
}
fn row(id: &str, contract_value: Value) -> Value {
    json!({"instId": id, "instType": "SWAP", "contractType": "linear", "baseCurrency": "BTC",
        "quoteCurrency": "USDT", "contractValue": contract_value, "lotSize": "1", "minSize": "1",
        "tickSize": "0.1", "maxLeverage": "100", "maxLimitSize": "1000", "maxMarketSize": "500",
        "state": "live"})
}

#[test]
fn a_bad_contract_value_row_does_not_hide_the_other_markets() {
    let (venue, rt, _, wire) = support::fixture();
    wire.bodies.borrow_mut().insert(
        "/api/v1/market/instruments".into(),
        json!({"code": "0", "data": [
            row("BTC-USDT", json!("0.001")),
            row("NEW-USDT", json!("bad")),
            row("ETH-USDT", Value::Null),
        ]}),
    );
    let markets = drive(&rt, venue.fetch_markets()).unwrap();
    let ids: Vec<&str> = markets.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(
        ids,
        ["BTC-USDT", "NEW-USDT", "ETH-USDT"],
        "every row maps, as in TS"
    );
    assert!(markets[1].contract_size.unwrap().is_nan());
    assert_eq!(markets[0].contract_size, Some(0.001));
}
