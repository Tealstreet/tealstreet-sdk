//! Pure position-value policy shared by engines, algos and numeric contract tests.
#![no_std]
/// Position quantities with independent native-unit evidence.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeSizeInputs {
    pub contracts: Option<f64>,
    pub notional: Option<f64>,
    pub contracts_native: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeSizeMarket {
    pub contract_size: Option<f64>,
    pub is_inverse: Option<bool>,
}

/// Native sizing follows the SDK's coherent-evidence policy; None is unknown.
pub fn native_position_size(position: &NativeSizeInputs, market: &NativeSizeMarket) -> Option<f64> {
    let contract_size = match market.contract_size {
        None | Some(0.0) => 1.0,
        Some(size) => size,
    };
    if !contract_size.is_finite() || contract_size <= 0.0 {
        return None;
    }
    let source = if market.is_inverse.unwrap_or(false) {
        position.notional
    } else {
        position.contracts
    };
    let derived = source
        .filter(|value| value.is_finite())
        .map(|value| value.abs() / contract_size)
        .filter(|value| value.is_finite());
    let Some(native) = position.contracts_native else {
        return derived;
    };
    if !native.is_finite() {
        return None;
    }
    let amount = native.abs();
    if derived.is_some_and(|value| (amount - value).abs() > f64::EPSILON * amount * 8.0) {
        return None;
    }
    Some(amount)
}

/// Native numeric callers retain NaN for unknown valuation, including with a zero balance.
pub fn calculate_position_account_leverage(notional: f64, balance: f64) -> f64 {
    if !notional.is_finite() || !balance.is_finite() {
        return f64::NAN;
    }
    if balance <= 0.0 {
        return 0.0;
    }
    let leverage = notional.abs() / balance;
    if leverage.is_finite() {
        leverage
    } else {
        f64::NAN
    }
}
pub fn calculate_total_account_leverage(notionals: &[f64], balance: f64) -> f64 {
    calculate_position_account_leverage(calculate_total_notional(notionals), balance)
}
pub fn calculate_total_notional(notionals: &[f64]) -> f64 {
    let mut total = 0.0;
    for notional in notionals {
        if !notional.is_finite() {
            return f64::NAN;
        }
        total += notional.abs();
    }
    if total.is_finite() { total } else { f64::NAN }
}
