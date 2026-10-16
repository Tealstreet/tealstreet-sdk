use crate::runtime::{ExchangeError, ListenerCleanup};
use safe_cex_types::{Candle, OHLCVOptions, OrderBook};
use std::rc::Rc;
/// Binding to the existing external/in-process provider, not a second provider implementation.
pub trait ExternalPublicData {
    fn connect(&self);
    fn dispose(&self);
    fn is_initialized(&self) -> bool;
    fn is_healthy(&self) -> bool;
    fn last_message_timestamp(&self) -> Option<f64>;
    fn listen_ohlcv(
        &self,
        options: OHLCVOptions,
        callback: Rc<dyn Fn(Candle)>,
    ) -> Result<ListenerCleanup, ExchangeError>;
    fn listen_order_book(
        &self,
        symbol: String,
        callback: Rc<dyn Fn(OrderBook)>,
    ) -> Result<ListenerCleanup, ExchangeError>;
}
