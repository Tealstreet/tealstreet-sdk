//! Ping timer cleanup bodies; WebSocket construction and scheduling stay pending.
use safe_cex_exchange::runtime::ExchangeTask;

// The source field is NodeJS.Timeout | undefined. A present runtime task is
// the corresponding truthy opaque timer handle; cancel before clearing the field.
fn clear_ping_interval(interval: &mut Option<Box<dyn ExchangeTask>>) {
    if let Some(timer) = interval.as_ref() {
        timer.cancel();
        *interval = None;
    }
}

// ts: PolymarketPublicWebSocket.clearPingInterval
pub fn clear_public_ping_interval(interval: &mut Option<Box<dyn ExchangeTask>>) {
    clear_ping_interval(interval);
}

// ts: PolymarketPrivateWebSocket.clearPingInterval
pub fn clear_private_ping_interval(interval: &mut Option<Box<dyn ExchangeTask>>) {
    clear_ping_interval(interval);
}
