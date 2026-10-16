use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use std::rc::Rc;

// ts: BinanceMarketWebsocket.connectAndSubscribe
pub fn connect_binance_market(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    start(socket, crate::binance_types::BASE_WS_URL, "market")
}

// ts: BinancePMMarketWebsocket.connectAndSubscribe
pub fn connect_binance_pm_market(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    start(socket, crate::binancepm_types::BASE_WS_URL, "market")
}

// ts: BinancePublicWebsocket.connectAndSubscribe (binancespot.ws-public.ts)
pub fn connect_binance_spot_public(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    start(socket, crate::binancespot_types::BASE_WS_URL, "public")
}

// ts: BinancePublicWebsocket.connectAndSubscribe (binance.ws-public.ts)
/// The companion market socket's concrete connect operation remains caller-owned;
/// both sockets' dial tasks and event delivery are owned by BaseWebSocket.
pub fn connect_binance_public(
    socket: &Rc<BaseWebSocket>,
    connect_market: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    start(socket, crate::binance_types::BASE_WS_URL, "public")?;
    connect_market()
}

// ts: BinancePMPublicWebsocket.connectAndSubscribe
pub fn connect_binance_pm_public(
    socket: &Rc<BaseWebSocket>,
    connect_market: impl FnOnce() -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    start(socket, crate::binancepm_types::BASE_WS_URL, "public")?;
    connect_market()
}

fn start(
    socket: &Rc<BaseWebSocket>,
    urls: &[(&str, &[(&str, &str)])],
    channel: &str,
) -> Result<(), ExchangeError> {
    let network = if socket.emitter().options().testnet == Some(true) {
        "testnet"
    } else {
        "livenet"
    };
    let url = urls
        .iter()
        .find(|(key, _)| *key == channel)
        .and_then(|(_, networks)| networks.iter().find(|(key, _)| *key == network))
        .map(|(_, url)| *url)
        .ok_or_else(|| ExchangeError::new("generated Binance websocket URL missing"))?;
    // None retains the source's omitted options. The core binds open, message
    // and close events to the installed protocol and shared close operation.
    socket.start_websocket(url.into(), vec![], None);
    Ok(())
}
