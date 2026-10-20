//! Contract V3 wire records. Strings borrow the incoming frame.
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

pub use safe_cex_engine::de::{number, optional_number};

pub fn timestamp(n: f64) -> f64 {
    if n >= 1e18 {
        (n / 1e6).floor()
    } else if n >= 1e15 {
        (n / 1e3).floor()
    } else if n >= 1e12 {
        n.floor()
    } else {
        (n * 1000.0).floor()
    }
}

// TS public mappers use toNumber(value, fallback); private financial rows remain strict.
fn public_optional_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Input {
        Number(f64),
        Text(String),
    }
    match Option::<Input>::deserialize(d)? {
        None => Ok(None),
        Some(Input::Text(text)) if text.trim().is_empty() => Ok(None),
        Some(Input::Text(text)) => safe_cex_engine::startup::number_or_zero(&text)
            .map(Some)
            .ok_or_else(|| serde::de::Error::custom("invalid public number")),
        Some(Input::Number(value)) if value.is_finite() => Ok(Some(value)),
        _ => Err(serde::de::Error::custom("invalid public number")),
    }
}
/// Public statistics may publish `null` or blank for an unknown figure; it reads as 0.
fn public_number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(public_optional_number(d)?.unwrap_or(0.0))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Symbol<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub base_asset: Str<'a>,
    #[serde(borrow)]
    pub quote_asset: Str<'a>,
    #[serde(borrow)]
    pub margin_asset: Str<'a>,
    pub forward_contract_flag: Option<bool>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub price_precision: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub quantity_precision: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub quote_precision: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub base_asset_precision: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub min_order_size: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub max_order_size: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub min_leverage: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub max_leverage: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub contract_val: Option<f64>,
    #[serde(default, deserialize_with = "public_number")]
    pub maker_fee_rate: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub taker_fee_rate: f64,
}
impl Symbol<'_> {
    pub fn market(self) -> Option<Market> {
        if self.quote_asset != "USDT" || self.margin_asset != "USDT" {
            return None;
        }
        let amount = self.min_order_size.unwrap_or_else(|| {
            decimal_step(
                self.quantity_precision
                    .or(self.base_asset_precision)
                    .unwrap_or(8.0),
            )
        });
        Some(Market {
            id: self.symbol.to_string(),
            symbol: self.symbol.into_owned(),
            pretty_symbol: None,
            base: self.base_asset.into_owned(),
            quote: self.quote_asset.into_owned(),
            active: self.forward_contract_flag != Some(false),
            precision: Precision {
                amount,
                price: decimal_step(self.price_precision.or(self.quote_precision).unwrap_or(8.0)),
            },
            limits: Limits {
                amount: MinMax {
                    min: amount,
                    max: self.max_order_size.unwrap_or(9_007_199_254_740_991.0),
                },
                leverage: MinMax {
                    min: self.min_leverage.unwrap_or(1.0),
                    max: self.max_leverage.unwrap_or(1.0),
                },
                price: None,
            },
            contract_size: Some(self.contract_val.unwrap_or(1.0)),
            maker_fee: Some(self.maker_fee_rate),
            taker_fee: Some(self.taker_fee_rate),
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(alias = "c", deserialize_with = "public_optional_number")]
    pub last_price: Option<f64>,
    #[serde(alias = "o", deserialize_with = "public_number")]
    pub open_price: f64,
    #[serde(alias = "m", deserialize_with = "public_optional_number")]
    pub mark_price: Option<f64>,
    #[serde(alias = "i", deserialize_with = "public_optional_number")]
    pub index_price: Option<f64>,
    #[serde(alias = "P", deserialize_with = "public_optional_number")]
    pub price_change_percent: Option<f64>,
    #[serde(alias = "v", deserialize_with = "public_number")]
    pub volume: f64,
    #[serde(alias = "q", deserialize_with = "public_number")]
    pub quote_volume: f64,
    #[serde(deserialize_with = "public_number")]
    pub open_interest: f64,
    #[serde(deserialize_with = "public_number")]
    pub funding_rate: f64,
    #[serde(deserialize_with = "public_number")]
    pub funding_time: f64,
    #[serde(deserialize_with = "public_optional_number")]
    pub bid_price: Option<f64>,
    #[serde(deserialize_with = "public_optional_number")]
    pub ask_price: Option<f64>,
}
impl Daily<'_> {
    /// A row without a last price cannot seed a usable ticker.
    pub fn ticker(&self, symbol: &str) -> Option<Ticker> {
        let last_price = self.last_price?;
        let mark = self.mark_price.unwrap_or(last_price);
        let percentage = self
            .price_change_percent
            .map(percentage)
            .unwrap_or_else(|| {
                if self.open_price != 0.0 {
                    (last_price - self.open_price) / self.open_price * 100.0
                } else {
                    0.0
                }
            });
        Some(Ticker {
            id: symbol.into(),
            symbol: symbol.into(),
            bid: self.bid_price.unwrap_or(last_price),
            ask: self.ask_price.unwrap_or(last_price),
            last: last_price,
            mark,
            index: self.index_price.unwrap_or(mark),
            percentage,
            open_interest: self.open_interest,
            funding_rate: self.funding_rate,
            funding_time: self.funding_time,
            volume: self.volume,
            quote_volume: self.quote_volume,
            mid_price: None,
        })
    }
}
pub fn percentage(p: f64) -> f64 {
    if p.abs() <= 1.0 { p * 100.0 } else { p }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookTicker<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub bid_price: Option<f64>,
    #[serde(default, deserialize_with = "public_optional_number")]
    pub ask_price: Option<f64>,
}
#[derive(Deserialize)]
pub struct Level(
    #[serde(deserialize_with = "number")] pub f64,
    #[serde(deserialize_with = "number")] pub f64,
);
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub last_update_id: u64,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}
#[derive(Deserialize)]
pub struct DepthEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(rename = "U")]
    pub first: u64,
    #[serde(rename = "u")]
    pub last: u64,
    pub b: Vec<Level>,
    pub a: Vec<Level>,
}
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub e: Str<'a>,
    #[serde(borrow)]
    pub event: Str<'a>,
    #[serde(borrow)]
    pub msg: Str<'a>,
    pub result: Option<bool>,
}
#[derive(Deserialize)]
pub struct Rows<'a, T> {
    #[serde(borrow)]
    pub s: Str<'a>,
    pub d: T,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum OneOrMany<T> {
    Many(Vec<T>),
    One(T),
}
impl<T> OneOrMany<T> {
    pub fn first(&self) -> Option<&T> {
        match self {
            Self::Many(v) => v.first(),
            Self::One(v) => Some(v),
        }
    }
}

