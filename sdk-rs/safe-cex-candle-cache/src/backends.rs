use crate::types::{
    CacheRuntime, CandleCacheBackend, CandleCacheChunkAccessOptions, CandleCacheChunkRecord,
    CandleCacheKeyValueStore, CandleCacheSparseChunk, Chunks, LoadedChunks, LocalFuture,
    validate_chunk_size,
};
use std::{cell::RefCell, collections::BTreeMap, future::poll_fn, rc::Rc, task::Poll};

async fn all<T: 'static>(
    futures: Vec<LocalFuture<'static, Result<T, String>>>,
    runtime: CacheRuntime,
) -> Result<Vec<T>, String> {
    let mut futures: Vec<_> = futures.into_iter().map(Some).collect();
    let mut results: Vec<Option<T>> = (0..futures.len()).map(|_| None).collect();
    poll_fn(move |cx| {
        for (index, future) in futures.iter_mut().enumerate() {
            if results[index].is_none() {
                match future.as_mut().unwrap().as_mut().poll(cx) {
                    Poll::Ready(Ok(value)) => {
                        results[index] = Some(value);
                        *future = None;
                    }
                    Poll::Ready(Err(error)) => {
                        *future = None;
                        let mut pending: Vec<_> = futures
                            .iter_mut()
                            .filter_map(Option::take)
                            .map(Some)
                            .collect();
                        (runtime.spawn)(Box::pin(async move {
                            poll_fn(move |cx| {
                                for future in &mut pending {
                                    if let Some(task) = future
                                        && task.as_mut().poll(cx).is_ready()
                                    {
                                        *future = None;
                                    }
                                }
                                if pending.iter().all(Option::is_none) {
                                    Poll::Ready(())
                                } else {
                                    Poll::Pending
                                }
                            })
                            .await;
                        }));
                        return Poll::Ready(Err(error));
                    }
                    Poll::Pending => {}
                }
            }
        }
        if results.iter().all(Option::is_some) {
            Poll::Ready(Ok(results.iter_mut().map(|v| v.take().unwrap()).collect()))
        } else {
            Poll::Pending
        }
    })
    .await
}
#[derive(Default)]
pub struct MemoryCandleCacheBackend {
    chunks_by_cache_key: Rc<RefCell<BTreeMap<String, Chunks>>>,
}
impl MemoryCandleCacheBackend {
    pub fn load_all_chunks(
        &self,
        key: String,
        _: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        let chunks = self
            .chunks_by_cache_key
            .borrow()
            .get(&key)
            .cloned()
            .unwrap_or_default();
        Some(Box::pin(async move {
            Ok(LoadedChunks {
                chunks,
                refused_keys: Vec::new(),
            })
        }))
    }
    pub fn load_chunks(
        &self,
        key: String,
        keys: Vec<i64>,
        _: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        let chunks = self
            .chunks_by_cache_key
            .borrow()
            .get(&key)
            .map(|all| {
                keys.into_iter()
                    .filter_map(|key| all.get(&key).cloned().map(|v| (key, v)))
                    .collect()
            })
            .unwrap_or_default();
        Some(Box::pin(async move { Ok(chunks) }))
    }
    pub fn persist_chunks(
        &self,
        key: String,
        chunks: Chunks,
        _: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        self.chunks_by_cache_key
            .borrow_mut()
            .entry(key)
            .or_default()
            .extend(chunks);
        Box::pin(async { Ok(()) })
    }
    pub fn clear(&self, key: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        if let Some(key) = key.filter(|v| !v.is_empty()) {
            self.chunks_by_cache_key.borrow_mut().remove(&key);
        } else {
            self.chunks_by_cache_key.borrow_mut().clear();
        }
        Some(Box::pin(async { Ok(()) }))
    }
}
pub struct KeyValueCandleCacheBackend {
    store: Rc<dyn CandleCacheKeyValueStore>,
    namespace: String,
    runtime: CacheRuntime,
}
impl KeyValueCandleCacheBackend {
    pub fn new(
        store: Rc<dyn CandleCacheKeyValueStore>,
        namespace: Option<String>,
        runtime: CacheRuntime,
    ) -> Self {
        Self {
            store,
            namespace: namespace.unwrap_or_else(|| "tealstreet-candles".into()),
            runtime,
        }
    }
    fn get_chunk_prefix(&self, key: &str) -> String {
        format!("{}::{key}::", self.namespace)
    }
}
// parseInt accepts a numeric prefix even when the remainder is not numeric.
pub(crate) fn parse_chunk_key(value: &str) -> Result<Option<i64>, ()> {
    let value = safe_cex_num::js_trim_start(value);
    let length = value
        .char_indices()
        .take_while(|(i, c)| c.is_ascii_digit() || (*i == 0 && (*c == '-' || *c == '+')))
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());
    let prefix = &value[..length];
    // TS filters NaN and Infinity, including digit prefixes too large for binary64.
    if !prefix.parse::<f64>().is_ok_and(f64::is_finite) {
        return Ok(None);
    }
    prefix.parse::<i64>().map(Some).map_err(|_| ())
}
impl KeyValueCandleCacheBackend {
    pub fn load_all_chunks(
        &self,
        key: String,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        if let Err(error) = validate_chunk_size(options.chunk_size) {
            return Some(Box::pin(async move { Err(error) }));
        }
        let prefix = self.get_chunk_prefix(&key);
        let store = self.store.clone();
        let backend = Self {
            store: store.clone(),
            namespace: self.namespace.clone(),
            runtime: self.runtime.clone(),
        };
        Some(Box::pin(async move {
            let mut keys = Vec::new();
            let mut refused_keys = Vec::new();
            for listed in store.list_keys(prefix.clone()).await? {
                let Some(suffix) = listed.strip_prefix(&prefix) else {
                    continue;
                };
                match parse_chunk_key(suffix) {
                    Ok(Some(index)) => keys.push(index),
                    Ok(None) => {}
                    Err(()) => refused_keys.push(listed),
                }
            }
            let chunks = backend.load_chunks(key, keys, options).unwrap().await?;
            Ok(LoadedChunks {
                chunks,
                refused_keys,
            })
        }))
    }
    pub fn load_chunks(
        &self,
        key: String,
        keys: Vec<i64>,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        if let Err(error) = validate_chunk_size(options.chunk_size) {
            return Some(Box::pin(async move { Err(error) }));
        }
        // ts-bug: KeyValueCandleCacheBackend.loadChunks cannot index zero-width chunks.
        if options.chunk_size == 0 {
            return Some(Box::pin(async { Ok(Chunks::new()) }));
        }
        let prefix = self.get_chunk_prefix(&key);
        let futures = keys
            .into_iter()
            .map(|key| {
                let read = self.store.get(format!("{prefix}{key}"));
                Box::pin(async move {
                    let Some(raw) = read.await?.filter(|v| !v.is_empty()) else {
                        return Ok(None);
                    };
                    let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) else {
                        return Ok(None);
                    };
                    let Some(candles) = value.get("candles").and_then(|v| v.as_array()) else {
                        return Ok(None);
                    };
                    let mut sparse = vec![None; options.chunk_size];
                    let mut matching = false;
                    // ts-bug: KeyValueCandleCacheBackend.loadChunks null elements reject the entire TS load.
                    // CORRECTED: chosen typed candles skip that element and retain healthy siblings.
                    // ts-bug: KeyValueCandleCacheBackend.loadChunks null timestamps coerce to zero in TS.
                    // CORRECTED: the chosen typed timestamp rejects null; generic JSON could emulate it.
                    for candle in candles {
                        let Some(timestamp) = candle.get("timestamp").and_then(|v| v.as_f64())
                        else {
                            continue;
                        };
                        if !timestamp.is_finite() {
                            continue;
                        }
                        let index = (timestamp / 1000. / options.interval_seconds).floor();
                        if !index.is_finite() || index < i64::MIN as f64 || index >= i64::MAX as f64
                        {
                            continue;
                        }
                        let index = index as i64;
                        if index.div_euclid(options.chunk_size as i64) != key {
                            continue;
                        }
                        let offset = index.rem_euclid(options.chunk_size as i64) as usize;
                        let Some(candle) = crate::codec::decode_json_candle(candle) else {
                            continue;
                        };
                        sparse[offset] = Some(candle);
                        matching = true;
                    }
                    Ok(matching.then_some((key, sparse)))
                })
                    as LocalFuture<'static, Result<Option<(i64, CandleCacheSparseChunk)>, String>>
            })
            .collect();
        let runtime = self.runtime.clone();
        Some(Box::pin(async move {
            Ok(all(futures, runtime).await?.into_iter().flatten().collect())
        }))
    }
    pub fn persist_chunks(
        &self,
        key: String,
        chunks: Chunks,
        _: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        let prefix = self.get_chunk_prefix(&key);
        let mut writes = Vec::new();
        for (key, chunk) in chunks {
            let candles: Vec<_> = chunk.into_iter().flatten().collect();
            if candles.is_empty() {
                continue;
            }
            let record = CandleCacheChunkRecord {
                candles,
                last_accessed: (self.runtime.now_ms)(),
            };
            let value = match serde_json::to_string(&record) {
                Ok(v) => v,
                Err(e) => return Box::pin(async move { Err(e.to_string()) }),
            };
            writes.push(self.store.set(format!("{prefix}{key}"), value));
        }
        let runtime = self.runtime.clone();
        Box::pin(async move { all(writes, runtime).await.map(|_| ()) })
    }
    pub fn clear(&self, key: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        let prefix = key
            .filter(|v| !v.is_empty())
            .map(|v| self.get_chunk_prefix(&v))
            .unwrap_or_else(|| format!("{}::", self.namespace));
        let store = self.store.clone();
        let runtime = self.runtime.clone();
        Some(Box::pin(async move {
            let keys = store.list_keys(prefix).await?;
            all(
                keys.into_iter().map(|key| store.delete(key)).collect(),
                runtime,
            )
            .await
            .map(|_| ())
        }))
    }
}

