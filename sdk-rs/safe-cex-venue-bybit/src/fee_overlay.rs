//! Per-account fee overlay; the last duplicate symbol wins, as in source Map.
use safe_cex_types::Market;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitFeeRate {
    pub symbol: String,
    pub taker_fee_rate: String,
    pub maker_fee_rate: String,
}
// Native captured closure replaces AccountMarketOverlay's function value.
pub fn bybit_fee_overlay(
    fees: &[BybitFeeRate],
) -> Option<impl Fn(&[Market]) -> Vec<Market> + use<>> {
    let by_symbol: HashMap<_, _> = fees
        .iter()
        .cloned()
        .map(|fee| (fee.symbol.clone(), fee))
        .collect();
    if by_symbol.is_empty() {
        return None;
    }
    Some(move |markets: &[Market]| {
        markets
            .iter()
            .map(|m| {
                let mut m = m.clone();
                if let Some(fee) = by_symbol.get(&m.symbol) {
                    m.taker_fee = Some(safe_cex_num::parse_float(&fee.taker_fee_rate));
                    m.maker_fee = Some(safe_cex_num::parse_float(&fee.maker_fee_rate));
                }
                m
            })
            .collect()
    })
}
