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

#[test]
fn real_native_family_margin_error_runs_before_poll() {
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
    let pending = h.exchange.change_margin_type(true, None);
    assert_eq!(
        errors.borrow().len(),
        1,
        "source emits Symbol is required before returning its Promise"
    );
    assert_eq!(h.drive(pending).unwrap_err().message, "Symbol is required");
    assert_eq!(errors.borrow().len(), 1);
    assert!(h.http.requests.borrow().is_empty());
    h.exchange.dispose().unwrap();
}

#[test]
fn real_native_family_dropped_mode_waiter_keeps_held_continuation() {
    const MODE: &str = "/openApi/swap/v1/positionSide/dual";
    let h = support::harness(ExchangeName::Bingx, support::options(), |http| {
        http.ok("POST", MODE, json!({}));
        http.ok("GET", MODE, json!({"dualSidePosition":true}));
        http.ok("GET", "/openApi/swap/v2/user/positions", json!([]));
    });
    let gate = Rc::new(support::Release::default());
    *h.http.held_request.borrow_mut() = Some(("POST".into(), MODE.into(), gate.clone()));
    let pending = h.exchange.change_position_mode(true, None);
    drop(pending);
    h.runtime.pump();
    assert_eq!(
        h.http.calls(MODE).len(),
        1,
        "unpolled waiter drop must retain the source POST"
    );
    assert_eq!(h.http.calls("/openApi/swap/v2/user/positions").len(), 0);
    gate.set(true);
    h.runtime.pump();
    assert_eq!(
        h.http.calls(MODE).len(),
        2,
        "held POST continuation must refetch mode after waiter drop"
    );
    assert_eq!(h.http.calls("/openApi/swap/v2/user/positions").len(), 1);
    assert!(h.runtime.reports.borrow().is_empty());
    h.exchange.dispose().unwrap();
}
