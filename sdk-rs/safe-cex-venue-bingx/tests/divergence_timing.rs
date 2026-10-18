//! Caller-visible prefix observations on the assembled BingX venue.
#[path = "support/binding_harness.rs"]
#[allow(dead_code)]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_types::ExchangeName;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};

#[test]
fn real_ohlcv_missing_market_emits_before_future_is_polled() {
    let h = support::harness(ExchangeName::Bingx, support::options(), |_| {});
    let errors = Rc::new(RefCell::new(Vec::new()));
    let capture = errors.clone();
    h.exchange.base().on(
        "error",
        Rc::new(move |_, error| {
            capture.borrow_mut().push(error.clone());
            Ok(())
        }),
    );
    let pending = h.exchange.fetch_ohlcv(
        serde_json::from_value(json!({
            "symbol":"MISSING", "interval":"1m", "limit":1
        }))
        .unwrap(),
    );
    assert_eq!(
        errors.borrow().len(),
        1,
        "actual source emits the missing-market error in the call prefix, before returning its promise"
    );
    assert!(h.http.requests.borrow().is_empty());
    assert!(h.drive(pending).unwrap().is_empty());
    assert_eq!(
        errors.borrow().len(),
        1,
        "polling cannot duplicate the eager error"
    );
    h.exchange.dispose().unwrap();
}

#[test]
fn real_historical_fills_logs_params_before_future_is_polled() {
    let h = support::harness(ExchangeName::Bingx, support::options(), |http| {
        http.ok("GET", "/openApi/swap/v2/trade/fillHistory", json!([]));
    });
    let logs = Rc::new(RefCell::new(Vec::new()));
    let capture = logs.clone();
    h.exchange.base().on(
        "log",
        Rc::new(move |_, message| {
            capture
                .borrow_mut()
                .push(String::from_utf16(message.as_str().unwrap()).unwrap());
            Ok(())
        }),
    );
    let pending = h.exchange.fetch_historical_fills(
        serde_json::from_value(json!({
            "symbol":"BTCUSDT", "limit":7
        }))
        .unwrap(),
    );
    assert_eq!(
        logs.borrow().len(),
        1,
        "source parameter logging is visible before its returned promise is awaited"
    );
    assert!(logs.borrow()[0].contains("fetchHistoricalFills params:"));
    assert!(h.drive(pending).unwrap().is_empty());
    assert_eq!(h.http.calls("/openApi/swap/v2/trade/fillHistory").len(), 1);
    assert_eq!(
        logs.borrow()
            .iter()
            .filter(|s| s.contains("fetchHistoricalFills params:"))
            .count(),
        1
    );
    h.exchange.dispose().unwrap();
}
