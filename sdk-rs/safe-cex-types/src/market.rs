//! Canonical public Market declaration; shared wiring re-exports this module.
use crate::{MarketLimits, MarketPrecision, MarketRuntime, OptionTerms};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, TS)]
#[ts(optional_fields, rename_all = "camelCase")]
pub struct Market {
    pub id: String,
    pub symbol: String,

    pub pretty_symbol: Option<String>,
    pub base: String,
    pub quote: String,
    pub active: bool,
    #[ts(inline)]
    pub precision: MarketPrecision,
    #[ts(inline)]
    pub limits: MarketLimits,

    pub option: Option<OptionTerms>,

    pub contract_size: Option<f64>,

    pub is_expiring: Option<bool>,

    pub is_inverse: Option<bool>,

    pub is_spot: Option<bool>,

    pub is_perpetual: Option<bool>,

    pub is_read_only: Option<bool>,

    #[ts(type = "any", optional)]
    pub info: Option<crate::retained_json::RetainedJsonInput>,

    pub taker_fee: Option<f64>,

    pub maker_fee: Option<f64>,
    /// Runtime venue properties; serialized at the Market object's top level.
    #[ts(skip)]
    pub runtime: MarketRuntime,
}
