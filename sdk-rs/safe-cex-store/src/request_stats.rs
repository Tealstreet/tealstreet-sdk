//! Closed store vocabulary: never accept venue error text as an outcome.
use crate::store::DefaultStore;
use safe_cex_types::RequestOutcome;

/// Host supplies the signing-capture predicate; Axios wiring remains pending.
pub fn record_request_outcome(
    store: Option<&mut DefaultStore>,
    status: Option<f64>,
    had_response: bool,
    signing_capture: bool,
) {
    if signing_capture {
        return;
    }
    let outcome = if !had_response {
        RequestOutcome::Network
    } else if status.is_none() {
        RequestOutcome::Other
    } else if matches!(status, Some(429.0 | 418.0)) {
        RequestOutcome::RateLimit
    } else if matches!(status, Some(401.0 | 403.0)) {
        RequestOutcome::Auth
    } else if status.is_some_and(|s| (200.0..300.0).contains(&s)) {
        RequestOutcome::Ok
    } else {
        RequestOutcome::Other
    };
    if let Some(store) = store {
        store.record_request_outcome(outcome);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    #[test]
    fn frozen_typescript_classification_counts_and_reset() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("../tests/request_oracle.json")).unwrap();
        let mut store = DefaultStore::new();
        let calls = Arc::new(AtomicUsize::new(0));
        let observed = calls.clone();
        let _unsubscribe = store.subscribe(Arc::new(move |_, _| {
            observed.fetch_add(1, Ordering::Relaxed);
            Ok(())
        }));
        for step in oracle["steps"].as_array().unwrap() {
            record_request_outcome(
                Some(&mut store),
                step["status"].as_f64(),
                step["hadResponse"].as_bool().unwrap(),
                step["capture"].as_bool().unwrap(),
            );
            crate::test_support::assert_json_eq(
                &store.serialize()["requestStats"],
                &step["counts"],
            );
            assert_eq!(
                calls.load(Ordering::Relaxed) as u64,
                step["notifications"].as_u64().unwrap()
            );
        }
        store.reset().unwrap();
        crate::test_support::assert_json_eq(
            &store.serialize()["requestStats"],
            &oracle["afterReset"],
        );
        assert_eq!(calls.load(Ordering::Relaxed), 1);
        assert!(serde_json::from_str::<RequestOutcome>("\"client-ip=secret\"").is_err());
        record_request_outcome(None, Some(200.0), true, false);
    }
}
