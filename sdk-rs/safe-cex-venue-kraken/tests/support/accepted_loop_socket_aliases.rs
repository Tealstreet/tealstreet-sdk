//! The actual socket and timer callback capabilities of a constructed Kraken Scope.
use super::accepted_read_receivers::terminal::{harness, options};
use crate::{
    classes,
    lifecycle::{self, ConnectionHost, SocketCallback},
    periodic_sync::{TickCallback, TickHost},
};
use safe_cex_exchange::{
    js_runtime::value::{Js, Thrown, error_object},
    runtime::{ExchangeError, ExchangeRuntime},
    websocket::WsEvent,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{ExchangeName, Ticker};
use std::{cell::Cell, rc::Rc};

fn ticker() -> Ticker {
    Ticker {
        id: "PF_XBTUSD".into(),
        symbol: "PF_XBTUSD".into(),
        bid: 1.0,
        ask: 2.0,
        last: 3.0,
        mark: 4.0,
        index: 5.0,
        mid_price: None,
        percentage: 6.0,
        open_interest: 7.0,
        funding_rate: 8.0,
        funding_time: 9.0,
        volume: 10.0,
        quote_volume: 11.0,
        timestamp: None,
        info: None,
    }
}

#[test]
fn socket_callback_reaches_real_message_dispatch_and_native_store() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    h.store
        .update(
            StoreUpdate {
                tickers: Some(vec![ticker()]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    let rt = &h.exchange.rt;
    let socket = classes::futures_public(rt, Rc::new(rt.this.clone())).unwrap();
    let host = rt.shared.extra.sockets.borrow().last().unwrap().clone();
    assert!(host.rt.this.strict_equals(&socket));
    lifecycle::futures_public_connect(host.clone()).unwrap();
    h.runtime.pump();
    h.runtime.advance(100.0);
    assert_eq!(h.runtime.sockets.borrow().len(), 1);
    let physical = h.runtime.sockets.borrow()[0].1.clone();
    physical.events.borrow_mut().push_back(WsEvent::Message(
        safe_cex_transport::socket::SocketFrame::Text(r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":"-0","bid":"12.5","relative_funding_rate":"0.003125"}"#.into())
    ));
    h.runtime.pump();
    let rows = h.store.tickers().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].last.to_bits(),
        (-0.0_f64).to_bits(),
        "physical message uses installed SocketCallback"
    );
    assert_eq!(rows[0].bid, 12.5);
    assert_eq!(rows[0].funding_rate, 0.003125);

    let callback: SocketCallback = ConnectionHost::on_message(host.as_ref());
    let retained = callback.clone();
    assert!(Rc::ptr_eq(&callback, &retained));
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    let event = Js::object(vec![]);
    event
        .define_accessor(
            "data",
            Some(Rc::new(move |_| {
                count.set(count.get() + 1);
                Ok(Js::str(
                    r#"{"feed":"ticker","product_id":"PF_XBTUSD","last":"17.25","ask":"19.5"}"#,
                ))
            })),
            None,
        )
        .unwrap();
    retained(&event).unwrap();
    assert_eq!(reads.get(), 1, "original event cell is read once");
    let rows = h.store.tickers().unwrap();
    assert_eq!(rows[0].last, 17.25);
    assert_eq!(rows[0].ask, 19.5);
    assert_eq!(
        rows[0].bid, 12.5,
        "unmentioned field survives a second callback"
    );

    let error = error_object("RangeError", "event data getter");
    let cause = error.clone();
    let event = Js::object(vec![]);
    event
        .define_accessor(
            "data",
            Some(Rc::new(move |_| Err(Thrown::new(cause.clone())))),
            None,
        )
        .unwrap();
    let error = callback(&event).unwrap_err();
    assert_eq!(error.name, "RangeError");
    assert_eq!(error.message, "event data getter");
    assert_eq!(h.store.tickers().unwrap()[0].last, 17.25);
}

#[test]
fn tick_callback_starts_real_signed_sync_only_at_its_deadline() {
    let h = harness(ExchangeName::Kraken, options(), |http| {
        http.ok(
            "GET",
            "/v3/leveragepreferences",
            serde_json::json!({"result":"success","leveragePreferences":[]}),
        );
        http.ok(
            "GET",
            "/v3/openorders",
            serde_json::json!({"result":"success","openOrders":[]}),
        );
        http.ok(
            "GET",
            "/v3/openpositions",
            serde_json::json!({"result":"success","openPositions":[]}),
        );
        http.ok("GET", "/v3/accounts", serde_json::json!({"result":"success","accounts":{"flex":{
            "type":"multiCollateralMarginAccount","portfolioValue":103.25,"availableMargin":80.5,"totalUnrealized":-2.75,"currencies":{}
        }}}));
        http.hold_responses.set(true);
    });
    let rt = h.exchange.rt.clone();
    let calls = Rc::new(Cell::new(0));
    let count = calls.clone();
    let callback: TickCallback = Rc::new(move || {
        count.set(count.get() + 1);
        TickHost::current_tick(&rt, 20.0)
    });
    let retained = callback.clone();
    assert!(Rc::ptr_eq(&callback, &retained));
    TickHost::schedule_loop(&h.exchange.rt, retained, 500.0).unwrap();
    h.runtime.pump();
    h.runtime.advance(499.0);
    assert_eq!(calls.get(), 0);
    assert!(h.http.requests.borrow().is_empty());
    h.runtime.advance(1.0);
    assert_eq!(calls.get(), 1);
    assert_eq!(h.http.calls("/v3/leveragepreferences").len(), 1);
    assert!(
        !h.store.loaded().unwrap().orders,
        "held native completion cannot mark loaded"
    );
    h.http.hold_responses.set(false);
    for waker in h.http.response_wakers.borrow_mut().drain(..) {
        waker.wake();
    }
    h.runtime.pump();
    let loaded = h.store.loaded().unwrap();
    assert!(
        loaded.orders && loaded.positions && loaded.balance,
        "real tick Promise completes its Store continuations"
    );
    assert_eq!(h.store.balance().unwrap().total, 103.25);
    assert_eq!(h.store.balance().unwrap().free, 80.5);
    for path in [
        "/v3/leveragepreferences",
        "/v3/openorders",
        "/v3/openpositions",
        "/v3/accounts",
    ] {
        let rows = h.http.calls(path);
        assert_eq!(rows.len(), 1, "one actual signed request for {path}");
        assert_eq!(rows[0].method, "GET");
        assert_eq!(rows[0].headers["APIKey"], "fixture-key");
        assert!(!rows[0].headers["Authent"].as_str().unwrap().is_empty());
    }
    assert!(h.runtime.reports.borrow().is_empty());
    h.runtime.advance(500.0);
    assert_eq!(calls.get(), 1, "the explicit capability is called once");
    assert_eq!(
        h.http.calls("/v3/accounts").len(),
        1,
        "next non-sync tick does not fetch again"
    );
}

#[test]
fn tick_callback_reports_synchronous_and_detached_future_refusals() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let sync: TickCallback = Rc::new(|| Err(ExchangeError::new("sync callback refusal")));
    TickHost::schedule_loop(&h.exchange.rt, sync, 7.0).unwrap();
    let runtime = h.runtime.clone();
    let asynchronous: TickCallback = Rc::new(move || {
        let sleep = runtime.sleep(11);
        Ok(Box::pin(async move {
            sleep.await?;
            Err(ExchangeError::new("held callback refusal"))
        }))
    });
    TickHost::schedule_loop(&h.exchange.rt, asynchronous, 7.0).unwrap();
    h.runtime.pump();
    h.runtime.advance(6.0);
    assert!(h.runtime.reports.borrow().is_empty());
    h.runtime.advance(1.0);
    assert_eq!(&*h.runtime.reports.borrow(), &["sync callback refusal"]);
    h.runtime.advance(10.0);
    assert_eq!(h.runtime.reports.borrow().len(), 1);
    h.runtime.advance(1.0);
    assert_eq!(
        &*h.runtime.reports.borrow(),
        &["sync callback refusal", "held callback refusal"]
    );
}
