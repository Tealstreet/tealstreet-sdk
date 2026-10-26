use super::*;
use safe_cex_types::{AnyTimeframe, Candle, ExchangeName};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::{Future, poll_fn},
    rc::Rc,
    task::{Context, Poll, Waker},
};

#[derive(Clone, Default)]
struct Executor {
    clock: Rc<Cell<f64>>,
    tasks: Rc<RefCell<Vec<LocalFuture<'static, ()>>>>,
}
impl Executor {
    fn runtime(&self) -> CacheRuntime {
        let clock = self.clock.clone();
        let timer = clock.clone();
        let tasks = self.tasks.clone();
        CacheRuntime {
            now_ms: Rc::new(move || clock.get()),
            sleep: Rc::new(move |ms| {
                let clock = timer.clone();
                let deadline = clock.get() + ms;
                Box::pin(poll_fn(move |_| {
                    if clock.get() >= deadline {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                }))
            }),
            spawn: Rc::new(move |task| tasks.borrow_mut().push(task)),
        }
    }
    fn tick(&self) {
        let mut tasks = std::mem::take(&mut *self.tasks.borrow_mut());
        for mut task in tasks.drain(..) {
            if task
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
            {
                self.tasks.borrow_mut().push(task);
            }
        }
    }
    fn run<T>(&self, future: impl Future<Output = T>) -> T {
        let mut future = std::pin::pin!(future);
        for _ in 0..100 {
            if let Poll::Ready(value) = future
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
            {
                self.tick();
                return value;
            }
            self.tick();
        }
        panic!("future did not finish");
    }
}
fn candle(timestamp: f64) -> Candle {
    Candle {
        timestamp,
        open: 100.,
        high: 110.,
        low: 90.,
        close: 105.,
        volume: 1000.,
        last_side: None,
    }
}
fn request(
    key: &str,
    interval: &str,
    from: f64,
    to: f64,
    limit: f64,
    now: f64,
    fetcher: Fetcher,
) -> CandleCacheFetchRequest {
    CandleCacheFetchRequest {
        cache_key: key.into(),
        interval: timeframe(interval),
        from_ms: from,
        to_ms: to,
        limit,
        now_ms: Some(now),
        use_persistent_backend: None,
        fetcher,
    }
}
fn recording(candles: Vec<Candle>) -> (Fetcher, Rc<RefCell<Vec<FetchOptions>>>) {
    let calls = Rc::new(RefCell::new(Vec::new()));
    let recorded = calls.clone();
    (
        Rc::new(move |options| {
            recorded.borrow_mut().push(options);
            let candles = candles.clone();
            Box::pin(async move { Ok(candles) })
        }),
        calls,
    )
}
fn base() -> f64 {
    1_699_999_800_000.
}
#[test]
fn keys_cleaning_and_reset() {
    let a = candle(1.);
    let mut duplicate = a.clone();
    duplicate.close = 999.;
    assert_eq!(
        clean_candles(vec![candle(2.), a.clone(), duplicate]),
        vec![a, candle(2.)]
    );
    for venue in [
        "binance",
        "binancepm",
        "hyperliquid",
        "grvt",
        "paradex",
        "asterdex",
    ] {
        assert_eq!(
            get_candle_cache_key(&exchange_name(venue), "BTCUSDT", &timeframe("1m"), None),
            format!("{venue}-BTCUSDT-1m")
        );
        assert_eq!(
            get_candle_cache_key(
                &exchange_name(venue),
                "BTCUSDT",
                &timeframe("1m"),
                Some(false)
            ),
            get_candle_cache_key(&exchange_name(venue), "BTCUSDT", &timeframe("1m"), None)
        );
        assert_eq!(
            get_candle_cache_key(
                &exchange_name(venue),
                "BTCUSDT",
                &timeframe("1m"),
                Some(true)
            ),
            format!("{venue}-testnet-BTCUSDT-1m")
        );
    }
    assert_eq!(
        get_candle_cache_key(&ExchangeName::Bybit, "BTCUSDT", &timeframe("1M"), None),
        "bybit-BTCUSDT-1M-cal"
    );
    assert_eq!(
        get_candle_cache_key(
            &ExchangeName::Bybit,
            "BTCUSDT",
            &timeframe("1w"),
            Some(true)
        ),
        "bybit-testnet-BTCUSDT-1w-cal"
    );
    let e = Executor::default();
    let cache = CandleCache::new(Default::default(), e.runtime());
    cache.reset(None, None);
    assert_eq!(cache.get_version(), 1);
}
#[test]
fn reuse_missing_middle_and_future_clamp() {
    let e = Executor::default();
    let cache = CandleCache::new(Default::default(), e.runtime());
    let b = base();
    let step = 300000.;
    let (fetch, calls) = recording((0..5).map(|i| candle(b + i as f64 * step)).collect());
    assert_eq!(
        e.run(cache.fetch(request(
            "k",
            "5m",
            b,
            b + 4. * step,
            10.,
            b + 20. * step,
            fetch
        )))
        .unwrap()
        .len(),
        5
    );
    assert_eq!(calls.borrow().len(), 1);
    let (fetch, calls) = recording((5..8).map(|i| candle(b + i as f64 * step)).collect());
    assert_eq!(
        e.run(cache.fetch(request(
            "k",
            "5m",
            b + 2. * step,
            b + 7. * step,
            10.,
            b + 20. * step,
            fetch
        )))
        .unwrap()
        .len(),
        6
    );
    assert_eq!(
        *calls.borrow(),
        vec![FetchOptions {
            interval: timeframe("5m"),
            from: b + 5. * step,
            to: b + 7. * step,
            limit: 7.
        }]
    );
    let (fetch, calls) = recording(Vec::new());
    e.run(cache.fetch(request(
        "other",
        "5m",
        b + 9. * step,
        b + 11. * step,
        10.,
        b + 10. * step,
        fetch.clone(),
    )))
    .unwrap();
    assert_eq!(calls.borrow()[0].to, b + 10. * step);
    assert_eq!(calls.borrow()[0].limit, 9.);
    e.run(cache.fetch(request(
        "other",
        "5m",
        b + 11. * step,
        b + 100. * step,
        10.,
        b + 10. * step,
        fetch,
    )))
    .unwrap();
    assert_eq!(calls.borrow().len(), 1);
    let (fetch, calls) = recording(vec![candle(b), candle(b + 2. * step)]);
    e.run(cache.fetch(request(
        "sparse",
        "5m",
        b,
        b + 2. * step,
        3.,
        b + 10. * step,
        fetch,
    )))
    .unwrap();
    let (fetch, gap) = recording(vec![candle(b + step)]);
    let result = e
        .run(cache.fetch(request(
            "sparse",
            "5m",
            b,
            b + 2. * step,
            3.,
            b + 10. * step,
            fetch,
        )))
        .unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(gap.borrow()[0].from, b + step);
    assert_eq!(gap.borrow()[0].to, b + step);
    assert_eq!(gap.borrow()[0].limit, 1.);
    assert_eq!(calls.borrow().len(), 1);
}
#[derive(Default)]
struct RecordingBackend {
    memory: MemoryCandleCacheBackend,
    all: Cell<usize>,
    reads: RefCell<Vec<Vec<i64>>>,
    fail: Cell<bool>,
    pending: Rc<RefCell<Option<Chunks>>>,
    hang_first: Cell<bool>,
}
impl CandleCacheBackend for RecordingBackend {
    fn load_all_chunks(
        &self,
        key: String,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        self.all.set(self.all.get() + 1);
        if self.fail.get() {
            return Some(Box::pin(async { Err("read failed".into()) }));
        }
        if self.hang_first.replace(false) {
            let pending = self.pending.clone();
            return Some(Box::pin(poll_fn(move |_| {
                pending.borrow_mut().take().map_or(Poll::Pending, |v| {
                    Poll::Ready(Ok(LoadedChunks {
                        chunks: v,
                        refused_keys: Vec::new(),
                    }))
                })
            })));
        }
        self.memory.load_all_chunks(key, options)
    }
    fn load_chunks(
        &self,
        key: String,
        keys: Vec<i64>,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        self.reads.borrow_mut().push(keys.clone());
        if self.fail.get() {
            Some(Box::pin(async { Err("read failed".into()) }))
        } else {
            self.memory.load_chunks(key, keys, options)
        }
    }
    fn persist_chunks(
        &self,
        key: String,
        chunks: Chunks,
        options: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        self.memory.persist_chunks(key, chunks, options)
    }
    fn clear(&self, key: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        self.memory.clear(key)
    }
}
#[test]
fn shared_read_through_and_hydrate_once() {
    let e = Executor::default();
    let backend = Rc::new(RecordingBackend::default());
    let make = |mode| {
        CandleCache::new(
            CandleCacheOptions {
                backend: Some(backend.clone()),
                backend_read_mode: Some(mode),
                ..Default::default()
            },
            e.runtime(),
        )
    };
    let a = make(CandleCacheBackendReadMode::ReadThrough);
    let b = make(CandleCacheBackendReadMode::ReadThrough);
    let t = base();
    let (fetch, _) = recording(vec![candle(t)]);
    e.run(a.fetch(request("k", "5m", t, t, 1., t + 300000., fetch)))
        .unwrap();
    let (empty, calls) = recording(Vec::new());
    assert_eq!(
        e.run(b.fetch(request("k", "5m", t, t, 1., t + 300000., empty.clone())))
            .unwrap(),
        vec![candle(t)]
    );
    e.run(b.fetch(request("k", "5m", t, t, 1., t + 300000., empty.clone())))
        .unwrap();
    assert!(calls.borrow().is_empty());
    assert_eq!(backend.reads.borrow().len(), 3);
    let once = make(CandleCacheBackendReadMode::HydrateOnce);
    for _ in 0..2 {
        e.run(once.fetch(request("k", "5m", t, t, 1., t + 300000., empty.clone())))
            .unwrap();
    }
    assert_eq!(backend.all.get(), 1);
    once.reset(Some(true), Some("k".into()));
    e.tick();
    assert!(
        e.run(
            backend
                .memory
                .load_all_chunks(
                    "k".into(),
                    CandleCacheChunkAccessOptions {
                        chunk_size: 100,
                        interval_seconds: 300.
                    }
                )
                .unwrap()
        )
        .unwrap()
        .chunks
        .is_empty()
    );
    let mut req = request("k", "5m", t, t, 1., t + 300000., empty);
    req.use_persistent_backend = Some(false);
    e.run(once.fetch(req)).unwrap();
    assert_eq!(backend.all.get(), 1);
}
#[test]
fn rejected_hydrations_release_slots_and_read_through_errors_propagate() {
    let e = Executor::default();
    let backend = Rc::new(RecordingBackend::default());
    backend.fail.set(true);
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            max_concurrent_hydrations: Some(1),
            ..Default::default()
        },
        e.runtime(),
    );
    let (fetch, _) = recording(Vec::new());
    for key in ["a", "b"] {
        e.run(cache.fetch(request(
            key,
            "5m",
            base(),
            base(),
            1.,
            base() + 300000.,
            fetch.clone(),
        )))
        .unwrap();
    }
    assert_eq!(backend.all.get(), 2);
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend),
            backend_read_mode: Some(CandleCacheBackendReadMode::ReadThrough),
            ..Default::default()
        },
        e.runtime(),
    );
    assert_eq!(
        e.run(cache.fetch(request(
            "a",
            "5m",
            base(),
            base(),
            1.,
            base() + 300000.,
            fetch
        )))
        .unwrap_err(),
        "read failed"
    );
}
#[test]
fn timeout_releases_queue_and_late_hydration_preserves_fresh_memory() {
    let e = Executor::default();
    let backend = Rc::new(RecordingBackend::default());
    backend.hang_first.set(true);
    let timeouts = Rc::new(RefCell::new(Vec::new()));
    let observed = timeouts.clone();
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            max_concurrent_hydrations: Some(1),
            hydration_timeout_ms: Some(10.),
            on_hydration_timeout: Some(Rc::new(move |k| observed.borrow_mut().push(k))),
            ..Default::default()
        },
        e.runtime(),
    );
    let t = base();
    let mut fresh = candle(t);
    fresh.close = 109.;
    let (fetch, calls) = recording(vec![fresh.clone()]);
    let mut first = Box::pin(cache.fetch(request("a", "5m", t, t, 1., t + 300000., fetch.clone())));
    let (empty, _) = recording(Vec::new());
    let mut second = Box::pin(cache.fetch(request("b", "5m", t, t, 1., t + 300000., empty)));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(second.as_mut().poll(&mut cx).is_pending());
    e.tick();
    assert_eq!(backend.all.get(), 1);
    e.clock.set(11.);
    e.tick();
    e.tick();
    assert_eq!(e.run(first).unwrap(), vec![fresh.clone()]);
    assert!(e.run(second).unwrap().is_empty());
    assert_eq!(backend.all.get(), 2);
    assert_eq!(*timeouts.borrow(), vec!["a"]);
    let index = (t / 1000. / 300.).floor() as i64;
    let mut chunk = vec![None; 100];
    let mut stale = candle(t);
    stale.close = 101.;
    chunk[index.rem_euclid(100) as usize] = Some(stale);
    chunk[(index + 1).rem_euclid(100) as usize] = Some(candle(t + 300000.));
    *backend.pending.borrow_mut() = Some(BTreeMap::from([(index.div_euclid(100), chunk)]));
    e.tick();
    assert_eq!(
        e.run(cache.fetch(request("a", "5m", t, t, 1., t + 600000., fetch)))
            .unwrap(),
        vec![fresh]
    );
    assert_eq!(calls.borrow().len(), 1);
    assert_eq!(
        cache
            .get_cached_candles("a", &timeframe("5m"), t, t + 300000., 2., Some(t + 600000.))
            .contiguous_behind_candles
            .len(),
        2
    );
}
#[test]
fn calendar_months_weeks_and_off_grid_candles() {
    assert_eq!(
        crate::calendar::close(&timeframe("1M"), 1_265_040_754.),
        1_267_459_954.,
        "monthly close retains 16:12:34 UTC"
    );
    let e = Executor::default();
    let backend = Rc::new(MemoryCandleCacheBackend::default());
    let make = || {
        CandleCache::new(
            CandleCacheOptions {
                backend: Some(backend.clone()),
                ..Default::default()
            },
            e.runtime(),
        )
    };
    let cache = make();
    let mut t = 1704067200.;
    let mut starts = Vec::new();
    for _ in 0..16 {
        starts.push(t * 1000.);
        t = crate::calendar::next(&timeframe("1M"), t);
    }
    assert_eq!(starts[1] - starts[0], 31. * 86400000.);
    assert_eq!(starts[2] - starts[1], 29. * 86400000.);
    assert_eq!(starts[14] - starts[13], 28. * 86400000.);
    let now = 1744718400000.;
    let (fetch, calls) = recording(starts.iter().copied().map(candle).collect());
    assert_eq!(
        e.run(cache.fetch(request(
            "month",
            "1M",
            starts[0] + 9. * 86400000.,
            1750377600000.,
            18.,
            now,
            fetch
        )))
        .unwrap()
        .len(),
        16
    );
    assert_eq!(
        calls.borrow()[0],
        FetchOptions {
            interval: timeframe("1M"),
            from: starts[0],
            to: starts[15],
            limit: 16.
        }
    );
    let (fetch, calls) = recording(vec![candle(starts[15])]);
    let fresh = make();
    assert_eq!(
        e.run(fresh.fetch(request("month", "1M", starts[0], now, 16., now, fetch)))
            .unwrap()
            .len(),
        16
    );
    assert_eq!(calls.borrow()[0].from, starts[15]);
    assert_eq!(calls.borrow()[0].limit, 1.);
    for now in [1791633600000., 1791374400000.] {
        let cache = make();
        let mondays = [
            1789344000000.,
            1789948800000.,
            1790553600000.,
            1791158400000.,
        ];
        let (fetch, _) = recording(mondays.into_iter().map(candle).collect());
        e.run(cache.fetch(request("week", "1w", mondays[0], now, 4., now, fetch)))
            .unwrap();
        let (fetch, calls) = recording(vec![candle(mondays[3])]);
        e.run(cache.fetch(request("week", "1w", mondays[0], now, 4., now, fetch)))
            .unwrap();
        assert_eq!(calls.borrow()[0].from, mondays[3]);
        assert_eq!(calls.borrow()[0].limit, 1.);
        e.run(backend.clear(None).unwrap()).unwrap();
    }
    let cache = CandleCache::new(Default::default(), e.runtime());
    let now = 1791626400000.;
    let closed = 1791475200000.;
    let forming = 1791561600000.;
    let (fetch, _) = recording(vec![candle(closed), candle(forming)]);
    e.run(cache.fetch(request("off", "1d", 1791417600000., now, 10., now, fetch)))
        .unwrap();
    assert_eq!(
        cache
            .get_cached_candles("off", &timeframe("1d"), 1791417600000., now, 10., Some(now))
            .contiguous_behind_candles,
        vec![candle(closed)]
    );
}
#[derive(Default)]
struct Store {
    reads: Cell<usize>,
    lists: Cell<usize>,
    values: Rc<RefCell<BTreeMap<String, String>>>,
}
impl CandleCacheKeyValueStore for Store {
    fn get(&self, key: String) -> LocalFuture<'static, Result<Option<String>, String>> {
        self.reads.set(self.reads.get() + 1);
        let v = self.values.borrow().get(&key).cloned();
        Box::pin(async move { Ok(v) })
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
        self.lists.set(self.lists.get() + 1);
        let keys = self
            .values
            .borrow()
            .keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        Box::pin(async move { Ok(keys) })
    }
}
#[test]
fn key_value_wrong_chunks_corruption_compaction_and_clear() {
    let e = Executor::default();
    let store = Rc::new(Store::default());
    let backend = KeyValueCandleCacheBackend::new(store.clone(), None, {
        let runtime = e.runtime();
        e.clock.set(42.);
        runtime
    });
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 2,
        interval_seconds: 60.,
    };
    let valid = candle(20. * 60000.);
    let wrong = candle(22. * 60000.);
    let key = "tealstreet-candles::k::10";
    store.values.borrow_mut().insert(
        key.into(),
        serde_json::json!({"candles":[valid,wrong],"lastAccessed":0}).to_string(),
    );
    assert_eq!(
        e.run(backend.load_chunks("k".into(), vec![10], options).unwrap())
            .unwrap()[&10],
        vec![Some(valid.clone()), None]
    );
    store.values.borrow_mut().insert(
        key.into(),
        serde_json::json!({"candles":[wrong]}).to_string(),
    );
    assert!(
        e.run(backend.load_all_chunks("k".into(), options).unwrap())
            .unwrap()
            .chunks
            .is_empty()
    );
    for raw in ["broken", "null", "{}", "{\"candles\":false}", ""] {
        store.values.borrow_mut().insert(key.into(), raw.into());
        assert!(
            e.run(backend.load_chunks("k".into(), vec![10], options).unwrap())
                .unwrap()
                .is_empty()
        );
    }
    e.run(backend.persist_chunks(
        "k".into(),
        BTreeMap::from([
            (10, vec![Some(valid.clone()), None]),
            (11, vec![None, None]),
        ]),
        options,
    ))
    .unwrap();
    let value: serde_json::Value = serde_json::from_str(&store.values.borrow()[key]).unwrap();
    assert_eq!(value["lastAccessed"], 42.);
    assert_eq!(value["candles"].as_array().unwrap().len(), 1);
    assert_eq!(
        e.run(backend.load_all_chunks("k".into(), options).unwrap())
            .unwrap()
            .chunks[&10],
        vec![Some(valid), None]
    );
    store
        .values
        .borrow_mut()
        .insert("other::k::10".into(), "keep".into());
    e.run(backend.clear(Some("k".into())).unwrap()).unwrap();
    assert_eq!(store.values.borrow().len(), 1);
    store
        .values
        .borrow_mut()
        .insert("tealstreet-candles::a::1".into(), "delete-a".into());
    store
        .values
        .borrow_mut()
        .insert("tealstreet-candles::b::2".into(), "delete-b".into());
    assert_eq!(store.values.borrow().len(), 3);
    e.run(backend.clear(None).unwrap()).unwrap();
    assert_eq!(
        *store.values.borrow(),
        BTreeMap::from([("other::k::10".into(), "keep".into())])
    );
}
struct Exchange {
    calls: Rc<RefCell<Vec<(bool, String, FetchOptions)>>>,
}
impl ExchangeCandleSource for Exchange {
    fn fetch_ohlcv_in_worker(&self) -> bool {
        true
    }
    fn fetch_ohlcv(
        &self,
        symbol: String,
        options: FetchOptions,
    ) -> LocalFuture<'static, Result<Vec<Candle>, String>> {
        self.calls.borrow_mut().push((false, symbol, options));
        Box::pin(async { Ok(Vec::new()) })
    }
    fn call_method_in_worker(
        &self,
        symbol: String,
        options: FetchOptions,
    ) -> LocalFuture<'static, Result<Vec<Candle>, String>> {
        self.calls.borrow_mut().push((true, symbol, options));
        Box::pin(async { Ok(Vec::new()) })
    }
}
#[test]
fn exchange_routing_and_fetch_with_exchange() {
    let e = Executor::default();
    let calls = Rc::new(RefCell::new(Vec::new()));
    let exchange = Rc::new(Exchange {
        calls: calls.clone(),
    });
    let cache = CandleCache::new(Default::default(), e.runtime());
    let t = base();
    e.run(cache.fetch_with_exchange(CandleCacheExchangeFetchRequest {
        cache_key: "k".into(),
        exchange: exchange.clone(),
        symbol: "BTCUSDT".into(),
        use_worker: None,
        interval: timeframe("5m"),
        from_ms: t + 1.,
        to_ms: t + 1.,
        limit: 1.,
        now_ms: Some(t + 300000.),
        use_persistent_backend: Some(false),
    }))
    .unwrap();
    assert!(calls.borrow()[0].0);
    assert_eq!(calls.borrow()[0].1, "BTCUSDT");
    assert_eq!(calls.borrow()[0].2.from, t);
    let options = FetchOptions {
        interval: timeframe("5m"),
        from: t,
        to: t,
        limit: 1.,
    };
    e.run(fetch_ohlcv_with_exchange(
        exchange.clone(),
        "BTCUSDT".into(),
        Some(false),
        options.clone(),
    ))
    .unwrap();
    assert!(!calls.borrow()[1].0);
    assert_eq!(calls.borrow()[1].2, options);
    let fetch = create_exchange_candle_fetcher(exchange, "BTCUSDT".into(), Some(true));
    e.run(fetch(options)).unwrap();
    assert!(calls.borrow()[2].0);
}
struct LimitedBackend {
    inner: Rc<RecordingBackend>,
    all: bool,
    chunks: bool,
}
impl CandleCacheBackend for LimitedBackend {
    fn load_all_chunks(
        &self,
        key: String,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        if self.all {
            self.inner.load_all_chunks(key, options)
        } else {
            None
        }
    }
    fn load_chunks(
        &self,
        key: String,
        keys: Vec<i64>,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        if self.chunks {
            self.inner.load_chunks(key, keys, options)
        } else {
            None
        }
    }
    fn persist_chunks(
        &self,
        key: String,
        chunks: Chunks,
        options: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        self.inner.persist_chunks(key, chunks, options)
    }
}
#[test]
fn optional_backend_methods_and_no_backend() {
    let e = Executor::default();
    let inner = Rc::new(RecordingBackend::default());
    let t = base();
    let index = (t / 300000.).floor() as i64;
    let mut sparse = vec![None; 100];
    sparse[index.rem_euclid(100) as usize] = Some(candle(t));
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 300.,
    };
    e.run(inner.persist_chunks(
        "k".into(),
        BTreeMap::from([(index.div_euclid(100), sparse)]),
        options,
    ))
    .unwrap();
    for (all, chunks, mode, expected) in [
        (true, false, CandleCacheBackendReadMode::ReadThrough, 0),
        (false, true, CandleCacheBackendReadMode::HydrateOnce, 1),
        (false, false, CandleCacheBackendReadMode::ReadThrough, 1),
    ] {
        let backend = Rc::new(LimitedBackend {
            inner: inner.clone(),
            all,
            chunks,
        });
        let cache = CandleCache::new(
            CandleCacheOptions {
                backend: Some(backend),
                backend_read_mode: Some(mode),
                ..Default::default()
            },
            e.runtime(),
        );
        let (fetch, calls) = recording(Vec::new());
        e.run(cache.fetch(request("k", "5m", t, t, 1., t + 300000., fetch)))
            .unwrap();
        assert_eq!(calls.borrow().len(), expected);
    }
    assert_eq!(inner.all.get(), 1);
    assert!(inner.reads.borrow().is_empty());
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend_read_mode: Some(CandleCacheBackendReadMode::ReadThrough),
            ..Default::default()
        },
        e.runtime(),
    );
    let (fetch, calls) = recording(Vec::new());
    e.run(cache.fetch(request("k", "5m", t, t, 1., t + 300000., fetch)))
        .unwrap();
    assert_eq!(calls.borrow().len(), 1);
}
struct FailingStore {
    released: Rc<Cell<bool>>,
    completed: Rc<Cell<usize>>,
}
impl CandleCacheKeyValueStore for FailingStore {
    fn get(&self, _: String) -> LocalFuture<'static, Result<Option<String>, String>> {
        Box::pin(async { Ok(None) })
    }
    fn set(&self, key: String, _: String) -> LocalFuture<'static, Result<(), String>> {
        self.delete(key)
    }
    fn delete(&self, key: String) -> LocalFuture<'static, Result<(), String>> {
        if key.ends_with("::0") {
            return Box::pin(async { Err("first failed".into()) });
        }
        let released = self.released.clone();
        let completed = self.completed.clone();
        Box::pin(poll_fn(move |_| {
            if released.get() {
                completed.set(completed.get() + 1);
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            }
        }))
    }
    fn list_keys(&self, prefix: String) -> LocalFuture<'static, Result<Vec<String>, String>> {
        Box::pin(async move { Ok(vec![format!("{prefix}0"), format!("{prefix}1")]) })
    }
}
#[test]
fn failed_parallel_writes_and_deletes_do_not_cancel_siblings() {
    for delete in [false, true] {
        let e = Executor::default();
        let released = Rc::new(Cell::new(false));
        let completed = Rc::new(Cell::new(0));
        let store = Rc::new(FailingStore {
            released: released.clone(),
            completed: completed.clone(),
        });
        let backend = KeyValueCandleCacheBackend::new(store, None, e.runtime());
        let future = if delete {
            backend.clear(None).unwrap()
        } else {
            backend.persist_chunks(
                "k".into(),
                BTreeMap::from([(0, vec![Some(candle(0.))]), (1, vec![Some(candle(60000.))])]),
                CandleCacheChunkAccessOptions {
                    chunk_size: 1,
                    interval_seconds: 60.,
                },
            )
        };
        assert_eq!(e.run(future).unwrap_err(), "first failed");
        assert_eq!(completed.get(), 0);
        released.set(true);
        e.tick();
        assert_eq!(completed.get(), 1);
    }
}
#[test]
fn memory_backend_clones_chunks_and_hydrates_empty_keys_once() {
    let e = Executor::default();
    let backend = Rc::new(RecordingBackend::default());
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 2,
        interval_seconds: 60.,
    };
    e.run(backend.persist_chunks(
        "k".into(),
        BTreeMap::from([(0, vec![Some(candle(0.)), None])]),
        options,
    ))
    .unwrap();
    let mut loaded = e
        .run(backend.memory.load_all_chunks("k".into(), options).unwrap())
        .unwrap();
    loaded.chunks.get_mut(&0).unwrap()[0] = None;
    assert!(
        e.run(
            backend
                .memory
                .load_chunks("k".into(), vec![0], options)
                .unwrap()
        )
        .unwrap()[&0][0]
            .is_some()
    );
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            ..Default::default()
        },
        e.runtime(),
    );
    let (fetch, calls) = recording(Vec::new());
    for _ in 0..3 {
        e.run(cache.fetch(request(
            "empty",
            "5m",
            base(),
            base(),
            1.,
            base() + 300000.,
            fetch.clone(),
        )))
        .unwrap();
    }
    assert_eq!(backend.all.get(), 1);
    assert_eq!(calls.borrow().len(), 3);
    cache.reset(None, Some("empty".into()));
    e.run(cache.fetch(request(
        "empty",
        "5m",
        base(),
        base(),
        1.,
        base() + 300000.,
        fetch,
    )))
    .unwrap();
    assert_eq!(backend.all.get(), 2);
}
#[test]
fn key_value_typed_boundary_and_parse_int_prefix() {
    let e = Executor::default();
    let store = Rc::new(Store::default());
    let backend = KeyValueCandleCacheBackend::new(store.clone(), None, e.runtime());
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 2,
        interval_seconds: 60.,
    };
    let valid = candle(1200000.);
    store.values.borrow_mut().insert(
        "tealstreet-candles::k::10".into(),
        serde_json::json!({"candles":[valid,{"timestamp":1320000.}]}).to_string(),
    );
    store
        .values
        .borrow_mut()
        .insert("tealstreet-candles::k::10junk".into(), "ignored".into());
    store
        .values
        .borrow_mut()
        .insert("tealstreet-candles::k::junk".into(), "ignored".into());
    store.values.borrow_mut().insert(
        "tealstreet-candles::k::9223372036854775808".into(),
        "ignored".into(),
    );
    let loaded = e
        .run(backend.load_all_chunks("k".into(), options).unwrap())
        .unwrap();
    assert_eq!(
        loaded.refused_keys,
        ["tealstreet-candles::k::9223372036854775808"]
    );
    assert_eq!(loaded.chunks[&10][0], Some(valid));
    store.values.borrow_mut().insert(
        "tealstreet-candles::k::10".into(),
        serde_json::json!({"candles":[{"timestamp":1200000.}]}).to_string(),
    );
    assert!(
        e.run(backend.load_chunks("k".into(), vec![10], options).unwrap())
            .unwrap()
            .is_empty()
    );
}
#[test]
fn throwing_hydrations_fill_cap_then_release_queued_hydration() {
    let e = Executor::default();
    let backend = Rc::new(RecordingBackend::default());
    backend.fail.set(true);
    let timeouts = Rc::new(Cell::new(0));
    let observed = timeouts.clone();
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            max_concurrent_hydrations: Some(2),
            on_hydration_timeout: Some(Rc::new(move |_| observed.set(observed.get() + 1))),
            ..Default::default()
        },
        e.runtime(),
    );
    let (fetch, _) = recording(Vec::new());
    let mut requests: Vec<_> = ["failure-a", "failure-b", "queued-after-cap"]
        .into_iter()
        .map(|key| {
            Box::pin(cache.fetch(request(
                key,
                "5m",
                base(),
                base(),
                1.,
                base() + 300000.,
                fetch.clone(),
            )))
        })
        .collect();
    let mut cx = Context::from_waker(Waker::noop());
    for request in &mut requests {
        assert!(request.as_mut().poll(&mut cx).is_pending());
    }
    // TS: queuedHydrate.catch calls finish, which decrements and drains at the cap.
    assert_eq!(e.tasks.borrow().len(), 2);
    e.tick();
    assert_eq!(backend.all.get(), 2);
    assert_eq!(
        e.tasks.borrow().len(),
        1,
        "the queued hydration must be spawned after failures"
    );
    backend.fail.set(false);
    e.tick();
    assert_eq!(
        backend.all.get(),
        3,
        "a later successful hydration must run"
    );
    for request in requests {
        assert!(e.run(request).unwrap().is_empty());
    }
    assert_eq!(e.clock.get(), 0.);
    assert_eq!(
        timeouts.get(),
        0,
        "error cleanup must work without timer advancement"
    );
}
#[test]
fn timeout_releases_cap_and_empty_key_retries_before_late_writer() {
    let e = Executor::default();
    let backend = Rc::new(RecordingBackend::default());
    backend.hang_first.set(true);
    let timeouts = Rc::new(Cell::new(0));
    let observed = timeouts.clone();
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            max_concurrent_hydrations: Some(1),
            hydration_timeout_ms: Some(10.),
            on_hydration_timeout: Some(Rc::new(move |_| observed.set(observed.get() + 1))),
            ..Default::default()
        },
        e.runtime(),
    );
    let t = base();
    let (empty, _) = recording(Vec::new());
    let mut first = Box::pin(cache.fetch(request(
        "timed-out",
        "5m",
        t,
        t,
        1.,
        t + 600000.,
        empty.clone(),
    )));
    let mut queued = Box::pin(cache.fetch(request(
        "queued",
        "5m",
        t,
        t,
        1.,
        t + 600000.,
        empty.clone(),
    )));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(queued.as_mut().poll(&mut cx).is_pending());
    e.tick();
    assert_eq!(backend.all.get(), 1);
    e.clock.set(9.);
    e.tick();
    assert_eq!(
        backend.all.get(),
        1,
        "the slot remains occupied before the deadline"
    );
    e.clock.set(11.);
    e.tick();
    e.tick();
    assert_eq!(
        backend.all.get(),
        2,
        "timeout must start the queued hydration"
    );
    assert!(e.run(first).unwrap().is_empty());
    assert!(e.run(queued).unwrap().is_empty());
    assert_eq!(timeouts.get(), 1);
    assert!(backend.pending.borrow().is_none());
    assert!(
        e.run(cache.fetch(request(
            "timed-out",
            "5m",
            t,
            t,
            1.,
            t + 600000.,
            empty.clone()
        )))
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        backend.all.get(),
        3,
        "timeout alone must not mark an empty key hydrated"
    );
    let index = (t / 300000.).floor() as i64;
    let mut chunk = vec![None; 100];
    let mut stale = candle(t);
    stale.close = 101.;
    chunk[index.rem_euclid(100) as usize] = Some(stale.clone());
    *backend.pending.borrow_mut() = Some(BTreeMap::from([(index.div_euclid(100), chunk)]));
    e.tick();
    assert_eq!(
        cache
            .get_cached_candles("timed-out", &timeframe("5m"), t, t, 1., Some(t + 600000.))
            .contiguous_behind_candles,
        vec![stale],
        "TS allows an uncancelled old load to land after a newer hydration"
    );
    assert!(
        e.run(cache.fetch(request("after-late", "5m", t, t, 1., t + 600000., empty)))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        backend.all.get(),
        4,
        "late completion must not release the slot a second time"
    );
}
#[test]
fn timeout_memory_map_suppresses_retry_even_when_no_closed_candle_cached() {
    let e = Executor::default();
    let backend = Rc::new(RecordingBackend::default());
    backend.hang_first.set(true);
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            max_concurrent_hydrations: Some(1),
            hydration_timeout_ms: Some(10.),
            ..Default::default()
        },
        e.runtime(),
    );
    let t = base();
    let (forming, _) = recording(vec![candle(t)]);
    let mut first =
        Box::pin(cache.fetch(request("timed-out", "5m", t, t, 1., t + 60000., forming)));
    assert!(
        first
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    e.tick();
    e.clock.set(11.);
    e.tick();
    assert_eq!(e.run(first).unwrap(), vec![candle(t)]);
    assert!(
        cache
            .get_cached_candles("timed-out", &timeframe("5m"), t, t, 1., Some(t + 60000.))
            .contiguous_behind_candles
            .is_empty()
    );
    let (empty, _) = recording(Vec::new());
    for _ in 0..3 {
        assert!(
            e.run(cache.fetch(request(
                "timed-out",
                "5m",
                t,
                t,
                1.,
                t + 60000.,
                empty.clone()
            )))
            .unwrap()
            .is_empty()
        );
    }
    assert_eq!(
        backend.all.get(),
        1,
        "ts-bug: CandleCache.queuedHydrate skips an unhydrated key with an empty memory map"
    );
    cache.reset(None, Some("timed-out".into()));
    assert!(
        e.run(cache.fetch(request("timed-out", "5m", t, t, 1., t + 60000., empty)))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        backend.all.get(),
        2,
        "reset removes the memory-map gate and allows hydration again"
    );
}

