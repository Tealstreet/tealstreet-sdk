//! TxFlow's REST envelopes and instrument records; public-stream records
//! live in `safe-cex-txflow-public-wire`.
use safe_cex_engine::de::number_or_zero;
use safe_cex_engine::model::{Limits, Market, MinMax, Precision};
pub use safe_cex_txflow_public_wire::*;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct Header {
    pub success: bool,
    pub code: i64,
}
impl Header {
    pub fn valid(body: &str) -> bool {
        serde_json::from_str::<Self>(body).is_ok_and(|h| h.success && h.code == 0)
    }
}
#[derive(Deserialize)]
pub struct Reply<T> {
    pub success: bool,
    pub code: i64,
    pub data: T,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument {
    #[serde(skip)]
    pub ticker_index: Option<u32>,
    pub index: u32,
    pub name: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub currency_type: String,
    pub delisted: bool,
    pub halt_trading: bool,
    #[serde(deserialize_with = "number_or_zero")]
    pub base_precision: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub price_tick: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub max_leverage: f64,
    #[serde(default)]
    pub only_isolated: bool,
    #[serde(default)]
    pub use_notional_margin: bool,
    #[serde(default)]
    pub max_limit_order_size: Option<f64>,
    #[serde(default)]
    pub max_market_order_size: Option<f64>,
}
impl Instrument {
    pub fn market(&self) -> Option<Market> {
        if self.currency_type != "usdc"
            || self.delisted
            || self.base_precision <= 0.0
            || self.price_tick <= 0.0
            || self.max_leverage < 1.0
        {
            return None;
        }
        Some(Market {
            id: self.index.to_string(),
            symbol: self.name.clone(),
            pretty_symbol: None,
            base: self.base_currency.clone(),
            quote: self.quote_currency.clone(),
            active: !self.halt_trading,
            precision: Precision {
                amount: self.base_precision,
                price: self.price_tick,
            },
            limits: Limits {
                amount: MinMax {
                    min: self.base_precision,
                    max: self.max_limit_order_size.unwrap_or(9_007_199_254_740_991.0),
                },
                leverage: MinMax {
                    min: 1.0,
                    max: self.max_leverage,
                },
                price: None,
            },
            contract_size: Some(1.0),
            is_expiring: None,
            is_inverse: Some(false),
            is_spot: Some(false),
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        })
    }
}
