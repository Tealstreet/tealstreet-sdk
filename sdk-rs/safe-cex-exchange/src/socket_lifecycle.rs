//! Exchange socket grouping and watchdog; see exchanges/base.ts lifecycle contract.
use crate::websocket::BaseWebSocket;
use serde_json::Value;
use std::rc::Rc;

pub enum SocketSlot {
    Private = 0,
    Public = 1,
    PublicSecondary = 2,
    PublicInverse = 3,
    Futures = 4,
    Business = 5,
    SecondsTickers = 6,
    ThirdTickers = 7,
}

pub(crate) fn socket_diagnostics(sockets: &[Rc<BaseWebSocket>], private: bool) -> Value {
    let sources: Vec<_> = sockets
        .iter()
        .map(|socket| {
            let state = socket.diagnostics();
            Some(safe_cex_store::socket_health::SocketDiagnosticsSource {
                last_close_code: state.last_close_code.map(f64::from),
                last_close_at: Some(state.last_close_at),
                lifetime_close_count: Some(state.lifetime_close_count as f64),
                on_close_count: Some(state.on_close_count as f64),
                is_reconnect_pause_active: Some(socket.is_reconnect_pause_active()),
            })
        })
        .collect();
    serde_json::to_value(safe_cex_store::socket_health::summarize_socket_diagnostics(
        &sources, private,
    ))
    .expect("socket diagnostic scalars serialize")
}
