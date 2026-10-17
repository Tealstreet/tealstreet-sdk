#![cfg(not(target_arch = "wasm32"))]
#[path = "support/native_account.rs"]
mod account;
mod support;
use account::{Account, run, started};
use safe_cex_registry::native_account::native_account_watch;
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc, time::Duration};

fn subscribed(h: &Account, kind: &str, channel: &str) -> bool {
    h.received.borrow().iter().any(|frame| {
        let frame: Value = serde_json::from_str(frame).unwrap();
        frame["type"] == kind && frame["channel"] == channel
    })
}
async fn stream(method: &str, args: Value, channel: &str, data: Value) {
    let h = started().await;
    let events = Rc::new(RefCell::new(vec![]));
    let captured = events.clone();
    let unwatch = native_account_watch(
        &h.account,
        Rc::new(move |event, data| {
            captured
                .borrow_mut()
                .push((event.to_string(), support::boundary_json(data.clone())))
        }),
        Rc::new(|_| {}),
    );
    let id = h.ok(method, args).await;
    assert_eq!(id.as_f64(), Some(1.0), "native callback cleanup handle");
    support::eventually("native gateway subscription", || {
        subscribed(&h, "subscribe", channel)
    })
    .await;
    let mut frame = json!({"type":"data","exchangeId":"binance","channel":channel,"symbol":"BTCUSDT","data":data});
    if channel == "candle" {
        frame["interval"] = json!("1m");
    }
    h.peers.borrow()[0].push.send(frame.to_string()).unwrap();
    support::eventually("native callback forwarded to host", || {
        events.borrow().iter().any(|(event, _)| event == method)
    })
    .await;
    let payload = events
        .borrow()
        .iter()
        .find(|(event, _)| event == method)
        .unwrap()
        .1
        .clone();
    assert_eq!(payload["subscriptionId"], id);
    assert_eq!(payload["data"], frame["data"]);
    h.ok("unlisten", json!([id])).await;
    h.ok("unlisten", json!([id])).await;
    support::eventually("native gateway unsubscribe", || {
        subscribed(&h, "unsubscribe", channel)
    })
    .await;
    let count = events
        .borrow()
        .iter()
        .filter(|(event, _)| event == method)
        .count();
    h.peers.borrow()[0].push.send(frame.to_string()).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        events
            .borrow()
            .iter()
            .filter(|(event, _)| event == method)
            .count(),
        count
    );
    unwatch();
    h.ok("dispose", json!([])).await;
}
#[test]
fn forwards_live_candles_and_runs_the_real_unsubscribe() {
    run(stream(
        "listenOHLCV",
        json!([{"symbol":"BTCUSDT","interval":"1m"}]),
        "candle",
        json!({"timestamp":1700000000000.0,"open":9.0,"high":12.0,"low":8.0,"close":11.0,"volume":17.0}),
    ));
}
#[test]
fn forwards_live_books_and_runs_the_real_unsubscribe() {
    run(stream(
        "listenOrderBook",
        json!(["BTCUSDT"]),
        "orderbook",
        json!({"bids":[{"price":9.9,"amount":2.0,"total":2.0}],"asks":[{"price":10.1,"amount":3.0,"total":3.0}]}),
    ));
}
#[test]
fn bulk_candles_reach_binances_source_base_noop() {
    run(async {
        let h = started().await;
        let id = h
            .ok(
                "listenOHLCVs",
                json!([{"symbols":["BTCUSDT"],"interval":"1m"}]),
            )
            .await;
        assert_eq!(id.as_f64(), Some(1.0));
        h.ok("unlisten", json!([id])).await;
        assert!(!subscribed(&h, "subscribe", "candle"));
        assert!(h.seen.borrow().is_empty());
        h.ok("dispose", json!([])).await;
    });
}
#[test]
fn dispose_closes_live_stream_registrations() {
    run(async {
        let h = started().await;
        h.ok("listenOHLCV", json!([{"symbol":"BTCUSDT","interval":"1m"}]))
            .await;
        support::eventually("native subscription", || {
            subscribed(&h, "subscribe", "candle")
        })
        .await;
        h.ok("dispose", json!([])).await;
        // The provider may close the whole socket before an unsubscribe frame is
        // delivered; invoking the cleanup must still be accepted exactly once.
        h.ok("unlisten", json!([1])).await;
    });
}
