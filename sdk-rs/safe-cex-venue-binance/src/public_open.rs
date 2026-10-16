use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use std::rc::Rc;

// ts: BinanceMarketWebsocket.onOpen
pub fn open_binance_market(
    socket: &Rc<BaseWebSocket>,
    subscribe: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    open_market(socket, subscribe)
}
// ts: BinancePMMarketWebsocket.onOpen
pub fn open_binance_pm_market(
    socket: &Rc<BaseWebSocket>,
    subscribe: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    open_market(socket, subscribe)
}
fn open_market(
    socket: &Rc<BaseWebSocket>,
    subscribe: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.send(r#"{"id":1,"method":"SUBSCRIBE","params":["!ticker@arr","!markPrice@arr@1s"]}"#)?;
    subscribe()
}
// ts: BinancePublicWebsocket.onOpen
pub fn open_binance_public(
    socket: &Rc<BaseWebSocket>,
    public_data_only: bool,
    subscribe: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    open_public(socket, public_data_only, subscribe)
}
// ts: BinancePMPublicWebsocket.onOpen
pub fn open_binance_pm_public(
    socket: &Rc<BaseWebSocket>,
    public_data_only: bool,
    subscribe: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    open_public(socket, public_data_only, subscribe)
}
fn open_public(
    socket: &Rc<BaseWebSocket>,
    public_data_only: bool,
    subscribe: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    if !public_data_only {
        socket.send(r#"{"id":1,"method":"SUBSCRIBE","params":["!bookTicker"]}"#)?;
    }
    socket.reset_orderbook();
    subscribe()
}
// ts: BinancePublicWebsocket.onOpen (binancespot.ws-public.ts)
pub fn open_binance_spot_public(
    socket: &Rc<BaseWebSocket>,
    subscribe: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.send(r#"{"id":1,"method":"SUBSCRIBE","params":["!ticker@arr","!bookTicker"]}"#)?;
    socket.reset_orderbook();
    subscribe()
}
