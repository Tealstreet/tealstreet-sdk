//! Storage parity port. Behavioral reference: packages/safe-cex/src/candleCache/CandleCache.ts.
mod backends;
mod cache;
pub mod calendar;
mod codec;
mod types;
pub use backends::{KeyValueCandleCacheBackend, MemoryCandleCacheBackend};
pub use cache::{
    CandleCache, clean_candles, create_exchange_candle_fetcher, fetch_ohlcv_with_exchange,
    get_candle_cache_key,
};
pub use types::*;
#[cfg(test)]
mod tests;

/// Interval arguments require the existing finite SDK union.
/// ```compile_fail
/// safe_cex_candle_cache::get_candle_cache_key(&safe_cex_types::ExchangeName::Binance, "BTCUSDT", "8h", None);
/// ```
#[cfg(doctest)]
struct IntervalGuardProof;

/// Native cache ownership is local; see CLAUDE.md for P3b adapter/migration costs.
/// ```compile_fail
/// fn needs_send<T: Send>() {}
/// needs_send::<safe_cex_candle_cache::CandleCache>();
/// ```
#[cfg(doctest)]
struct NativeLocalProof;

/// Exchange names require the finite SDK union, so a testnet prefix cannot be forged.
/// ```compile_fail
/// use safe_cex_types::{AnyTimeframe, Timeframe};
/// safe_cex_candle_cache::get_candle_cache_key(
///     "binance-testnet", "BTCUSDT", &AnyTimeframe::Variant1(Timeframe::M1), Some(false));
/// ```
#[cfg(doctest)]
struct ExchangeGuardProof;

pub mod async_storage;
#[cfg(target_arch = "wasm32")]
pub mod indexed_db;
#[cfg(not(target_arch = "wasm32"))]
pub mod native;
