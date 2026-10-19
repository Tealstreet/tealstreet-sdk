//! Typed Backpack payloads. Source: https://docs.backpack.exchange/.
use safe_cex_engine::num;
use serde::Deserialize;
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

pub fn number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Bare(f64),
        Text(#[serde(borrow)] Str<'a>),
    }
    let n = match N::deserialize(d)? {
        N::Bare(n) => n,
        N::Text(s) => s.parse().map_err(serde::de::Error::custom)?,
    };
    if !n.is_finite() {
        return Err(serde::de::Error::custom("non-finite number"));
    }
    Ok(n)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub market_type: Str<'a>,
    #[serde(default)]
    pub visible: Option<bool>,
    #[serde(default)]
    pub quantity_increment: Option<Number>,
    #[serde(default)]
    pub price_increment: Option<Number>,
    #[serde(default)]
    pub min_quantity: Option<Number>,
    #[serde(default)]
    pub max_quantity: Option<Number>,
    #[serde(borrow, default)]
    pub imf_function: Option<MarginFunction<'a>>,
    #[serde(default)]
    pub filters: Filters,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginFunction<'a> {
    #[serde(borrow)]
    pub base: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub last_price: f64,
    #[serde(deserialize_with = "number")]
    pub price_change_percent: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
    #[serde(deserialize_with = "number")]
    pub quote_volume: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mark<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub mark_price: f64,
    #[serde(deserialize_with = "number")]
    pub index_price: f64,
    #[serde(deserialize_with = "number")]
    pub funding_rate: f64,
    #[serde(deserialize_with = "number")]
    pub next_funding_timestamp: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Depth {
    pub last_update_id: u64,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}

#[derive(Deserialize)]
pub struct Level(
    #[serde(deserialize_with = "number")] pub f64,
    #[serde(deserialize_with = "number")] pub f64,
);

#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow, default)]
    pub stream: Str<'a>,
    #[serde(borrow)]
    pub data: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow)]
    pub error: Option<WsError<'a>>,
}
#[derive(Deserialize)]
pub struct WsError<'a> {
    pub code: u32,
    #[serde(borrow, default)]
    pub message: Str<'a>,
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

#[derive(Deserialize)]
pub struct TickerEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub c: f64,
    #[serde(deserialize_with = "number", rename = "P")]
    pub percentage: f64,
    #[serde(deserialize_with = "number")]
    pub v: f64,
    #[serde(deserialize_with = "number")]
    pub q: f64,
}

#[derive(Deserialize)]
pub struct TradeEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub p: f64,
}

#[derive(Deserialize)]
pub struct BookEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub b: f64,
    #[serde(deserialize_with = "number")]
    pub a: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kline<'a> {
    #[serde(borrow)]
    pub start: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub open: f64,
    #[serde(deserialize_with = "number")]
    pub high: f64,
    #[serde(deserialize_with = "number")]
    pub low: f64,
    #[serde(deserialize_with = "number")]
    pub close: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
}

#[derive(Deserialize)]
pub struct KlineEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub t: Str<'a>,
    #[serde(borrow)]
    pub o: Option<Str<'a>>,
    #[serde(borrow)]
    pub h: Option<Str<'a>>,
    #[serde(borrow)]
    pub l: Option<Str<'a>>,
    #[serde(borrow)]
    pub c: Option<Str<'a>>,
    #[serde(borrow)]
    pub v: Option<Str<'a>>,
}

pub fn candle(row: Kline<'_>) -> Result<safe_cex_engine::model::Candle, String> {
    Ok(safe_cex_engine::model::Candle {
        timestamp: timestamp(&row.start)?,
        open: row.open,
        high: row.high,
        low: row.low,
        close: row.close,
        volume: row.volume,
    })
}

/// ISO UTC dates (with either separator), plus numeric epochs in history rows.
pub fn timestamp(text: &str) -> Result<f64, String> {
    if let Ok(n) = text.parse::<f64>() {
        if !n.is_finite() {
            return Err("invalid timestamp".into());
        }
        return Ok(if n < 1e12 { n * 1000.0 } else { n });
    }
    let text = text.trim_end_matches('Z');
    if text.len() < 19 || !text.is_ascii() {
        return Err("invalid timestamp".into());
    }
    let part = |a: usize, b: usize| {
        text.get(a..b)
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| "invalid timestamp".to_owned())
    };
    let (year, month, day) = (part(0, 4)?, part(5, 7)?, part(8, 10)?);
    let (hour, minute, second) = (part(11, 13)?, part(14, 16)?, part(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return Err("invalid timestamp".into());
    }
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    let days =
        era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + (153 * m + 2) / 5 + day - 1 - 719468;
    let fraction = text
        .get(19..)
        .filter(|s| s.starts_with('.'))
        .map_or(0.0, num::parse);
    Ok((days * 86400 + hour * 3600 + minute * 60 + second) as f64 * 1000.0 + fraction * 1000.0)
}

#[derive(Deserialize)]
pub struct Number(#[serde(deserialize_with = "number")] pub f64);
#[derive(Default, Deserialize)]
pub struct Filters {
    #[serde(default)]
    pub price: PriceFilter,
    #[serde(default)]
    pub quantity: QuantityFilter,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceFilter {
    pub min_price: Option<Number>,
    pub tick_size: Option<Number>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuantityFilter {
    pub min_quantity: Option<Number>,
    pub max_quantity: Option<Number>,
    pub step_size: Option<Number>,
}

pub fn finite(text: &str) -> Result<f64, String> {
    text.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .ok_or_else(|| "invalid number".into())
}
