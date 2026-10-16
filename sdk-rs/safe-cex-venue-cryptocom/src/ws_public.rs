//! Stateless policy bodies from CryptocomPublicWebSocket.
use crate::types::{CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS, CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS};

pub fn should_enforce_pong_miss_reconnect() -> bool {
    false
}

pub fn get_watchdog_stale_threshold_ms() -> f64 {
    CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS
}

pub fn get_watchdog_ping_threshold_ms() -> f64 {
    CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS
}

pub fn ping() {}
