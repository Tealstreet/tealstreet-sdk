// Deliberately do not import CandleCacheBackend: TS class methods are directly callable.
use safe_cex_candle_cache::{
    CacheRuntime, CandleCacheChunkAccessOptions, CandleCacheKeyValueStore, Chunks,
    KeyValueCandleCacheBackend, LocalFuture, MemoryCandleCacheBackend,
};
use std::rc::Rc;

struct Store;
impl CandleCacheKeyValueStore for Store {
    fn get(&self, _: String) -> LocalFuture<'static, Result<Option<String>, String>> {
        Box::pin(async { Ok(None) })
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

#[test]
fn concrete_backend_methods_are_public_without_trait_import() {
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    let memory = MemoryCandleCacheBackend::default();
    assert!(memory.load_all_chunks("k".into(), options).is_some());
    assert!(memory.load_chunks("k".into(), vec![], options).is_some());
    drop(memory.persist_chunks("k".into(), Chunks::new(), options));
    assert!(memory.clear(None).is_some());
    let kv = KeyValueCandleCacheBackend::new(
        Rc::new(Store),
        None,
        CacheRuntime {
            now_ms: Rc::new(|| 0.),
            sleep: Rc::new(|_| Box::pin(async {})),
            spawn: Rc::new(drop),
        },
    );
    assert!(kv.load_all_chunks("k".into(), options).is_some());
    assert!(kv.load_chunks("k".into(), vec![], options).is_some());
    drop(kv.persist_chunks("k".into(), Chunks::new(), options));
    assert!(kv.clear(None).is_some());
}

#[test]
fn public_store_keys_reach_integer_limit_independently_of_chunk_width() {
    use std::{
        cell::RefCell,
        task::{Context, Poll, Waker},
    };
    struct ListedStore(Rc<RefCell<Vec<String>>>, Vec<String>);
    impl CandleCacheKeyValueStore for ListedStore {
        fn get(&self, key: String) -> LocalFuture<'static, Result<Option<String>, String>> {
            self.0.borrow_mut().push(key);
            Box::pin(async { Ok(None) })
        }
        fn set(&self, _: String, _: String) -> LocalFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
        fn delete(&self, _: String) -> LocalFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
        fn list_keys(&self, prefix: String) -> LocalFuture<'static, Result<Vec<String>, String>> {
            let keys = self.1.clone();
            Box::pin(async move {
                Ok(keys
                    .into_iter()
                    .map(|key| {
                        if key.starts_with("other::") {
                            key
                        } else {
                            format!("{prefix}{key}")
                        }
                    })
                    .collect())
            })
        }
    }
    let nonfinite = "9".repeat(400);
    for rejected in [
        None,
        Some("9223372036854775808"),
        Some("-9223372036854775809"),
        Some("not-a-number"),
        Some("\u{0085}12"),
        Some("other::9223372036854775808"),
        Some("+"),
        Some(nonfinite.as_str()),
    ] {
        let reads = Rc::new(RefCell::new(Vec::new()));
        let mut keys = vec![
            "9007199254740993".to_owned(),
            "9223372036854775807".to_owned(),
        ];
        if let Some(key) = rejected {
            keys.push(key.to_owned());
        }
        let backend = KeyValueCandleCacheBackend::new(
            Rc::new(ListedStore(reads.clone(), keys)),
            None,
            CacheRuntime {
                now_ms: Rc::new(|| 0.),
                sleep: Rc::new(|_| Box::pin(async {})),
                spawn: Rc::new(drop),
            },
        );
        let mut future = backend
            .load_all_chunks(
                "k".into(),
                CandleCacheChunkAccessOptions {
                    chunk_size: 1,
                    interval_seconds: 1.,
                },
            )
            .unwrap();
        let mut context = Context::from_waker(Waker::noop());
        let outcome = future.as_mut().poll(&mut context);
        let Poll::Ready(Ok(loaded)) = outcome else {
            panic!("listing must succeed");
        };
        let expected_refusals: Vec<_> = rejected
            .filter(|s| s.starts_with("922") || s.starts_with("-922"))
            .map(|s| format!("tealstreet-candles::k::{s}"))
            .into_iter()
            .collect();
        assert_eq!(loaded.refused_keys, expected_refusals);
        assert!(loaded.chunks.is_empty());
        assert_eq!(
            *reads.borrow(),
            [
                "tealstreet-candles::k::9007199254740993",
                "tealstreet-candles::k::9223372036854775807"
            ]
        );
    }
}
