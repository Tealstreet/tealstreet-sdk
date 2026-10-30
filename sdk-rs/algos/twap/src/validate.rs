//! Bounds on client-supplied TWAP options and restored state. The TS worker
//! trusted them; a server shared by every account cannot.

use crate::types::{TwapOptions, TwapState, TwapStatus};

/// The web form's and frame API's slice cap (`TWAP_MAX_ORDERS`).
pub const MAX_ORDERS_COUNT: u32 = 1_000;

/// Running TWAPs one account may hold; the chaser book caps chases at 50.
pub const MAX_TWAPS_PER_ACCOUNT: usize = 20;

/// How long past its next slice a restored running TWAP may still resume.
pub const SCHEDULE_GRACE_MS: f64 = 60_000.0;

/// Finished TWAP ids an account remembers so a stale client cannot revive them.
pub const RETIRED_IDS_KEPT: usize = 1_024;

pub fn options(options: &TwapOptions) -> Result<(), String> {
    if !(options.size.is_finite() && options.size > 0.0) {
        return Err("size must be a positive number".to_owned());
    }
    if !(options.duration.is_finite() && options.duration > 0.0) {
        return Err("duration must be a positive number of minutes".to_owned());
    }
    if !(1..=MAX_ORDERS_COUNT).contains(&options.orders_count) {
        return Err(format!(
            "ordersCount must be between 1 and {MAX_ORDERS_COUNT}"
        ));
    }
    if !(0.0..=100.0).contains(&options.randomness_percentage) {
        return Err("randomnessPercentage must be between 0 and 100".to_owned());
    }
    if options
        .timeout_pct
        .is_some_and(|pct| !(0.0..=100.0).contains(&pct))
    {
        return Err("timeoutPct must be between 0 and 100".to_owned());
    }
    if !(options.chaser_delay.is_finite() && options.chaser_delay >= 0.0) {
        return Err("chaserDelay must be a non-negative number".to_owned());
    }
    Ok(())
}

/// A restore is a claim from one client's storage, which can be stale or come
/// from another tab: refuse anything finished, inflated or no longer driven.
pub fn restored(options: &TwapOptions, state: &TwapState, now_ms: f64) -> Result<(), String> {
    self::options(options)?;
    if state.orders_count != options.orders_count
        || state.orders_sizes.len() > options.orders_count as usize
    {
        return Err("restored slices exceed ordersCount".to_owned());
    }
    if state
        .orders_sizes
        .iter()
        .any(|size| !(size.is_finite() && *size >= 0.0))
    {
        return Err("restored slice sizes must be finite and non-negative".to_owned());
    }
    if !state.next_order_at.is_finite() {
        return Err("restored nextOrderAt must be finite".to_owned());
    }
    match state.status {
        TwapStatus::Completed => return Err("it already completed".to_owned()),
        TwapStatus::Idle => return Err("it never started".to_owned()),
        TwapStatus::Running | TwapStatus::Paused => {}
    }
    if state.orders_sizes.is_empty() {
        return Err("no slices remain".to_owned());
    }
    if state.orders_sizes.len() as u64 + u64::from(state.orders_executed)
        > u64::from(options.orders_count)
    {
        return Err("remaining and executed slices exceed ordersCount".to_owned());
    }
    let remaining: f64 = state.orders_sizes.iter().sum();
    if remaining > options.size * (1.0 + 1e-9) {
        return Err("remaining slices exceed the TWAP size".to_owned());
    }
    // A paused TWAP places nothing until the user resumes it.
    if state.status == TwapStatus::Running {
        let interval_ms = options.duration / f64::from(options.orders_count) * 60_000.0;
        let latest_ms = state.next_order_at
            + interval_ms * (1.0 + options.randomness_percentage / 100.0)
            + SCHEDULE_GRACE_MS;
        if now_ms > latest_ms {
            return Err("its schedule lapsed, so no client was running it".to_owned());
        }
    }
    Ok(())
}
