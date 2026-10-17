//! Raw TS conformance mirrors, not venue-aware financial interpretation.
//! Inverse price/PnL remains UNSOLVED / REFUSED; see DERIVED_PARITY.md.
use safe_cex_types::{Position, PositionSide};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Returns {
    pub roe: f64,
    pub roi: f64,
}

/// Open positions table: venue UPnL over entry price times contracts.
pub fn calculate_returns(position: &Position, override_upnl: Option<f64>) -> Returns {
    // ts-bug: TS truthiness means exactly zero (also -0 and NaN) falls back to
    // the whole position's UPnL, rather than reporting zero override returns.
    let upnl = override_upnl
        .filter(|v| *v != 0.0 && !v.is_nan())
        .unwrap_or(position.unrealized_pnl);
    let margin = position.entry_price * position.contracts;
    if margin == 0.0 || margin.is_nan() {
        return Returns { roe: 0.0, roi: 0.0 };
    }
    Returns {
        roe: (upnl / margin) * position.leverage * 100.0,
        roi: (upnl / margin) * 100.0,
    }
}

/// Closed/Recent tables: price move over entry; CLI $roe<N> inverts this.
pub fn calculate_relative_returns(position: &Position, exit_price: f64) -> Returns {
    let upnl = if position.side == PositionSide::Long {
        exit_price - position.entry_price
    } else {
        position.entry_price - exit_price
    };
    let margin = position.entry_price;
    if margin == 0.0 || margin.is_nan() {
        return Returns { roe: 0.0, roi: 0.0 };
    }
    Returns {
        roe: (upnl / margin) * position.leverage * 100.0,
        roi: (upnl / margin) * 100.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn position() -> Position {
        serde_json::from_value(serde_json::json!({"id":"p","symbol":"BTC","side":"long","entryPrice":100,"notional":200,"leverage":5,"unrealizedPnl":10,"contracts":2,"liquidationPrice":0})).unwrap()
    }
    #[test]
    fn zero_override_bug_and_distinct_returns_are_preserved() {
        let p = position();
        for value in [None, Some(0.0), Some(-0.0), Some(f64::NAN)] {
            assert_eq!(
                calculate_returns(&p, value),
                Returns {
                    roe: 25.0,
                    roi: 5.0
                }
            );
        }
        assert_eq!(
            calculate_returns(&p, Some(-20.0)),
            Returns {
                roe: -50.0,
                roi: -10.0
            }
        );
        assert_eq!(
            calculate_relative_returns(&p, 110.0),
            Returns {
                roe: 50.0,
                roi: 10.0
            }
        );
        let mut short = p.clone();
        short.side = safe_cex_types::ModelField::Value(PositionSide::Short);
        assert_eq!(
            calculate_relative_returns(&short, 110.0),
            Returns {
                roe: -50.0,
                roi: -10.0
            }
        );
    }
    #[test]
    fn margin_truthiness_and_inverse_mark_drift_do_not_establish_valid_notional() {
        let mut p = position();
        p.entry_price = f64::NAN;
        assert_eq!(calculate_returns(&p, None), Returns { roe: 0.0, roi: 0.0 });
        assert_eq!(
            calculate_relative_returns(&p, 110.0),
            Returns { roe: 0.0, roi: 0.0 }
        );
        p.entry_price = 100.0;
        p.contracts = 0.0;
        assert_eq!(calculate_returns(&p, None), Returns { roe: 0.0, roi: 0.0 });
        p.entry_price = 100_000.0;
        p.notional = 100_000.0;
        p.contracts = p.notional / 110_000.0;
        assert_ne!(p.entry_price * p.contracts, p.notional);
        // This raw mirror lacks venue context. Consumers must retain inverse refusal.
    }
}
