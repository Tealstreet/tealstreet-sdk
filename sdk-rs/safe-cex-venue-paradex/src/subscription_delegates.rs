//! Forward existing websocket capabilities without projecting host values.

// ts: ParadexExchange.listenOrderBook
pub fn listen_order_book<S, C, R, E>(
    symbol: S,
    callback: C,
    listen: impl FnOnce(S, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(symbol, callback)
}

// ts: ParadexExchange.listenOHLCV
pub fn listen_ohlcv<O, C, R, E>(
    opts: O,
    callback: C,
    listen: impl FnOnce(O, C) -> Result<R, E>,
) -> Result<R, E> {
    listen(opts, callback)
}
