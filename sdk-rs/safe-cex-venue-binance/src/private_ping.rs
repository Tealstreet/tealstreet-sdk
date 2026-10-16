use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use std::rc::Rc;

// ts: BinancePrivateWebsocket.ping
pub fn ping_binance_private(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    ping(socket, r#"{"id":42,"method":"LIST_SUBSCRIPTIONS"}"#)
}
// ts: BinancePMPrivateWebsocket.ping
pub fn ping_binance_pm_private(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    ping(socket, r#"{"id":42,"method":"LIST_SUBSCRIPTIONS"}"#)
}
// ts: BinancePrivateWebsocket.ping (binancespot.ws-private.ts)
pub fn ping_binance_spot_private(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    ping(
        socket,
        &format!(
            r#"{{"id":{},"method":"session.status"}}"#,
            safe_cex_num::js_number_string(crate::spot_protocol::BINANCE_SPOT_HEARTBEAT_REQUEST_ID)
        ),
    )
}
fn ping(socket: &Rc<BaseWebSocket>, frame: &str) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    socket.send_with_log(frame)
}
