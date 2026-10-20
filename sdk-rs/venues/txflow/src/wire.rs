//! TxFlow's public envelopes and streamed market records.
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision};
use serde::Deserialize;
use std::borrow::Cow;

pub fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Number<'a> {
        Native(f64),
        Text(#[serde(borrow)] Cow<'a, str>),
        Null(()),
    }
    let n = match Number::deserialize(d)? {
        Number::Native(n) => n,
        Number::Null(()) => 0.0,
        Number::Text(s) if s.is_empty() => 0.0,
        Number::Text(s) => s.parse().map_err(serde::de::Error::custom)?,
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("nonfinite number"))
    }
}
fn optional_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    struct Number(#[serde(deserialize_with = "number")] f64);
    Ok(Option::<Number>::deserialize(d)?.map(|n| n.0))
}
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
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow)]
    pub channel: Cow<'a, str>,
    #[serde(borrow)]
    pub data: &'a serde_json::value::RawValue,
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument {
    pub index: u32,
    pub name: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub currency_type: String,
    pub delisted: bool,
    pub halt_trading: bool,
    #[serde(deserialize_with = "number")]
    pub base_precision: f64,
    #[serde(deserialize_with = "number")]
    pub price_tick: f64,
    #[serde(deserialize_with = "number")]
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
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    #[serde(default, deserialize_with = "number")]
    pub funding: f64,
    #[serde(default, deserialize_with = "number")]
    pub mark_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub oracle_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub open_interest: f64,
    #[serde(default, deserialize_with = "number")]
    pub mid_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub last_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub prev_day_px: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub day_ntl_vlm: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub day_base_vlm: Option<f64>,
    #[serde(default)]
    pub next_funding_time: Option<f64>,
}
#[derive(Deserialize)]
pub struct ContextFrame<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub ctx: Context,
}
#[derive(Deserialize)]
pub struct Contexts<'a> {
    #[serde(borrow)]
    pub ctxs: Vec<ContextFrame<'a>>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerRow {
    #[serde(default)]
    pub instrument_id: u32,
    #[serde(default, deserialize_with = "number")]
    pub last_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub prev_day_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub mark_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub oracle_px: f64,
    #[serde(default, deserialize_with = "number")]
    pub open_interest: f64,
    #[serde(default, deserialize_with = "number")]
    pub day_ntl_vlm: f64,
    #[serde(default, deserialize_with = "number")]
    pub coin_vlm: f64,
    #[serde(default)]
    pub node_ctx: Option<Context>,
}
#[derive(Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number")]
    pub px: f64,
    #[serde(deserialize_with = "number")]
    pub sz: f64,
}
#[derive(Deserialize)]
pub struct Depth<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub time: f64,
    pub levels: [Vec<Level>; 2],
}
#[derive(Deserialize)]
pub struct CandleRow<'a> {
    #[serde(borrow)]
    pub s: Cow<'a, str>,
    #[serde(borrow)]
    pub i: Cow<'a, str>,
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
impl CandleRow<'_> {
    pub fn candle(&self) -> Candle {
        Candle {
            timestamp: self.t / 1000.0,
            open: self.o,
            high: self.h,
            low: self.l,
            close: self.c,
            volume: self.v,
        }
    }
}
