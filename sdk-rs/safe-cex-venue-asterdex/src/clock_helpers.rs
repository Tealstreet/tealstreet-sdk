//! Pure clock protocol helpers; lifecycle stays with the venue clock.
use crate::types::AsterDEXAPIVersion;
use safe_cex_num::{js_number, js_trim};
use serde_json::Value;

// ts: AsterDEXVirtualClock.getTimePathCandidates
pub fn get_time_path_candidates(preferred: AsterDEXAPIVersion) -> [&'static str; 2] {
    if preferred == AsterDEXAPIVersion::V3 {
        ["/fapi/v3/time", "/fapi/v1/time"]
    } else {
        ["/fapi/v1/time", "/fapi/v3/time"]
    }
}

// ts: AsterDEXVirtualClock.parseServerTime
pub fn parse_server_time(value: Option<&Value>) -> Option<f64> {
    match value {
        Some(Value::Number(_)) => {}
        Some(Value::String(value)) if !js_trim(value).is_empty() => {}
        _ => return None,
    }
    // The type gate excludes all object coercion hooks, so Number cannot throw.
    let parsed = js_number(value).ok()?;
    (parsed.is_finite() && parsed > 0.0).then_some(parsed)
}

/// Millisecond value of the Dayjs result. Snapshot inputs are epoch milliseconds;
/// converting them applies the same TimeClip as construction of the TS Dayjs state.
// ts: AsterDEXVirtualClock.getCurrentTime
pub fn get_current_time(server: Option<f64>, client: Option<f64>, now: f64) -> f64 {
    use safe_cex_num::js_time_clip;
    match (server, client) {
        (Some(server), Some(client)) => {
            js_time_clip(js_time_clip(server) + (js_time_clip(now) - js_time_clip(client)))
        }
        _ => js_time_clip(now),
    }
}

// ts: AsterDEXVirtualClock.getCurrentTimeMs
pub fn get_current_time_ms(server: Option<f64>, client: Option<f64>, now: f64) -> f64 {
    get_current_time(server, client, now)
}

// ts: AsterDEXVirtualClock.getTimeOffset
pub fn get_time_offset(server: Option<f64>, client: Option<f64>) -> f64 {
    match (server, client) {
        (Some(server), Some(client)) => {
            safe_cex_num::js_time_clip(server) - safe_cex_num::js_time_clip(client)
        }
        _ => 0.0,
    }
}

// ts: AsterDEXVirtualClock.needsSync
pub fn needs_sync(server: Option<f64>, client: Option<f64>, now: f64) -> bool {
    match (server, client) {
        (Some(_), Some(client)) => {
            safe_cex_num::js_time_clip(now) - safe_cex_num::js_time_clip(client) > 300_000.0
        }
        _ => true,
    }
}
