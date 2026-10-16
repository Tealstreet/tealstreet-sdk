use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};

// ts: the five public Binance-family recordHandlerStats fields.
/// Shared extraction of the identical forwarding bodies. Keep core's registry,
/// inherited-member read semantics and caller-visible reconciliation errors.
/// Mutable builtin referents are shared through the socket's captured realm.
pub fn record_binance_handler_stats(
    socket: &BaseWebSocket,
    topic: &str,
) -> Result<(), ExchangeError> {
    socket.record_message_handler_stats(topic)
}
