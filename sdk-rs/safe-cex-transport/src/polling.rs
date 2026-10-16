use std::collections::BTreeMap;
use std::sync::RwLock;

pub const ALL_EXCHANGES: &str = "*";

static MULTIPLIERS: RwLock<BTreeMap<String, f64>> = RwLock::new(BTreeMap::new());
pub fn set_tick_interval_multipliers(next: Option<&BTreeMap<String, f64>>) {
    let mut values = MULTIPLIERS.write().expect("tick multiplier lock");
    values.clear();
    if let Some(next) = next {
        for (exchange, raw) in next {
            if !raw.is_finite() {
                continue;
            }
            let clamped = raw.clamp(1.0, 60.0);
            if clamped != 1.0 {
                values.insert(exchange.clone(), clamped);
            }
        }
    }
}
pub fn get_tick_interval_multiplier(exchange: &str) -> f64 {
    let values = MULTIPLIERS.read().expect("tick multiplier lock");
    values
        .get(exchange)
        .or_else(|| values.get(ALL_EXCHANGES))
        .copied()
        .unwrap_or(1.0)
}
pub fn get_tick_interval_multipliers_for_tests() -> BTreeMap<String, f64> {
    MULTIPLIERS.read().expect("tick multiplier lock").clone()
}

pub fn get_tick_interval(
    exchange: &str,
    configured: Option<f64>,
    auth_failure_streak: u32,
) -> Option<f64> {
    configured.map(|configured| {
        let base = if configured != 0.0 {
            configured * get_tick_interval_multiplier(exchange)
        } else {
            configured
        };
        if base == 0.0 || base.is_nan() || auth_failure_streak == 0 {
            base
        } else {
            (base * 2_f64.powf(f64::from(auth_failure_streak))).min(60_000.0)
        }
    })
}
