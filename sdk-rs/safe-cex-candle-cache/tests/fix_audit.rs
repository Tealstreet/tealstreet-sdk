#![cfg(not(target_arch = "wasm32"))]
use safe_cex_candle_cache::{
    CandleCache, CandleCacheBackendReadMode, CandleCacheChunkAccessOptions,
    CandleCacheFetchRequest, CandleCacheKeyValueStore, CandleCacheOptions, Chunks, Fetcher,
    KeyValueCandleCacheBackend, LocalFuture, calendar, native::local_runtime,
};
use safe_cex_types::{AnyTimeframe, Candle};
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
#[derive(Default)]
struct Terminal {
    values: RefCell<BTreeMap<String, String>>,
    reads: RefCell<Vec<String>>,
    listed: RefCell<Option<Vec<String>>>,
}
impl CandleCacheKeyValueStore for Terminal {
    fn get(&self, key: String) -> LocalFuture<'static, Result<Option<String>, String>> {
        self.reads.borrow_mut().push(key.clone());
        let value = self.values.borrow().get(&key).cloned();
        Box::pin(async { Ok(value) })
    }
    fn set(&self, key: String, value: String) -> LocalFuture<'static, Result<(), String>> {
        self.values.borrow_mut().insert(key, value);
        Box::pin(async { Ok(()) })
    }
    fn delete(&self, key: String) -> LocalFuture<'static, Result<(), String>> {
        self.values.borrow_mut().remove(&key);
        Box::pin(async { Ok(()) })
    }
    fn list_keys(&self, prefix: String) -> LocalFuture<'static, Result<Vec<String>, String>> {
        let keys = self.listed.borrow().clone().unwrap_or_else(|| {
            self.values
                .borrow()
                .keys()
                .filter(|k| k.starts_with(&prefix))
                .cloned()
                .collect()
        });
        Box::pin(async { Ok(keys) })
    }
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn row(timestamp: f64) -> Candle {
    Candle {
        timestamp,
        open: 11.,
        high: 17.,
        low: 7.,
        close: 13.,
        volume: 19.,
        last_side: None,
    }
}
fn fields(actual: &Candle, timestamp: f64) {
    for (a, b) in [
        (actual.timestamp, timestamp),
        (actual.open, 11.),
        (actual.high, 17.),
        (actual.low, 7.),
        (actual.close, 13.),
        (actual.volume, 19.),
    ] {
        assert_eq!(
            a.to_bits(),
            b.to_bits(),
            "CACHE-AUDIT native field before JSON"
        );
    }
}
// JS numbers have one binary64 representation despite Serde lexical number kinds.
fn observation(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "CACHE-AUDIT binary64 observation"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len());
            for (a, b) in a.iter().zip(b) {
                observation(a, b);
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.keys().collect::<Vec<_>>(), b.keys().collect::<Vec<_>>());
            for (k, v) in a {
                observation(v, &b[k]);
            }
        }
        _ => assert_eq!(actual, expected),
    }
}
#[test]
fn all_calendar_adapters_and_both_real_kv_cache_paths_match_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fix_audit_source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CACHE-AUDIT source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        54,
        "CACHE-AUDIT independent 27 intervals x 2 modes"
    );
    runtime().block_on(tokio::task::LocalSet::new().run_until(async {
        for capture in rows {
            let interval:AnyTimeframe=serde_json::from_value(capture["interval"].clone()).unwrap();
            let start=calendar::start(&interval,1706745600.);
            let next=calendar::next(&interval,start);
            let third=calendar::next(&interval,next);
            let now=calendar::next(&interval,third);
            let to=calendar::next(&interval,now);
            assert_eq!(calendar::count(&interval,start,third).to_bits(),3f64.to_bits());
            observation(&json!([calendar::calendar(&interval),start,next,calendar::close(&interval,start),calendar::count(&interval,start,third)]),&capture["boundaries"]);
            let mode=if capture["mode"]=="read-through" {CandleCacheBackendReadMode::ReadThrough} else {CandleCacheBackendReadMode::HydrateOnce};
            let store=Rc::new(Terminal::default());
            let calls=Rc::new(RefCell::new(Vec::new()));
            let seen=calls.clone();
            let fetcher:Fetcher=Rc::new(move |request| {
                seen.borrow_mut().push((request.from,request.to,request.limit));
                Box::pin(async move {Ok([start,next,third].map(|t|row(t*1000.)).to_vec())})
            });
            let make=||CandleCache::new(CandleCacheOptions {
                backend:Some(Rc::new(KeyValueCandleCacheBackend::new(store.clone(),None,local_runtime()))),
                backend_read_mode:Some(mode),..Default::default()
            },local_runtime());
            for cache in [make(),make()] {
                let result=cache.fetch(CandleCacheFetchRequest {
                    cache_key:"audit".into(),interval:interval.clone(),from_ms:start*1000.,to_ms:third*1000.,limit:3.,now_ms:Some(now*1000.),fetcher:fetcher.clone(),use_persistent_backend:None,
                }).await.unwrap();
                assert_eq!(result.len(),3,"CACHE-AUDIT real cache row count");
                for (c,t) in result.iter().zip([start,next,third]) {fields(c,t*1000.);}
                observation(&json!(result.iter().map(|r|r.timestamp).collect::<Vec<_>>()),&capture["timestamps"]);
                let lookup=cache.get_cached_candles("audit",&interval,start*1000.,to*1000.,5.,Some(now*1000.));
                observation(&json!({"behind":lookup.contiguous_behind_candles.iter().map(|r|r.timestamp).collect::<Vec<_>>(),"forward":lookup.contiguous_forward_candles.iter().map(|r|r.timestamp).collect::<Vec<_>>(),"from":lookup.from,"to":lookup.to,"limit":lookup.limit}),&capture["lookup"]);
                tokio::task::yield_now().await; // flush detached writes, without changing wall time
            }
            assert_eq!(*calls.borrow(),[(start*1000.,third*1000.,3.)],"CACHE-AUDIT reopened cache must make no venue request");
            assert!(!store.reads.borrow().is_empty());
            assert!(!store.values.borrow().is_empty());
        }
    }));
    for alias in ["1W", "W", "M", "bad"] {
        assert!(serde_json::from_value::<AnyTimeframe>(json!(alias)).is_err());
    }
}
#[test]
fn listed_extreme_keys_report_refusals_without_losing_healthy_chunks() {
    runtime().block_on(tokio::task::LocalSet::new().run_until(async {
        let store = Rc::new(Terminal::default());
        let prefix = "tealstreet-candles::audit::";
        let listed = [
            "0",
            "\u{feff}0tail",
            "9007199254740993",
            "9223372036854775808",
            "junk",
        ]
        .map(|s| format!("{prefix}{s}"));
        *store.listed.borrow_mut() = Some(
            listed
                .into_iter()
                .chain(["other::0000000000000000000012".into()])
                .collect(),
        );
        store.values.borrow_mut().insert(
            format!("{prefix}0"),
            json!({"candles":[row(0.)],"lastAccessed":0}).to_string(),
        );
        let backend = KeyValueCandleCacheBackend::new(store.clone(), None, local_runtime());
        let options = CandleCacheChunkAccessOptions {
            chunk_size: 1,
            interval_seconds: 1.,
        };
        let loaded = backend
            .load_all_chunks("audit".into(), options)
            .unwrap()
            .await
            .unwrap();
        assert_eq!(
            loaded.refused_keys,
            [format!("{prefix}9223372036854775808")]
        );
        fields(loaded.chunks[&0][0].as_ref().unwrap(), 0.);
        assert_eq!(
            *store.reads.borrow(),
            [
                format!("{prefix}0"),
                format!("{prefix}0"),
                format!("{prefix}9007199254740993")
            ]
        );
        store.reads.borrow_mut().clear();
        for size in [0, 16_385] {
            let result = backend
                .load_all_chunks(
                    "audit".into(),
                    CandleCacheChunkAccessOptions {
                        chunk_size: size,
                        ..options
                    },
                )
                .unwrap()
                .await;
            if size == 0 {
                assert!(result.unwrap().chunks.is_empty());
            } else {
                assert!(result.is_err());
            }
            assert!(
                store.reads.borrow().is_empty(),
                "CACHE-AUDIT allocation refusal precedes GET"
            );
        }
    }));
}
#[test]
fn pre_epoch_typed_slots_survive_real_kv_persist_and_reload() {
    runtime().block_on(tokio::task::LocalSet::new().run_until(async {
        let store = Rc::new(Terminal::default());
        let make = || KeyValueCandleCacheBackend::new(store.clone(), None, local_runtime());
        let options = CandleCacheChunkAccessOptions {
            chunk_size: 100,
            interval_seconds: 60.,
        };
        let mut slots = vec![None; 100];
        slots[99] = Some(row(-60000.));
        make()
            .persist_chunks("negative".into(), Chunks::from([(-1, slots)]), options)
            .await
            .unwrap();
        let loaded = make()
            .load_chunks("negative".into(), vec![-1], options)
            .unwrap()
            .await
            .unwrap();
        fields(loaded[&-1][99].as_ref().unwrap(), -60000.);
    }));
}