fn timeframe(value: &str) -> AnyTimeframe {
    serde_json::from_value(serde_json::Value::String(value.into())).unwrap()
}
#[test]
fn every_ts_timeframe_has_finite_slots_and_cache_round_trips() {
    let entries = [
        ("1s", 1.),
        ("5s", 5.),
        ("15s", 15.),
        ("30s", 30.),
        ("1m", 60.),
        ("3m", 180.),
        ("5m", 300.),
        ("15m", 900.),
        ("30m", 1800.),
        ("1h", 3600.),
        ("2h", 7200.),
        ("3h", 10800.),
        ("4h", 14400.),
        ("5h", 18000.),
        ("6h", 21600.),
        ("7h", 25200.),
        ("8h", 28800.),
        ("9h", 32400.),
        ("10h", 36000.),
        ("11h", 39600.),
        ("12h", 43200.),
        ("24h", 86400.),
        ("1d", 86400.),
        ("2d", 172800.),
        ("3d", 259200.),
        ("1w", 604800.),
        ("1M", 2592000.),
    ];
    for (label, seconds) in entries {
        let interval = timeframe(label);
        assert_eq!(
            crate::calendar::name(&interval),
            label,
            "literal interval label"
        );
        let suffix = if matches!(label, "1w" | "1M") {
            "-cal"
        } else {
            ""
        };
        for testnet in [None, Some(false), Some(true)] {
            let environment = if testnet == Some(true) {
                "-testnet"
            } else {
                ""
            };
            assert_eq!(
                get_candle_cache_key(&ExchangeName::Binance, "BTCUSDT", &interval, testnet),
                format!("binance{environment}-BTCUSDT-{label}{suffix}"),
                "literal namespace for {label}, {testnet:?}"
            );
        }
        assert_eq!(crate::calendar::seconds(&interval), seconds, "{label}");
        let e = Executor::default();
        let backend = Rc::new(RecordingBackend::default());
        let make = || {
            CandleCache::new(
                CandleCacheOptions {
                    backend: Some(backend.clone()),
                    backend_read_mode: Some(CandleCacheBackendReadMode::ReadThrough),
                    ..Default::default()
                },
                e.runtime(),
            )
        };
        let first = crate::calendar::start(&interval, base() / 1000.) * 1000.;
        let second = crate::calendar::next(&interval, first / 1000.) * 1000.;
        let now = crate::calendar::next(&interval, second / 1000.) * 1000.;
        let expected = vec![candle(first), candle(second)];
        let (fetch, calls) = recording(expected.clone());
        assert_eq!(
            e.run(make().fetch(request(label, label, first, second, 2., now, fetch)))
                .unwrap(),
            expected
        );
        assert_eq!(
            calls.borrow()[0],
            FetchOptions {
                interval: interval.clone(),
                from: first,
                to: second,
                limit: 2.
            }
        );
        assert!(calls.borrow()[0].from.is_finite() && calls.borrow()[0].to.is_finite());
        let chunks = e
            .run(
                backend
                    .memory
                    .load_all_chunks(
                        label.into(),
                        CandleCacheChunkAccessOptions {
                            chunk_size: 100,
                            interval_seconds: seconds,
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            chunks.chunks.values().flatten().flatten().count(),
            2,
            "{label} must retain both candles"
        );
        assert!(
            chunks.chunks.keys().all(|key| *key != 0),
            "{label} must not saturate into chunk zero"
        );
        let (fetch, calls) = recording(Vec::new());
        assert_eq!(
            e.run(make().fetch(request(label, label, first, second, 2., now, fetch)))
                .unwrap(),
            expected
        );
        assert!(
            calls.borrow().is_empty(),
            "{label} must hydrate both candles"
        );
    }
    for unsupported in ["nonsense", "0s", "1W", "W", "M", ""] {
        assert!(
            serde_json::from_value::<AnyTimeframe>(serde_json::Value::String(unsupported.into()))
                .is_err()
        );
    }
}

#[test]
fn dropped_unpolled_hydration_tasks_release_slots_and_wake_callers() {
    let e = Executor::default();
    let discard = Rc::new(Cell::new(true));
    let dropping = discard.clone();
    let tasks = e.tasks.clone();
    let mut runtime = e.runtime();
    runtime.spawn = Rc::new(move |task| {
        if dropping.get() {
            drop(task);
        } else {
            tasks.borrow_mut().push(task);
        }
    });
    let backend = Rc::new(RecordingBackend::default());
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            max_concurrent_hydrations: Some(2),
            ..Default::default()
        },
        runtime,
    );
    let (empty, calls) = recording(Vec::new());
    for index in 0..6 {
        assert!(
            e.run(cache.fetch(request(
                &format!("discarded-{index}"),
                "5m",
                base(),
                base(),
                1.,
                base() + 300000.,
                empty.clone()
            )))
            .unwrap()
            .is_empty()
        );
    }
    assert_eq!(backend.all.get(), 0);
    assert_eq!(
        calls.borrow().len(),
        6,
        "discarding a task must wake each caller for fallback"
    );
    discard.set(false);
    assert!(
        e.run(cache.fetch(request(
            "healthy",
            "5m",
            base(),
            base(),
            1.,
            base() + 300000.,
            empty
        )))
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        backend.all.get(),
        1,
        "discarded tasks must not exhaust the cap"
    );
    assert_eq!(e.clock.get(), 0.);
}
#[test]
fn dropped_pending_hydration_drains_queue_and_leaves_key_retryable() {
    let e = Executor::default();
    let backend = Rc::new(RecordingBackend::default());
    backend.hang_first.set(true);
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            max_concurrent_hydrations: Some(1),
            ..Default::default()
        },
        e.runtime(),
    );
    let (empty, _) = recording(Vec::new());
    let mut first = Box::pin(cache.fetch(request(
        "discarded",
        "5m",
        base(),
        base(),
        1.,
        base() + 300000.,
        empty.clone(),
    )));
    let mut second = Box::pin(cache.fetch(request(
        "queued",
        "5m",
        base(),
        base(),
        1.,
        base() + 300000.,
        empty.clone(),
    )));
    let mut cx = Context::from_waker(Waker::noop());
    assert!(first.as_mut().poll(&mut cx).is_pending());
    assert!(second.as_mut().poll(&mut cx).is_pending());
    e.tick();
    assert_eq!(backend.all.get(), 1);
    let abandoned = std::mem::take(&mut *e.tasks.borrow_mut());
    assert_eq!(abandoned.len(), 1);
    drop(abandoned);
    assert_eq!(
        e.tasks.borrow().len(),
        1,
        "drop cleanup must spawn the queued hydration"
    );
    e.tick();
    assert_eq!(backend.all.get(), 2);
    assert!(e.run(first).unwrap().is_empty());
    assert!(e.run(second).unwrap().is_empty());
    assert!(
        e.run(cache.fetch(request(
            "discarded",
            "5m",
            base(),
            base(),
            1.,
            base() + 300000.,
            empty
        )))
        .unwrap()
        .is_empty()
    );
    assert_eq!(
        backend.all.get(),
        3,
        "task drop must not mark the abandoned key hydrated"
    );
    assert_eq!(
        e.clock.get(),
        0.,
        "the timer is not needed for drop cleanup"
    );
}

