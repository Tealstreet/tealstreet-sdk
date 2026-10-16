use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use std::{cell::Cell, rc::Rc};

// ts: BinancePrivateWebsocket.onOpen
pub fn open_binance_private(
    socket: &Rc<BaseWebSocket>,
    resync_orders_on_open: &Cell<bool>,
    has_opened: &Cell<bool>,
    throttled_fetch_orders: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    open(
        socket,
        resync_orders_on_open,
        has_opened,
        crate::private_ping::ping_binance_private,
        throttled_fetch_orders,
    )
}
// ts: BinancePMPrivateWebsocket.onOpen
pub fn open_binance_pm_private(
    socket: &Rc<BaseWebSocket>,
    resync_orders_on_open: &Cell<bool>,
    has_opened: &Cell<bool>,
    throttled_fetch_orders: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    open(
        socket,
        resync_orders_on_open,
        has_opened,
        crate::private_ping::ping_binance_pm_private,
        throttled_fetch_orders,
    )
}
fn open(
    socket: &Rc<BaseWebSocket>,
    resync_orders_on_open: &Cell<bool>,
    has_opened: &Cell<bool>,
    ping: fn(&Rc<BaseWebSocket>) -> Result<(), ExchangeError>,
    throttled_fetch_orders: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    ping(socket)?;
    if resync_orders_on_open.get() || has_opened.get() {
        resync_orders_on_open.set(false);
        throttled_fetch_orders()?;
    }
    has_opened.set(true);
    Ok(())
}
