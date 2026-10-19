//! Exchange socket grouping and watchdog; see exchanges/base.ts lifecycle contract.
use crate::websocket::BaseWebSocket;
use safe_cex_types::retained_json::RetainedJsonInput;
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

pub(crate) fn socket_diagnostics(
    sockets: &[Rc<BaseWebSocket>],
    private: bool,
) -> Vec<(Vec<u16>, RetainedJsonInput)> {
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
    let summary = safe_cex_store::socket_health::summarize_socket_diagnostics(&sources, private);
    let kind = if private { "private" } else { "public" };
    [
        (
            format!("{kind}CloseCode"),
            summary
                .close_code
                .map_or(RetainedJsonInput::Undefined, RetainedJsonInput::Float),
        ),
        (
            format!("{kind}CloseCountTotal"),
            RetainedJsonInput::Float(summary.close_count_total),
        ),
        (
            format!("{kind}CloseStreak"),
            RetainedJsonInput::Float(summary.close_streak),
        ),
        (
            format!("{kind}ReconnectPaused"),
            RetainedJsonInput::Bool(summary.reconnect_paused),
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.encode_utf16().collect(), value))
    .collect()
}