#[test]
fn malformed_stored_candle_keeps_valid_siblings_and_fetches_only_its_gap() {
    for mode in [
        CandleCacheBackendReadMode::ReadThrough,
        CandleCacheBackendReadMode::HydrateOnce,
    ] {
        let e = Executor::default();
        let store = Rc::new(Store::default());
        let backend = Rc::new(KeyValueCandleCacheBackend::new(
            store.clone(),
            None,
            e.runtime(),
        ));
        let t = base();
        let expected = vec![candle(t), candle(t + 300000.), candle(t + 600000.)];
        let mut malformed = serde_json::to_value(&expected[1]).unwrap();
        malformed.as_object_mut().unwrap().remove("volume");
        let chunk_key = (t / 300000.).floor() as i64 / 2;
        store.values.borrow_mut().insert(
            format!("tealstreet-candles::mixed::{chunk_key}"),
            serde_json::json!({"candles":[expected[0],malformed],"lastAccessed":0}).to_string(),
        );
        store.values.borrow_mut().insert(
            format!("tealstreet-candles::mixed::{}", chunk_key + 1),
            serde_json::json!({"candles":[expected[2]],"lastAccessed":0}).to_string(),
        );
        let cache = CandleCache::new(
            CandleCacheOptions {
                backend: Some(backend.clone()),
                backend_read_mode: Some(mode),
                chunk_size: Some(2),
                ..Default::default()
            },
            e.runtime(),
        );
        let (fetch, calls) = recording(vec![expected[1].clone()]);
        assert_eq!(
            e.run(cache.fetch(request(
                "mixed",
                "5m",
                t,
                t + 600000.,
                3.,
                t + 900000.,
                fetch
            )))
            .unwrap(),
            expected
        );
        assert_eq!(
            *calls.borrow(),
            vec![FetchOptions {
                interval: timeframe("5m"),
                from: t + 300000.,
                to: t + 300000.,
                limit: 1.
            }],
            "only the malformed candle's gap should reach the fetcher"
        );
        let reread = e
            .run(
                backend
                    .load_chunks(
                        "mixed".into(),
                        vec![chunk_key],
                        CandleCacheChunkAccessOptions {
                            chunk_size: 2,
                            interval_seconds: 300.,
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            reread[&chunk_key],
            vec![Some(expected[0].clone()), Some(expected[1].clone())]
        );
    }
}

#[test]
fn malformed_cached_candle_is_a_miss_and_refetches_without_error() {
    let mut cases = 0;
    for mode in [
        CandleCacheBackendReadMode::ReadThrough,
        CandleCacheBackendReadMode::HydrateOnce,
    ] {
        for field in ["timestamp", "open", "high", "low", "close", "volume"] {
            for invalid in [
                None,
                Some(serde_json::Value::Null),
                Some(serde_json::json!("invalid")),
                Some(serde_json::json!(false)),
            ] {
                // Numeric null is the JSON encoding of NaN, covered by real
                // persistence roundtrips; timestamp null remains malformed.
                if field != "timestamp" && invalid == Some(serde_json::Value::Null) {
                    continue;
                }
                cases += 1;
                let e = Executor::default();
                let store = Rc::new(Store::default());
                let backend = Rc::new(KeyValueCandleCacheBackend::new(
                    store.clone(),
                    None,
                    e.runtime(),
                ));
                let expected = candle(1_700_004_000_000.);
                let mut malformed = serde_json::to_value(&expected).unwrap();
                if let Some(value) = invalid {
                    malformed[field] = value;
                } else {
                    malformed.as_object_mut().unwrap().remove(field);
                }
                store.values.borrow_mut().insert(
                    "tealstreet-candles::malformed::283334".into(),
                    serde_json::json!({"candles":[malformed],"lastAccessed":0}).to_string(),
                );
                let diagnostics = Rc::new(RefCell::new(Vec::new()));
                let reports = diagnostics.clone();
                let cache = CandleCache::new(
                    CandleCacheOptions {
                        backend: Some(backend.clone()),
                        backend_read_mode: Some(mode),
                        chunk_size: Some(100),
                        on_hydration_error: Some(Rc::new(move |key, error| {
                            reports.borrow_mut().push((key, error))
                        })),
                        ..Default::default()
                    },
                    e.runtime(),
                );
                let (fetch, calls) = recording(vec![expected.clone()]);
                let result = e
                    .run(cache.fetch(request(
                        "malformed",
                        "1m",
                        expected.timestamp,
                        expected.timestamp,
                        1.,
                        expected.timestamp + 120_000.,
                        fetch.clone(),
                    )))
                    .expect(
                        "CACHE-MALFORMED unsupported cache rows must not fail the candle request",
                    );
                assert_eq!(
                    result,
                    vec![expected.clone()],
                    "CACHE-MALFORMED returns the fetched candle for {field}"
                );
                assert_eq!(
                    *calls.borrow(),
                    vec![FetchOptions {
                        interval: timeframe("1m"),
                        from: expected.timestamp,
                        to: expected.timestamp,
                        limit: 1.
                    }],
                    "CACHE-MALFORMED must refetch exactly the missing slot for {field}"
                );
                assert!(
                    store.reads.get() > 0,
                    "CACHE-MALFORMED must read the real KV backend"
                );
                assert!(
                    diagnostics.borrow().is_empty(),
                    "malformed cache content is a miss, not a backend failure"
                );
                let reread = e
                    .run(
                        backend
                            .load_chunks(
                                "malformed".into(),
                                vec![283334],
                                CandleCacheChunkAccessOptions {
                                    chunk_size: 100,
                                    interval_seconds: 60.,
                                },
                            )
                            .unwrap(),
                    )
                    .unwrap();
                assert_eq!(
                    reread[&283334][0],
                    Some(expected.clone()),
                    "CACHE-MALFORMED must persist the fetched replacement"
                );
                assert_eq!(
                    e.run(cache.fetch(request(
                        "malformed",
                        "1m",
                        expected.timestamp,
                        expected.timestamp,
                        1.,
                        expected.timestamp + 120_000.,
                        fetch
                    )))
                    .unwrap(),
                    vec![expected]
                );
                assert_eq!(
                    calls.borrow().len(),
                    1,
                    "CACHE-MALFORMED repaired slot must be a cache hit"
                );
            }
        }
    }
    assert_eq!(cases, 38, "structurally malformed case floor");
}

#[test]
fn zero_chunk_size_degrades_to_exchange_without_cache_io() {
    let e = Executor::default();
    let store = Rc::new(Store::default());
    let backend = Rc::new(KeyValueCandleCacheBackend::new(
        store.clone(),
        None,
        e.runtime(),
    ));
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 0,
        interval_seconds: 60.,
    };
    store.values.borrow_mut().insert(
        "tealstreet-candles::k::0".into(),
        serde_json::json!({"candles":[candle(base())]}).to_string(),
    );
    assert!(
        e.run(backend.load_chunks("k".into(), vec![0], options).unwrap())
            .unwrap()
            .is_empty()
    );
    let cache = CandleCache::new(
        CandleCacheOptions {
            chunk_size: Some(0),
            backend: Some(backend),
            ..Default::default()
        },
        e.runtime(),
    );
    let (fetcher, calls) = recording(vec![candle(base())]);
    let before = store.values.borrow().clone();
    for _ in 0..2 {
        assert_eq!(
            e.run(cache.fetch(request(
                "k",
                "1m",
                base(),
                base(),
                1.,
                base() + 60000.,
                fetcher.clone()
            )))
            .unwrap(),
            vec![candle(base())]
        );
        assert!(
            cache
                .get_cached_candles(
                    "k",
                    &timeframe("1m"),
                    base(),
                    base(),
                    1.,
                    Some(base() + 60000.)
                )
                .contiguous_behind_candles
                .is_empty()
        );
    }
    assert_eq!(calls.borrow().len(), 2);
    assert_eq!(store.reads.get(), 0);
    assert_eq!(store.lists.get(), 0);
    assert_eq!(*store.values.borrow(), before);
    assert!(e.tasks.borrow().is_empty());
}

#[test]
fn pre_epoch_candles_keep_euclidean_slots_across_persistence() {
    let e = Executor::default();
    let store = Rc::new(Store::default());
    let backend = Rc::new(KeyValueCandleCacheBackend::new(
        store.clone(),
        None,
        e.runtime(),
    ));
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            ..Default::default()
        },
        e.runtime(),
    );
    let expected = candle(-60000.);
    let (fetcher, _) = recording(vec![candle(-60.)]);
    assert_eq!(
        e.run(cache.fetch(request("old", "1m", -60., -60., 1., 60000., fetcher)))
            .unwrap(),
        vec![candle(-60.)]
    );
    let chunks = e
        .run(
            backend
                .load_chunks(
                    "old".into(),
                    vec![-1],
                    CandleCacheChunkAccessOptions {
                        chunk_size: 100,
                        interval_seconds: 60.,
                    },
                )
                .unwrap(),
        )
        .unwrap();
    assert_eq!(chunks[&-1][99], Some(expected.clone()));
    let fresh = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend),
            backend_read_mode: Some(CandleCacheBackendReadMode::ReadThrough),
            ..Default::default()
        },
        e.runtime(),
    );
    let (empty, calls) = recording(vec![]);
    assert_eq!(
        e.run(fresh.fetch(request("old", "1m", -60., -60., 1., 60000., empty)))
            .unwrap(),
        vec![expected]
    );
    assert!(calls.borrow().is_empty());
}

