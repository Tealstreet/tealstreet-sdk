use super::comparison;
use crate::native_store::DefaultStore;
use safe_cex_store::{Store, request_stats::record_request_outcome};
use safe_cex_types::RequestOutcome;
fn snapshot(store: &DefaultStore) -> serde_json::Value {
    serde_json::to_value(store.serialize().unwrap()).unwrap()
}
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
#[test]
fn frozen_typescript_classification_counts_and_reset() {
    let oracle: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../safe-cex-store/tests/request_oracle.json"
    )))
    .unwrap();
    let store = DefaultStore::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let _unsubscribe = store.subscribe(Arc::new(move |_, _| {
        observed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }));
    for step in oracle["steps"].as_array().unwrap() {
        record_request_outcome(
            Some(&store),
            step["status"].as_f64(),
            step["hadResponse"].as_bool().unwrap(),
            step["capture"].as_bool().unwrap(),
        );
        comparison::assert_json_eq(&snapshot(&store)["requestStats"], &step["counts"]);
        assert_eq!(
            calls.load(Ordering::Relaxed) as u64,
            step["notifications"].as_u64().unwrap()
        );
    }
    store.reset().unwrap();
    comparison::assert_json_eq(&snapshot(&store)["requestStats"], &oracle["afterReset"]);
    assert_eq!(calls.load(Ordering::Relaxed), 1);
    assert!(serde_json::from_str::<RequestOutcome>("\"client-ip=secret\"").is_err());
    record_request_outcome(None, Some(200.0), true, false);
}