pub struct CandleRow(pub Candle);
impl<'de> Deserialize<'de> for CandleRow {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Row;
        impl<'de> serde::de::Visitor<'de> for Row {
            type Value = CandleRow;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("a candle array of at least six numbers")
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut n = [0.0; 6];
                for slot in &mut n {
                    *slot = seq
                        .next_element::<safe_cex_engine::de::Number>()?
                        .ok_or_else(|| serde::de::Error::custom("short candle"))?
                        .0;
                }
                while seq.next_element::<serde::de::IgnoredAny>()?.is_some() {}
                Ok(CandleRow(Candle {
                    timestamp: (timestamp(n[0]) / 1000.0).floor(),
                    open: n[1],
                    high: n[2],
                    low: n[3],
                    close: n[4],
                    volume: n[5],
                }))
            }
        }
        d.deserialize_seq(Row)
    }
}
#[derive(Deserialize)]
pub struct CandleEvent<'a> {
    #[serde(borrow)]
    pub i: Str<'a>,
    #[serde(alias = "time", deserialize_with = "number")]
    pub t: f64,
    #[serde(deserialize_with = "number")]
    pub o: f64,
    #[serde(deserialize_with = "number")]
    pub h: f64,
    #[serde(deserialize_with = "number")]
    pub l: f64,
    #[serde(deserialize_with = "number")]
    pub c: f64,
    #[serde(deserialize_with = "number")]
    pub v: f64,
}
impl CandleRow {
    pub fn candle(&self) -> Candle {
        self.0
    }
}
impl CandleEvent<'_> {
    pub fn candle(&self) -> Candle {
        Candle {
            timestamp: (timestamp(self.t) / 1000.0).floor(),
            open: self.o,
            high: self.h,
            low: self.l,
            close: self.c,
            volume: self.v,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct TickerUpdate {
    #[serde(deserialize_with = "optional_number")]
    pub c: Option<f64>,
    #[serde(deserialize_with = "optional_number")]
    pub m: Option<f64>,
    #[serde(deserialize_with = "optional_number")]
    pub i: Option<f64>,
    #[serde(rename = "P", deserialize_with = "optional_number")]
    pub percentage: Option<f64>,
    #[serde(deserialize_with = "optional_number")]
    pub v: Option<f64>,
    #[serde(deserialize_with = "optional_number")]
    pub q: Option<f64>,
}

fn decimal_step(digits: f64) -> f64 {
    1.0 / 10f64.powi(digits as i32)
}
#[cfg(test)]
mod tests {
    #[test]
    fn decimal_precision_is_an_exact_decimal_step() {
        for n in 0..=16 {
            assert_eq!(
                super::decimal_step(f64::from(n)),
                format!("1e-{n}").parse::<f64>().unwrap()
            );
        }
    }
}
