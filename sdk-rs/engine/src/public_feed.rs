//! Host-owned public data only. Account metadata and private state remain local.
use crate::model::{Candle, Ticker};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PublicFeedPolicy {
    #[default]
    Direct,
    Disabled,
    Shared,
}

#[derive(Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicFeedUpdate {
    #[serde(default)]
    pub tickers: Vec<PublicTicker>,
    #[serde(default)]
    pub books: Vec<PublicBook>,
    #[serde(default)]
    pub candles: Vec<PublicCandle>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PublicTicker {
    pub symbol: String,
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    pub last: Option<f64>,
    pub mark: Option<f64>,
    pub index: Option<f64>,
    pub percentage: Option<f64>,
    pub open_interest: Option<f64>,
    pub funding_rate: Option<f64>,
    pub funding_time: Option<f64>,
    pub volume: Option<f64>,
    pub quote_volume: Option<f64>,
    /// Omitted preserves, null clears, a number replaces.
    #[serde(default, deserialize_with = "midpoint_update")]
    pub mid_price: Option<Option<f64>>,
}

fn midpoint_update<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<f64>>, D::Error> {
    Option::deserialize(deserializer).map(Some)
}

impl PublicFeedUpdate {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.tickers.iter().any(|ticker| !ticker.valid())
            || self.candles.iter().any(|candle| {
                [
                    candle.timestamp,
                    candle.open,
                    candle.high,
                    candle.low,
                    candle.close,
                    candle.volume,
                ]
                .iter()
                .any(|value| !value.is_finite())
            })
        {
            return Err("public feed numbers must be finite");
        }
        if self
            .books
            .iter()
            .flat_map(|book| book.bids.iter().chain(&book.asks))
            .any(|(price, amount)| {
                !price.is_finite() || *price <= 0.0 || !amount.is_finite() || *amount < 0.0
            })
        {
            return Err("invalid public book level");
        }
        Ok(())
    }
}

impl PublicTicker {
    pub(crate) fn has_values(&self) -> bool {
        [
            self.bid,
            self.ask,
            self.last,
            self.mark,
            self.index,
            self.percentage,
            self.open_interest,
            self.funding_rate,
            self.funding_time,
            self.volume,
            self.quote_volume,
        ]
        .iter()
        .any(Option::is_some)
            || self.mid_price.is_some()
    }
    fn valid(&self) -> bool {
        [
            self.bid,
            self.ask,
            self.last,
            self.mark,
            self.index,
            self.percentage,
            self.open_interest,
            self.funding_rate,
            self.funding_time,
            self.volume,
            self.quote_volume,
        ]
        .iter()
        .flatten()
        .all(|value| value.is_finite())
            && self.mid_price.flatten().is_none_or(f64::is_finite)
    }
    pub(crate) fn merge(&mut self, update: Self) {
        macro_rules! fields {
            ($($field:ident),*) => { $(if update.$field.is_some() { self.$field = update.$field; })* };
        }
        fields!(
            bid,
            ask,
            last,
            mark,
            index,
            percentage,
            open_interest,
            funding_rate,
            funding_time,
            volume,
            quote_volume
        );
        if update.mid_price.is_some() {
            self.mid_price = update.mid_price;
        }
    }
    pub(crate) fn apply(&self, target: &mut Ticker) {
        macro_rules! fields {
            ($($field:ident),*) => { $(if let Some(value) = self.$field { target.$field = value; })* };
        }
        fields!(
            bid,
            ask,
            last,
            mark,
            index,
            percentage,
            open_interest,
            funding_rate,
            funding_time,
            volume,
            quote_volume
        );
        if let Some(mid_price) = self.mid_price {
            target.mid_price = mid_price;
        }
    }
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicBook {
    pub symbol: String,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCandle {
    pub symbol: String,
    pub interval: crate::model::Interval,
    pub timestamp: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl PublicCandle {
    pub(crate) fn candle(&self) -> Candle {
        Candle {
            timestamp: self.timestamp,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
        }
    }
}
