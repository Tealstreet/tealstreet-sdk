use crate::{
    calendar::{calendar, close, count, month, name, next, normalize, seconds, start},
    types::{
        CacheRuntime, CandleCacheBackendReadMode, CandleCacheChunkAccessOptions,
        CandleCacheExchangeFetchRequest, CandleCacheFetchRequest, CandleCacheOptions,
        CandleCacheRangeLookup, Chunks, ExchangeCandleSource, FetchOptions, Fetcher,
        validate_chunk_size,
    },
};
use safe_cex_types::{AnyTimeframe, Candle, ExchangeName};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashSet, VecDeque},
    future::poll_fn,
    rc::Rc,
    task::{Poll, Waker},
};

pub fn clean_candles(candles: Vec<Candle>) -> Vec<Candle> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for candle in candles {
        let timestamp = if candle.timestamp == 0. {
            0
        } else if candle.timestamp.is_nan() {
            f64::NAN.to_bits()
        } else {
            candle.timestamp.to_bits()
        };
        if seen.insert(timestamp) {
            unique.push(candle);
        }
    }
    let sorted = safe_cex_num::sorting::stable_sort::<_, std::convert::Infallible>(
        &mut unique,
        |left, right| {
            Ok((left.timestamp - right.timestamp)
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal))
        },
    );
    match sorted {
        Ok(()) => {}
        Err(never) => match never {},
    }
    unique
}
/// Testnet is a per-account flag, not a different exchange name: omitting it
/// mixes live and testnet history. `None` and `Some(false)` retain historical
/// live keys; `Some(true)` isolates every venue's testnet series.
/// Calendar `1w`/`1M` keys retain `-cal` to exclude old boundaries/slot widths
/// and formerly cached forming candles. Shared rationale: TS `getCandleCacheKey`
/// in `packages/safe-cex/src/candleCache/CandleCache.ts`.
pub fn get_candle_cache_key(
    exchange: &ExchangeName,
    symbol: &str,
    interval: &AnyTimeframe,
    testnet: Option<bool>,
) -> String {
    let exchange_value =
        serde_json::to_value(exchange).expect("ExchangeName is a serializable unit enum");
    let exchange = exchange_value
        .as_str()
        .expect("ExchangeName serializes to its TS wire name");
    let interval_name = name(interval);
    format!(
        "{exchange}{}-{symbol}-{interval_name}{}",
        if testnet == Some(true) {
            "-testnet"
        } else {
            ""
        },
        if calendar(interval) { "-cal" } else { "" }
    )
}
// ts-bug: slotSecondsFor("1W") reads a missing duration; the typed union rejects this alias.
fn slot_seconds_for(interval: &AnyTimeframe) -> f64 {
    if month(interval) {
        28. * 86400.
    } else {
        seconds(interval)
    }
}
pub async fn fetch_ohlcv_with_exchange(
    exchange: Rc<dyn ExchangeCandleSource>,
    symbol: String,
    use_worker: Option<bool>,
    options: FetchOptions,
) -> Result<Vec<Candle>, String> {
    if use_worker.unwrap_or_else(|| exchange.fetch_ohlcv_in_worker()) {
        exchange.call_method_in_worker(symbol, options).await
    } else {
        exchange.fetch_ohlcv(symbol, options).await
    }
}
pub fn create_exchange_candle_fetcher(
    exchange: Rc<dyn ExchangeCandleSource>,
    symbol: String,
    use_worker: Option<bool>,
) -> Fetcher {
    Rc::new(move |options| {
        Box::pin(fetch_ohlcv_with_exchange(
            exchange.clone(),
            symbol.clone(),
            use_worker,
            options,
        ))
    })
}
struct HydrationJob {
    key: String,
    interval: AnyTimeframe,
    completion: Rc<RefCell<Signal>>,
}
#[derive(Default)]
struct State {
    chunked_candle_cache: BTreeMap<String, Chunks>,
    hydration_queue: VecDeque<HydrationJob>,
    hydrated_cache_keys: BTreeSet<String>,
    hydration_running: usize,
    draining_hydrations: bool,
    cache_version: u64,
}
struct HydrationDrain(Rc<RefCell<State>>);
impl Drop for HydrationDrain {
    fn drop(&mut self) {
        self.0.borrow_mut().draining_hydrations = false;
    }
}
#[derive(Default)]
struct Signal {
    resolved: bool,
    wakers: Vec<Waker>,
}
fn resolve(signal: &Rc<RefCell<Signal>>) {
    let wakers = {
        let mut s = signal.borrow_mut();
        s.resolved = true;
        std::mem::take(&mut s.wakers)
    };
    for waker in wakers {
        waker.wake();
    }
}
struct HydrationSlot {
    cache: Option<CandleCache>,
    completion: Rc<RefCell<Signal>>,
}
impl HydrationSlot {
    fn finish(&mut self) {
        if let Some(cache) = self.cache.take() {
            cache.state.borrow_mut().hydration_running -= 1;
            resolve(&self.completion);
            cache.drain_hydration_queue();
        }
    }
}
impl Drop for HydrationSlot {
    fn drop(&mut self) {
        self.finish();
    }
}
#[derive(Clone)]
pub struct CandleCache {
    options: Rc<CandleCacheOptions>,
    runtime: CacheRuntime,
    state: Rc<RefCell<State>>,
    chunk_size: usize,
    hydration_timeout_ms: f64,
    max_concurrent_hydrations: usize,
    backend_read_mode: CandleCacheBackendReadMode,
}
impl CandleCache {
    pub fn new(options: CandleCacheOptions, runtime: CacheRuntime) -> Self {
        Self {
            chunk_size: options.chunk_size.unwrap_or(100),
            hydration_timeout_ms: options.hydration_timeout_ms.unwrap_or(2000.),
            max_concurrent_hydrations: options.max_concurrent_hydrations.unwrap_or(2),
            backend_read_mode: options.backend_read_mode.unwrap_or_default(),
            options: Rc::new(options),
            runtime,
            state: Rc::new(RefCell::new(State::default())),
        }
    }
    pub fn get_version(&self) -> u64 {
        self.state.borrow().cache_version
    }
    pub fn reset(&self, clear_persisted: Option<bool>, cache_key: Option<String>) {
        let key = cache_key.filter(|k| !k.is_empty());
        {
            let mut s = self.state.borrow_mut();
            if let Some(key) = &key {
                s.chunked_candle_cache.remove(key);
                s.hydrated_cache_keys.remove(key);
            } else {
                s.chunked_candle_cache.clear();
                s.hydrated_cache_keys.clear();
            }
            s.cache_version += 1;
        }
        if clear_persisted == Some(true)
            && let Some(backend) = &self.options.backend
            && let Some(clear) = backend.clear(key)
        {
            (self.runtime.spawn)(Box::pin(async move {
                let _ = clear.await;
            }));
        }
    }
    pub fn get_cached_candles(
        &self,
        key: &str,
        interval: &AnyTimeframe,
        from_ms: f64,
        to_ms: f64,
        limit: f64,
        now_ms: Option<f64>,
    ) -> CandleCacheRangeLookup {
        let range = self.normalize_request_range(interval, from_ms, to_ms, limit, now_ms);
        if self.chunk_size == 0 || validate_chunk_size(self.chunk_size).is_err() {
            return range;
        }
        let mut from = range.from;
        let mut to = range.to;
        let mut limit = range.limit;
        let mut behind = Vec::new();
        let mut forward = Vec::new();
        let state = self.state.borrow();
        let Some(chunks) = state
            .chunked_candle_cache
            .get(key)
            .filter(|c| !c.is_empty())
        else {
            return range;
        };
        let lookup = |ms| {
            let (key, offset) = self.get_chunk_info(ms / 1000., slot_seconds_for(interval))?;
            chunks
                .get(&key)
                .and_then(|c| c.get(offset))
                .and_then(Option::as_ref)
                .cloned()
        };
        while from <= to && limit > 0. {
            let Some(candle) = lookup(from) else {
                break;
            };
            behind.push(candle);
            from = next(interval, from / 1000.) * 1000.;
            limit -= 1.;
        }
        if from <= to && limit > 0. {
            while to >= from && limit > 0. {
                let Some(candle) = lookup(to) else {
                    break;
                };
                forward.push(candle);
                to = start(interval, to / 1000. - 1.) * 1000.;
                limit -= 1.;
            }
            forward.reverse();
        }
        CandleCacheRangeLookup {
            contiguous_forward_candles: forward,
            contiguous_behind_candles: behind,
            limit,
            from,
            to,
        }
    }
    pub async fn fetch(&self, request: CandleCacheFetchRequest) -> Result<Vec<Candle>, String> {
        validate_chunk_size(self.chunk_size)?;
        let now = request.now_ms.unwrap_or_else(|| (self.runtime.now_ms)());
        let persistent = request.use_persistent_backend.unwrap_or(true);
        let range = self.normalize_request_range(
            &request.interval,
            request.from_ms,
            request.to_ms,
            request.limit,
            Some(now),
        );
        // ts-bug: CandleCache.constructor accepts zero, producing unusable JS slots.
        if self.chunk_size == 0 {
            if range.limit <= 0. || range.from > range.to {
                return Ok(Vec::new());
            }
            return (request.fetcher)(FetchOptions {
                interval: request.interval,
                from: range.from,
                to: range.to,
                limit: range.limit,
            })
            .await
            .map(clean_candles);
        }
        if persistent && self.options.backend.is_some() {
            self.ensure_backend_state_for_request(
                &request.cache_key,
                &request.interval,
                range.from,
                range.to,
            )
            .await?;
        }
        let lookup = self.get_cached_candles(
            &request.cache_key,
            &request.interval,
            request.from_ms,
            request.to_ms,
            request.limit,
            Some(now),
        );
        let new = if lookup.limit <= 0. || lookup.from > lookup.to {
            Vec::new()
        } else {
            (request.fetcher)(FetchOptions {
                interval: request.interval.clone(),
                from: lookup.from,
                to: lookup.to,
                limit: lookup.limit,
            })
            .await?
        };
        let mut touched = Chunks::new();
        if !new.is_empty() {
            let mut state = self.state.borrow_mut();
            let chunks = state
                .chunked_candle_cache
                .entry(request.cache_key.clone())
                .or_default();
            for candle in &new {
                let t = normalize(candle.timestamp);
                if close(&request.interval, t) > now / 1000. {
                    continue;
                }
                let Some((key, offset)) =
                    self.get_chunk_info(t, slot_seconds_for(&request.interval))
                else {
                    continue;
                };
                let chunk = chunks
                    .entry(key)
                    .or_insert_with(|| vec![None; self.chunk_size]);
                if offset >= chunk.len() {
                    chunk.resize(offset + 1, None);
                }
                let mut candle = candle.clone();
                candle.timestamp = t * 1000.;
                chunk[offset] = Some(candle);
                touched.insert(key, chunk.clone());
            }
        }
        if persistent
            && !touched.is_empty()
            && let Some(backend) = &self.options.backend
        {
            let persist = backend.persist_chunks(
                request.cache_key,
                touched,
                self.access_options(&request.interval),
            );
            (self.runtime.spawn)(Box::pin(async move {
                let _ = persist.await;
            }));
        }
        Ok(clean_candles(
            lookup
                .contiguous_behind_candles
                .into_iter()
                .chain(new)
                .chain(lookup.contiguous_forward_candles)
                .collect(),
        ))
    }
    pub async fn fetch_with_exchange(
        &self,
        request: CandleCacheExchangeFetchRequest,
    ) -> Result<Vec<Candle>, String> {
        self.fetch(CandleCacheFetchRequest {
            cache_key: request.cache_key,
            interval: request.interval,
            from_ms: request.from_ms,
            to_ms: request.to_ms,
            limit: request.limit,
            now_ms: request.now_ms,
            use_persistent_backend: request.use_persistent_backend,
            fetcher: create_exchange_candle_fetcher(
                request.exchange,
                request.symbol,
                request.use_worker,
            ),
        })
        .await
    }
    fn get_chunk_info(&self, t: f64, interval: f64) -> Option<(i64, usize)> {
        if !t.is_finite() || !interval.is_finite() || interval <= 0. || self.chunk_size == 0 {
            return None;
        }
        let index = (t / interval).floor();
        if !index.is_finite() || index < i64::MIN as f64 || index >= i64::MAX as f64 {
            return None;
        }
        let index = index as i64;
        Some((
            index.div_euclid(self.chunk_size as i64),
            // Explicit divergence: pre-epoch candles use real slots, not JS negative array properties.
            index.rem_euclid(self.chunk_size as i64) as usize,
        ))
    }
    fn normalize_request_range(
        &self,
        interval: &AnyTimeframe,
        from_ms: f64,
        to_ms: f64,
        mut limit: f64,
        now_ms: Option<f64>,
    ) -> CandleCacheRangeLookup {
        let from = start(interval, normalize(from_ms)) * 1000.;
        let mut to = start(interval, normalize(to_ms));
        let max_to = start(
            interval,
            now_ms.unwrap_or_else(|| (self.runtime.now_ms)()) / 1000.,
        );
        if to > max_to {
            limit -= count(interval, max_to, to) - 1.;
            to = max_to;
        }
        CandleCacheRangeLookup {
            contiguous_forward_candles: Vec::new(),
            contiguous_behind_candles: Vec::new(),
            from,
            to: to * 1000.,
            limit,
        }
    }
    fn get_chunk_keys_for_range(&self, from: f64, to: f64, interval: f64) -> Vec<i64> {
        if from > to {
            return Vec::new();
        }
        let Some((first, _)) = self.get_chunk_info(from / 1000., interval) else {
            return Vec::new();
        };
        let Some((last, _)) = self.get_chunk_info(to / 1000., interval) else {
            return Vec::new();
        };
        (first..=last).collect()
    }
    fn merge_chunks(&self, key: &str, chunks: Chunks, existing_candles_win: bool) {
        if chunks.is_empty() {
            return;
        }
        let mut state = self.state.borrow_mut();
        let existing = state
            .chunked_candle_cache
            .entry(key.to_owned())
            .or_default();
        for (key, chunk) in chunks {
            if existing_candles_win && let Some(current) = existing.get_mut(&key) {
                for (offset, candle) in chunk.into_iter().enumerate() {
                    if current.len() <= offset {
                        current.resize(offset + 1, None);
                    }
                    if current[offset].is_none() {
                        current[offset] = candle;
                    }
                }
            } else {
                existing.insert(key, chunk);
            }
        }
    }
    fn access_options(&self, interval: &AnyTimeframe) -> CandleCacheChunkAccessOptions {
        CandleCacheChunkAccessOptions {
            chunk_size: self.chunk_size,
            interval_seconds: slot_seconds_for(interval),
        }
    }
    async fn ensure_backend_state_for_request(
        &self,
        key: &str,
        interval: &AnyTimeframe,
        from: f64,
        to: f64,
    ) -> Result<(), String> {
        if self.options.backend.is_none() {
            return Ok(());
        }
        if self.backend_read_mode == CandleCacheBackendReadMode::ReadThrough {
            let keys = self.get_chunk_keys_for_range(from, to, slot_seconds_for(interval));
            if keys.is_empty() {
                return Ok(());
            }
            let chunks = self.load_chunks_from_backend(key, keys, interval).await?;
            self.merge_chunks(key, chunks, false);
        } else {
            self.queued_hydrate(key, interval).await;
        }
        Ok(())
    }
    async fn load_chunks_from_backend(
        &self,
        key: &str,
        keys: Vec<i64>,
        interval: &AnyTimeframe,
    ) -> Result<Chunks, String> {
        let Some(backend) = &self.options.backend else {
            return Ok(Chunks::new());
        };
        if keys.is_empty() {
            return Ok(Chunks::new());
        }
        let options = self.access_options(interval);
        if let Some(load) = backend.load_chunks(key.into(), keys.clone(), options) {
            return load.await;
        }
        if let Some(load) = backend.load_all_chunks(key.into(), options) {
            let loaded = load.await?;
            for refused in loaded.refused_keys {
                self.report_hydration_error(
                    key,
                    format!("persisted chunk key outside i64: {refused}"),
                );
            }
            let mut all = loaded.chunks;
            return Ok(keys
                .into_iter()
                .filter_map(|key| all.remove(&key).map(|v| (key, v)))
                .collect());
        }
        Ok(Chunks::new())
    }
    async fn hydrate_from_backend(&self, key: &str, interval: &AnyTimeframe) -> Result<(), String> {
        let Some(backend) = &self.options.backend else {
            return Ok(());
        };
        if self.state.borrow().hydrated_cache_keys.contains(key) {
            return Ok(());
        }
        let Some(load) = backend.load_all_chunks(key.into(), self.access_options(interval)) else {
            return Ok(());
        };
        let loaded = load.await?;
        for refused in loaded.refused_keys {
            self.report_hydration_error(key, format!("persisted chunk key outside i64: {refused}"));
        }
        self.merge_chunks(key, loaded.chunks, true);
        self.state
            .borrow_mut()
            .hydrated_cache_keys
            .insert(key.into());
        Ok(())
    }
    fn drain_hydration_queue(&self) {
        if std::mem::replace(&mut self.state.borrow_mut().draining_hydrations, true) {
            return;
        }
        let _drain = HydrationDrain(self.state.clone());
        loop {
            let job = {
                let mut state = self.state.borrow_mut();
                if state.hydration_running >= self.max_concurrent_hydrations {
                    return;
                }
                state.hydration_queue.pop_front()
            };
            match job {
                Some(job) => self.start_hydration(job),
                None => return,
            }
        }
    }
    fn start_hydration(&self, job: HydrationJob) {
        let cache = self.clone();
        let HydrationJob {
            key,
            interval,
            completion,
        } = job;
        cache.state.borrow_mut().hydration_running += 1;
        // Own cleanup before spawning so even an unpolled discarded task releases its slot.
        let mut slot = HydrationSlot {
            cache: Some(cache.clone()),
            completion,
        };
        let spawner = cache.runtime.spawn.clone();
        spawner(Box::pin(async move {
            let mut hydration = Box::pin(cache.hydrate_from_backend(&key, &interval));
            let mut timer = (cache.runtime.sleep)(cache.hydration_timeout_ms);
            let completed = poll_fn(|cx| {
                if let Poll::Ready(result) = hydration.as_mut().poll(cx) {
                    return Poll::Ready(Some(result));
                }
                if timer.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(None);
                }
                Poll::Pending
            })
            .await;
            if let Some(Err(error)) = &completed {
                cache.report_hydration_error(&key, error.clone());
            }
            if completed.is_none()
                && let Some(callback) = &cache.options.on_hydration_timeout
            {
                callback(key.clone());
            }
            slot.finish();
            // A timed-out hydration remains alive and fills only remaining gaps.
            if completed.is_none()
                && let Err(error) = hydration.await
            {
                cache.report_hydration_error(&key, error);
            }
        }));
    }
    fn report_hydration_error(&self, key: &str, error: String) {
        // ts-bug: queuedHydrate.catch silently discards initial and late hydration errors.
        // Preserve network fallback, but never abandon a failed read without a diagnostic.
        if let Some(callback) = &self.options.on_hydration_error {
            callback(key.to_owned(), error);
        } else {
            let message = format!("Candle cache hydration failed for {key}: {error}");
            #[cfg(not(target_arch = "wasm32"))]
            eprintln!("{message}");
            #[cfg(target_arch = "wasm32")]
            web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&message));
        }
    }
    async fn queued_hydrate(&self, key: &str, interval: &AnyTimeframe) {
        // ts-bug: CandleCache.queuedHydrate: memory maps suppress timeout retries even without closed candles.
        if self.options.backend.is_none()
            || self.state.borrow().chunked_candle_cache.contains_key(key)
        {
            return;
        }
        let signal = Rc::new(RefCell::new(Signal::default()));
        let job = HydrationJob {
            key: key.to_owned(),
            interval: interval.clone(),
            completion: signal.clone(),
        };
        if self.state.borrow().hydration_running < self.max_concurrent_hydrations {
            self.start_hydration(job);
        } else {
            self.state.borrow_mut().hydration_queue.push_back(job);
        }
        poll_fn(|cx| {
            let mut signal = signal.borrow_mut();
            if signal.resolved {
                Poll::Ready(())
            } else {
                signal.wakers.push(cx.waker().clone());
                Poll::Pending
            }
        })
        .await;
    }
}
