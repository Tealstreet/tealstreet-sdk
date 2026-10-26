//! Field-identity supplement. Original ingress/runtime/archive artifacts remain unchanged.
use safe_cex_candle_cache::CandleCacheChunkRecord;
use serde_json::Value;
#[test]
fn decoded_cache_record_native_field_identity() {
    let raw = r#"{"candles":[{"timestamp":120000,"open":11.25,"high":19.5,"low":7.75,"close":13.125,"volume":23.5}],"lastAccessed":1700000000123}"#;
    let value: Value = serde_json::from_str(raw).unwrap();
    for (profile, record) in [
        (
            "raw",
            serde_json::from_str::<CandleCacheChunkRecord>(raw).unwrap(),
        ),
        (
            "owned",
            serde_json::from_value::<CandleCacheChunkRecord>(value.clone()).unwrap(),
        ),
        ("borrowed", serde::Deserialize::deserialize(&value).unwrap()),
    ] {
        assert_eq!(record.candles.len(), 1, "CACHE-CARDINALITY decoded candles");
        let c = &record.candles[0];
        for (field, actual, expected) in [
            ("timestamp", c.timestamp, 120000_f64),
            ("open", c.open, 11.25),
            ("high", c.high, 19.5),
            ("low", c.low, 7.75),
            ("close", c.close, 13.125),
            ("volume", c.volume, 23.5),
            ("lastAccessed", record.last_accessed, 1700000000123_f64),
        ] {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "cache decoded native identity {profile}/{field}"
            );
        }
    }
}
#[test]
fn kv_consumer_native_field_identity() {
    use safe_cex_candle_cache::{
        CacheRuntime, CandleCacheChunkAccessOptions, CandleCacheKeyValueStore,
        KeyValueCandleCacheBackend, LocalFuture,
    };
    use std::{
        rc::Rc,
        task::{Context, Poll, Waker},
    };
    struct Fixture;
    impl CandleCacheKeyValueStore for Fixture {
        fn get(&self, _: String) -> LocalFuture<'static, Result<Option<String>, String>> {
            Box::pin(async {
                Ok(Some(r#"{"candles":[{"timestamp":120000,"open":11.25,"high":19.5,"low":7.75,"close":13.125,"volume":23.5}],"lastAccessed":1700000000123}"#.into()))
            })
        }
        fn set(&self, _: String, _: String) -> LocalFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
        fn delete(&self, _: String) -> LocalFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
        fn list_keys(&self, _: String) -> LocalFuture<'static, Result<Vec<String>, String>> {
            Box::pin(async { Ok(vec![]) })
        }
    }
    let backend = KeyValueCandleCacheBackend::new(
        Rc::new(Fixture),
        None,
        CacheRuntime {
            now_ms: Rc::new(|| 0.),
            sleep: Rc::new(|_| Box::pin(async {})),
            spawn: Rc::new(drop),
        },
    );
    let mut future = backend
        .load_chunks(
            "identity".into(),
            vec![0],
            CandleCacheChunkAccessOptions {
                chunk_size: 100,
                interval_seconds: 60.,
            },
        )
        .unwrap();
    let Poll::Ready(Ok(chunks)) = future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    else {
        panic!("identity fixture must complete inline")
    };
    assert_eq!(chunks.len(), 1, "CACHE-CARDINALITY native chunk count");
    assert_eq!(chunks[&0].len(), 100, "CACHE-CARDINALITY sparse width");
    assert_eq!(
        chunks[&0].iter().flatten().count(),
        1,
        "CACHE-CARDINALITY occupied candles"
    );
    let c = chunks[&0][2].as_ref().unwrap();
    for (field, actual, expected) in [
        ("timestamp", c.timestamp, 120000_f64),
        ("open", c.open, 11.25),
        ("high", c.high, 19.5),
        ("low", c.low, 7.75),
        ("close", c.close, 13.125),
        ("volume", c.volume, 23.5),
    ] {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "KV consumer native field identity {field}"
        );
    }
}
