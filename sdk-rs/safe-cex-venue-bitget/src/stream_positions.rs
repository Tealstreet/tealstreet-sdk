//! Pure selection of REST versus recent private-stream position records.
use safe_cex_types::{Position, retained_json::RetainedJsonInput as Input};
use std::collections::{HashMap, HashSet};

// ts: BitgetPrivateWebsocket.getPositionPayloadUpdateTime
pub fn get_position_payload_update_time(position: &Position) -> Option<f64> {
    let raw = position.info.as_ref().and_then(|info| {
        ["uTime", "updatedTime", "utime", "cTime", "createdTime"]
            .into_iter()
            .find_map(|key| info.get(key).filter(|v| !v.is_nullish()))
    });
    let parsed = match raw? {
        value @ Input::Text(_) => safe_cex_num::js_retained_parse_float(Some(value))
            .expect("primitive text parseFloat cannot throw"),
        value @ (Input::Float(_) | Input::Lexeme(_)) => value.as_f64()?,
        _ => return None,
    };
    parsed.is_finite().then_some(parsed)
}

// ts: BitgetPrivateWebsocket.mergeRestPositionsWithFreshWsState
pub fn merge_rest_positions_with_fresh_ws_state<'a>(
    rest_positions: &'a [Position],
    current_positions: &'a [Position],
    request_started_at: f64,
    last_snapshot_at: f64,
    snapshot_ids: &HashSet<String>,
    updated_at_by_id: &HashMap<String, f64>,
    stale_guard_ms: f64,
) -> Vec<&'a Position> {
    // Map construction selects the last current duplicate; the final append
    // loop below still visits the original slice and keeps its first unused ID.
    let current_by_id: HashMap<_, _> = current_positions
        .iter()
        .map(|position| (position.id.as_str(), position))
        .collect();
    let mut merged = Vec::new();
    let mut used_ids = HashSet::new();
    let snapshot_is_fresh =
        last_snapshot_at > 0.0 && request_started_at - last_snapshot_at <= stale_guard_ms;
    for rest_position in rest_positions {
        if snapshot_is_fresh && !snapshot_ids.contains(&rest_position.id) {
            continue;
        }
        let current_position = current_by_id.get(rest_position.id.as_str());
        let ws_updated_at = updated_at_by_id
            .get(&rest_position.id)
            .copied()
            .unwrap_or(0.0);
        let ws_is_fresh =
            ws_updated_at > 0.0 && request_started_at - ws_updated_at <= stale_guard_ms;
        if let Some(current_position) = current_position.filter(|_| ws_is_fresh) {
            let current_time = get_position_payload_update_time(current_position);
            let rest_time = get_position_payload_update_time(rest_position);
            if rest_time.is_none_or(|time| time == 0.0)
                || current_time.is_some_and(|current| {
                    current != 0.0 && rest_time.is_some_and(|rest| current > rest)
                })
            {
                merged.push(*current_position);
                used_ids.insert(current_position.id.as_str());
                continue;
            }
        }
        merged.push(rest_position);
        used_ids.insert(rest_position.id.as_str());
    }
    for current_position in current_positions {
        if used_ids.contains(current_position.id.as_str()) {
            continue;
        }
        let ws_updated_at = updated_at_by_id
            .get(&current_position.id)
            .copied()
            .unwrap_or(0.0);
        if ws_updated_at > 0.0 && request_started_at - ws_updated_at <= stale_guard_ms {
            merged.push(current_position);
            used_ids.insert(current_position.id.as_str());
        }
    }
    merged
}