#[test]
fn eager_spawner_drains_1200_queued_hydrations_without_recursion() {
    struct Backend {
        ready: Rc<Cell<bool>>,
        calls: Rc<Cell<usize>>,
        inner: crate::async_storage::ReactNativeAsyncStorageCandleCacheBackend,
    }
    impl CandleCacheBackend for Backend {
        fn load_all_chunks(
            &self,
            key: String,
            options: CandleCacheChunkAccessOptions,
        ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
            self.calls.set(self.calls.get() + 1);
            let ready = self.ready.clone();
            let read = self.inner.load_all_chunks(key, options).unwrap();
            Some(Box::pin(async move {
                poll_fn(move |_| {
                    if ready.get() {
                        Poll::Ready(())
                    } else {
                        Poll::Pending
                    }
                })
                .await;
                read.await
            }))
        }
        fn persist_chunks(
            &self,
            _: String,
            _: Chunks,
            _: CandleCacheChunkAccessOptions,
        ) -> LocalFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }
    let e = Executor::default();
    let depth = Rc::new(Cell::new(0usize));
    let maximum = Rc::new(Cell::new(0));
    let mut runtime = e.runtime();
    let tasks = e.tasks.clone();
    let active = depth.clone();
    let peak = maximum.clone();
    runtime.spawn = Rc::new(move |mut task| {
        active.set(active.get() + 1);
        peak.set(peak.get().max(active.get()));
        // Fail early under the negative control instead of aborting the test process.
        assert!(active.get() < 16, "recursive eager drain");
        let pending = task
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending();
        active.set(active.get() - 1);
        if pending {
            tasks.borrow_mut().push(task);
        }
    });
    let ready = Rc::new(Cell::new(false));
    let calls = Rc::new(Cell::new(0));
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(Rc::new(Backend {
                ready: ready.clone(),
                calls: calls.clone(),
                inner: crate::async_storage::ReactNativeAsyncStorageCandleCacheBackend::new(
                    Rc::new(Store::default()),
                    None,
                    runtime.clone(),
                ),
            })),
            max_concurrent_hydrations: Some(1),
            ..Default::default()
        },
        runtime,
    );
    let (empty, _) = recording(vec![]);
    let mut fetches: Vec<_> = (0..1201)
        .map(|i| {
            let cache = cache.clone();
            let empty = empty.clone();
            Box::pin(async move {
                cache
                    .fetch(request(
                        &format!("eager-{i}"),
                        "1m",
                        base(),
                        base(),
                        1.,
                        base() + 60000.,
                        empty,
                    ))
                    .await
            })
        })
        .collect();
    for fetch in &mut fetches {
        assert!(
            fetch
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    assert_eq!(calls.get(), 1);
    ready.set(true);
    e.tick();
    assert_eq!(calls.get(), 1201);
    assert_eq!(maximum.get(), 1);
    for fetch in fetches {
        assert!(e.run(fetch).unwrap().is_empty());
    }
    // The guard resets after draining, so a subsequent request also starts.
    assert!(
        e.run(cache.fetch(request(
            "eager-next",
            "1m",
            base(),
            base(),
            1.,
            base() + 60000.,
            empty
        )))
        .unwrap()
        .is_empty()
    );
    assert_eq!(calls.get(), 1202);
}

#[test]
fn non_finite_fetcher_timestamps_never_write_chunk_zero() {
    for mode in [
        CandleCacheBackendReadMode::ReadThrough,
        CandleCacheBackendReadMode::HydrateOnce,
    ] {
        let e = Executor::default();
        let store = Rc::new(Store::default());
        let backend = Rc::new(KeyValueCandleCacheBackend::new(
            store.clone(),
            None,
            e.runtime(),
        ));
        let cache = CandleCache::new(
            CandleCacheOptions {
                backend: Some(backend),
                backend_read_mode: Some(mode),
                ..Default::default()
            },
            e.runtime(),
        );
        let (bad, _) = recording(vec![
            candle(f64::NAN),
            candle(f64::NEG_INFINITY),
            candle(f64::INFINITY),
        ]);
        e.run(cache.fetch(request(
            "finite",
            "1m",
            base(),
            base(),
            1.,
            base() + 60000.,
            bad,
        )))
        .unwrap();
        assert!(
            store.values.borrow().is_empty(),
            "invalid timestamps must not be persisted"
        );
        let (good, calls) = recording(vec![candle(base())]);
        assert_eq!(
            e.run(cache.fetch(request(
                "finite",
                "1m",
                base(),
                base(),
                1.,
                base() + 60000.,
                good.clone()
            )))
            .unwrap(),
            vec![candle(base())]
        );
        assert_eq!(
            e.run(cache.fetch(request(
                "finite",
                "1m",
                base(),
                base(),
                1.,
                base() + 60000.,
                good
            )))
            .unwrap(),
            vec![candle(base())]
        );
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(store.values.borrow().len(), 1);
        assert!(
            !store
                .values
                .borrow()
                .contains_key("tealstreet-candles::finite::0")
        );
    }
}

fn exchange_name(value: &str) -> ExchangeName {
    serde_json::from_value(serde_json::json!(value)).unwrap()
}

#[test]
fn exchange_union_rejects_forged_testnet_names() {
    for exchange in ["binance-testnet", "hyperliquid-testnet", "unknown"] {
        assert!(serde_json::from_value::<ExchangeName>(serde_json::json!(exchange)).is_err());
    }
    assert_eq!(
        get_candle_cache_key(&ExchangeName::Binance, "BTCUSDT", &timeframe("1m"), None),
        "binance-BTCUSDT-1m"
    );
    assert_eq!(
        get_candle_cache_key(
            &ExchangeName::Binance,
            "BTCUSDT",
            &timeframe("1m"),
            Some(false)
        ),
        "binance-BTCUSDT-1m"
    );
    assert_eq!(
        get_candle_cache_key(
            &ExchangeName::Binance,
            "BTCUSDT",
            &timeframe("1m"),
            Some(true)
        ),
        "binance-testnet-BTCUSDT-1m"
    );
}

fn assert_cleaning_source_profile(actual: &[Candle], profile: usize) {
    static SOURCE: std::sync::OnceLock<Vec<serde_json::Value>> = std::sync::OnceLock::new();
    let source = SOURCE.get_or_init(|| {
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fix_audit_source.cjs"
            ))
            .arg("--seeded-clean-candles")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "seeded source {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(rows.len(), 2002, "independent legacy source floor");
        rows
    });
    let observed: Vec<_> = actual
        .iter()
        .map(|r| {
            [r.timestamp, r.open, r.high, r.low, r.close, r.volume]
                .map(|v| format!("{:016x}", v.to_bits()))
        })
        .collect();
    assert_eq!(
        serde_json::json!(observed),
        source[profile],
        "CLEAN-CANDLES exact source fields/order {profile}"
    );
}

