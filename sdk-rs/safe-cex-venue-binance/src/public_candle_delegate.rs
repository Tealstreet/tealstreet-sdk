use crate::delegated_methods::CandleOptionsHandle;

// ts: BinancePublicWebsocket.listenOHLCV
pub fn listen_futures_public_ohlcv<C, R, E>(
    opts: CandleOptionsHandle,
    callback: C,
    listen_market: impl FnOnce(CandleOptionsHandle, C) -> Result<R, E>,
) -> Result<R, E> {
    listen_market(opts, callback)
}

// ts: BinancePMPublicWebsocket.listenOHLCV
pub fn listen_pm_public_ohlcv<C, R, E>(
    opts: CandleOptionsHandle,
    callback: C,
    listen_market: impl FnOnce(CandleOptionsHandle, C) -> Result<R, E>,
) -> Result<R, E> {
    listen_market(opts, callback)
}
