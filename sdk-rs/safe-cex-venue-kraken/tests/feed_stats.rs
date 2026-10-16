use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_kraken::feed_stats;
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    sync::OnceLock,
};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/feed-stats-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["header"]["hashes"].as_object().unwrap().len(), 2);
        value
    })
}
fn units(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u16)
        .collect()
}
fn failure(pointer: &Cell<usize>) -> ExchangeError {
    let mut error = ExchangeError::new("feed stats failure");
    error.name = "FeedError".into();
    pointer.set((&*error.metadata as *const _) as usize);
    error
}
fn compare(family: &str) {
    let cases = oracle()[family].as_array().unwrap();
    assert_eq!(cases.len(), 12, "FEED independent cases");
    let mut reads = 0;
    let mut cleanups = 0;
    let mut failures = 0;
    for case in cases {
        let config = &case["config"];
        let feed = units(&config["feed"]);
        let pointer = Cell::new(0);
        let trace = RefCell::new(Vec::new());
        let cleanup_count = Cell::new(0);
        let own = config["own"].as_array().unwrap();
        let mut entries = Vec::new();
        let mut seen = Vec::new();
        for rows in [Some(own), config["proto"].as_array()]
            .into_iter()
            .flatten()
        {
            let mut sorted = rows.iter().collect::<Vec<_>>();
            sorted.sort_by_key(|row| {
                let key = units(&row["key"]);
                let number = String::from_utf16(&key)
                    .ok()
                    .and_then(|s| safe_cex_types::js_coercion::js_array_index(&s));
                number.map_or((1, 0), |index| (0, index))
            });
            for row in sorted {
                let key = units(&row["key"]);
                if !seen.contains(&key) {
                    seen.push(key);
                    entries.push(row)
                }
            }
        }
        let lengths = entries.into_iter().map(|row| {
            trace.borrow_mut().push(("read", units(&row["key"])));
            if row["fail"].as_bool() == Some(true) {
                Err(failure(&pointer))
            } else {
                Ok(row["len"].as_u64().map(|len| len as usize))
            }
        });
        let cleanup = |provided: &Vec<u16>| {
            assert!(std::ptr::eq(provided, &feed), "FEED native feed reference");
            cleanup_count.set(cleanup_count.get() + 1);
            trace.borrow_mut().push(("cleanup", provided.clone()));
            if config["cleanupFail"].as_bool() == Some(true) {
                Err(failure(&pointer))
            } else {
                Ok(())
            }
        };
        let result = if family == "Futures" {
            feed_stats::futures_clear_feed_stats_if_idle(&feed, lengths, cleanup)
        } else {
            feed_stats::spot_clear_feed_stats_if_idle(&feed, lengths, cleanup)
        };
        let expected = case["trace"].as_array().unwrap();
        let source_cleanup = expected.iter().filter(|e| e["kind"] == "cleanup").count();
        assert_eq!(
            cleanup_count.get(),
            source_cleanup,
            "FEED native cleanup count"
        );
        cleanups += cleanup_count.get();
        match result {
            Ok(()) => assert!(case["failure"].is_null(), "FEED native result"),
            Err(error) => {
                failures += 1;
                assert_eq!(error.name, case["failure"]["name"]);
                assert_eq!(error.message, case["failure"]["message"]);
                assert_eq!(
                    (&*error.metadata as *const _) as usize,
                    pointer.get(),
                    "FEED native original error identity"
                );
                assert_eq!(case["failure"]["identity"], true)
            }
        }
        let trace = trace.borrow();
        assert_eq!(
            trace.len(),
            expected.len(),
            "FEED native enumeration length"
        );
        for ((kind, key), value) in trace.iter().zip(expected) {
            assert_eq!(*kind, value["kind"], "FEED native enumeration order");
            assert_eq!(
                *key,
                units(&value[if *kind == "read" { "key" } else { "feed" }]),
                "FEED native UTF16 argument"
            );
            reads += usize::from(*kind == "read")
        }
    }
    assert_eq!(reads, 16, "FEED independent reads");
    assert_eq!(cleanups, 6, "FEED independent cleanups");
    assert_eq!(failures, 2, "FEED independent failures");
}
#[test]
fn futures_feed_stats_live_source() {
    compare("Futures")
}
#[test]
fn spot_feed_stats_live_source() {
    compare("Spot")
}
