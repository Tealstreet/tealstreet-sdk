//! Pure private websocket decisions; owned runtime and state transitions remain separate.

const STANDARD_PING_INTERVAL_MS: f64 = 10_000.0;
const DEGRADED_PING_INTERVAL_MS: f64 = 20_000.0;
const STANDARD_IDLE_PING_THRESHOLD_MS: f64 = 20_000.0;
const DEGRADED_IDLE_PING_THRESHOLD_MS: f64 = 45_000.0;
const STANDARD_IDLE_RECONNECT_THRESHOLD_MS: f64 = 180_000.0;
const DEGRADED_IDLE_RECONNECT_THRESHOLD_MS: f64 = 180_000.0;
const RATE_LIMIT_COOLDOWN_MS: f64 = 60_000.0;
const RATE_LIMIT_FORCE_TOAST_AFTER_MS: f64 = RATE_LIMIT_COOLDOWN_MS;

// ts: LighterPrivateWebsocket.shouldEnforcePongMissReconnect
pub fn should_enforce_pong_miss_reconnect() -> bool {
    false
}

// ts: LighterPrivateWebsocket.isReadonlyFallbackActive
pub fn is_readonly_fallback_active(use_readonly_fallback: bool) -> bool {
    use_readonly_fallback
}

// ts: LighterPrivateWebsocket.shouldUseRestOrderReconciliation
pub fn should_use_rest_order_reconciliation(
    has_received_account_all_orders_ack: bool,
    use_readonly_fallback: bool,
) -> bool {
    !has_received_account_all_orders_ack || use_readonly_fallback
}

// ts: LighterPrivateWebsocket.getPingIntervalMs
pub fn get_ping_interval_ms(use_readonly_fallback: bool) -> f64 {
    if use_readonly_fallback {
        DEGRADED_PING_INTERVAL_MS
    } else {
        STANDARD_PING_INTERVAL_MS
    }
}

// ts: LighterPrivateWebsocket.getIdlePingThresholdMs
pub fn get_idle_ping_threshold_ms(use_readonly_fallback: bool) -> f64 {
    if use_readonly_fallback {
        DEGRADED_IDLE_PING_THRESHOLD_MS
    } else {
        STANDARD_IDLE_PING_THRESHOLD_MS
    }
}

// ts: LighterPrivateWebsocket.getIdleReconnectThresholdMs
pub fn get_idle_reconnect_threshold_ms(use_readonly_fallback: bool) -> f64 {
    if use_readonly_fallback {
        DEGRADED_IDLE_RECONNECT_THRESHOLD_MS
    } else {
        STANDARD_IDLE_RECONNECT_THRESHOLD_MS
    }
}

// ts: LighterPrivateWebsocket.getWatchdogStaleThresholdMs
pub fn get_watchdog_stale_threshold_ms(use_readonly_fallback: bool) -> f64 {
    get_idle_reconnect_threshold_ms(use_readonly_fallback)
}

// ts: LighterPrivateWebsocket.isWsRateLimited
pub fn is_ws_rate_limited(now_ms: f64, ws_rate_limited_until: f64) -> bool {
    now_ms < ws_rate_limited_until
}

// ts: LighterPrivateWebsocket.shouldForceToastPrivateStreamRateLimit
pub fn should_force_toast_private_stream_rate_limit(
    now_ms: f64,
    error_count: f64,
    first_seen_at: f64,
) -> bool {
    error_count > 1.0
        && first_seen_at > 0.0
        && now_ms - first_seen_at >= RATE_LIMIT_FORCE_TOAST_AFTER_MS
}

// ts: LighterPrivateWebsocket.isRecentStreamEvent
pub fn is_recent_stream_event(
    timestamp_ms: f64,
    wall_now_ms: f64,
    virtual_now_ms: f64,
    last_connect_time: f64,
) -> bool {
    if !timestamp_ms.is_finite() || timestamp_ms <= 0.0 {
        return false;
    }
    let is_fresh_event = wall_now_ms - timestamp_ms <= 5_000.0;
    let is_enough_time_passed_after_connect = virtual_now_ms - last_connect_time > 5_000.0;
    is_fresh_event && is_enough_time_passed_after_connect
}

// ts: LighterPrivateWebsocket.flattenByMarketPayload
pub fn flatten_by_market_payload(
    payload: Option<&serde_json::Value>,
    include_single_objects: bool,
) -> Vec<&serde_json::Value> {
    use serde_json::Value;
    let entries: Vec<&Value> = match payload {
        Some(Value::Object(values)) => safe_cex_num::js_own_keys(values.keys().map(String::as_str))
            .into_iter()
            .filter_map(|key| values.get(key))
            .collect(),
        Some(Value::Array(values)) => values.iter().collect(),
        _ => return Vec::new(),
    };
    let mut flattened = Vec::new();
    for entry in entries {
        match entry {
            Value::Array(values) => flattened.extend(values),
            Value::Object(_) if include_single_objects => flattened.push(entry),
            _ => {}
        }
    }
    flattened
}

// ts: LighterPrivateWebsocket.flattenOrdersByMarket
pub fn flatten_orders_by_market(payload: Option<&serde_json::Value>) -> Vec<&serde_json::Value> {
    flatten_by_market_payload(payload, true)
}
