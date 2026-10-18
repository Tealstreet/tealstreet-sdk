#[path = "support/adapter_runtime.rs"]
#[allow(dead_code)]
mod support;

use safe_cex_exchange::exchange::Exchange;
use safe_cex_transport::RestResponseBody;
use safe_cex_types::Market;
use serde_json::json;
use std::{
    future::Future,
    io::Write,
    process::{Command, Stdio},
    task::{Context, Poll, Waker},
};

fn markets() -> Vec<Market> {
    let (owner, runtime, _, wire) = support::fixture();
    wire.bodies.lock().unwrap().insert(
        "/api/v1/markets".into(),
        RestResponseBody::Untransformed(safe_cex_types::retained_json::RetainedJsonInput::from_value(json!([
            {"symbol":"BTC_USDC_PERP","marketType":"PERP","filters":{"price":{"minPrice":"5.25","maxPrice":"900","tickSize":"0.25"}}},
            {"symbol":"BTC_USDC","marketType":"SPOT","filters":{"price":{"minPrice":"0.5","maxPrice":"800","tickSize":"0.1"}}}
        ])).unwrap()),
    );
    let mut future = std::pin::pin!(Exchange::fetch_markets(owner.as_ref()));
    let result = (0..64).find_map(|_| {
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(result) => Some(result.unwrap()),
            Poll::Pending => {
                runtime.pump();
                runtime.now.set(runtime.now.get() + 100.0);
                None
            }
        }
    });
    let result = result.expect("real market request must settle");
    let requests = wire.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method.to_uppercase(), "GET");
    assert_eq!(requests[0].url, "/api/v1/markets");
    assert_eq!(result.len(), 2);
    result
}

#[test]
fn fetched_markets_enforce_the_source_minimum_price_guard() {
    let markets = markets();
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/minimum_price_guard.cjs"
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
        .write_all(&serde_json::to_vec(&markets).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "actual TS guard: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(markets[0].limits.price.as_ref().unwrap().min, 5.25);
    assert_eq!(markets[1].limits.price.as_ref().unwrap().min, 0.5);
}

#[test]
fn fetched_markets_do_not_invent_is_perpetual() {
    let markets = markets();
    for (market, is_spot) in markets.iter().zip([false, true]) {
        assert_eq!(market.is_spot, Some(is_spot));
        assert_eq!(market.is_perpetual, None, "TS has swap, not isPerpetual");
    }
}

#[test]
fn fetched_markets_do_not_invent_is_expiring() {
    for market in markets() {
        assert_eq!(market.is_expiring, None, "TS has future, not isExpiring");
    }
}

#[test]
fn fetched_markets_do_not_invent_is_inverse() {
    for market in markets() {
        assert_eq!(market.is_inverse, None, "TS has inverse, not isInverse");
    }
}
