//! Exchange subscription delegates; the caller supplies its existing public socket.
use safe_cex_exchange::{
    external::ExternalPublicData,
    runtime::{ExchangeError, ListenerCleanup},
};
use safe_cex_types::{Candle, OHLCVOptions, OrderBook};
use std::rc::Rc;

// ts: BitgetExchange.listenOHLCV
pub fn listen_ohlcv(
    public_websocket: &dyn ExternalPublicData,
    options: OHLCVOptions,
    callback: Rc<dyn Fn(Candle)>,
) -> Result<ListenerCleanup, ExchangeError> {
    public_websocket.listen_ohlcv(options, callback)
}

// ts: BitgetExchange.listenOrderBook
pub fn listen_order_book(
    public_websocket: &dyn ExternalPublicData,
    symbol: String,
    callback: Rc<dyn Fn(OrderBook)>,
) -> Result<ListenerCleanup, ExchangeError> {
    public_websocket.listen_order_book(symbol, callback)
}