#[test]
fn cleaning_source_nan_permutations_keep_first_duplicates() {
    let mut first_nan = candle(f64::NAN);
    first_nan.close = 777.;
    let mut candles: Vec<_> = [10., 9., 8., 7., 6.].into_iter().map(candle).collect();
    candles.push(first_nan);
    candles.extend([5., 4., 3., 2., 1.].into_iter().map(candle));
    candles.push(candle(f64::from_bits(0x7ff8_0000_0000_0001)));
    let result = clean_candles(candles);
    assert_eq!(result.len(), 11);
    assert_cleaning_source_profile(&result, 0);
    let retained_nan = result.iter().find(|c| c.timestamp.is_nan()).unwrap();
    assert_eq!(retained_nan.close, 777.);
    let result = clean_candles(vec![
        candle(f64::NAN),
        candle(f64::INFINITY),
        candle(-0.),
        candle(f64::NEG_INFINITY),
        candle(0.),
    ]);
    assert_cleaning_source_profile(&result, 1);
    assert_eq!(
        result
            .iter()
            .find(|c| c.timestamp == 0.)
            .unwrap()
            .timestamp
            .to_bits(),
        (-0f64).to_bits()
    );
}

#[test]
fn cleaning_2000_nan_permutations_match_actual_source_order() {
    let mut state = 0x1234_5678_9abc_def0u64;
    for case in 0..2000 {
        let mut input = Vec::new();
        let mut first_nan = candle(f64::NAN);
        first_nan.close = f64::from(case);
        input.push(first_nan);
        for index in 0..64 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            if state.is_multiple_of(7) {
                input.push(candle(f64::NAN));
            } else {
                let timestamp = ((state >> 32) % 81) as i32 - 40;
                let mut value = candle(f64::from(timestamp));
                value.close = f64::from(index);
                input.push(value);
            }
        }
        let actual = clean_candles(input);
        assert_cleaning_source_profile(&actual, case as usize + 2);
    }
}

