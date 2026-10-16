use safe_cex_types::{AnyTimeframe, Candle};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, future::Future, pin::Pin, rc::Rc};

pub type LocalFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;
pub type CandleCacheSparseChunk = Vec<Option<Candle>>;
pub type Chunks = BTreeMap<i64, CandleCacheSparseChunk>;
/// Successful listing keeps usable chunks alongside finite numeric keys outside i64.
#[derive(Debug, Default)]
pub struct LoadedChunks {
    pub chunks: Chunks,
    pub refused_keys: Vec<String>,
}
#[derive(Clone, Copy)]
pub struct CandleCacheChunkAccessOptions {
    pub chunk_size: usize,
    pub interval_seconds: f64,
}
pub trait CandleCacheBackend {
    fn load_all_chunks(
        &self,
        _key: String,
        _options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<LoadedChunks, String>>> {
        None
    }
    fn load_chunks(
        &self,
        _key: String,
        _keys: Vec<i64>,
        _options: CandleCacheChunkAccessOptions,
    ) -> Option<LocalFuture<'static, Result<Chunks, String>>> {
        None
    }
    fn persist_chunks(
        &self,
        key: String,
        chunks: Chunks,
        options: CandleCacheChunkAccessOptions,
    ) -> LocalFuture<'static, Result<(), String>>;
    fn clear(&self, _key: Option<String>) -> Option<LocalFuture<'static, Result<(), String>>> {
        None
    }
}
#[derive(Clone, Copy, Default, PartialEq)]
pub enum CandleCacheBackendReadMode {
    #[default]
    HydrateOnce,
    ReadThrough,
}
#[derive(Clone, Debug, PartialEq)]
pub struct FetchOptions {
    pub interval: AnyTimeframe,
    pub from: f64,
    pub to: f64,
    pub limit: f64,
}
pub type Fetcher = Rc<dyn Fn(FetchOptions) -> LocalFuture<'static, Result<Vec<Candle>, String>>>;
pub struct CandleCacheFetchRequest {
    pub cache_key: String,
    pub interval: AnyTimeframe,
    pub from_ms: f64,
    pub to_ms: f64,
    pub limit: f64,
    pub fetcher: Fetcher,
    pub now_ms: Option<f64>,
    pub use_persistent_backend: Option<bool>,
}
pub trait ExchangeCandleSource {
    fn fetch_ohlcv_in_worker(&self) -> bool;
    fn fetch_ohlcv(
        &self,
        symbol: String,
        options: FetchOptions,
    ) -> LocalFuture<'static, Result<Vec<Candle>, String>>;
    fn call_method_in_worker(
        &self,
        symbol: String,
        options: FetchOptions,
    ) -> LocalFuture<'static, Result<Vec<Candle>, String>>;
}
pub struct CandleCacheExchangeFetchRequest {
    pub cache_key: String,
    pub exchange: Rc<dyn ExchangeCandleSource>,
    pub symbol: String,
    pub use_worker: Option<bool>,
    pub interval: AnyTimeframe,
    pub from_ms: f64,
    pub to_ms: f64,
    pub limit: f64,
    pub now_ms: Option<f64>,
    pub use_persistent_backend: Option<bool>,
}
#[derive(Debug)]
pub struct CandleCacheRangeLookup {
    pub contiguous_forward_candles: Vec<Candle>,
    pub contiguous_behind_candles: Vec<Candle>,
    pub limit: f64,
    pub from: f64,
    pub to: f64,
}
#[derive(Clone)]
pub struct CacheRuntime {
    pub now_ms: Rc<dyn Fn() -> f64>,
    pub sleep: Rc<dyn Fn(f64) -> LocalFuture<'static, ()>>,
    pub spawn: Rc<dyn Fn(LocalFuture<'static, ()>)>,
}
#[derive(Default)]
pub struct CandleCacheOptions {
    pub backend: Option<Rc<dyn CandleCacheBackend>>,
    pub backend_read_mode: Option<CandleCacheBackendReadMode>,
    pub chunk_size: Option<usize>,
    pub hydration_timeout_ms: Option<f64>,
    pub max_concurrent_hydrations: Option<usize>,
    pub on_hydration_timeout: Option<Rc<dyn Fn(String)>>,
    /// Observe failed background reads without failing a successful network fallback.
    /// With no callback, errors are emitted to stderr (native) or console.error (wasm).
    pub on_hydration_error: Option<Rc<dyn Fn(String, String)>>,
}
pub trait CandleCacheKeyValueStore {
    fn get(&self, key: String) -> LocalFuture<'static, Result<Option<String>, String>>;
    fn set(&self, key: String, value: String) -> LocalFuture<'static, Result<(), String>>;
    fn delete(&self, key: String) -> LocalFuture<'static, Result<(), String>>;
    fn list_keys(&self, prefix: String) -> LocalFuture<'static, Result<Vec<String>, String>>;
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CandleCacheChunkRecord {
    pub candles: Vec<Candle>,
    pub last_accessed: f64,
}

// Private wire companion: no permissive public inherent decoder is generated.
#[derive(Deserialize)]
#[serde(remote = "CandleCacheChunkRecord", rename_all = "camelCase")]
struct ChunkRecordWire {
    candles: Vec<Candle>,
    last_accessed: f64,
}
impl<'de> Deserialize<'de> for CandleCacheChunkRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                ChunkRecordWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for CandleCacheChunkRecord {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { candles, last_accessed });
    }
}

// A bounded dense sparse chunk prevents public options from triggering multi-GB allocations.
pub(crate) const MAX_CHUNK_SIZE: usize = 16_384;
pub(crate) fn validate_chunk_size(size: usize) -> Result<(), String> {
    if size > MAX_CHUNK_SIZE {
        Err(format!(
            "chunk_size exceeds supported maximum {MAX_CHUNK_SIZE}"
        ))
    } else {
        Ok(())
    }
}