impl CandleCacheBackend for MemoryCandleCacheBackend {
    fn load_all_chunks(
        &self,
        key: String,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        MemoryCandleCacheBackend::load_all_chunks(self, key, options)
    }
    fn load_chunks(
        &self,
        key: String,
        keys: Vec<i64>,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        MemoryCandleCacheBackend::load_chunks(self, key, keys, options)
    }
    fn persist_chunks(
        &self,
        key: String,
        chunks: Chunks,
        options: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        MemoryCandleCacheBackend::persist_chunks(self, key, chunks, options)
    }
    fn clear(&self, key: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        MemoryCandleCacheBackend::clear(self, key)
    }
}
impl CandleCacheBackend for KeyValueCandleCacheBackend {
    fn load_all_chunks(
        &self,
        key: String,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        KeyValueCandleCacheBackend::load_all_chunks(self, key, options)
    }
    fn load_chunks(
        &self,
        key: String,
        keys: Vec<i64>,
        options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        KeyValueCandleCacheBackend::load_chunks(self, key, keys, options)
    }
    fn persist_chunks(
        &self,
        key: String,
        chunks: Chunks,
        options: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>> {
        KeyValueCandleCacheBackend::persist_chunks(self, key, chunks, options)
    }
    fn clear(&self, key: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        KeyValueCandleCacheBackend::clear(self, key)
    }
}

#[cfg(test)]
mod language_tests {
    #[test]
    fn chunk_prefix_parsing_matches_js_bom_and_nel_boundaries() {
        let output = std::process::Command::new("node").args(["-e", r#"console.log(JSON.stringify(["\ufeff12tail","\u008512tail","\u001c12","  -12x","\ufeff+12"].map(s => {const n=parseInt(s,10);return Number.isFinite(n)?n:null})))"#]).output().expect("Node required for parseInt oracle");
        assert!(output.status.success());
        let expected: Vec<Option<i64>> = serde_json::from_slice(&output.stdout).unwrap();
        let actual: Vec<_> = [
            "\u{feff}12tail",
            "\u{0085}12tail",
            "\u{001c}12",
            "  -12x",
            "\u{feff}+12",
        ]
        .map(|value| super::parse_chunk_key(value).expect("fixture within i64"))
        .into();
        assert_eq!(actual, expected);
    }
    #[test]
    fn listed_key_source_filters_nonfinite_but_trusts_prefix() {
        let script = r#"
const fs=require('node:fs'), assert=require('node:assert/strict');
const source=fs.readFileSync(process.argv[1],'utf8');
const expression=source.match(/const chunkKeys = keys([\s\S]*?);/)[1];
const prefix='tealstreet-candles::k::';
const keys=[prefix+'junk',prefix+'9'.repeat(400),prefix+'10tail','other::9223372036854775808'];
console.log(JSON.stringify(Function('keys','prefix','return keys'+expression)(keys,prefix)));
"#;
        let output = std::process::Command::new("node").args(["-e", script,
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../packages/safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend.ts")]).output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<Vec<i64>>(&output.stdout).unwrap(),
            [10, 808]
        );
        assert_eq!(super::parse_chunk_key(&"9".repeat(400)), Ok(None));
        assert_eq!(super::parse_chunk_key("9223372036854775808"), Err(()));
        assert_eq!(
            super::parse_chunk_key("9007199254740993"),
            Ok(Some(9007199254740993))
        );
    }
}