#[test]
fn oversized_chunks_return_errors_before_allocation_or_io() {
    let e = Executor::default();
    let store = Rc::new(Store::default());
    let backend = Rc::new(KeyValueCandleCacheBackend::new(
        store.clone(),
        None,
        e.runtime(),
    ));
    store.values.borrow_mut().insert(
        "tealstreet-candles::huge::0".into(),
        serde_json::json!({"candles":[candle(base())]}).to_string(),
    );
    for size in [usize::MAX, 16_385, u32::MAX as usize] {
        let options = CandleCacheChunkAccessOptions {
            chunk_size: size,
            interval_seconds: 60.,
        };
        assert!(
            e.run(
                backend
                    .load_chunks("huge".into(), vec![0], options)
                    .unwrap()
            )
            .unwrap_err()
            .contains("chunk_size")
        );
        assert!(
            e.run(backend.load_all_chunks("huge".into(), options).unwrap())
                .unwrap_err()
                .contains("chunk_size")
        );
        let cache = CandleCache::new(
            CandleCacheOptions {
                backend: Some(backend.clone()),
                chunk_size: Some(size),
                ..Default::default()
            },
            e.runtime(),
        );
        let (fetcher, calls) = recording(vec![candle(base())]);
        assert!(
            e.run(cache.fetch(request(
                "huge",
                "1m",
                base(),
                base(),
                1.,
                base() + 60000.,
                fetcher
            )))
            .unwrap_err()
            .contains("chunk_size")
        );
        assert!(calls.borrow().is_empty());
        assert!(
            cache
                .get_cached_candles(
                    "huge",
                    &timeframe("1m"),
                    base(),
                    base(),
                    1.,
                    Some(base() + 60000.)
                )
                .contiguous_behind_candles
                .is_empty()
        );
    }
    assert_eq!(store.reads.get(), 0);
    assert_eq!(store.lists.get(), 0);
    assert!(e.tasks.borrow().is_empty());
    let cache = CandleCache::new(
        CandleCacheOptions {
            chunk_size: Some(16_384),
            ..Default::default()
        },
        e.runtime(),
    );
    let (fetcher, calls) = recording(vec![candle(base())]);
    for _ in 0..2 {
        assert_eq!(
            e.run(cache.fetch(request(
                "boundary",
                "1m",
                base(),
                base(),
                1.,
                base() + 60000.,
                fetcher.clone()
            )))
            .unwrap(),
            vec![candle(base())]
        );
    }
    assert_eq!(
        calls.borrow().len(),
        1,
        "maximum supported width still caches"
    );
}

