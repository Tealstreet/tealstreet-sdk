//! Borrowed MEXC futures records. Numeric identifiers never pass through f64.
use safe_cex_engine::model::Interval;
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

pub use safe_cex_engine::de::{number, optional_number};

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
fn public_number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    Ok(public_optional_number(d)?.unwrap_or(0.0))
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub success: Option<bool>,
    #[serde(default, deserialize_with = "number")]
    pub code: f64,
    #[serde(borrow, default, alias = "msg")]
    pub message: Str<'a>,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}
impl<'a> Envelope<'a> {
    pub fn checked(body: &'a str) -> Result<Self, String> {
        let e: Self = serde_json::from_str(body).map_err(|e| format!("MEXC response: {e}"))?;
        if e.success != Some(true) || e.code != 0.0 {
            return Err(format!("MEXC {}: {}", e.code, e.message));
        }
        Ok(e)
    }
    pub fn payload(&self) -> Result<&'a str, String> {
        self.data
            .map(RawValue::get)
            .ok_or_else(|| "MEXC response has no data".into())
    }
}

#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(borrow, default)]
    pub channel: Str<'a>,
    #[serde(borrow, default)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
    pub success: Option<bool>,
}

#[derive(Deserialize)]
pub struct ContractIdentity<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub base_coin: Str<'a>,
    #[serde(borrow)]
    pub quote_coin: Str<'a>,
    #[serde(borrow, default)]
    pub settle_coin: Str<'a>,
    pub api_allowed: Option<bool>,
    pub is_hidden: Option<bool>,
    #[serde(default, deserialize_with = "number")]
    pub state: f64,
    #[serde(deserialize_with = "number")]
    pub contract_size: f64,
    #[serde(deserialize_with = "number")]
    pub vol_unit: f64,
    #[serde(deserialize_with = "number")]
    pub price_unit: f64,
    #[serde(deserialize_with = "number")]
    pub min_vol: f64,
    #[serde(deserialize_with = "number")]
    pub max_vol: f64,
    #[serde(deserialize_with = "number")]
    pub min_leverage: f64,
    #[serde(deserialize_with = "number")]
    pub max_leverage: f64,
    #[serde(default, deserialize_with = "number")]
    pub maker_fee_rate: f64,
    #[serde(default, deserialize_with = "number")]
    pub taker_fee_rate: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tick<'a> {
    #[serde(borrow, default)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "public_number")]
    pub last_price: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub fair_price: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub index_price: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub bid1: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub ask1: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub rise_fall_rate: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub hold_vol: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub funding_rate: f64,
    #[serde(default, alias = "nextSettleTime", deserialize_with = "public_number")]
    pub funding_time: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub volume24: f64,
    #[serde(default, deserialize_with = "public_number")]
    pub amount24: f64,
}

/// The all-contract channel intentionally omits detailed ticker fields.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MiniTick {
    #[serde(default, deserialize_with = "optional_number")]
    pub last_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub fair_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub index_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub rise_fall_rate: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub volume24: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub amount24: Option<f64>,
}

#[derive(Deserialize)]
pub struct Depth {
    pub version: u64,
    #[serde(default)]
    pub bids: Vec<Level>,
    #[serde(default)]
    pub asks: Vec<Level>,
}
#[derive(Deserialize)]
pub struct DepthSnapshot {
    pub version: u64,
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}
#[derive(Deserialize)]
pub struct Level(
    #[serde(deserialize_with = "number")] pub f64,
    #[serde(deserialize_with = "number")] pub f64,
    #[serde(default)] pub serde::de::IgnoredAny,
);

#[derive(Deserialize)]
pub struct Kline<'a> {
    #[serde(default, alias = "time", deserialize_with = "number")]
    pub t: f64,
    #[serde(alias = "open", deserialize_with = "number")]
    pub o: f64,
    #[serde(alias = "high", deserialize_with = "number")]
    pub h: f64,
    #[serde(alias = "low", deserialize_with = "number")]
    pub l: f64,
    #[serde(alias = "close", deserialize_with = "number")]
    pub c: f64,
    #[serde(default, alias = "vol", deserialize_with = "number")]
    pub q: f64,
    #[serde(borrow, default)]
    pub interval: Str<'a>,
}
#[derive(Deserialize)]
pub struct Klines {
    pub time: Vec<f64>,
    pub open: Vec<f64>,
    pub high: Vec<f64>,
    pub low: Vec<f64>,
    pub close: Vec<f64>,
    pub vol: Vec<f64>,
}

