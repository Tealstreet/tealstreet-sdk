use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use std::rc::Rc;

pub fn private_ping(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    socket.send_with_log("ping")
}

pub fn business_ping(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    socket.send_with_log("ping")
}
