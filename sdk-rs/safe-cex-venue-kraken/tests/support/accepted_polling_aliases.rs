//! Production polling continuations over a constructed Futures Scope and real client.
use super::{
    Rt,
    accepted_read_receivers::terminal::{Harness, harness, options},
};
use crate::{
    ohlcv_polling::{self, CandleCallback, Cleanup, PollHost, Slot},
    rest_account::Handle,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown};
use safe_cex_types::ExchangeName;
use std::{cell::RefCell, rc::Rc};

const URL: &str = "https://futures.kraken.com/api/charts/v1/trade/PI_XBTUSD/1m";
const KEY: &[u16] = &[80, 73, 95, 88, 66, 84, 85, 83, 68, 58, 49, 109];

fn response(close: &str, volume: &str) -> serde_json::Value {
    serde_json::json!({"candles":[
        {"time":1000,"open":"1","high":"2","low":"0.5","close":"1.5","volume":"2.5"},
        {"time":2000,"open":"2.25","high":"4.5","low":"-0","close":close,"volume":volume}
    ]})
}
fn start(h: &Harness, rows: Rc<RefCell<Vec<Handle<Rt>>>>) -> Cleanup<Rt> {
    let callback: CandleCallback<Rt> = Rc::new(move |candle| {
        rows.borrow_mut().push(candle);
        Ok(())
    });
    ohlcv_polling::listen_ohlcv(
        Rc::new(h.exchange.rt.clone()),
        Rc::new(Js::object(vec![
            ("symbol", Js::str("PI_XBTUSD")),
            ("interval", Js::str("1m")),
        ])),
        callback,
    )
    .unwrap()
}
fn release(h: &Harness) {
    h.http.hold_responses.set(false);
    for waker in h.http.response_wakers.borrow_mut().drain(..) {
        waker.wake();
    }
    h.runtime.pump();
}

#[test]
fn poll_future_retains_held_native_http_completion_and_latest_candle() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", URL, response("3.125", "5.75"));
        http.hold_responses.set(true);
    });
    let rows = Rc::new(RefCell::new(vec![]));
    let cleanup = start(&h, rows.clone());
    h.runtime.pump();
    let calls = h.http.calls(URL);
    assert_eq!(
        calls.len(),
        1,
        "production PollFuture started one real chart request"
    );
    let prefix = format!("{URL}?from=");
    let query = calls[0]
        .url
        .strip_prefix(&prefix)
        .expect("source chart query");
    let (from, to) = query.split_once("&to=").expect("both source window bounds");
    let from: i64 = from.parse().expect("finite second-valued lower bound");
    let to: i64 = to.parse().expect("finite second-valued upper bound");
    assert!(to > 0);
    assert_eq!(to - from, 120, "source limit=2 at one-minute resolution");
    assert!(
        rows.borrow().is_empty(),
        "held REST completion must not emit early"
    );
    release(&h);
    let rows = rows.borrow();
    assert_eq!(rows.len(), 1);
    let candle = &rows[0];
    for (field, expected) in [
        ("timestamp", 2.0f64),
        ("open", 2.25),
        ("high", 4.5),
        ("low", -0.0),
        ("close", 3.125),
        ("volume", 5.75),
    ] {
        assert_eq!(
            candle.get(field).unwrap().as_f64().unwrap().to_bits(),
            expected.to_bits(),
            "native {field}"
        );
    }
    let cache = PollHost::map_get(&h.exchange.rt, Slot::Candle, KEY).unwrap();
    assert!(cache.strict_equals(candle));
    cleanup().unwrap();
}

