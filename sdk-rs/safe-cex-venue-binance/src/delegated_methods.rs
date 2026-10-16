use safe_cex_types::OHLCVOptions;
use std::{cell::RefCell, future::Future, rc::Rc};

/// Stable decoded options object, forwarded without cloning or rewriting fields.
pub type CandleOptionsHandle = Rc<RefCell<OHLCVOptions>>;

// ts: BinancePMExchange.listenOHLCV
/// Configured publicWebsocket method; preserve callback and returned cleanup/error.
pub fn listen_binance_pm_ohlcv<C, R, E>(
    opts: CandleOptionsHandle,
    callback: C,
    listen: impl FnOnce(CandleOptionsHandle, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(opts, callback)
}

// ts: BinanceSpotExchange.listenOHLCV
pub fn listen_binance_spot_ohlcv<C, R, E>(
    opts: CandleOptionsHandle,
    callback: C,
    listen: impl FnOnce(CandleOptionsHandle, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(opts, callback)
}

// ts: BinanceExchange.listenOrderBook
/// Provider presence is resolved by the configured class boundary. Its errors
/// propagate directly; there is no fallback after a provider throws.
pub fn listen_binance_order_book<C, R, E>(
    symbol: &str,
    callback: C,
    has_external_provider: bool,
    subscribe_external: impl FnOnce(&str, C) -> Result<R, E>,
    listen_public: impl FnOnce(&str, C) -> Result<R, E>,
) -> Result<R, E> {
    if has_external_provider {
        return subscribe_external(symbol, callback);
    }
    listen_public(symbol, callback)
}

// ts: BinancePMExchange.listenOrderBook
pub fn listen_binance_pm_order_book<C, R, E>(
    symbol: &str,
    callback: C,
    listen: impl FnOnce(&str, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(symbol, callback)
}

// ts: BinanceSpotExchange.listenOrderBook
pub fn listen_binance_spot_order_book<C, R, E>(
    symbol: &str,
    callback: C,
    listen: impl FnOnce(&str, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(symbol, callback)
}

// ts: BinanceExchange.fetchHistoricalPositions
/// Configured foldClosedPositionRecords; forward options intact and require
/// symbolScoped=true. The configured boundary models sync throw separately from
/// future rejection; both original errors escape unchanged. Concrete class,
/// websocket/provider/journal bindings and arrow receiver wiring remain pending.
pub async fn fetch_binance_historical_positions<O, T, E, F>(
    opts: O,
    fold: impl FnOnce(O, bool) -> Result<F, E>,
) -> Result<T, E>
where
    F: Future<Output = Result<T, E>>,
{
    fold(opts, true)?.await
}
