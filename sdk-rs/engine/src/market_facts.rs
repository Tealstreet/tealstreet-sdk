//! Native planning facts. Deliberately absent from frames, JSON and TS types.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum NotionalBasis {
    #[default]
    Base,
    /// Native quote contracts: independent of the execution price.
    Quote { per_amount: f64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MarketAmountRules {
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub step: Option<f64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MarketOrderFacts {
    pub min_notional: Option<f64>,
    pub reduce_only_exempt: bool,
    pub market_notional_exempt: bool,
    pub notional_basis: NotionalBasis,
    /// A native tick that differs from the display precision.
    pub price_tick: Option<f64>,
    /// Native marketable-limit price offset, e.g. Lighter's two percent.
    pub market_slippage: Option<f64>,
    pub market_amount: Option<MarketAmountRules>,
}

impl MarketOrderFacts {
    pub fn round_price(price: f64, tick: f64) -> f64 {
        crate::num::adjust(price, tick)
    }

    /// Quote value per placement unit, using the adapter's native price policy.
    pub fn value_per_amount(
        &self,
        price: Option<f64>,
        side: crate::model::OrderSide,
        ticker: Option<&crate::model::Ticker>,
        tick: f64,
    ) -> f64 {
        match self.notional_basis {
            NotionalBasis::Quote { per_amount } => per_amount,
            NotionalBasis::Base => price.unwrap_or_else(|| {
                let reference = if self.market_slippage.is_some() {
                    ticker.and_then(|ticker| {
                        [ticker.last, ticker.mark, ticker.index]
                            .into_iter()
                            .find(|value| value.is_finite() && *value > 0.0)
                    })
                } else {
                    ticker.map(|ticker| ticker.last)
                };
                let reference = reference.unwrap_or(f64::NAN);
                if let Some(slippage) = self.market_slippage {
                    let multiplier = match side {
                        crate::model::OrderSide::Buy => 1.0 + slippage,
                        crate::model::OrderSide::Sell => 1.0 - slippage,
                    };
                    crate::num::adjust(reference * multiplier, tick)
                } else {
                    reference
                }
            }),
        }
    }
}