pub fn millis(t: f64) -> f64 {
    if t >= 1e12 {
        t.floor()
    } else {
        (t * 1000.0).floor()
    }
}
pub fn compact(s: &str) -> String {
    s.replace('_', "")
}
pub fn interval(i: Interval) -> Option<&'static str> {
    use Interval::*;
    Some(match i {
        M1 => "Min1",
        M5 => "Min5",
        M15 => "Min15",
        M30 => "Min30",
        H1 => "Min60",
        H4 => "Hour4",
        D1 => "Day1",
        W1 => "Week1",
        _ => return None,
    })
}

pub fn id<'de, D: Deserializer<'de>>(d: D) -> Result<Str<'de>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Id<'a> {
        Text(#[serde(borrow)] Str<'a>),
        Int(u64),
        Signed(i64),
    }
    Ok(match Id::deserialize(d)? {
        Id::Text(s) => s,
        Id::Int(n) => Cow::Owned(n.to_string()),
        Id::Signed(n) => Cow::Owned(n.to_string()),
    })
}

/// What a private list row that failed to decode still says about its identity.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RowIdentity<'a> {
    #[serde(borrow)]
    pub currency: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, deserialize_with = "id")]
    pub order_id: Str<'a>,
    #[serde(borrow, alias = "stopPlanOrderId", deserialize_with = "id")]
    pub id: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset<'a> {
    #[serde(borrow)]
    pub currency: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub available_balance: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub equity: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub cash_balance: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub unrealized: f64,
    #[serde(default, deserialize_with = "number")]
    pub frozen_balance: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow, deserialize_with = "id")]
    pub position_id: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    pub position_type: f64,
    #[serde(deserialize_with = "number")]
    pub hold_vol: f64,
    #[serde(default, deserialize_with = "number")]
    pub hold_avg_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub leverage: f64,
    #[serde(default, deserialize_with = "number")]
    pub open_type: f64,
    #[serde(default, deserialize_with = "number")]
    pub position_mode: f64,
    #[serde(default, deserialize_with = "number", alias = "unrealizedPnl")]
    pub unrealized: f64,
    #[serde(default, deserialize_with = "number")]
    pub realised: f64,
    #[serde(default, deserialize_with = "number")]
    pub liquidate_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub im: f64,
    #[serde(default, deserialize_with = "number")]
    pub update_time: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(default, deserialize_with = "optional_number")]
    pub trend: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub profit_trend: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub loss_trend: Option<f64>,

    #[serde(default)]
    pub reduce_only: bool,
    #[serde(borrow, default, deserialize_with = "id")]
    pub order_id: Str<'a>,
    #[serde(borrow, default, alias = "stopPlanOrderId", deserialize_with = "id")]
    pub id: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    pub side: f64,
    #[serde(default, deserialize_with = "number", alias = "type")]
    pub order_type: f64,
    #[serde(deserialize_with = "number")]
    pub state: f64,
    #[serde(default, deserialize_with = "number")]
    pub vol: f64,
    #[serde(default, deserialize_with = "number")]
    pub deal_vol: f64,
    #[serde(default, deserialize_with = "number")]
    pub price: f64,
    #[serde(default, deserialize_with = "number")]
    pub trigger_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub trigger_type: f64,
    #[serde(default, deserialize_with = "number")]
    pub position_type: f64,
    #[serde(default, deserialize_with = "number")]
    pub vol_type: f64,
    #[serde(default, deserialize_with = "number")]
    pub trigger_side: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub take_profit_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub stop_loss_price: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub update_time: f64,
    #[serde(default, deserialize_with = "number")]
    pub create_time: f64,
    #[serde(default, deserialize_with = "number")]
    pub error_code: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DealRow<'a> {
    #[serde(borrow, default, deserialize_with = "id")]
    pub trade_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "id")]
    pub id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "id")]
    pub order_id: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default, deserialize_with = "number")]
    pub side: f64,
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub vol: f64,
    #[serde(default, deserialize_with = "optional_fill_number")]
    pub fee: Option<f64>,
    #[serde(default, deserialize_with = "optional_fill_number")]
    pub profit: Option<f64>,
    #[serde(default, deserialize_with = "optional_fill_number")]
    pub realized_pnl: Option<f64>,
}

// Optional financial metadata must not suppress an otherwise valid execution.
fn optional_fill_number<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Bare(f64),
        Text(#[serde(borrow)] Str<'a>),
        Other(serde::de::IgnoredAny),
    }
    let value = match N::deserialize(d)? {
        N::Bare(n) => Some(n),
        N::Text(s) => s.parse().ok(),
        N::Other(_) => None,
    };
    Ok(value.filter(|n| n.is_finite()))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeverageRow {
    #[serde(deserialize_with = "number")]
    pub position_type: f64,
    #[serde(deserialize_with = "number")]
    pub leverage: f64,
    #[serde(deserialize_with = "number")]
    pub open_type: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub max_leverage_view: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub max_leverage: Option<f64>,
}
