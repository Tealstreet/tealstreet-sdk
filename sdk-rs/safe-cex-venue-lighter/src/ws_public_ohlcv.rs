//! Pure topic/recovery helpers from lighter.ws-public-ohlcv.ts.
use safe_cex_num::{PrecisionInput, js_number_string};

// ts: buildLighterCandleTopic
pub fn build_lighter_candle_topic(
    market_id: impl Into<PrecisionInput>,
    resolution: &str,
) -> String {
    let market_id = match market_id.into() {
        PrecisionInput::String(value) => value,
        PrecisionInput::Number(value) => js_number_string(value),
    };
    format!("candle/{market_id}/{resolution}")
}
// ts: buildLighterTradesTopic
pub fn build_lighter_trades_topic(market_id: impl Into<PrecisionInput>) -> String {
    let market_id = match market_id.into() {
        PrecisionInput::String(value) => value,
        PrecisionInput::Number(value) => js_number_string(value),
    };
    format!("trade/{market_id}")
}
// ts: shouldAttemptLighterOhlcvRecovery
pub fn should_attempt_lighter_ohlcv_recovery(
    has_topic_handlers: bool,
    now_ms: f64,
    last_recovery_reconnect_at_ms: f64,
    recovery_cooldown_ms: f64,
) -> bool {
    if !has_topic_handlers {
        return false;
    }
    now_ms - last_recovery_reconnect_at_ms >= recovery_cooldown_ms
}
// ts: shouldPromoteLighterOhlcvRecoveryToProxy
pub fn should_promote_lighter_ohlcv_recovery_to_proxy(
    is_ws_proxy_route_enabled: bool,
    use_proxy_route: bool,
    connected_via_proxy_route: bool,
) -> bool {
    is_ws_proxy_route_enabled && !use_proxy_route && !connected_via_proxy_route
}