#[test]
fn cancelled_queued_hydration_does_not_retain_cache_backend() {
    struct Sentinel {
        dropped: Rc<Cell<bool>>,
    }
    impl Drop for Sentinel {
        fn drop(&mut self) {
            self.dropped.set(true);
        }
    }
    impl CandleCacheBackend for Sentinel {
        fn persist_chunks(
            &self,
            _: String,
            _: Chunks,
            _: CandleCacheChunkAccessOptions,
        ) -> LocalFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }
    let e = Executor::default();
    let dropped = Rc::new(Cell::new(false));
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(Rc::new(Sentinel {
                dropped: dropped.clone(),
            })),
            // A zero cap isolates a queued job from legitimate active task ownership.
            max_concurrent_hydrations: Some(0),
            ..Default::default()
        },
        e.runtime(),
    );
    let (empty, calls) = recording(vec![]);
    let mut fetch = Box::pin(cache.fetch(request(
        "queued-owner",
        "1m",
        base(),
        base(),
        1.,
        base() + 60000.,
        empty,
    )));
    assert!(
        fetch
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
            .is_pending()
    );
    assert!(
        !dropped.get(),
        "the backend is live while the cache is owned"
    );
    assert!(calls.borrow().is_empty());
    assert!(
        e.tasks.borrow().is_empty(),
        "no active host task may own the cache"
    );
    drop(fetch);
    assert!(!dropped.get());
    drop(cache);
    assert!(
        dropped.get(),
        "queue must not keep the unreachable cache/backend alive"
    );
}

#[test]
fn february_month_slots_preserve_both_candles_in_storage_and_fresh_cache() {
    for (year, february_ms, march_ms, april_ms) in [
        (1970, 2_678_400_000., 5_097_600_000., 7_776_000_000.),
        (1987, 539_136_000_000., 541_555_200_000., 544_233_600_000.),
        (
            2010,
            1_264_982_400_000.,
            1_267_401_600_000.,
            1_270_080_000_000.,
        ),
        (
            2027,
            1_801_440_000_000.,
            1_803_859_200_000.,
            1_806_537_600_000.,
        ),
        (
            2050,
            2_527_286_400_000.,
            2_529_705_600_000.,
            2_532_384_000_000.,
        ),
    ] {
        let e = Executor::default();
        let store = Rc::new(Store::default());
        let backend = Rc::new(KeyValueCandleCacheBackend::new(
            store.clone(),
            None,
            e.runtime(),
        ));
        let make = || {
            CandleCache::new(
                CandleCacheOptions {
                    backend: Some(backend.clone()),
                    backend_read_mode: Some(CandleCacheBackendReadMode::ReadThrough),
                    ..Default::default()
                },
                e.runtime(),
            )
        };
        let cache = make();
        let key = format!("month-{year}-1M-cal");
        let seconds_candles = vec![candle(february_ms / 1000.), candle(march_ms / 1000.)];
        let (fetcher, _) = recording(seconds_candles.clone());
        assert_eq!(
            e.run(cache.fetch(request(
                &key,
                "1M",
                february_ms / 1000.,
                march_ms / 1000.,
                2.,
                april_ms,
                fetcher
            )))
            .unwrap(),
            seconds_candles
        );
        let mut persisted: Vec<_> = store
            .values
            .borrow()
            .values()
            .flat_map(|raw| {
                serde_json::from_str::<CandleCacheChunkRecord>(raw)
                    .unwrap()
                    .candles
            })
            .map(|c| c.timestamp)
            .collect();
        persisted.sort_by(f64::total_cmp);
        assert_eq!(
            persisted,
            vec![february_ms, march_ms],
            "both real months persist in {year}"
        );
        let chunks = e
            .run(
                backend
                    .load_all_chunks(
                        key.clone(),
                        CandleCacheChunkAccessOptions {
                            chunk_size: 100,
                            interval_seconds: 28. * 86400.,
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            chunks
                .chunks
                .values()
                .flat_map(|chunk| chunk.iter())
                .flatten()
                .count(),
            2,
            "distinct 28-day sparse slots in {year}"
        );
        let (empty, calls) = recording(vec![]);
        assert_eq!(
            e.run(make().fetch(request(
                &key,
                "1M",
                february_ms / 1000.,
                march_ms / 1000.,
                2.,
                april_ms,
                empty
            )))
            .unwrap(),
            vec![candle(february_ms), candle(march_ms)],
            "fresh cache round trip in {year}"
        );
        assert!(calls.borrow().is_empty(), "no exchange fallback in {year}");
    }
}

#[test]
fn twenty_pending_hydrations_never_exceed_cap_one() {
    struct Backend {
        calls: Rc<Cell<usize>>,
        keys: Rc<RefCell<Vec<String>>>,
        active: Rc<Cell<usize>>,
        peak: Rc<Cell<usize>>,
        permits: Rc<Cell<usize>>,
    }
    impl CandleCacheBackend for Backend {
        fn load_all_chunks(
            &self,
            key: String,
            _: CandleCacheChunkAccessOptions,
        ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
            self.keys.borrow_mut().push(key);
            self.calls.set(self.calls.get() + 1);
            self.active.set(self.active.get() + 1);
            self.peak.set(self.peak.get().max(self.active.get()));
            let active = self.active.clone();
            let permits = self.permits.clone();
            Some(Box::pin(poll_fn(move |_| {
                if permits.get() == 0 {
                    Poll::Pending
                } else {
                    permits.set(permits.get() - 1);
                    active.set(active.get() - 1);
                    Poll::Ready(Ok(LoadedChunks::default()))
                }
            })))
        }
        fn persist_chunks(
            &self,
            _: String,
            _: Chunks,
            _: CandleCacheChunkAccessOptions,
        ) -> LocalFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }
    let e = Executor::default();
    let calls = Rc::new(Cell::new(0));
    let keys = Rc::new(RefCell::new(Vec::new()));
    let active = Rc::new(Cell::new(0));
    let peak = Rc::new(Cell::new(0));
    let permits = Rc::new(Cell::new(0));
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(Rc::new(Backend {
                calls: calls.clone(),
                keys: keys.clone(),
                active: active.clone(),
                peak: peak.clone(),
                permits: permits.clone(),
            })),
            max_concurrent_hydrations: Some(1),
            ..Default::default()
        },
        e.runtime(),
    );
    let (empty, _) = recording(vec![]);
    let mut fetches: Vec<_> = (0..20)
        .map(|i| {
            let cache = cache.clone();
            let empty = empty.clone();
            Box::pin(async move {
                cache
                    .fetch(request(
                        &format!("cap-{i}"),
                        "1m",
                        base(),
                        base(),
                        1.,
                        base() + 60000.,
                        empty,
                    ))
                    .await
            })
        })
        .collect();
    for fetch in &mut fetches {
        assert!(
            fetch
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
    }
    e.tick();
    assert_eq!(calls.get(), 1, "nineteen reads must remain queued");
    assert_eq!(active.get(), 1);
    for completed in 1..=20 {
        permits.set(1);
        e.tick();
        e.tick();
        assert_eq!(
            calls.get(),
            (completed + 1).min(20),
            "one replacement after completion {completed}"
        );
        assert_eq!(active.get(), usize::from(completed != 20));
        assert_eq!(
            peak.get(),
            1,
            "cap holds while draining after completion {completed}"
        );
    }
    assert_eq!(
        *keys.borrow(),
        (0..20).map(|i| format!("cap-{i}")).collect::<Vec<_>>(),
        "FIFO backend start order"
    );
    for fetch in fetches {
        assert!(e.run(fetch).unwrap().is_empty());
    }
    assert_eq!(
        e.clock.get(),
        0.,
        "no timeout release substitutes for cap enforcement"
    );
}

#[test]
fn normalization_threshold_is_strict_and_milliseconds_are_floored() {
    for (input, expected) in [
        (1_699_999_800., 1_699_999_800.),
        (1_699_999_800.25, 1_699_999_800.25),
        (999_999_999_999., 999_999_999_999.),
        (1_000_000_000_000., 1_000_000_000_000.),
        (1_000_000_000_001., 1_000_000_000.),
        (1_000_000_000_999., 1_000_000_000.),
        (1_000_000_001_000., 1_000_000_001.),
    ] {
        assert_eq!(
            crate::calendar::normalize(input),
            expected,
            "units at {input}"
        );
    }
}

#[test]
fn seconds_stamped_venue_probe_persists_canonical_ms_and_reuses() {
    let e = Executor::default();
    let store = Rc::new(Store::default());
    let backend = Rc::new(KeyValueCandleCacheBackend::new(
        store.clone(),
        None,
        e.runtime(),
    ));
    let make = || {
        CandleCache::new(
            CandleCacheOptions {
                backend: Some(backend.clone()),
                backend_read_mode: Some(CandleCacheBackendReadMode::ReadThrough),
                ..Default::default()
            },
            e.runtime(),
        )
    };
    let cache = make();
    let seconds = vec![candle(1_699_999_800.), candle(1_699_999_860.)];
    let canonical_ms = vec![candle(1_699_999_800_000.), candle(1_699_999_860_000.)];
    let (fetcher, calls) = recording(seconds.clone());
    assert_eq!(
        e.run(cache.fetch(request(
            "seconds-venue",
            "1m",
            1_699_999_800.,
            1_699_999_860.,
            2.,
            1_699_999_980_000.,
            fetcher.clone()
        )))
        .unwrap(),
        seconds
    );
    assert_eq!(
        calls.borrow()[0],
        FetchOptions {
            interval: timeframe("1m"),
            from: 1_699_999_800_000.,
            to: 1_699_999_860_000.,
            limit: 2.
        }
    );
    let mut persisted: Vec<_> = store
        .values
        .borrow()
        .values()
        .flat_map(|raw| {
            serde_json::from_str::<CandleCacheChunkRecord>(raw)
                .unwrap()
                .candles
        })
        .collect();
    persisted.sort_by(|a, b| a.timestamp.total_cmp(&b.timestamp));
    assert_eq!(persisted, canonical_ms);
    assert_eq!(
        e.run(cache.fetch(request(
            "seconds-venue",
            "1m",
            1_699_999_800.,
            1_699_999_860.,
            2.,
            1_699_999_980_000.,
            fetcher
        )))
        .unwrap(),
        canonical_ms
    );
    assert_eq!(calls.borrow().len(), 1);
    let (empty, fresh_calls) = recording(vec![]);
    assert_eq!(
        e.run(make().fetch(request(
            "seconds-venue",
            "1m",
            1_699_999_800.,
            1_699_999_860.,
            2.,
            1_699_999_980_000.,
            empty
        )))
        .unwrap(),
        canonical_ms
    );
    assert!(fresh_calls.borrow().is_empty());
}

#[test]
fn hydration_errors_are_diagnosed_before_and_after_timeout_without_losing_fallback() {
    struct FailingBackend(Rc<Cell<bool>>);
    impl CandleCacheBackend for FailingBackend {
        fn load_all_chunks(
            &self,
            _: String,
            _: CandleCacheChunkAccessOptions,
        ) -> Option<LocalFuture<'static, Result<crate::LoadedChunks, String>>> {
            let ready = self.0.clone();
            Some(Box::pin(poll_fn(move |_| {
                if ready.get() {
                    Poll::Ready(Err("persisted key refused".into()))
                } else {
                    Poll::Pending
                }
            })))
        }
        fn persist_chunks(
            &self,
            _: String,
            _: Chunks,
            _: CandleCacheChunkAccessOptions,
        ) -> LocalFuture<'static, Result<(), String>> {
            Box::pin(async { Ok(()) })
        }
    }
    for late in [false, true] {
        let e = Executor::default();
        let ready = Rc::new(Cell::new(!late));
        let diagnostics = Rc::new(RefCell::new(Vec::new()));
        let observed = diagnostics.clone();
        let cache = CandleCache::new(
            CandleCacheOptions {
                backend: Some(Rc::new(FailingBackend(ready.clone()))),
                hydration_timeout_ms: Some(10.),
                max_concurrent_hydrations: Some(1),
                on_hydration_error: Some(Rc::new(move |key, error| {
                    observed.borrow_mut().push((key, error))
                })),
                ..Default::default()
            },
            e.runtime(),
        );
        let t = base();
        let expected = candle(t);
        let (fetch, calls) = recording(vec![expected.clone()]);
        let completion_order = Rc::new(RefCell::new(Vec::new()));
        let first_order = completion_order.clone();
        let queued_order = completion_order.clone();
        let first_fetch = fetch.clone();
        let first_cache = cache.clone();
        let mut first = Box::pin(async move {
            let result = first_cache
                .fetch(request("first", "5m", t, t, 1., t + 300000., first_fetch))
                .await;
            first_order.borrow_mut().push("first");
            result
        });
        let mut second = Box::pin(async move {
            let result = cache
                .fetch(request("queued", "5m", t, t, 1., t + 300000., fetch))
                .await;
            queued_order.borrow_mut().push("queued");
            result
        });
        let mut cx = Context::from_waker(Waker::noop());
        assert!(first.as_mut().poll(&mut cx).is_pending());
        assert!(second.as_mut().poll(&mut cx).is_pending());
        assert_eq!(e.tasks.borrow().len(), 1);
        e.tick();
        if late {
            assert!(diagnostics.borrow().is_empty());
            e.clock.set(10.);
            e.tick(); // timeout frees the slot and starts the queued key
            assert_eq!(
                first.as_mut().poll(&mut cx),
                Poll::Ready(Ok(vec![expected.clone()]))
            );
            ready.set(true);
        }
        e.tick();
        e.tick();
        if !late {
            assert_eq!(e.run(first).unwrap(), vec![expected.clone()]);
        }
        assert_eq!(e.run(second).unwrap(), vec![expected]);
        assert_eq!(calls.borrow().len(), 2);
        assert_eq!(
            *completion_order.borrow(),
            ["first", "queued"],
            "source caller settlement order"
        );
        let mut messages = diagnostics.borrow().clone();
        messages.sort();
        assert_eq!(
            messages,
            vec![
                ("first".into(), "persisted key refused".into()),
                ("queued".into(), "persisted key refused".into())
            ]
        );
    }
}