#[test]
fn poll_callback_runs_again_at_source_five_second_delay_without_duplicate_emit() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", URL, response("3.125", "5.75"))
    });
    let rows = Rc::new(RefCell::new(vec![]));
    let cleanup = start(&h, rows.clone());
    h.runtime.pump();
    assert_eq!(h.http.calls(URL).len(), 1);
    assert_eq!(rows.borrow().len(), 1);
    h.runtime.advance(4999.0);
    assert_eq!(
        h.http.calls(URL).len(),
        1,
        "no timer callback before deadline"
    );
    h.runtime.advance(1.0);
    assert_eq!(
        h.http.calls(URL).len(),
        2,
        "real PollCallback drives its next REST request at 5000ms"
    );
    assert_eq!(rows.borrow().len(), 1, "unchanged candle is suppressed");
    assert!(PollHost::map_has(&h.exchange.rt, Slot::Timeout, KEY).unwrap());
    cleanup().unwrap();
}

#[test]
fn candle_callback_keeps_latest_native_cell_identity_and_changed_fields() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", URL, response("3.125", "5.75"));
        http.ok("GET", URL, response("3.125", "5.75"));
        http.ok("GET", URL, response("3.875", "6.25"));
    });
    let rows = Rc::new(RefCell::new(vec![]));
    let sink = rows.clone();
    let callback: CandleCallback<Rt> = Rc::new(move |candle| {
        candle.set("consumerTag", Js::utf16(&[0xd800]))?;
        sink.borrow_mut().push(candle);
        Ok(())
    });
    let cleanup = ohlcv_polling::listen_ohlcv(
        Rc::new(h.exchange.rt.clone()),
        Rc::new(Js::object(vec![
            ("symbol", Js::str("PI_XBTUSD")),
            ("interval", Js::str("1m")),
        ])),
        callback,
    )
    .unwrap();
    h.runtime.pump();
    h.runtime.advance(5000.0);
    assert_eq!(rows.borrow().len(), 1);
    h.runtime.advance(5000.0);
    let rows = rows.borrow();
    assert_eq!(
        rows.len(),
        2,
        "only the two distinct latest candles are emitted"
    );
    assert_eq!(
        rows[1].get("close").unwrap().as_f64().unwrap().to_bits(),
        3.875f64.to_bits()
    );
    assert_eq!(
        rows[1].get("volume").unwrap().as_f64().unwrap().to_bits(),
        6.25f64.to_bits()
    );
    assert!(!rows[0].strict_equals(&rows[1]));
    let cache = PollHost::map_get(&h.exchange.rt, Slot::Candle, KEY).unwrap();
    assert!(cache.strict_equals(&rows[1]));
    assert_eq!(
        cache.get("consumerTag").unwrap().as_str().unwrap().as_ref(),
        &[0xd800]
    );
    cleanup().unwrap();
}

#[test]
fn cleanup_alias_removes_native_cache_timer_and_stops_future_http() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok("GET", URL, response("3.125", "5.75"))
    });
    let rows = Rc::new(RefCell::new(vec![]));
    let cleanup: Cleanup<Rt> = start(&h, rows.clone());
    let independently_typed: &Rc<dyn Fn() -> Result<(), Thrown>> = &cleanup;
    h.runtime.pump();
    assert_eq!(rows.borrow().len(), 1);
    assert!(PollHost::map_has(&h.exchange.rt, Slot::Timeout, KEY).unwrap());
    assert!(PollHost::map_has(&h.exchange.rt, Slot::Candle, KEY).unwrap());
    independently_typed().unwrap();
    assert!(
        !PollHost::map_get(&h.exchange.rt, Slot::Active, KEY)
            .unwrap()
            .truthy()
    );
    assert!(!PollHost::map_has(&h.exchange.rt, Slot::Timeout, KEY).unwrap());
    assert!(
        !PollHost::map_has(&h.exchange.rt, Slot::Candle, KEY).unwrap(),
        "cleanup removes the native candle cache"
    );
    cleanup().unwrap();
    h.runtime.advance(15000.0);
    assert_eq!(
        h.http.calls(URL).len(),
        1,
        "cleanup cancels the actual repeating HTTP poll"
    );
    assert_eq!(rows.borrow().len(), 1);
}
