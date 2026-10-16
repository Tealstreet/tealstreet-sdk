//! packages/safe-cex/src/config/config.ts; host-evaluated initialization inputs.

pub const WS_MAX_RECONNECTION_DELAY: f64 = 5000.0;
pub const WS_BACKOFF_BASE_DELAY: f64 = 2000.0;
pub const WS_BACKOFF_MAX_DELAY: f64 = 30000.0;
pub const WS_BACKOFF_FACTOR: f64 = 2.0;
pub const WS_FAILURE_STREAK_PAUSE_THRESHOLD: usize = 8;
pub const WS_FAILURE_STREAK_PAUSE_MS: f64 = 120000.0;
pub const ENABLE_RECONNECTING_WEBSOCKET: bool = false;

/// The host samples Math.random's unit interval once during runtime startup.
pub fn ws_min_reconnection_delay(random_unit: f64) -> f64 {
    1000.0 + random_unit * 2000.0
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReconnectingWebsocketTuning {
    pub max_reconnection_delay: f64,
    pub min_reconnection_delay: f64,
}

/// Retain this value for the runtime lifetime, like the source module constant.
pub fn reconnecting_websocket_tuning(random_unit: f64) -> ReconnectingWebsocketTuning {
    ReconnectingWebsocketTuning {
        max_reconnection_delay: WS_MAX_RECONNECTION_DELAY,
        min_reconnection_delay: ws_min_reconnection_delay(random_unit),
    }
}

fn environment_flag(value: Option<&str>) -> bool {
    // Boolean(+(env || '0')), including hex, Infinity, NaN and JS whitespace.
    let number = safe_cex_num::number_from_string(value.filter(|s| !s.is_empty()).unwrap_or("0"));
    number != 0.0 && !number.is_nan()
}

/// Pass NEXT_PUBLIC_SIMULATE_HTTP_OVER_WEBSOCKET at runtime initialization.
pub fn simulate_http_over_websocket(value: Option<&str>) -> bool {
    environment_flag(value)
}

/// Pass NEXT_PUBLIC_WS_LIFECYCLE_DEBUG at runtime initialization.
pub fn enable_ws_lifecycle_debug_logs(value: Option<&str>) -> bool {
    environment_flag(value)
}