#[test]
fn null_stored_elements_keep_siblings_and_null_timestamps_are_rejected() {
    let executor = Executor::default();
    let store = Rc::new(Store::default());
    let backend = KeyValueCandleCacheBackend::new(store.clone(), None, executor.runtime());
    let options = CandleCacheChunkAccessOptions {
        chunk_size: 100,
        interval_seconds: 60.,
    };
    let healthy = candle(6_000_000.);
    store.values.borrow_mut().insert(
        "tealstreet-candles::null-proof::1".into(),
        serde_json::json!({"candles":[healthy]}).to_string(),
    );
    for malformed in [
        serde_json::Value::Null,
        serde_json::json!({"timestamp":null,"open":1,"high":2,"low":0,"close":1,"volume":1}),
    ] {
        store.values.borrow_mut().insert(
            "tealstreet-candles::null-proof::0".into(),
            serde_json::json!({"candles":[malformed]}).to_string(),
        );
        let result = executor
            .run(
                backend
                    .load_chunks("null-proof".into(), vec![0, 1], options)
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[&1][0], Some(healthy.clone()));
        assert!(!result.contains_key(&0));
    }
}

#[test]
#[ignore = "requires Node/TypeScript; run with --include-ignored"]
fn maintained_typescript_null_record_bugs() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/kv-null-oracle.mjs"
        ))
        .output()
        .expect("Node is required for the maintained source oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    for (mutation, assertion) in [
        ("--mutate-null-element", "Missing expected rejection"),
        (
            "--mutate-null-timestamp",
            "source null timestamp must retain both chunks",
        ),
    ] {
        let child = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/kv-null-oracle.mjs"
            ))
            .arg(mutation)
            .output()
            .expect("Node oracle child must execute");
        let diagnostic = String::from_utf8_lossy(&child.stderr);
        assert_eq!(child.status.code(), Some(1), "{mutation}: {diagnostic}");
        assert!(
            diagnostic.contains("AssertionError") && diagnostic.contains(assertion),
            "{mutation} must fail its behavioral assertion, not dependency loading: {diagnostic}"
        );
    }
}

#[test]
fn chunk_record_qualified_decoder_rejects_positional_arrays() {
    use serde::Deserialize;
    let mut input = serde_json::Deserializer::from_str("[[],0]");
    assert!(CandleCacheChunkRecord::deserialize(&mut input).is_err());
    let mut input = serde_json::Deserializer::from_str(r#"{"candles":[],"lastAccessed":0}"#);
    assert!(CandleCacheChunkRecord::deserialize(&mut input).is_ok());
}

#[test]
fn overflow_refusal_does_not_prevent_valid_sibling_hydration() {
    let e = Executor::default();
    let store = Rc::new(Store::default());
    let t = base();
    let index = (t / 60000.) as i64;
    let key = index.to_string();
    let valid = candle(t);
    for (suffix, data) in [
        (
            key.as_str(),
            serde_json::json!({"candles":[valid]}).to_string(),
        ),
        ("9223372036854775808", "ignored".into()),
        ("junk", "ignored".into()),
        ("\u{0085}12", "ignored".into()),
    ] {
        store
            .values
            .borrow_mut()
            .insert(format!("tealstreet-candles::k::{suffix}"), data);
    }
    let backend = Rc::new(KeyValueCandleCacheBackend::new(
        store.clone(),
        None,
        e.runtime(),
    ));
    let diagnostics = Rc::new(RefCell::new(Vec::new()));
    let observed = diagnostics.clone();
    let cache = CandleCache::new(
        CandleCacheOptions {
            backend: Some(backend.clone()),
            chunk_size: Some(1),
            on_hydration_error: Some(Rc::new(move |key, error| {
                observed.borrow_mut().push((key, error))
            })),
            ..Default::default()
        },
        e.runtime(),
    );
    let (fetch, calls) = recording(vec![]);
    let actual = e
        .run(cache.fetch(request("k", "1m", t, t, 1., t + 60000., fetch)))
        .unwrap();
    assert_eq!(actual, vec![valid.clone()]);
    assert!(
        calls.borrow().is_empty(),
        "valid cached sibling must avoid network fallback"
    );
    assert_eq!(
        *diagnostics.borrow(),
        vec![(
            "k".into(),
            "persisted chunk key outside i64: tealstreet-candles::k::9223372036854775808".into()
        )],
        "refused hydration keys must remain observable"
    );
    assert_eq!(store.lists.get(), 1);
    let loaded = e
        .run(
            backend
                .load_all_chunks(
                    "k".into(),
                    CandleCacheChunkAccessOptions {
                        chunk_size: 1,
                        interval_seconds: 60.,
                    },
                )
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        loaded.refused_keys,
        ["tealstreet-candles::k::9223372036854775808"]
    );
    assert_eq!(loaded.chunks[&index][0], Some(valid));
}

#[test]
fn invalid_backend_slot_arithmetic_never_becomes_saturated_cache_hit() {
    let executor = Executor::default();
    let store = Rc::new(Store::default());
    let healthy = candle(6_000_000.);
    for key in [0, i64::MAX, i64::MIN] {
        store.values.borrow_mut().insert(
            format!("tealstreet-candles::slots::{key}"),
            serde_json::json!({"candles":[healthy]}).to_string(),
        );
    }
    let backend = KeyValueCandleCacheBackend::new(store, None, executor.runtime());
    for (width, key) in [
        (f64::NAN, 0),
        (0., i64::MAX),
        (-0., i64::MIN),
        (1e-300, i64::MAX),
    ] {
        let actual = executor
            .run(
                backend
                    .load_chunks(
                        "slots".into(),
                        vec![key],
                        CandleCacheChunkAccessOptions {
                            chunk_size: 1,
                            interval_seconds: width,
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
        assert!(
            actual.is_empty(),
            "invalid derived index must never report a cache hit: width {width}, key {key}"
        );
    }
    for (width, key) in [(60., 100), (f64::INFINITY, 0)] {
        let store = Rc::new(Store::default());
        store.values.borrow_mut().insert(
            format!("tealstreet-candles::slots::{key}"),
            serde_json::json!({"candles":[healthy]}).to_string(),
        );
        let backend = KeyValueCandleCacheBackend::new(store, None, executor.runtime());
        let actual = executor
            .run(
                backend
                    .load_chunks(
                        "slots".into(),
                        vec![key],
                        CandleCacheChunkAccessOptions {
                            chunk_size: 1,
                            interval_seconds: width,
                        },
                    )
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(
            actual[&key][0],
            Some(healthy.clone()),
            "guard the derived index rather than rejecting every non-finite width"
        );
    }
}
