//! Typed Backpack payloads. Source: https://docs.backpack.exchange/.
use safe_cex_engine::num;
use serde::Deserialize;
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

pub fn sequence<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id<'a> {
        Bare(u64),
        Text(#[serde(borrow)] Str<'a>),
    }
    match Id::deserialize(d)? {
        Id::Bare(id) => Ok(id),
        Id::Text(id) => id.parse().map_err(serde::de::Error::custom),
    }
}

pub use safe_cex_engine::de::{Number, number};
use safe_cex_engine::de::{number_or_zero, optional_number};

/// Names a row that failed to decode, for the skip log; never echoes the raw row.
pub fn row_symbol(item: &str) -> String {
    #[derive(Deserialize)]
    struct Row<'a> {
        #[serde(borrow)]
        symbol: Str<'a>,
    }
    serde_json::from_str::<Row<'_>>(item).map_or_else(|_| "unknown".into(), |r| r.symbol.into())
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
    #[serde(deserialize_with = "sequence")]
    pub last_update_id: u64,
    #[serde(deserialize_with = "levels")]
    pub bids: Vec<(f64, f64)>,
    #[serde(deserialize_with = "levels")]
    pub asks: Vec<(f64, f64)>,
}

fn levels<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<(f64, f64)>, D::Error> {
    struct Levels;
    impl<'de> serde::de::Visitor<'de> for Levels {
        type Value = Vec<(f64, f64)>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("price/quantity levels")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let mut levels = Vec::with_capacity(seq.size_hint().unwrap_or(0));
            while let Some(Level(p, q)) = seq.next_element()? {
                levels.push((p, q));
            }
            Ok(levels)
        }
    }
    d.deserialize_seq(Levels)
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
    #[serde(rename = "U", deserialize_with = "sequence")]
    pub first: u64,
    #[serde(rename = "u", deserialize_with = "sequence")]
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
    #[serde(default, rename = "P")]
    pub percentage: Option<Number>,
    #[serde(default)]
    pub o: Option<Number>,
    #[serde(deserialize_with = "number")]
    pub v: f64,
    #[serde(deserialize_with = "number", alias = "V")]
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
    #[serde(deserialize_with = "number_or_zero")]
    pub b: f64,
    #[serde(deserialize_with = "number_or_zero")]
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
    let text = text
        .strip_suffix('Z')
        .or_else(|| text.strip_suffix("+00:00"))
        .unwrap_or(text);
    if text.len() < 19
        || !text.is_ascii()
        || &text[4..5] != "-"
        || &text[7..8] != "-"
        || !matches!(&text[10..11], "T" | " ")
        || &text[13..14] != ":"
        || &text[16..17] != ":"
    {
        return Err("invalid timestamp".into());
    }
    let suffix = &text[19..];
    if !suffix.is_empty()
        && !(suffix.starts_with('.')
            && suffix.len() > 1
            && suffix[1..].bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("invalid timestamp".into());
    }
    let part = |a: usize, b: usize| {
        text.get(a..b)
            .and_then(|s| s.parse::<i64>().ok())
            .ok_or_else(|| "invalid timestamp".to_owned())
    };
    let (year, month, day) = (part(0, 4)?, part(5, 7)?, part(8, 10)?);
    let (hour, minute, second) = (part(11, 13)?, part(14, 16)?, part(17, 19)?);
    let days_in_month = safe_cex_engine::time::days_in_month(year, month).unwrap_or(0);
    if !(1..=days_in_month).contains(&day)
        || !(0..=23).contains(&hour)
        || !(0..=59).contains(&minute)
        || !(0..=59).contains(&second)
    {
        return Err("invalid timestamp".into());
    }
    let days = safe_cex_engine::time::days_from_civil(year, month, day);
    let fraction = text
        .get(19..)
        .filter(|s| s.starts_with('.'))
        .map_or(0.0, num::parse);
    Ok((days * 86400 + hour * 3600 + minute * 60 + second) as f64 * 1000.0 + fraction * 1000.0)
}

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
    num::parse_decimal(text).ok_or_else(|| "invalid number".into())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRow {
    #[serde(deserialize_with = "number")]
    pub leverage_limit: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collateral {
    #[serde(deserialize_with = "number")]
    pub net_equity: f64,
    #[serde(deserialize_with = "number")]
    pub net_equity_available: f64,
    #[serde(deserialize_with = "number")]
    pub net_equity_locked: f64,
    #[serde(deserialize_with = "number")]
    pub pnl_unrealized: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub net_quantity: f64,
    #[serde(deserialize_with = "number")]
    pub entry_price: f64,
    #[serde(deserialize_with = "number")]
    pub net_exposure_notional: f64,
    #[serde(deserialize_with = "number")]
    pub pnl_unrealized: f64,
    #[serde(default, deserialize_with = "number")]
    pub est_liquidation_price: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(borrow)]
    pub id: Str<'a>,
    pub client_id: Option<u32>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub side: Str<'a>,
    #[serde(borrow)]
    pub order_type: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    pub price: f64,
    pub trigger_price: Option<Number>,
    #[serde(borrow, default)]
    pub trigger_by: Option<Str<'a>>,
    #[serde(default, deserialize_with = "optional_number")]
    pub quantity: Option<f64>,
    pub trigger_quantity: Option<Number>,
    #[serde(default, deserialize_with = "number")]
    pub executed_quantity: f64,
    pub executed_quote_quantity: Option<Number>,
    #[serde(borrow, default)]
    pub time_in_force: Str<'a>,
    #[serde(default)]
    pub post_only: bool,
    #[serde(borrow)]
    pub status: Str<'a>,
    #[serde(deserialize_with = "datetime")]
    pub created_at: f64,
    pub reduce_only: Option<bool>,
}
impl OrderRow<'_> {
    /// Missing base quantity needs a positive amount proven by native trigger
    /// fields. Mappers retain their existing present-value precedence.
    pub fn base_quantity(&self) -> Result<f64, &'static str> {
        if let Some(quantity) = self.quantity {
            return Ok(quantity);
        }
        self.trigger_quantity
            .as_ref()
            .map(|n| n.0)
            .filter(|amount| {
                *amount > 0.0
                    && self.trigger_price.as_ref().is_some_and(|p| p.0 > 0.0)
                    && self.executed_quantity >= 0.0
                    && self.executed_quantity <= *amount
            })
            .ok_or("Backpack order quantity is missing without a proven trigger amount")
    }
}
#[derive(Deserialize)]
pub struct OrderEvent<'a> {
    #[serde(borrow)]
    pub e: Str<'a>,
    #[serde(borrow)]
    pub i: Str<'a>,
    pub c: Option<ClientId<'a>>,
    #[serde(rename = "E", deserialize_with = "number")]
    pub event_time: f64,
    #[serde(rename = "X", borrow)]
    pub status: Str<'a>,
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub o: Str<'a>,
    #[serde(rename = "S", borrow)]
    pub side: Str<'a>,
    pub p: Option<Number>,
    #[serde(rename = "P")]
    pub trigger_price: Option<Number>,
    #[serde(rename = "B", borrow)]
    pub trigger_by: Option<Str<'a>>,
    #[serde(default, deserialize_with = "number")]
    pub q: f64,
    #[serde(rename = "Y")]
    pub trigger_quantity: Option<Number>,
    pub l: Option<Number>,
    #[serde(rename = "L")]
    pub execution_price: Option<Number>,
    #[serde(deserialize_with = "number")]
    pub z: f64,
    #[serde(borrow, default)]
    pub f: Str<'a>,
    #[serde(default)]
    pub y: bool,
    #[serde(default)]
    pub r: bool,
    pub t: Option<u64>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum ClientId<'a> {
    Number(u32),
    Text(#[serde(borrow)] Str<'a>),
}
impl ClientId<'_> {
    pub fn text(&self) -> String {
        match self {
            Self::Number(n) => n.to_string(),
            Self::Text(s) => s.to_string(),
        }
    }
}
#[derive(Deserialize)]
pub struct PositionEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    pub b: Option<Number>,
    #[serde(rename = "B")]
    pub entry: Option<Number>,
    #[serde(deserialize_with = "number")]
    pub q: f64,
    #[serde(default, deserialize_with = "number")]
    pub n: f64,
    #[serde(rename = "P", default, deserialize_with = "number")]
    pub upnl: f64,
    #[serde(default, deserialize_with = "number")]
    pub l: f64,
    #[serde(deserialize_with = "number")]
    pub f: f64,
    #[serde(rename = "E", deserialize_with = "number")]
    pub event_time: f64,
}

pub fn datetime<'de, D: serde::Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Date<'a> {
        Bare(f64),
        Text(#[serde(borrow)] Str<'a>),
    }
    match Date::deserialize(d)? {
        Date::Bare(n) if n.is_finite() => Ok(n),
        Date::Bare(_) => Err(serde::de::Error::custom("invalid timestamp")),
        Date::Text(s) => timestamp(&s).map_err(serde::de::Error::custom),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillRow<'a> {
    #[serde(borrow)]
    pub order_id: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub side: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub quantity: f64,
    #[serde(deserialize_with = "number")]
    pub fee: f64,
    #[serde(borrow)]
    pub fee_symbol: Str<'a>,
    pub trade_id: u64,
    #[serde(borrow)]
    pub timestamp: Str<'a>,
    pub is_maker: Option<bool>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FundingRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub quantity: f64,
    #[serde(borrow)]
    pub interval_end_timestamp: Str<'a>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicTrade {
    pub id: u64,
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub quantity: f64,
    #[serde(deserialize_with = "number")]
    pub quote_quantity: f64,
    #[serde(deserialize_with = "number")]
    pub timestamp: f64,
    pub is_buyer_maker: bool,
}
