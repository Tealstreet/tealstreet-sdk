//! The concrete binding must expose typed market refusals with their original cause.
#[allow(dead_code)]
#[path = "support/venue_runtime.rs"]
mod support;
use safe_cex_exchange::exchange::Exchange;
use safe_cex_store::{Store, StoreUpdate};
use serde_json::json;
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};

fn drive<F: Future>(rt: &support::Runtime, f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    for _ in 0..64 {
        if let Poll::Ready(value) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
            return value;
        }
        rt.now.set(rt.now.get() + 1000.0);
        rt.pump();
    }
    panic!("scripted markets did not settle");
}

#[test]
fn typed_market_refusal_logs_cause_at_error_level_and_keeps_cached_fallback() {
    for empty_cache in [false, true] {
        let (venue, rt, store, wire) = support::fixture();
        if empty_cache {
            Store::update(
                store.as_ref(),
                StoreUpdate {
                    markets: Some(vec![]),
                    ..Default::default()
                },
                false,
            )
            .unwrap();
        }
        let cached = store.markets().to_vec();
        wire.bodies.lock().unwrap().insert(
            "/api/v1/futures/market/trading_pairs".into(),
            json!({"code":0,"data":[{"base":false,"symbol":"BTCUSDT","quote":"USDT","symbolStatus":"OPEN","isApiSupported":true}]}),
        );
        let markets = drive(&rt, venue.fetch_markets()).unwrap();
        assert_eq!(
            markets, cached,
            "refusal retains the existing typed fallback"
        );
        let logs = rt.console.borrow();
        assert!(
            logs.iter().any(|(severity, cause)| severity == "error"
                && cause.contains("Market base must be a string")),
            "typed market refusal must be visible at error level with its original cause: {logs:?}"
        );
        drop(logs);
        venue.dispose().unwrap();
    }
}
