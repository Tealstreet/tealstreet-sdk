use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};

// ts: BinancePublicWebsocket.dispose (binance.ws-public.ts)
pub fn dispose_binance_public(
    socket: &BaseWebSocket,
    dispose_market: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    dispose_market()?;
    socket.dispose();
    Ok(())
}

// ts: BinancePMPublicWebsocket.dispose
pub fn dispose_binance_pm_public(
    socket: &BaseWebSocket,
    dispose_market: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    dispose_market()?;
    socket.dispose();
    Ok(())
}
