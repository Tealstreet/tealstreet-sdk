// ts: BinanceExchange.markPositionStateUpdatedByWebsocket
pub fn mark_position_state_updated_by_websocket(version: &mut f64) {
    *version += 1.0;
}

// ts: BinanceExchange.getPositionStateVersion
pub fn get_position_state_version(version: f64) -> f64 {
    version
}

// ts: BinanceExchange.pauseBackgroundReconciliation
pub fn pause_background_reconciliation(paused_until: &mut f64, now_ms: f64, duration_ms: f64) {
    let next = now_ms + duration_ms;
    if paused_until.is_nan() || next.is_nan() {
        *paused_until = f64::NAN;
    } else if next > *paused_until
        || (next == 0.0 && !next.is_sign_negative() && *paused_until == 0.0)
    {
        *paused_until = next;
    }
}

// ts: BinanceExchange.isBackgroundReconciliationPaused
pub fn is_background_reconciliation_paused(paused_until: f64, now_ms: f64) -> bool {
    now_ms < paused_until
}