#[test]
fn clean_candles_and_real_fetch_keep_source_nan_permutations_and_first_rows() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fix_audit_source.cjs"
        ))
        .arg("--clean-candles")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLEAN-CANDLES source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let profiles: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        profiles.len(),
        133,
        "CLEAN-CANDLES independent source floor"
    );
    fn decode(rows: &Value) -> Vec<Candle> {
        rows.as_array()
            .unwrap()
            .iter()
            .map(|row| {
                let values: Vec<_> = row
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap()))
                    .collect();
                assert_eq!(values.len(), 6);
                Candle {
                    timestamp: values[0],
                    open: values[1],
                    high: values[2],
                    low: values[3],
                    close: values[4],
                    volume: values[5],
                    last_side: None,
                }
            })
            .collect()
    }
    fn observed(rows: &[Candle]) -> Vec<[u64; 6]> {
        rows.iter()
            .map(|r| [r.timestamp, r.open, r.high, r.low, r.close, r.volume].map(f64::to_bits))
            .collect()
    }
    runtime().block_on(tokio::task::LocalSet::new().run_until(async {
        for profile in profiles {
            let input = decode(&profile["input"]);
            let expected = decode(&profile["expected"]);
            let cleaned = safe_cex_candle_cache::clean_candles(input.clone());
            let fetched = Rc::new(input);
            let fetcher: Fetcher = Rc::new(move |_| {
                let input = fetched.clone();
                Box::pin(async move { Ok(input.as_ref().clone()) })
            });
            let cache = CandleCache::new(CandleCacheOptions::default(), local_runtime());
            let result = cache
                .fetch(CandleCacheFetchRequest {
                    cache_key: "sort".into(),
                    interval: serde_json::from_value(json!("1m")).unwrap(),
                    from_ms: 1699999980000.,
                    to_ms: 1699999980000.,
                    limit: 1.,
                    now_ms: Some(1700000040000.),
                    fetcher,
                    use_persistent_backend: Some(false),
                })
                .await
                .unwrap();
            assert_eq!(
                observed(&result),
                observed(&expected),
                "CLEAN-CANDLES real fetch source permutation {}",
                profile["id"]
            );
            assert_eq!(
                observed(&cleaned),
                observed(&expected),
                "CLEAN-CANDLES native source permutation {}",
                profile["id"]
            );
        }
    }));
}
