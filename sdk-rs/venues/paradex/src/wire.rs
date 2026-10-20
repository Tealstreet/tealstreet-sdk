//! Borrowed Paradex public payloads.
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;

/// Public rows: blank reads as 0, null and malformed reject the row.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(transparent)]
pub struct Number(#[serde(deserialize_with = "safe_cex_engine::de::number_or_blank_zero")] pub f64);
pub use safe_cex_engine::de::Number as StrictNumber;

fn summary_number<'de, D: Deserializer<'de>>(d: D) -> Result<Number, D::Error> {
    // Summary fields use TS's `value || '0'`; do not widen instrument,
    // depth, signed quantity or private numeric parsing.
    Ok(Option::<Number>::deserialize(d)?.unwrap_or_default())
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct InstrumentIdentity<'a> {
    #[serde(borrow, alias = "symbol")]
    pub market: Str<'a>,
    #[serde(borrow)]
    pub status: Str<'a>,
    #[serde(borrow)]
    pub asset_kind: Str<'a>,
    #[serde(borrow)]
    pub market_kind: Str<'a>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Instrument<'a> {
    #[serde(borrow, alias = "symbol")]
    pub market: Str<'a>,
    #[serde(borrow)]
    pub base_currency: Str<'a>,
    #[serde(borrow)]
    pub quote_currency: Str<'a>,
    #[serde(alias = "order_size_increment")]
    pub step_size: Number,
    #[serde(alias = "price_tick_size")]
    pub tick_size: Number,
    #[serde(alias = "min_notional")]
    pub min_order_size: Number,
    pub max_order_size: Number,
    pub price_bands_width: Number,
    pub max_slippage: Number,
    pub initial_margin_fraction: Number,
    pub delta1_cross_margin_params: Margin,
    pub maker_fee: Number,
    pub taker_fee: Number,
}
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Margin {
    pub imf_base: Number,
}

#[derive(Deserialize)]
pub struct MarketIdentity<'a> {
    #[serde(borrow, alias = "market")]
    pub symbol: Str<'a>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Tick<'a> {
    #[serde(borrow, alias = "market")]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "summary_number")]
    pub bid: Number,
    #[serde(deserialize_with = "summary_number")]
    pub ask: Number,
    #[serde(
        alias = "last_price",
        alias = "last",
        deserialize_with = "summary_number"
    )]
    pub last_traded_price: Number,
    #[serde(deserialize_with = "summary_number")]
    pub mark_price: Number,
    #[serde(deserialize_with = "summary_number")]
    pub underlying_price: Number,
    #[serde(deserialize_with = "summary_number")]
    pub oracle_price: Number,
    #[serde(deserialize_with = "summary_number")]
    pub index_price: Number,
    #[serde(deserialize_with = "summary_number")]
    pub price_change_rate_24h: Number,
    #[serde(alias = "volume", deserialize_with = "summary_number")]
    pub volume_24h: Number,
    #[serde(deserialize_with = "summary_number")]
    pub open_interest: Number,
    #[serde(deserialize_with = "summary_number")]
    pub funding_rate: Number,
}
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Bbo<'a> {
    #[serde(borrow)]
    pub market: Str<'a>,
    pub bid_price: Number,
    pub ask_price: Number,
    pub index_price: Number,
}
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct Trade<'a> {
    #[serde(borrow)]
    pub market: Str<'a>,
    pub price: Number,
    pub size: Number,
    #[serde(alias = "timestamp")]
    pub created_at: Number,
}
#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(borrow, default)]
    pub jsonrpc: Str<'a>,
    #[serde(borrow, default)]
    pub method: Str<'a>,
    #[serde(borrow, default, rename = "type")]
    pub kind: Str<'a>,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(borrow, default)]
    pub result: Option<RpcResult<'a>>,
    #[serde(borrow, default)]
    pub params: Option<Params<'a>>,
    #[serde(borrow, default)]
    pub error: Option<RpcError<'a>>,
}
#[derive(Deserialize)]
pub struct RpcResult<'a> {
    #[serde(borrow)]
    pub channel: Str<'a>,
}
#[derive(Deserialize)]
pub struct Params<'a> {
    #[serde(borrow)]
    pub channel: Str<'a>,
    #[serde(borrow)]
    pub data: &'a serde_json::value::RawValue,
}
#[derive(Deserialize)]
pub struct RpcError<'a> {
    #[serde(borrow, default)]
    pub message: Str<'a>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Many<T> {
    Rows(Vec<T>),
    One(T),
}
impl<T> Many<T> {
    pub fn rows(self) -> Vec<T> {
        match self {
            Self::Rows(v) => v,
            Self::One(v) => vec![v],
        }
    }
}

#[derive(Deserialize)]
pub struct Level<'a> {
    #[serde(borrow)]
    pub side: Str<'a>,
    pub price: StrictNumber,
    pub size: StrictNumber,
}
#[derive(Deserialize)]
pub struct Depth<'a> {
    #[serde(borrow, default)]
    pub market: Str<'a>,
    #[serde(borrow)]
    pub update_type: Str<'a>,
    pub seq_no: u64,
    #[serde(borrow)]
    pub inserts: Vec<Level<'a>>,
    #[serde(borrow)]
    pub updates: Vec<Level<'a>>,
    #[serde(borrow)]
    pub deletes: Vec<Level<'a>>,
}
#[derive(Deserialize)]
pub struct TradingView<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(default)]
    pub t: Vec<Number>,
    #[serde(default)]
    pub o: Vec<Number>,
    #[serde(default)]
    pub h: Vec<Number>,
    #[serde(default)]
    pub l: Vec<Number>,
    #[serde(default)]
    pub c: Vec<Number>,
    #[serde(default)]
    pub v: Vec<Number>,
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow, default)]
    pub error: Option<&'a serde_json::value::RawValue>,
    #[serde(borrow, default)]
    pub message: Str<'a>,
}
pub fn check(response: &safe_cex_engine::Response<'_>) -> Result<(), String> {
    if !(200..300).contains(&response.status) {
        return Err(format!("Paradex HTTP {}", response.status));
    }
    let e: Envelope<'_> =
        serde_json::from_str(response.body).map_err(|_| "Malformed Paradex response")?;
    if e.error.is_some() {
        Err(format!("Paradex API error: {}", e.message))
    } else {
        Ok(())
    }
}
