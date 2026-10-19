//! Live class-member continuations through constructed Scopes and real chart clients.
use super::{
    Rt,
    accepted_read_receivers::terminal::{Harness, harness, options},
};
use crate::{
    classes::{BodyFuture, ClassHost, Kind, NativeMember, Return},
    rest_account::Handle,
};
use safe_cex_exchange::js_runtime::{promise::await_js, value::Js};
use safe_cex_types::ExchangeName;
use std::{
    rc::Rc,
    task::{Context, Poll, Waker},
};

const LIVE: &str = "https://futures.kraken.com/api/charts/v1/trade/PI_XBTUSD/1m";
const DEMO: &str = "https://demo-futures.kraken.com/api/charts/v1/trade/PF_ETHUSD/5m";
fn candles(close: &str) -> serde_json::Value {
    serde_json::json!({"candles":[{"time":3250,"open":"1.125","high":"4.5","low":"-0","close":close,"volume":"7.25"}]})
}
fn opts(symbol: &str, interval: &str, limit: f64) -> Handle<Rt> {
    Rc::new(Js::object(vec![
        ("symbol", Js::str(symbol)),
        ("interval", Js::str(interval)),
        ("limit", Js::Number(limit)),
        ("to", Js::Number(1_700_000_000_000.0)),
    ]))
}
fn member(h: &Harness) -> NativeMember<Rt> {
    ClassHost::member(
        &h.exchange.rt,
        Kind::FuturesExchange,
        "fetchOHLCV",
        "safe-cex-venue-kraken/src/futures_candles.rs::fetch_ohlcv",
    )
    .unwrap()
}
fn body(result: Return<Rt>) -> BodyFuture<Rt> {
    let Return::Future(future) = result else {
        panic!("fetchOHLCV must return a native body continuation");
    };
    future
}
fn complete(hosts: &[&Harness], mut future: BodyFuture<Rt>) -> Handle<Rt> {
    for _ in 0..200 {
        if let Poll::Ready(result) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return result.expect("native member completion");
        }
        for h in hosts {
            h.runtime.pump();
            h.runtime.advance(1.0);
        }
    }
    panic!("native member continuation did not settle");
}
fn assert_close(value: &Js, close: f64) {
    let rows = value
        .array_values()
        .expect("native candles")
        .expect("native array");
    assert_eq!(rows.len(), 1);
    for (field, expected) in [
        ("timestamp", 3.25f64),
        ("open", 1.125),
        ("high", 4.5),
        ("low", -0.0),
        ("close", close),
        ("volume", 7.25),
    ] {
        assert_eq!(
            rows[0].get(field).unwrap().as_f64().unwrap().to_bits(),
            expected.to_bits(),
            "native {field}"
        );
    }
}

#[test]
fn body_future_keeps_held_http_and_completed_native_candle_fields() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", LIVE, candles("3.75"));
        http.hold_responses.set(true);
    });
    let mut future: BodyFuture<Rt> = body(
        member(&h)(
            Rc::new(h.exchange.rt.this.clone()),
            vec![opts("PI_XBTUSD", "1m", 2.0)],
        )
        .unwrap(),
    );
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    h.runtime.pump();
    assert_eq!(h.http.calls(LIVE).len(), 1);
    assert!(
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    h.http.hold_responses.set(false);
    for waker in h.http.response_wakers.borrow_mut().drain(..) {
        waker.wake();
    }
    let value = complete(&[&h], future);
    assert_close(&value, 3.75);
    let calls = h.http.calls(LIVE);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].url,
        format!("{LIVE}?from=1699999880&to=1700000000")
    );
}

#[test]
fn native_member_uses_supplied_constructed_receiver_and_original_arguments() {
    let original = harness(ExchangeName::Kraken, options(), |_| {});
    let mut testnet = options();
    testnet.testnet = Some(true);
    let supplied = harness(ExchangeName::Kraken, testnet, |http| {
        http.ok("GET", DEMO, candles("8.875"))
    });
    let unbound: NativeMember<Rt> = member(&original);
    let future = body(
        unbound(
            Rc::new(supplied.exchange.rt.this.clone()),
            vec![opts("PF_ETHUSD", "5m", 3.0)],
        )
        .unwrap(),
    );
    let value = complete(&[&original, &supplied], future);
    assert_close(&value, 8.875);
    assert!(
        original.http.requests.borrow().is_empty(),
        "supplied receiver selects its actual client"
    );
    let calls = supplied.http.calls(DEMO);
    assert_eq!(calls.len(), 1);
    assert_eq!(
        calls[0].url,
        format!("{DEMO}?from=1699999100&to=1700000000")
    );
}

#[test]
fn bound_member_arrow_keeps_captured_receiver_arguments_and_promise_result() {
    let captured = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", LIVE, candles("3.875"))
    });
    let mut testnet = options();
    testnet.testnet = Some(true);
    let foreign = harness(ExchangeName::Kraken, testnet, |_| {});
    // The real constructor initialized this arrow through BoundMember and arrow().
    // Calling it with another actual exchange as `this` must retain lexical this.
    let arrow = captured.exchange.rt.this.get("fetchOHLCV").unwrap();
    let promise = arrow
        .call(
            foreign.exchange.rt.this.clone(),
            vec![(*opts("PI_XBTUSD", "1m", 2.0)).clone()],
        )
        .unwrap();
    let runtime = captured.exchange.rt.exchange_runtime();
    let value = complete(
        &[&captured, &foreign],
        Box::pin(async move { await_js(&runtime, promise).await.map(Rc::new) }),
    );
    assert_close(&value, 3.875);
    assert_eq!(captured.http.calls(LIVE).len(), 1);
    assert!(
        foreign.http.requests.borrow().is_empty(),
        "arrow must ignore a foreign call receiver"
    );
}
