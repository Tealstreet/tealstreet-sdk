//! Observe the real constructed amendment member before typed input projection.
use super::*;
use safe_cex_exchange::js_runtime::value::from_json;
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
#[allow(dead_code)]
mod fixture {
    use crate as safe_cex_venue_grvt;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/binding_harness.rs"
    ));
}
#[test]
fn constructed_update_counts_all_own_raw_keys_before_validation() {
    let h = fixture::harness(
        safe_cex_types::ExchangeName::Grvt,
        fixture::options(),
        |_| {},
    );
    let messages = Rc::new(RefCell::new(Vec::new()));
    let capture = messages.clone();
    h.exchange.base().on(
        "log",
        Rc::new(move |_, value| {
            capture.borrow_mut().push(value.to_display());
            Ok(())
        }),
    );
    for (update, count) in [
        (json!({"amount":2}), 1),
        (json!({"amount":2,"extra":7}), 2),
        (json!({"amount":2,"price":3,"extra":null}), 3),
    ] {
        messages.borrow_mut().clear();
        let input = from_json(&json!({
            "order":{"id":"71","symbol":"BTC_USDT_PERP","type":"limit","side":"buy","status":"open",
                "price":50000,"amount":0.125,"filled":0,"remaining":0.125,"reduceOnly":false,"lastUpdate":1},
            "update":update
        }));
        let result = h
            .exchange
            .rt
            .0
            .this
            .call_method("updateOrder", vec![input])
            .unwrap();
        let runtime = h.exchange.rt.0.exchange_runtime();
        let failure = h
            .drive(Box::pin(async move {
                await_js(&runtime, result).await.map_err(err)
            }))
            .unwrap_err();
        assert_eq!(
            failure.message,
            "Cannot safely update GRVT order 71: missing time-in-force"
        );
        assert!(
            messages
                .borrow()
                .iter()
                .any(|s| s.contains(&format!("Updating order 71 (fieldCount={count})"))),
            "all raw keys must reach the source diagnostic before typed projection: {:?}",
            messages.borrow()
        );
        assert!(
            h.http.requests.borrow().is_empty(),
            "preflight failure never cancels or places"
        );
    }
    h.exchange.dispose().unwrap();
}
