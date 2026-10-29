//! Bounds on client-supplied TWAP options and restored state. The TS worker
//! trusted them; a server shared by every account cannot.

use crate::types::{TwapOptions, TwapState};

/// The web form's and frame API's slice cap (`TWAP_MAX_ORDERS`).
pub const MAX_ORDERS_COUNT: u32 = 1_000;

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

pub fn restored(options: &TwapOptions, state: &TwapState) -> Result<(), String> {
    self::options(options)?;
    if state.orders_count != options.orders_count
        || state.orders_sizes.len() > options.orders_count as usize
    {
        return Err("restored slices exceed ordersCount".to_owned());
    }
    if state.orders_sizes.iter().any(|size| !size.is_finite()) {
        return Err("restored slice sizes must be finite".to_owned());
    }
    if !state.next_order_at.is_finite() {
        return Err("restored nextOrderAt must be finite".to_owned());
    }
    Ok(())
}
